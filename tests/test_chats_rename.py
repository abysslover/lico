"""M10-P01: the `past` -> `chats` rename is complete (no alias, no old name), and a host or a command line of another build
says so in one sentence instead of printing a bare `unknown_op`."""

from __future__ import annotations

import json
import os
import re
import socket
import subprocess
import threading
from pathlib import Path

from lico_measure.p02 import LICO, ProtoConn as Conn, cli, start
from lico_measure.rig import private_dir

REPO = Path(__file__).resolve().parents[1]

# Identifiers and labels of the retired name. English prose ("past the end", "in the past") and `paste`/`pasted` do not match.
RETIRED = re.compile(
    r"session\.past|sessions/past|/v1/past|session past|lico past|session_past|scan_ssh_past|sidebar_past|PastItem|PastEntry|PastSidebar|PastResume"
    r"|SidebarPast|past_tab_rect|past_total|past_page|past_entries|past_loading|past_error|past_load_count|past_start_fetch|past_sync|past_step"
    r"|past_activate|draw_past|insert_past|on_past_async|parse_past|calc_past|past_target|\[ ?Past ?\]|\bPast tab\b|\"Past\"|in Past\b|Past /"
    r"|ui\.sidebar\.past|sidebar\.past\b|\"tab\"\] == \"past\"|mod past\b|crate::past\b|past::|test_past_|past/mod|past/seed|\.past\b"
)
ROOTS = ["src", "tests", "README.md", "CONTRIBUTING.md", "doc/manual_end_to_end.md", "doc/design", "doc/README.md"]
SUFFIXES = {".rs", ".py", ".md", ".toml", ".json", ".sh"}


def _tracked_text_files() -> list[Path]:
    names = subprocess.run(["git", "ls-files", "-z", "--", *ROOTS], cwd=REPO, capture_output=True, check=True).stdout.split(b"\0")
    return [REPO / n.decode() for n in names if n and Path(n.decode()).suffix in SUFFIXES]


def test_no_retired_name_is_left_in_the_source_the_tests_or_the_living_docs():
    # this file names the retired words to look for, and the end-to-end test asserts that the old operation is refused: both are meant to say them
    says_it_on_purpose = {Path(__file__).resolve(), (REPO / "tests" / "test_chats_end_to_end.py").resolve()}
    found = []
    for path in _tracked_text_files():
        if path.resolve() in says_it_on_purpose or not path.exists():
            continue
        for number, line in enumerate(path.read_text(encoding="utf-8", errors="replace").splitlines(), 1):
            if RETIRED.search(line):
                found.append(f"{path.relative_to(REPO)}:{number}: {line.strip()[:100]}")
    assert not found, "retired name(s) left:\n" + "\n".join(found[:40])


def test_the_old_op_is_an_ordinary_unknown_op_and_the_new_one_is_listed_and_answered():
    with private_dir() as directory:
        host, _log = start(directory, sessions=0, producing=0, history=0)
        try:
            conn = Conn(host.socket)
            old = conn.call(op="session.past", **{"global": True})
            assert old["ok"] == 0 and old["error"] == "unknown_op", old
            new = conn.call(op="session.chats", **{"global": True})
            assert new["ok"] == 1 and new["items"] == [] and new["total"] == 0, new
            hello = conn.call(op="hello")
            assert "session.chats" in hello["ops"] and "session.past" not in hello["ops"]
            caps = json.loads(cli(host, "capabilities").stdout)
            text = json.dumps(caps)
            assert "session.chats" in text and "/v1/sessions/chats" in text and "past" not in text.replace("pasted", ""), text[:300]
        finally:
            host.stop()


def _fake_old_host(path: str, build: str, seen: list[str]) -> threading.Thread:
    """A host of another build: it answers `status` with its build and every other op with `unknown_op`."""
    server = socket.socket(socket.AF_UNIX)
    server.bind(path)
    server.listen(1)

    def serve() -> None:
        conn, _ = server.accept()
        with conn, conn.makefile("rwb") as stream:
            for line in stream:
                request = json.loads(line)
                seen.append(request["op"])
                if request["op"] == "status":
                    reply = {"v": 1, "ok": 1, "build": build, "sessions": []}
                else:
                    reply = {"v": 1, "ok": 0, "error": "unknown_op", "id": request.get("id")}
                stream.write((json.dumps(reply) + "\n").encode())
                stream.flush()
        server.close()

    thread = threading.Thread(target=serve, daemon=True)
    thread.start()
    return thread


def test_the_command_line_explains_a_host_of_another_build_in_one_sentence():
    with private_dir() as directory:
        path = os.path.join(directory, "old.sock")
        seen: list[str] = []
        thread = _fake_old_host(path, "oldbuild0123456789", seen)
        done = subprocess.run([str(LICO), "--socket", path, "chat", "list", "--global"], capture_output=True, text=True, timeout=20)
        thread.join(5)
        assert done.returncode == 2, (done.returncode, done.stdout, done.stderr)
        assert "session.chats" in seen
        message = done.stderr.strip()
        assert len(message.splitlines()) == 1, message
        assert "session.chats" in message and "oldbuild0123456789" in message and "lico host stop" in message, message
        assert json.loads(done.stdout)["error"] == "unknown_op"


def test_the_old_command_is_gone_and_the_new_one_lists_chats():
    with private_dir() as directory:
        host, _log = start(directory, sessions=0, producing=0, history=0)
        try:
            gone = cli(host, "session", "past", "--global")
            assert gone.returncode != 0 and "usage" in (gone.stdout + gone.stderr).lower()
            listed = cli(host, "chat", "list", "--global")
            assert listed.returncode == 0 and json.loads(listed.stdout)["total"] == 0, listed
            bare = cli(host, "chat")
            assert bare.returncode == 0 and json.loads(bare.stdout)["scope"] == "global", bare
        finally:
            host.stop()
