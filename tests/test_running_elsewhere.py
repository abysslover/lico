"""M11-P02: a chat that another program is writing right now is listed as `running elsewhere` and cannot be resumed from Lico (SES-07: a running external process is never
adopted), `New` stays allowed, and an empty Resume says where the other chats of the folder are. Signals: the transcript was modified in the last 180 s, or a same-user
`claude`/`codex` process that is not one of this host's own names the session id or runs in the chat's folder and started no later than the chat's last write. Fake
provider processes (a copy of `sleep` named `claude`), synthetic homes, mocked providers: no real terminal or home of the owner is looked at."""

from __future__ import annotations

import json
import os
import subprocess
import time
from pathlib import Path

from lico_measure.p02 import ProtoConn as Conn, cli, start
from lico_measure.rig import private_dir
from lico_protocol import v1
from tests.account_helpers import account
from tests.test_chats_pagination_and_resume import _seed_claude_jsonl
from tests.test_claude_adapter import argv_of
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_new_session_screen import claude_args


def _fake_claude(root: Path) -> str:
    """A program that /proc names `claude` (a script is named after its file) and that waits, a second at a time, for up to ten minutes; it ignores its arguments."""
    bindir = root / "fakebin"
    bindir.mkdir(exist_ok=True)
    target = bindir / "claude"
    target.write_text("#!/bin/sh\ni=0\nwhile [ $i -lt 600 ]; do sleep 1; i=$((i+1)); done\n")
    target.chmod(0o755)
    return str(target)


def _host(directory: str, root: Path, env: dict | None = None):
    home = root / "synthetic-home"
    home.mkdir(mode=0o700, exist_ok=True)
    c_args, c_rec = claude_args(root)
    base = {"HOME": str(home), "CLAUDE_CONFIG_DIR": str(home / ".claude"), "CODEX_HOME": str(home / ".codex"), **(env or {})}
    host, log = start(directory, extra=["--text", "--state-dir", str(root / "state"), *c_args], sessions=0, producing=0, env_overrides=base)
    return host, log, home, c_rec


def _chat(root: Path, acc: str, sid: str, proj: Path, mtime: float, title: str = "task") -> None:
    path = _seed_claude_jsonl(root / "state" / "accounts" / acc / "claude", "p", sid, str(proj), f"{title} {sid}", "answer", int(mtime))
    os.utime(path, (mtime, mtime))


def _items(conn: Conn, proj: Path, acc: str, **extra) -> dict:
    reply = conn.call(op="session.chats", cwd=str(proj), account=acc, limit=100, **extra)
    assert reply["ok"] == 1 and v1.validate_response("session.chats", reply) == [], reply
    return {i["provider_session"]: i for i in reply["items"]}


def test_a_recent_write_marks_a_chat_live_and_an_old_one_not_and_only_a_chat_of_ours_is_never_elsewhere():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = (root / "proj").resolve()
        proj.mkdir(mode=0o700)
        host, _log, _home, c_rec = _host(directory, root)
        try:
            conn = Conn(host.socket)
            acc = account(conn, "ClaudeWork", "claude")
            now = time.time()
            _chat(root, acc, "cl-fresh", proj, now - 5)
            _chat(root, acc, "cl-old", proj, now - 3600)
            items = _items(conn, proj, acc)
            assert (items["cl-fresh"]["running_elsewhere"], items["cl-fresh"]["live_reason"]) == (True, "recent_write"), items["cl-fresh"]
            assert (items["cl-old"]["running_elsewhere"], items["cl-old"]["live_reason"]) == (False, None), items["cl-old"]

            # a tab of ours that holds the old chat is Lico's own: a write to its transcript is not "elsewhere"
            made = conn.call(op="session.create", kind="claude", account=acc, cwd=str(proj), mode="auto", resume_id="cl-old")
            assert made["ok"] == 1, made
            wait_for(lambda: any("cl-old" in av for av in argv_of(c_rec)))
            path = root / "state" / "accounts" / acc / "claude" / "projects" / "p" / "cl-old.jsonl"
            os.utime(path, (time.time(), time.time()))
            again = _items(conn, proj, acc)["cl-old"]
            assert again["open_session"] == made["session"] and again["running_elsewhere"] is False, again
        finally:
            host.stop()


def test_a_provider_process_marks_the_chats_it_can_be_writing_by_id_or_by_folder_and_start_time():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = (root / "proj").resolve()
        other = (root / "other").resolve()
        for d in (proj, other):
            d.mkdir(mode=0o700)
        host, _log, _home, _c = _host(directory, root, env={"LICO_LIVE_WRITE_SECS": "0"})  # the recent-write signal is off here: the process signal alone
        procs = []
        try:
            conn = Conn(host.socket)
            acc = account(conn, "ClaudeWork", "claude")
            base = time.time()
            _chat(root, acc, "cl-before", proj, base - 3600)   # older than any process: a closed chat
            _chat(root, acc, "cl-named", proj, base - 3600)    # a process is told to resume it
            assert {i["running_elsewhere"] for i in _items(conn, proj, acc).values()} == {False}, "no process yet: nothing is live"

            fake = _fake_claude(root)
            procs.append(subprocess.Popen([fake], cwd=proj))                                    # a terminal in the folder
            procs.append(subprocess.Popen([fake, "--resume", "cl-named"], cwd=other))           # a terminal that names a chat
            _chat(root, acc, "cl-after", proj, int(time.time()))  # written in the second the terminal started: after it, as a chat of that terminal is
            time.sleep(1.2)  # the write signal is off (window 0) and compares whole seconds: let this write be a second old
            live = wait_for(lambda: (i := _items(conn, proj, acc)) and i["cl-after"]["running_elsewhere"] and i["cl-named"]["running_elsewhere"] and i)  # the list of processes is kept for 2 s
            assert live["cl-after"]["live_reason"] == "process_in_folder", live["cl-after"]
            assert live["cl-named"]["live_reason"] == "process_has_session", live["cl-named"]
            assert (live["cl-before"]["running_elsewhere"], live["cl-before"]["live_reason"]) == (False, None), "a closed chat of the same folder stays free"

            for p in procs:
                p.kill()
                p.wait()
            procs.clear()
            wait_for(lambda: {i["running_elsewhere"] for i in _items(conn, proj, acc).values()} == {False})
        finally:
            for p in procs:
                p.kill()
                p.wait()
            host.stop()


def test_a_live_chat_cannot_be_resumed_by_id_or_as_the_latest_but_new_and_a_closed_chat_are_fine():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = (root / "proj").resolve()
        proj.mkdir(mode=0o700)
        host, _log, _home, c_rec = _host(directory, root)
        try:
            conn = Conn(host.socket)
            acc = account(conn, "ClaudeWork", "claude")
            now = time.time()
            _chat(root, acc, "cl-live", proj, now - 2)
            _chat(root, acc, "cl-closed", proj, now - 3600)
            base = dict(op="session.create", kind="claude", account=acc, cwd=str(proj), mode="auto")
            by_id = conn.call(**base, resume_id="cl-live")
            assert by_id["ok"] == 0 and by_id["error"] == "chat_running_elsewhere", by_id
            latest = conn.call(**base, resume_latest=True)
            assert latest["ok"] == 0 and latest["error"] == "chat_running_elsewhere", "the latest is live: not a silent pick of an older chat"
            assert len(argv_of(c_rec)) == 0, "nothing was started by the refusals"
            fresh = conn.call(**base)
            assert fresh["ok"] == 1, "New stays allowed"
            got = json.loads(cli(host, "chat", "latest", "--cwd", str(proj), "--account", acc).stdout)
            assert got["items"][0]["provider_session"] == "cl-live" and got["items"][0]["running_elsewhere"] is True, got
            closed = conn.call(**base, resume_id="cl-closed")
            assert closed["ok"] == 1, "a closed chat of the same folder resumes"
            wait_for(lambda: any("cl-closed" in av for av in argv_of(c_rec)))
        finally:
            host.stop()


def test_an_empty_resume_says_how_many_chats_of_the_folder_are_under_other_accounts_and_homes_and_names_no_title():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = (root / "proj").resolve()
        proj.mkdir(mode=0o700)
        host, _log, home, c_rec = _host(directory, root)
        try:
            conn = Conn(host.socket)
            empty = account(conn, "Empty", "claude")
            busy = account(conn, "Busy", "claude")
            now = time.time()
            _chat(root, busy, "cl-b1", proj, now - 4000, "busy one")
            _chat(root, busy, "cl-b2", proj, now - 5000, "busy two")
            _seed_claude_jsonl(home / ".claude", "p", "cl-nat-1", str(proj), "native secret title", "answer", int(now - 6000))
            # the default home is not chosen: no chat of it is listed, but the hint may count it
            reply = conn.call(op="session.chats", cwd=str(proj), account=empty, others=True)
            assert reply["ok"] == 1 and reply["items"] == [] and v1.validate_response("session.chats", reply) == [], reply
            assert reply["others"]["total"] == 3, reply
            assert [(o["label"], o["chats"]) for o in reply["others"]["by_account"]] == [("Busy", 2), ("Native (~/.claude)", 1)], reply
            assert "title" not in json.dumps(reply["others"]) and "secret" not in json.dumps(reply), "counts only"
            # an account that has chats gets no hint, and it is only asked for when wanted
            assert "others" not in conn.call(op="session.chats", cwd=str(proj), account=busy, others=True)
            assert "others" not in conn.call(op="session.chats", cwd=str(proj), account=empty)

            # `--resume-latest` on the empty account says which case it was and where the others are
            made = conn.call(op="session.create", kind="claude", account=empty, cwd=str(proj), mode="auto", resume_latest=True)
            assert made["ok"] == 1 and made["resumed"] is False and made["others"]["total"] == 3, made
            assert "3 chats of this folder" in made["note"] and "other accounts" in made["note"], made["note"]
            assert v1.validate_response("session.create", made) == [], made
        finally:
            host.stop()
