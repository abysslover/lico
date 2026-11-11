"""M03-P01: account.rename, in-window session.rebind, and live session.mode across IPC, CLI, HTTP, and GUI."""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

from lico_measure.p02 import HOSTD_DIR, ProtoConn as Conn, cli, start
from lico_measure.rig import Xvfb, private_dir
from lico_protocol import v1

from tests.account_helpers import account
from tests.test_claude_adapter import FIXTURE, SIM, argv_of, claude_host, new_claude, records, status_of
from tests.test_codex_adapter import codex_host, sent, usual_umask, wait_for
from tests.test_first_view import ui, xdo
from tests.test_host_control import _http
from tests.gui_helpers import centre, click_at, drawn, item, menu_client


def _claude_http_host(directory: str):
    received = Path(directory) / "claude_received.txt"
    token_file = os.path.join(directory, "http.token")
    extra = [
        "--claude", sys.executable, "--claude-arg", "-I", "--claude-arg", str(SIM),
        "--claude-arg", "--received", "--claude-arg", str(received),
        "--claude-arg", "--fixture", "--claude-arg", str(FIXTURE),
        "--cred-bin", str(HOSTD_DIR / "lico-cred"), "--cred-dir", str(Path(directory) / "credentials"),
        "--http-port", "auto", "--http-token-file", token_file,
    ]
    with usual_umask():
        host, log = start(directory, extra=extra, sessions=0, producing=0, history=0, retain=100000)
    return host, log, received, Path(token_file).read_text().strip()


def test_account_rename_session_mode_and_rebind_across_ipc_cli_and_http():
    with private_dir() as directory:
        host, log, received, token = _claude_http_host(directory)
        try:
            conn = Conn(host.socket)
            port = host.http_port
            auth = {"Authorization": f"Bearer {token}"}

            a1 = account(conn, "Alpha", "claude")
            a2 = account(conn, "Beta", "claude")

            # 1) account.rename over IPC, CLI, and HTTP + schema/error checks
            r_ipc = conn.call(op="account.rename", account=a1, label="Alpha Team")
            assert r_ipc["ok"] == 1 and r_ipc["label"] == "Alpha Team" and v1.validate_response("account.rename", r_ipc) == []

            r_cli = cli(host, "account", "rename", a1, "Alpha Ops")
            assert r_cli.returncode == 0 and json.loads(r_cli.stdout)["label"] == "Alpha Ops"

            st, r_http = _http(port, "POST", f"/v1/accounts/{a2}/rename", auth, json.dumps({"label": "Beta Backup"}).encode())
            assert st == 200 and r_http["label"] == "Beta Backup"
            assert _http(port, "POST", "/v1/accounts/a000000/rename", auth, json.dumps({"label": "Nope"}).encode())[0] == 404
            assert _http(port, "POST", f"/v1/accounts/{a2}/rename", auth, json.dumps({"label": ""}).encode())[0] == 400

            labels = {entry["account"]: entry["label"] for entry in conn.call(op="account.list")["accounts"]}
            assert labels == {a1: "Alpha Ops", a2: "Beta Backup"}

            # 2) Claude session.mode: refuses while approval is open, restarts cleanly with --resume when idle
            sid, gen = new_claude(conn, account=a1, mode="default")
            wait_for(lambda: len(argv_of(received)) >= 1)
            conn.call(op="input", session=sid, generation=gen, request_id="t1", text="fixture")
            wait_for(lambda: status_of(conn, sid).get("approvals"))

            # Refused with "busy" while approval/turn is open
            busy_reply = conn.call(op="session.mode", session=sid, generation=gen, mode="auto")
            assert busy_reply["ok"] == 0 and busy_reply["error"] == "busy"

            # Finish the open turn
            for _ in range(2):
                wait_for(lambda: status_of(conn, sid).get("approvals"))
                ap = status_of(conn, sid)["approvals"][0]
                conn.call(op="approval.answer", session=sid, generation=gen, approval_id=ap["approval_id"], answer="allow")
            wait_for(lambda: "[turn completed]" in records(log, sid) and status_of(conn, sid).get("activity") == "idle")

            # Switch default -> auto via CLI: respawns Claude with bypassPermissions + --resume
            done_cli = cli(host, "session", "mode", sid, "auto")
            assert done_cli.returncode == 0, done_cli.stderr
            body_cli = json.loads(done_cli.stdout)
            assert v1.validate_response("session.mode", body_cli) == []
            gen2 = body_cli["generation"]
            assert gen2 == gen + 1 and body_cli["mode"] == "auto"
            wait_for(lambda: len(argv_of(received)) >= 2)
            second_argv = argv_of(received)[-1]
            assert second_argv[second_argv.index("--permission-mode") + 1] == "bypassPermissions"
            assert "--resume" in second_argv
            assert "[permission mode: auto]" in records(log, sid)

            # Switch auto -> default via HTTP
            st, body_http = _http(port, "POST", f"/v1/sessions/{sid}/mode", auth, json.dumps({"generation": gen2, "mode": "default"}).encode())
            assert st == 200 and body_http["mode"] == "default" and body_http["generation"] == gen2 + 1
            gen3 = body_http["generation"]

            # 3) Close and rebind via HTTP POST /v1/sessions/<id>/rebind
            assert conn.call(op="session.close", session=sid, generation=gen3)["ok"] == 1
            st, reb = _http(port, "POST", f"/v1/sessions/{sid}/rebind", auth, json.dumps({"generation": gen3, "account": a2}).encode())
            assert st == 200 and reb["generation"] == gen3 + 1
            s_after = status_of(conn, sid)
            assert s_after["account"] == a2 and s_after["state"] == "running"
        finally:
            host.stop()

    # 4) Codex session.mode switches per-turn approvalPolicy without restarting the app-server
    with private_dir() as directory:
        host, log, received = codex_host(directory)
        try:
            conn = Conn(host.socket)
            created = conn.call(op="session.create", kind="codex", mode="auto")
            sid, gen = created["session"], created["generation"]
            wait_for(lambda: any(m.get("method") == "thread/start" for m in sent(received)))
            switched = conn.call(op="session.mode", session=sid, generation=gen, mode="default")
            assert switched["ok"] == 1 and switched["generation"] == gen and switched["mode"] == "default"
            conn.call(op="input", session=sid, generation=gen, request_id="cx-1", text="hello")
            turns = wait_for(lambda: [m["params"] for m in sent(received) if m.get("method") == "turn/start"])
            assert turns[-1]["approvalPolicy"] == "untrusted"
        finally:
            host.stop()


def test_gui_account_rename_session_rebind_and_permission_mode_toggle():
    with private_dir() as directory:
        xvfb = Xvfb()
        host = client = None
        try:
            host, log, received = claude_host(directory)
            conn = Conn(host.socket)
            a1 = account(conn, "Primary", "claude")
            a2 = account(conn, "Backup", "claude")
            sid, gen = new_claude(conn, account=a1, mode="auto")
            # Give the session a conversation id so close + rebind is allowed
            conn.call(op="input", session=sid, generation=gen, request_id="init-1", text="hello")
            wait_for(lambda: "[turn completed]" in records(log, sid) and status_of(conn, sid).get("activity") == "idle")

            client = menu_client(host, xvfb.display)
            ui(client, lambda u: len(u["rows"]) == 1)

            # 1) Rename account 1 in Ctrl+A -> 1 -> R -> BackSpace.. -> type "AdminMain" -> Return
            xdo(xvfb.display, "key", "ctrl+a")
            ui(client, lambda u: u["panel"] and u["panel"]["title"] == "Accounts")
            xdo(xvfb.display, "key", "1")
            state = ui(client, lambda u: u["panel"] and u["panel"]["title"].startswith("Account:"))
            assert any("R  rename this account" in line for line in state["panel"]["body"])
            xdo(xvfb.display, "key", "r")
            ui(client, lambda u: u["panel"] and u["panel"]["title"].startswith("Rename account:") and u["panel"]["text_entry"])
            xdo(xvfb.display, "key", "ctrl+u")
            xdo(xvfb.display, "type", "--delay", "5", "AdminMain")
            ui(client, lambda u: u["panel"] and u["panel"]["input"] == "AdminMain")
            xdo(xvfb.display, "key", "Return")
            state = ui(client, lambda u: u["panel"] and "Renamed." in "\n".join(u["panel"]["body"]))
            assert "AdminMain" in state["panel"]["title"]
            xdo(xvfb.display, "key", "Escape")
            xdo(xvfb.display, "key", "Escape")
            ui(client, lambda u: u["panel"] is None)

            # 2) Select session (Alt+1) and toggle permission mode via Ctrl+Shift+M and via Session menu
            xdo(xvfb.display, "key", "alt+1")
            state = ui(client, lambda u: u["selected"] == sid and "AdminMain" in u["header"])
            assert "mode: auto (permissions skipped)" in state["header"]

            xdo(xvfb.display, "key", "ctrl+shift+m")
            state = ui(client, lambda u: "mode: asks for approval" in u["header"])
            assert "Permission mode: asks for approval." in state["header"]

            xdo(xvfb.display, "key", "alt+s")
            m = ui(client, lambda u: drawn(u, 1))["menu"]
            assert item(m, "Permission mode: approval")["enabled"] is True
            assert item(m, "Rebind account...")["enabled"] is False and item(m, "Rebind account...")["reason"] == "close the session first"
            click_at(xvfb.display, *centre(item(m, "Permission mode: approval")["rect"]))
            ui(client, lambda u: "mode: auto (permissions skipped)" in u["header"])

            # 3) Close session (Ctrl+Shift+W) and rebind via Ctrl+Shift+B -> 2 (Backup)
            xdo(xvfb.display, "key", "ctrl+shift+w")
            ui(client, lambda u: u["rows"][0]["state"] == "disconnected")
            xdo(xvfb.display, "key", "ctrl+shift+b")
            state = ui(client, lambda u: u["panel"] and u["panel"]["title"] == "Rebind session to account")
            assert any("2  claude / Backup" in line for line in state["panel"]["body"])
            xdo(xvfb.display, "key", "2")
            state = ui(client, lambda u: u["panel"] is None and u["rows"][0]["account"] == a2 and u["rows"][0]["state"] == "running")
            assert "Backup" in state["header"]
        finally:
            if client and client.proc.poll() is None:
                client.kill()
            if host:
                host.stop()
            xvfb.stop()
