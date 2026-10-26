import os
import signal
import subprocess
from collections import Counter
import time

import pytest

from lico_measure.ime import _focus
from lico_measure.p02 import start
from lico_measure.report import build_measurement
from lico_measure.rig import Client, Xvfb, private_dir

from tests.test_first_view import click, ui, xdo

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


@pytest.mark.parametrize("workload", ["--text", "--realistic-text"])
def test_lines_in_several_scripts_look_the_same_with_and_without_the_font_shortcuts(workload):
    with private_dir() as directory:
        xvfb = Xvfb()
        host = client = None
        try:
            # Korean, ideographs, an emoji, code and ASCII lines; with --text also Hebrew and a combining accent (the exact path), with --realistic-text an arrow and a check mark
            host, _ = start(directory, extra=[workload, "--payload", "110"], sessions=1, producing=0, history=60)
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
            rows = differing_rows(a, b)
            assert not rows, f"{len(rows)} pixel rows differ between the two, from y={rows[0]} to y={rows[-1]}"
        finally:
            if client and client.proc.poll() is None:
                client.kill()
            if host:
                host.stop()
            xvfb.stop()


def without_total_line(shot, width=1200 * 3, list_w=330, lines=2, line_height=18):
    """The picture with the corner under the session list blanked: the last two lines there are the window's memory total and the hint. The total
    carries the window's own memory in whole MiB, which two windows (or one window a few seconds later) can show a MiB apart: not a drawing difference."""
    pic = bytearray(shot)
    rows = len(pic) // width
    for y in range(rows - lines * line_height, rows):
        pic[y * width:y * width + list_w * 3] = bytes(list_w * 3)
    return bytes(pic)


def differing_rows(a, b, width=1200 * 3):
    a, b = without_total_line(a, width), without_total_line(b, width)
    return sorted({i // width for i in range(len(a)) if a[i] != b[i]})


def test_the_picture_comparison_leaves_out_the_corner_with_the_memory_total_and_nothing_else():
    width, rows = 1200 * 3, 800
    plain = bytes(width * rows)
    in_corner = bytearray(plain)
    in_corner[(rows - 12) * width + 100 * 3] = 7                    # the total line, under the list
    above = bytearray(plain)
    above[(rows - 37) * width + 100 * 3] = 7                        # one row above the two lines
    beside = bytearray(plain)
    beside[(rows - 12) * width + 340 * 3] = 7                       # the same rows, right of the list: the transcript's side
    assert differing_rows(plain, bytes(in_corner)) == []
    assert differing_rows(plain, bytes(above)) == [rows - 37]
    assert differing_rows(plain, bytes(beside)) == [rows - 12]


def test_a_flowing_stream_drawn_by_scroll_steps_looks_like_a_full_repaint_on_the_x_server():
    """A real X server and a real softbuffer buffer: the shift is taken (so the buffer's age is 1 there), a menu opens and closes in the middle of the flow,
    and once the host is stopped the screen that the steps left equals the screen of a full repaint of the same lines."""
    with private_dir() as directory:
        xvfb = Xvfb()
        host = client = None
        try:
            host, _ = start(directory, extra=["--realistic-text", "--payload", "110"], sessions=1, producing=1, history=60)
            client = client_on(host, xvfb.display, "--menu")
            show(client, host.sessions[0], 35)
            time.sleep(4)                                    # about 40 frames of a tail that follows
            flowing = ui(client)["frame"]
            click(xvfb.display, 50, 10)                      # the first menu title (after the marker squares)
            time.sleep(1.5)                                  # output flows behind the open menu
            xdo(xvfb.display, "key", "Escape")
            time.sleep(3)
            host.proc.send_signal(signal.SIGSTOP)            # nothing new arrives from here on
            time.sleep(1.5)
            state = ui(client)["frame"]
            stepped = pixels(xvfb.display)
            client.send("repaint")
            time.sleep(1.0)
            repainted = pixels(xvfb.display)
            after = ui(client)["frame"]
            causes = state["causes"]
            print("frames", state["frames"], "kept_rows", state["kept_rows"], "causes", {k: v for k, v in causes.items() if v})   # for the report: run with -s
            assert flowing["causes"]["shift"] >= 10, flowing["causes"]
            assert causes["shift"] > flowing["causes"]["shift"], causes
            assert causes["menu"] >= 2, causes                       # the frames with the menu open, and the one after it closed
            assert state["kept_rows"] > 0 and state["scroll_step"] is True
            assert causes["buffer_age"] <= 2, f"softbuffer's buffer must be age 1 on this server: {causes}"
            assert after["causes"]["first"] == causes["first"] + 1, "the repaint is a full repaint"
            assert len(stepped) == len(repainted) > 1000000
            rows = differing_rows(stepped, repainted)
            assert not rows, f"{len(rows)} pixel rows differ between the stepped screen and a full repaint, from y={rows[0]} to y={rows[-1]}; causes {causes}"
        finally:
            if host:
                host.proc.send_signal(signal.SIGCONT)
            if client and client.proc.poll() is None:
                client.kill()
            if host:
                host.stop()
            xvfb.stop()


def test_with_the_scroll_step_off_every_frame_is_a_full_repaint():
    with private_dir() as directory:
        xvfb = Xvfb()
        host = client = None
        try:
            host, _ = start(directory, extra=["--realistic-text", "--payload", "110"], sessions=1, producing=1, history=60)
            client = client_on(host, xvfb.display, "--menu", "--no-scroll-step")
            show(client, host.sessions[0], 35)
            time.sleep(3)
            frame = ui(client)["frame"]
            assert frame["scroll_step"] is False and frame["kept_rows"] == 0
            assert frame["causes"]["shift"] == 0 and frame["causes"]["disabled"] == frame["frames"], frame["causes"]
        finally:
            if client and client.proc.poll() is None:
                client.kill()
            if host:
                host.stop()
            xvfb.stop()


def test_the_two_text_workloads_take_the_two_font_paths():
    """The realistic text is all on the fast path; the standard mixed-script text has right-to-left and combining lines, which go whole to the exact path."""
    found = {}
    for name, flag in (("realistic", "--realistic-text"), ("mixed", "--text")):
        with private_dir() as directory:
            xvfb = Xvfb()
            host = client = None
            try:
                host, _ = start(directory, extra=[flag, "--payload", "110"], sessions=1, producing=0, history=60)
                client = client_on(host, xvfb.display)
                show(client, host.sessions[0], 35)
                client.send("lines")
                lines = client.expect("lines")["lines"]
                found[name] = (ui(client)["frame"], lines)
            finally:
                if client and client.proc.poll() is None:
                    client.kill()
                if host:
                    host.stop()
                xvfb.stop()
    (real, real_lines), (mixed, mixed_lines) = found["realistic"], found["mixed"]
    assert real["lines_shaped"] >= 30 and real["lines_exact"] == 0, real
    assert not any("שלום" in line or "\u0301" in line for line in real_lines)
    assert any("\u2192" in line for line in real_lines) and any("\u2713" in line for line in real_lines)
    assert mixed["lines_shaped"] >= 30 and mixed["lines_exact"] * 2 > mixed["lines_shaped"], mixed         # most of the standard lines hold a Hebrew word or a combining accent
    assert any("שלום" in line for line in mixed_lines) and any("\u0301" in line for line in mixed_lines)


def test_the_noisy_case_has_a_limit_of_five_percent_of_a_core():
    met = build_measurement("noisy_cpu_percent", SCOPE, [4.9])
    assert met["classification"] == "met" and met["target"]["limit"] == 5.0
    assert build_measurement("noisy_cpu_percent", SCOPE, [5.5])["classification"] == "missed"
    assert build_measurement("noisy_cpu_percent", dict(SCOPE, interval_s=10), [1.0])["classification"] == "unverified"   # a short interval proves nothing
    assert pytest.approx(met["value"]) == 4.9
