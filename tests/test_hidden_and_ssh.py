"""M06-P01: default-off hidden-file (`.dotfiles`) filtering + toggle across IPC, CLI, HTTP,
`settings.json`, and GUI (`Files` toolbar, `.` / `Ctrl+H`, `View` menu), plus safe `~/.ssh/config`
host discovery (`ssh.hosts`) and bounded non-interactive connectivity probing (`ssh.check`)."""

from __future__ import annotations

import json
import os
import stat
import subprocess
import urllib.parse
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn, cli, start
from lico_measure.rig import private_dir
from lico_protocol import v1
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_control_parity import _http


def _centre(rect: list[float]) -> tuple[float, float]:
    return (rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0


def _tool_btn(u: dict, btn_id: str) -> dict:
    return next(b for b in u["toolbar"]["buttons"] if b["id"] == btn_id)


def _line_centre(u: dict, line_idx: int) -> tuple[float, float]:
    y0, y1, _ = next(l for l in u["layout"]["lines"] if l[2] == line_idx)
    return 520.0, (y0 + y1) / 2.0


def _fast_key(w: ProductWindow, *keys: str) -> None:
    subprocess.run(["xdotool", "key", *keys], env=w.env, check=True)


def test_hidden_dotfiles_default_off_and_parity_across_socket_cli_http_and_settings():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        token_file = str(root / "http.token")
        work = root / "workspace"
        (work / ".git").mkdir(parents=True, exist_ok=True)
        (work / "src").mkdir(parents=True, exist_ok=True)
        (work / ".env.example").write_text("PORT=8080\n", encoding="utf-8")
        (work / "README.md").write_text("# Demo\n", encoding="utf-8")

        host, _ = start(
            directory,
            sessions=0,
            producing=0,
            extra=["--text", "--http-port", "auto", "--http-token-file", token_file],
        )
        try:
            port = host.http_port
            token = Path(token_file).read_text(encoding="utf-8").strip()
            conn = Conn(host.socket)

            # 1. Default: hidden entries are excluded across socket, POST /v1, GET /v1/files, and CLI
            req_def = {"op": "folder.list", "cwd": str(work)}
            assert v1.validate_request("folder.list", req_def) == []
            sock_def = conn.call(**req_def)
            assert v1.validate_response("folder.list", sock_def) == []
            assert sock_def["show_hidden"] is False
            assert [e["name"] for e in sock_def["entries"]] == ["src", "README.md"]

            q_cwd = urllib.parse.quote(str(work), safe="")
            st_get, get_def = _http(port, "GET", f"/v1/files?cwd={q_cwd}", token)
            assert st_get == 200 and get_def == sock_def

            cli_def = cli(host, "files", str(work))
            assert cli_def.returncode == 0 and json.loads(cli_def.stdout.strip()) == sock_def

            # 2. Explicit hidden=true: dot-prefixed dirs/files included, "." and ".." never included
            req_hid = {"op": "folder.list", "cwd": str(work), "show_hidden": True}
            assert v1.validate_request("folder.list", req_hid) == []
            sock_hid = conn.call(**req_hid)
            assert v1.validate_response("folder.list", sock_hid) == []
            assert sock_hid["show_hidden"] is True
            assert [e["name"] for e in sock_hid["entries"]] == [".git", "src", ".env.example", "README.md"]

            st_gh, get_hid = _http(port, "GET", f"/v1/files?cwd={q_cwd}&hidden=1", token)
            assert st_gh == 200 and get_hid == sock_hid

            cli_hid = cli(host, "files", str(work), "--hidden")
            assert cli_hid.returncode == 0 and json.loads(cli_hid.stdout.strip()) == sock_hid
            cli_short = cli(host, "files", "-a", str(work))
            assert cli_short.returncode == 0 and json.loads(cli_short.stdout.strip()) == sock_hid

            st_bad, _ = _http(port, "GET", f"/v1/files?cwd={q_cwd}&hidden=maybe", token)
            assert st_bad == 400

            # 3. Persistent `show_hidden` in settings across CLI, HTTP, and status
            cli_set_on = cli(host, "settings", "--show-hidden", "on")
            assert cli_set_on.returncode == 0
            st_on = conn.call(op="status")
            assert v1.validate_response("status", st_on) == []
            assert st_on["settings"].get("show_hidden") is True

            st_off_code, off_body = _http(port, "POST", "/v1/settings", token, {"show_hidden": False})
            assert st_off_code == 200 and "show_hidden" not in off_body["settings"]
        finally:
            host.stop()


def test_ssh_hosts_discovery_and_ssh_check_across_socket_cli_and_http():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        token_file = str(root / "http.token")
        ssh_dir = root / "ssh_cfg"
        conf_d = ssh_dir / "conf.d"
        conf_d.mkdir(parents=True, exist_ok=True)

        (conf_d / "01_lab.conf").write_text(
            "Host gpu-box\n  HostName 10.10.0.12\n  User trainer\n  Port 2222\n",
            encoding="utf-8",
        )
        outside = root / "outside.conf"
        outside.write_text("Host leaked-symlink\n  HostName 10.99.0.1\n", encoding="utf-8")
        os.symlink(outside, conf_d / "02_symlink.conf")

        ssh_cfg = ssh_dir / "config"
        ssh_cfg.write_text(
            "Include conf.d/*.conf\n\n"
            "Host *.wildcard !negated -flaglike\n"
            "  User nobody\n\n"
            "Host build-srv offline-box\n"
            "  HostName build.internal\n\n"
            "Host *\n"
            "  User devops\n",
            encoding="utf-8",
        )

        mock_ssh = root / "mock_ssh.sh"
        mock_ssh.write_text(
            "#!/bin/sh\n"
            "host=''\n"
            "while [ $# -gt 0 ]; do\n"
            "  if [ \"$1\" = \"--\" ]; then host=\"$2\"; shift 2; break; fi\n"
            "  shift\n"
            "done\n"
            "case \"$host\" in\n"
            "  gpu-box)\n"
            "    printf 'LICO_HOME=/remote/trainer\\nLICO_PROV=claude\\nLICO_PROV=codex\\n'\n"
            "    exit 0 ;;\n"
            "  build-srv)\n"
            "    printf 'LICO_HOME=/remote/devops\\nLICO_PROV=codex\\n'\n"
            "    exit 0 ;;\n"
            "  *)\n"
            "    echo 'ssh: connect to host offline-box port 22: No route to host' >&2\n"
            "    exit 255 ;;\n"
            "esac\n",
            encoding="utf-8",
        )
        mock_ssh.chmod(0o700)

        host, _ = start(
            directory,
            sessions=0,
            producing=0,
            extra=[
                "--text",
                "--ssh",
                str(mock_ssh),
                "--ssh-config",
                str(ssh_cfg),
                "--http-port",
                "auto",
                "--http-token-file",
                token_file,
            ],
        )
        try:
            port = host.http_port
            token = Path(token_file).read_text(encoding="utf-8").strip()
            conn = Conn(host.socket)

            cap = conn.call(op="capabilities")
            assert "ssh.hosts" in cap["ops"] and "ssh.check" in cap["ops"]
            assert "GET /v1/ssh/hosts" in cap["http_routes"] and "POST /v1/ssh/check" in cap["http_routes"]

            # 1. ssh.hosts parity
            sock_hosts = conn.call(op="ssh.hosts")
            assert v1.validate_response("ssh.hosts", sock_hosts) == []
            assert sock_hosts["hosts"] == [
                {"host": "gpu-box", "hostname": "10.10.0.12", "user": "trainer", "port": 2222},
                {"host": "build-srv", "hostname": "build.internal", "user": "devops", "port": None},
                {"host": "offline-box", "hostname": "build.internal", "user": "devops", "port": None},
            ]
            st_h, http_hosts = _http(port, "GET", "/v1/ssh/hosts", token)
            assert st_h == 200 and http_hosts == sock_hosts
            cli_hosts = cli(host, "ssh", "hosts")
            assert cli_hosts.returncode == 0 and json.loads(cli_hosts.stdout.strip()) == sock_hosts

            # 2. ssh.check parity for reachable & unreachable hosts + injection refusal
            chk_gpu = conn.call(op="ssh.check", host="gpu-box")
            assert v1.validate_response("ssh.check", chk_gpu) == []
            assert chk_gpu["reachable"] is True
            assert chk_gpu["providers"] == ["claude", "codex"]
            assert chk_gpu["home"] == "/remote/trainer"
            assert chk_gpu["error"] is None

            st_c, http_chk = _http(port, "POST", "/v1/ssh/check", token, {"host": "build-srv"})
            assert st_c == 200 and http_chk["reachable"] is True and http_chk["providers"] == ["codex"]

            cli_off = cli(host, "ssh", "check", "offline-box")
            assert cli_off.returncode == 0
            off_json = json.loads(cli_off.stdout.strip())
            assert off_json["reachable"] is False and "No route to host" in off_json["error"]

            assert conn.call(op="ssh.check", host="-oProxyCommand=id")["error"] == "bad_request"
            st_inj, _ = _http(port, "POST", "/v1/ssh/check", token, {"host": "-oProxyCommand=id"})
            assert st_inj == 400
        finally:
            host.stop()


def test_gui_hidden_files_toggle_via_toolbar_key_and_menu_persists():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = root / "repo"
        (proj / ".git").mkdir(parents=True, exist_ok=True)
        (proj / "src").mkdir(parents=True, exist_ok=True)
        (proj / ".hidden_note.md").write_text("# Hidden Guide\nDotfile preview works.\n", encoding="utf-8")
        (proj / "main.rs").write_text("fn main() {}\n", encoding="utf-8")

        def setup(w: ProductWindow) -> None:
            conn = Conn(w.socket)
            assert conn.call(op="settings.set", last_cwd=str(proj), last_view="files")["ok"] == 1

        with ProductWindow(
            host_binary=LICO,
            env_extra={"LICO_NO_AUTO_OPEN": "1"},
            before_window=setup,
        ) as w:
            conn = Conn(w.socket)
            w.proc.stdin.write("load\n")
            w.proc.stdin.flush()

            # 1. Restores into `Files` with hidden files OFF by default
            u0 = wait_for(
                lambda: (u := w.ui())["panel"] is not None
                and u["panel"]["files"] is not None
                and all(b["rect"] for b in u["toolbar"]["buttons"])
                and u
            )
            assert u0["panel"]["files"]["show_hidden"] is False
            assert [e["name"] for e in u0["panel"]["files"]["entries"]] == ["src", "main.rs"]
            btn0 = _tool_btn(u0, "toggle_hidden")
            assert btn0["label"] == "Hidden: off" and btn0["active"] is False

            # 2. Click toolbar `Hidden: off` -> turns ON, reveals `.git` and `.hidden_note.md`, persists in settings
            w.click(*_centre(btn0["rect"]))
            u_on = wait_for(lambda: (u := w.ui())["panel"]["files"]["show_hidden"] is True and u)
            assert [e["name"] for e in u_on["panel"]["files"]["entries"]] == [".git", "src", ".hidden_note.md", "main.rs"]
            assert _tool_btn(u_on, "toggle_hidden")["label"] == "Hidden: on"
            assert _tool_btn(u_on, "toggle_hidden")["active"] is True
            assert conn.call(op="status")["settings"].get("show_hidden") is True

            # 3. Press `.` key -> toggles OFF cleanly
            _fast_key(w, "period")
            u_off = wait_for(lambda: (u := w.ui())["panel"]["files"]["show_hidden"] is False and u)
            assert [e["name"] for e in u_off["panel"]["files"]["entries"]] == ["src", "main.rs"]
            assert conn.call(op="status")["settings"].get("show_hidden") is None

            # 4. Toggle back ON via `View > Hidden files: hidden` menu item and preview `.hidden_note.md`
            w.click(*_centre(u_off["menu"]["titles"][2]["rect"]))  # View menu (index 2)
            u_menu = wait_for(
                lambda: (u := w.ui())["menu"]["open"] == 2
                and len(u["menu"]["items"]) >= 2
                and all(b["rect"] for b in u["menu"]["items"])
                and u
            )
            hid_item = next(it for it in u_menu["menu"]["items"] if it["label"].startswith("Hidden files:"))
            assert hid_item["enabled"] is True
            w.click(*_centre(hid_item["rect"]))
            u_on2 = wait_for(lambda: (u := w.ui())["panel"]["files"]["show_hidden"] is True and u)

            # Row 0: `+ New`, Row 1: `..`, Row 2: `.git/`, Row 3: `src/`, Row 4: `.hidden_note.md`
            w.click(*_line_centre(u_on2, 4))
            u_prev = wait_for(lambda: (u := w.ui())["panel"]["files"]["mode"] == "preview" and u)
            assert u_prev["panel"]["files"]["preview"]["name"] == ".hidden_note.md"
            assert any("Dotfile preview works." in line for line in u_prev["panel"]["body"])
