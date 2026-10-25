"""Acceptance checks of the three toolkit clients (M01-P07a), one row each of the plan's acceptance list.

Run explicitly (it needs the built clients and starts private X servers, so it is not part of the repository hook):
    PYTHONPATH=src LICO_BUILD_ROOT=<build root> python3 -m pytest -q exp/toolkit_compare/test_acceptance.py
A missing client binary skips a test; with LICO_P07A_REQUIRE=1 it fails instead.
"""

from __future__ import annotations

import os
import sys
import time
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import harness  # noqa: E402
import toolkits  # noqa: E402
from lico_measure import rig  # noqa: E402

NEUTRAL = 0x202020


def _binary(name: str) -> None:
    path = toolkits.build_root() / f"toolkit_{name}" / "release" / f"lico-toolkit-{name}"
    if not path.exists():
        if os.environ.get("LICO_P07A_REQUIRE") == "1":
            pytest.fail(f"missing client binary {path}")
        pytest.skip(f"no binary {path}")


@pytest.fixture(params=toolkits.NAMES)
def scene(request):
    _binary(request.param)
    with harness.Scene(request.param, sessions=100, producing=1) as sc:
        yield sc


def test_first_correct_frame_carries_session_and_sequence_markers(scene):
    sid = scene.host.sessions[0]
    deadline = time.monotonic() + 5
    while scene.reader.pixel(8, 8) != rig.session_rgb(sid) and time.monotonic() < deadline:
        time.sleep(0.05)
    assert scene.reader.pixel(8, 8) == rig.session_rgb(sid)
    assert scene.reader.pixel(24, 8) != NEUTRAL


def test_hide_releases_the_view_and_neutralises_the_markers(scene):
    scene.client.send("hide")
    state = scene.client.expect("state", "hidden")
    assert state["view_resident"] is False
    time.sleep(0.5)
    assert scene.reader.pixel(8, 8) == NEUTRAL
    assert scene.reader.pixel(24, 8) == NEUTRAL


def test_all_hundred_sessions_are_known(scene):
    scene.client.send("load")
    assert scene.client.expect("state", "loaded")["known_sessions"] == 100


def test_the_entry_line_echoes_typed_text(scene):
    lay = harness.layout(scene.candidate)
    scene.click(*lay["entry"])
    scene.xdo("type", "--delay", "40", "hello")
    time.sleep(0.3)
    assert scene.text() == "hello"


def test_quit_ends_the_process(scene):
    scene.client.quit()
    assert scene.client.proc.poll() == 0
