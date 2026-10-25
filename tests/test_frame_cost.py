import os
import subprocess
from collections import Counter
import time

import pytest

from lico_measure.ime import _focus
from lico_measure.p02 import start
from lico_measure.report import build_measurement
from lico_measure.rig import Client, Xvfb, private_dir

from tests.test_first_view import ui

SCOPE = {"candidate": "a2", "workload": "noisy", "process_scope": "gui tree", "interval_s": 60}


def client_on(host, display, *args):
    client = Client("a2", host.socket, display, extra_args=["--ui", *args], extra_env={"LICO_NO_AUTO_OPEN": "1"})
    time.sleep(0.8)
    _focus(display)                                  # no window manager here: the keyboard goes where the focus is put
    time.sleep(0.3)
    return client


def show(client, sid, at_least):
    client.send(f"show {sid}")
    client.expect("state", "subscribed")
    end = time.monotonic() + 15
    while True:
        client.send("lines")
        if len(client.expect("lines")["lines"]) >= at_least:
            break
        assert time.monotonic() < end
        time.sleep(0.3)
    time.sleep(1.0)                                  # the frame that carries them


def pixels(display):
    env = dict(os.environ, DISPLAY=display)
    return subprocess.run(["import", "-window", "root", "-depth", "8", "rgb:-"], env=env, capture_output=True, check=True).stdout


def test_an_unchanged_frame_shapes_no_line_and_probes_no_character():
    with private_dir() as directory:
        xvfb = Xvfb()
        host = client = None
        try:
            host, _ = start(directory, extra=["--text", "--payload", "110"], sessions=1, producing=0, history=60)
            client = client_on(host, xvfb.display)
            show(client, host.sessions[0], 35)
            first = ui(client)["frame"]
            assert first["lines_shaped"] >= 30 and first["probes"] > 0 and first["frames"] > 0       # the first frame did the work
            assert 0 < first["layout_us"] <= first["total_us"] and first["paint_us"] > 0              # and the dump says what it cost
            for _ in range(3):
                client.send("redraw")
                time.sleep(0.4)
            again = ui(client, until=lambda s: s["frame"]["frames"] >= first["frames"] + 3)["frame"]
            assert again["frames"] >= first["frames"] + 3
            assert again["lines_shaped"] == first["lines_shaped"], "an unchanged line was shaped again"
            assert again["probes"] == first["probes"], "a character was looked up again"
            assert again["label_shapes"] == first["label_shapes"], "an unchanged label was shaped again"
        finally:
            if client and client.proc.poll() is None:
                client.kill()
            if host:
                host.stop()
            xvfb.stop()


def test_lines_in_several_scripts_look_the_same_with_and_without_the_font_shortcuts():
    with private_dir() as directory:
        xvfb = Xvfb()
        host = client = None
        try:
            # Korean, ideographs, Hebrew, an emoji, a combining accent, code and ASCII lines (the text workload)
            host, _ = start(directory, extra=["--text", "--payload", "110"], sessions=1, producing=0, history=60)
            sid = host.sessions[0]
            shots = {}
            for name, args in (("fast", []), ("plain", ["--plain-fonts"])):
                client = client_on(host, xvfb.display, *args)
                show(client, sid, 35)
                frame = ui(client)["frame"]
                shots[name] = (pixels(xvfb.display), frame)
                client.send("quit")
                client.proc.wait(timeout=10)
                time.sleep(0.5)
            fast, plain = shots["fast"][1], shots["plain"][1]
            assert fast["probes"] > 0, "the picture must come from the shortcut"
            assert plain["probes"] == 0, "--plain-fonts keeps the old search"
            a, b = shots["fast"][0], shots["plain"][0]
            assert len(a) == len(b) and len(a) > 1000000
            background = Counter(a[i:i + 3] for i in range(0, len(a), 3)).most_common(1)[0][1]
            assert background < len(a) // 3 * 0.97, "the pictures must hold text, or comparing them proves nothing"
            width = 1200 * 3
            rows = sorted({i // width for i in range(len(a)) if a[i] != b[i]})
            assert not rows, f"{len(rows)} pixel rows differ between the two, from y={rows[0]} to y={rows[-1]}"
        finally:
            if client and client.proc.poll() is None:
                client.kill()
            if host:
                host.stop()
            xvfb.stop()


def test_the_noisy_case_has_a_limit_of_five_percent_of_a_core():
    met = build_measurement("noisy_cpu_percent", SCOPE, [4.9])
    assert met["classification"] == "met" and met["target"]["limit"] == 5.0
    assert build_measurement("noisy_cpu_percent", SCOPE, [5.5])["classification"] == "missed"
    assert build_measurement("noisy_cpu_percent", dict(SCOPE, interval_s=10), [1.0])["classification"] == "unverified"   # a short interval proves nothing
    assert pytest.approx(met["value"]) == 4.9
