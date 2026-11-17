"""M22-P01: `session.chats` with `all`: every past session of the host in ONE list, newest first, whatever its folder.

The window's Chats tab reads this list, and so do the CLI (`chat list --all`) and REST (`GET /v1/sessions/chats?all=1`): the four routes answer alike. A record under the workspace root is
an Anon chat (`global` true, no `cwd`); any other has its own `cwd`. `cwd`, `global` and `ssh_host` do not combine with `all`, and the other scopes answer as before."""

from __future__ import annotations

import json
import urllib.parse
from pathlib import Path

from lico_measure.p02 import ProtoConn as Conn, cli, start
from lico_measure.rig import private_dir
from lico_protocol import v1
from tests.account_helpers import account
from tests.test_chats_pagination_and_resume import _seed_claude_jsonl, _seed_codex_jsonl
from tests.test_claude_adapter import argv_of
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_control_parity import _http
from tests.test_new_session_screen import claude_args
from tests.test_reconnect_window import codex_args

CODEX_A = "019a1111-2222-7000-8000-000000000001"
KEYS = {"v", "ok", "scope", "cwd", "ssh_host", "total", "offset", "limit", "has_more", "items"}


def _all(conn, **extra):
    reply = conn.call(op="session.chats", all=True, **extra)
    assert v1.validate_response("session.chats", reply) == [], reply
    return reply


def test_all_lists_every_folder_and_anon_newest_first_and_the_four_routes_agree():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        state_dir = root / "state"
        proj_a, proj_b = (root / "proj-alpha").resolve(), (root / "proj-beta").resolve()
        for p in (proj_a, proj_b):
            p.mkdir(mode=0o700, parents=True)
        token_file = str(root / "http.token")
        c_args, c_rec = claude_args(root)
        x_args, _x_rec = codex_args(root)
        host, _log_dir = start(directory, extra=["--text", "--state-dir", str(state_dir), "--http-port", "auto", "--http-token-file", token_file, *c_args, *x_args], sessions=0, producing=0)
        try:
            wait_for(lambda: Path(token_file).exists() and host.http_port is not None)
            token, port = Path(token_file).read_text(encoding="utf-8").strip(), host.http_port
            conn = Conn(host.socket)
            acc_c, acc_x = account(conn, "LocalClaude", "claude"), account(conn, "LocalCodex", "codex")
            c_home, x_home = state_dir / "accounts" / acc_c / "claude", state_dir / "accounts" / acc_x / "codex"
            ws_root = (state_dir / "workspaces").resolve()
            anon = []
            for n in (1, 2):
                d = ws_root / f"sanon{n}"
                d.mkdir(mode=0o700, parents=True)
                anon.append(d)

            _seed_claude_jsonl(c_home, "proj-alpha", "cl-a1", str(proj_a), "alpha first question", "answer", 1_760_000_010)
            _seed_codex_jsonl(x_home, CODEX_A, str(proj_a), "alpha codex question", "answer", 1_760_000_030)
            _seed_claude_jsonl(c_home, "proj-beta", "cl-b1", str(proj_b), "beta first question", "answer", 1_760_000_020)
            _seed_claude_jsonl(c_home, "proj-beta", "cl-b2", str(proj_b), "beta second question", "answer", 1_760_000_040)
            _seed_claude_jsonl(c_home, "workspaces", "cl-g1", str(anon[0]), "anon newest question", "answer", 1_760_000_050)
            _seed_claude_jsonl(c_home, "workspaces", "cl-g2", str(anon[1]), "anon tied question", "answer", 1_760_000_020)
            # newest first; the tie at 1_760_000_020 is broken by the session id, so a page boundary never repeats or loses a chat
            expected = ["cl-g1", "cl-b2", CODEX_A, "cl-b1", "cl-g2", "cl-a1"]

            req = {"op": "session.chats", "all": True, "offset": 0, "limit": 100}
            assert v1.validate_request("session.chats", req) == []
            sock = _all(conn)
            assert sock["scope"] == "all" and sock["cwd"] is None and sock["total"] == 6 and sock["has_more"] is False
            assert [it["provider_session"] for it in sock["items"]] == expected
            assert sock.get("truncated", False) is False

            by_id = {it["provider_session"]: it for it in sock["items"]}
            for sid, proj in (("cl-a1", proj_a), (CODEX_A, proj_a), ("cl-b1", proj_b), ("cl-b2", proj_b)):
                assert (by_id[sid]["cwd"], by_id[sid]["global"]) == (str(proj), False), sid
            for sid in ("cl-g1", "cl-g2"):
                assert (by_id[sid]["cwd"], by_id[sid]["global"]) == (None, True), sid

            cli_reply = json.loads(cli(host, "chat", "list", "--all").stdout)
            st_get, http_get = _http(port, "GET", "/v1/sessions/chats?all=1", token=token)
            st_post, http_post = _http(port, "POST", "/v1", token=token, body={"op": "session.chats", "all": True})
            assert (st_get, st_post) == (200, 200)
            assert sock == cli_reply == http_get == http_post

            # a walk of three pages of two crosses the tie: nothing repeats, nothing is lost
            walked = []
            for idx, off in enumerate((0, 2, 4)):
                page = _all(conn, offset=off, limit=2)
                assert page["total"] == 6 and page["has_more"] is (idx < 2)
                walked.extend(it["provider_session"] for it in page["items"])
            assert walked == expected

            # the query and the account narrow it; the account of a codex chat names it alone
            assert [it["provider_session"] for it in _all(conn, query="beta")["items"]] == ["cl-b2", "cl-b1"]
            assert [it["provider_session"] for it in _all(conn, account=acc_x)["items"]] == [CODEX_A]

            # one scope at a time
            for other in ({"cwd": str(proj_a)}, {"global": True}, {"ssh_host": "gpu-box"}):
                bad = conn.call(op="session.chats", all=True, **other)
                assert bad["ok"] == 0 and bad["error"] == "bad_request", other
            qs = urllib.parse.urlencode({"all": 1, "cwd": str(proj_a)})
            assert _http(port, "GET", f"/v1/sessions/chats?{qs}", token=token)[0] == 400
            assert _http(port, "GET", "/v1/sessions/chats?all=maybe", token=token)[0] == 400

            # the other scopes answer as before: the same keys, the same scope words, only their own chats
            folder = conn.call(op="session.chats", cwd=str(proj_a))
            assert set(folder) == KEYS and folder["scope"] == "folder" and folder["total"] == 2
            glob = conn.call(op="session.chats", **{"global": True})
            assert set(glob) == KEYS and glob["scope"] == "global" and glob["total"] == 2

            # a chat that is open in a tab is the same chat in the list: marked open, never twice
            made = json.loads(cli(host, "session", "create", "--kind", "claude", "--account", acc_c, "--cwd", str(proj_a), "--resume", "cl-a1").stdout)
            assert made["ok"] == 1
            wait_for(lambda: any("--resume" in av and "cl-a1" in av for av in argv_of(c_rec)))
            after = _all(conn)
            assert after["total"] == 6
            opened = {it["provider_session"]: it for it in after["items"]}["cl-a1"]
            assert opened["open_session"] == made["session"] and opened["open_state"] == "running"
        finally:
            host.stop()
