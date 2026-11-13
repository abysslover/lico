"""M07-P04: bidirectional Claude <-> Codex MCP session communication, local `/exit` (`/quit`)
tab dismissal without LLM echo or `/exit` auto-title pollution, slash-title recovery on restart,
`codex_mapping` `mcpToolCall` parity, and toolbar `Send -> Session (N)` + `Copy` UX (`0.2.3`)."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn, read_log, start
from lico_measure.rig import REPO, finished_lines, private_dir
from lico_protocol import codex_mapping, v1
from tests.account_helpers import account
from tests.test_claude_adapter import FIXTURE as CLAUDE_FIXTURE, SIM as CLAUDE_SIM, records, status_of
from tests.test_codex_adapter import FIXTURE as CODEX_FIXTURE, SIM as CODEX_SIM, sent as codex_sent, usual_umask, wait_for
from tests.test_first_view import PixelReader


def _centre(rect: list[float]) -> tuple[float, float]:
    return (rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0


def _dual_provider_args(directory: Path, *, codex_mcp: bool = True) -> tuple[list[str], Path, Path]:
    claude_rec = directory / "claude_received.jsonl"
    codex_rec = directory / "codex_received.jsonl"
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
    ]
    if codex_mcp:
        args += ["--codex-arg", "--lico-mcp"]
    return args, claude_rec, codex_rec


def test_bidirectional_claude_and_codex_mcp_session_comms_and_mcp_tool_records():
    # Protocol reference mapping parity for Codex `mcpToolCall` items
    m1 = codex_mapping.Mapper().feed([
        {"dir": "server", "msg": {"method": "item/started", "params": {"item": {"id": "m1", "type": "mcpToolCall", "server": "lico", "tool": "send"}}}},
        {"dir": "server", "msg": {"method": "item/completed", "params": {"item": {"id": "m1", "type": "mcpToolCall", "server": "lico", "tool": "send", "result": {"isError": True, "content": [{"type": "text", "text": "denied"}]}}}}},
    ])
    assert m1.records == ["[tool] mcp__lico__send", "[tool error]", "> denied"]

    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        extra, _claude_rec, codex_rec = _dual_provider_args(root, codex_mcp=True)
        host, log = start(directory, extra=extra, sessions=0, producing=0, history=0, retain=10000)
        try:
            conn = Conn(host.socket)
            c_made = conn.call(op="session.create", kind="claude", name="claude_peer")
            x_made = conn.call(op="session.create", kind="codex", name="codex_peer")
            assert c_made["ok"] == 1 and x_made["ok"] == 1
            claude_sid, claude_gen = c_made["session"], c_made["generation"]
            codex_sid, codex_gen = x_made["session"], x_made["generation"]

            # Wait until Codex simulator logs its ARGV and verify MCP config flags (no raw secret in ARGV)
            argv_lines = wait_for(lambda: [l for l in finished_lines(codex_rec) if l.startswith("ARGV ")])
            codex_argv = json.loads(argv_lines[0][5:])
            assert any(a.startswith('mcp_servers.lico.url="http://127.0.0.1:') for a in codex_argv), codex_argv
            assert 'mcp_servers.lico.bearer_token_env_var="LICO_MCP_TOKEN"' in codex_argv
            assert 'shell_environment_policy.exclude=["LICO_MCP_TOKEN"]' in codex_argv
            assert "--lico-mcp" not in codex_argv

            # 1) Codex calls `sessions` over loopback MCP and emits `[tool] mcp__lico__sessions`
            assert conn.call(op="input", session=codex_sid, generation=codex_gen, request_id="x-1", text="tools")["ok"] == 1
            wait_for(lambda: "[turn completed]" in records(log, codex_sid))
            codex_recs = records(log, codex_sid)
            assert "[tool] mcp__lico__sessions" in codex_recs, codex_recs
            assert any(r.startswith("> ") and claude_sid in r and '"kind":"claude"' in r for r in codex_recs), codex_recs

            # 2) Claude calls `mcp__lico__send` targeting Codex (previously rejected as not_allowed!)
            msg_c2x = "안녕 Codex! Claude 세션에서 인사 보냅니다."
            assert conn.call(
                op="input",
                session=claude_sid,
                generation=claude_gen,
                request_id="c-1",
                text=f"tool:send {json.dumps({'to': codex_sid, 'text': msg_c2x}, ensure_ascii=False)}",
            )["ok"] == 1
            cards_c = wait_for(lambda: status_of(conn, claude_sid).get("approvals"))
            aid_c = cards_c[0]["approval_id"]
            ans_c = conn.call(op="approval.answer", session=claude_sid, generation=claude_gen, approval_id=aid_c, answer="allow")
            assert ans_c["ok"] == 1 and ans_c["outcome"] == "done", ans_c
            wait_for(lambda: f">> {msg_c2x}" in records(log, codex_sid))
            assert any(r.startswith(">> [from session 'claude_peer'") for r in records(log, codex_sid))

            # 3) Codex calls `mcp__lico__send` targeting Claude back!
            msg_x2c = "반가워요 Claude! Codex 세션에서 답장합니다."
            assert conn.call(
                op="input",
                session=codex_sid,
                generation=codex_gen,
                request_id="x-2",
                text=f"tool:send {json.dumps({'to': claude_sid, 'text': msg_x2c}, ensure_ascii=False)}",
            )["ok"] == 1
            cards_x = wait_for(lambda: status_of(conn, codex_sid).get("approvals"))
            aid_x = cards_x[0]["approval_id"]
            assert "[tool] mcp__lico__send" in records(log, codex_sid)
            ans_x = conn.call(op="approval.answer", session=codex_sid, generation=codex_gen, approval_id=aid_x, answer="allow")
            assert ans_x["ok"] == 1 and ans_x["outcome"] == "done", ans_x
            wait_for(lambda: f">> {msg_x2c}" in records(log, claude_sid))
            assert any(r.startswith(">> [from session 'codex_peer'") for r in records(log, claude_sid))
        finally:
            host.stop()


def test_local_exit_and_quit_dismiss_tabs_preserve_chats_purge_blank_tabs_and_recover_slash_titles():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        state_dir = root / "state"
        extra, _claude_rec, codex_rec = _dual_provider_args(root, codex_mcp=True)
        host_args = [
            *extra,
            "--log-dir", str(state_dir / "log"),
            "--registry-dir", str(state_dir / "registry"),
            "--accounts-dir", str(state_dir / "accounts"),
            "--workspace-root", str(state_dir / "workspaces"),
            "--handoff-dir", str(state_dir / "handoff"),
        ]
        with ProductWindow(host_binary=LICO, host_args=host_args, host_sessions=0) as w:
            conn = Conn(w.socket)
            s1_res = conn.call(op="session.create", kind="claude")
            s2_res = conn.call(op="session.create", kind="codex")
            s3_res = conn.call(op="session.create", kind="codex")
            s1, g1 = s1_res["session"], s1_res["generation"]
            s2, g2 = s2_res["session"], s2_res["generation"]
            s3 = s3_res["session"]

            # Real conversation on s1; slash command then real prompt on s2; s3 left blank (0 turns)
            assert conn.call(op="input", session=s1, generation=g1, request_id="p1", text="리코 세션 유지 확인")["ok"] == 1
            wait_for(lambda: "[turn completed]" in records(state_dir / "log", s1))

            assert conn.call(op="input", session=s2, generation=g2, request_id="p2-slash", text="/model")["ok"] == 1
            wait_for(lambda: "echo: /model" in [p.decode() for _, p in read_log(state_dir / "log", s2)])
            assert "auto_title" not in status_of(conn, s2), "slash command must never become auto_title"

            assert conn.call(op="input", session=s2, generation=g2, request_id="p2-real", text="지금은 다른 메신저와 대화 가능?")["ok"] == 1
            wait_for(lambda: status_of(conn, s2).get("auto_title") == "지금은 다른 메신저와 대화 가능?")
            # Ensure s3's Codex thread has initialized before testing blank-tab `/quit`
            wait_for(lambda: len([m for m in codex_sent(codex_rec) if m.get("method") == "thread/start"]) >= 2)

            # Focus blank s3 and type `/quit`: purges s3 cleanly without sending `/quit` to Codex
            w.proc.stdin.write(f"show {s3}\n")
            w.proc.stdin.flush()
            wait_for(lambda: u if (u := w.ui()) and u["selected"] == s3 and any(r["session"] == s3 for r in u["rows"]) else None)
            w.type_text("/quit")
            w.key("Return")
            wait_for(lambda: all(r["session"] != s3 for r in conn.call(op="status")["sessions"]) or None)
            assert not any("/quit" in json.dumps(m) for m in codex_sent(codex_rec))

            # Focus conversation-bearing s2 and type `/exit`: hides tab, switches to s1, never sends `/exit`!
            w.proc.stdin.write(f"show {s2}\n")
            w.proc.stdin.flush()
            wait_for(lambda: u if (u := w.ui()) and u["selected"] == s2 and any(r["session"] == s2 for r in u["rows"]) else None)
            w.type_text("/exit")
            w.key("Return")
            wait_for(lambda: (st := status_of(conn, s2)).get("state") == "disconnected" and st.get("tab_closed") is True or None)
            st_full = conn.call(op="status")
            assert v1.validate_response("status", st_full) == []
            assert status_of(conn, s2).get("auto_title") == "지금은 다른 메신저와 대화 가능?"
            assert not any("/exit" in json.dumps(m) for m in codex_sent(codex_rec))

            ui_after = wait_for(lambda: u if (u := w.ui()) and u["selected"] == s1 and all(t["session"] != s2 for t in u["tabs"]["items"]) else None)
            assert [t["session"] for t in ui_after["tabs"]["items"]] == [s1]

            # Switch left sidebar to `[ Chats ]`: s2 is preserved there, and clicking it reopens its tab!
            chats_tab_rect = ui_after["sidebar"]["chats_tab_rect"]
            w.click(*_centre(chats_tab_rect))
            ui_chats = wait_for(
                lambda: u
                if (u := w.ui()) and u["sidebar"]["tab"] == "chats" and any(it.get("open_session") == s2 for it in u["sidebar"]["chats_entries"])
                else None
            )
            s2_item = next(it for it in ui_chats["sidebar"]["chats_entries"] if it.get("open_session") == s2)
            assert "지금은 다른 메신저와 대화 가능?" in s2_item["title"]
            w.click(*_centre(s2_item["rect"]))
            wait_for(lambda: u if (u := w.ui()) and u["selected"] == s2 and any(t["session"] == s2 for t in u["tabs"]["items"]) else None)
            wait_for(lambda: status_of(conn, s2).get("tab_closed") is not True or None)

        # Verify legacy `/exit` auto_title in `sessions.json` is automatically healed from `.seg` on host load
        reg_file = state_dir / "registry" / "sessions.json"
        reg_data = json.loads(reg_file.read_text(encoding="utf-8"))
        for entry in reg_data["sessions"]:
            if entry["session"] == s2:
                entry["auto_title"] = "/exit"
        reg_file.write_text(json.dumps(reg_data), encoding="utf-8")

        host2, _ = start(
            str(state_dir),
            extra=host_args,
            sessions=0,
            producing=0,
            history=0,
        )
        try:
            conn2 = Conn(host2.socket)
            healed = status_of(conn2, s2)
            assert healed.get("auto_title") == "지금은 다른 메신저와 대화 가능?", healed
        finally:
            host2.stop()


def test_toolbar_send_to_session_copy_button_and_selection_invariants():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        extra, _, _ = _dual_provider_args(root, codex_mcp=True)
        with ProductWindow(host_binary=LICO, host_args=extra, host_sessions=0) as w:
            conn = Conn(w.socket)
            acc = account(conn, "Work", "claude")
            s1_res = conn.call(op="session.create", kind="claude", account=acc, name="sender_tab")
            s2_res = conn.call(op="session.create", kind="codex", name="receiver_tab")
            s1, g1 = s1_res["session"], s1_res["generation"]
            s2 = s2_res["session"]

            payload_lines = [
                "# 8단계 검수 결과 2026-11-12T07:15",
                "결정3:SLM-EMR\t승인\t단순 대치 파일",
                "$ /bin/bash -lc 'lico session list'",
                "[exit 1]",
            ]
            assert conn.call(
                op="input",
                session=s1,
                generation=g1,
                request_id="turn-1",
                text="say:" + json.dumps(payload_lines, ensure_ascii=False),
            )["ok"] == 1

            w.proc.stdin.write(f"show {s1}\n")
            w.proc.stdin.flush()
            reader = PixelReader(w.xvfb.display)
            try:
                ui0 = wait_for(
                    lambda: u
                    if (u := w.ui())
                    and u["selected"] == s1
                    and len(u["rows"]) == 2
                    and len(u["layout"]["lines"]) >= 5
                    and any(b["id"] == "send_to" for b in u["toolbar"]["buttons"])
                    else None
                )
                send_btn0 = next(b for b in ui0["toolbar"]["buttons"] if b["id"] == "send_to")
                assert send_btn0["label"] == "Send -> Session (1)"
                assert not any(b["id"] == "copy" for b in ui0["toolbar"]["buttons"])
                assert reader.selection_owner("CLIPBOARD") == 0 and reader.selection_owner("PRIMARY") == 0

                # Drag mouse across transcript lines: mouse release auto-copies to CLIPBOARD, and lights up `Copy` + `Send -> Session (1)`!
                lines = ui0["layout"]["lines"]
                y0 = (lines[1][0] + lines[1][1]) / 2.0
                y1 = (lines[2][0] + lines[2][1]) / 2.0
                wid = w.window_id()
                subprocess.run(
                    ["xdotool", "mousemove", "--window", wid, "520", str(int(y0)), "mousedown", "1", "mousemove", "--window", wid, "520", str(int(y1)), "mouseup", "1"],
                    env=w.env,
                    check=True,
                )
                ui_sel = wait_for(
                    lambda: u
                    if (u := w.ui()) and u["selection"] and any(b["id"] == "copy" for b in u["toolbar"]["buttons"])
                    else None
                )
                wait_for(lambda: reader.selection_owner("CLIPBOARD") != 0 or None)
                assert reader.selection_text("CLIPBOARD") == ui_sel["selection"]
                assert reader.selection_owner("PRIMARY") == 0
                copied_after_drag = ui_sel["copied"]
                copy_btn = next(b for b in ui_sel["toolbar"]["buttons"] if b["id"] == "copy")
                send_btn = next(b for b in ui_sel["toolbar"]["buttons"] if b["id"] == "send_to")
                assert copy_btn["enabled"] is True and send_btn["enabled"] is True

                # Click toolbar `Copy`: owns CLIPBOARD with exact UTF-8 selection, leaves PRIMARY untouched
                w.click(*_centre(copy_btn["rect"]))
                wait_for(lambda: w.ui()["copied"] > copied_after_drag or None)
                assert reader.selection_text("CLIPBOARD") == ui_sel["selection"]
                assert reader.selection_owner("PRIMARY") == 0

                # Click toolbar `Send -> Session (1)`: opens Send panel with s2 pre-selected and sends on Return!
                w.click(*_centre(send_btn["rect"]))
                wait_for(lambda: (p := w.ui().get("panel")) and p.get("title") == "Send to session" or None)
                w.key("Return")
                wait_for(lambda: w.ui().get("panel") is None and "Sent to" in w.ui().get("header", "") or None)
                s2_status = wait_for(lambda: st if (st := status_of(conn, s2)) and st.get("seq", -1) >= 1 else None)
                assert s2_status["state"] == "running"
            finally:
                reader.close()


def test_claude_get_usage_quota_and_symlink_dir_and_file_navigation():
    import os

    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        work = root / "home_view"
        target_dir = root / "external_storage" / "Pictures"
        target_dir.mkdir(parents=True, exist_ok=True)
        work.mkdir(parents=True, exist_ok=True)
        (target_dir / "album_note.txt").write_text("Inside symlinked Pictures folder!\n", encoding="utf-8")
        real_script = root / "external_storage" / "view_cascade.sh"
        real_script.write_text("#!/bin/sh\necho symlink-file-preview-ok\n", encoding="utf-8")
        os.symlink(target_dir, work / "Pictures")
        os.symlink(real_script, work / "view_cascade.sh")

        extra, _, _ = _dual_provider_args(root, codex_mcp=True)
        # Configure Claude simulator with plan `get_usage` rate limits: 18.0% 7d used -> 82% left, 4.0% 5h used -> 96% left
        extra += ["--claude-arg", "--usage", "--claude-arg", "18.0:4.0"]

        with ProductWindow(host_binary=LICO, host_args=extra, host_sessions=0) as w:
            conn = Conn(w.socket)
            c_acc = account(conn, "ClaudePlan", "claude")
            created = conn.call(op="session.create", kind="claude", account=c_acc, cwd=str(work), name="claude_main")
            assert created["ok"] == 1
            sid, gen = created["session"], created["generation"]

            # 1) Without sending any user message or `rate_limit_event`, `get_usage` immediately populates Claude 5h & Wk quota!
            st_ready = wait_for(
                lambda: st
                if (st := status_of(conn, sid)).get("account_usage")
                and st["account_usage"].get("weekly_remaining_pct") == 82
                and st["account_usage"].get("short_remaining_pct") == 96
                else None
            )
            assert st_ready["account_usage"]["weekly_resets_at"] == 1_794_902_400
            assert st_ready["account_usage"]["short_resets_at"] == 1_794_492_000

            w.proc.stdin.write("load\n")
            w.proc.stdin.flush()
            ui_tabs = wait_for(
                lambda: u
                if (u := w.ui())
                and any(t["session"] == sid for t in u["tabs"]["items"])
                and any(r["session"] == sid and r["weekly_remaining_pct"] == 82 and r["short_remaining_pct"] == 96 for r in u["rows"])
                else None
            )
            tab_item = next(t for t in ui_tabs["tabs"]["items"] if t["session"] == sid)
            w.click((tab_item["rect"][0] + tab_item["close_rect"][0]) / 2.0, (tab_item["rect"][1] + tab_item["rect"][3]) / 2.0)
            ui0 = wait_for(
                lambda: u
                if (u := w.ui())
                and u["selected"] == sid
                and u["sidebar"]["cwd"] == str(work.resolve())
                and any(e["name"] == "Pictures" for e in u["sidebar"]["entries"])
                and "5h:96%" in u["telemetry"]["quota_text"]
                and "Wk:82%" in u["telemetry"]["quota_text"]
                else None
            )
            assert "7d 82% left" in ui0["header"], ui0["header"]

            # 2) Update simulated `get_usage` state in a turn and verify post-turn `result` refreshes Claude quota automatically!
            assert conn.call(op="input", session=sid, generation=gen, request_id="u-upd", text="usage:35.0:11.0")["ok"] == 1
            wait_for(
                lambda: (st := status_of(conn, sid)).get("account_usage", {}).get("weekly_remaining_pct") == 65
                and st["account_usage"].get("short_remaining_pct") == 89
                or None
            )

            # 3) Symlink file `view_cascade.sh` is classified as `"file"` and clicking it opens inline preview!
            entries0 = {e["name"]: e for e in ui0["sidebar"]["entries"]}
            assert entries0["Pictures"]["kind"] == "dir"
            assert entries0["view_cascade.sh"]["kind"] == "file"
            w.click(*_centre(entries0["view_cascade.sh"]["rect"]))
            ui_prev = wait_for(
                lambda: u
                if (u := w.ui()) and u.get("panel") and any("symlink-file-preview-ok" in line for line in u["panel"].get("body", []))
                else None
            )
            assert "view_cascade.sh" in ui_prev["panel"]["title"]
            w.key("Escape")
            wait_for(lambda: w.ui().get("panel") is None or None)

            # 4) Clicking symlink directory `Pictures` enters `target_dir` and lists `album_note.txt`!
            ui1 = wait_for(lambda: u if (u := w.ui()) and any(e["name"] == "Pictures" for e in u["sidebar"]["entries"]) else None)
            pic_entry = next(e for e in ui1["sidebar"]["entries"] if e["name"] == "Pictures")
            w.click(*_centre(pic_entry["rect"]))
            ui_in_sym = wait_for(
                lambda: u
                if (u := w.ui())
                and u["sidebar"]["cwd"] == str(target_dir.resolve())
                and any(e["name"] == "album_note.txt" for e in u["sidebar"]["entries"])
                else None
            )
            note_entry = next(e for e in ui_in_sym["sidebar"]["entries"] if e["name"] == "album_note.txt")
            w.click(*_centre(note_entry["rect"]))
            wait_for(
                lambda: (p := w.ui().get("panel"))
                and any("Inside symlinked Pictures folder!" in line for line in p.get("body", []))
                or None
            )
