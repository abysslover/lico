import tomllib
from pathlib import Path

from lico_measure import testmap

ALLOWLIST = tomllib.loads((testmap.REPO / "tests" / "size_allowlist.toml").read_text())


def counted(files, root=testmap.REPO):
    """path -> number of lines, for the Rust and Python files under src/ and tests/."""
    return {f: len((Path(root) / f).read_text(errors="replace").splitlines())
            for f in files if f.endswith((".rs", ".py")) and f.startswith(("src/", "tests/"))}


def problems(lines, allowed, limit):
    found = []
    for path, n in sorted(lines.items()):
        if n > limit and path not in allowed:
            found.append(f"{path} has {n} lines, over the limit of {limit}: split it, or ask the owner for an entry in tests/size_allowlist.toml")
        elif path in allowed and n > allowed[path]:
            found.append(f"{path} grew to {n} lines; the allowlist holds it at {allowed[path]} (it may only shrink)")
    for path, n in sorted(allowed.items()):
        if path not in lines:
            found.append(f"{path} is on the allowlist but is gone: remove the entry")
        elif lines[path] <= limit:
            found.append(f"{path} is back to {lines[path]} lines, under the limit of {limit}: remove the entry")
    return found


def test_no_source_or_test_file_is_over_the_size_limit_except_the_listed_ones_which_may_not_grow():
    assert problems(counted(testmap.tracked()), ALLOWLIST["files"], ALLOWLIST["limit"]) == []


def test_the_guard_names_a_new_big_file_a_grown_one_and_a_stale_entry():
    lines = {"src/a.py": 801, "src/b.rs": 800, "src/c.py": 1300, "src/d.py": 700, "tests/e.py": 40}
    allowed = {"src/c.py": 1200, "src/d.py": 900, "src/gone.rs": 1000}
    found = problems(lines, allowed, 800)
    for line in found:
        print(line)                                           # the output the report quotes
    assert found == [
        "src/a.py has 801 lines, over the limit of 800: split it, or ask the owner for an entry in tests/size_allowlist.toml",
        "src/c.py grew to 1300 lines; the allowlist holds it at 1200 (it may only shrink)",
        "src/d.py is back to 700 lines, under the limit of 800: remove the entry",
        "src/gone.rs is on the allowlist but is gone: remove the entry",
    ]
    assert problems({"src/c.py": 1100}, {"src/c.py": 1200}, 800) == []          # shrinking is free

