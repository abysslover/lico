"""F11: no tracked text file holds a character that draws as nothing, or as something other than what it is: a control character (but
tab, line feed and carriage return), a format character (zero-width, bidirectional, word joiner, byte-order mark, tags), a line or
paragraph separator, a space that is not the ASCII one, or a default-ignorable code point (variation selectors, Hangul fillers,
and the like). Such a character in a source file reads as one thing and means another, and a reviewer does not see it. When a
test or a table needs one, it writes it as an escape (`\\u202e`, `chr(0x202E)`, `\\u{3000}`). There is no allowlist.

Without a `.git` (a `git archive` export) the scan walks the tree and skips what is not source: the directories `.git`, `private`,
`target`, `build*`, `__pycache__`, `.venv`, `venv`, `node_modules` (a directory only: a file named `build-release.sh` is source),
and any file that is not UTF-8 text."""
import re
import subprocess
import unicodedata
from pathlib import Path

from lico_measure.rig import REPO

SKIPPED = {".git", "private", "target", "__pycache__", ".venv", "venv", "node_modules"}
IGNORABLE = ((0x34F, 0x34F), (0x115F, 0x1160), (0x17B4, 0x17B5), (0x180B, 0x180F), (0x200B, 0x200F), (0x202A, 0x202E), (0x2060, 0x206F), (0x3164, 0x3164),
             (0xFE00, 0xFE0F), (0xFEFF, 0xFEFF), (0xFFA0, 0xFFA0), (0xFFF0, 0xFFF8), (0x1BCA0, 0x1BCA3), (0x1D173, 0x1D17A), (0xE0000, 0xE0FFF))
ASCII_CONTROL = re.compile("[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]")


def reason(c):
    """Why this character is refused, or None."""
    if c in "\t\r\n ":
        return None
    category = unicodedata.category(c)
    if category in ("Cc", "Cf", "Zl", "Zp", "Zs"):
        return category
    return "default-ignorable" if any(low <= ord(c) <= high for low, high in IGNORABLE) else None


def tracked_text(root):
    if (root / ".git").exists():
        names = subprocess.run(["git", "-C", str(root), "ls-files", "-z"], capture_output=True, check=True).stdout.decode().split("\0")
        paths = [Path(n) for n in names if n]
    else:
        paths = [p.relative_to(root) for p in root.rglob("*") if not any(part in SKIPPED or part.startswith("build") for part in p.relative_to(root).parts[:-1])]
    for path in sorted(paths):
        full = root / path
        if full.is_symlink() or not full.is_file():
            continue
        raw = full.read_bytes()
        try:
            text = raw.decode("utf-8") if b"\0" not in raw else None
        except UnicodeDecodeError:
            text = None
        if text is not None:
            yield str(path), text


def found_in(root):
    found = []
    for path, text in tracked_text(root):
        for number, line in enumerate(text.split("\n"), 1):                       # not splitlines(): U+2028 and its kin would end a line there
            if line.isascii() and not ASCII_CONTROL.search(line):
                continue
            found += [f"{path}:{number}: U+{ord(c):04X} ({why})" for c in line if (why := reason(c))]
    return found


def test_no_tracked_text_file_holds_an_invisible_or_misleading_character():
    found = found_in(REPO)
    assert not found, "\n".join(found)


def test_the_scan_names_file_line_and_code_point_in_any_kind_of_file_and_leaves_escapes_and_ordinary_text_alone(tmp_path):
    (tmp_path / "tests").mkdir()
    (tmp_path / "private").mkdir()
    (tmp_path / "build").mkdir()
    (tmp_path / "tests" / "a.py").write_text("x = 1\nname = 'a" + chr(0x200B) + "b'\n")
    (tmp_path / "table.json").write_text('{"k": "' + chr(0x202E) + '"}\n')
    (tmp_path / "gui.rs").write_text("fn f() {}\nfn g() {}\nlet s = \"" + chr(0x2028) + "\";\n")
    (tmp_path / "note.md").write_text("a" + chr(0x3000) + "b and a tab\there, a bell " + chr(7) + "\n")
    (tmp_path / "escaped.py").write_text("s = '\\u202e' + chr(0x200B)\n" + "\ud55c\uae00 text, an en dash \u2013 and quotes \u201c\u201d\n")
    (tmp_path / "private" / "mine.txt").write_text(chr(0x200B) + "\n")
    (tmp_path / "build" / "x.txt").write_text(chr(0x200B) + "\n")                         # a directory named build* is skipped ...
    (tmp_path / "build-release.sh").write_text("#!/bin/sh\n" + chr(0x200B) + "echo built\n")   # ... a file of that name is source
    (tmp_path / "image.bin").write_bytes(b"\x00" + chr(0x200B).encode())
    (tmp_path / "latin.txt").write_bytes(b"\xff\xfe broken")
    assert found_in(tmp_path) == ["build-release.sh:2: U+200B (Cf)", "gui.rs:3: U+2028 (Zl)", "note.md:1: U+3000 (Zs)", "note.md:1: U+0007 (Cc)", "table.json:1: U+202E (Cf)",
                                  "tests/a.py:2: U+200B (Cf)"]
