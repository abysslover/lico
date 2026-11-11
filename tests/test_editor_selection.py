"""M03-P03: prompt editor mouse/keyboard selection, copy/cut, bounded per-session undo/redo, and resize reflow."""

from __future__ import annotations

import subprocess
import time

from lico_measure.p02 import ProtoConn as Conn
from lico_measure.rig import Xvfb, private_dir

from tests.account_helpers import account
from tests.gui_helpers import centre, click_at, drawn, item, menu_client
from tests.test_claude_adapter import claude_host, new_claude
from tests.test_codex_adapter import wait_for
from tests.test_first_view import ui, window_id, xdo


def _clipboard_utf8(display: str) -> str:
    done = subprocess.run(
        ["xclip", "-o", "-selection", "clipboard", "-t", "UTF8_STRING"],
        env={"DISPLAY": display, "PATH": "/usr/bin:/bin"},
        capture_output=True,
        timeout=10,
    )
    assert done.returncode == 0, done.stderr
    return done.stdout.decode("utf-8")


def test_mouse_click_drag_shift_selection_copy_cut_undo_redo_and_ctrl_a_accounts():
    with private_dir() as directory:
        xvfb = Xvfb()
        display = xvfb.display
        host = client = None
        try:
            host, _log, _received = claude_host(directory)
            conn = Conn(host.socket)
            acc = account(conn, "Work", "claude")
            s1, _ = new_claude(conn, account=acc, mode="default")
            s2, _ = new_claude(conn, account=acc, mode="default")
            client = menu_client(host, display)
            client.send(f"show {s1}")
            client.expect("state", "subscribed")
            wid = window_id(display)

            ui(client, lambda u: u["selected"] == s1 and u["editor"]["rect"] is not None)
            xdo(display, "type", "--delay", "15", "alpha beta gamma")
            ui(client, lambda u: u["editor"]["text"] == "alpha beta gamma")

            # Shift+Ctrl+Left selects "gamma" in the prompt box with highlight rects and Session menu support.
            xdo(display, "key", "ctrl+shift+Left")
            sel_state = ui(client, lambda u: u["editor"]["selection"] == "gamma" and len(u["editor"]["sel_rects"]) == 1)
            assert sel_state["editor"]["selection_range"] == [11, 16]

            xdo(display, "key", "alt+s")
            m = ui(client, lambda u: drawn(u, 1))["menu"]
            assert item(m, "Copy")["label"] == "Copy selected text" and item(m, "Copy")["enabled"] is True
            assert item(m, "Cut selection")["enabled"] is True
            xdo(display, "key", "Escape")
            ui(client, lambda u: u["menu"]["open"] is None)

            # Ctrl+C copies prompt selection when no conversation lines are selected.
            xdo(display, "key", "ctrl+c")
            copied_state = ui(client, lambda u: u["copied"] == 1 and "Copied selection." in u["header"])
            assert copied_state["selection"] is None
            assert _clipboard_utf8(display) == "gamma"

            # Ctrl+X cuts prompt selection onto CLIPBOARD and records undo.
            xdo(display, "key", "ctrl+x")
            ui(
                client,
                lambda u: u["editor"]["text"] == "alpha beta "
                and u["editor"]["selection"] is None
                and "Cut selection." in u["header"],
            )
            assert _clipboard_utf8(display) == "gamma"

            # Ctrl+Z undoes the cut; Ctrl+Y and Ctrl+Shift+Z redo it.
            xdo(display, "key", "ctrl+z")
            ui(client, lambda u: u["editor"]["text"] == "alpha beta gamma" and u["editor"]["can_redo"] is True)
            xdo(display, "key", "ctrl+y")
            ui(client, lambda u: u["editor"]["text"] == "alpha beta ")
            xdo(display, "key", "ctrl+z")
            ui(client, lambda u: u["editor"]["text"] == "alpha beta gamma")
            xdo(display, "key", "ctrl+shift+Z")
            ui(client, lambda u: u["editor"]["text"] == "alpha beta ")
            xdo(display, "key", "ctrl+z")
            ed = ui(client, lambda u: u["editor"]["text"] == "alpha beta gamma")["editor"]

            # Mouse click places caret near start; mouse drag selects across prompt text; Esc clears only prompt selection.
            x0, y0, x1, y1 = ed["rect"]
            ymid = int((y0 + y1) / 2)
            xdo(display, "mousemove", "--window", wid, str(int(x0 + 20)), str(ymid), "click", "1")
            clicked = ui(client, lambda u: u["editor"]["cursor"] <= 2 and u["editor"]["selection"] is None)
            assert clicked["editor"]["cursor"] <= 2

            xdo(
                display,
                "mousemove",
                "--window",
                wid,
                str(int(x0 + 20)),
                str(ymid),
                "mousedown",
                "1",
                "mousemove",
                "--window",
                wid,
                str(int(min(x0 + 130, x1 - 10))),
                str(ymid),
                "mouseup",
                "1",
            )
            dragged = ui(
                client,
                lambda u: u["editor"]["selection"] is not None
                and len(u["editor"]["selection"]) >= 3
                and bool(u["editor"]["sel_rects"]),
            )
            assert dragged["editor"]["sel_rects"]

            xdo(display, "key", "Escape")
            cleared = ui(client, lambda u: u["editor"]["selection"] is None)
            assert cleared["selected"] == s1

            # Ctrl+A MUST open Accounts (never select-all in prompt), and closing the panel preserves draft + undo stack.
            xdo(display, "key", "ctrl+a")
            ui(client, lambda u: u["panel"]["open"] is True and u["panel"]["title"] == "Accounts")
            xdo(display, "key", "Escape")
            restored = ui(client, lambda u: u["panel"] is None and u["editor"]["text"] == "alpha beta gamma")
            assert restored["editor"]["can_undo"] is True

            # Session switching isolates undo/redo per session.
            row2 = next(r for r in restored["layout"]["rows"] if r[2] == s2)
            click_at(display, 100, (row2[0] + row2[1]) / 2)
            ui(client, lambda u: u["selected"] == s2 and u["editor"]["text"] == "" and u["editor"]["can_undo"] is False)
            xdo(display, "type", "--delay", "15", "two")
            ui(client, lambda u: u["editor"]["text"] == "two" and u["editor"]["can_undo"] is True)
            xdo(display, "key", "ctrl+z")
            ui(client, lambda u: u["editor"]["text"] == "" and u["editor"]["can_redo"] is True)

            row1 = next(r for r in ui(client)["layout"]["rows"] if r[2] == s1)
            click_at(display, 100, (row1[0] + row1[1]) / 2)
            back = ui(client, lambda u: u["selected"] == s1 and u["editor"]["text"] == "alpha beta gamma")
            assert back["editor"]["can_undo"] is True
        finally:
            if client and client.proc.poll() is None:
                client.kill()
            if host:
                host.stop()
            xvfb.stop()


def test_window_resize_reflows_wrapped_lines_and_preserves_scroll_anchor():
    with private_dir() as directory:
        xvfb = Xvfb()
        display = xvfb.display
        host = client = None
        try:
            host, _log, _received = claude_host(directory)
            conn = Conn(host.socket)
            s1, _ = new_claude(conn, mode="default")
            client = menu_client(host, display)
            client.send(f"show {s1}")
            client.expect("state", "subscribed")
            wid = window_id(display)

            long_block = "\n".join(f"line-{i:02d}: " + ("wrapsegment " * 14) for i in range(45))
            conn.call(op="input", session=s1, generation=1, request_id="r1", text=long_block)
            ui(client, lambda u: u["lines_buffered"] >= 45 and bool(u["rows"]) and u["rows"][0]["state_text"] == "idle")

            xdo(display, "type", "--delay", "15", "draft across resize")
            ui(client, lambda u: u["editor"]["text"] == "draft across resize")

            xdo(display, "key", "Page_Up")
            before = ui(
                client,
                lambda u: u["scroll"] > 0
                and bool(u["layout"]["lines"])
                and u["visible"][1] == u["lines_buffered"] - u["scroll"],
            )
            anchor_end = before["visible"][1]
            scroll_before = before["scroll"]
            wide_heights = [ln[1] - ln[0] for ln in before["layout"]["lines"]]
            frames_before_narrow = before["frame"]["frames"]

            xdo(display, "windowsize", wid, "860", "640")
            narrow = ui(
                client,
                lambda u: u["layout"]["window_size"] == [860, 640]
                and u["frame"]["frames"] > frames_before_narrow
                and bool(u["layout"]["lines"])
                and max(ln[1] - ln[0] for ln in u["layout"]["lines"]) > max(wide_heights),
            )
            narrow_heights = [ln[1] - ln[0] for ln in narrow["layout"]["lines"]]
            assert narrow["scroll"] == scroll_before
            assert narrow["visible"][1] == anchor_end
            assert narrow["editor"]["text"] == "draft across resize"
            assert max(narrow_heights) > max(wide_heights)
            frames_before_restore = narrow["frame"]["frames"]

            xdo(display, "windowsize", wid, "1200", "800")
            restored = ui(
                client,
                lambda u: u["layout"]["window_size"] == [1200, 800]
                and u["frame"]["frames"] > frames_before_restore
                and bool(u["layout"]["lines"])
                and max(ln[1] - ln[0] for ln in u["layout"]["lines"]) == max(wide_heights),
            )
            assert restored["scroll"] == scroll_before
            assert restored["visible"][1] == anchor_end
            assert restored["editor"]["text"] == "draft across resize"
            rx0, ry0, rx1, ry1 = restored["editor"]["rect"]
            assert rx0 < rx1 <= 1200 and 0 < ry0 < ry1 <= 800
            time.sleep(0.05)
        finally:
            if client and client.proc.poll() is None:
                client.kill()
            if host:
                host.stop()
            xvfb.stop()
