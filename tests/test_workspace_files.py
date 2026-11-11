"""M04-P04: on-demand working-directory file viewer (`folder.list`, `folder.read`, `lico files`,
`GET /v1/files*`, GUI `Files` panel, explicit `F5` / `Refresh` with zero background polling,
external `.html` launch via `LICO_OPEN_CMD` / `xdg-open`, and `+ New here` session creation)."""

from __future__ import annotations

import json
import os
import stat
import subprocess
import time
import urllib.parse
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn, cli, start
from lico_measure.rig import private_dir
from lico_protocol import v1
from tests.account_helpers import account
from tests.test_claude_adapter import status_of
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_control_parity import _http
from tests.test_new_session_screen import claude_args


def _centre(rect: list[float]) -> tuple[float, float]:
    return (rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0


def _tool_btn(u: dict, btn_id: str) -> dict:
    return next(b for b in u["toolbar"]["buttons"] if b["id"] == btn_id)


def _line_centre(u: dict, line_idx: int) -> tuple[float, float]:
    y0, y1, _ = next(l for l in u["layout"]["lines"] if l[2] == line_idx)
    return 520.0, (y0 + y1) / 2.0


def _fast_key(w: ProductWindow, *keys: str) -> None:
    subprocess.run(["xdotool", "key", *keys], env=w.env, check=True)


def test_folder_list_and_read_parity_and_security_across_socket_cli_and_http():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        token_file = str(root / "http.token")
        work = root / "workspace"
        (work / "src").mkdir(parents=True, exist_ok=True)
        (work / "index.html").write_text("<!doctype html><title>Demo</title>\n", encoding="utf-8")
        (work / "notes.md").write_text("alpha\r\nbeta\t한글\n", encoding="utf-8")
        (work / "binary.bin").write_bytes(b"\x00\x01\x02\x03")
        os.symlink(work / "notes.md", work / "link.md")

        host, log_dir = start(
            directory,
            sessions=0,
            producing=0,
            extra=["--text", "--http-port", "auto", "--http-token-file", token_file],
        )
        try:
            port = host.http_port
            token = Path(token_file).read_text(encoding="utf-8").strip()
            conn = Conn(host.socket)

            cap = conn.call(op="capabilities")
            assert "folder.list" in cap["ops"] and "folder.read" in cap["ops"]
            assert "GET /v1/files" in cap["http_routes"] and "GET /v1/files/read" in cap["http_routes"]

            # 1. folder.list parity across socket, POST /v1, GET /v1/files, and CLI
            req_list = {"op": "folder.list", "cwd": str(work)}
            assert v1.validate_request("folder.list", req_list) == []
            sock_list = conn.call(**req_list)
            assert v1.validate_response("folder.list", sock_list) == []
            assert [e["name"] for e in sock_list["entries"]] == ["src", "binary.bin", "index.html", "link.md", "notes.md"]
            by_name = {e["name"]: e for e in sock_list["entries"]}
            assert by_name["src"]["kind"] == "dir"
            assert by_name["index.html"]["is_html"] is True
            assert by_name["notes.md"]["is_html"] is False
            assert by_name["link.md"]["kind"] == "link"

            st_post, post_list = _http(port, "POST", "/v1", token, req_list)
            assert st_post == 200 and post_list == sock_list

            q_cwd = urllib.parse.quote(str(work), safe="")
            st_get, get_list = _http(port, "GET", f"/v1/files?cwd={q_cwd}", token)
            assert st_get == 200 and get_list == sock_list

            done_cli = cli(host, "files", str(work))
            assert done_cli.returncode == 0 and json.loads(done_cli.stdout.strip()) == sock_list

            # 2. folder.read parity (UTF-8 normalization + binary detection)
            notes_path = str(work / "notes.md")
            req_read = {"op": "folder.read", "path": notes_path}
            assert v1.validate_request("folder.read", req_read) == []
            sock_read = conn.call(**req_read)
            assert v1.validate_response("folder.read", sock_read) == []
            assert sock_read["binary"] is False and sock_read["truncated"] is False
            assert sock_read["text"] == "alpha\nbeta\t한글\n"

            q_path = urllib.parse.quote(notes_path, safe="")
            st_read, get_read = _http(port, "GET", f"/v1/files/read?path={q_path}", token)
            assert st_read == 200 and get_read == sock_read

            done_rcli = cli(host, "files", "--read", notes_path)
            assert done_rcli.returncode == 0 and json.loads(done_rcli.stdout.strip()) == sock_read

            bin_read = conn.call(op="folder.read", path=str(work / "binary.bin"))
            assert v1.validate_response("folder.read", bin_read) == []
            assert bin_read["binary"] is True and bin_read["text"] == ""

            # 3. Security refusals: protected host state dir, symlinks, missing files, relative paths
            secret = Path(log_dir) / "probe.txt"
            secret.write_text("nope", encoding="utf-8")
            assert conn.call(op="folder.list", cwd=str(log_dir))["error"] == "folder_protected"
            assert conn.call(op="folder.read", path=str(secret))["error"] == "folder_protected"
            assert conn.call(op="folder.read", path=str(work / "link.md"))["error"] == "file_not_regular"
            assert conn.call(op="folder.read", path="relative.txt")["error"] == "folder_not_absolute"
            st_miss, body_miss = _http(
                port,
                "GET",
                f"/v1/files/read?path={urllib.parse.quote(str(work / 'missing.md'), safe='')}",
                token,
            )
            assert st_miss == 404 and body_miss["error"] == "file_missing"
        finally:
            host.stop()


def test_gui_files_viewer_on_demand_f5_refresh_html_open_and_new_session_here():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = root / "alpha"
        sub = proj / "sub"
        sub.mkdir(parents=True, exist_ok=True)
        (proj / "initial.txt").write_text("first file\n", encoding="utf-8")
        (sub / "readme.md").write_text("# Subfolder Guide\nLine two of preview.\n", encoding="utf-8")
        (sub / "report.html").write_text("<!doctype html><h1>Inspection OK</h1>\n", encoding="utf-8")

        opened_log = root / "opened_targets.log"
        opener = root / "record_open.sh"
        opener.write_text(f"#!/bin/sh\nprintf '%s\\n' \"$1\" >> '{opened_log}'\n", encoding="utf-8")
        opener.chmod(opener.stat().st_mode | stat.S_IXUSR)

        log_dir = root / "logs"
        args, _received = claude_args(root)
        made: dict[str, str] = {}

        def setup(w: ProductWindow) -> None:
            conn = Conn(w.socket)
            acc = account(conn, "Work", "claude")
            made["acc"] = acc
            res = conn.call(op="session.create", kind="claude", account=acc, mode="auto", cwd=str(proj), name="alpha-main")
            assert res["ok"] == 1, res
            made["sid"] = res["session"]

        with ProductWindow(
            host_binary=LICO,
            host_args=[*args, "--log-dir", str(log_dir)],
            env_extra={"LICO_OPEN_CMD": str(opener), "LICO_NO_AUTO_OPEN": "1"},
            before_window=setup,
        ) as w:
            conn = Conn(w.socket)
            sid = made["sid"]
            w.proc.stdin.write("load\n")
            w.proc.stdin.flush()

            # Select session sid in left sidebar
            u0 = wait_for(lambda: (u := w.ui()) and any(r[2] == sid for r in u["layout"]["rows"]) and u)
            y0, y1, _ = next(r for r in u0["layout"]["rows"] if r[2] == sid)
            w.click(100.0, (y0 + y1) / 2.0)
            u_sel = wait_for(lambda: (u := w.ui())["selected"] == sid and u)

            # 1. Click Toolbar 'Files' -> opens Files viewer in proj, exposes '+ New here' & 'Refresh'
            w.click(*_centre(_tool_btn(u_sel, "files")["rect"]))
            u_files = wait_for(
                lambda: (u := w.ui())["panel"] is not None
                and u["panel"]["files"] is not None
                and all(b["rect"] for b in u["toolbar"]["buttons"])
                and u
            )
            f0 = u_files["panel"]["files"]
            assert f0["cwd"] == str(proj.resolve())
            assert f0["load_count"] == 1 and f0["refresh_count"] == 0
            assert [e["name"] for e in f0["entries"]] == ["sub", "initial.txt"]
            assert [b["id"] for b in u_files["toolbar"]["buttons"]] == [
                "new_session",
                "accounts",
                "files",
                "new_here",
                "refresh_files",
                "toggle_group",
            ]
            assert _tool_btn(u_files, "files")["active"] is True

            # 2. Create a new file on disk -> NO background inotify/timer refresh occurs!
            (proj / "added_on_disk.txt").write_text("created later\n", encoding="utf-8")
            time.sleep(1.3)
            f_quiet = w.ui()["panel"]["files"]
            assert f_quiet["load_count"] == 1 and f_quiet["refresh_count"] == 0
            assert [e["name"] for e in f_quiet["entries"]] == ["sub", "initial.txt"]

            # 3. Explicit F5 refreshes immediately; toolbar 'Refresh' click refreshes again
            _fast_key(w, "F5")
            u_f5 = wait_for(lambda: (u := w.ui())["panel"]["files"]["refresh_count"] == 1 and u)
            assert u_f5["panel"]["files"]["load_count"] == 2
            assert [e["name"] for e in u_f5["panel"]["files"]["entries"]] == ["sub", "added_on_disk.txt", "initial.txt"]
            assert any("Refreshed." in line for line in u_f5["panel"]["body"])

            (proj / "added_via_toolbar.txt").write_text("second refresh\n", encoding="utf-8")
            w.click(*_centre(_tool_btn(u_f5, "refresh_files")["rect"]))
            u_tb_ref = wait_for(lambda: (u := w.ui())["panel"]["files"]["refresh_count"] == 2 and u)
            assert "added_via_toolbar.txt" in [e["name"] for e in u_tb_ref["panel"]["files"]["entries"]]

            # 4. Click '[dir]   sub/' (line 2: line 0 is '+ New session', line 1 is '..') -> enters sub/
            sub_idx = next(i for i, line in enumerate(u_tb_ref["panel"]["body"]) if "[dir]   sub/" in line)
            w.click(*_line_centre(u_tb_ref, sub_idx))
            u_sub = wait_for(
                lambda: (u := w.ui())["panel"]["files"]["cwd"] == str(sub.resolve())
                and len(u["layout"]["lines"]) >= 4
                and u
            )
            assert [e["name"] for e in u_sub["panel"]["files"]["entries"]] == ["readme.md", "report.html"]

            # 5. Click 'readme.md' -> in-window text preview; Escape returns to sub/ directory listing
            md_idx = next(i for i, line in enumerate(u_sub["panel"]["body"]) if "readme.md" in line)
            w.click(*_line_centre(u_sub, md_idx))
            u_prev = wait_for(lambda: (u := w.ui())["panel"]["files"]["mode"] == "preview" and u)
            assert any("# Subfolder Guide" in line for line in u_prev["panel"]["body"])
            _fast_key(w, "Escape")
            u_back = wait_for(lambda: (u := w.ui())["panel"]["files"]["mode"] == "dir" and u)
            assert u_back["panel"]["files"]["cwd"] == str(sub.resolve())

            # 6. Click 'report.html' -> invokes LICO_OPEN_CMD external browser opener; 'v' previews HTML source
            html_idx = next(i for i, line in enumerate(u_back["panel"]["body"]) if "report.html" in line)
            w.click(*_line_centre(u_back, html_idx))
            expected_html = str((sub / "report.html").resolve())
            wait_for(lambda: opened_log.exists() and expected_html in opened_log.read_text(encoding="utf-8"))
            u_opened = wait_for(lambda: (u := w.ui())["panel"]["files"]["last_opened"] == expected_html and u)
            assert any("Opened in browser: report.html" in line for line in u_opened["panel"]["body"])

            _fast_key(w, "v")
            u_html_src = wait_for(lambda: (u := w.ui())["panel"]["files"]["mode"] == "preview" and u)
            assert any("<h1>Inspection OK</h1>" in line for line in u_html_src["panel"]["body"])
            # Click line 0 ('[<- Back to directory]') to step back to directory listing
            w.click(*_line_centre(u_html_src, 0))
            u_dir_again = wait_for(lambda: (u := w.ui())["panel"]["files"]["mode"] == "dir" and u)

            # 7. Click Toolbar '+ New here' -> opens New session pre-filled with sub/ and launches session there!
            w.click(*_centre(_tool_btn(u_dir_again, "new_here")["rect"]))
            u_form = wait_for(
                lambda: (u := w.ui())["panel"] is not None
                and u["panel"]["title"] == "New session"
                and u["panel"]["form"] is not None
                and u
            )
            assert u_form["panel"]["form"]["folder"] == str(sub.resolve())
            _fast_key(w, "Return")
            u_created = wait_for(
                lambda: (u := w.ui())["panel"] is None
                and u["selected"] not in ("", sid)
                and u
            )
            new_sid = u_created["selected"]
            assert status_of(conn, new_sid)["cwd"] == str(sub.resolve())
            assert u_created["header"].endswith("/alpha/sub"), u_created["header"]
