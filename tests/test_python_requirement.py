"""The Python the project asks for (`requires-python` of pyproject.toml) has every standard module the code imports: `tomllib` is in the standard
library from Python 3.11, and the second reviewer's Python 3.10 could not run the files that read a TOML file."""
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
SINCE = {"tomllib": (3, 11)}                                     # the standard modules the code uses that an older supported Python lacks


def requires_python():
    found = re.search(r'^requires-python\s*=\s*"([^"]*)"', (REPO / "pyproject.toml").read_text(encoding="utf-8"), re.M)       # no tomllib: this must speak on 3.10
    assert found, "pyproject.toml has no requires-python"
    return found[1]


def lower_bound(spec):
    found = re.fullmatch(r">=\s*(\d+)\.(\d+)", spec.strip())
    assert found, f"requires-python is {spec!r}: this test reads only a '>=X.Y' bound"
    return int(found[1]), int(found[2])


def importers(module):
    pattern = re.compile(rf"^\s*(?:import|from)\s+{module}\b", re.M)
    return sorted(str(p.relative_to(REPO)) for root in ("src", "tests") for p in (REPO / root).rglob("*.py") if pattern.search(p.read_text(encoding="utf-8")))


def test_requires_python_is_new_enough_for_every_standard_module_the_code_imports():
    wanted = lower_bound(requires_python())
    assert sys.version_info[:2] >= wanted, f"this Python is {sys.version_info[0]}.{sys.version_info[1]}, the project asks for {wanted[0]}.{wanted[1]} or newer"
    for module, since in SINCE.items():
        users = importers(module)
        assert users, f"no file imports {module}: the scan is blind, or the module is no longer used and SINCE should lose it"
        assert wanted >= since, f"requires-python allows Python {wanted[0]}.{wanted[1]} but {module} (Python {since[0]}.{since[1]}) is imported by {users}"
