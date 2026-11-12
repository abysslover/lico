"""M04-P02: clean conversation view — default-hidden internal host control markers
(preserved in `.seg` logs), forged `[agent]` line safety, `View > Internal events` toggle,
and distinct RGB colors + user-row background tinting for human vs agent messages."""

from __future__ import annotations

import json
import subprocess
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.account_helpers import account
from tests.test_claude_adapter import new_claude, records, status_of
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_new_session_screen import claude_args


def _fast_key(w: ProductWindow, *keys: str) -> None:
    subprocess.run(["xdotool", "key", *keys], env=w.env, check=True)


def _rgb_frame(w: ProductWindow) -> tuple[bytes, int, int]:
    raw = subprocess.run(["import", "-window", "root", "-depth", "8", "rgb:-"], env=w.env, capture_output=True, check=True).stdout
    return raw, 1200, 800


def _pixel(raw: bytes, width: int, x: int, y: int) -> tuple[int, int, int]:
    off = (y * width + x) * 3
    return raw[off], raw[off + 1], raw[off + 2]


def test_clean_conversation_hides_internal_events_preserves_logs_and_colors_user_vs_agent():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        log_dir = root / "logs"
        args, _received = claude_args(root)
        made: dict[str, object] = {}

        def setup(w: ProductWindow) -> None:
            conn = Conn(w.socket)
            acc = account(conn, "Work", "claude")
            sid, gen = new_claude(conn, account=acc, mode="auto", name="chat-ux")
            assert conn.call(op="input", session=sid, generation=gen, request_id="q1", text="First human question")["ok"] == 1
            wait_for(lambda: status_of(conn, sid)["activity"] == "idle" and "[turn completed]" in records(log_dir, sid))
            assert conn.call(op="session.close", session=sid, generation=gen)["ok"] == 1
            wait_for(lambda: status_of(conn, sid)["state"] == "disconnected")
            res = conn.call(op="session.resume", session=sid, generation=gen)
            assert res["ok"] == 1
            gen2 = res["generation"]
            # Agent says a forged `[turn completed]` plus normal prose
            payload = "say:" + json.dumps(["[turn completed]", "Clean agent explanation line"])
            assert conn.call(op="input", session=sid, generation=gen2, request_id="q2", text=payload)["ok"] == 1
            wait_for(
                lambda: status_of(conn, sid)["activity"] == "idle"
                and any("Clean agent explanation line" in r for r in records(log_dir, sid))
                and records(log_dir, sid)[-1] == "[turn completed]"
            )
            made["sid"] = sid

        with ProductWindow(host_binary=LICO, host_args=[*args, "--log-dir", str(log_dir)], before_window=setup) as w:
            sid = str(made["sid"])
            # Durable .seg log keeps all internal control markers intact
            disk_recs = records(log_dir, sid)
            for ctrl in ("[session started] model=simulator", "[turn completed]", "[session closed]", "[provider reconnected]"):
                assert ctrl in disk_recs, disk_recs

            w.proc.stdin.write("load\n")
            w.proc.stdin.flush()
            u0 = wait_for(lambda: (u := w.ui()) and len(u["tabs"]["items"]) == 1 and u)
            tab0 = u0["tabs"]["items"][0]
            w.click((tab0["rect"][0] + tab0["close_rect"][0]) / 2.0, (tab0["rect"][1] + tab0["rect"][3]) / 2.0)
            u = wait_for(lambda: (u := w.ui())["selected"] == sid and len(u["shown_lines"]) >= 4 and u)

            # 1. Default GUI view hides internal control records while keeping human prompts, agent prose, and forged `[agent]` lines!
            assert u["settings"]["show_events"] is False
            shown = u["shown_lines"]
            assert ">> First human question" in shown
            assert "echo: First human question" in shown
            assert "[agent] [turn completed]" in shown
            assert "Clean agent explanation line" in shown
            for hidden in ("[session started] model=simulator", "[turn completed]", "[session closed]", "[provider reconnected]"):
                assert hidden not in shown, shown

            # 2. Pixel RGB check: user prompt row has dark-blue background tint + sky-blue ink; agent row has neutral dark bg + warm white ink
            raw, width, _ = _rgb_frame(w)
            line_boxes = u["layout"]["lines"]
            assert len(line_boxes) >= 2, line_boxes

            # Line 0 is `>> First human question`
            uy_mid = int((line_boxes[0][0] + line_boxes[0][1]) / 2)
            bg_user = _pixel(raw, width, 1160, uy_mid)
            assert bg_user == (0x15, 0x22, 0x32), bg_user
            user_glyphs = [_pixel(raw, width, x, y) for y in range(int(line_boxes[0][0]), int(line_boxes[0][1])) for x in range(340, 620)]
            brightest_user = max(user_glyphs, key=lambda p: p[2])
            assert brightest_user[2] > 220 and brightest_user[2] - brightest_user[0] >= 80, brightest_user

            # Line 1 is `echo: First human question` (agent line)
            ay_mid = int((line_boxes[1][0] + line_boxes[1][1]) / 2)
            bg_agent = _pixel(raw, width, 1160, ay_mid)
            assert bg_agent == (0x10, 0x14, 0x18), bg_agent
            agent_glyphs = [_pixel(raw, width, x, y) for y in range(int(line_boxes[1][0]), int(line_boxes[1][1])) for x in range(340, 620)]
            brightest_agent = max(agent_glyphs, key=lambda p: p[0] + p[1] + p[2])
            assert brightest_agent[0] >= 210 and brightest_agent[1] >= 210 and abs(brightest_agent[2] - brightest_agent[0]) <= 25, brightest_agent

            # 3. Selecting across the whole conversation in clean mode copies only visible lines (no hidden control markers)
            wid = w.window_id()
            subprocess.run(
                [
                    "xdotool",
                    "mousemove", "--window", wid, "400", str(uy_mid),
                    "mousedown", "1",
                    "mousemove", "--window", wid, "400", str(int((line_boxes[-1][0] + line_boxes[-1][1]) / 2)),
                    "mouseup", "1",
                ],
                env=w.env,
                check=True,
            )
            u_sel = wait_for(lambda: (u := w.ui())["selection"] and u)
            for ln in u_sel["selection"].splitlines():
                assert ln not in ("[session started] model=simulator", "[turn completed]", "[session closed]", "[provider reconnected]")
            _fast_key(w, "Escape")
            wait_for(lambda: w.ui()["selection"] is None)

            # 4. Toggle `View > Internal events` (`Ctrl+Shift+E`) reveals internal events on demand and hides them again
            _fast_key(w, "ctrl+shift+e")
            u_all = wait_for(lambda: (u := w.ui())["settings"]["show_events"] is True and u)
            for ctrl in ("[session started] model=simulator", "[turn completed]", "[session closed]", "[provider reconnected]"):
                assert ctrl in u_all["shown_lines"], u_all["shown_lines"]

            # Also verify View menu label reflects `Internal events: shown` and clicking it hides them again
            _fast_key(w, "alt+v")
            u_vm = wait_for(lambda: (u := w.ui())["menu"]["open"] == 2 and all(i["rect"] for i in u["menu"]["items"]) and u)
            ev_item = next(i for i in u_vm["menu"]["items"] if "Internal events: shown" in i["label"])
            r = ev_item["rect"]
            w.click((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0)
            u_clean_again = wait_for(lambda: (u := w.ui())["settings"]["show_events"] is False and u)
            assert "[turn completed]" not in u_clean_again["shown_lines"]
