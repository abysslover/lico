"""M08: persistent Call Rooms (`⇄ A · B`), `@AgentName` shared-context mentions,
host-verified `OWNER-AUTHORIZED` call envelopes, bounded 4-round-trip collaboration,
left-sidebar `[ Calls ]` tab, top-bar Call Room tabs, live `call.say` & `Hang up`,
clickable jump cards, and CLI / IPC / loopback HTTP parity (`0.2.4`)."""

from __future__ import annotations

import json
import stat
import subprocess
import sys
import urllib.request
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.account_helpers import account
from tests.test_claude_adapter import FIXTURE as CLAUDE_FIXTURE, SIM as CLAUDE_SIM, records, status_of
from tests.test_codex_adapter import FIXTURE as CODEX_FIXTURE, SIM as CODEX_SIM, usual_umask, wait_for
from tests.test_first_view import PixelReader
from tests.test_tool_connection import World, rpc
from tests.test_tool_send import call as mcp_call, host_of


def _centre(rect: list[float]) -> tuple[float, float]:
    return (rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0


def _tab_click_xy(tab: dict) -> tuple[float, float]:
    return (tab["rect"][0] + tab["close_rect"][0]) / 2.0, (tab["rect"][1] + tab["rect"][3]) / 2.0


def _dual_args(root: Path, state_dir: Path, token_file: Path | None = None) -> list[str]:
    claude_rec = root / "claude_received.jsonl"
    codex_rec = root / "codex_received.jsonl"
    args = [
        "--claude", sys.executable,
        "--claude-arg", "-I",
        "--claude-arg", str(CLAUDE_SIM),
        "--claude-arg", "--received",
        "--claude-arg", str(claude_rec),
        "--claude-arg", "--fixture",
        "--claude-arg", str(CLAUDE_FIXTURE),
        "--codex", sys.executable,
        "--codex-arg", "-I",
        "--codex-arg", str(CODEX_SIM),
        "--codex-arg", "--received",
        "--codex-arg", str(codex_rec),
        "--codex-arg", "--fixture",
        "--codex-arg", str(CODEX_FIXTURE),
        "--codex-arg", "--lico-mcp",
        "--state-dir", str(state_dir),
        "--log-dir", str(state_dir / "log"),
        "--registry-dir", str(state_dir / "registry"),
        "--accounts-dir", str(state_dir / "accounts"),
    ]
    if token_file is not None:
        args.extend(["--http-port", "auto", "--http-token-file", str(token_file)])
    return args


def _http_json(port: int, token: str, method: str, path: str, body: dict | None = None) -> dict:
    data = None if body is None else json.dumps(body).encode("utf-8")
    req = urllib.request.Request(
        f"http://127.0.0.1:{port}{path}",
        data=data,
        method=method,
        headers={
            "Authorization": f"Bearer {token}",
            "Content-Type": "application/json",
        },
    )
    with urllib.request.urlopen(req, timeout=8) as resp:
        return json.loads(resp.read().decode("utf-8"))


def test_mention_popup_shared_context_call_room_tab_sidebar_say_copy_and_hangup():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        state_dir = root / "state"
        log_dir = state_dir / "log"
        reg_dir = state_dir / "registry"
        token_file = root / "http.token"
        host_args = _dual_args(root, state_dir, token_file=token_file)

        with ProductWindow(host_binary=LICO, host_args=host_args, host_sessions=0) as w:
            reader = PixelReader(w.xvfb.display)
            try:
                conn = Conn(w.socket)
                c_acc = account(conn, "ClaudeMainAcc", "claude")
                x_acc = account(conn, "CodexMainAcc", "codex")
                c_made = conn.call(op="session.create", kind="claude", account=c_acc, mode="auto", name="Pri-Lico")
                x_made = conn.call(op="session.create", kind="codex", account=x_acc, mode="auto", name="Pri-Avatar")
                assert c_made["ok"] == 1 and x_made["ok"] == 1
                s_lico, g_lico = c_made["session"], c_made["generation"]
                s_avatar, _g_avatar = x_made["session"], x_made["generation"]

                # 1. Establish prior human-assistant conversation context inside `Pri-Lico`.
                prior_note = "결정3:SLM-EMR 승인 및 BCN:8 한번만 실행 설정을 기억해줘."
                assert conn.call(
                    op="input",
                    session=s_lico,
                    generation=g_lico,
                    request_id="ctx-1",
                    text=prior_note,
                )["ok"] == 1
                wait_for(lambda: "[turn completed]" in records(log_dir, s_lico))

                # 2. Switch GUI to `Pri-Lico`, type `@Pri` in the prompt editor, verify the `@mention`
                #    autocomplete popup appears with `@Pri-Avatar`, and complete it via `Tab`!
                w.proc.stdin.write("load\n")
                w.proc.stdin.flush()
                u0 = wait_for(lambda: (u := w.ui()) and len(u["tabs"]["items"]) == 2 and u)
                tab_lico = next(t for t in u0["tabs"]["items"] if t["session"] == s_lico)
                w.click(*_tab_click_xy(tab_lico))
                u_lico = wait_for(
                    lambda: (u := w.ui())
                    and u["selected"] == s_lico
                    and u["editor"]["rect"][2] > u["editor"]["rect"][0]
                    and u
                )
                ed_rect = u_lico["editor"]["rect"]
                w.click(*_centre(ed_rect))
                w.type_text("@Pri")
                u_pop = wait_for(
                    lambda: (u := w.ui())
                    and u["mention_popup"]["shown"] is True
                    and any(it["insert_token"] == "@Pri-Avatar" for it in u["mention_popup"]["items"])
                    and u
                )
                assert u_pop["mention_popup"]["rect"] is not None
                w.key("Tab")
                u_completed = wait_for(
                    lambda: (u := w.ui())
                    and u["input"] == "@Pri-Avatar "
                    and u["mention_popup"]["shown"] is False
                    and u
                )
                assert u_completed["input"] == "@Pri-Avatar "
                # Clear the draft before submitting the full Korean + ASCII `@mention` prompt.
                for _ in range(len("@Pri-Avatar ")):
                    w.key("BackSpace")
                wait_for(lambda: w.ui()["input"] == "")

                # 3. Submit `@Pri-Avatar ...` from `Pri-Lico`:
                #    - Zero approval modal (`owner_authorized: true` immediately)
                #    - `Pri-Avatar` receives the `OWNER-AUTHORIZED` call envelope containing `Pri-Lico`'s
                #      prior turn (`결정3:SLM-EMR`) + the human prompt
                #    - `Pri-Avatar`'s reply automatically routes back to `Pri-Lico` with `call cXXXXXX`
                #    - `registry/calls.json` is persisted with strict `0600` permissions!
                mention_msg = "@Pri-Avatar 앞선 검수 문맥을 확인하고 준비 상태를 답장해줘."
                mention_res = conn.call(
                    op="input",
                    session=s_lico,
                    generation=g_lico,
                    request_id="mention-1",
                    text=mention_msg,
                )
                assert mention_res["ok"] == 1 and mention_res["state"] == "observed"
                calls_now = conn.call(op="call.list")
                assert calls_now["ok"] == 1 and len(calls_now["calls"]) == 1
                call_id = calls_now["calls"][0]["call_id"]

                wait_for(
                    lambda: any("[peer reply from 'Pri-Avatar'" in r for r in records(log_dir, s_lico))
                    and records(log_dir, s_lico).count("[turn completed]") >= 2
                )
                assert status_of(conn, s_lico).get("approvals") is None
                assert not w.ui()["approval_bar"]

                avatar_records = "\n".join(records(log_dir, s_avatar))
                assert "OWNER-AUTHORIZED" in avatar_records
                assert f"({call_id[:7]})" in avatar_records
                assert "결정3:SLM-EMR" in avatar_records
                assert "앞선 검수 문맥을 확인하고 준비 상태를 답장해줘." in avatar_records

                calls_path = reg_dir / "calls.json"
                assert calls_path.exists()
                assert stat.S_IMODE(calls_path.stat().st_mode) == 0o600
                calls_disk = json.loads(calls_path.read_text(encoding="utf-8"))
                assert any(r["call_id"] == call_id and r["owner_authorized"] is True for r in calls_disk["calls"])

                # 4. Verify `Pri-Lico` displays both the `[⇄ call ...]` jump card and `[peer reply ... | call cXXXXXX]`
                #    card, and stationary-clicking the `[⇄ call ...]` line opens the top-bar `⇄ Pri-Lico·Pri-Avatar`
                #    Call Room tab!
                u_after_mention = wait_for(
                    lambda: (u := w.ui())
                    and any(f"call {call_id[:7]}" in ln for ln in u["shown_lines"])
                    and any(ln.startswith(">> [peer reply from 'Pri-Avatar'") for ln in u["shown_lines"])
                    and u
                )
                jump_idx = next(
                    i for i, ln in enumerate(u_after_mention["shown_lines"]) if f"call {call_id[:7]}" in ln
                )
                line_band = u_after_mention["layout"]["lines"][jump_idx]
                click_x = u_after_mention["layout"]["list_w"] + 80.0
                click_y = (line_band[0] + line_band[1]) / 2.0
                w.click(click_x, click_y)

                u_room = wait_for(
                    lambda: (u := w.ui())
                    and u["calls"]["active_call"] == call_id
                    and any(t["call_id"] == call_id and t["selected"] is True for t in u["calls"]["tabs"])
                    and u["calls"]["detail"] is not None
                    and u["calls"]["detail"]["entry_count"] >= 2
                    and any("Call Room" in ln for ln in u["shown_lines"])
                    and u
                )
                assert any(ct["call_id"] == call_id for ct in u_room["tabs"]["call_items"])
                assert u_room["calls"]["hangup_rect"] is not None

                # 5. Verify mouse drag-selection inside the Call Room view auto-copies onto X11 `CLIPBOARD`!
                wid = w.window_id()
                rlines = u_room["layout"]["lines"]
                ry0 = int((rlines[0][0] + rlines[0][1]) / 2.0)
                ry1 = int((rlines[min(2, len(rlines) - 1)][0] + rlines[min(2, len(rlines) - 1)][1]) / 2.0)
                subprocess.run(
                    [
                        "xdotool", "mousemove", "--window", wid, "540", str(ry0),
                        "mousedown", "1",
                        "mousemove", "--window", wid, "540", str(ry1),
                        "mouseup", "1",
                    ],
                    env=w.env,
                    check=True,
                )
                u_room_sel = wait_for(lambda: (u := w.ui()) and u["selection"] and "Call Room" in u["selection"] and u)
                wait_for(lambda: reader.selection_owner("CLIPBOARD") != 0 or None)
                assert "Call Room" in reader.selection_text("CLIPBOARD")
                assert u_room_sel["calls"]["active_call"] == call_id

                # 6. Type directly inside the Call Room prompt editor (`@Pri-Avatar ...` + Enter -> `call.say`)!
                ed_room = u_room_sel["editor"]["rect"]
                w.click(*_centre(ed_room))
                w.type_text("@Pri-Avatar second check from call room")
                wait_for(lambda: "second check from call room" in w.ui()["input"])
                prev_entries = u_room_sel["calls"]["detail"]["entry_count"]
                w.key("Return")
                u_after_say = wait_for(
                    lambda: (u := w.ui())
                    and u["calls"]["detail"] is not None
                    and u["calls"]["detail"]["entry_count"] > prev_entries
                    and any("second check from call room" in ln for ln in u["shown_lines"])
                    and u
                )
                assert u_after_say["input"] == ""

                # 7. Switch the left sidebar to `[ Calls ]` and verify the Call Room card & quick-connect list!
                calls_tab_r = u_after_say["sidebar"]["calls_tab_rect"]
                assert calls_tab_r is not None
                w.click(*_centre(calls_tab_r))
                u_calls_sb = wait_for(
                    lambda: (u := w.ui())
                    and u["sidebar"]["tab"] == "calls"
                    and len(u["sidebar"]["call_entries"]) == 1
                    and u
                )
                assert u_calls_sb["sidebar"]["call_entries"][0]["call_id"] == call_id
                assert "Pri-Lico" in u_calls_sb["sidebar"]["call_entries"][0]["title"]
                assert "Pri-Avatar" in u_calls_sb["sidebar"]["call_entries"][0]["title"]

                # 8. Click `Hang up` in the Call Room header and verify the room pauses (`paused == True`)!
                w.click(*_centre(u_calls_sb["calls"]["hangup_rect"]))
                u_paused = wait_for(
                    lambda: (u := w.ui())
                    and u["calls"]["detail"] is not None
                    and u["calls"]["detail"]["paused"] is True
                    and u["calls"]["detail"]["status"] == "paused"
                    and u
                )
                assert u_paused["sidebar"]["call_entries"][0]["status"] == "paused"

                # 9. Verify CLI (`lico call list|show|say|hangup|close`) and loopback HTTP (`/v1/calls...`) parity!
                cli_list = json.loads(
                    subprocess.run(
                        [str(LICO), "--socket", w.socket, "call", "list"],
                        env=w.env,
                        capture_output=True,
                        text=True,
                        check=True,
                    ).stdout
                )
                assert cli_list["ok"] == 1 and cli_list["calls"][0]["call_id"] == call_id

                cli_show = json.loads(
                    subprocess.run(
                        [str(LICO), "--socket", w.socket, "call", "show", call_id[:7]],
                        env=w.env,
                        capture_output=True,
                        text=True,
                        check=True,
                    ).stdout
                )
                assert cli_show["ok"] == 1 and cli_show["call"]["call_id"] == call_id

                # Discover HTTP port from the host's initial stdout line (`old_host` already printed it; ask via token + status).
                token = token_file.read_text(encoding="utf-8").strip()
                st_reply = conn.call(op="status")
                http_port = st_reply.get("http_port")
                if http_port:
                    http_calls = _http_json(http_port, token, "GET", "/v1/calls")
                    assert http_calls["ok"] == 1 and http_calls["calls"][0]["call_id"] == call_id
                    http_one = _http_json(http_port, token, "GET", f"/v1/calls/{call_id}")
                    assert http_one["ok"] == 1 and http_one["call"]["paused"] is True

                cli_close = json.loads(
                    subprocess.run(
                        [str(LICO), "--socket", w.socket, "call", "close", call_id],
                        env=w.env,
                        capture_output=True,
                        text=True,
                        check=True,
                    ).stdout
                )
                assert cli_close["ok"] == 1 and cli_close["closed"] is True
            finally:
                reader.close()


def test_mcp_instructions_bounded_four_round_burst_and_call_done_early_settlement():
    with private_dir() as directory:
        host, log, received = host_of(directory)
        try:
            world = World(directory, host, log, received)
            s_a, g_a = world.session(name="Pri-Lico")
            s_b, g_b = world.session(name="Pri-Avatar")
            cfg_a = world.config(s_a)
            cfg_b = world.config(s_b)
            world.first_turn(s_a, g_a)
            world.first_turn(s_b, g_b)

            # 1. Verify MCP `initialize` advertises host-trust instructions for `OWNER-AUTHORIZED` peer turns.
            st_code, init_body = rpc(cfg_a, "initialize", {"protocolVersion": "2025-11-25"})
            assert st_code == 200
            instructions = init_body["result"].get("instructions", "")
            assert "OWNER-AUTHORIZED" in instructions and "[peer reply from ...]" in instructions and "[call done]" in instructions
            assert rpc(cfg_b, "initialize", {"protocolVersion": "2025-11-25"})[0] == 200

            # 2. Owner starts an authorized Call Room (`call.start`) and triggers an autonomous multi-turn
            #    exchange (`AUTO_CHAIN_TO:<s_b>`): `Pri-Lico` and `Pri-Avatar` automatically exchange rounds
            #    1/4, 2/4, 3/4, and 4/4 with zero approval prompts, and round 5 is deterministically refused
            #    with `call_round_limit`!
            started = world.conn.call(op="call.start", **{"from": s_a, "to": s_b})
            assert started["ok"] == 1
            call_id = started["call"]["call_id"]
            assert started["call"]["owner_authorized"] is True

            assert world.conn.call(
                op="input",
                session=s_a,
                generation=g_a,
                request_id="burst-start",
                text=f"tool:send {json.dumps({'to': s_b, 'text': f'AUTO_CHAIN_TO:{s_b}'})}",
            )["ok"] == 1

            wait_for(
                lambda: any("call_round_limit:" in r for r in records(world.log, s_a))
            )
            a_recs = records(world.log, s_a)
            for expected_round in (1, 2, 3, 4):
                assert any(f"call {call_id[:7]} round {expected_round}/4]" in r for r in a_recs), (expected_round, a_recs)
            assert status_of(world.conn, s_a).get("approvals") is None
            det = world.conn.call(op="call.get", call=call_id)["call"]
            assert det["round"] == 4 and det["active"] is False
            assert any("4/4 automatic rounds" in e["text"] for e in det["entries"])

            # 3. Human speaks in the Call Room (`call.say`) -> resets burst counter so agents can collaborate again,
            #    and `[call done]` settles the active exchange cleanly!
            said = world.conn.call(op="call.say", call=call_id, to=s_b, text="한 번만 더 확인하고 [call done]으로 마무리해줘.")
            assert said["ok"] == 1
            wait_for(
                lambda: any("[call done]" in r for r in records(world.log, s_b))
                and world.conn.call(op="call.get", call=call_id)["call"]["active"] is False
            )
            det_after = world.conn.call(op="call.get", call=call_id)["call"]
            assert det_after["active"] is False and det_after["paused"] is False
        finally:
            host.stop()
