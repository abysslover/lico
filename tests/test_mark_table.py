"""M02-P03, the V4 gate: the table of characters that draw nothing alone or only on the character before them (the format characters, the nonspacing and
enclosing marks of Unicode 15.0.0, and U+2800), that a line of agent text is judged without: a lead cannot be hidden behind one. The generator
(lico_protocol/gen_unicode_marks.py) writes the Python mirror (lico_protocol/unicode_marks.py) and the Rust table (hostd/src/marks/table.rs). What does not need
a Python with Unicode 15.0.0 runs everywhere; what compares the files with `unicodedata` says so and is skipped with another version."""
import re
import unicodedata

import pytest

from lico_measure.rig import REPO
from lico_protocol import gen_unicode_marks as gen
from lico_protocol.codex_mapping import reserved
from lico_protocol.unicode_marks import RANGES, UNICODE

RUST_TABLE = REPO / "src" / "host" / "hostd" / "src" / "marks" / "table.rs"
PINNED = pytest.mark.skipif(unicodedata.unidata_version != gen.UNICODE, reason=f"this Python has Unicode {unicodedata.unidata_version}; the table is pinned to {gen.UNICODE} (Python 3.12)")
# Cf that are not default-ignorable (the second reviewer's V4 list, 32 code points), by range
CF_GAP = [(0x600, 0x605), (0x6DD, 0x6DD), (0x70F, 0x70F), (0x890, 0x891), (0x8E2, 0x8E2), (0xFFF9, 0xFFFB), (0x110BD, 0x110BD), (0x110CD, 0x110CD), (0x13430, 0x1343F)]


def members(table):
    return {code for low, high in table for code in range(low, high + 1)}


def rust_ranges():
    return [(int(low, 16), int(high, 16)) for low, high in re.findall(r"\(0x([0-9a-f]+), 0x([0-9a-f]+)\)", RUST_TABLE.read_text(encoding="utf-8"))]


def test_the_table_is_sorted_with_neighbours_joined_and_holds_no_surrogate():
    assert UNICODE == "15.0.0" and len(RANGES) > 300
    for (low, high), (next_low, _) in zip(RANGES, RANGES[1:]):
        assert low <= high < next_low - 1, f"{low:X}..{high:X} then {next_low:X}"
    assert RANGES[-1][0] <= RANGES[-1][1] <= 0x10FFFF and not members(RANGES) & set(range(0xD800, 0xE000))


def test_the_table_has_the_numbers_of_unicode_15_and_what_the_second_reviewer_named_and_leaves_ordinary_text_out():
    held = members(RANGES)
    assert len(held) == 1985 + 170 + 13 + 1, "Mn, Cf, Me and U+2800 of Unicode 15.0.0"
    gap = members(CF_GAP)
    assert len(gap) == 32 and gap <= held
    assert {0x300, 0x301, 0x94D, 0xE31, 0x1E94A, 0x488, 0x20DD, 0x20E3, 0x2800} <= held
    for code in [*b"aZ0 >$[]", 0xE9, 0xD55C, 0xE01, 0x3042, 0x1F600, 0x3000, 0xFF1E]:       # Latin, Hangul, Thai, kana, an emoji, an ideographic space
        assert code not in held, f"U+{code:04X}"


def test_the_python_mirror_and_the_rust_table_are_one_table():
    assert rust_ranges() == list(RANGES)
    assert f'pub const UNICODE: &str = "{UNICODE}";' in RUST_TABLE.read_text(encoding="utf-8")


def test_a_lead_is_reserved_behind_every_code_point_of_the_table_and_a_letter_in_front_or_a_missing_space_behind_is_not():
    for code in members(RANGES):
        mark = chr(code)
        assert reserved(mark + ">> x") and reserved(">" + mark + "> x") and reserved("$" + mark + " x") and reserved("[turn" + mark + " completed]"), f"U+{code:04X}"
        assert not reserved("a" + mark + ">> x") and not reserved(">" + mark + "x"), f"U+{code:04X}"


@PINNED
def test_both_files_are_what_unicodedata_gives():
    assert gen.differing() == []
    table = gen.ranges()
    assert table == list(RANGES) and sum(high - low + 1 for low, high in table) == 2169


@PINNED
def test_the_generator_writes_what_it_checks_and_names_the_file_that_differs(tmp_path, monkeypatch, capsys):
    monkeypatch.setattr(gen, "PYTHON_FILE", tmp_path / "unicode_marks.py")
    monkeypatch.setattr(gen, "RUST_FILE", tmp_path / "marks" / "table.rs")
    assert gen.main(["--check"]) == 1                                       # nothing written yet
    assert gen.main([]) == 0 and gen.main(["--check"]) == 0
    assert (tmp_path / "unicode_marks.py").read_text(encoding="utf-8") == (REPO / "src" / "lico_protocol" / "unicode_marks.py").read_text(encoding="utf-8")
    assert (tmp_path / "marks" / "table.rs").read_text(encoding="utf-8") == RUST_TABLE.read_text(encoding="utf-8")
    (tmp_path / "marks" / "table.rs").write_text("pub const RANGES: &[(u32, u32)] = &[];\n", encoding="utf-8")
    capsys.readouterr()
    assert gen.main(["--check"]) == 1
    said = capsys.readouterr().out
    assert "table.rs differs" in said and "unicode_marks.py differs" not in said


def test_the_generator_refuses_to_write_from_another_version_of_unicode(tmp_path, monkeypatch, capsys):
    monkeypatch.setattr(gen, "PYTHON_FILE", tmp_path / "unicode_marks.py")
    monkeypatch.setattr(gen, "RUST_FILE", tmp_path / "marks" / "table.rs")
    monkeypatch.setattr(gen.unicodedata, "unidata_version", "13.0.0")
    assert gen.main([]) == 2 and gen.main(["--check"]) == 2
    assert not list(tmp_path.iterdir()), "nothing is written"
    assert "pinned to 15.0.0" in capsys.readouterr().err
