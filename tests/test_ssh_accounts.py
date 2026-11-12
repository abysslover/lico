"""M06-P02: SSH remote provider accounts (`[ssh: <host>]`) across IPC, CLI, loopback HTTP,
and `lico gui`, plus supervised remote `Sign in` (`claude auth login`, `codex login`,
`codex login status`) over `BatchMode=yes` SSH inside isolated `0700` remote account homes."""

from __future__ import annotations

import json
import os
import stat
import subprocess
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn, cli, start
from lico_measure.rig import files_under, private_dir
from lico_protocol import v1
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_control_parity import _http

REMOTE_URL = "https://auth.example.invalid/remote?state=REMOTESTATE5"


def _make_remote_ssh_rig(root: Path) -> tuple[Path, Path, Path]:
    """Creates a synthetic ssh_config (`gpu-box`, `build-srv`) and a mock `ssh` executable that
    executes the remote command inside `remote_root/<host>/user_home` with user-local `~/.local/bin/{claude,codex}`."""
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
        "  User builder\n",
        encoding="utf-8",
    )
    os.chmod(ssh_cfg, 0o600)

    remote_root = root / "remote_hosts"
    for alias in ("gpu-box", "build-srv"):
        uhome = remote_root / alias / "user_home"
        bin_dir = uhome / ".local" / "bin"
        bin_dir.mkdir(mode=0o700, parents=True, exist_ok=True)

        claude_sim = bin_dir / "claude"
        claude_sim.write_text(
            "#!/usr/bin/env python3\n"
            "import json, os, sys\n"
            "from pathlib import Path\n"
            "args = sys.argv[1:]\n"
            "if args == ['auth', 'login']:\n"
            "    sys.stdout.write(f'Open {os.environ.get(\"TEST_URL\", \"" + REMOTE_URL + "\")}\\n')\n"
            "    sys.stdout.flush()\n"
            "    sys.stdout.write('Paste code here if prompted > ')\n"
            "    sys.stdout.flush()\n"
            "    code = sys.stdin.readline().strip()\n"
            "    cfg = Path(os.environ['CLAUDE_CONFIG_DIR'])\n"
            "    home = Path(os.environ['HOME'])\n"
            "    (cfg / 'remote_auth.json').write_text(json.dumps({'ok': True, 'args': args, 'cwd': os.getcwd(), 'home': str(home), 'env_keys': sorted(os.environ.keys())}))\n"
            "    (home / 'received_code').write_text(code)\n"
            "    sys.exit(0 if code else 1)\n"
            "sys.exit(2)\n",
            encoding="utf-8",
        )
        os.chmod(claude_sim, 0o700)

        codex_sim = bin_dir / "codex"
        codex_sim.write_text(
            "#!/usr/bin/env python3\n"
            "import json, os, sys\n"
            "from pathlib import Path\n"
            "args = sys.argv[1:]\n"
            "cfg = Path(os.environ['CODEX_HOME'])\n"
            "home = Path(os.environ['HOME'])\n"
            "if args in (['login'], ['login', '--device-auth']):\n"
            "    sys.stdout.write(f'Visit " + REMOTE_URL + "\\nEnter the code: ')\n"
            "    sys.stdout.flush()\n"
            "    code = sys.stdin.readline().strip()\n"
            "    (cfg / 'auth.json').write_text(json.dumps({'mode': args}))\n"
            "    (home / 'received_code').write_text(code)\n"
            "    sys.exit(0 if code else 1)\n"
            "if args == ['login', 'status']:\n"
            "    sys.exit(0 if (cfg / 'auth.json').exists() else 1)\n"
            "sys.exit(2)\n",
            encoding="utf-8",
        )
        os.chmod(codex_sim, 0o700)

    mock_ssh = root / "mock_ssh.py"
    mock_ssh.write_text(
        "#!/usr/bin/env python3\n"
        "import os, subprocess, sys\n"
        "from pathlib import Path\n"
        "args = sys.argv[1:]\n"
        "assert '-o' in args and 'BatchMode=yes' in args, args\n"
        "sep = args.index('--')\n"
        "host = args[sep + 1]\n"
        "remote_cmd = args[sep + 2]\n"
        f"rroot = Path({str(remote_root)!r})\n"
        "uhome = rroot / host / 'user_home'\n"
        "if not uhome.is_dir():\n"
        "    sys.stderr.write(f'ssh: Could not resolve hostname {host}\\n')\n"
        "    sys.exit(255)\n"
        "env = {'HOME': str(uhome), 'PATH': '/usr/bin:/bin', 'LANG': 'C.UTF-8'}\n"
        "res = subprocess.run(['/bin/sh', '-c', remote_cmd], env=env, cwd=str(uhome))\n"
        "sys.exit(res.returncode)\n",
        encoding="utf-8",
    )
    os.chmod(mock_ssh, 0o700)
    return ssh_cfg, mock_ssh, remote_root


def test_ssh_account_parity_and_remote_login_isolates_0700_home_and_hides_code():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        token_file = str(root / "http.token")
        ssh_cfg, mock_ssh, remote_root = _make_remote_ssh_rig(root)

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

            # 1. Local + SSH remote account creation across socket, CLI, and HTTP
            local_rep = conn.call(op="account.create", provider="claude", label="LocalWork")
            assert local_rep["ok"] == 1 and local_rep["ssh_host"] is None
            assert v1.validate_response("account.create", local_rep) == []

            cli_out = json.loads(
                cli(host, "account", "create", "--provider", "claude", "--label", "Cluster", "--ssh-host", "gpu-box").stdout
            )
            assert cli_out["ok"] == 1 and cli_out["ssh_host"] == "gpu-box"
            assert v1.validate_response("account.create", cli_out) == []
            claude_ssh_id = cli_out["account"]

            st_http, http_out = _http(
                port, "POST", "/v1/accounts", token, {"provider": "codex", "label": "BuildBot", "ssh_host": "build-srv"}
            )
            assert st_http == 200 and http_out["ssh_host"] == "build-srv"
            assert v1.validate_response("account.create", http_out) == []
            codex_ssh_id = http_out["account"]

            # Reject option-injection and empty ssh_host
            assert conn.call(op="account.create", provider="claude", label="Bad", ssh_host="-oProxyCommand=id")["error"] == "bad_request"
            assert conn.call(op="account.create", provider="claude", label="Bad2", ssh_host="")["error"] == "bad_request"
            st_bad, _ = _http(port, "POST", "/v1/accounts", token, {"provider": "claude", "label": "Bad3", "ssh_host": "-V"})
            assert st_bad == 400

            # Rename preserves ssh_host; account.list conforms to v1
            ren = conn.call(op="account.rename", account=claude_ssh_id, label="GPU Cluster")
            assert ren["ok"] == 1 and ren["ssh_host"] == "gpu-box" and v1.validate_response("account.rename", ren) == []
            listed = conn.call(op="account.list")
            assert v1.validate_response("account.list", listed) == []
            by_id = {a["account"]: a for a in listed["accounts"]}
            assert by_id[local_rep["account"]]["ssh_host"] is None
            assert by_id[claude_ssh_id]["ssh_host"] == "gpu-box" and by_id[claude_ssh_id]["label"] == "GPU Cluster"
            assert by_id[codex_ssh_id]["ssh_host"] == "build-srv"

            # 2. Supervised remote Sign in for Claude on `gpu-box`
            assert conn.call(op="account.login.start", account=claude_ssh_id, method="browser")["ok"] == 1
            st_claude = wait_for(
                lambda: (s := conn.call(op="account.login.status", account=claude_ssh_id)).get("needs_code") and s
            )
            assert st_claude["url"] == REMOTE_URL and st_claude["state"] == "code"
            assert conn.call(op="account.login.input", account=claude_ssh_id, code="REMOTECANARY42")["ok"] == 1
            wait_for(lambda: conn.call(op="account.login.status", account=claude_ssh_id)["state"] == "succeeded")

            remote_acct_dir = remote_root / "gpu-box" / "user_home" / ".local" / "state" / "lico" / "accounts" / claude_ssh_id
            assert stat.S_IMODE(remote_acct_dir.stat().st_mode) == 0o700
            assert stat.S_IMODE((remote_acct_dir / "home").stat().st_mode) == 0o700
            assert stat.S_IMODE((remote_acct_dir / "claude").stat().st_mode) == 0o700
            assert (remote_acct_dir / "home" / "received_code").read_text(encoding="utf-8") == "REMOTECANARY42"
            meta = json.loads((remote_acct_dir / "claude" / "remote_auth.json").read_text(encoding="utf-8"))
            assert meta["cwd"] == str(remote_acct_dir / "home")
            assert "DISPLAY" not in meta["env_keys"]

            # Local account folder holds no remote auth token
            assert not list((root / "accounts" / claude_ssh_id).rglob("*auth*.json"))
            for path, data in files_under(root / "log", root / "registry", root / "host.err"):
                assert b"REMOTECANARY42" not in data and b"REMOTESTATE5" not in data, path

            # 3. Remote Codex `signed_in` check before and after remote login on `build-srv`
            assert conn.call(op="account.login.status", account=codex_ssh_id, check=True)["signed_in"] is False
            done = subprocess.run(
                [str(LICO), "--socket", host.socket, "account", "login", codex_ssh_id],
                input="REMOTECODEX88\n",
                capture_output=True,
                text=True,
                timeout=20,
            )
            assert done.returncode == 0 and "signed in" in done.stderr and "REMOTECODEX88" not in done.stdout + done.stderr
            assert conn.call(op="account.login.status", account=codex_ssh_id, check=True)["signed_in"] is True
        finally:
            host.stop()


def test_gui_ssh_account_creation_badges_and_remote_sign_in():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        ssh_cfg, mock_ssh, remote_root = _make_remote_ssh_rig(root)

        with ProductWindow(
            host_binary=LICO,
            host_args=["--ssh", str(mock_ssh), "--ssh-config", str(ssh_cfg)],
            env_extra={"LICO_NO_AUTO_OPEN": "1"},
        ) as w:
            conn = Conn(w.socket)
            # Add a local account first so we see both local and SSH remote badges side by side
            assert conn.call(op="account.create", provider="claude", label="Local")["ok"] == 1

            # Open Accounts (`Ctrl+A`) -> press `a` -> switch Target to `gpu-box` via `Down` -> type `Cluster` -> Enter
            w.key("ctrl+a")
            u_accts = wait_for(lambda: (u := w.ui())["panel"] is not None and u["panel"]["title"] == "Accounts" and u)
            assert any("A  add a new account" in line for line in u_accts["panel"]["body"])

            w.key("a")
            u_lbl0 = wait_for(lambda: (u := w.ui())["panel"]["title"] == "New claude account" and u["panel"]["text_entry"] and u)
            assert any("local (this machine)" in line for line in u_lbl0["panel"]["body"])

            w.key("Down")
            u_lbl1 = wait_for(lambda: (u := w.ui())["panel"]["title"] == "New claude account [ssh: gpu-box]" and u)
            assert any("ssh: gpu-box" in line for line in u_lbl1["panel"]["body"])

            w.type_text("Cluster")
            w.key("Return")
            u_act = wait_for(lambda: (u := w.ui())["panel"]["title"] == "Account: claude / Cluster [ssh: gpu-box]" and u)
            assert any("host: ssh:gpu-box" in line for line in u_act["panel"]["body"])

            # Press `s` to sign in over SSH, verify masked code entry and clean completion
            w.key("s")
            u_login = wait_for(
                lambda: (u := w.ui())["panel"]["title"] == "Sign in: claude / Cluster [ssh: gpu-box]"
                and REMOTE_URL in u["panel"]["body"]
                and u["panel"]["text_entry"]
                and u
            )
            assert any("host: ssh:gpu-box" in line for line in u_login["panel"]["body"])

            w.type_text("GUICANARY77")
            u_masked = wait_for(lambda: (u := w.ui())["panel"]["input_len"] == 11 and u)
            assert u_masked["panel"]["input"] == "*" * 11 and "GUICANARY77" not in json.dumps(u_masked)

            w.key("Return")
            u_done = wait_for(
                lambda: (u := w.ui())["panel"]["title"] == "Account: claude / Cluster [ssh: gpu-box]"
                and "Signed in." in u["panel"]["body"]
                and u
            )
            assert "GUICANARY77" not in json.dumps(u_done)

            ssh_acct = next(a["account"] for a in conn.call(op="account.list")["accounts"] if a["ssh_host"] == "gpu-box")
            assert (
                remote_root / "gpu-box" / "user_home" / ".local" / "state" / "lico" / "accounts" / ssh_acct / "home" / "received_code"
            ).read_text(encoding="utf-8") == "GUICANARY77"

            # Check `Accounts` list and `New session` (`Ctrl+N`) both show `[ssh: gpu-box]`
            w.key("Escape")
            u_list = wait_for(lambda: (u := w.ui())["panel"]["title"] == "Accounts" and u)
            assert any("claude / Cluster [ssh: gpu-box]" in line for line in u_list["panel"]["body"])

            w.key("Escape")
            wait_for(lambda: w.ui()["panel"] is None)
            w.key("ctrl+n")
            u_form = wait_for(lambda: (u := w.ui())["panel"] is not None and u["panel"]["form"] is not None and u)
            w.key("Up")
            w.key("Right")
            u_form2 = wait_for(lambda: (u := w.ui())["panel"]["form"]["account"] == ssh_acct and u)
            assert any("< Cluster [ssh: gpu-box] >" in line for line in u_form2["panel"]["body"])
