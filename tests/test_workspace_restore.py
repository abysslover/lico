"""M05-P01: persist and restore previous working directory and workspace view across `lico` restarts."""

from __future__ import annotations

import json
import queue
import shutil
import subprocess
import threading
import time
from pathlib import Path

from lico_measure.p02 import LICO, ProtoConn as Conn, cli, start
from lico_measure.rig import Xvfb, private_dir, short_base
from lico_protocol import v1
from tests.account_helpers import account
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_control_parity import _http
from tests.test_new_session_screen import claude_args


class _GuiInstance:
    """Lightweight `lico gui` process wrapper against an existing socket and Xvfb display."""

    def __init__(self, env: dict[str, str]) -> None:
        self.env = env
        self.proc = subprocess.Popen(
            [str(LICO), "gui"],
            env=env,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
        )
        self.events: queue.Queue[dict] = queue.Queue()
        threading.Thread(target=self._read, daemon=True).start()
        end = time.monotonic() + 15.0
        while time.monotonic() < end and self.proc.poll() is None:
            try:
                if self.events.get(timeout=0.3).get("ev") == "ready":
                    break
            except queue.Empty:
                pass
        from lico_measure.ime import _focus

        wait_for(lambda: self.ui()["frame"]["frames"] >= 1)
        _focus(env["DISPLAY"])

    def _read(self) -> None:
        assert self.proc.stdout is not None
        for line in self.proc.stdout:
            try:
                self.events.put(json.loads(line))
            except ValueError:
                pass

    def ui(self) -> dict:
        assert self.proc.stdin is not None
        self.proc.stdin.write("ui\n")
        self.proc.stdin.flush()
        end = time.monotonic() + 5.0
        while time.monotonic() < end:
            try:
                ev = self.events.get(timeout=0.5)
                if ev.get("ev") == "ui":
                    return ev
            except queue.Empty:
                pass
        raise TimeoutError("timed out waiting for ui dump")

    def key(self, *keys: str) -> None:
        subprocess.run(["xdotool", "key", *keys], env=self.env, check=True)

    def click(self, x: float, y: float) -> None:
        wid_out = subprocess.check_output(["xdotool", "search", "--onlyvisible", "--name", "."], env=self.env, text=True).split()
        wid = wid_out[-1]
        subprocess.run(["xdotool", "mousemove", "--window", wid, str(int(x)), str(int(y)), "click", "1"], env=self.env, check=True)

    def close(self) -> None:
        if self.proc.poll() is None and self.proc.stdin is not None:
            try:
                self.proc.stdin.write("quit\n")
                self.proc.stdin.flush()
                self.proc.wait(timeout=5.0)
            except (OSError, subprocess.TimeoutExpired):
                self.proc.kill()
                self.proc.wait(timeout=5.0)


def test_settings_workspace_persistence_and_security_across_socket_cli_and_http():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        token_file = str(root / "http.token")
        proj_a = root / "proj_a"
        proj_b = root / "proj_b"
        proj_a.mkdir(parents=True)
        proj_b.mkdir(parents=True)

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

            st0 = conn.call(op="status")
            assert v1.validate_response("status", st0) == []
            assert st0["settings"].get("last_view", "start") == "start"
            assert "last_cwd" not in st0["settings"]

            req = {"op": "settings.set", "last_cwd": str(proj_a), "last_view": "files", "last_session": "s1234abcd"}
            assert v1.validate_request("settings.set", req) == []
            rep = conn.call(**req)
            assert v1.validate_response("settings.set", rep) == []
            assert rep["settings"]["last_cwd"] == str(proj_a.resolve())
            assert rep["settings"]["last_view"] == "files"
            assert rep["settings"]["last_session"] == "s1234abcd"

            # Syncs recent_folders.json + last.cwd in session.options too.
            opts = conn.call(op="session.options")
            assert opts["last"]["cwd"] == str(proj_a.resolve())
            assert opts["recent"][0] == str(proj_a.resolve())

            # HTTP parity
            code, http_rep = _http(port, "POST", "/v1/settings", token, {"last_cwd": str(proj_b), "last_view": "session"})
            assert code == 200 and http_rep["settings"]["last_cwd"] == str(proj_b.resolve())
            assert http_rep["settings"]["last_view"] == "session"

            # CLI parity
            r_cli = cli(host, "settings", "--last-cwd", str(proj_a), "--last-view", "files", "--last-session", "")
            assert r_cli.returncode == 0
            parsed = json.loads(r_cli.stdout.strip())
            assert parsed["settings"]["last_cwd"] == str(proj_a.resolve())
            assert "last_session" not in parsed["settings"]

            # Security refusals
            assert conn.call(op="settings.set", last_cwd=str(log_dir))["error"] == "folder_protected"
            assert conn.call(op="settings.set", last_cwd=str(root / "missing"))["error"] == "folder_missing"
            assert conn.call(op="settings.set", last_cwd="relative/dir")["error"] == "folder_not_absolute"

            # Deleting the remembered directory omits it cleanly on readback.
            shutil.rmtree(proj_a)
            st_after = conn.call(op="status")
            assert v1.validate_response("status", st_after) == []
            assert "last_cwd" not in st_after["settings"]
        finally:
            host.stop()


def test_gui_restores_browsed_working_directory_across_window_and_host_restart():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = root / "workspace_alpha"
        nested = proj / "packages" / "core"
        nested.mkdir(parents=True)
        (nested / "lib.rs").write_text("pub fn answer() -> u32 { 42 }\n", encoding="utf-8")

        rt = short_base("rt") / root.name[-6:]
        (rt / "lico").mkdir(mode=0o700, parents=True, exist_ok=True)
        state_dir = root / "state" / "lico"
        home_dir = root / "home"
        state_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
        home_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
        sock = str(rt / "lico" / "host.sock")

        xvfb = Xvfb()
        env = {
            "PATH": "/usr/bin:/bin",
            "LANG": "C.UTF-8",
            "HOME": str(home_dir),
            "XDG_RUNTIME_DIR": str(rt),
            "XDG_STATE_HOME": str(root / "state"),
            "DISPLAY": xvfb.display,
            "LICO_NO_AUTO_OPEN": "1",
        }
        host1 = subprocess.Popen(
            [str(LICO), "host", "--socket", sock, "--state-dir", str(state_dir)],
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
        )
        host2 = None
        try:
            assert host1.stdout is not None
            json.loads(host1.stdout.readline())
            # Seed initial recent folder via settings so Ctrl+E opens proj first, then navigate into packages/core.
            Conn(sock).call(op="settings.set", last_cwd=str(proj), last_view="start")

            g1 = _GuiInstance(env)
            try:
                assert g1.ui()["panel"] is None
                g1.key("ctrl+e")
                u_proj = wait_for(lambda: (u := g1.ui())["panel"] is not None and u["panel"]["files"]["cwd"] == str(proj.resolve()) and u)
                pkg_idx = next(i for i, line in enumerate(u_proj["panel"]["body"]) if "[dir]   packages/" in line)
                y0, y1, _ = next(l for l in u_proj["layout"]["lines"] if l[2] == pkg_idx)
                g1.click(520.0, (y0 + y1) / 2.0)
                u_pkg = wait_for(lambda: (u := g1.ui())["panel"]["files"]["cwd"] == str((proj / "packages").resolve()) and u)
                core_idx = next(i for i, line in enumerate(u_pkg["panel"]["body"]) if "[dir]   core/" in line)
                y0, y1, _ = next(l for l in u_pkg["layout"]["lines"] if l[2] == core_idx)
                g1.click(520.0, (y0 + y1) / 2.0)
                u_core = wait_for(lambda: (u := g1.ui())["panel"]["files"]["cwd"] == str(nested.resolve()) and u)
                assert u_core["workspace_state"]["last_cwd"] == str(nested.resolve())
                assert u_core["workspace_state"]["last_view"] == "files"
            finally:
                g1.close()

            # Stop host1 completely and start a fresh host2 on the same state_dir.
            Conn(sock).call(op="host.stop", force=True)
            host1.wait(timeout=5.0)

            host2 = subprocess.Popen(
                [str(LICO), "host", "--socket", sock, "--state-dir", str(state_dir)],
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.DEVNULL,
                text=True,
            )
            assert host2.stdout is not None
            json.loads(host2.stdout.readline())

            # Reopen `lico gui` -> restores `Files` directly in `packages/core`!
            g2 = _GuiInstance(env)
            try:
                u_restored = wait_for(
                    lambda: (u := g2.ui())["panel"] is not None
                    and u["panel"]["files"] is not None
                    and u["panel"]["files"]["cwd"] == str(nested.resolve())
                    and u
                )
                assert [e["name"] for e in u_restored["panel"]["files"]["entries"]] == ["lib.rs"]
            finally:
                g2.close()

            # Remove the nested directory while closed -> next GUI launch degrades gracefully to start screen.
            shutil.rmtree(nested)
            g3 = _GuiInstance(env)
            try:
                u_fallback = wait_for(lambda: (u := g3.ui())["frame"]["frames"] >= 1 and u)
                assert u_fallback["panel"] is None and u_fallback["selected"] == ""
            finally:
                g3.close()
        finally:
            for proc in (host1, host2):
                if proc is not None and proc.poll() is None:
                    proc.terminate()
                    proc.wait(timeout=5.0)
            xvfb.stop()
            shutil.rmtree(rt, ignore_errors=True)


def test_gui_restores_selected_session_and_start_screen_working_directory_across_restart():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = root / "service_repo"
        proj.mkdir(parents=True)
        args, _received = claude_args(root)
        log_dir = root / "logs"

        rt = short_base("rt") / root.name[-6:]
        (rt / "lico").mkdir(mode=0o700, parents=True, exist_ok=True)
        state_dir = root / "state" / "lico"
        home_dir = root / "home"
        state_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
        home_dir.mkdir(mode=0o700, parents=True, exist_ok=True)
        sock = str(rt / "lico" / "host.sock")

        xvfb = Xvfb()
        env = {
            "PATH": "/usr/bin:/bin",
            "LANG": "C.UTF-8",
            "HOME": str(home_dir),
            "XDG_RUNTIME_DIR": str(rt),
            "XDG_STATE_HOME": str(root / "state"),
            "DISPLAY": xvfb.display,
            "LICO_NO_AUTO_OPEN": "1",
        }
        host = subprocess.Popen(
            [str(LICO), "host", "--socket", sock, "--state-dir", str(state_dir), "--log-dir", str(log_dir), *args],
            env=env,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
        )
        try:
            assert host.stdout is not None
            json.loads(host.stdout.readline())
            conn = Conn(sock)
            acc = account(conn, "Work", "claude")
            created = conn.call(op="session.create", kind="claude", account=acc, mode="auto", cwd=str(proj), name="svc-session")
            assert created["ok"] == 1
            sid = created["session"]

            # Window 1: select `sid`, then close `lico gui`.
            g1 = _GuiInstance(env)
            try:
                u0 = wait_for(lambda: (u := g1.ui()) and any(t["session"] == sid for t in u["tabs"]["items"]) and u)
                tab0 = next(t for t in u0["tabs"]["items"] if t["session"] == sid)
                g1.click((tab0["rect"][0] + tab0["close_rect"][0]) / 2.0, (tab0["rect"][1] + tab0["rect"][3]) / 2.0)
                u_sel = wait_for(lambda: (u := g1.ui())["selected"] == sid and u)
                assert u_sel["workspace_state"] == {
                    "last_cwd": str(proj.resolve()),
                    "last_view": "session",
                    "last_session": sid,
                }
            finally:
                g1.close()

            # Window 2: automatically re-attaches `sid` on startup; pressing Escape saves start view while keeping `last_cwd`.
            g2 = _GuiInstance(env)
            try:
                u_reopen = wait_for(lambda: (u := g2.ui())["selected"] == sid and u)
                assert u_reopen["header"].endswith("/service_repo")
                g2.key("Escape")
                u_start = wait_for(lambda: (u := g2.ui())["selected"] == "" and u)
                assert u_start["workspace_state"]["last_view"] == "start"
                assert u_start["workspace_state"]["last_cwd"] == str(proj.resolve())
                assert f"Working directory: {proj.resolve()}" in u_start["start_screen"]
            finally:
                g2.close()

            # Window 3: opens on start screen showing the saved working directory, and Ctrl+N pre-fills it.
            g3 = _GuiInstance(env)
            try:
                u3 = wait_for(lambda: f"Working directory: {proj.resolve()}" in (u := g3.ui())["start_screen"] and u)
                assert u3["selected"] == "" and u3["panel"] is None
                g3.key("ctrl+n")
                u_form = wait_for(lambda: (u := g3.ui())["panel"] is not None and u["panel"]["form"] is not None and u)
                assert u_form["panel"]["form"]["folder"] == str(proj.resolve())
            finally:
                g3.close()
        finally:
            if host.poll() is None:
                host.terminate()
                host.wait(timeout=5.0)
            xvfb.stop()
            shutil.rmtree(rt, ignore_errors=True)
