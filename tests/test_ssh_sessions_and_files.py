"""M06-P03: SSH remote directory browsing (`folder.list`, `folder.read`, `folder.git` with
`ssh_host`), `Files` `Host: local v` / `Host: <alias> v` combo-box dropdown, non-blocking
`ssh_unreachable` handling, `+ New here` session creation, and persistent remote `claude`/`codex`
session execution, approval, quota telemetry, mode switch, and `session.resume` (`lico 0.2.2`)."""

from __future__ import annotations

import json
import os
import stat
import subprocess
import sys
import time
import urllib.parse
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn, cli, read_log, start
from lico_measure.rig import REPO, finished_lines, private_dir
from lico_protocol import v1
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_control_parity import _http

CLAUDE_SIM = REPO / "src" / "host" / "adapters" / "claude" / "simulator.py"
CLAUDE_FIXTURE = REPO / "tests" / "fixtures" / "claude_stream" / "turn_with_permission.json"
CODEX_SIM = REPO / "src" / "host" / "adapters" / "codex" / "simulator.py"
CODEX_FIXTURE = REPO / "tests" / "fixtures" / "codex_app_server" / "turn_with_approval.json"


def _make_remote_workbench_rig(root: Path) -> tuple[Path, Path, Path]:
    """Creates a synthetic ssh_config (`gpu-box`, `build-srv`), remote project files, and
    remote `~/.local/bin/{claude,codex}` wrappers backed by the protocol simulators."""
    ssh_dir = root / "ssh_cfg"
    ssh_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
    ssh_cfg = ssh_dir / "config"
    ssh_cfg.write_text(
        "Host gpu-box\n"
        "  HostName 10.20.0.11\n"
        "  User researcher\n"
        "\n"
        "Host build-srv\n"
        "  HostName 10.20.0.12\n"
        "  User builder\n"
        "\n"
        "Host offline-box\n"
        "  HostName 10.20.0.98\n"
        "  User ghost\n"
        "\n"
        "Host hang-box\n"
        "  HostName 10.20.0.99\n"
        "  User slow\n",
        encoding="utf-8",
    )
    os.chmod(ssh_cfg, 0o600)

    remote_root = root / "remote_hosts"
    uhome = remote_root / "gpu-box" / "user_home"
    bin_dir = uhome / ".local" / "bin"
    bin_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
    (remote_root / "build-srv" / "user_home" / ".local" / "bin").mkdir(mode=0o700, parents=True, exist_ok=True)

    # Remote project folder on gpu-box with text, HTML, hidden dotfile, subdir, symlink, and git repo
    exp_dir = uhome / "projects" / "experiment"
    (exp_dir / "sub").mkdir(mode=0o755, parents=True, exist_ok=True)
    (exp_dir / "README.md").write_text("# Remote GPU Experiment\nReady on gpu-box.\n", encoding="utf-8")
    (exp_dir / "report.html").write_text("<!doctype html><html><body>Remote Report</body></html>\n", encoding="utf-8")
    (exp_dir / ".env.local").write_text("GPU_SLOT=0\n", encoding="utf-8")
    (exp_dir / "sub" / "notes.txt").write_text("sub-note\n", encoding="utf-8")
    os.symlink("/etc/hosts", exp_dir / "escape_link")
    subprocess.run(["git", "init", "-q"], cwd=str(exp_dir), check=True)

    # Protected remote Lico state directory
    state_lico = uhome / ".local" / "state" / "lico"
    state_lico.mkdir(mode=0o700, parents=True, exist_ok=True)
    (state_lico / "secret.txt").write_text("do-not-read\n", encoding="utf-8")

    claude_wrapper = bin_dir / "claude"
    claude_wrapper.write_text(
        f"#!{sys.executable}\n"
        "import json, os, sys\n"
        "from pathlib import Path\n"
        f"uhome = Path({str(uhome)!r})\n"
        "args = sys.argv[1:]\n"
        "with open(uhome / 'claude_runs.jsonl', 'a', encoding='utf-8') as fh:\n"
        "    fh.write(json.dumps({\n"
        "        'argv': args,\n"
        "        'cwd': os.getcwd(),\n"
        "        'home': os.environ.get('HOME', ''),\n"
        "        'cfg': os.environ.get('CLAUDE_CONFIG_DIR', ''),\n"
        "        'env_keys': sorted(os.environ.keys()),\n"
        "    }) + '\\n')\n"
        f"os.execv({sys.executable!r}, [{sys.executable!r}, '-I', {str(CLAUDE_SIM)!r},\n"
        "    '--received', str(uhome / 'claude_received.txt'),\n"
        f"    '--fixture', {str(CLAUDE_FIXTURE)!r}, *args])\n",
        encoding="utf-8",
    )
    os.chmod(claude_wrapper, 0o700)

    codex_wrapper = bin_dir / "codex"
    codex_wrapper.write_text(
        f"#!{sys.executable}\n"
        "import json, os, sys\n"
        "from pathlib import Path\n"
        f"uhome = Path({str(uhome)!r})\n"
        "args = sys.argv[1:]\n"
        "assert args == ['app-server'], f'expected app-server, got {args!r}'\n"
        "with open(uhome / 'codex_runs.jsonl', 'a', encoding='utf-8') as fh:\n"
        "    fh.write(json.dumps({\n"
        "        'argv': args,\n"
        "        'cwd': os.getcwd(),\n"
        "        'home': os.environ.get('HOME', ''),\n"
        "        'cfg': os.environ.get('CODEX_HOME', ''),\n"
        "        'env_keys': sorted(os.environ.keys()),\n"
        "    }) + '\\n')\n"
        f"os.execv({sys.executable!r}, [{sys.executable!r}, '-I', {str(CODEX_SIM)!r},\n"
        "    '--received', str(uhome / 'codex_received.jsonl'),\n"
        f"    '--fixture', {str(CODEX_FIXTURE)!r}])\n",
        encoding="utf-8",
    )
    os.chmod(codex_wrapper, 0o700)

    mock_ssh = root / "mock_ssh.py"
    mock_ssh.write_text(
        f"#!{sys.executable}\n"
        "import os, sys, time\n"
        "from pathlib import Path\n"
        "args = sys.argv[1:]\n"
        "assert '-o' in args and 'BatchMode=yes' in args, args\n"
        "sep = args.index('--')\n"
        "host = args[sep + 1]\n"
        "remote_cmd = args[sep + 2]\n"
        f"root_dir = Path({str(root)!r})\n"
        "with open(root_dir / 'ssh_calls.log', 'a', encoding='utf-8') as fh:\n"
        "    fh.write(host + '\\n')\n"
        "if host == 'hang-box':\n"
        "    (root_dir / 'hang_started.txt').write_text('started\\n', encoding='utf-8')\n"
        "    time.sleep(1.8)\n"
        "    sys.stderr.write('ssh: connect to host 10.20.0.99 port 22: Operation timed out\\n')\n"
        "    sys.exit(255)\n"
        f"rroot = Path({str(remote_root)!r})\n"
        "uhome = rroot / host / 'user_home'\n"
        "if not uhome.is_dir():\n"
        "    sys.stderr.write(f'ssh: Could not resolve hostname {host}\\n')\n"
        "    sys.exit(255)\n"
        "os.chdir(str(uhome))\n"
        "env = {'HOME': str(uhome), 'PATH': '/usr/bin:/bin', 'LANG': 'C.UTF-8'}\n"
        "os.execve('/bin/sh', ['/bin/sh', '-c', remote_cmd], env)\n",
        encoding="utf-8",
    )
    os.chmod(mock_ssh, 0o700)
    return ssh_cfg, mock_ssh, remote_root


def _records(log_dir: Path, sid: str) -> list[str]:
    return [p.decode("utf-8", errors="replace") for _, p in read_log(log_dir, sid)]


def _status_of(conn: Conn, sid: str) -> dict:
    return [s for s in conn.call(op="status")["sessions"] if s["session"] == sid][0]


def test_remote_files_parity_and_safety_across_socket_cli_and_http():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        token_file = str(root / "http.token")
        ssh_cfg, mock_ssh, remote_root = _make_remote_workbench_rig(root)
        uhome = (remote_root / "gpu-box" / "user_home").resolve()
        exp_dir = (uhome / "projects" / "experiment").resolve()

        host, _ = start(
            directory,
            extra=[
                "--text",
                "--http-port",
                "auto",
                "--http-token-file",
                token_file,
                "--ssh",
                str(mock_ssh),
                "--ssh-config",
                str(ssh_cfg),
            ],
            sessions=0,
            producing=0,
        )
        try:
            wait_for(lambda: Path(token_file).exists() and host.http_port is not None)
            token = Path(token_file).read_text(encoding="utf-8").strip()
            port = host.http_port
            conn = Conn(host.socket)

            # 1. Remote `folder.list` on `~` expands remote $HOME and filters dotfiles by default
            home_off = conn.call(op="folder.list", cwd="~", ssh_host="gpu-box")
            assert home_off["ok"] == 1 and v1.validate_response("folder.list", home_off) == []
            assert home_off["cwd"] == str(uhome) and home_off["ssh_host"] == "gpu-box"
            assert [e["name"] for e in home_off["entries"]] == ["projects"]

            home_on = conn.call(op="folder.list", cwd="~", show_hidden=True, ssh_host="gpu-box")
            assert home_on["ok"] == 1 and v1.validate_response("folder.list", home_on) == []
            assert ".local" in [e["name"] for e in home_on["entries"]]

            # 2. CLI & HTTP parity on `~/projects/experiment`
            cli_off = json.loads(cli(host, "files", "~/projects/experiment", "--ssh-host", "gpu-box").stdout)
            assert cli_off["ok"] == 1 and v1.validate_response("folder.list", cli_off) == []
            assert cli_off["cwd"] == str(exp_dir) and cli_off["ssh_host"] == "gpu-box"
            assert [e["name"] for e in cli_off["entries"]] == ["sub", "escape_link", "README.md", "report.html"]

            cli_on = json.loads(cli(host, "files", "~/projects/experiment", "--hidden", "--ssh-host", "gpu-box").stdout)
            assert ".env.local" in [e["name"] for e in cli_on["entries"]]

            q_cwd = urllib.parse.quote("~/projects/experiment", safe="")
            st_l, http_l = _http(port, "GET", f"/v1/files?cwd={q_cwd}&ssh_host=gpu-box&hidden=1", token)
            assert st_l == 200 and v1.validate_response("folder.list", http_l) == []
            assert http_l["ssh_host"] == "gpu-box" and ".env.local" in [e["name"] for e in http_l["entries"]]

            # 3. Remote `folder.read` across socket, CLI, and HTTP
            readme_path = str(exp_dir / "README.md")
            sock_r = conn.call(op="folder.read", path=readme_path, ssh_host="gpu-box")
            assert sock_r["ok"] == 1 and v1.validate_response("folder.read", sock_r) == []
            assert sock_r["ssh_host"] == "gpu-box" and sock_r["binary"] is False
            assert "Remote GPU Experiment" in sock_r["text"]

            cli_r = json.loads(cli(host, "files", "--read", "~/projects/experiment/README.md", "--ssh-host", "gpu-box").stdout)
            assert cli_r["ok"] == 1 and cli_r["text"] == sock_r["text"] and cli_r["ssh_host"] == "gpu-box"

            q_path = urllib.parse.quote(readme_path, safe="")
            st_r, http_r = _http(port, "GET", f"/v1/files/read?path={q_path}&ssh_host=gpu-box", token)
            assert st_r == 200 and v1.validate_response("folder.read", http_r) == []
            assert http_r["text"] == sock_r["text"] and http_r["ssh_host"] == "gpu-box"

            # 4. Safety refusals: symlink (`file_not_regular`), remote state root (`folder_protected`), bad ssh_host
            assert conn.call(op="folder.read", path=str(exp_dir / "escape_link"), ssh_host="gpu-box")["error"] == "file_not_regular"
            state_root = str(uhome / ".local" / "state" / "lico")
            assert conn.call(op="folder.list", cwd=state_root, ssh_host="gpu-box")["error"] == "folder_protected"
            assert conn.call(op="folder.read", path=f"{state_root}/secret.txt", ssh_host="gpu-box")["error"] == "folder_protected"
            assert conn.call(op="folder.list", cwd="~", ssh_host="-oProxyCommand=id")["error"] == "bad_request"

            # 5. Remote `folder.git`
            git_yes = conn.call(op="folder.git", cwd=str(exp_dir / "sub"), ssh_host="gpu-box")
            assert git_yes["ok"] == 1 and v1.validate_response("folder.git", git_yes) == []
            assert git_yes["is_git"] is True and git_yes["repo"] == str(exp_dir) and git_yes["ssh_host"] == "gpu-box"

            git_no = conn.call(op="folder.git", cwd=str(uhome), ssh_host="gpu-box")
            assert git_no["ok"] == 1 and git_no["is_git"] is False and git_no["ssh_host"] == "gpu-box"

            # 6. Removed/unreachable SSH host returns `ssh_unreachable` across socket and HTTP (503)
            unr_l = conn.call(op="folder.list", cwd="~", ssh_host="offline-box")
            assert unr_l["ok"] == 0 and unr_l["error"] == "ssh_unreachable" and v1.validate_error(unr_l) == []
            unr_r = conn.call(op="folder.read", path="/etc/hostname", ssh_host="offline-box")
            assert unr_r["ok"] == 0 and unr_r["error"] == "ssh_unreachable" and v1.validate_error(unr_r) == []
            st_unr, http_unr = _http(port, "GET", "/v1/files?cwd=~&ssh_host=offline-box", token)
            assert st_unr == 503 and http_unr["ok"] == 0 and http_unr["error"] == "ssh_unreachable"
            assert v1.validate_error(http_unr) == []
        finally:
            host.stop()


def test_remote_claude_and_codex_sessions_stream_approve_quota_mode_and_resume_over_ssh():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        ssh_cfg, mock_ssh, remote_root = _make_remote_workbench_rig(root)
        uhome = (remote_root / "gpu-box" / "user_home").resolve()
        exp_dir = (uhome / "projects" / "experiment").resolve()

        host, log_dir = start(
            directory,
            extra=["--ssh", str(mock_ssh), "--ssh-config", str(ssh_cfg)],
            sessions=0,
            producing=0,
            history=0,
        )
        try:
            conn = Conn(host.socket)
            claude_acct = conn.call(op="account.create", provider="claude", label="GPUClaude", ssh_host="gpu-box")["account"]
            codex_acct = conn.call(op="account.create", provider="codex", label="GPUCodex", ssh_host="gpu-box")["account"]
            offline_acct = conn.call(op="account.create", provider="claude", label="OfflineClaude", ssh_host="offline-box")["account"]

            # Remote folder validation, unreachable SSH host refusal, and local worktree refusal
            unr_s = conn.call(op="session.create", kind="claude", account=offline_acct)
            assert unr_s["ok"] == 0 and unr_s["error"] == "ssh_unreachable"
            assert (
                conn.call(op="session.create", kind="claude", account=claude_acct, cwd="/nonexistent/remote")["error"]
                == "folder_missing"
            )
            assert (
                conn.call(
                    op="session.create",
                    kind="claude",
                    account=claude_acct,
                    cwd=str(exp_dir),
                    worktree_branch="feat/remote",
                )["error"]
                == "option_unavailable"
            )
            assert (
                conn.call(
                    op="session.create",
                    kind="claude",
                    account=claude_acct,
                    cwd=str(uhome / ".local" / "state" / "lico"),
                )["error"]
                == "folder_protected"
            )

            # 1. Remote Claude session: prompt echo, 0700 home isolation, approval, mode switch, and resume
            created_c = conn.call(
                op="session.create", kind="claude", account=claude_acct, cwd="~/projects/experiment", mode="default"
            )
            assert created_c["ok"] == 1
            csid, cgen = created_c["session"], created_c["generation"]

            st_full = conn.call(op="status")
            assert v1.validate_response("status", st_full) == []
            cst = [s for s in st_full["sessions"] if s["session"] == csid][0]
            assert cst["ssh_host"] == "gpu-box" and cst["cwd"] == str(exp_dir)
            # Remote folder must not pollute local recent folders
            assert conn.call(op="session.options")["recent"] == []

            assert conn.call(op="input", session=csid, generation=cgen, request_id="c-1", text="hello remote claude")["ok"] == 1
            wait_for(lambda: "echo: hello remote claude" in _records(log_dir, csid))

            c_runs = [json.loads(line) for line in finished_lines(uhome / "claude_runs.jsonl")]
            assert len(c_runs) == 1
            assert c_runs[0]["cwd"] == str(exp_dir)
            assert "DISPLAY" not in c_runs[0]["env_keys"]
            c_acct_root = uhome / ".local" / "state" / "lico" / "accounts" / claude_acct
            assert stat.S_IMODE(c_acct_root.stat().st_mode) == 0o700
            assert stat.S_IMODE((c_acct_root / "home").stat().st_mode) == 0o700
            assert stat.S_IMODE((c_acct_root / "claude").stat().st_mode) == 0o700

            # Approval round-trip over SSH
            assert conn.call(op="input", session=csid, generation=cgen, request_id="c-fx", text="fixture")["ok"] == 1
            for _ in range(2):
                pending = wait_for(lambda: _status_of(conn, csid).get("approvals"))
                assert (
                    conn.call(
                        op="approval.answer",
                        session=csid,
                        generation=cgen,
                        approval_id=pending[0]["approval_id"],
                        answer="allow",
                    )["ok"]
                    == 1
                )
            wait_for(lambda: _records(log_dir, csid).count("[turn completed]") >= 2)

            first_provider_sid = [
                line.split()[1]
                for line in finished_lines(uhome / "claude_received.txt")
                if line.startswith("SESSION ")
            ][0]

            # Live permission mode switch (`default` -> `auto`) restarts remote Claude with `--resume`
            mode_rep = conn.call(op="session.mode", session=csid, generation=cgen, mode="auto")
            assert mode_rep["ok"] == 1
            cgen = mode_rep["generation"]
            c_runs2 = wait_for(
                lambda: (r := [json.loads(l) for l in finished_lines(uhome / "claude_runs.jsonl")]) and len(r) >= 2 and r
            )
            assert "--resume" in c_runs2[1]["argv"]
            assert c_runs2[1]["argv"][c_runs2[1]["argv"].index("--resume") + 1] == first_provider_sid
            assert c_runs2[1]["argv"][c_runs2[1]["argv"].index("--permission-mode") + 1] == "bypassPermissions"

            # Crash + explicit `session.resume` over SSH
            assert conn.call(op="input", session=csid, generation=cgen, request_id="c-boom", text="crash")["ok"] == 1
            wait_for(lambda: _status_of(conn, csid)["state"] == "disconnected")
            res_c = conn.call(op="session.resume", session=csid, generation=cgen)
            assert res_c["ok"] == 1
            cgen = res_c["generation"]
            assert conn.call(op="input", session=csid, generation=cgen, request_id="c-after", text="back online")["ok"] == 1
            wait_for(lambda: "echo: back online" in _records(log_dir, csid))

            # 2. Remote Codex session (scratch workspace): 7d quota telemetry + crash + `session.resume`
            created_x = conn.call(op="session.create", kind="codex", account=codex_acct)
            assert created_x["ok"] == 1
            xsid, xgen = created_x["session"], created_x["generation"]
            assert _status_of(conn, xsid)["ssh_host"] == "gpu-box"

            assert conn.call(op="input", session=xsid, generation=xgen, request_id="x-rl", text="ratelimit:28:12")["ok"] == 1
            wait_for(lambda: "quota updated: 7d used 28%" in _records(log_dir, xsid))
            xst = wait_for(lambda: (s := _status_of(conn, xsid)).get("account_usage") and s)
            assert xst["account_usage"]["weekly_remaining_pct"] == 72

            remote_scratch = uhome / ".local" / "state" / "lico" / "workspaces" / xsid
            assert remote_scratch.is_dir() and stat.S_IMODE(remote_scratch.stat().st_mode) == 0o700

            assert conn.call(op="input", session=xsid, generation=xgen, request_id="x-boom", text="crash")["ok"] == 1
            wait_for(lambda: _status_of(conn, xsid)["state"] == "disconnected")
            res_x = conn.call(op="session.resume", session=xsid, generation=xgen)
            assert res_x["ok"] == 1
            xgen = res_x["generation"]
            assert conn.call(op="input", session=xsid, generation=xgen, request_id="x-after", text="resumed codex")["ok"] == 1
            wait_for(lambda: "echo: resumed codex" in _records(log_dir, xsid))
        finally:
            host.stop()


def test_gui_remote_files_host_switch_preview_html_notice_and_new_here_remote_session():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        ssh_cfg, mock_ssh, remote_root = _make_remote_workbench_rig(root)
        uhome = (remote_root / "gpu-box" / "user_home").resolve()
        exp_dir = (uhome / "projects" / "experiment").resolve()

        opener_log = root / "opened_locally.txt"
        opener_cmd = root / "record_open.py"
        opener_cmd.write_text(
            f"#!{sys.executable}\nimport sys\nfrom pathlib import Path\n"
            f"Path({str(opener_log)!r}).write_text('\\n'.join(sys.argv[1:]), encoding='utf-8')\n",
            encoding="utf-8",
        )
        os.chmod(opener_cmd, 0o700)

        local_dir = root / "local_work"
        local_dir.mkdir(mode=0o755, parents=True, exist_ok=True)
        (local_dir / "local.txt").write_text("local file\n", encoding="utf-8")
        made: dict[str, str] = {}

        def setup(win: ProductWindow) -> None:
            c = Conn(win.socket)
            assert c.call(op="account.create", provider="claude", label="Local")["ok"] == 1
            made["gpu_acct"] = c.call(op="account.create", provider="claude", label="GPUBox", ssh_host="gpu-box")["account"]
            assert c.call(op="settings.set", last_cwd=str(local_dir), last_view="start")["ok"] == 1

        def centre(rect: list[float]) -> tuple[float, float]:
            return (rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0

        def tool_btn(u: dict, btn_id: str) -> dict:
            return next(b for b in u["toolbar"]["buttons"] if b["id"] == btn_id)

        with ProductWindow(
            host_binary=LICO,
            host_args=["--ssh", str(mock_ssh), "--ssh-config", str(ssh_cfg)],
            env_extra={"LICO_OPEN_CMD": str(opener_cmd), "LICO_NO_AUTO_OPEN": "1"},
            before_window=setup,
        ) as w:
            conn = Conn(w.socket)
            gpu_acct = made["gpu_acct"]
            w.proc.stdin.write("load\n")
            w.proc.stdin.flush()

            # 1. Open Files (`Ctrl+E`) on local_dir, verify `Host: local v` toolbar combo-box button
            w.key("ctrl+e")
            u0 = wait_for(
                lambda: (u := w.ui())["panel"] is not None
                and u["panel"]["files"] is not None
                and all(b["rect"] for b in u["toolbar"]["buttons"])
                and u
            )
            assert u0["panel"]["files"]["ssh_host"] is None
            assert u0["panel"]["files"]["cwd"] == str(local_dir.resolve())
            host_btn = tool_btn(u0, "files_host")
            assert host_btn["label"] == "Host: local v" and host_btn["enabled"] is True
            assert u0["toolbar"]["host_popup"]["open"] is False

            # 2. Click `Host: local v` -> opens combo-box dropdown list with ZERO SSH calls (no immediate host jump)
            w.click(*centre(host_btn["rect"]))
            u_pop = wait_for(lambda: (u := w.ui())["toolbar"]["host_popup"]["open"] is True and u)
            assert u_pop["panel"]["files"]["ssh_host"] is None
            assert not (root / "ssh_calls.log").exists()
            popup_items = u_pop["toolbar"]["host_popup"]["items"]
            assert [it["label"] for it in popup_items] == [
                "* local (this machine)",
                "  ssh: gpu-box",
                "  ssh: build-srv",
                "  ssh: offline-box",
                "  ssh: hang-box",
            ]
            assert all(it["rect"] is not None for it in popup_items)

            # Escape closes combo-box cleanly without triggering any SSH connection
            w.key("Escape")
            wait_for(lambda: (u := w.ui())["toolbar"]["host_popup"]["open"] is False and u)
            assert not (root / "ssh_calls.log").exists()

            # 3. Select slow/hanging host (`hang-box`) -> GUI stays immediately responsive ("응답 없음" zero freeze),
            #    Escape cancels immediately back to local_dir, and from a removed host (`offline-box`) the Host
            #    combo-box switches directly onward to `gpu-box` without ever getting stuck
            w.key("h")
            u_pop2 = wait_for(lambda: (u := w.ui())["toolbar"]["host_popup"]["open"] is True and u)
            hang_item = next(it for it in u_pop2["toolbar"]["host_popup"]["items"] if it["host"] == "hang-box")
            w.click(*centre(hang_item["rect"]))
            wait_for(lambda: (root / "hang_started.txt").exists())

            t0 = time.monotonic()
            u_load = wait_for(
                lambda: (u := w.ui())["panel"] is not None
                and u["panel"]["files"] is not None
                and u["panel"]["files"]["mode"] == "loading"
                and u
            )
            assert time.monotonic() - t0 < 0.6
            assert u_load["panel"]["files"]["ssh_host"] == "hang-box"
            assert "Connecting to ssh:hang-box..." in (u_load["panel"]["files"]["loading"] or "")
            assert tool_btn(u_load, "new_here")["reason"] == "connecting to SSH host"

            # Escape while `hang-box` is still hanging cancels immediately and restores local_dir
            w.key("Escape")
            u_cancel = wait_for(
                lambda: (u := w.ui())["panel"] is not None
                and u["panel"]["files"] is not None
                and u["panel"]["files"]["ssh_host"] is None
                and u["panel"]["files"]["mode"] == "dir"
                and u
            )
            assert u_cancel["panel"]["files"]["cwd"] == str(local_dir.resolve())

            # Select removed host (`offline-box`) -> transitions cleanly to `unreachable`
            w.click(*centre(tool_btn(u_cancel, "files_host")["rect"]))
            u_pop3 = wait_for(lambda: (u := w.ui())["toolbar"]["host_popup"]["open"] is True and u)
            offline_item = next(it for it in u_pop3["toolbar"]["host_popup"]["items"] if it["host"] == "offline-box")
            w.click(*centre(offline_item["rect"]))
            u_unr = wait_for(
                lambda: (u := w.ui())["panel"] is not None
                and u["panel"]["files"] is not None
                and u["panel"]["files"]["mode"] == "unreachable"
                and u
            )
            assert u_unr["panel"]["files"]["unreachable"] is True
            assert u_unr["panel"]["title"] == "Files [ssh:offline-box]: unreachable"
            assert tool_btn(u_unr, "new_here")["reason"] == "SSH host is unreachable"

            # 4. Directly from `unreachable` (`Host: offline-box v`), open combo-box and advance to `gpu-box`
            w.click(*centre(tool_btn(u_unr, "files_host")["rect"]))
            u_pop4 = wait_for(lambda: (u := w.ui())["toolbar"]["host_popup"]["open"] is True and u)
            gpu_item = next(it for it in u_pop4["toolbar"]["host_popup"]["items"] if it["host"] == "gpu-box")
            w.click(*centre(gpu_item["rect"]))
            u_rem = wait_for(
                lambda: (u := w.ui())["panel"] is not None
                and u["panel"]["files"] is not None
                and u["panel"]["files"]["ssh_host"] == "gpu-box"
                and u["panel"]["files"]["mode"] == "dir"
                and u
            )
            assert u_rem["panel"]["files"]["cwd"] == str(uhome)
            assert u_rem["panel"]["title"].startswith(f"Files [ssh:gpu-box]: {uhome}")
            host_btn2 = tool_btn(u_rem, "files_host")
            assert host_btn2["label"] == "Host: gpu-box v" and host_btn2["active"] is True

            # 5. Enter `projects` -> `experiment`, preview `README.md`, then verify `.html` shows remote notice
            # Cursor 0 is `[+ New session]`, Cursor 1 is `[..]`, Cursor 2 is `projects/`
            w.key("Down")
            w.key("Down")
            w.key("Return")
            wait_for(lambda: (u := w.ui())["panel"]["files"]["cwd"] == str(uhome / "projects") and u)

            w.key("Down")
            w.key("Down")
            w.key("Return")
            u_exp = wait_for(lambda: (u := w.ui())["panel"]["files"]["cwd"] == str(exp_dir) and u)
            assert [e["name"] for e in u_exp["panel"]["files"]["entries"]] == [
                "sub",
                "escape_link",
                "README.md",
                "report.html",
            ]

            # Entries: 0=[+New], 1=[..], 2=sub/, 3=escape_link, 4=README.md, 5=report.html
            for _ in range(4):
                w.key("Down")
            w.key("Return")
            u_pv = wait_for(lambda: (u := w.ui())["panel"]["files"]["mode"] == "preview" and u)
            assert u_pv["panel"]["title"].startswith(f"File [ssh:gpu-box]: {exp_dir}/README.md")
            assert any("Remote GPU Experiment" in line for line in u_pv["panel"]["body"])

            w.key("BackSpace")
            wait_for(lambda: (u := w.ui())["panel"]["files"]["mode"] == "dir" and u)
            w.key("Down")  # move from README.md (4) to report.html (5)
            w.key("Return")
            u_html = wait_for(
                lambda: (u := w.ui())["panel"] is not None
                and any("Remote non-text file on ssh:gpu-box" in line for line in u["panel"]["body"])
                and u
            )
            assert any("report.html" in line for line in u_html["panel"]["body"])
            assert not opener_log.exists()

            # 4. Click `+ New here` (`new_here`) -> pre-selects `GPUBox [ssh: gpu-box]` and remote `exp_dir` (no Worktree!)
            w.click(*centre(tool_btn(u_html, "new_here")["rect"]))
            u_form = wait_for(
                lambda: (u := w.ui())["panel"] is not None
                and u["panel"]["form"] is not None
                and u
            )
            assert u_form["panel"]["form"]["account"] == gpu_acct
            assert u_form["panel"]["form"]["folder"] == str(exp_dir)
            assert "Worktree" not in u_form["panel"]["form"]["rows"]

            # 5. Start remote session (`Enter`), verify session header `ssh:gpu-box:...` and send prompt
            w.key("Return")
            u_sess = wait_for(lambda: (u := w.ui())["panel"] is None and u["selected"] not in ("", None) and u)
            assert f"ssh:gpu-box:{exp_dir}" in u_sess["header"], u_sess["header"]

            w.type_text("ping remote gui")
            w.key("Return")
            wait_for(lambda: any("echo: ping remote gui" in line for line in w.ui()["shown_lines"]))

            # Local `last_cwd` remains local_dir (not polluted by remote path)
            assert conn.call(op="status")["settings"]["last_cwd"] == str(local_dir.resolve())
