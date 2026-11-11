"""M04-P03 tests: provider weekly quota (`7d` remaining) and working-directory context display.

Verifies that:
1. Accounts with no observed rate-limit telemetry continue to report `usage: null` (never fabricating 0% or 100%).
2. Both `claude` (`rate_limit_event` `seven_day` / `five_hour`) and `codex` (`account/rateLimits/updated` + `account/rateLimits/read`) populate `usage` on `account.list` and `account_usage` on `status` within protocol v1 schemas.
3. `lico gui` surfaces `7d XX% left` (and `7d LOW` / `7d CRITICAL` below 20% / 5%) alongside `dir: <workspace>` in the session header, account group heading, `Accounts` list, and `Account` actions view.
"""

from __future__ import annotations

import subprocess
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir
from lico_protocol.v1 import validate_response
from tests.account_helpers import account
from tests.test_claude_adapter import status_of
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_new_session_screen import claude_args
from tests.test_reconnect_window import codex_args


def test_claude_and_codex_weekly_quota_telemetry_and_gui_context_display():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = root / "quota-workbench"
        proj.mkdir(mode=0o700, parents=True, exist_ok=True)
        log_dir = root / "logs"
        c_args, _ = claude_args(root)
        x_args, _ = codex_args(root)
        made: dict[str, str] = {}

        def setup(w: ProductWindow) -> None:
            conn = Conn(w.socket)
            a_claude = account(conn, "WorkClaude", "claude")
            a_codex = account(conn, "WorkCodex", "codex")

            # Before any telemetry is reported, usage must be strictly null.
            pre_accounts = conn.call(op="account.list")
            assert validate_response("account.list", pre_accounts) == []
            by_id = {a["account"]: a for a in pre_accounts["accounts"]}
            assert by_id[a_claude]["usage"] is None
            assert by_id[a_codex]["usage"] is None

            s_claude = conn.call(op="session.create", kind="claude", account=a_claude, mode="auto", cwd=str(proj), name="ClaudeTask")
            s_codex = conn.call(op="session.create", kind="codex", account=a_codex, mode="auto", cwd=str(proj), name="CodexTask")
            assert s_claude["ok"] == 1 and s_codex["ok"] == 1

            # Trigger Claude rate_limit_event: 32% used -> 68% weekly remaining, 9% used -> 91% 5h remaining.
            assert conn.call(
                op="input", session=s_claude["session"], generation=s_claude["generation"],
                request_id="q-claude-1", text="ratelimit:0.32:0.09",
            )["ok"] == 1

            # Trigger Codex rateLimits update + read: 86% used -> 14% weekly remaining (LOW), 20% used -> 80% 5h remaining.
            assert conn.call(
                op="input", session=s_codex["session"], generation=s_codex["generation"],
                request_id="q-codex-1", text="ratelimit:86:20",
            )["ok"] == 1

            def both_reported():
                accts = conn.call(op="account.list")
                assert validate_response("account.list", accts) == []
                m = {a["account"]: a["usage"] for a in accts["accounts"]}
                if (
                    m[a_claude]
                    and m[a_codex]
                    and status_of(conn, s_claude["session"])["activity"] == "idle"
                    and status_of(conn, s_codex["session"])["activity"] == "idle"
                ):
                    return m
                return None

            usage_map = wait_for(both_reported)
            assert usage_map[a_claude]["weekly_remaining_pct"] == 68
            assert usage_map[a_claude]["short_remaining_pct"] == 91
            assert usage_map[a_codex]["weekly_remaining_pct"] == 14
            assert usage_map[a_codex]["short_remaining_pct"] == 80

            st = conn.call(op="status")
            assert validate_response("status", st) == []
            st_by_sid = {s["session"]: s for s in st["sessions"]}
            assert st_by_sid[s_claude["session"]]["account_usage"]["weekly_remaining_pct"] == 68
            assert st_by_sid[s_codex["session"]]["account_usage"]["weekly_remaining_pct"] == 14
            made["s_claude"] = s_claude["session"]
            made["s_codex"] = s_codex["session"]

        with ProductWindow(
            host_binary=LICO,
            host_args=[*c_args, *x_args, "--log-dir", str(log_dir)],
            env_extra={"LANG": "C.UTF-8"},
            before_window=setup,
        ) as w:
            w.proc.stdin.write("load\n")
            w.proc.stdin.flush()

            ui0 = wait_for(
                lambda: (u := w.ui())
                and len(u["layout"]["rows"]) == 2
                and all(r["weekly_remaining_pct"] is not None for r in u["rows"])
                and u
            )

            # Click Claude session row and verify header shows title + dir + 7d 68% left.
            claude_hit = next(r for r in ui0["layout"]["rows"] if r[2] == made["s_claude"])
            w.click(100.0, (claude_hit[0] + claude_hit[1]) / 2.0)
            ui_claude = wait_for(lambda: (u := w.ui()) and u["selected"] == made["s_claude"] and "7d 68% left" in u["header"] and u)
            assert f"dir: {proj}" in ui_claude["header"]
            assert "ClaudeTask" in ui_claude["header"]
            assert "claude / WorkClaude  7d 68%" in ui_claude["list_text"], ui_claude["list_text"]

            # Click Codex session row and verify LOW (<20%) indicator in header and `!` in sidebar heading.
            codex_hit = next(r for r in ui_claude["layout"]["rows"] if r[2] == made["s_codex"])
            w.click(100.0, (codex_hit[0] + codex_hit[1]) / 2.0)
            ui_codex = wait_for(lambda: (u := w.ui()) and u["selected"] == made["s_codex"] and "7d LOW 14% left" in u["header"] and u)
            assert f"dir: {proj}" in ui_codex["header"]
            assert "codex / WorkCodex  7d 14%!" in ui_codex["list_text"], ui_codex["list_text"]

            # Open Accounts via toolbar and verify both accounts show detailed quota breakdown.
            acct_rect = next(b for b in ui_codex["toolbar"]["buttons"] if b["id"] == "accounts")["rect"]
            w.click((acct_rect[0] + acct_rect[2]) / 2.0, (acct_rect[1] + acct_rect[3]) / 2.0)
            ui_accts = wait_for(lambda: (u := w.ui()) and u["panel"] and u["panel"]["title"] == "Accounts" and u)
            body = "\n".join(ui_accts["panel"]["body"])
            assert "7d 68% left · 5h 91% left" in body, body
            assert "7d LOW 14% left · 5h 80% left" in body, body

            # Enter Account 1 actions view and verify quota line appears there too.
            subprocess.run(["xdotool", "key", "1"], env=w.env, check=True)
            ui_act = wait_for(lambda: (u := w.ui()) and u["panel"] and u["panel"]["title"].startswith("Account:") and u)
            assert any(line.startswith("quota: ") for line in ui_act["panel"]["body"]), ui_act["panel"]["body"]
