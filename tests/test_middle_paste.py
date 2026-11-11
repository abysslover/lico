"""M02-P04 S06: the middle button pastes the PRIMARY selection into the input box (XF-06), with a real window on a private display, a real host and the fake
Claude program. `xclip` owns the selections and `xdotool` presses the buttons. What the person sees is read from the window's dump (`editor.text`, `editor.cursor`,
`editor.rect`, `editor.picture`, `editor.note`, `selection`, `menu.open`). The hit test and the choice (press only, middle only, no menu) are unit tests in
`gui/clip_tests.rs`; the pictures and Shift+Insert are in `test_picture_paste.py` and `test_paste_open.py`.
"""
import subprocess
import time
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO
from lico_measure.rig import private_dir

from tests.test_clipboard_read import make_png
from tests.test_codex_adapter import wait_for
from tests.test_picture_paste import Owners, claude_args, editor, sessions_two, shown
from tests.test_session_actions_window import row_centre


def box_point(w, dx=0.0, dy=0.0):
    """A point in the middle of the input box as the last frame drew it (window pixels, as the left-button tests use)."""
    r = wait_for(lambda: editor(w)["rect"])
    return (r[0] + r[2]) / 2 + dx, (r[1] + r[3]) / 2 + dy


def slow_middle(w, x, y, hold=0.2):
    """Press the middle button, wait, release: a paste that waits for the release too would paste twice."""
    wid = w.window_id()
    since = w.frames()
    subprocess.run(["xdotool", "mousemove", "--window", wid, str(int(x)), str(int(y)), "mousedown", "2"], env=w.env, check=True)
    time.sleep(hold)
    subprocess.run(["xdotool", "mouseup", "2"], env=w.env, check=True)
    w.settle(since, 0.4)


def press(w, x, y, button):
    """A click of `button` at (x, y) without waiting for a frame: the cases where nothing may happen wait a moment in `nothing_happens` instead."""
    subprocess.run(["xdotool", "mousemove", "--window", w.window_id(), str(int(x)), str(int(y)), "click", str(button)], env=w.env, check=True)


def clipboard_text(w):
    return subprocess.run(["xclip", "-o", "-selection", "clipboard"], env={"DISPLAY": w.xvfb.display, "PATH": "/usr/bin:/bin"}, capture_output=True, text=True).stdout


def nothing_happens(w, before, what):
    """Wait a moment (an answer takes a few ms): the box, the selection and the menu are as `before`."""
    time.sleep(0.08)
    now = (editor(w)["text"], editor(w)["cursor"], w.ui()["selection"], w.ui()["menu"]["open"])
    assert now == before, (what, now, before)


def test_a_middle_press_over_the_box_pastes_the_primary_selection_once_at_the_caret_and_leaves_the_selected_lines_and_the_clipboard_alone():
    ids = {}
    with private_dir() as directory:
        args, _ = claude_args(directory)
        log = Path(directory) / "log"
        with ProductWindow(host_binary=LICO, host_args=[*args, "--log-dir", str(log)], before_window=sessions_two(ids)) as w, Owners(w.xvfb.display, directory) as owners:
            wait_for(lambda: len(w.ui()["rows"]) == 3 or None)
            sid = ids["a"]
            w.click(*row_centre(w, sid))
            assert w.ui()["selected"] == sid
            # (1) PRIMARY and CLIPBOARD hold different texts: the middle press takes PRIMARY, at the caret, once; the clipboard is untouched
            owners.own(b"CLIP", selection="clipboard")
            owners.own(b"MIDDLE", selection="primary")
            w.type_text("xz")
            w.key("Left")
            assert editor(w)["cursor"] == 1
            press(w, *box_point(w), 2)
            state = wait_for(lambda: editor(w) if editor(w)["text"] == "xMIDDLEz" else None)
            assert state["cursor"] == 1 + len("MIDDLE"), state
            time.sleep(0.1)
            assert editor(w)["text"] == "xMIDDLEz" and editor(w)["cursor"] == 7, "one press is one paste"
            assert clipboard_text(w) == "CLIP", "the clipboard was not touched"
            # the release does nothing: a press held for 0.2 s and released pastes once
            w.key("ctrl+u")
            w.type_text("xz")
            w.key("Left")
            slow_middle(w, *box_point(w))
            wait_for(lambda: editor(w)["text"] == "xMIDDLEz" or None)
            time.sleep(0.15)
            assert editor(w)["text"] == "xMIDDLEz", "the release did not paste again"
            # (2) the press does not move the selected lines
            w.key("ctrl+u")
            for word in ("one", "two"):                                                    # output lines to select: two sent lines, each echoed
                subprocess.run(["xdotool", "type", "--delay", "2", word], env=w.env, check=True)
                wait_for(lambda w_word=word: editor(w)["text"] == w_word or None)
                subprocess.run(["xdotool", "key", "Return"], env=w.env, check=True)
                wait_for(lambda: editor(w)["text"] == "" or None)
            lines = wait_for(lambda: w.ui()["layout"]["lines"] if len(w.ui()["layout"]["lines"]) >= 3 else None)
            y0, y1 = (lines[0][0] + lines[0][1]) / 2, (lines[1][0] + lines[1][1]) / 2
            wid = w.window_id()
            subprocess.run(["xdotool", "mousemove", "--window", wid, "500", str(int(y0)), "mousedown", "1", "mousemove", "--window", wid, "500", str(int(y1)), "mouseup", "1"], env=w.env, check=True)
            selection = wait_for(lambda: w.ui()["selection"] or None)
            press(w, *box_point(w), 2)
            wait_for(lambda: editor(w)["text"] == "MIDDLE" or None)
            assert w.ui()["selection"] == selection, "the selected lines stay as they are"
            # (3) anywhere else a middle press does nothing: the list, an output line, the menu bar, the margins of the box; and the right button over the box
            before = (editor(w)["text"], editor(w)["cursor"], w.ui()["selection"], None)
            r = editor(w)["rect"]
            title = w.ui()["menu"]["titles"][0]["rect"]
            elsewhere = {
                "list": row_centre(w, ids["b"]),
                "output line": (500, y0),
                "menu bar": ((title[0] + title[2]) / 2, (title[1] + title[3]) / 2),
                "left of the box": (r[0] - 2, (r[1] + r[3]) / 2),
                "right of the box": (r[2] + 2, (r[1] + r[3]) / 2),
                "below the box": ((r[0] + r[2]) / 2, r[3] + 1),
            }
            for what, (x, y) in elsewhere.items():
                press(w, x, y, 2)
                nothing_happens(w, before, what)
            assert w.ui()["selected"] == sid, "the middle press over the list did not switch the session"
            press(w, *box_point(w), 3)
            nothing_happens(w, before, "right button over the box")
            # (5) a menu is open: the press is ignored and the menu stays open
            rect = w.ui()["menu"]["titles"][1]["rect"]
            w.click((rect[0] + rect[2]) / 2, (rect[1] + rect[3]) / 2)
            wait_for(lambda: w.ui()["menu"]["open"] == 1 or None)
            w.click(*box_point(w), button=2)
            time.sleep(0.25)
            assert w.ui()["menu"]["open"] == 1 and editor(w)["text"] == "MIDDLE", (w.ui()["menu"]["open"], editor(w)["text"])
            w.key("Escape")
            wait_for(lambda: w.ui()["menu"]["open"] is None or None)
            # (4) PRIMARY is empty and the CLIPBOARD holds a picture: nothing, and never a chip
            png, _ = make_png(40, 30)
            owners.nothing()
            owners.png(png)
            w.key("ctrl+u")
            press(w, *box_point(w), 2)
            assert shown(w, "Nothing to paste.") and editor(w)["picture"] is None and editor(w)["chip"] is None and editor(w)["text"] == "", editor(w)
            # PRIMARY holds a text while the CLIPBOARD holds a picture: the text comes, and still no chip
            owners.own(b"words", selection="primary")
            press(w, *box_point(w), 2)
            wait_for(lambda: editor(w)["text"] == "words" or None)
            assert editor(w)["picture"] is None and editor(w)["chip"] is None
            # a start screen has no box: a middle press there does nothing
            w.key("ctrl+u")
            for _ in range(2):                                                             # the first Escape may only drop the selected lines
                if w.ui()["selected"] == sid:
                    w.key("Escape")
            wait_for(lambda: w.ui()["selected"] != sid or None)
            width = w.ui()["layout"].get("list_w", 330)
            w.click(width + 200, 200, button=2)
            time.sleep(0.25)
            assert w.ui()["selected"] != sid and editor(w)["text"] == ""
