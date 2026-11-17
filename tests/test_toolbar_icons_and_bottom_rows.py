"""M22-P04: the top and the bottom of the window. The toolbar under the menu bar is three icon buttons (New session, New Home session, Open browser), the tab row holds tabs only (no `+ +B +BL +G`),
the window never says Global (the menu, the start screen, the tab caption `H`, the telemetry strip), and the bottom is the telemetry strip, the PATH row, then the HINT row last; the path row is
never empty (`lico://home` in Home). A private Xvfb, synthetic homes, mocked providers: no real home is read."""

from __future__ import annotations

from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.account_helpers import account
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_new_session_screen import claude_args
from tests.test_running_elsewhere_window import _centre

LEFT_OVER_TAB_BUTTONS = ("new_rect", "new_global_rect", "new_browser_rect", "new_browser_local_rect")


def _menu_labels(w: ProductWindow, index: int) -> list[str]:
    w.click(*_centre(w.ui()["menu"]["titles"][index]["rect"]))
    items = wait_for(lambda: (m := w.ui()["menu"]) and m["open"] == index and m["items"] and m["items"])
    labels = [i["label"] for i in items]
    w.key("Escape")
    wait_for(lambda: w.ui()["menu"]["open"] is None)
    return labels


def test_the_toolbar_is_three_icon_buttons_the_tab_row_has_none_and_no_word_says_global():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        c_args, _rec = claude_args(root)

        def setup(w: ProductWindow) -> None:
            account(Conn(w.socket), "ClaudeWork", "claude")

        with ProductWindow(host_binary=LICO, host_args=[*c_args, "--log-dir", str(root / "logs")], env_extra={"LANG": "C.UTF-8"}, before_window=setup) as w:
            w.proc.stdin.write("load\n")
            w.proc.stdin.flush()
            u = wait_for(lambda: (x := w.ui()) and x["toolbar"]["shown"] and x["sidebar"]["shown"] and x["start_screen"] and x)

            # the toolbar: three icon buttons, square, with their words in the state; the text buttons are not drawn
            buttons = u["toolbar"]["buttons"]
            assert [b["id"] for b in buttons] == ["new_session", "new_home", "new_browser"], [b["id"] for b in buttons]
            assert [b["icon"] for b in buttons] == ["plus", "house", "browser"]
            assert [b["label"] for b in buttons] == ["New session", "New Home session", "Open browser"]
            assert all(b["enabled"] and b["rect"] for b in buttons)
            for b in buttons:
                wd, ht = b["rect"][2] - b["rect"][0], b["rect"][3] - b["rect"][1]
                assert abs(wd - ht) <= 2 and ht >= 16, (b["id"], b["rect"])
            assert buttons[0]["rect"][2] <= buttons[1]["rect"][0] and buttons[1]["rect"][2] <= buttons[2]["rect"][0], "left to right, not overlapping"

            # the tab row holds tabs only
            assert not any(k in u["tabs"] for k in LEFT_OVER_TAB_BUTTONS), sorted(u["tabs"])
            assert "Global" not in u["start_screen"] and "Ctrl+Shift+N  new Home session" in u["start_screen"], u["start_screen"]

            # the menus: no entry says Global; the Session menu has the new entries with their keys
            session_menu = _menu_labels(w, 1)
            assert "New Home session" in session_menu and "New browser tab" in session_menu and "New local browser tab" in session_menu, session_menu
            for index in range(4):
                assert not any("Global" in label for label in _menu_labels(w, index)), index

            # a click on each icon opens what it says
            w.click(*_centre(buttons[0]["rect"]))
            wait_for(lambda: (x := w.ui()) and x["panel"] and x["panel"]["title"] == "New session" and x)
            w.key("Escape")
            wait_for(lambda: w.ui()["panel"] is None)
            w.click(*_centre(buttons[1]["rect"]))
            home = wait_for(lambda: (x := w.ui()) and x["panel"] and x["panel"]["title"] == "New session" and x["panel"]["form"]["folder"] == "" and x)
            assert any("Home session" in line for line in home["panel"]["body"]) and not any("Global" in line for line in home["panel"]["body"]), home["panel"]["body"]
            w.key("Escape")
            wait_for(lambda: w.ui()["panel"] is None)
            w.click(*_centre(buttons[2]["rect"]))
            wait_for(lambda: (b := w.ui()["browser"]) and b["shown"] and b["prompt"] is False)


def test_a_home_session_has_the_caption_h_and_a_telemetry_strip_that_says_home_and_the_path_row_says_lico_home():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        c_args, _rec = claude_args(root)

        def setup(w: ProductWindow) -> None:
            account(Conn(w.socket), "ClaudeWork", "claude")

        with ProductWindow(host_binary=LICO, host_args=[*c_args, "--log-dir", str(root / "logs")], env_extra={"LANG": "C.UTF-8"}, before_window=setup) as w:
            w.proc.stdin.write("load\n")
            w.proc.stdin.flush()
            u = wait_for(lambda: (x := w.ui()) and x["toolbar"]["shown"] and x["sidebar"]["shown"] and x)

            # nothing selected, Home: the path row says where the person is, never `none`
            bar = u["path_bar"]
            assert bar["text"] != "" and bar["source"] != "none", bar

            w.click(*_centre(u["toolbar"]["buttons"][1]["rect"]))
            wait_for(lambda: (x := w.ui()) and x["panel"] and x["panel"]["title"] == "New session" and x["panel"]["form"]["folder"] == "" and x)
            w.key("Return")
            home = wait_for(lambda: (x := w.ui()) and x["panel"] is None and len(x["tabs"]["items"]) == 1 and x["telemetry"]["shown"] and x)
            tab = home["tabs"]["items"][0]
            assert tab["global"] is True and tab["label"].startswith("H·"), tab["label"]
            assert "Home" in home["telemetry"]["cwd_text"] and "Global" not in home["telemetry"]["cwd_text"], home["telemetry"]["cwd_text"]
            # the sidebar still shows a folder: the row says that folder (the selection); with the sidebar on Home too, it says lico://home
            assert home["path_bar"]["source"] == "selection" and home["path_bar"]["text"] == home["sidebar"]["cwd"], home["path_bar"]
            w.click(*_centre(home["sidebar"]["global_rect"]))
            home = wait_for(lambda: (x := w.ui()) and x["sidebar"]["global_scope"] is True and x)
            assert home["path_bar"]["text"] == "lico://home" and home["path_bar"]["source"] == "home", home["path_bar"]

            # the bottom: the hint row is the last row, the path row is above it, the telemetry strip above that
            width, height = home["layout"]["window_size"]
            bar, hint = home["path_bar"], home["path_bar"]["hint_rect"]
            assert hint[0] == 0 and hint[2] == width and hint[3] == height and hint[3] - hint[1] >= 16, (hint, width, height)
            assert bar["rect"][0] == 0 and bar["rect"][2] == width and bar["rect"][3] == hint[1] and bar["rect"][3] - bar["rect"][1] >= 16, (bar["rect"], hint)
            assert bar["telem_rect"] is None or bar["telem_rect"][3] <= bar["rect"][1], bar
            assert "Ctrl+N" in home["hint"], home["hint"]
