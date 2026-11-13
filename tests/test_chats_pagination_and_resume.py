"""M07-P03: full-pagination `session.chats` discovery (`offset`/`limit`/`total`/`has_more` with
zero top-N truncation across `claude`, `codex`, and registered `lico` sessions in `cwd`, `Global`,
and `[ssh: <host>]` scopes), `resume_id` transcript seeding across IPC/CLI/HTTP, and `lico gui`
`[ Chats ]` tab multi-page navigation, query filtering, scope switching, and 1-click resume (`0.2.3`)."""

from __future__ import annotations

import json
import os
import subprocess
import urllib.parse
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn, cli, read_log, start
from lico_measure.rig import private_dir
from lico_protocol import v1
from tests.account_helpers import account
from tests.test_claude_adapter import argv_of, status_of
from tests.test_codex_adapter import sent as codex_sent, usual_umask, wait_for
from tests.test_control_parity import _http
from tests.test_new_session_screen import claude_args
from tests.test_reconnect_window import codex_args
from tests.test_ssh_sessions_and_files import _make_remote_workbench_rig


def _centre(rect: list[float]) -> tuple[float, float]:
    return (rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0


def _tab_click_xy(tab: dict) -> tuple[float, float]:
    return (tab["rect"][0] + tab["close_rect"][0]) / 2.0, (tab["rect"][1] + tab["rect"][3]) / 2.0


def _records(log_dir: Path, sid: str) -> list[str]:
    return [p.decode("utf-8", errors="replace") for _, p in read_log(log_dir, sid)]


def _seed_claude_jsonl(
    provider_home: Path,
    proj_slug: str,
    sid: str,
    cwd: str,
    prompt: str,
    reply: str,
    mtime: int,
) -> Path:
    proj_dir = provider_home / "projects" / proj_slug
    proj_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
    path = proj_dir / f"{sid}.jsonl"
    lines = [
        json.dumps({"type": "user", "cwd": cwd, "message": {"role": "user", "content": prompt}}),
        json.dumps(
            {
                "type": "assistant",
                "cwd": cwd,
                "message": {"role": "assistant", "content": [{"type": "text", "text": reply}]},
            }
        ),
    ]
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    os.chmod(path, 0o600)
    os.utime(path, (mtime, mtime))
    return path


def _seed_codex_jsonl(
    provider_home: Path,
    sid: str,
    cwd: str,
    prompt: str,
    reply: str,
    mtime: int,
) -> Path:
    day_dir = provider_home / "sessions" / "2026" / "11" / "12"
    day_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
    path = day_dir / f"rollout-2026-11-12T10-00-00-{sid}.jsonl"
    lines = [
        json.dumps({"type": "session_meta", "payload": {"id": sid, "cwd": cwd}}),
        json.dumps(
            {
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "<environment_context>\n<cwd>/tmp</cwd>"}],
                },
            }
        ),
        json.dumps(
            {
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": prompt}],
                },
            }
        ),
        json.dumps(
            {
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "assistant",
                    "content": [{"type": "output_text", "text": reply}],
                },
            }
        ),
    ]
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    os.chmod(path, 0o600)
    os.utime(path, (mtime, mtime))
    return path


def test_chats_pagination_parity_local_global_and_ssh():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        state_dir = root / "state"
        proj_a = (root / "proj-alpha").resolve()
        proj_b = (root / "proj-other").resolve()
        proj_a.mkdir(mode=0o700, parents=True, exist_ok=True)
        proj_b.mkdir(mode=0o700, parents=True, exist_ok=True)

        token_file = str(root / "http.token")
        ssh_cfg, mock_ssh, remote_root = _make_remote_workbench_rig(root)
        uhome = (remote_root / "gpu-box" / "user_home").resolve()
        exp_dir = (uhome / "projects" / "experiment").resolve()
        c_args, c_rec = claude_args(root)
        x_args, x_rec = codex_args(root)

        host, log_dir = start(
            directory,
            extra=[
                "--text",
                "--state-dir",
                str(state_dir),
                "--http-port",
                "auto",
                "--http-token-file",
                token_file,
                "--ssh",
                str(mock_ssh),
                "--ssh-config",
                str(ssh_cfg),
                *c_args,
                *x_args,
            ],
            sessions=0,
            producing=0,
        )
        try:
            wait_for(lambda: Path(token_file).exists() and host.http_port is not None)
            token = Path(token_file).read_text(encoding="utf-8").strip()
            port = host.http_port
            conn = Conn(host.socket)

            acc_c = account(conn, "LocalClaude", "claude")
            acc_x = account(conn, "LocalCodex", "codex")
            rc_reply = conn.call(op="account.create", provider="claude", label="RemoteClaude", ssh_host="gpu-box")
            rx_reply = conn.call(op="account.create", provider="codex", label="RemoteCodex", ssh_host="gpu-box")
            assert rc_reply["ok"] == 1 and rx_reply["ok"] == 1
            acc_rc, acc_rx = rc_reply["account"], rx_reply["account"]

            c_home = state_dir / "accounts" / acc_c / "claude"
            x_home = state_dir / "accounts" / acc_x / "codex"
            expected_desc: list[str] = []

            # Seed 20 Claude + 15 Codex = 35 chats in proj_a (plus noise in proj_b)
            for idx in range(1, 21):
                sid = f"cl-past-{idx:02d}"
                _seed_claude_jsonl(
                    c_home,
                    "proj-alpha",
                    sid,
                    str(proj_a),
                    f"Claude task {idx:02d} in alpha",
                    f"Claude answer {idx:02d}",
                    1_760_000_000 + idx,
                )
                expected_desc.append(sid)
            for idx in range(1, 16):
                sid = f"019a1111-2222-7000-8000-0000000000{idx:02d}"
                _seed_codex_jsonl(
                    x_home,
                    sid,
                    str(proj_a),
                    f"Codex needle {idx:02d} in alpha",
                    f"Codex answer {idx:02d}",
                    1_760_000_020 + idx,
                )
                expected_desc.append(sid)
            expected_desc.reverse()  # newest mtime first

            _seed_claude_jsonl(c_home, "proj-other", "cl-other-99", str(proj_b), "Unrelated folder", "Skip", 1_760_000_999)

            # Seed 3 Global chats inside workspace_root
            ws_root = (state_dir / "workspaces").resolve()
            for idx in range(1, 4):
                g_dir = ws_root / f"sglob{idx}"
                g_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
                _seed_claude_jsonl(
                    c_home,
                    "workspaces",
                    f"cl-glob-{idx:02d}",
                    str(g_dir),
                    f"Quick global question {idx}",
                    f"Global answer {idx}",
                    1_760_000_100 + idx,
                )

            # Seed 2 SSH remote chats on gpu-box
            rem_c_home = uhome / ".local" / "state" / "lico" / "accounts" / acc_rc / "claude"
            rem_x_home = uhome / ".local" / "state" / "lico" / "accounts" / acc_rx / "codex"
            _seed_claude_jsonl(rem_c_home, "exp", "cl-ssh-01", str(exp_dir), "Remote GPU debug", "Fixed CUDA kernel", 1_760_000_201)
            _seed_codex_jsonl(
                rem_x_home,
                "019a9999-8888-7000-8000-000000000001",
                str(exp_dir),
                "Remote benchmark run",
                "Throughput 920 tok/s",
                1_760_000_202,
            )

            # 1. Parity across Unix socket, CLI, HTTP GET /v1/sessions/chats, and HTTP POST /v1
            req0 = {"op": "session.chats", "cwd": str(proj_a), "offset": 0, "limit": 12}
            assert v1.validate_request("session.chats", req0) == []
            sock_p0 = conn.call(**req0)
            assert v1.validate_response("session.chats", sock_p0) == []
            assert sock_p0["total"] == 35 and sock_p0["has_more"] is True and len(sock_p0["items"]) == 12

            cli_p0 = json.loads(cli(host, "chat", "list", "--cwd", str(proj_a), "--offset", "0", "--limit", "12").stdout)
            qs = urllib.parse.urlencode({"cwd": str(proj_a), "offset": 0, "limit": 12})
            st_get, http_get_p0 = _http(port, "GET", f"/v1/sessions/chats?{qs}", token=token)
            st_post, http_post_p0 = _http(port, "POST", "/v1", token=token, body=req0)
            assert (st_get, st_post) == (200, 200)
            assert sock_p0 == cli_p0 == http_get_p0 == http_post_p0

            # 2. Full pagination walk across all 35 sessions (zero top-N truncation!)
            walked_ids: list[str] = []
            for idx, off in enumerate((0, 12, 24)):
                page_res = conn.call(op="session.chats", cwd=str(proj_a), offset=off, limit=12)
                assert v1.validate_response("session.chats", page_res) == []
                assert page_res["total"] == 35
                assert page_res["has_more"] is (idx < 2)
                walked_ids.extend(it["provider_session"] for it in page_res["items"])
            assert walked_ids == expected_desc

            # 3. Query filter, Global scope, SSH remote scope, and mutual-exclusion guard
            q_res = conn.call(op="session.chats", cwd=str(proj_a), query="needle 07")
            assert q_res["total"] == 1 and q_res["items"][0]["provider_session"].endswith("07")

            g_sock = conn.call(op="session.chats", **{"global": True})
            g_cli = json.loads(cli(host, "chat", "list", "--global").stdout)
            st_g, g_http = _http(port, "GET", "/v1/sessions/chats?global=1", token=token)
            assert st_g == 200 and g_sock == g_cli == g_http
            assert g_sock["scope"] == "global" and g_sock["total"] == 3
            assert all(it["global"] is True and it["cwd"] is None for it in g_sock["items"])

            r_qs = urllib.parse.urlencode({"ssh_host": "gpu-box", "cwd": str(exp_dir)})
            r_sock = conn.call(op="session.chats", ssh_host="gpu-box", cwd=str(exp_dir))
            r_cli = json.loads(cli(host, "chat", "list", "--ssh-host", "gpu-box", "--cwd", str(exp_dir)).stdout)
            st_r, r_http = _http(port, "GET", f"/v1/sessions/chats?{r_qs}", token=token)
            assert st_r == 200 and r_sock == r_cli == r_http
            assert r_sock["total"] == 2 and {it["provider"] for it in r_sock["items"]} == {"claude", "codex"}

            bad = conn.call(op="session.chats", cwd=str(proj_a), **{"global": True})
            assert bad["ok"] == 0 and bad["error"] == "bad_request"

            # 4. CLI `session create --resume` seeds transcript and passes `--resume` / `thread/resume`
            res_c = json.loads(
                cli(host, "session", "create", "--kind", "claude", "--account", acc_c, "--cwd", str(proj_a), "--resume", "cl-past-20").stdout
            )
            assert res_c["ok"] == 1
            sid_c = res_c["session"]
            wait_for(lambda: any("--resume" in av and "cl-past-20" in av for av in argv_of(c_rec)))
            c_lines = _records(log_dir, sid_c)
            assert any("resumed cl-past-20" in ln for ln in c_lines)
            assert any(">> Claude task 20 in alpha" in ln for ln in c_lines)
            assert any("Claude answer 20" in ln for ln in c_lines)

            target_x = "019a1111-2222-7000-8000-000000000015"
            res_x = json.loads(
                cli(host, "session", "create", "--kind", "codex", "--account", acc_x, "--cwd", str(proj_a), "--resume", target_x).stdout
            )
            assert res_x["ok"] == 1
            sid_x = res_x["session"]
            wait_for(lambda: any(m.get("method") == "thread/resume" and m.get("params", {}).get("threadId") == target_x for m in codex_sent(x_rec)))
            x_lines = _records(log_dir, sid_x)
            assert any(f"resumed {target_x}" in ln for ln in x_lines)
            assert any(">> Codex needle 15 in alpha" in ln for ln in x_lines)
            assert any("Codex answer 15" in ln for ln in x_lines)
        finally:
            host.stop()


def test_gui_chats_tab_multi_page_navigation_and_one_click_resume():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj_a = (root / "proj-workbench").resolve()
        proj_a.mkdir(mode=0o700, parents=True, exist_ok=True)
        (proj_a / "README.md").write_text("# Workbench\n", encoding="utf-8")

        log_dir = root / "logs"
        c_args, c_rec = claude_args(root)
        x_args, x_rec = codex_args(root)
        made: dict[str, str] = {}
        top_claude_ext = "cl-gui-ext-top"
        top_codex_ext = "019a2222-3333-7000-8000-000000000034"

        def setup(w: ProductWindow) -> None:
            conn = Conn(w.socket)
            acc_c = account(conn, "ClaudeWork", "claude")
            acc_x = account(conn, "CodexWork", "codex")
            made["acc_c"] = acc_c
            made["acc_x"] = acc_x

            s_dir = Path(w.directory) / "state" / "lico"
            c_home = s_dir / "accounts" / acc_c / "claude"
            x_home = s_dir / "accounts" / acc_x / "codex"

            # Seed 33 older chats + 2 top external sessions = 35 external `.jsonl` sessions
            for idx in range(1, 20):
                _seed_claude_jsonl(
                    c_home,
                    "proj-workbench",
                    f"cl-hist-{idx:02d}",
                    str(proj_a),
                    f"Historical Claude item {idx:02d}",
                    f"Historical Claude reply {idx:02d}",
                    1_700_000_000 + idx,
                )
            for idx in range(1, 15):
                _seed_codex_jsonl(
                    x_home,
                    f"019a2222-3333-7000-8000-0000000000{idx:02d}",
                    str(proj_a),
                    f"CodexCard{idx:02d} refactor",
                    f"CodexCard{idx:02d} done",
                    1_700_000_100 + idx,
                )

            _seed_codex_jsonl(
                x_home,
                top_codex_ext,
                str(proj_a),
                "Optimize sidebar pagination",
                "Pagination optimized cleanly.",
                1_750_000_001,
            )
            _seed_claude_jsonl(
                c_home,
                "proj-workbench",
                top_claude_ext,
                str(proj_a),
                "Fix telemetry layout",
                "Telemetry layout updated.",
                1_750_000_002,
            )

            # Seed 2 Global chats under workspaces
            ws_dir = (s_dir / "workspaces" / "s_glob_hist").resolve()
            ws_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
            _seed_claude_jsonl(c_home, "workspaces", "cl-glob-hist-1", str(ws_dir), "Global scratch math", "42", 1_750_000_010)
            _seed_claude_jsonl(c_home, "workspaces", "cl-glob-hist-2", str(ws_dir), "Global shell tip", "Use ripgrep", 1_750_000_011)

            # Create 1 disconnected session + 1 open running session in proj_a (total = 37 in proj_a)
            sc = conn.call(op="session.create", kind="claude", account=acc_c, mode="auto", cwd=str(proj_a), name="ClosedChat")
            so = conn.call(op="session.create", kind="claude", account=acc_c, mode="auto", cwd=str(proj_a), name="OpenChat")
            assert sc["ok"] == 1 and so["ok"] == 1
            assert conn.call(op="input", session=sc["session"], generation=sc["generation"], request_id="c0", text="initial turn")["ok"] == 1
            wait_for(lambda: "[turn completed]" in _records(log_dir, sc["session"]))
            assert conn.call(op="session.close", session=sc["session"], generation=sc["generation"])["ok"] == 1
            wait_for(lambda: status_of(conn, sc["session"])["state"] == "disconnected")
            made["s_closed"] = sc["session"]
            made["s_open"] = so["session"]

        with ProductWindow(
            host_binary=LICO,
            host_args=[*c_args, *x_args, "--log-dir", str(log_dir)],
            env_extra={"LANG": "C.UTF-8"},
            before_window=setup,
        ) as w:
            conn = Conn(w.socket)
            s_closed, s_open = made["s_closed"], made["s_open"]
            w.proc.stdin.write("load\n")
            w.proc.stdin.flush()

            u0 = wait_for(lambda: (u := w.ui()) and len(u["tabs"]["items"]) == 2 and u["sidebar"]["shown"] is True and u)
            open_tab = next(t for t in u0["tabs"]["items"] if t["session"] == s_open)
            w.click(*_tab_click_xy(open_tab))
            ua = wait_for(lambda: (u := w.ui()) and u["selected"] == s_open and u["sidebar"]["cwd"] == str(proj_a) and u)

            # 1. Switch to `[ Chats ]` tab -> loads Page 1 of 37 sessions (>= 2 pages)
            w.click(*_centre(ua["sidebar"]["chats_tab_rect"]))
            up1 = wait_for(
                lambda: (u := w.ui())
                and u["sidebar"]["tab"] == "chats"
                and u["sidebar"]["chats_loading"] is False
                and u["sidebar"]["chats_total"] == 37
                and u
            )
            total_pages = up1["sidebar"]["chats_total_pages"]
            assert total_pages >= 2
            assert up1["sidebar"]["chats_page"] == 1

            # 2. Traverse ALL pages via `>` and verify all 37 distinct chats are reachable
            seen_ids = [e["provider_session"] for e in up1["sidebar"]["chats_entries"]]
            cur_u = up1
            for expected_page in range(2, total_pages + 1):
                w.click(*_centre(cur_u["sidebar"]["next_rect"]))
                cur_u = wait_for(
                    lambda ep=expected_page: (u := w.ui())
                    and u["sidebar"]["chats_loading"] is False
                    and u["sidebar"]["chats_page"] == ep
                    and u
                )
                seen_ids.extend(e["provider_session"] for e in cur_u["sidebar"]["chats_entries"])
            assert len(seen_ids) == 37 and len(set(seen_ids)) == 37

            # Return to Page 1 via `<`
            while cur_u["sidebar"]["chats_page"] > 1:
                prev_target = cur_u["sidebar"]["chats_page"] - 1
                w.click(*_centre(cur_u["sidebar"]["prev_rect"]))
                cur_u = wait_for(
                    lambda pt=prev_target: (u := w.ui())
                    and u["sidebar"]["chats_loading"] is False
                    and u["sidebar"]["chats_page"] == pt
                    and u
                )

            # 3. Filter chats in `[ Chats ]` and clear filter
            w.click(*_centre(cur_u["sidebar"]["filter_rect"]))
            subprocess.run(["xdotool", "type", "--delay", "5", "CodexCard07"], env=w.env, check=True)
            uf = wait_for(
                lambda: (u := w.ui())
                and u["sidebar"]["chats_loading"] is False
                and u["sidebar"]["chats_total"] == 1
                and len(u["sidebar"]["chats_entries"]) == 1
                and u
            )
            assert "CodexCard07" in uf["sidebar"]["chats_entries"][0]["title"]
            w.click(*_centre(uf["sidebar"]["filter_clear_rect"]))
            uf_cleared = wait_for(
                lambda: (u := w.ui())
                and u["sidebar"]["chats_loading"] is False
                and u["sidebar"]["chats_total"] == 37
                and u
            )

            # 4. Toggle `Global (general)` scope in `[ Chats ]`, then switch back to active directory
            w.click(*_centre(uf_cleared["sidebar"]["global_rect"]))
            ug = wait_for(
                lambda: (u := w.ui())
                and u["sidebar"]["global_scope"] is True
                and u["sidebar"]["chats_loading"] is False
                and u["sidebar"]["chats_total"] == 2
                and u
            )
            w.click(*_centre(ug["sidebar"]["active_dir_rect"]))
            udir = wait_for(
                lambda: (u := w.ui())
                and u["sidebar"]["global_scope"] is False
                and u["sidebar"]["chats_loading"] is False
                and u["sidebar"]["chats_total"] == 37
                and u
            )

            # 5. One-click resume on `[closed]` (`disconnected`) tab -> reconnects in-place
            closed_card = next(e for e in udir["sidebar"]["chats_entries"] if e["open_session"] == s_closed)
            assert closed_card["open_state"] == "disconnected"
            w.click(*_centre(closed_card["rect"]))
            wait_for(lambda: (u := w.ui()) and u["selected"] == s_closed and status_of(conn, s_closed)["state"] == "running")
            assert len(w.ui()["tabs"]["items"]) == 2

            # 6. One-click on `[open]` (`running`) tab -> switches straight to existing tab
            u_after_reconn = wait_for(lambda: (u := w.ui()) and u["sidebar"]["chats_loading"] is False and u)
            open_card = next(e for e in u_after_reconn["sidebar"]["chats_entries"] if e["open_session"] == s_open)
            w.click(*_centre(open_card["rect"]))
            u_switched = wait_for(lambda: (u := w.ui()) and u["selected"] == s_open and u)
            assert len(u_switched["tabs"]["items"]) == 2

            # 7. One-click resume on external Claude `.jsonl` -> opens new tab with `--resume` & seeded history
            ext_c_card = next(e for e in u_switched["sidebar"]["chats_entries"] if e["provider_session"] == top_claude_ext)
            w.click(*_centre(ext_c_card["rect"]))
            u_res_c = wait_for(
                lambda: (u := w.ui())
                and len(u["tabs"]["items"]) == 3
                and u["selected"] not in (s_open, s_closed)
                and u
            )
            resumed_c_sid = u_res_c["selected"]
            wait_for(lambda: any("--resume" in av and top_claude_ext in av for av in argv_of(c_rec)))
            c_seeded = _records(log_dir, resumed_c_sid)
            assert any(f"resumed {top_claude_ext}" in ln for ln in c_seeded)
            assert any(">> Fix telemetry layout" in ln for ln in c_seeded)
            assert any("Telemetry layout updated." in ln for ln in c_seeded)

            # 8. One-click resume on external Codex `.jsonl` -> opens new tab with `thread/resume` & seeded history
            u_ready_x = wait_for(
                lambda: (u := w.ui())
                and u["sidebar"]["chats_loading"] is False
                and any(e["provider_session"] == top_codex_ext for e in u["sidebar"]["chats_entries"])
                and u
            )
            ext_x_card = next(e for e in u_ready_x["sidebar"]["chats_entries"] if e["provider_session"] == top_codex_ext)
            w.click(*_centre(ext_x_card["rect"]))
            u_res_x = wait_for(
                lambda: (u := w.ui())
                and len(u["tabs"]["items"]) == 4
                and u["selected"] not in (s_open, s_closed, resumed_c_sid)
                and u
            )
            resumed_x_sid = u_res_x["selected"]
            wait_for(lambda: any(m.get("method") == "thread/resume" and m.get("params", {}).get("threadId") == top_codex_ext for m in codex_sent(x_rec)))
            x_seeded = _records(log_dir, resumed_x_sid)
            assert any(f"resumed {top_codex_ext}" in ln for ln in x_seeded)
            assert any(">> Optimize sidebar pagination" in ln for ln in x_seeded)
            assert any("Pagination optimized cleanly." in ln for ln in x_seeded)
