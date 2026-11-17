"""M22-P02: the chat index. A list of chats is read from an index the host keeps per provider home, so that no call waits for a scan.

What the system test pins down (the unit tests of `chats/index.rs` pin the rest): only `.jsonl` files are records (a home full of other files, as Codex keeps, is listed whole and not
`truncated`); a record written after a listing shows in the next one and a removed one goes (the answer stays exact); a record that has not changed is not opened again (one made unreadable
after the first listing stays listed, which a scan per call could not do); a home of more records than the bound shows the newest and says so, in every call, whatever was added since.
Synthetic homes only: no real home of the owner is read."""

from __future__ import annotations

import os
import time
from pathlib import Path

from lico_measure.p02 import ProtoConn as Conn, start
from lico_measure.rig import private_dir
from lico_protocol import v1
from tests.account_helpers import account
from tests.test_chats_pagination_and_resume import _seed_claude_jsonl, _seed_codex_jsonl
from tests.test_codex_adapter import usual_umask
from tests.test_new_session_screen import claude_args
from tests.test_reconnect_window import codex_args

CODEX_A = "019a1111-2222-7000-8000-0000000000a1"
CODEX_B = "019a1111-2222-7000-8000-0000000000b2"
BOUND = 2500  # `MAX_SCAN_FILES`: the records one home keeps in the index


def _host(directory: str, root: Path):
    state_dir = root / "state"
    c_args, _c = claude_args(root)
    x_args, _x = codex_args(root)
    host, _log = start(directory, extra=["--text", "--state-dir", str(state_dir), *c_args, *x_args], sessions=0, producing=0)
    conn = Conn(host.socket)
    acc_c, acc_x = account(conn, "IndexClaude", "claude"), account(conn, "IndexCodex", "codex")
    return host, conn, state_dir / "accounts" / acc_c / "claude", state_dir / "accounts" / acc_x / "codex", state_dir


def _all(conn, **extra):
    reply = conn.call(op="session.chats", all=True, offset=0, limit=100, **extra)
    assert v1.validate_response("session.chats", reply) == [], reply
    return reply


def _ids(reply) -> list[str]:
    return [item["provider_session"] for item in reply["items"]]


def test_a_home_full_of_other_files_is_listed_whole_and_is_not_truncated():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj_a, proj_b = (root / "proj-alpha").resolve(), (root / "proj-beta").resolve()
        for p in (proj_a, proj_b):
            p.mkdir(mode=0o700, parents=True)
        host, conn, c_home, x_home, state_dir = _host(directory, root)
        try:
            ws_root = (state_dir / "workspaces").resolve()
            (ws_root / "sanon1").mkdir(mode=0o700, parents=True)
            _seed_claude_jsonl(c_home, "proj-alpha", "cl-a1", str(proj_a), "alpha question", "answer", 1_760_000_010)
            _seed_claude_jsonl(c_home, "proj-beta", "cl-b1", str(proj_b), "beta question", "answer", 1_760_000_020)
            _seed_claude_jsonl(c_home, "workspaces", "cl-h1", str(ws_root / "sanon1"), "home question", "answer", 1_760_000_030)
            _seed_codex_jsonl(x_home, CODEX_A, str(proj_a), "alpha codex question", "answer", 1_760_000_040)
            _seed_codex_jsonl(x_home, CODEX_B, str(proj_b), "beta codex question", "answer", 1_760_000_050)
            # what Codex keeps beside its transcripts, and what a project folder of Claude may hold: many files, none of them a chat
            for folder in (c_home / "projects" / "proj-alpha", x_home / "sessions" / "2026" / "11" / "12"):
                for n in range(BOUND + 500):
                    (folder / f"noise-{n:05d}.txt").touch()
            expected = [CODEX_B, CODEX_A, "cl-h1", "cl-b1", "cl-a1"]

            everything = _all(conn)
            assert _ids(everything) == expected and everything["total"] == 5, everything
            assert everything["truncated"] is False, "files that are not records do not count against the bound"
            assert _ids(conn.call(op="session.chats", cwd=str(proj_a))) == [CODEX_A, "cl-a1"]
            assert _ids(conn.call(op="session.chats", cwd=str(proj_b))) == [CODEX_B, "cl-b1"]
            assert _ids(conn.call(op="session.chats", **{"global": True})) == ["cl-h1"]
        finally:
            host.stop()


def test_the_answer_stays_exact_a_new_record_shows_a_removed_one_goes_and_an_unchanged_one_is_not_opened_again():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = (root / "proj-alpha").resolve()
        proj.mkdir(mode=0o700, parents=True)
        host, conn, c_home, _x_home, _state = _host(directory, root)
        try:
            kept = _seed_claude_jsonl(c_home, "proj-alpha", "cl-1", str(proj), "first question", "answer", 1_760_000_010)
            _seed_claude_jsonl(c_home, "proj-alpha", "cl-2", str(proj), "second question", "answer", 1_760_000_020)
            doomed = _seed_claude_jsonl(c_home, "proj-alpha", "cl-3", str(proj), "third question", "answer", 1_760_000_030)
            assert _ids(_all(conn)) == ["cl-3", "cl-2", "cl-1"]

            # a record written a moment after the listing is in the next one, first (it is the newest)
            _seed_claude_jsonl(c_home, "proj-alpha", "cl-4", str(proj), "fourth question", "answer", 1_760_000_040)
            assert _ids(_all(conn)) == ["cl-4", "cl-3", "cl-2", "cl-1"]

            # a record that grew is read again: its title is the one it has now
            _seed_claude_jsonl(c_home, "proj-alpha", "cl-2", str(proj), "a much longer second question than before", "answer", 1_760_000_050)
            reply = _all(conn)
            assert _ids(reply) == ["cl-2", "cl-4", "cl-3", "cl-1"], reply
            assert reply["items"][0]["title"].startswith("a much longer second"), reply["items"][0]

            # a removed record goes at once
            doomed.unlink()
            assert _ids(_all(conn)) == ["cl-2", "cl-4", "cl-1"]

            # a record that did not change is not opened again: made unreadable, it stays listed with the title it had (a scan per call would have lost it)
            kept.chmod(0)
            try:
                reply = _all(conn)
                assert _ids(reply) == ["cl-2", "cl-4", "cl-1"], reply
                assert next(i for i in reply["items"] if i["provider_session"] == "cl-1")["title"].startswith("first question")
            finally:
                kept.chmod(0o600)
        finally:
            host.stop()


def test_a_home_of_more_records_than_the_bound_shows_the_newest_and_says_so_whatever_was_added_since():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = (root / "proj-scale").resolve()
        proj.mkdir(mode=0o700, parents=True)
        host, conn, c_home, _x_home, _state = _host(directory, root)
        try:
            base = 1_760_000_000
            for n in range(BOUND + 100):
                folder = c_home / "projects" / f"p{n % 40:02d}"
                folder.mkdir(mode=0o700, parents=True, exist_ok=True)
                path = folder / f"cl-{n:05d}.jsonl"
                path.write_text(f'{{"type":"user","cwd":"{proj}","message":{{"role":"user","content":"question {n}"}}}}\n', encoding="utf-8")
                os.utime(path, (base + n, base + n))
            first = _all(conn)
            assert first["total"] == BOUND and first["truncated"] is True, first
            assert _ids(first)[0] == f"cl-{BOUND + 99:05d}", "the newest first"
            listed = {i["provider_session"] for p in range(0, BOUND, 100) for i in conn.call(op="session.chats", all=True, offset=p, limit=100)["items"]}
            assert len(listed) == BOUND and f"cl-{99:05d}" not in listed and f"cl-{100:05d}" in listed, "the oldest hundred are the ones left out"

            # one more, the newest: it is in, and the oldest kept one is out; the bound holds and so does the notice
            _seed_claude_jsonl(c_home, "p00", "cl-newest", str(proj), "the newest question", "answer", base + BOUND + 1000)
            again = _all(conn)
            assert again["total"] == BOUND and again["truncated"] is True and _ids(again)[0] == "cl-newest", again
            last = conn.call(op="session.chats", all=True, offset=BOUND - 100, limit=100)
            assert f"cl-{100:05d}" not in _ids(last) and f"cl-{101:05d}" in _ids(last), "the record that was the oldest kept one went out"
        finally:
            host.stop()


def test_a_repeated_page_of_a_large_home_is_ready_at_once():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = (root / "proj-fast").resolve()
        proj.mkdir(mode=0o700, parents=True)
        host, conn, c_home, _x_home, _state = _host(directory, root)
        try:
            base = 1_760_000_000
            for n in range(1500):
                folder = c_home / "projects" / f"p{n % 30:02d}"
                folder.mkdir(mode=0o700, parents=True, exist_ok=True)
                path = folder / f"cl-{n:05d}.jsonl"
                path.write_text(f'{{"type":"user","cwd":"{proj}","message":{{"role":"user","content":"question {n}"}}}}\n', encoding="utf-8")
                os.utime(path, (base + n, base + n))
            _all(conn)  # the first call builds what it needs
            took = []
            for _ in range(5):
                t = time.perf_counter()
                reply = _all(conn)
                took.append(time.perf_counter() - t)
                assert reply["total"] == 1500
            # a generous bound (the suite runs under load); the figures of the budget are measured by the probe in the plan, not asserted here
            assert min(took) < 0.25, took
        finally:
            host.stop()
