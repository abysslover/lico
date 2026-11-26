"""M24-P11d in the window: the theme button among the top icons goes round dark, light and system on each press; the window is painted in the look it says; the host keeps the choice and a window
that starts later takes it; `system` is what the desktop says (a stand-in `gsettings` on the PATH answers). A private Xvfb: nothing of the real machine's desktop is looked at."""

from __future__ import annotations

import stat
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.test_codex_adapter import usual_umask, wait_for

FAKE = """#!/bin/sh
# the desktop's two keys, as `gsettings get org.gnome.desktop.interface KEY` says them; what it says is the first line of the file named by FAKE_DESKTOP
want="$(head -n 1 "$FAKE_DESKTOP" 2>/dev/null)"
case "$3" in
  color-scheme) echo "'$want'" ;;
  gtk-theme) echo "'Adwaita'" ;;
esac
"""


def _button(w: ProductWindow) -> dict:
    return next(b for b in w.ui()["toolbar"]["buttons"] if b["id"] == "theme")


def _centre(rect: list[float]) -> tuple[float, float]:
    return (rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0


def _ready(w: ProductWindow) -> dict:
    w.proc.stdin.write("load\n")
    w.proc.stdin.flush()
    return wait_for(lambda: (x := w.ui()) and x["toolbar"]["shown"] and x["sidebar"]["shown"] and x["toolbar"]["buttons"][0]["rect"] and x)


def _grey_at(w: ProductWindow, x: float, y: float) -> int:
    data, width, _ = w.gray()
    return data[int(y) * width + int(x)]


def _grey_mean(w: ProductWindow, rect: list[float]) -> float:
    data, width, _ = w.gray()
    values = [data[y * width + x] for y in range(int(rect[1]), int(rect[3])) for x in range(int(rect[0]), int(rect[2]))]
    return sum(values) / len(values)


def _settings(w: ProductWindow) -> dict:
    return Conn(w.socket).call(op="status")["settings"]


def _theme(u: dict) -> dict:
    """What the dump says of the theme, without the count of the questions asked of the desktop (a test of its own says that)."""
    return {k: v for k, v in u["theme"].items() if k != "asked"}


def _press(w: ProductWindow, then) -> dict:
    w.click(*_centre(_button(w)["rect"]))
    return wait_for(lambda: (u := w.ui()) and then(u) and u)


def test_the_button_goes_round_dark_light_system_paints_the_window_and_the_host_keeps_it():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        bin_dir = root / "bin"
        bin_dir.mkdir()
        fake = bin_dir / "gsettings"
        fake.write_text(FAKE)
        fake.chmod(fake.stat().st_mode | stat.S_IXUSR)
        desktop = root / "desktop"
        desktop.write_text("prefer-light\n")
        env = {"LANG": "C.UTF-8", "FAKE_DESKTOP": str(desktop)}
        with ProductWindow(host_binary=LICO, host_args=["--log-dir", str(root / "logs")], env_extra=env, path_prefix=str(bin_dir)) as w:
            u = _ready(w)
            ids = [b["id"] for b in u["toolbar"]["buttons"]]
            assert ids[-2:] == ["new_browser", "theme"], ids
            button = _button(w)
            assert (button["icon"], button["label"], button["enabled"], button["active"]) == ("moon", "Theme: dark", True, False)
            assert _theme(u) == {"choice": "dark", "mode": "dark", "system": "unknown"} and u["theme"]["asked"] == 0
            browser = next(b for b in u["toolbar"]["buttons"] if b["id"] == "new_browser")
            assert browser["rect"][2] <= button["rect"][0], "the theme icon is the one after the browser"
            side = button["rect"][3] - button["rect"][1]
            assert abs((browser["rect"][3] - browser["rect"][1]) - side) <= 1 and abs((button["rect"][2] - button["rect"][0]) - side) <= 1, "as big as the others"
            main = [u["layout"]["list_w"] + 300, 500]
            assert _grey_at(w, *main) < 60, "the window is dark, as it always was"
            assert "theme" not in _settings(w), "dark is the default: the host keeps no field for it"

            # the first press: light. The window is pale, the ink is dark, the host keeps the word
            u = _press(w, lambda s: s["theme"]["choice"] == "light")
            assert _theme(u) == {"choice": "light", "mode": "light", "system": "unknown"} and u["theme"]["asked"] == 0, "a chosen light asks nothing"
            button = _button(w)
            assert (button["icon"], button["label"]) == ("sun", "Theme: light")
            wait_for(lambda: _grey_at(w, *main) > 190)
            menu_bar = [0, u["menu"]["titles"][0]["rect"][1], u["layout"]["list_w"], u["menu"]["titles"][0]["rect"][3]]
            data, width, _ = w.gray()
            bar = [data[y * width + x] for y in range(int(menu_bar[1]), int(menu_bar[3])) for x in range(40, 300)]
            assert sum(1 for g in bar if g < 110) >= 20 and sum(1 for g in bar if g > 190) > len(bar) // 2, "dark letters on a pale bar"
            assert _settings(w)["theme"] == "light"
            sun_mean = _grey_mean(w, button["rect"])

            # the second: system, and the desktop says light
            u = _press(w, lambda s: s["theme"]["choice"] == "system" and s["theme"]["system"] == "light")
            assert _theme(u) == {"choice": "system", "mode": "light", "system": "light"} and u["theme"]["asked"] == 1, "the desktop was asked once, when the system was chosen"
            button = _button(w)
            assert (button["icon"], button["label"]) == ("auto", "Theme: system (now light)")
            assert _grey_at(w, *main) > 190, "light while the desktop is light"
            assert _settings(w)["theme"] == "system"

            # the desktop goes dark: the next time the system is chosen it is asked again (and about every thirty seconds while it stays chosen)
            desktop.write_text("prefer-dark\n")
            _press(w, lambda s: s["theme"]["choice"] == "dark")
            assert _button(w)["icon"] == "moon"
            wait_for(lambda: _grey_at(w, *main) < 60)
            assert "theme" not in _settings(w), "back to dark: the field is gone again"
            _press(w, lambda s: s["theme"]["choice"] == "light")
            u = _press(w, lambda s: s["theme"]["choice"] == "system" and s["theme"]["system"] == "dark")
            assert (u["theme"]["mode"], _button(w)["label"]) == ("dark", "Theme: system (now dark)")
            wait_for(lambda: _grey_at(w, *main) < 60)
            assert _grey_mean(w, _button(w)["rect"]) != sun_mean, "the picture on the button is not the sun's"

            # the panels keep the press: the Accounts screen stays open and is painted in the look
            w.click(*_centre(next(b for b in w.ui()["toolbar"]["buttons"] if b["id"] == "accounts")["rect"]))
            wait_for(lambda: w.ui()["panel"])
            _press(w, lambda s: s["theme"]["choice"] == "dark")
            assert w.ui()["panel"], "a press on the theme does not close the panel"
            _press(w, lambda s: s["theme"]["choice"] == "light")
            wait_for(lambda: _grey_at(w, *main) > 190)
            assert w.ui()["panel"]


def test_a_window_that_starts_later_takes_the_choice_the_host_keeps():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)

        def setup(w: ProductWindow) -> None:
            assert Conn(w.socket).call(op="settings.set", theme="light")["ok"] == 1

        with ProductWindow(host_binary=LICO, host_args=["--log-dir", str(root / "logs")], env_extra={"LANG": "C.UTF-8"}, before_window=setup) as w:
            u = _ready(w)
            wait_for(lambda: w.ui()["theme"]["choice"] == "light")
            assert (_button(w)["icon"], _button(w)["label"]) == ("sun", "Theme: light")
            main = [u["layout"]["list_w"] + 300, 500]
            wait_for(lambda: _grey_at(w, *main) > 190)
            assert Conn(w.socket).call(op="settings.set", theme="blue")["ok"] != 1, "the host takes three words only"
            assert _settings(w)["theme"] == "light"
