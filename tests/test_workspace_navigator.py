"""M03-P02: persistent workspace-grouped session navigator and adjustable text size across IPC, CLI, HTTP, and GUI."""

from __future__ import annotations

import json
import os
import subprocess
from pathlib import Path

from lico_measure.p02 import ProtoConn as Conn, cli
from lico_measure.rig import Xvfb, private_dir
from lico_protocol import v1

from tests.account_helpers import account
from tests.gui_helpers import centre, click_at, drawn, item, menu_client
from tests.test_admin_controls import _claude_http_host
from tests.test_claude_adapter import claude_host, new_claude, status_of
from tests.test_codex_adapter import wait_for
from tests.test_first_view import ui, xdo
from tests.test_host_control import _http


def _init_git_repo(repo: Path) -> None:
    repo.mkdir(parents=True, exist_ok=True)
    subprocess.run(["git", "init", "-q", "-b", "main"], cwd=repo, check=True)
    subprocess.run(["git", "config", "user.email", "test@example.com"], cwd=repo, check=True)
    subprocess.run(["git", "config", "user.name", "Test"], cwd=repo, check=True)
    (repo / "README.md").write_text("hello\n")
    subprocess.run(["git", "add", "README.md"], cwd=repo, check=True)
    subprocess.run(["git", "commit", "-q", "-m", "init"], cwd=repo, check=True)


def test_settings_group_by_and_font_px_across_ipc_cli_and_http():
    with private_dir() as directory:
        host, _log, _received, token = _claude_http_host(directory)
        conn = Conn(host.socket)
        try:
            port = host.http_port
            auth = {"Authorization": f"Bearer {token}"}

            st0 = conn.call(op="status")
            assert st0["ok"] == 1 and v1.validate_response("status", st0) == []
            assert st0["settings"]["group_by"] == "account" and st0["settings"]["font_px"] == 14

            code, http_get = _http(port, "GET", "/v1/settings", auth)
            assert code == 200 and http_get["settings"]["group_by"] == "account" and http_get["settings"]["font_px"] == 14

            r_cli = cli(host, "settings", "--group-by", "workspace", "--font-size", "18")
            assert r_cli.returncode == 0
            parsed = json.loads(r_cli.stdout)
            assert parsed["settings"]["group_by"] == "workspace" and parsed["settings"]["font_px"] == 18
            assert v1.validate_response("settings.set", parsed) == []

            code, http_post = _http(port, "POST", "/v1/settings", auth, json.dumps({"font_px": 12}).encode())
            assert code == 200 and http_post["settings"]["group_by"] == "workspace" and http_post["settings"]["font_px"] == 12

            assert _http(port, "POST", "/v1/settings", auth, json.dumps({"font_px": 9}).encode())[0] == 400
            assert _http(port, "POST", "/v1/settings", auth, json.dumps({"group_by": "invalid"}).encode())[0] == 400
            assert conn.call(op="settings.set", font_px=24)["error"] == "bad_request"
        finally:
            conn.close()
            host.stop()


def test_gui_workspace_grouping_rolls_worktrees_up_and_font_size_persists_across_window_reopen():
    with private_dir() as directory:
        xvfb = Xvfb()
        host = client = conn = None
        try:
            repo = Path(directory) / "alpha-repo"
            _init_git_repo(repo)
            host, log, received = claude_host(directory)
            conn = Conn(host.socket)
            a1 = account(conn, "TeamA", "claude")
            a2 = account(conn, "TeamB", "claude")

            # Session 1 in repo root on account a1; Session 2 in git worktree of same repo on account a2; Session 3 scratch on a1
            r1 = conn.call(op="session.create", kind="claude", account=a1, mode="auto", cwd=str(repo), name="main-work")
            assert r1["ok"] == 1
            s1 = r1["session"]
            r2 = conn.call(op="session.create", kind="claude", account=a2, mode="auto", cwd=str(repo), worktree_branch="feat-wt", name="wt-work")
            assert r2["ok"] == 1
            s2 = r2["session"]
            s3, _ = new_claude(conn, account=a1, mode="auto")
            wait_for(lambda: all((status_of(conn, s) or {}).get("state") == "running" for s in (s1, s2, s3)))

            client = menu_client(host, xvfb.display)
            state = ui(client, lambda u: len(u["rows"]) == 3)
            assert state["settings"]["group_by"] == "account" and state["settings"]["font_px"] == 14
            assert "claude / TeamA" in state["list_text"] and "claude / TeamB" in state["list_text"]

            # Toggle grouping to workspace via Ctrl+Shift+G: s1 and worktree s2 roll up under `workspace / alpha-repo`!
            row_h_14 = state["layout"]["rows"][0][1] - state["layout"]["rows"][0][0]
            xdo(xvfb.display, "key", "ctrl+shift+g")
            state = ui(client, lambda u: u["settings"]["group_by"] == "workspace")
            lines = state["list_text"].splitlines()
            assert "workspace / alpha-repo" in lines and "workspace / (scratch)" in lines
            repo_idx = lines.index("workspace / alpha-repo")
            next_hdr = next(
                (i for i in range(repo_idx + 1, len(lines)) if lines[i].startswith("workspace / ")),
                len(lines),
            )
            # Both main-work and wt-work roll up together under `workspace / alpha-repo`
            under_repo = "\n".join(lines[repo_idx + 1 : next_hdr])
            assert "main-work" in under_repo and "wt-work" in under_repo and "(wt)" in under_repo

            # Increase font size twice via Ctrl+= and once via View menu -> 17 px; row height grows measurably
            xdo(xvfb.display, "key", "ctrl+equal")
            ui(client, lambda u: u["settings"]["font_px"] == 15)
            xdo(xvfb.display, "key", "ctrl+equal")
            ui(client, lambda u: u["settings"]["font_px"] == 16)
            xdo(xvfb.display, "key", "alt+v")
            m = ui(client, lambda u: drawn(u, 2))["menu"]
            assert item(m, "Group sessions: workspace")["enabled"] is True
            assert item(m, "Text size: 14 px (reset)")["enabled"] is True
            click_at(xvfb.display, *centre(item(m, "Text size: larger")["rect"]))
            state = ui(client, lambda u: u["settings"]["font_px"] == 17)
            row_h_17 = state["layout"]["rows"][0][1] - state["layout"]["rows"][0][0]
            assert row_h_17 > row_h_14 + 2.5

            # Reopen window against same host: workspace grouping and 17 px font size persist immediately, and Ctrl+0 resets to 14 px
            client.kill()
            client = menu_client(host, xvfb.display)
            state = ui(client, lambda u: len(u["rows"]) == 3 and u["settings"]["font_px"] == 17)
            assert state["settings"]["group_by"] == "workspace" and "workspace / alpha-repo" in state["list_text"]
            xdo(xvfb.display, "key", "ctrl+0")
            state = ui(client, lambda u: u["settings"]["font_px"] == 14)
            assert abs((state["layout"]["rows"][0][1] - state["layout"]["rows"][0][0]) - row_h_14) < 0.5
        finally:
            if conn:
                conn.close()
            if client and client.proc.poll() is None:
                client.kill()
            if host:
                host.stop()
            xvfb.stop()
