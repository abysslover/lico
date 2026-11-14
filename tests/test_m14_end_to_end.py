"""M14-P03: the whole of M14 in one window on a private Xvfb with the window-manager model and a synthetic home (no real terminal, home or window manager): the window starts maximized (the
state is read from the X server), the path bar is the last row and shows the session's folder, a row of the directory list pointed at shows its full path, the icon in the menu row
switches to windowed and back (the bar stays on the last row and the menu row keeps the icon), and two idle seconds draw nothing."""

from __future__ import annotations

import time
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO
from lico_measure.rig import private_dir
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_maximized_start import Model, _centre, is_maximized
from tests.test_new_session_screen import claude_args
from tests.test_path_status_bar import _setup, _start


def test_the_window_starts_maximized_shows_the_path_bar_and_switches_with_its_icon():
    model = Model()
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = (root / "proj-alpha").resolve()
        (proj / "sub").mkdir(mode=0o700, parents=True)
        c_args, _rec = claude_args(root)
        try:
            with ProductWindow(host_binary=LICO, host_args=[*c_args, "--log-dir", str(root / "logs")], env_extra={"LANG": "C.UTF-8"}, before_window=_chain(model, _setup(proj))) as w:
                wait_for(lambda: is_maximized(w))  # 1. the first start is maximized, as the X server holds it
                _start(w, proj)
                u = w.ui()
                width, height = u["layout"]["window_size"]
                assert u["layout"]["maximized"] is True and u["window_btn"]["maximized"] is True, u["window_btn"]

                # 2. the bar is the last row and shows the folder; a pointed row shows its full path
                bar = u["path_bar"]
                assert bar["rect"][3] == height and bar["text"] == str(proj), bar
                entry = next(e for e in u["sidebar"]["entries"] if e["name"] == "sub")
                w.move(*_centre(entry["rect"]))
                assert wait_for(lambda: (b := w.ui()["path_bar"]) and b["source"] == "pointer" and b)["text"] == str(proj / "sub")
                w.move(width - 40, 300)
                wait_for(lambda: w.ui()["path_bar"]["source"] != "pointer")

                # 3. the icon: windowed, then maximized again; the bar stays the last row and the icon stays in the menu row
                w.click(*_centre(u["window_btn"]["rect"]))
                wait_for(lambda: not is_maximized(w))
                windowed = wait_for(lambda: (x := w.ui()) and x["window_btn"]["maximized"] is False and x)
                assert windowed["path_bar"]["rect"][3] == windowed["layout"]["window_size"][1] and windowed["window_btn"]["rect"][1] < 24, windowed["window_btn"]
                w.click(*_centre(windowed["window_btn"]["rect"]))
                wait_for(lambda: is_maximized(w))

                # 4. two idle seconds draw nothing
                w.move(width - 40, 300)
                time.sleep(0.5)
                before = w.frames()
                time.sleep(2.0)
                assert w.frames() == before
        finally:
            model.stop()


def _chain(*steps):
    def run(w):
        for step in steps:
            step(w)

    return run
