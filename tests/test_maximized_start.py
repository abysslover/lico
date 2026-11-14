"""M14-P02: the OUTCOME of the maximized start, not the request. The first window of `lico gui` must end up maximized (`_NET_WM_STATE` carries both maximized atoms, read from the
X server) under a window manager. No window manager is installed on the test machine and the owner's mutter is not to be touched, so the window manager is a MODEL
(`lico_measure.fakewm`): it follows the standard and can be told to drop the `_NET_WM_STATE` message (what the owner's mutter was seen to do: no maximized bit, size as requested) or to
need time before it takes a window. What these tests show: a window manager that follows the standard maximizes the window at its first start, whether it needs time or ignores the
message; what they cannot show: that mutter on the owner's screen does. The request itself (`maximized_requested`) is still checked by test_session_tabs_and_telemetry.
A private Xvfb, synthetic homes, no real terminal or home."""

from __future__ import annotations

import subprocess
import sys
import time
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO
from lico_measure.rig import REPO, private_dir
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_new_session_screen import claude_args

MAXIMIZED = ("_NET_WM_STATE_MAXIMIZED_VERT", "_NET_WM_STATE_MAXIMIZED_HORZ")


class Model:
    """The window-manager model on the display of a ProductWindow, started before the window and stopped with it."""

    def __init__(self, *flags: str) -> None:
        self.flags, self.proc = flags, None

    def __call__(self, w: ProductWindow) -> None:
        env = {"PATH": "/usr/bin:/bin", "PYTHONPATH": str(REPO / "src")}
        self.proc = subprocess.Popen([sys.executable, "-m", "lico_measure.fakewm", w.xvfb.display, *self.flags], env=env, stdout=subprocess.PIPE, text=True)
        assert self.proc.stdout.readline().strip() == "ready"

    def stop(self) -> None:
        if self.proc and self.proc.poll() is None:
            self.proc.terminate()
            self.proc.wait(timeout=5)


def state_of(w: ProductWindow) -> list[str]:
    """The `_NET_WM_STATE` atoms of the window as the X server holds them."""
    out = subprocess.run(["xprop", "-id", w.window_id(), "_NET_WM_STATE"], env=w.env, capture_output=True, text=True).stdout
    return [a.strip() for a in out.partition("=")[2].split(",") if a.strip()]


def is_maximized(w: ProductWindow) -> bool:
    return all(a in state_of(w) for a in MAXIMIZED)


def run_case(*flags: str, env_extra: dict | None = None, expect: bool) -> None:
    model = Model(*flags)
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        c_args, _rec = claude_args(root)
        try:
            with ProductWindow(host_binary=LICO, host_args=[*c_args, "--log-dir", str(root / "logs")], env_extra={"LANG": "C.UTF-8", **(env_extra or {})}, before_window=model) as w:
                w.proc.stdin.write("load\n")
                w.proc.stdin.flush()
                if expect:
                    wait_for(lambda: is_maximized(w))
                    wait_for(lambda: w.ui()["layout"]["maximized"] is True)  # the window's own view of it agrees with the server's
                else:
                    time.sleep(1.5)
                    assert not is_maximized(w), state_of(w)
        finally:
            model.stop()


def test_a_window_manager_that_takes_the_window_at_once_gives_a_maximized_first_start():
    run_case(expect=True)


def test_a_window_manager_that_needs_time_to_take_the_window_still_gives_a_maximized_first_start():
    run_case("--manage-delay", "0.3", expect=True)


def test_a_window_manager_that_drops_the_state_message_still_gives_a_maximized_first_start_by_the_initial_state():
    run_case("--ignore-messages", expect=True)


def test_windowed_starts_windowed():
    run_case(env_extra={"LICO_WINDOWED": "1"}, expect=False)


def _centre(rect: list[float]) -> tuple[float, float]:
    return (rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0


def test_the_icon_in_the_window_switches_between_maximized_and_windowed_in_both_directions():
    model = Model()
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        c_args, _rec = claude_args(root)
        try:
            with ProductWindow(host_binary=LICO, host_args=[*c_args, "--log-dir", str(root / "logs")], env_extra={"LANG": "C.UTF-8"}, before_window=model) as w:
                w.proc.stdin.write("load\n")
                w.proc.stdin.flush()
                wait_for(lambda: is_maximized(w))
                u = wait_for(lambda: (x := w.ui()) and x["window_btn"]["maximized"] is True and x["window_btn"]["rect"] and x)
                icon, width = u["window_btn"], u["layout"]["window_size"][0]
                assert icon["label"] == "Restore window" and icon["rect"][2] <= width and width - icon["rect"][2] <= 12 and icon["rect"][1] >= 0 and icon["rect"][3] <= 24, icon
                w.click(*_centre(icon["rect"]))
                wait_for(lambda: not is_maximized(w))
                windowed = wait_for(lambda: (x := w.ui()) and x["window_btn"]["maximized"] is False and x["window_btn"])
                assert windowed["label"] == "Maximize window", windowed
                w.click(*_centre(windowed["rect"]))
                wait_for(lambda: is_maximized(w))
                assert wait_for(lambda: (x := w.ui()) and x["window_btn"]["maximized"] is True and x["window_btn"])["label"] == "Restore window"
        finally:
            model.stop()


def test_a_windowed_start_offers_to_maximize_and_the_icon_does_it():
    model = Model()
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        c_args, _rec = claude_args(root)
        try:
            with ProductWindow(host_binary=LICO, host_args=[*c_args, "--log-dir", str(root / "logs")], env_extra={"LANG": "C.UTF-8", "LICO_WINDOWED": "1"}, before_window=model) as w:
                w.proc.stdin.write("load\n")
                w.proc.stdin.flush()
                icon = wait_for(lambda: (x := w.ui()) and x["window_btn"]["rect"] and x["window_btn"])
                assert icon["maximized"] is False and icon["label"] == "Maximize window" and not is_maximized(w), (icon, state_of(w))
                w.click(*_centre(icon["rect"]))
                wait_for(lambda: is_maximized(w))
                assert wait_for(lambda: (x := w.ui()) and x["window_btn"]["maximized"] is True and x["window_btn"])["label"] == "Restore window"
        finally:
            model.stop()
