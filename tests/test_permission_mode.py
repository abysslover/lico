import json
import os
import subprocess
import time

from lico_measure.p02 import ProtoConn as Conn
from lico_measure.rig import Xvfb, private_dir

from tests.test_claude_adapter import argv_of, claude_host, lines_of, new_claude, records, status_of
from tests.test_codex_adapter import codex_host, sent, wait_for
from tests.test_first_view import responses, start_client, ui, xdo


def test_a_new_session_skips_permissions_by_default_asks_the_window_for_the_rest_and_ask_mode_is_one_choice_away():
    with private_dir() as directory:
        host, log, received = claude_host(directory)
        try:
            conn = Conn(host.socket)
            auto, _ = new_claude(conn)
            ask, _ = new_claude(conn, mode="default")
            wait_for(lambda: len(argv_of(received)) >= 2)
            first, second = argv_of(received)[:2]
            assert first[first.index("--permission-mode") + 1] == "bypassPermissions" and second[second.index("--permission-mode") + 1] == "default"
            for argv in (first, second):
                assert argv[argv.index("--permission-prompt-tool") + 1] == "stdio"                      # always: what still needs a person reaches the window
            assert status_of(conn, auto)["mode"] == "auto" and status_of(conn, ask)["mode"] == "default"
        finally:
            host.stop()
    with private_dir() as directory:
        host, log, received = codex_host(directory)
        try:
            conn = Conn(host.socket)
            conn.call(op="session.create", kind="codex")
            conn.call(op="session.create", kind="codex", mode="default")
            starts = [m["params"] for m in sent(received) if m.get("method") == "thread/start"]
            assert (starts[0]["approvalPolicy"], starts[0]["sandbox"]) == ("never", "danger-full-access")
            assert (starts[1]["approvalPolicy"], starts[1]["sandbox"]) == ("untrusted", "read-only")
        finally:
            host.stop()


def test_ask_mode_turns_a_tool_into_an_approval_answered_exactly_and_auto_mode_asks_only_what_always_needs_a_person():
    with private_dir() as directory:
        host, log, received = claude_host(directory)
        try:
            conn = Conn(host.socket)
            ask, generation = new_claude(conn, mode="default")
            conn.call(op="input", session=ask, generation=generation, request_id="a", text="fixture")
            wait_for(lambda: status_of(conn, ask).get("approvals"))
            first = status_of(conn, ask)["approvals"][0]
            assert conn.call(op="approval.answer", session=ask, generation=generation, approval_id=first["approval_id"], answer="allow")["ok"] == 1
            replies = wait_for(lambda: [json.loads(l)["response"] for l in lines_of(received) if l.startswith("{") and json.loads(l).get("type") == "control_response"])      # the program logs it a moment after the reply
            assert replies[0]["response"]["behavior"] == "allow" and "updatedInput" in replies[0]["response"]      # the exact shape: allow with the original input
            wait_for(lambda: len(status_of(conn, ask).get("approvals", [])) == 1 and status_of(conn, ask)["approvals"][0]["approval_id"] != first["approval_id"])
            second = status_of(conn, ask)["approvals"][0]
            assert conn.call(op="approval.answer", session=ask, generation=generation, approval_id=second["approval_id"], answer="deny")["ok"] == 1
            wait_for(lambda: "[turn completed]" in records(log, ask))
            denied = [json.loads(l)["response"]["response"] for l in lines_of(received) if l.startswith("{") and json.loads(l).get("type") == "control_response"][-1]
            assert denied["behavior"] == "deny" and denied["message"]                                          # deny with a message
            # the default (auto) session: an ordinary tool needs no approval, a question to the person still does
            auto, agen = new_claude(conn)
            conn.call(op="input", session=auto, generation=agen, request_id="b", text="fixture")
            wait_for(lambda: "[turn completed]" in records(log, auto) or status_of(conn, auto).get("approvals"))
            assert not status_of(conn, auto).get("approvals") and any("tool ran without approval" in r for r in records(log, auto))
            wait_for(lambda: "[turn completed]" in records(log, auto))
            conn.call(op="input", session=auto, generation=agen, request_id="c", text="askuser")
            wait_for(lambda: status_of(conn, auto).get("approvals"))
            ask_input = status_of(conn, auto)["approvals"][0]
            assert "AskUserQuestion" in ask_input["prompt"]
            assert conn.call(op="approval.answer", session=auto, generation=agen, approval_id=ask_input["approval_id"], answer="allow")["ok"] == 1
            wait_for(lambda: any("question answered" in r for r in records(log, auto)))
        finally:
            host.stop()


def test_the_window_answers_a_pending_approval_with_y_and_n_and_says_what_is_asked():
    with private_dir() as directory:
        xvfb = Xvfb()
        host = client = None
        try:
            host, log, received = claude_host(directory)
            conn = Conn(host.socket)
            sid, generation = new_claude(conn, mode="default")
            conn.call(op="input", session=sid, generation=generation, request_id="a", text="fixture")
            wait_for(lambda: status_of(conn, sid).get("approvals"))
            client = start_client(host, xvfb.display)
            client.send(f"show {sid}")
            client.expect("state", "subscribed")
            state = ui(client, lambda u: u["approval_bar"])
            assert "run: ls -la" in state["approval_bar"] and "mode: asks for approval" in state["header"]
            xdo(xvfb.display, "key", "y")
            wait_for(lambda: len(responses(received)) == 1)
            assert responses(received)[0]["response"]["behavior"] == "allow"
            state = ui(client, lambda u: u["approval_bar"] and "Write" in u["approval_bar"])             # the next one is offered, with the name of its tool
            xdo(xvfb.display, "key", "n")
            wait_for(lambda: len(responses(received)) == 2)
            assert responses(received)[1]["response"]["behavior"] == "deny"
            state = ui(client, lambda u: not u["approval_bar"])
            assert state["input"] == "" if "input" in state else True                                    # the keys were answers, not typing
        finally:
            if client and client.proc.poll() is None:
                client.kill()
            if host:
                host.stop()
            xvfb.stop()


def bright_columns(png_path, x_from, x_to, y_from=0, y_to=None, threshold=140):
    """The leftmost and rightmost columns of light pixels inside a box of a window capture."""
    data = subprocess.run(["convert", str(png_path), "-colorspace", "Gray", "-depth", "8", "gray:-"], capture_output=True, check=True).stdout
    size = subprocess.run(["identify", "-format", "%w %h", str(png_path)], capture_output=True, text=True).stdout.split()
    width, height = int(size[0]), int(size[1])
    y_to = height if y_to is None else y_to
    cols = [x for x in range(x_from, min(x_to, width)) if any(data[y * width + x] > threshold for y in range(y_from, y_to))]
    return (cols[0], cols[-1], width, height) if cols else (None, None, width, height)


def window_shot(display, directory, name):
    from tests.test_first_view import window_id
    path = f"{directory}/{name}.png"
    subprocess.run(["import", "-window", window_id(display), path], env={"DISPLAY": display, "PATH": "/usr/bin:/bin"}, check=True)
    return path


def test_no_glyph_is_cut_at_the_right_edge_of_the_output_or_the_left_edge_of_the_list():
    import lico_measure.rig as rig
    from tests.test_first_view import window_id
    from tests.test_login_window import panel_host
    from tests.account_helpers import account

    for scale, width in ((1, 1203), (1, 1171), (2, 1203)):
        with private_dir() as directory:
            xvfb = Xvfb()
            host = client = None
            rig.DISPLAY_SCALE = scale
            try:
                host, log, received = claude_host(directory)
                conn = Conn(host.socket)
                made = account(conn, "claude_initial", "claude")
                sid, generation = new_claude(conn, account=made)
                conn.call(op="input", session=sid, generation=generation, request_id="h", text="hangul:600")
                wait_for(lambda: "[turn completed]" in records(log, sid))
                client = start_client(host, xvfb.display)
                client.send(f"show {sid}")
                client.expect("state", "subscribed")
                ui(client, lambda u: u["lines_buffered"] > 0)
                subprocess.run(["xdotool", "windowsize", window_id(xvfb.display), str(width), "800"], env={"DISPLAY": xvfb.display, "PATH": "/usr/bin:/bin"}, check=True)
                time.sleep(1.5)
                path = window_shot(xvfb.display, directory, f"w{scale}_{width}")
                list_w = 280 * scale
                _, right, w, h = bright_columns(path, list_w + 4, 100000)
                assert right is not None and right < w - 3 * scale, (scale, width, right, w)            # the rightmost light column is inside the area: no half glyph at the edge
                left, _, _, _ = bright_columns(path, 0, list_w - 2, 40 * scale, 58 * scale)                      # the list's first heading row
                assert left is not None and left >= 2, (scale, width, left)                             # its first letter is whole, not cut at the edge
            finally:
                rig.DISPLAY_SCALE = 1
                if client and client.proc.poll() is None:
                    client.kill()
                if host:
                    host.stop()
                xvfb.stop()
