"""M22-P04: the top and the bottom of the window. The toolbar under the menu bar is icon buttons (`M22-P04b`: Accounts first, then New session, New Home session, Open browser), the tab row holds tabs only (no `+ +B +BL +G`),
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


def test_the_toolbar_is_four_icon_buttons_the_tab_row_has_none_and_no_word_says_global():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        c_args, _rec = claude_args(root)

        def setup(w: ProductWindow) -> None:
            account(Conn(w.socket), "ClaudeWork", "claude")

        with ProductWindow(host_binary=LICO, host_args=[*c_args, "--log-dir", str(root / "logs")], env_extra={"LANG": "C.UTF-8"}, before_window=setup) as w:
            w.proc.stdin.write("load\n")
            w.proc.stdin.flush()
            u = wait_for(lambda: (x := w.ui()) and x["toolbar"]["shown"] and x["sidebar"]["shown"] and x["start_screen"] and x)

            # the toolbar: four icon buttons (the account first), square, with their words in the state; the text buttons are not drawn
            buttons = u["toolbar"]["buttons"]
            assert [b["id"] for b in buttons] == ["accounts", "new_session", "new_home", "new_browser", "theme"], [b["id"] for b in buttons]
            assert [b["icon"] for b in buttons] == ["account", "plus", "house", "browser", "moon"]
            assert [b["label"] for b in buttons] == ["Accounts", "New session", "New Home session", "Open browser", "Theme: dark"]
            assert all(b["enabled"] and b["rect"] for b in buttons)
            # M24-P09: each icon is a square of TWICE the old side (one text line and a pixel), at any text size or scale: the menu bar's height is (line + 4) x scale, so the old side is
            # (menu bar - 3 x scale) and the scale is the menu bar's height over (font + 8)
            font = u["settings"]["font_px"]
            menu_h = u["menu"]["titles"][0]["rect"][3] - u["menu"]["titles"][0]["rect"][1]
            scale = menu_h / (font + 8)
            want = 2 * (font + 5) * scale
            for b in buttons:
                wd, ht = b["rect"][2] - b["rect"][0], b["rect"][3] - b["rect"][1]
                assert abs(wd - ht) <= 2 and abs(ht - want) <= 2, (b["id"], b["rect"], want)
                assert b["rect"][1] >= u["menu"]["titles"][0]["rect"][3], ("below the menu bar", b["id"], b["rect"])
            assert all(a["rect"][2] <= b["rect"][0] for a, b in zip(buttons, buttons[1:])), "left to right, not overlapping"

            # the tab row holds tabs only
            assert not any(k in u["tabs"] for k in LEFT_OVER_TAB_BUTTONS), sorted(u["tabs"])
            assert "Global" not in u["start_screen"] and "Ctrl+Shift+N  new Home session" in u["start_screen"], u["start_screen"]

            # the menus: no entry says Global; the Session menu has the new entries with their keys
            session_menu = _menu_labels(w, 1)
            assert "New Home session" in session_menu and "New browser tab" in session_menu, session_menu
            assert "New local browser tab" in _menu_labels(w, 4), "M24-P04: the local tab is on the menu More"
            for index in range(5):
                assert not any("Global" in label for label in _menu_labels(w, index)), index

            # a click on each icon opens what it says (M22-P04b: the plus and the house open the wizard, on its first step)
            w.click(*_centre(buttons[0]["rect"]))
            wait_for(lambda: (x := w.ui()) and x["panel"] and x["panel"]["title"] == "Accounts" and x)
            w.key("Escape")
            wait_for(lambda: w.ui()["panel"] is None)
            w.click(*_centre(buttons[1]["rect"]))
            wait_for(lambda: (x := w.ui()) and x["panel"] and x["panel"]["title"] == "New session - step 1 of 3 - Choose" and x)
            w.key("Escape")
            wait_for(lambda: w.ui()["panel"] is None)
            w.click(*_centre(buttons[2]["rect"]))
            home = wait_for(lambda: (x := w.ui()) and x["panel"] and x["panel"]["title"] == "New Home session - step 1 of 3 - Choose" and x["panel"]["form"]["folder"] == "" and x)
            assert not any("Global" in line for line in home["panel"]["body"]) and "Global" not in home["panel"]["title"], home["panel"]
            w.key("Escape")
            wait_for(lambda: w.ui()["panel"] is None)
            w.click(*_centre(buttons[3]["rect"]))
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

            w.click(*_centre(next(b for b in u["toolbar"]["buttons"] if b["id"] == "new_home")["rect"]))
            wait_for(lambda: (x := w.ui()) and x["panel"] and x["panel"]["title"] == "New Home session - step 1 of 3 - Choose" and x["panel"]["form"]["folder"] == "" and x)
            w.key("shift+Return")  # the icon that has the focus, with the options (a plain Enter starts the Home session at once, M24-P02)
            wait_for(lambda: (x := w.ui()) and x["panel"] and x["panel"]["form"]["wizard"]["page"] == 1 and x)
            w.key("Return")  # Next
            wait_for(lambda: (x := w.ui()) and x["panel"] and x["panel"]["form"]["wizard"]["page"] == 2 and x)
            w.key("Return")  # Enter on the name row starts
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


def test_the_sidebar_runs_from_the_top_of_the_window_and_the_menu_the_toolbar_and_the_dropdowns_stay_to_its_right():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        c_args, _rec = claude_args(root)

        def setup(w: ProductWindow) -> None:
            account(Conn(w.socket), "ClaudeWork", "claude")

        with ProductWindow(host_binary=LICO, host_args=[*c_args, "--log-dir", str(root / "logs")], env_extra={"LANG": "C.UTF-8"}, before_window=setup) as w:
            w.proc.stdin.write("load\n")
            w.proc.stdin.flush()
            u = wait_for(lambda: (x := w.ui()) and x["toolbar"]["shown"] and x["sidebar"]["shown"] and x["start_screen"] and x)
            list_w = u["layout"]["list_w"]
            assert list_w > 100, list_w

            # the sidebar's tabs are the first thing at the top of the window, and its column runs the whole height
            sb = u["sidebar"]
            assert sb["dir_tab_rect"][1] <= 4 and sb["chats_tab_rect"][1] <= 4, (sb["dir_tab_rect"], sb["chats_tab_rect"])
            assert sb["dir_tab_rect"][0] < list_w and sb["chats_tab_rect"][2] <= list_w, "the tabs are inside the column"

            # the menu bar, the toolbar and the window button are all to the right of the column
            titles = u["menu"]["titles"]
            assert all(t["rect"][0] >= list_w for t in titles), [t["rect"] for t in titles]
            assert all(b["rect"][0] >= list_w for b in u["toolbar"]["buttons"]), [b["rect"] for b in u["toolbar"]["buttons"]]
            assert u["window_btn"]["rect"][0] >= list_w, u["window_btn"]

            # a click on the sidebar's tab at the very top reaches the sidebar, not the menu
            w.click(*_centre(sb["chats_tab_rect"]))
            wait_for(lambda: w.ui()["sidebar"]["tab"] == "chats")

            # every dropdown opens to the right of the column, whichever title opened it
            for index in range(len(titles)):
                w.click(*_centre(titles[index]["rect"]))
                items = wait_for(lambda: (m := w.ui()["menu"]) and m["open"] == index and m["items"] and m["items"])
                assert all(i["rect"][0] >= list_w for i in items), (index, [i["rect"] for i in items])
                w.key("Escape")
                wait_for(lambda: w.ui()["menu"]["open"] is None)
