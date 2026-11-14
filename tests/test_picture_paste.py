"""M02-P04 S05b: a pasted picture in the product window (XF-05), with a real window on a private display, a real host and the fake Claude program.

`xclip` is the owner of the clipboard and `xdotool` presses the keys. The window reads a PNG only when the clipboard has no text, shows it as a chip under the box,
and Enter sends it: the window asks the host for a place (`picture.slot`), writes the bytes there and sends `input` with the NAME. What Claude got is read from the file
the fake Claude program keeps; what the person sees is read from the window's dump (`editor.picture`, `editor.chip`, `editor.note`). The cases that need many sessions or
a host that answers in a way the real one cannot are unit tests in `gui/picture_tests.rs`.
"""
import base64
import os
import signal
import subprocess
import time
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir

from tests.test_claude_adapter import new_claude, records, sent_users, status_of, user_text
from tests.test_clipboard_read import forged_head, make_png
from tests.test_codex_adapter import wait_for
from tests.test_new_session_screen import claude_args as claude_only
from tests.test_reconnect_window import codex_args
from tests.test_session_actions_window import row_centre

MIB3 = 3 * 1024 * 1024


class Owners:
    """Owners of a selection made with `xclip`; the previous owner of a selection goes first. Ended by their exact pids."""

    def __init__(self, display, directory):
        self.display, self.directory, self.made = display, Path(directory), []

    def own(self, data, selection="clipboard", target=None):
        for old in [o for o in self.made if o[1] == selection]:
            old[0].kill()
            old[0].wait()
            self.made.remove(old)
        file = self.directory / f"owned_{len(self.made)}_{time.monotonic_ns()}"
        file.write_bytes(data)
        env = {"DISPLAY": self.display, "PATH": "/usr/bin:/bin"}
        cmd = ["xclip", "-quiet", "-selection", selection] + (["-t", target] if target else []) + ["-i", str(file)]
        proc = subprocess.Popen(cmd, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.made.append((proc, selection))
        for _ in range(200):
            probe = subprocess.run(["xclip", "-o", "-selection", selection, "-t", "TARGETS"], env=env, capture_output=True)
            if probe.returncode == 0:
                return proc
            time.sleep(0.02)
        raise AssertionError("xclip did not take the selection")

    def png(self, data):
        return self.own(data, target="image/png")

    def nothing(self):
        for proc, _ in self.made:
            proc.kill()
            proc.wait()
        self.made.clear()

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.nothing()


def claude_args(directory):
    """The arguments of a host with the fake Claude and the fake Codex programs; where the first one writes what it received."""
    args, received = claude_only(directory)
    return [*args, *codex_args(directory)[0]], received


def editor(w):
    return w.ui()["editor"]


def shown(w, note):
    """The note under the box is `note` (the dump has it once the window has drawn)."""
    return wait_for(lambda: editor(w)["note"] == note or None) and editor(w)["note"]


def pasted_dir(w):
    return Path(w.directory) / "state" / "lico" / "pasted"


def press_paste(w):
    subprocess.run(["xdotool", "key", "ctrl+v"], env=w.env, check=True)


def fast_type(w, text):
    subprocess.run(["xdotool", "type", "--delay", "2", text], env=w.env, check=True)
    wait_for(lambda: editor(w)["text"] == text or None)


def picture_sent(received):
    return [m for m in sent_users(received) if any(b.get("type") == "image" for b in m["message"]["content"])]


def sessions_two(w_holder, synthetic=False):
    def before(w):
        conn = Conn(w.socket)
        if synthetic:                                        # a synthetic session is not shown in the window, so a Codex session stands for "not Claude"
            made = conn.call(op="session.create", kind="codex")
            assert made["ok"] == 1, made
            w_holder["s"] = made["session"]
        w_holder["a"], w_holder["ga"] = new_claude(conn, name="alpha")
        w_holder["b"], w_holder["gb"] = new_claude(conn, name="beta")
        w_holder["c"], w_holder["gc"] = new_claude(conn, name="gamma")
    return before


def menu_paste(w):
    """Session menu, the entry Paste."""
    titles = w.ui()["menu"]["titles"]
    rect = titles[1]["rect"]
    w.click((rect[0] + rect[2]) / 2, (rect[1] + rect[3]) / 2)
    wait_for(lambda: w.ui()["menu"]["open"] == 1 and all(i["rect"] for i in w.ui()["menu"]["items"]) or None)
    item = next(i for i in w.ui()["menu"]["items"] if i["label"] == "Paste")
    assert item["enabled"], item
    r = item["rect"]
    w.click((r[0] + r[2]) / 2, (r[1] + r[3]) / 2)


def test_ctrl_v_of_a_png_shows_a_chip_enter_sends_it_to_claude_before_the_text_and_the_chip_and_the_file_are_gone():
    png, _ = make_png(40, 30)
    ids = {}
    with private_dir() as directory:
        args, received = claude_args(directory)
        log = Path(directory) / "log"
        with ProductWindow(host_binary=LICO, host_args=[*args, "--log-dir", str(log)], before_window=sessions_two(ids)) as w, Owners(w.xvfb.display, directory) as owners:
            wait_for(lambda: len(w.ui()["rows"]) == 3 or None)
            sid = ids["a"]
            w.click(*row_centre(w, sid))
            assert w.ui()["selected"] == sid
            # text is unchanged: a text on the clipboard pastes as text, and no chip is made
            owners.own(b"plain words")
            press_paste(w)
            wait_for(lambda: editor(w)["text"] == "plain words" or None)
            assert editor(w)["picture"] is None and editor(w)["chip"] is None
            w.key("ctrl+u")
            # Ctrl+V of a PNG: the chip and its line, the box untouched
            owners.png(png)
            fast_type(w, "what is this?")
            press_paste(w)
            state = wait_for(lambda: editor(w) if editor(w)["picture"] else None)
            assert state["picture"] == {"bytes": len(png), "width": 40, "height": 30}, state
            assert state["chip"] == "[picture 1 KiB, 40x30] - Esc removes it" and state["text"] == "what is this?", state
            # Escape removes the chip and the text stays; the second Escape (box not empty) does not leave the session; with an empty box it does
            w.key("Escape")
            state = editor(w)
            assert state["picture"] is None and state["chip"] is None and state["text"] == "what is this?", state
            assert w.ui()["selected"] == sid
            # Shift+Insert reads PRIMARY, which never carries a picture: the PNG on the CLIPBOARD is not taken, and no chip is made
            owners.png(png)
            subprocess.run(["xdotool", "key", "shift+Insert"], env=w.env, check=True)
            assert shown(w, "Nothing to paste.") and editor(w)["picture"] is None and editor(w)["chip"] is None, editor(w)
            w.key("ctrl+u")
            fast_type(w, "what is this?")
            # Ctrl+V with no PNG left has nothing to paste: no chip
            owners.nothing()
            press_paste(w)
            assert shown(w, "Nothing to paste.") and editor(w)["picture"] is None
            # the picture comes back, Enter sends: the slot, the write, the input; Claude gets the image block first
            owners.png(png)
            press_paste(w)
            wait_for(lambda: editor(w)["picture"] or None)
            w.key("Return")
            wait_for(lambda: picture_sent(received) or None)
            message = picture_sent(received)[0]["message"]["content"]
            assert [b["type"] for b in message] == ["image", "text"], message
            assert base64.b64decode(message[0]["source"]["data"], validate=True) == png, "the bytes are the PNG's"
            assert message[1]["text"] == "what is this?"
            wait_for(lambda: any(l == ">> [picture 1 KiB] what is this?" for l in records(log, sid)) or None)
            state = editor(w)
            assert state["picture"] is None and state["chip"] is None and state["text"] == "", state
            wait_for(lambda: not os.listdir(pasted_dir(w)) or None)
            assert os.listdir(pasted_dir(w)) == [], "the host removed the file"
            # Enter with a chip and an empty box sends nothing and the chip stays
            before = len(sent_users(received))
            press_paste(w)
            wait_for(lambda: editor(w)["picture"] or None)
            w.key("Return")
            assert shown(w, "Type a line to go with the picture.") and editor(w)["picture"], editor(w)
            assert len(sent_users(received)) == before and os.listdir(pasted_dir(w)) == []
            # Escape with a selection of lines only drops the selection; the chip stays and the next Escape removes it
            lines = wait_for(lambda: w.ui()["layout"]["lines"] if len(w.ui()["layout"]["lines"]) >= 2 else None)
            y0, y1 = (lines[0][0] + lines[0][1]) / 2, (lines[1][0] + lines[1][1]) / 2
            wid = w.window_id()
            subprocess.run(["xdotool", "mousemove", "--window", wid, "500", str(int(y0)), "mousedown", "1", "mousemove", "--window", wid, "500", str(int(y1)), "mouseup", "1"], env=w.env, check=True)
            wait_for(lambda: w.ui()["selection"] or None)
            w.key("Escape")
            assert w.ui()["selection"] is None and editor(w)["picture"] is not None, "the selection went first, the chip stays"
            # the chip stays with its session: open the other one and come back
            w.click(*row_centre(w, ids["b"]))
            assert w.ui()["selected"] == ids["b"] and editor(w)["picture"] is None and editor(w)["chip"] is None
            w.click(*row_centre(w, sid))
            assert editor(w)["picture"]["bytes"] == len(png)
            # a second picture replaces the first
            png2, _ = make_png(50, 20)
            owners.png(png2)
            press_paste(w)
            state = wait_for(lambda: editor(w) if editor(w)["picture"] and editor(w)["picture"]["width"] == 50 else None)
            assert state["note"] == "The picture was replaced." and state["picture"]["height"] == 20, state
            # Escape of a selection of lines goes first; with the chip gone and an empty box the next Escape goes to the start screen
            w.key("Escape")
            assert editor(w)["picture"] is None and w.ui()["selected"] == sid
            w.key("Escape")
            wait_for(lambda: w.ui()["selected"] != sid or None)
            # the menu's Paste works the same
            w.click(*row_centre(w, sid))
            wait_for(lambda: w.ui()["selected"] == sid or None)
            owners.png(png)
            menu_paste(w)
            state = wait_for(lambda: editor(w) if editor(w)["picture"] else None)
            assert state["chip"] == "[picture 1 KiB, 40x30] - Esc removes it", state


def test_a_picture_that_cannot_be_taken_gives_its_sentence_and_no_chip_and_the_limit_of_six_mib_holds():
    ids = {}
    with private_dir() as directory:
        args, received = claude_args(directory)
        log = Path(directory) / "log"
        with ProductWindow(host_binary=LICO, host_args=[*args, "--log-dir", str(log)], before_window=sessions_two(ids, synthetic=True)) as w, Owners(w.xvfb.display, directory) as owners:
            wait_for(lambda: len(w.ui()["rows"]) == 4 or None)
            synthetic = ids["s"]
            w.click(*row_centre(w, ids["a"]))

            def refused(data, sentence, target="image/png"):
                owners.own(data, target=target)
                press_paste(w)
                assert shown(w, sentence), editor(w)["note"]
                assert editor(w)["picture"] is None and editor(w)["chip"] is None

            refused(forged_head(40, 30) + os.urandom(MIB3 + 1 - len(forged_head(40, 30))), "Paste refused: the picture is over 3 MiB or 8000 pixels on a side.")
            refused(forged_head(9000, 30) + os.urandom(1200), "Paste refused: the picture is over 3 MiB or 8000 pixels on a side.")
            refused(b"\xff\xd8\xff\xe0" + os.urandom(100), "Nothing to paste: only text and PNG pictures can be pasted.", target="image/jpeg")
            owners.nothing()
            press_paste(w)
            assert shown(w, "Nothing to paste.")
            # exactly 3 MiB is taken; 3 MiB in the second session is taken (6 MiB in all); a third picture is not
            three = forged_head(40, 30) + os.urandom(MIB3 - len(forged_head(40, 30)))
            owners.png(three)
            press_paste(w)
            state = wait_for(lambda: editor(w) if editor(w)["picture"] else None)
            assert state["picture"]["bytes"] == MIB3 and state["chip"] == "[picture 3072 KiB, 40x30] - Esc removes it", state
            w.click(*row_centre(w, ids["b"]))
            press_paste(w)
            wait_for(lambda: editor(w)["picture"] or None)
            w.click(*row_centre(w, synthetic))
            assert w.ui()["selected"] == synthetic
            press_paste(w)
            assert shown(w, "Pictures can be pasted into Claude sessions only.") and editor(w)["picture"] is None
            # the third session cannot add a picture while the other two hold 6 MiB; a replacement of one of the two is judged without the old one
            w.click(*row_centre(w, ids["c"]))
            owners.png(make_png(40, 30)[0])
            press_paste(w)
            assert shown(w, "Send or remove the waiting picture of another session first.") and editor(w)["picture"] is None
            w.click(*row_centre(w, ids["b"]))
            w.key("Escape")                                                                # the chip of the second session goes: its 3 MiB are free again
            w.click(*row_centre(w, ids["c"]))
            press_paste(w)
            assert wait_for(lambda: editor(w)["picture"] or None)


def test_a_slot_the_host_refuses_and_an_input_it_refuses_give_the_text_and_the_chip_back():
    ids = {}
    png, _ = make_png(40, 30)
    with private_dir() as directory:
        args, received = claude_args(directory)
        log = Path(directory) / "log"
        with ProductWindow(host_binary=LICO, host_args=[*args, "--log-dir", str(log)], before_window=sessions_two(ids)) as w, Owners(w.xvfb.display, directory) as owners:
            wait_for(lambda: len(w.ui()["rows"]) == 3 or None)
            sid = ids["a"]
            w.click(*row_centre(w, sid))
            conn = Conn(w.socket)
            owners.png(png)
            fast_type(w, "kept text")
            press_paste(w)
            wait_for(lambda: editor(w)["picture"] or None)
            # eight places taken through the socket: the host is busy and the window says so; text and chip are back
            for _ in range(8):
                assert conn.call(op="picture.slot")["ok"] == 1
            subprocess.run(["xdotool", "key", "Return"], env=w.env, check=True)
            assert shown(w, "Too many pictures are waiting, try again in a moment.")
            state = editor(w)
            assert state["text"] == "kept text" and state["picture"]["bytes"] == len(png), state
            assert len(picture_sent(received)) == 0
            for name in os.listdir(pasted_dir(w)):
                os.unlink(pasted_dir(w) / name)
            # the host refuses the input: the window's generation is old (the session was closed and resumed while the window could not see it)
            first = status_of(conn, sid)["generation"]
            assert conn.call(op="input", session=sid, generation=first, request_id="first", text="first")["ok"] == 1
            wait_for(lambda: "[turn completed]" in records(log, sid) or None)                # a session with a conversation can be resumed
            # the window learns a new generation from the host once a second; when the machine is slow it can learn it before it has read the key, and then the
            # input is right and goes (measured: 3 of 200 runs under load, the window's row at generation 2, the picture sent): that attempt proves nothing,
            # so it is told apart by what happened to the picture and made again
            for attempt in range(6):
                sent_before = len(picture_sent(received))
                window_pid = w.proc.pid
                os.kill(window_pid, signal.SIGSTOP)
                try:
                    gen = status_of(conn, sid)["generation"]
                    closed = conn.call(op="session.close", session=sid, generation=gen)
                    assert closed["ok"] == 1, closed
                    wait_for(lambda: status_of(conn, sid)["state"] == "disconnected" or None)
                    again = conn.call(op="session.resume", session=sid, generation=gen)
                    assert again["ok"] == 1, again
                    wait_for(lambda: status_of(conn, sid)["state"] == "running" or None)
                    subprocess.run(["xdotool", "key", "Return"], env=w.env, check=True)
                finally:
                    os.kill(window_pid, signal.SIGCONT)
                try:
                    state = wait_for(lambda: (e := editor(w)) and e["note"] and "Not sent" in e["note"] and e, timeout=5)
                    break
                except AssertionError:
                    assert len(picture_sent(received)) == sent_before + 1 and editor(w)["picture"] is None, "neither refused nor sent"
                    wait_for(lambda: os.listdir(pasted_dir(w)) == [] or None)
                    owners.png(png)
                    fast_type(w, "kept text")
                    press_paste(w)
                    wait_for(lambda: editor(w)["picture"] or None)
            else:
                raise AssertionError("six attempts and the window knew the new generation before the key every time")
            assert state["note"] == "Not sent: the session changed meanwhile; look at it and send again. The text is back in the box.", state["note"]
            assert state["text"] == "kept text" and state["picture"] is not None, state
            assert len(picture_sent(received)) == sent_before and os.listdir(pasted_dir(w)) == []
