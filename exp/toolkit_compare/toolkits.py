"""Register the toolkit experiments with the unchanged M01-P01 rig (only exp/ code changes; src/lico_measure is untouched)."""

from __future__ import annotations

import os
from pathlib import Path

from lico_measure import rig

REPO = Path(__file__).resolve().parents[2]
NAMES = ("egui", "iced", "slint")
# per candidate: environment of the client process beyond the rig's private one
EXTRA_ENV = {
    "egui": {},
    "iced": {},
    "slint": {},
}

_original_command = rig.candidate_command


def build_root() -> Path:
    return Path(os.environ.get("LICO_BUILD_ROOT", REPO / "private" / "build"))


def candidate_command(name: str, sock: str) -> list[str]:
    if name in NAMES:
        return [str(build_root() / f"toolkit_{name}" / "release" / f"lico-toolkit-{name}"), "--socket", sock]
    return _original_command(name, sock)


SHORT_ROOT = Path(os.environ.get("LICO_SPIKE_SHORT_ROOT", "/dev/shm/lg_spike_run"))


def register() -> None:
    """Point the rig at a short private root: the rig puts unix sockets under REPO/private, which is too long in a worktree
    (a unix socket path must be under 108 bytes) and ibus keeps sockets there too. Only the rig's REPO constant changes."""
    SHORT_ROOT.mkdir(mode=0o700, parents=True, exist_ok=True)
    rig.REPO = SHORT_ROOT
    rig.candidate_command = candidate_command
    for name, env in EXTRA_ENV.items():
        rig.CANDIDATE_ENV[name] = dict(env)
