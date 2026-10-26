"""The Python the project asks for (`requires-python` of pyproject.toml) has every standard module the code imports: `tomllib` is in the standard
library from Python 3.11, and the second reviewer's Python 3.10 could not run the files that read a TOML file."""
import re
import tomllib
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
SINCE = {"tomllib": (3, 11)}                                     # the standard modules the code uses that an older supported Python lacks


def lower_bound(spec):
    found = re.fullmatch(r">=\s*(\d+)\.(\d+)", spec.strip())
    assert found, f"requires-python is {spec!r}: this test reads only a '>=X.Y' bound"
    return int(found[1]), int(found[2])


def importers(module):
    pattern = re.compile(rf"^\s*(?:import|from)\s+{module}\b", re.M)
    return sorted(str(p.relative_to(REPO)) for root in ("src", "tests") for p in (REPO / root).rglob("*.py") if pattern.search(p.read_text(encoding="utf-8")))


def test_requires_python_is_new_enough_for_every_standard_module_the_code_imports():
    wanted = lower_bound(tomllib.loads((REPO / "pyproject.toml").read_text(encoding="utf-8"))["project"]["requires-python"])
    for module, since in SINCE.items():
        users = importers(module)
        assert users, f"no file imports {module}: the scan is blind, or the module is no longer used and SINCE should lose it"
        assert wanted >= since, f"requires-python allows Python {wanted[0]}.{wanted[1]} but {module} (Python {since[0]}.{since[1]}) is imported by {users}"
