"""The retired prototypes the daily suite no longer runs: the candidate clients a, b, c, c2 and d, and the host prototypes rust_std,
rust_serde and c, behind the `legacy_candidates` marker. Nothing is deleted: `python -m lico_measure.suite legacy` runs them.
They stay out of the daily run because the toolkit is decided, their binaries need the GTK, Qt and WebKitGTK development packages
and a hand build (a public clone has the sources, not the binaries), and every protocol change would otherwise force their upkeep.
`a2` is the product under an old name (`lico-candidate-a2` is a link to `lico`); the reference host (`oracle`) and the product host
(`hostd`) stay in the daily run."""
import pytest

CLIENTS = frozenset({"a", "b", "b2", "b3", "c", "c2", "d"})
HOSTS = frozenset({"rust_std", "rust_serde", "c"})
MARK = pytest.mark.legacy_candidates


def _marked(names, legacy):
    return [pytest.param(name, marks=MARK) if name in legacy else name for name in names]


def clients(names):
    """Parameters for a test over candidate clients: the retired ones carry the marker."""
    return _marked(names, CLIENTS)


def hosts(names):
    """Parameters for a test over host implementations: the retired prototypes carry the marker."""
    return _marked(names, HOSTS)
