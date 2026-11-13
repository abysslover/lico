"""M10-P02: `turns` in the chats list, ONE host rule for "the latest chat of this directory and account" (the list, `lico chat latest`
and `session.create {resume_latest: true}` all use it), and the three provider resume paths (running tab, disconnected tab, closed or
external transcript) for Claude and Codex, on mocked providers (no billable turn)."""

from __future__ import annotations

import json
import os
from pathlib import Path

from lico_measure.p02 import ProtoConn as Conn, cli, start
from lico_measure.rig import private_dir
from lico_protocol import v1
from tests.account_helpers import account
from tests.test_chats_pagination_and_resume import _records, _seed_claude_jsonl, _seed_codex_jsonl
from tests.test_claude_adapter import argv_of
from tests.test_codex_adapter import sent as codex_sent, usual_umask, wait_for
from tests.test_new_session_screen import claude_args
from tests.test_reconnect_window import codex_args
from tests.test_ssh_sessions_and_files import _make_remote_workbench_rig

CODEX_A = "019a1111-2222-7000-8000-0000000000a1"
CODEX_B = "019a1111-2222-7000-8000-0000000000b2"


def _claude_lines(path: Path, cwd: str, lines: list[dict], mtime: int) -> None:
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    path.write_text("\n".join(json.dumps({"cwd": cwd, **line}) for line in lines) + "\n", encoding="utf-8")
    os.chmod(path, 0o600)
    os.utime(path, (mtime, mtime))


def _text(role: str, text: str) -> dict:
    content = text if role == "user" else [{"type": "text", "text": text}]
    return {"type": role, "message": {"role": role, "content": content}}


def _host(directory: str, root: Path, *, ssh: bool = False):
    c_args, c_rec = claude_args(root)
    x_args, x_rec = codex_args(root)
    extra = ["--text", "--state-dir", str(root / "state"), *c_args, *x_args]
    rig = None
    if ssh:
        rig = _make_remote_workbench_rig(root)
        extra += ["--ssh", str(rig[1]), "--ssh-config", str(rig[0])]
    host, log_dir = start(directory, extra=extra, sessions=0, producing=0)
    return host, log_dir, c_rec, x_rec, rig


def test_turns_count_visible_user_and_assistant_text_and_are_null_beyond_the_bound():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = (root / "proj").resolve()
        proj.mkdir(mode=0o700)
        host, _log, _c, _x, _rig = _host(directory, root)
        try:
            conn = Conn(host.socket)
            acc_c, acc_x = account(conn, "C", "claude"), account(conn, "X", "codex")
            c_home = root / "state" / "accounts" / acc_c / "claude"
            x_home = root / "state" / "accounts" / acc_x / "codex"
            tool_use = {"type": "assistant", "message": {"role": "assistant", "content": [{"type": "tool_use", "id": "t1", "name": "Bash", "input": {}}]}}
            tool_result = {"type": "user", "message": {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "ok"}]}}
            _claude_lines(c_home / "projects" / "p" / "cl-three.jsonl", str(proj),
                          [_text("user", "first question"), _text("assistant", "first answer"), tool_use, tool_result, _text("assistant", "second answer")], 1_760_000_300)
            _claude_lines(c_home / "projects" / "p" / "cl-huge.jsonl", str(proj),
                          [_text("user", "huge chat"), _text("assistant", "x" * (9 * 1024 * 1024))], 1_760_000_200)
            _seed_codex_jsonl(x_home, CODEX_A, str(proj), "codex question", "codex answer", 1_760_000_100)
            by_id = {}
            for acc in (acc_c, acc_x):
                reply = conn.call(op="session.chats", cwd=str(proj), account=acc)
                assert reply["ok"] == 1 and v1.validate_response("session.chats", reply) == [], reply
                by_id.update({it["provider_session"]: it for it in reply["items"]})
            assert by_id["cl-three"]["turns"] == 3, by_id["cl-three"]
            assert by_id["cl-huge"]["turns"] is None, by_id["cl-huge"]
            assert by_id[CODEX_A]["turns"] == 2, by_id[CODEX_A]
        finally:
            host.stop()


def test_one_latest_rule_serves_the_list_the_command_and_the_creation_for_claude_and_codex():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = (root / "proj").resolve()
        other = (root / "other").resolve()
        empty = (root / "empty").resolve()
        for d in (proj, other, empty):
            d.mkdir(mode=0o700)
        host, log_dir, c_rec, x_rec, _rig = _host(directory, root)
        try:
            conn = Conn(host.socket)
            acc_c, acc_x = account(conn, "C", "claude"), account(conn, "X", "codex")
            c_home = root / "state" / "accounts" / acc_c / "claude"
            x_home = root / "state" / "accounts" / acc_x / "codex"
            _seed_claude_jsonl(c_home, "p", "cl-old", str(proj), "old task", "old answer", 1_760_000_100)
            _seed_claude_jsonl(c_home, "p", "cl-new", str(proj), "new task", "new answer", 1_760_000_500)
            _seed_claude_jsonl(c_home, "o", "cl-other", str(other), "other dir task", "other answer", 1_760_000_900)
            _seed_codex_jsonl(x_home, CODEX_A, str(proj), "codex old", "codex old answer", 1_760_000_150)
            _seed_codex_jsonl(x_home, CODEX_B, str(proj), "codex new", "codex new answer", 1_760_000_600)

            # a tab that has said nothing yet has no saved transcript: it is in the list but is never "the latest chat"
            blank = conn.call(op="session.create", kind="claude", account=acc_c, cwd=str(proj), mode="auto")
            assert blank["ok"] == 1, blank
            plain = conn.call(op="session.chats", cwd=str(proj), account=acc_c, limit=1)
            assert plain["items"][0]["open_session"] == blank["session"], plain

            for acc, want in ((acc_c, "cl-new"), (acc_x, CODEX_B)):
                listed = conn.call(op="session.chats", cwd=str(proj), account=acc, limit=1, recorded=True)
                latest = json.loads(cli(host, "chat", "latest", "--cwd", str(proj), "--account", acc).stdout)
                assert listed["items"][0]["provider_session"] == want
                assert latest == listed and latest["limit"] == 1, (latest, listed)

            made = conn.call(op="session.create", kind="claude", account=acc_c, cwd=str(proj), mode="auto", resume_latest=True)
            assert made["ok"] == 1 and made["resumed"] is True and made.get("already_open") is not True, made
            assert made["resumed_chat"]["provider_session"] == "cl-new" and made["resumed_chat"]["turns"] == 2, made
            assert v1.validate_response("session.create", made) == []
            wait_for(lambda: any("--resume" in av and "cl-new" in av for av in argv_of(c_rec)))
            assert any(">> new task" in ln for ln in _records(log_dir, made["session"]))

            made_x = conn.call(op="session.create", kind="codex", account=acc_x, cwd=str(proj), mode="auto", resume_latest=True)
            assert made_x["ok"] == 1 and made_x["resumed"] is True and made_x["resumed_chat"]["provider_session"] == CODEX_B, made_x
            wait_for(lambda: any(m.get("method") == "thread/resume" and m.get("params", {}).get("threadId") == CODEX_B for m in codex_sent(x_rec)))

            # no chat in this directory: a new session, and the reply says so
            before = len(argv_of(c_rec))
            fresh = conn.call(op="session.create", kind="claude", account=acc_c, cwd=str(empty), mode="auto", resume_latest=True)
            assert fresh["ok"] == 1 and fresh["resumed"] is False and "no earlier chat" in fresh["note"] and "resumed_chat" not in fresh, fresh
            wait_for(lambda: len(argv_of(c_rec)) == before + 1)
            assert not any("--resume" in av for av in argv_of(c_rec)[before:])
        finally:
            host.stop()


def test_the_three_resume_paths_running_tab_disconnected_tab_and_a_closed_transcript():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = (root / "proj").resolve()
        proj.mkdir(mode=0o700)
        host, _log, c_rec, _x, _rig = _host(directory, root)
        try:
            conn = Conn(host.socket)
            acc_c = account(conn, "C", "claude")
            _seed_claude_jsonl(root / "state" / "accounts" / acc_c / "claude", "p", "cl-only", str(proj), "only task", "only answer", 1_760_000_500)
            ask = dict(op="session.create", kind="claude", account=acc_c, cwd=str(proj), mode="auto", resume_latest=True)

            first = conn.call(**ask)  # closed or external transcript -> a new tab on that chat
            assert first["ok"] == 1 and first["resumed"] is True and first.get("already_open") is not True, first
            wait_for(lambda: len(argv_of(c_rec)) == 1)

            again = conn.call(**ask)  # running tab -> that tab, no second process
            assert again["ok"] == 1 and again["resumed"] is True and again["already_open"] is True and again["session"] == first["session"], again
            assert len(argv_of(c_rec)) == 1

            closed = conn.call(op="session.close", session=first["session"], generation=first["generation"])
            assert closed["ok"] == 1, closed
            wait_for(lambda: next(s for s in conn.call(op="status")["sessions"] if s["session"] == first["session"])["state"] == "disconnected")
            back = conn.call(**ask)  # disconnected tab -> resumed in place, a new generation
            assert back["ok"] == 1 and back["resumed"] is True and back["session"] == first["session"] and back["generation"] > first["generation"], back
            wait_for(lambda: len(argv_of(c_rec)) == 2)
        finally:
            host.stop()


def test_resume_latest_is_refused_with_a_chat_id_a_worktree_a_synthetic_session_no_folder_or_a_non_boolean():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = (root / "proj").resolve()
        proj.mkdir(mode=0o700)
        host, _log, _c, _x, _rig = _host(directory, root)
        try:
            conn = Conn(host.socket)
            acc_c = account(conn, "C", "claude")
            base = dict(op="session.create", kind="claude", account=acc_c, cwd=str(proj), mode="auto")
            both = conn.call(**base, resume_latest=True, resume_id="cl-x")
            assert both["ok"] == 0 and both["error"] == "bad_request", both
            wt = conn.call(**base, resume_latest=True, worktree_branch="feature-x")
            assert wt["ok"] == 0 and wt["error"] in ("option_unavailable", "bad_request"), wt
            synth = conn.call(op="session.create", kind="synthetic", resume_latest=True)
            assert synth["ok"] == 0 and synth["error"] == "option_unavailable", synth
            nofolder = conn.call(op="session.create", kind="claude", account=acc_c, mode="auto", resume_latest=True)
            assert nofolder["ok"] == 0 and nofolder["error"] == "bad_request", nofolder  # the latest chat is of a folder
            junk = conn.call(**base, resume_latest="yes")
            assert junk["ok"] == 0 and junk["error"] == "bad_request", junk
        finally:
            host.stop()


def test_resume_latest_on_a_remote_account_uses_the_remote_chats():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        host, _log, _c, _x, rig = _host(directory, root, ssh=True)
        try:
            _cfg, _ssh, remote_root = rig
            uhome = (remote_root / "gpu-box" / "user_home").resolve()
            exp_dir = (uhome / "projects" / "experiment").resolve()
            conn = Conn(host.socket)
            made = conn.call(op="account.create", provider="claude", label="RemoteClaude", ssh_host="gpu-box")
            assert made["ok"] == 1, made
            acc_rc = made["account"]
            rem_home = uhome / ".local" / "state" / "lico" / "accounts" / acc_rc / "claude"
            _seed_claude_jsonl(rem_home, "exp", "cl-ssh-old", str(exp_dir), "older remote", "older answer", 1_760_000_100)
            _seed_claude_jsonl(rem_home, "exp", "cl-ssh-new", str(exp_dir), "newer remote", "newer answer", 1_760_000_300)
            listed = conn.call(op="session.chats", ssh_host="gpu-box", cwd=str(exp_dir), account=acc_rc, limit=1)
            assert listed["items"][0]["provider_session"] == "cl-ssh-new" and listed["items"][0]["turns"] is None, listed
            made_s = conn.call(op="session.create", kind="claude", account=acc_rc, cwd=str(exp_dir), mode="auto", resume_latest=True)
            assert made_s["ok"] == 1 and made_s["resumed"] is True and made_s["resumed_chat"]["provider_session"] == "cl-ssh-new", made_s
        finally:
            host.stop()


def test_the_tool_create_with_resume_continues_the_latest_chat_of_the_callers_folder_and_says_what_it_did():
    from tests.test_claude_adapter import records, status_of
    from tests.test_tool_connection import World
    from tests.test_tool_create import call, program_of, rows, wants
    from tests.test_tool_send import decide, host_of

    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        state = root  # without --state-dir the host keeps its state (accounts, tool configs) beside its socket
        project, empty = (root / "work").resolve(), (root / "empty").resolve()
        project.mkdir(mode=0o700)
        empty.mkdir(mode=0o700)
        host, log, received = host_of(directory)
        try:
            w = World(directory, host, log, received)
            acc = account(w.conn, "Claude-Main", provider="claude")
            _seed_claude_jsonl(state / "accounts" / acc / "claude", "w", "cl-mcp-latest", str(project), "mcp old task", "mcp old answer", 1_760_000_500)
            c_made = w.conn.call(op="session.create", kind="claude", account=acc, name="Claude-L", cwd=str(project))
            e_made = w.conn.call(op="session.create", kind="claude", account=acc, name="Claude-E", cwd=str(empty))
            assert c_made["ok"] == 1 and e_made["ok"] == 1
            caller, gen = c_made["session"], c_made["generation"]

            failed, _text = call(w.config(caller), "create", resume="maybe")
            assert failed, "a resume that is not true or false is refused"

            before = {r["session"] for r in rows(w)}
            number = wants(w, caller, resume=True)
            card = status_of(w.conn, caller)["approvals"][0]
            assert card["tool"] == "create" and "continues the latest chat here if one exists" in card["prompt"], card
            assert decide(w, caller, gen, number, "allow")["outcome"] == "done"
            [new] = [r for r in rows(w) if r["session"] not in before]
            argv, _folder = wait_for(lambda: program_of(w, new["session"]))
            assert "--resume" in argv and "cl-mcp-latest" in argv, argv
            told = wait_for(lambda: [r for r in records(w.log, caller) if r.startswith(">> approved and done: created")])
            assert "it continues the latest chat there ('mcp old task', 2 turns)" in told[0], told

            # that chat is now open in the new tab: a second ask makes no new session and says so
            number = wants(w, caller, resume=True)
            assert decide(w, caller, gen, number, "allow")["outcome"] == "done"
            again = wait_for(lambda: [r for r in records(w.log, caller) if r.startswith(">> approved and done: no new session")])
            assert new["session"] in again[0] and "was already open" in again[0], again

            # a folder without a chat: a new session, and the caller is told
            e_gen = e_made["generation"]
            number = wants(w, e_made["session"], resume=True)
            assert decide(w, e_made["session"], e_gen, number, "allow")["outcome"] == "done"
            none = wait_for(lambda: [r for r in records(w.log, e_made["session"]) if r.startswith(">> approved and done: created")])
            assert "there was no earlier chat there, so it is a new one" in none[0], none
        finally:
            host.stop()
