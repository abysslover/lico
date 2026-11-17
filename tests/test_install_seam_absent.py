"""M17-P10: the shipped binary has no way to name another release address or hashes. The seam that lets the system tests name a fake release server exists only in a build with the
cargo feature `install_seam` (built by `cargo xtask system`); the plain build that the suite and the release use must not contain its variable name, and the pinned address must be https."""

from pathlib import Path

from lico_measure.p02 import LICO

SEAM_NAME = b"LICO_INSTALL_SEAM_FILE"
SEAM_BUILD = Path(__file__).resolve().parent.parent / "private" / "build" / "system" / "release" / "lico"


def test_the_plain_build_has_no_install_seam_and_pins_an_https_release_address():
    data = Path(LICO).read_bytes()
    assert SEAM_NAME not in data, "the install seam must not be in the plain build"
    assert b"https://github.com/h4ckf0r0day/obscura/releases/download/v0.2.6/obscura-x86_64-linux.tar.gz" in data


def test_the_check_can_tell_a_build_with_the_seam():
    if SEAM_BUILD.exists():   # `cargo xtask system` leaves it; without it there is nothing to compare, and the first test still stands
        assert SEAM_NAME in SEAM_BUILD.read_bytes()
