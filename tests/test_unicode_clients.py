import json
import os
import socket
import subprocess
import threading
import time

import pytest

from lico_measure.ime import InputMethod, _focus
from lico_measure.p02 import ProtoConn as Conn
from lico_measure.rig import Client, Xvfb, private_dir
from lico_measure.xpixel import PixelReader

from tests.test_codex_adapter import codex_host, new_session, sent, texts, wait_for

CLIENTS = ["a2", "c2"]
DANGEROUS = ["\x1b", "\x07", "\x08", "‮", "‬"]


def shown_lines(client):
    client.send("lines")
    return client.expect("lines")["lines"]


HOSTILE = "\x1b]52;c;cHduZWQ=\x07\x1b]0;pwned\x07\x1b[2J\x08\x08\r visible\u202e evil\u202c"


def compact(message):
    return (json.dumps(message, separators=(",", ":")) + "\n").encode()   # the wire form of the real host


def hostile_peer(path, sid):
    """A stand-in host that sends raw control sequences, which the real host never would:
    the client must refuse them on its own."""
    server = socket.socket(socket.AF_UNIX)
    server.bind(path)
    server.listen(1)

    def serve():
        conn, _ = server.accept()
        reader = conn.makefile("rb")
        for raw in reader:
            request = json.loads(raw)
            if request.get("op") == "view.attach":
                snapshot = {"session": sid, "generation": 1, "seq": 0, "text": True, "lines": [[0, "snap " + HOSTILE]]}
                reply = {"ok": 1, "session": sid, "generation": 1, "gap": False, "from": 1, "view": "v1", "snapshot": snapshot}
                conn.sendall(compact(reply))
                for seq in (1, 2):
                    rec = {"v": 1, "ev": "rec", "session": sid, "generation": 1, "seq": seq, "data": "", "text": f"rec{seq} " + HOSTILE}
                    conn.sendall(compact(rec))
        conn.close()

    threading.Thread(target=serve, daemon=True).start()
    return server


def window_title(display):
    env = dict(os.environ, DISPLAY=display)
    ids = subprocess.run(["xdotool", "search", "--name", "Lico candidate"], env=env, capture_output=True, text=True).stdout.split()
    assert ids, "the client window is missing"
    out = subprocess.run(["xprop", "-id", ids[-1], "WM_NAME"], env=env, capture_output=True, text=True).stdout
    return out


@pytest.mark.parametrize("name", CLIENTS)
def test_untrusted_output_cannot_act_on_the_display_or_clipboard(name):
    with private_dir() as directory:
        xvfb = Xvfb()
        host = client = reader = None
        try:
            host, log, received = codex_host(directory)
            conn, sid, generation = new_session(host)
            client = Client(name, host.socket, xvfb.display)
            client.send(f"show {sid}")
            client.expect("state", "subscribed")
            reader = PixelReader(xvfb.display)
            title_before = window_title(xvfb.display)
            conn.call(op="input", session=sid, generation=generation, request_id="esc", text="escape")
            wait_for(lambda: "visible after escapes" in texts(log, sid))
            time.sleep(0.8)
            # the host's own record of the output carries none of the control sequences
            records = texts(log, sid)
            assert not any(c in line for line in records for c in DANGEROUS)
            assert all(len(line.encode()) <= 4096 for line in records)         # a very long line is cut
            # the client did not obey any of them: nothing owns the clipboard, the title is unchanged
            assert reader.selection_owner("CLIPBOARD") == 0 and reader.selection_owner("PRIMARY") == 0
            assert window_title(xvfb.display) == title_before and "pwned" not in window_title(xvfb.display)
            lines = shown_lines(client)
            assert any(line.endswith("visible after escapes") for line in lines), lines[-5:]
            assert any(line.endswith("before") for line in lines) or len(lines) >= 5      # the screen was not cleared
            assert not any(c in line for line in lines for c in DANGEROUS)
            # the client alone: raw control sequences straight from a peer are shown without their effect
            client.kill()
            peer_path = os.path.join(directory, "peer.sock")
            peer = hostile_peer(peer_path, sid)
            client = Client(name, peer_path, xvfb.display)
            client.send(f"show {sid}")
            client.expect("state", "subscribed")
            time.sleep(0.8)
            lines = shown_lines(client)
            assert any(line.startswith("rec2 ") and line.endswith("visible evil") for line in lines), lines
            assert any(line.startswith("snap ") for line in lines), lines
            assert not any(c in line for line in lines for c in DANGEROUS + ["\r", "\x00"])
            assert reader.selection_owner("CLIPBOARD") == 0 and reader.selection_owner("PRIMARY") == 0
            assert window_title(xvfb.display) == title_before
            peer.close()
        finally:
            if client and client.proc.poll() is None:
                client.kill()
            if reader:
                reader.close()
            if host:
                host.stop()
            xvfb.stop()


@pytest.mark.parametrize("name", CLIENTS)
def test_unicode_and_korean_round_trip_through_input_method_and_agent(name):
    mixed = "hello é é 漢字 שלום 🙂 "
    with private_dir() as directory:
        xvfb = Xvfb()
        host = method = client = None
        try:
            host, log, received = codex_host(directory)
            conn, sid, generation = new_session(host)
            method = InputMethod(xvfb.display)
            client = Client(name, host.socket, xvfb.display, extra_env=method.client_env)
            client.send(f"show {sid}")
            client.expect("state", "subscribed")
            time.sleep(1.0)
            _focus(xvfb.display)
            time.sleep(0.6)
            env = dict(os.environ, DISPLAY=xvfb.display)
            # mixed scripts typed directly (the input method is still in Latin mode), then Korean through it
            subprocess.run(["xdotool", "type", "--delay", "40", mixed], env=env, check=True)
            time.sleep(0.5)
            subprocess.run(["xdotool", "key", "shift+space"], env=env, check=True)        # Latin to Hangul
            time.sleep(0.5)
            subprocess.run(["xdotool", "key", "--delay", "60", "d", "k", "s", "s", "u", "d", "g", "k", "t", "p", "d", "y", "space", "Return"], env=env, check=True)
            expected = mixed + "안녕하세요 "
            turns = wait_for(lambda: [m for m in sent(received) if m.get("method") == "turn/start" and m["params"]["input"][0]["text"] != ""])
            assert turns[-1]["params"]["input"][0]["text"] == expected, turns[-1]["params"]["input"][0]["text"]
            wait_for(lambda: ("echo: " + expected) in texts(log, sid))
            lines = shown_lines(client)
            assert any(line.endswith("echo: " + expected) or line == "echo: " + expected for line in lines), lines[-3:]
        finally:
            if client and client.proc.poll() is None:
                client.kill()
            if method:
                method.stop()
            if host:
                host.stop()
            xvfb.stop()
