"""P02-S03: the input box of a session, driven in a real window the way a person uses it, at scale 1 and at scale 2."""
import subprocess

import lico_measure.rig as rig
from lico_measure.p02 import ProtoConn as Conn
from lico_measure.rig import Xvfb, private_dir

from tests.test_claude_adapter import claude_host, new_claude, sent_users, user_text
from tests.test_codex_adapter import wait_for
from tests.test_first_view import click, start_client, ui, window_id, xdo
from tests.test_paste_open import own

LINES = 40


def clipboard_text():
    """What an editor on another system leaves on the clipboard: CRLF line ends, tabs, accents, Hangul."""
    return "".join(f"line {i}\tcafé 한글\r\n" for i in range(LINES))


def box(client, until=None):
    """The window's model of its input box, asked for until `until(box)` holds."""
    return ui(client, (lambda u: until(u["editor"])) if until else None)["editor"]


def height(e):
    return e["rect"][3] - e["rect"][1]


def inside(e, list_w, screen):
    """The box is right of the list and inside the window, and the caret bar is inside the box."""
    x0, y0, x1, y1 = e["rect"]
    cx, cy, cw, ch = e["caret"]
    assert list_w <= x0 < x1 <= screen[0] and 0 <= y0 < y1 <= screen[1], (e["rect"], list_w, screen)
    assert x0 <= cx and cx + cw <= x1 + 1 and y0 <= cy and cy + ch <= y1 + 1, (e["rect"], e["caret"])


def run_box(scale, keys):
    rig.DISPLAY_SCALE = scale
    try:
        with private_dir() as directory:
            xvfb = Xvfb()
            display = xvfb.display
            host = client = None
            try:
                host, log, received = claude_host(directory)
                conn = Conn(host.socket)
                s1, _ = new_claude(conn, mode="default")
                s2, _ = new_claude(conn, mode="default")
                client = start_client(host, display)
                client.send(f"show {s1}")
                client.expect("state", "subscribed")
                key = lambda *k: xdo(display, "key", *k)
                type_ = lambda text: xdo(display, "type", "--delay", "20", text)
                found = subprocess.run(["xdotool", "getwindowgeometry", window_id(display)], env={"DISPLAY": display, "PATH": "/usr/bin:/bin"},
                                       capture_output=True, text=True).stdout.split("Geometry:")[1].split()[0]
                screen = tuple(int(v) for v in found.split("x"))              # the window, in pixels: twice the size at scale 2
                state = ui(client, lambda u: u["editor"]["rect"] is not None and u["selected"] == s1)
                list_w, rows_before = state["layout"]["list_w"], state["layout"]["rows"]
                one = height(state["editor"])
                assert state["editor"]["text"] == "" and one > 0

                def settled(text, n_rows, cursor=None):
                    """The text, the row count and the drawn height agree: the frame that shows the edit has been drawn."""
                    return box(client, lambda e: e["text"] == text and e["rows"] == n_rows and e["rect"] is not None and
                               abs(height(e) - min(n_rows, 8) * one) <= 2 and (cursor is None or e["cursor"] == cursor))

                if keys:
                    type_("alpha beta")
                    settled("alpha beta", 1, 10)
                    key("shift+Return")
                    type_("gamma delta")
                    e = settled("alpha beta\ngamma delta", 2, 22)           # Shift+Enter is a newline and the box has grown by a row
                    inside(e, list_w, screen)
                    key("ctrl+Left")
                    settled("alpha beta\ngamma delta", 2, 17)                # the start of the last word
                    key("ctrl+BackSpace")
                    settled("alpha beta\ndelta", 2, 11)                      # the word before the cursor and the space after it
                    key("Up")
                    settled("alpha beta\ndelta", 2, 0)                       # the column was 0: the first row, column 0
                    key("Delete")
                    settled("lpha beta\ndelta", 2, 0)
                    key("End")
                    settled("lpha beta\ndelta", 2, 9)
                    key("ctrl+k")
                    settled("lpha betadelta", 1, 9)                          # at the end of a line it takes the line break; the box is one row again
                    key("ctrl+u")
                    settled("", 1, 0)

                own(display, clipboard_text())
                key("ctrl+v")
                text = "".join(f"line {i}\tcafé 한글\n" for i in range(LINES))
                end = len(text.encode())                                      # the cursor is a byte offset
                e = settled(text, LINES + 1, end)                      # all of it, as LF, and the box shows 8 rows however long it is
                assert e["first"] == LINES + 1 - 8
                inside(e, list_w, screen)
                after = ui(client)["layout"]
                assert after["rows"] == rows_before and after["list_w"] == list_w          # the session list did not move
                if keys:
                    for _ in range(10):
                        key("Up")
                    e = box(client, lambda e: e["text"].encode()[:e["cursor"]].count(b"\n") == LINES - 10 and e["first"] == LINES - 10)
                    inside(e, list_w, screen)
                    assert height(e) <= 8 * one + 2 and e["recall"] is None                 # Up inside the box moves, it does not recall
                    for _ in range(10):
                        key("Down")
                    box(client, lambda e: e["cursor"] == end and e["first"] == LINES + 1 - 8)
                key("Return")
                wait_for(lambda: sent_users(received))
                assert [user_text(m) for m in sent_users(received)] == [text]            # the same bytes, whole
                e = box(client, lambda e: e["text"] == "" and e["sent"] == 1)
                assert height(e) == one

                def echoed():
                    client.send("lines")
                    return any(line.startswith(">> line 0") for line in client.expect("lines")["lines"])

                wait_for(echoed)                                                        # the host's echo of the input is the agent line that starts with ">> "
                if keys:
                    key("Up")
                    e = box(client, lambda e: e["text"] == text)                         # Up on an empty box brings back what this session was sent
                    assert e["recall"] is not None and e["sent"] == 1
                    key("Down")
                    box(client, lambda e: e["text"] == "" and e["recall"] is None)       # and Down puts the empty box back
                    own(display, "z" * (70 * 1024))
                    key("ctrl+v")
                    e = box(client, lambda e: e["note"] and "Paste refused" in e["note"])
                    assert e["text"] == "" and len(sent_users(received)) == 1            # refused with the reason, nothing added and nothing sent
                    type_("draft one")
                    box(client, lambda e: e["text"] == "draft one")
                    row2 = [r for r in ui(client)["layout"]["rows"] if r[2] == s2][0]
                    click(display, 100, (row2[0] + row2[1]) / 2)
                    e = ui(client, lambda u: u["selected"] == s2 and u["editor"]["text"] == "")["editor"]
                    assert e["parked"] == 1 and e["sent"] == 0                          # each session has its own draft and its own history
                    row1 = [r for r in ui(client)["layout"]["rows"] if r[2] == s1][0]
                    click(display, 100, (row1[0] + row1[1]) / 2)
                    ui(client, lambda u: u["selected"] == s1 and u["editor"]["text"] == "draft one")
            finally:
                if client and client.proc.poll() is None:
                    client.kill()
                if host:
                    host.stop()
                xvfb.stop()
    finally:
        rig.DISPLAY_SCALE = 1


def test_the_input_box_edits_grows_scrolls_recalls_and_sends_a_long_multi_line_text_whole():
    run_box(1, keys=True)


def test_the_input_box_geometry_and_a_long_paste_at_double_scale():
    run_box(2, keys=False)
