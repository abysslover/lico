"""M02-P05: private per-session virtual displays (`vdisplay`), XTEST computer-use helper (`xjob computer`),
and gated time-bounded real-screen control (`CU-01`..`CU-06`, `CU-08`, `MB-03`, `SF-01`, `SF-03`, `SF-05`,
`SF-06`, `XF-07`)."""
import base64
import json
import os
import subprocess
import sys
import time
from pathlib import Path

import pytest

from lico_measure.captures import ProductWindow
from lico_measure.p02 import HOSTD_DIR, LICO, ProtoConn as Conn, start
from lico_measure.rig import Xvfb, private_dir
from lico_protocol import v1
from tests.test_capture_cli import read_png
from tests.test_card_window import ready
from tests.test_claude_adapter import FIXTURE, SIM, new_claude, status_of
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_new_session_screen import claude_args
from tests.test_session_actions_window import row_centre
from tests.test_tool_connection import World, config_of, rpc

EXE = HOSTD_DIR / "lico"


def host_with_err(directory, display, extra_env=None):
    received = Path(directory) / "claude_received.txt"
    extra = [
        "--claude", sys.executable, "--claude-arg", "-I", "--claude-arg", str(SIM),
        "--claude-arg", "--received", "--claude-arg", str(received),
        "--claude-arg", "--fixture", "--claude-arg", str(FIXTURE),
        "--cred-bin", str(HOSTD_DIR / "lico-cred"), "--cred-dir", str(Path(directory) / "credentials"),
    ]
    env = {"DISPLAY": display}
    if extra_env:
        env.update(extra_env)
    with usual_umask():
        host, log = start(directory, extra=extra, sessions=0, producing=0, history=0, retain=100000, env_overrides=env)
    return host, log, received


def call_tool(world, sid, **arguments):
    status, body = rpc(world.config(sid), "tools/call", {"name": "computer", "arguments": arguments})
    assert status == 200, body
    return body["result"]


def cli_run(socket, *args, env_more=None):
    env = dict(os.environ, LICO_SOCKET=socket)
    if env_more:
        env.update(env_more)
    return subprocess.run([str(LICO), *args], env=env, capture_output=True, text=True, timeout=15)


@pytest.fixture(scope="module")
def real_x():
    server = Xvfb("1920x1080x24")
    yield server.display
    server.stop()


@pytest.fixture(scope="module")
def world(real_x):
    with private_dir() as directory:
        host, log, received = host_with_err(directory, real_x)
        try:
            yield World(directory, host, log, received)
        finally:
            host.stop()


def test_virtual_displays_are_isolated_reported_in_session_memory_cli_accessible_and_cleaned_up(world, real_x):
    s1, g1 = world.session(name="alpha")
    s2, g2 = world.session(name="beta")

    r1 = call_tool(world, s1, action="look")
    r2 = call_tool(world, s2, action="look")
    assert not r1["isError"] and not r2["isError"]
    m1 = json.loads(r1["content"][0]["text"])
    m2 = json.loads(r2["content"][0]["text"])
    d1, d2 = m1["display"], m2["display"]
    assert d1 != d2 and d1 != real_x and d2 != real_x
    assert (m1["screen_width"], m1["screen_height"], m1["width"], m1["height"], m1["scale"]) == (1280, 800, 1280, 800, 1.0)
    w, h, _ = read_png_bytes(base64.b64decode(r1["content"][1]["data"]))
    assert (w, h) == (1280, 800)

    # CLI: export line, status JSON (with Xvfb memory), and `display run`
    exp = cli_run(world.host.socket, "display", "--session", s1)
    assert exp.returncode == 0 and exp.stdout.strip() == f"export DISPLAY='{d1}'"
    st_cli = json.loads(cli_run(world.host.socket, "display", "status", "--session", s1).stdout)
    assert v1.validate_response("display.status", st_cli) == []
    assert st_cli["running"] is True and st_cli["display"] == d1 and st_cli["memory"]["private_kb"] > 0
    ran = cli_run(world.host.socket, "display", "run", "--session", s1, "--", "sh", "-c", 'printf "%s" "$DISPLAY"')
    assert ran.returncode == 0 and ran.stdout == d1

    # Protocol v1 status conformance + MB-03 memory attribution to session
    world.conn.call(op="status")
    def has_mem():
        st = world.conn.call(op="status")
        assert v1.validate_response("status", st) == []
        row = next(s for s in st["sessions"] if s["session"] == s1)
        return row if row.get("virtual_display") == d1 and row.get("memory", {}).get("processes", 0) >= 2 else None
    row = wait_for(has_mem, timeout=10)
    assert row["memory"]["private_kb"] >= st_cli["memory"]["private_kb"]

    # CU-08: WM-less Xvfb focus — xmessage accepts Return via `computer` without any click or WM
    xm = subprocess.Popen(
        ["xmessage", "-geometry", "+180+140", "-buttons", "OK:0", "-default", "OK", "focus-check"],
        env=dict(os.environ, DISPLAY=d2), stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
    )
    try:
        wait_for(lambda: b"focus-check" in b"" or _window_mapped(d2), timeout=5)
        kr = call_tool(world, s2, action="key", key="Return")
        assert not kr["isError"], kr
        assert xm.wait(timeout=5) == 0
    finally:
        if xm.poll() is None:
            xm.kill()
            xm.wait()

    # No temporary capture files left behind in captures/
    captures_dir = Path(world.directory) / "captures"
    assert not captures_dir.exists() or list(captures_dir.iterdir()) == []

    # Purging s1 and stopping s2 tear down their Xvfb processes immediately
    assert world.conn.call(op="session.purge", session=s1, generation=g1)["ok"] == 1
    assert world.conn.call(op="display.stop", session=s2)["stopped"] is True
    assert world.conn.call(op="display.status", session=s2)["running"] is False


def test_downscaled_1080p_coordinate_scaling_and_xjob_actions(real_x):
    with private_dir() as d:
        out_png = Path(d) / "out.png"
        # 1920x1080 shrinks to 1568x882; (1470, 816) in agent coords maps to ~(1800, 1000) in screen coords
        done = subprocess.run(
            [str(EXE), "xjob", "computer", "--display", real_x, "--action", "move", "--x", "1470", "--y", "816", "--out", str(out_png)],
            env={"PATH": "/usr/bin:/bin", "LANG": "C.UTF-8"}, capture_output=True, text=True, timeout=15,
        )
        assert done.returncode == 0, done.stderr
        ans = json.loads(done.stdout)
        assert (ans["width"], ans["height"], ans["agent"]["width"], ans["agent"]["height"]) == (1920, 1080, 1568, 882)
        assert 1799 <= ans["screen_x"] <= 1801 and 999 <= ans["screen_y"] <= 1001
        bad = subprocess.run(
            [str(EXE), "xjob", "computer", "--display", real_x, "--action", "click", "--x", "1568", "--y", "10", "--out", str(out_png)],
            env={"PATH": "/usr/bin:/bin", "LANG": "C.UTF-8"}, capture_output=True, text=True, timeout=15,
        )
        assert bad.returncode == 2 and json.loads(bad.stdout)["error"] == "out_of_bounds"


def test_idle_virtual_display_is_reaped_automatically():
    with private_dir() as directory:
        host, log, received = host_with_err(directory, "", extra_env={"LICO_DISPLAY_IDLE_S": "1"})
        try:
            w = World(directory, host, log, received)
            sid, _ = w.session(name="idle-vd")
            ens = w.conn.call(op="display.ensure", session=sid)
            assert v1.validate_response("display.ensure", ens) == []
            assert w.conn.call(op="display.status", session=sid)["running"] is True
            wait_for(lambda: w.conn.call(op="display.status", session=sid)["running"] is False, timeout=6)
        finally:
            host.stop()


def test_real_screen_audit_log_never_records_typed_secrets_and_cli_stop_revokes(world):
    canary = "SECRET_TYPED_CANARY_8842"
    sid, gen = world.session(name="auditor")
    first = json.loads(call_tool(world, sid, action="look", target="real")["content"][0]["text"])
    assert first["status"] == "pending" and first["target"] == "real"
    aid = first["approval_id"]
    assert world.conn.call(op="approval.answer", session=sid, generation=gen, approval_id=aid, answer="allow", keep=True)["error"] == "bad_request"
    assert world.conn.call(op="approval.answer", session=sid, generation=gen, approval_id=aid, answer="allow")["ok"] == 1
    typed = call_tool(world, sid, action="type", target="real", text=canary)
    assert typed["isError"] is False
    err_text = (Path(world.directory) / "host.err").read_text(encoding="utf-8", errors="replace")
    assert "audit tool=computer target=real" in err_text
    assert canary not in err_text
    stopped = cli_run(world.host.socket, "real-screen", "stop", "--session", sid)
    assert stopped.returncode == 0 and json.loads(stopped.stdout)["stopped"] == 1
    again = json.loads(call_tool(world, sid, action="look", target="real")["content"][0]["text"])
    assert again["status"] == "pending"


def test_real_screen_is_off_by_default_gated_and_stopped_from_gui_button_and_hotkey():
    with private_dir() as d:
        args, _ = claude_args(d)
        made = {}

        def sessions(w):
            conn = Conn(w.socket)
            made["sid"], made["gen"] = new_claude(conn, name="desk-bot")

        with ProductWindow(host_binary=LICO, host_args=args, before_window=sessions) as w:
            conn = Conn(w.socket)
            sid, gen = made["sid"], made["gen"]
            cfg = wait_for(lambda: config_of(Path(w.directory) / "state" / "lico", sid))
            rpc(cfg, "initialize", {})

            wait_for(lambda: len(w.ui()["rows"]) == 1)
            w.click(*row_centre(w, sid))
            wait_for(lambda: w.ui()["selected"] == sid)
            assert w.ui()["real_screen"]["active"] is False

            # 1. Default off: target="real" returns pending and puts a host-owned card on the row
            _, body = rpc(cfg, "tools/call", {"name": "computer", "arguments": {"action": "look", "target": "real"}})
            pending = json.loads(body["result"]["content"][0]["text"])
            assert pending["status"] == "pending" and pending["target"] == "real"
            aid = pending["approval_id"]

            # Never sticky for the session: keep=True is refused by the host and Shift+S is absent from the GUI hint
            assert conn.call(op="approval.answer", session=sid, generation=gen, approval_id=aid, answer="allow", keep=True)["error"] == "bad_request"
            w.move(700, 400)
            u_card = ready(w)
            assert "Shift+S" not in u_card["hint"], u_card["hint"]

            # 2. Allow via Y in the GUI -> crimson real-screen bar appears with countdown and [ Stop now ] button
            w.key("y")
            rs = wait_for(lambda: (r := w.ui()["real_screen"]) and r["active"] and r["stop_rect"] and r)
            assert "!! REAL SCREEN: 'desk-bot' is controlling this screen" in rs["banner"]
            assert "left" in rs["banner"]

            # 3. Real-screen actions succeed inside the active grant
            _, look_res = rpc(cfg, "tools/call", {"name": "computer", "arguments": {"action": "look", "target": "real"}})
            assert look_res["result"]["isError"] is False
            look_meta = json.loads(look_res["result"]["content"][0]["text"])
            assert look_meta["target"] == "real" and look_meta["seconds_left"] > 0

            # 4. Clicking [ Stop now ] revokes immediately; next call is pending again
            sr = rs["stop_rect"]
            w.click((sr[0] + sr[2]) / 2, (sr[1] + sr[3]) / 2)
            wait_for(lambda: w.ui()["real_screen"]["active"] is False)
            _, again = rpc(cfg, "tools/call", {"name": "computer", "arguments": {"action": "look", "target": "real"}})
            assert json.loads(again["result"]["content"][0]["text"])["status"] == "pending"

            # 5. Allow again, then revoke via Ctrl+Alt+Esc global hotkey in GUI
            ready(w)
            w.key("y")
            wait_for(lambda: w.ui()["real_screen"]["active"] is True)
            w.key("ctrl+alt+Escape")
            wait_for(lambda: w.ui()["real_screen"]["active"] is False)


def read_png_bytes(raw: bytes):
    with private_dir() as d:
        p = Path(d) / "t.png"
        p.write_bytes(raw)
        return read_png(p)


def _window_mapped(display: str) -> bool:
    out = subprocess.run(["xwininfo", "-display", display, "-root", "-tree"], capture_output=True, text=True).stdout
    return "xmessage" in out.lower()
