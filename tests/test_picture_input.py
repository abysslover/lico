"""M02-P04 S05a: the host side of a pasted picture (XF-05, SF-04), with a real host, the fake Claude program and no display.

The window asks the op `picture.slot` for a place, writes the PNG there, and sends `input` with the member `attachment` (the name). Claude gets an image block
FIRST and the text after it; the file is removed after every `input` that names one; every refusal has its own word and sends nothing to the program.
"""
import base64
import json
import os
import re
import sys
import threading
import time
from pathlib import Path

import pytest

from lico_measure.p02 import HOSTD_DIR, ProtoConn as Conn, read_log, start  # noqa: F401
from lico_measure.rig import private_dir
from tests.test_claude_adapter import (CAPABILITIES, FIXTURE, SIM, new_claude, records, sent_users, status_of, user_text, usual_umask)  # noqa: F401
from tests.test_clipboard_read import forged_head, make_png
from tests.test_codex_adapter import codex_host, new_session, wait_for
from tests.test_host_control import _http

MIB3 = 3 * 1024 * 1024
NAME = re.compile(r"[0-9a-f]{32}\.png")
HEAD_TEXT = "see the picture"


def claude_host(directory, extra=()):
    received = Path(directory) / "claude_received.txt"
    args = ["--claude", sys.executable, "--claude-arg", "-I", "--claude-arg", str(SIM), "--claude-arg", "--received", "--claude-arg", str(received),
            "--claude-arg", "--fixture", "--claude-arg", str(FIXTURE), "--cred-bin", str(HOSTD_DIR / "lico-cred"),
            "--cred-dir", str(Path(directory) / "credentials"), *extra]
    with usual_umask():
        host, log = start(directory, extra=args, sessions=0, producing=0, history=0, retain=100000)
    return host, log, received


def slot(conn):
    reply = conn.call(op="picture.slot")
    assert reply["ok"] == 1, reply
    return reply["name"], Path(reply["path"])


def fill(path, data):
    """What the window does: open the handed-out file for writing WITHOUT creating it, and write."""
    fd = os.open(path, os.O_WRONLY)
    try:
        view = memoryview(data)
        while view:
            view = view[os.write(fd, view):]
    finally:
        os.close(fd)


def padded(head, total):
    return head + os.urandom(total - len(head))


def pasted_dir(directory):
    return Path(directory) / "pasted"


def mode(path):
    return os.lstat(path).st_mode & 0o7777


def pictures_sent(received):
    return [m for m in sent_users(received) if any(b.get("type") == "image" for b in m["message"]["content"])]


@pytest.fixture(scope="module")
def world():
    with private_dir() as directory:
        host, log, received = claude_host(directory, ["--http-port", "auto", "--http-token-file", str(Path(directory) / "http.token")])
        try:
            conn = Conn(host.socket, timeout=15)
            sid, generation = new_claude(conn, mode="default")
            yield type("World", (), dict(directory=directory, host=host, log=log, received=received, conn=conn, sid=sid, generation=generation, counter=[0]))
        finally:
            host.stop()


def send(world, name, text=HEAD_TEXT, **extra):
    world.counter[0] += 1
    rid = f"p-{world.counter[0]}"
    request = dict(op="input", session=extra.pop("session", world.sid), generation=extra.pop("generation", world.generation), request_id=extra.pop("request_id", rid), text=text)
    if name is not None:
        request["attachment"] = name
    reply = extra.pop("conn", world.conn).call(**request)
    return reply


# ------------------------------------------------------------------------------------------------------------------------------ the slot
def test_a_slot_is_an_empty_private_file_in_a_private_folder_nine_are_busy_and_old_ones_are_swept():
    with private_dir() as directory:
        host, log, received = claude_host(directory)
        try:
            conn = Conn(host.socket)
            first = conn.call(op="picture.slot")
            assert set(first) == {"v", "ok", "name", "path"} and first["ok"] == 1, first
            assert NAME.fullmatch(first["name"]) and Path(first["path"]).is_absolute() and Path(first["path"]).name == first["name"]
            assert Path(first["path"]).parent.resolve() == pasted_dir(directory).resolve(), "beside the handoff folder, in the state folder"
            assert (mode(pasted_dir(directory)), mode(first["path"]), os.stat(first["path"]).st_size) == (0o700, 0o600, 0)
            assert conn.call(op="picture.slot", id="x1")["id"] == "x1"
            names = {first["name"]}
            for _ in range(5):                                          # with the first and the one above: seven
                names.add(slot(conn)[0])
            assert len(names) == 6
            assert conn.call(op="picture.slot")["ok"] == 1 and len(os.listdir(pasted_dir(directory))) == 8
            busy = conn.call(op="picture.slot")
            assert busy["ok"] == 0 and busy["error"] == "busy" and len(os.listdir(pasted_dir(directory))) == 8, busy
            # a file 11 minutes old is swept by the next call, a young one stays
            old = time.time() - 660
            os.utime(first["path"], (old, old))
            again = conn.call(op="picture.slot")
            assert again["ok"] == 1 and not os.path.exists(first["path"]) and len(os.listdir(pasted_dir(directory))) == 8
            # the shape: no member but the common ones
            for extra in [dict(name="x"), dict(path="/x"), dict(attachment=again["name"]), dict(session="s"), dict(text="t")]:
                assert conn.call(op="picture.slot", **extra) == {"v": 1, "ok": 0, "error": "bad_request"}, extra
            # files left at the end of a host's life are gone after a restart; the folder is private again
            os.chmod(pasted_dir(directory), 0o755)
            host.stop()
            assert os.listdir(pasted_dir(directory))
            host, log, received = claude_host(directory)
            assert os.listdir(pasted_dir(directory)) == [] and mode(pasted_dir(directory)) == 0o700
            assert Conn(host.socket).call(op="picture.slot")["ok"] == 1
        finally:
            host.stop()


# ------------------------------------------------------------------------------------------------------------------------------ the picture reaches Claude
def test_a_picture_goes_to_claude_before_the_text_the_file_is_removed_and_the_transcript_says_so(world):
    data, _ = make_png(40, 30)
    assert len(data) < 1500, len(data)
    name, path = slot(world.conn)
    fill(path, data)
    reply = send(world, name, "what is in it?", request_id="pic-1")
    assert reply == {"v": 1, "ok": 1, "session": world.sid, "request_id": "pic-1", "state": "accepted", "duplicate": False}, reply
    assert not path.exists(), "removed once it is sent"
    messages = wait_for(lambda: pictures_sent(world.received))
    content = messages[0]["message"]["content"]
    assert [b["type"] for b in content] == ["image", "text"], "the image FIRST"
    assert content[0]["source"] == {"type": "base64", "media_type": "image/png", "data": base64.b64encode(data).decode()}
    assert base64.b64decode(content[0]["source"]["data"], validate=True) == data
    assert content[1]["text"] == "what is in it?" and user_text(messages[0]) == "what is in it?", "the program gets the text unchanged"
    line = [l for l in records(world.log, world.sid) if l.startswith(">>") and "what is in it?" in l]
    assert line == [">> [picture 1 KiB] what is in it?"], line
    # the input is a message like any other: a text without a picture has no image block and no marker
    assert send(world, None, "plain", request_id="plain-1")["ok"] == 1
    wait_for(lambda: any(user_text(m) == "plain" for m in sent_users(world.received)))
    plain = [m for m in sent_users(world.received) if user_text(m) == "plain"][0]
    assert [b["type"] for b in plain["message"]["content"]] == ["text"] and ">> plain" in records(world.log, world.sid)


def test_the_base64_text_is_right_for_a_file_of_every_length_modulo_three(world):
    """The padding of the last group (two, one or no `=`) is judged by an independent decoder, on three real sends."""
    for extra in (0, 1, 2):
        data = padded(forged_head(40, 30), 1200 + extra)
        name, path = slot(world.conn)
        fill(path, data)
        assert send(world, name, f"mod three {extra}")["ok"] == 1
        message = wait_for(lambda: [m for m in pictures_sent(world.received) if user_text(m) == f"mod three {extra}"])[0]
        coded = message["message"]["content"][0]["source"]["data"]
        assert coded.count("=") == (3 - len(data) % 3) % 3 and base64.b64decode(coded, validate=True) == data, (extra, coded[-8:])


def test_exactly_three_mib_passes_and_a_big_picture_costs_the_host_little_memory_for_a_short_time(world):
    data = padded(forged_head(40, 30), MIB3)
    name, path = slot(world.conn)
    fill(path, data)
    host_pid = world.host.pid
    with open(f"/proc/{host_pid}/clear_refs", "w") as handle:
        handle.write("5")                                       # the peak (VmHWM) starts again from what the host holds now
    before = rss_kb(host_pid)
    started = time.monotonic()
    reply = send(world, name, "big one", request_id="pic-big")
    took = time.monotonic() - started
    assert reply["ok"] == 1 and reply["state"] == "accepted", reply
    messages = wait_for(lambda: [m for m in pictures_sent(world.received) if user_text(m) == "big one"], timeout=20)
    assert base64.b64decode(messages[0]["message"]["content"][0]["source"]["data"], validate=True) == data, "a 3 MiB round trip against an independent decoder"
    time.sleep(0.5)
    peak_kb, after_kb = rss_peak_kb(host_pid), rss_kb(host_pid)
    print(f"S05a MEMORY host before {before / 1024:.1f} MiB, peak {peak_kb / 1024:.1f} MiB (+{(peak_kb - before) / 1024:.1f}), after {after_kb / 1024:.1f} MiB; the input call took {took * 1000:.0f} ms")
    assert peak_kb - before < 48 * 1024, "the picture, its text and the line exist at the same time, and are then dropped"
    assert after_kb - before < 24 * 1024
    assert not path.exists()
    # the provider echoes the whole message back; the reader's line limit cuts a line this size (see the limits of the slice mail), and the input stays usable
    state = world.conn.call(op="input.status", session=world.sid, request_id="pic-big")["state"]
    omitted = [l for l in records(world.log, world.sid) if "omitted" in l]
    print(f"S05a REPLAY input state {state}; records with omitted: {omitted}")
    assert state in ("submitted", "observed")


def rss_kb(pid):
    return int(re.search(r"VmRSS:\s+(\d+)", Path(f"/proc/{pid}/status").read_text()).group(1))


def rss_peak_kb(pid):
    return int(re.search(r"VmHWM:\s+(\d+)", Path(f"/proc/{pid}/status").read_text()).group(1))


# ------------------------------------------------------------------------------------------------------------------------------ the refusals
def refuse(world, name, word, text="refused one"):
    before = len(sent_users(world.received))
    reply = send(world, name, text)
    assert reply["ok"] == 0 and reply["error"] == word, (name, reply)
    return before


def test_every_refusal_has_its_word_removes_the_file_and_sends_nothing(world):
    before = len(sent_users(world.received))
    head = forged_head(40, 30)
    cases = []

    def case(label, word, make):
        name, path = slot(world.conn)
        made = make(path)
        reply = send(world, name, f"refused {label}")
        assert reply["ok"] == 0 and reply["error"] == word, (label, reply)
        assert not os.path.lexists(path), f"{label}: the file is gone afterwards"
        cases.append((label, reply["error"]))
        return made

    # a name that is not there; the other shapes are refused before the host looks at any file
    assert send(world, "0" * 32 + ".png")["error"] == "attachment_gone"
    for bad in ["../" + "0" * 29 + ".png", "a/" + "0" * 33 + ".png", "0" * 31 + "/.png", "0" * 32 + "..png", ("0" * 32 + ".png").upper(), "A" * 32 + ".png", "0" * 31 + ".png",
                "0" * 33 + ".png", "0" * 32 + ".jpg", "0" * 32, "", "/etc/passwd", "..", "0" * 32 + ".png\n"]:
        reply = send(world, bad)
        assert reply["ok"] == 0 and reply["error"] == "bad_request", (bad, reply)
    assert send(world, 7)["error"] == "bad_request" and send(world, None)["ok"] == 1 and send(world, ["x"])["error"] == "bad_request"
    plain = len(sent_users(world.received))
    # the files
    case("empty", "bad_image", lambda p: None)
    case("text", "bad_image", lambda p: fill(p, b"this is a text file; it is long enough to be read, and it is no picture"))
    case("short", "bad_image", lambda p: fill(p, head[:7]))
    case("zero side", "bad_image", lambda p: fill(p, forged_head(0, 30)))
    case("wide", "bad_image", lambda p: fill(p, padded(forged_head(8001, 30), 2000)))
    case("high", "bad_image", lambda p: fill(p, padded(forged_head(30, 8001), 2000)))
    case("too big by one", "attachment_too_large", lambda p: fill(p, padded(head, MIB3 + 1)))
    case("much too big", "attachment_too_large", lambda p: fill(p, padded(head, MIB3 + 100_000)))
    target = Path(world.directory) / "target-of-a-link.png"
    target.write_bytes(make_png(8, 8)[0])

    def link(path):
        path.unlink()
        path.symlink_to(target)

    case("link", "attachment_gone", link)
    assert target.read_bytes() == make_png(8, 8)[0], "the target of a link is never touched"

    def folder(path):
        path.unlink()
        path.mkdir()

    case("folder", "attachment_gone", folder)

    def fifo(path):
        path.unlink()
        os.mkfifo(path)

    started = time.monotonic()
    case("fifo", "attachment_gone", fifo)                   # no wait for a writer that never comes
    assert time.monotonic() - started < 5
    assert len(sent_users(world.received)) == plain, "no line reached the program"
    assert [w for _, w in cases] == ["bad_image"] * 6 + ["attachment_too_large"] * 2 + ["attachment_gone"] * 3
    assert before <= plain


def test_the_host_answers_a_repeat_with_its_state_and_does_not_touch_the_file(world):
    name, path = slot(world.conn)
    fill(path, make_png(10, 10)[0])
    first = send(world, name, "once", request_id="rep-1")
    assert first["ok"] == 1 and first["duplicate"] is False
    seen = len(pictures_sent(world.received))
    name2, path2 = slot(world.conn)
    # the repeat carries an EMPTY file (a picture that read would be bad_image): the answer is still the state of the first, so the file was not looked at
    again = send(world, name2, "once", request_id="rep-1")
    assert again["ok"] == 1 and again["duplicate"] is True and again["state"] in ("accepted", "submitted", "observed"), again
    assert not path2.exists(), "the file of a repeat is removed, unread"
    time.sleep(0.3)
    assert len(pictures_sent(world.received)) == seen, "no second line to the program"


def test_a_stale_generation_and_an_unknown_session_remove_the_file_too(world):
    for extra, word in [(dict(generation=world.generation + 5), "stale_generation"), (dict(session="s" + "0" * 16), "unknown_session")]:
        name, path = slot(world.conn)
        fill(path, make_png(10, 10)[0])
        reply = send(world, name, "never", **extra)
        assert reply["ok"] == 0 and reply["error"] == word, reply
        assert not path.exists()


def test_a_disconnected_and_an_exited_session_refuse_and_the_file_goes(world):
    conn = world.conn
    sid, generation = new_claude(conn, mode="default")
    assert conn.call(op="input", session=sid, generation=generation, request_id="boom", text="crash")["ok"] == 1
    wait_for(lambda: status_of(conn, sid)["state"] == "disconnected")
    name, path = slot(conn)
    fill(path, make_png(10, 10)[0])
    reply = send(world, name, "x", session=sid, generation=generation)
    assert (reply["ok"], reply["error"]) == (0, "session_disconnected") and not path.exists(), reply
    made = conn.call(op="session.create", kind="synthetic", history=3, rate=0)         # a program that ends by itself: an exited session
    assert made["ok"] == 1, made
    wait_for(lambda: status_of(conn, made["session"])["state"] == "exited")
    name, path = slot(conn)
    fill(path, make_png(10, 10)[0])
    reply = send(world, name, "x", session=made["session"], generation=made["generation"])
    assert (reply["ok"], reply["error"]) == (0, "session_exited") and not path.exists(), reply


def test_the_member_belongs_to_input_only(world):
    name, path = slot(world.conn)
    fill(path, make_png(10, 10)[0])
    others = [
        dict(op="transfer.send", to=world.sid, kind="text", text="t"),
        dict(op="clipboard.write", kind="text", text="t"),
        dict(op="input.status", session=world.sid, request_id="p-1"),
        dict(op="snapshot", session=world.sid),
        dict(op="status"),
        dict(op="hello"),
        dict(op="session.rename", session=world.sid, name="n"),
        dict(op="account.login.input", account="a", code="c"),
    ]
    for request in others:
        reply = world.conn.call(**request, attachment=name)
        assert reply == {"v": 1, "ok": 0, "error": "bad_request"}, (request["op"], reply)
    assert path.exists(), "a request that is refused by its shape never reaches the file"
    path.unlink()


def test_the_http_route_refuses_the_member_and_reads_no_file(world):
    port = world.host.http_port
    token = (Path(world.directory) / "http.token").read_text().strip()
    auth = {"Authorization": f"Bearer {token}"}
    name, path = slot(world.conn)
    data = make_png(10, 10)[0]
    fill(path, data)
    seen = len(pictures_sent(world.received))
    status, body = _http(port, "POST", f"/v1/sessions/{world.sid}/input", auth,
                         json.dumps({"generation": world.generation, "request_id": "h-pic", "text": "via http", "attachment": name}).encode())
    assert status == 400 and body["error"] == "bad_request", (status, body)
    assert path.read_bytes() == data, "the file was not read, not removed"
    time.sleep(0.2)
    assert len(pictures_sent(world.received)) == seen
    status, body = _http(port, "POST", f"/v1/sessions/{world.sid}/input", auth,
                         json.dumps({"generation": world.generation, "request_id": "h-plain", "text": "via http"}).encode())
    assert status == 200 and body["request_id"] == "h-plain", "the route itself works"
    path.unlink()


# ------------------------------------------------------------------------------------------------------------------------------ other kinds of session
def test_a_session_that_is_not_claude_refuses_a_picture_with_its_word_and_the_file_goes():
    with private_dir() as directory:
        host, log = start(directory, sessions=1, producing=0, history=3)
        try:
            conn = Conn(host.socket)
            sid = host.sessions[0]
            name, path = slot(conn)
            fill(path, make_png(10, 10)[0])
            reply = conn.call(op="input", session=sid, generation=1, request_id="syn-1", text="hi", attachment=name)
            assert (reply["ok"], reply["error"]) == (0, "attachment_unsupported") and not path.exists(), reply
            assert conn.call(op="input", session=sid, generation=1, request_id="syn-2", text="hi")["ok"] == 1, "the session itself is fine"
            name, path = slot(conn)                         # the kind is judged before the file: not even a missing file changes the word
            path.unlink()
            assert conn.call(op="input", session=sid, generation=1, request_id="syn-3", text="hi", attachment=name)["error"] == "attachment_unsupported"
        finally:
            host.stop()


def test_a_codex_session_refuses_a_picture_and_nothing_is_guessed_about_its_items():
    with private_dir() as directory:
        host, log, received = codex_host(directory)
        try:
            conn, sid, generation = new_session(host)
            name, path = slot(conn)
            fill(path, make_png(10, 10)[0])
            reply = conn.call(op="input", session=sid, generation=generation, request_id="cx-1", text="hi", attachment=name)
            assert (reply["ok"], reply["error"]) == (0, "attachment_unsupported") and not path.exists(), reply
            time.sleep(0.3)
            assert "hi" not in Path(received).read_text() if Path(received).exists() else True
        finally:
            host.stop()
