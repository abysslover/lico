"""M07-P05: approval banner & button placement, hidden unsolicited peer turns with automatic
peer-reply routing back to the caller, mouse-selection auto-copy to CLIPBOARD, universal
copyable right-pane panels, and persistent `[ Directories ]` Back/Forward history + Bookmarks."""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir
from lico_protocol import v1
from tests.account_helpers import account
from tests.test_claude_adapter import FIXTURE as CLAUDE_FIXTURE, SIM as CLAUDE_SIM, records, status_of
from tests.test_codex_adapter import FIXTURE as CODEX_FIXTURE, SIM as CODEX_SIM, usual_umask, wait_for
from tests.test_first_view import PixelReader


def _centre(rect: list[float]) -> tuple[float, float]:
    return (rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0


def _tab_click_xy(tab: dict) -> tuple[float, float]:
    return (tab["rect"][0] + tab["close_rect"][0]) / 2.0, (tab["rect"][1] + tab["rect"][3]) / 2.0


def _dual_args(root: Path, state_dir: Path) -> list[str]:
    claude_rec = root / "claude_received.jsonl"
    codex_rec = root / "codex_received.jsonl"
    return [
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


def test_approved_peer_send_hides_unsolicited_turn_in_receiver_and_auto_routes_reply_to_caller():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        state_dir = root / "state"
        host_args = _dual_args(root, state_dir)
        log_dir = state_dir / "log"

        with ProductWindow(host_binary=LICO, host_args=host_args, host_sessions=0) as w:
            conn = Conn(w.socket)
            c_acc = account(conn, "ClaudeAcc", "claude")
            x_acc = account(conn, "CodexAcc", "codex")
            c_made = conn.call(op="session.create", kind="claude", account=c_acc, mode="auto", name="ClaudeMain")
            x_made = conn.call(op="session.create", kind="codex", account=x_acc, mode="auto", name="CodexWorker")
            assert c_made["ok"] == 1 and x_made["ok"] == 1
            s_claude, g_claude = c_made["session"], c_made["generation"]
            s_codex, g_codex = x_made["session"], x_made["generation"]

            # 1. Human asks a direct question in CodexWorker: must remain visible in CodexWorker's chat view.
            assert conn.call(
                op="input",
                session=s_codex,
                generation=g_codex,
                request_id="human-1",
                text="사용자가 직접 질문한 항목",
            )["ok"] == 1
            wait_for(lambda: "[turn completed]" in records(log_dir, s_codex))

            # 2. Switch GUI to ClaudeMain and trigger a long Korean/ASCII `mcp__lico__send` to CodexWorker.
            w.proc.stdin.write("load\n")
            w.proc.stdin.flush()
            u0 = wait_for(lambda: (u := w.ui()) and len(u["tabs"]["items"]) == 2 and u)
            tab_c = next(t for t in u0["tabs"]["items"] if t["session"] == s_claude)
            w.click(*_tab_click_xy(tab_c))
            wait_for(lambda: w.ui()["selected"] == s_claude)

            greeting = "안녕하세요 Codex! 저는 ClaudeMain 세션입니다. 사용자의 요청으로 상태 확인 인사를 보냅니다."
            assert conn.call(
                op="input",
                session=s_claude,
                generation=g_claude,
                request_id="c-send-1",
                text=f"tool:send {json.dumps({'to': s_codex, 'text': greeting}, ensure_ascii=False)}",
            )["ok"] == 1

            # 3. Verify centered modal notification window & `Allow` / `Deny` button geometry, then click `Allow` once.
            u_card = wait_for(
                lambda: (u := w.ui())
                and u["approval_bar"]
                and u["layout"]["allow"] is not None
                and u["layout"]["deny"] is not None
                and u["card_ready"] is True
                and u
            )
            allow_r = u_card["layout"]["allow"]
            deny_r = u_card["layout"]["deny"]
            win_w, win_h = u_card["layout"]["window_size"]
            assert u_card["layout"]["list_w"] < allow_r[0] < allow_r[2] < deny_r[0] < deny_r[2] <= win_w
            assert win_h * 0.25 <= allow_r[1] < allow_r[3] <= win_h * 0.80, (allow_r, win_h)

            w.click(*_centre(allow_r))

            # 4. CodexWorker runs the peer turn AND its assistant reply is auto-delivered back to ClaudeMain
            #    visibly inside a teal peer card in ClaudeMain's chat window (1 event-driven turn, no loop)!
            wait_for(
                lambda: any("[peer reply from 'CodexWorker'" in r for r in records(log_dir, s_claude))
                and records(log_dir, s_claude).count("[turn completed]") >= 3
            )
            assert status_of(conn, s_codex).get("approvals") is None
            assert status_of(conn, s_claude).get("approvals") is None

            # Verify ClaudeMain's chat window visibly shows:
            # (a) default-collapsed `▸ [tool] mcp__lico__send` pill (which expands on click to show `> [to <id7>] <greeting>`),
            # (b) incoming `>> [peer reply from 'CodexWorker' ...]` + `>> echo: ...` peer reply lines,
            # (c) Claude's 1 event-driven response turn, while hiding internal `>> [from lico | ...]` host notices!
            u_claude_after = wait_for(
                lambda: (u := w.ui())
                and any(ln.startswith("\u25b8 [tool] mcp__lico__send") for ln in u["shown_lines"])
                and any(ln.startswith(">> [peer reply from 'CodexWorker'") for ln in u["shown_lines"])
                and any(ln.startswith(">> echo: [from session 'ClaudeMain'") for ln in u["shown_lines"])
                and any("echo: [peer reply from 'CodexWorker'" in ln for ln in u["shown_lines"])
                and u
            )
            assert not any(f"> [to {s_codex[:7]}] {greeting}" in ln for ln in u_claude_after["shown_lines"]), u_claude_after["shown_lines"]
            assert not any(ln.startswith(">> [from lico |") for ln in u_claude_after["shown_lines"])
            # Click the collapsed `▸ [tool] mcp__lico__send` pill to expand and reveal outgoing payload, then collapse again!
            pill_idx = next(i for i, ln in enumerate(u_claude_after["shown_lines"]) if ln.startswith("\u25b8 [tool] mcp__lico__send"))
            pill_box = u_claude_after["layout"]["lines"][pill_idx]
            w.click(440, (pill_box[0] + pill_box[1]) / 2.0)
            u_claude_exp = wait_for(
                lambda: (u := w.ui())
                and any(f"> [to {s_codex[:7]}] {greeting}" in ln for ln in u["shown_lines"])
                and u
            )
            assert any(ln.startswith("\u25be [tool] mcp__lico__send") for ln in u_claude_exp["shown_lines"])
            head_idx = next(i for i, ln in enumerate(u_claude_exp["shown_lines"]) if ln.startswith("\u25be [tool] mcp__lico__send"))
            head_box = u_claude_exp["layout"]["lines"][head_idx]
            w.click(440, (head_box[0] + head_box[1]) / 2.0)
            wait_for(lambda: any(ln.startswith("\u25b8 [tool] mcp__lico__send") for ln in w.ui()["shown_lines"]))
            # Strictly 1 round-trip turn on CodexWorker (human-1 + 1 peer turn = 2 completed turns, never an infinite loop)
            assert records(log_dir, s_codex).count("[turn completed]") == 2

            # 5. Session-persistent `Allow`: sending a SECOND message from ClaudeMain -> CodexWorker goes through
            #    IMMEDIATELY without popping a second approval modal!
            second_msg = "두 번째 확인 인사입니다!"
            assert conn.call(
                op="input",
                session=s_claude,
                generation=g_claude,
                request_id="c-send-2",
                text=f"tool:send {json.dumps({'to': s_codex, 'text': second_msg}, ensure_ascii=False)}",
            )["ok"] == 1
            wait_for(
                lambda: any(f">> {second_msg}" in r for r in records(log_dir, s_codex))
                and records(log_dir, s_codex).count("[turn completed]") == 3
            )
            assert status_of(conn, s_claude).get("approvals") is None
            assert not w.ui()["approval_bar"]
            wait_for(
                lambda: (u := w.ui())
                and sum(1 for ln in u["shown_lines"] if ln.startswith(">> [peer reply from 'CodexWorker'")) >= 2
                and u
            )

            # 6. Switch GUI to CodexWorker: unsolicited peer turns are HIDDEN in normal chat view,
            #    while the earlier human question & answer remain visible; also verify a background-tab
            #    approval modal still surfaces when viewing CodexWorker!
            tab_x = next(t for t in w.ui()["tabs"]["items"] if t["session"] == s_codex)
            w.click(*_tab_click_xy(tab_x))
            u_codex = wait_for(
                lambda: (u := w.ui())
                and u["selected"] == s_codex
                and any("사용자가 직접 질문한 항목" in ln for ln in u["shown_lines"])
                and u
            )
            assert not any("안녕하세요 Codex!" in ln for ln in u_codex["shown_lines"]), u_codex["shown_lines"]
            assert not any(second_msg in ln for ln in u_codex["shown_lines"]), u_codex["shown_lines"]
            assert not any("[from session 'ClaudeMain'" in ln for ln in u_codex["shown_lines"]), u_codex["shown_lines"]
            # Durable `.seg` still holds the peer turn, and `Ctrl+Shift+E` reveals it on demand!
            assert any("안녕하세요 Codex!" in r for r in records(log_dir, s_codex))
            w.key("ctrl+shift+e")
            u_codex_ev = wait_for(lambda: (u := w.ui()) and u["settings"]["show_events"] is True and u)
            assert any("안녕하세요 Codex!" in ln for ln in u_codex_ev["shown_lines"])
            assert any("[peer turn started]" in ln for ln in u_codex_ev["shown_lines"])
            w.key("ctrl+shift+e")
            wait_for(lambda: w.ui()["settings"]["show_events"] is False)

            # Background-tab modal notification: create a 3rd session `CodexExtra`, stay focused on `CodexWorker`,
            # and trigger `ClaudeMain -> CodexExtra`; the centered modal surfaces immediately over `CodexWorker`!
            x2_made = conn.call(op="session.create", kind="codex", account=x_acc, mode="auto", name="CodexExtra")
            assert x2_made["ok"] == 1
            s_extra = x2_made["session"]
            u_tabs3 = wait_for(lambda: (u := w.ui()) and len(u["tabs"]["items"]) == 3 and u)
            w.click(*_tab_click_xy(next(t for t in u_tabs3["tabs"]["items"] if t["session"] == s_codex)))
            wait_for(lambda: w.ui()["selected"] == s_codex)
            assert conn.call(
                op="input",
                session=s_claude,
                generation=g_claude,
                request_id="c-send-bg",
                text=f"tool:send {json.dumps({'to': s_extra, 'text': '백그라운드 탭 승인 테스트'}, ensure_ascii=False)}",
            )["ok"] == 1
            u_bg_modal = wait_for(
                lambda: (u := w.ui())
                and u["selected"] == s_codex
                and u["approval_bar"]
                and "CodexExtra" in u["approval_bar"]
                and u["card_ready"] is True
                and u
            )
            w.click(*_centre(u_bg_modal["layout"]["deny"]))
            wait_for(lambda: not w.ui()["approval_bar"])


def test_mouse_drag_auto_copies_and_all_right_pane_panels_are_copyable_while_keeping_click_activation():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        state_dir = root / "state"
        work = root / "proj"
        work.mkdir(mode=0o700, parents=True, exist_ok=True)
        (work / "guide.md").write_text("Alpha preview line 1\nBeta preview line 2\nGamma preview line 3\n", encoding="utf-8")
        host_args = _dual_args(root, state_dir)

        with ProductWindow(host_binary=LICO, host_args=host_args, host_sessions=0) as w:
            reader = PixelReader(w.xvfb.display)
            try:
                conn = Conn(w.socket)
                acc = account(conn, "CopyAcc", "claude")
                x_acc = account(conn, "CopyCodex", "codex")
                s1 = conn.call(op="session.create", kind="claude", account=acc, mode="auto", cwd=str(work), name="One")["session"]
                _s2 = conn.call(op="session.create", kind="codex", account=x_acc, mode="auto", cwd=str(work), name="Two")["session"]

                w.proc.stdin.write("load\n")
                w.proc.stdin.flush()
                u0 = wait_for(
                    lambda: (u := w.ui())
                    and len(u["tabs"]["items"]) == 2
                    and u
                )
                tab1 = next(t for t in u0["tabs"]["items"] if t["session"] == s1)
                w.click(*_tab_click_xy(tab1))
                u1 = wait_for(
                    lambda: (u := w.ui())
                    and u["selected"] == s1
                    and any(e["name"] == "guide.md" for e in u["sidebar"]["entries"])
                    and u
                )
                assert reader.selection_owner("CLIPBOARD") == 0 and reader.selection_owner("PRIMARY") == 0
                wid = w.window_id()

                # 1. Inline sidebar file preview (`Step::Files` preview): mouse drag selects & auto-copies, toolbar `Copy` keeps panel open!
                entry = next(e for e in u1["sidebar"]["entries"] if e["name"] == "guide.md")
                w.click(*_centre(entry["rect"]))
                u_prev = wait_for(
                    lambda: (u := w.ui())
                    and u["panel"] is not None
                    and u["panel"]["files"] is not None
                    and u["panel"]["files"]["mode"] == "preview"
                    and len(u["layout"]["lines"]) >= 4
                    and u
                )
                plines = u_prev["layout"]["lines"]
                py0 = int((plines[1][0] + plines[1][1]) / 2.0)
                py1 = int((plines[2][0] + plines[2][1]) / 2.0)
                subprocess.run(
                    ["xdotool", "mousemove", "--window", wid, "560", str(py0), "mousedown", "1", "mousemove", "--window", wid, "560", str(py1), "mouseup", "1"],
                    env=w.env,
                    check=True,
                )
                u_prev_sel = wait_for(
                    lambda: (u := w.ui())
                    and u["selection"]
                    and "Alpha preview line 1" in u["selection"]
                    and any(b["id"] == "copy" and b["rect"] is not None for b in u["toolbar"]["buttons"])
                    and u
                )
                wait_for(lambda: reader.selection_owner("CLIPBOARD") != 0 or None)
                assert reader.selection_text("CLIPBOARD") == u_prev_sel["selection"]
                assert reader.selection_owner("PRIMARY") == 0
                copy_btn = next(b for b in u_prev_sel["toolbar"]["buttons"] if b["id"] == "copy")
                copied_before = u_prev_sel["copied"]
                w.click(*_centre(copy_btn["rect"]))
                u_after_btn = wait_for(lambda: (u := w.ui()) and u["copied"] > copied_before and u)
                assert u_after_btn["panel"] is not None and u_after_btn["panel"]["files"]["mode"] == "preview"
                w.key("Escape")
                u_after_esc = wait_for(
                    lambda: (u := w.ui())
                    and u["panel"] is None
                    and u["editor"]["rect"][2] > u["editor"]["rect"][0]
                    and u
                )

                # 2. Prompt editor drag auto-copy (also populates draft text so `Send -> Session (1)` is enabled):
                ed_rect = u_after_esc["editor"]["rect"]
                w.click((ed_rect[0] + ed_rect[2]) / 2.0, (ed_rect[1] + ed_rect[3]) / 2.0)
                subprocess.run(["xdotool", "type", "--delay", "5", "prompt_auto_copy_ok"], env=w.env, check=True)
                u_typed = wait_for(
                    lambda: (u := w.ui())
                    and "prompt_auto_copy_ok" in u["input"]
                    and u["editor"]["caret"] is not None
                    and u["editor"]["caret"][0] > ed_rect[0] + 60
                    and u
                )
                ed_rect = u_typed["editor"]["rect"]
                ed_end_x = int(u_typed["editor"]["caret"][0] + 24)
                ed_y = int((ed_rect[1] + ed_rect[3]) / 2.0)
                subprocess.run(
                    ["xdotool", "mousemove", "--window", wid, str(int(ed_rect[0] + 4)), str(ed_y), "mousedown", "1", "mousemove", "--window", wid, str(ed_end_x), str(ed_y), "mouseup", "1"],
                    env=w.env,
                    check=True,
                )
                u_ed_sel = wait_for(
                    lambda: (u := w.ui())
                    and u["editor"]["selection"]
                    and len(u["editor"]["selection"]) >= 8
                    and u
                )
                ed_copied_str = u_ed_sel["editor"]["selection"]
                wait_for(lambda: reader.selection_text("CLIPBOARD") == ed_copied_str or None)
                assert reader.selection_owner("PRIMARY") == 0

                # 3. `Send to session` panel (`Step::Send`): `< 4 px` single click still toggles row, while mouse drag selects & auto-copies!
                u_chat = wait_for(
                    lambda: (u := w.ui())
                    and any(b["id"] == "send_to" and b["enabled"] and b["rect"] is not None for b in u["toolbar"]["buttons"])
                    and u
                )
                send_btn = next(b for b in u_chat["toolbar"]["buttons"] if b["id"] == "send_to")
                w.click(*_centre(send_btn["rect"]))
                u_send = wait_for(
                    lambda: (u := w.ui())
                    and u["panel"] is not None
                    and u["panel"]["title"].startswith("Send to session")
                    and len(u["layout"]["lines"]) >= 6
                    and u
                )
                # Single stationary click on `[ ] C  Whole conversation` activates without overwriting CLIPBOARD!
                target_row_i = next(i for i, t in enumerate(u_send["panel"]["body"]) if t.lstrip().startswith("[ ] C "))
                t_line = next(l for l in u_send["layout"]["lines"] if l[2] == target_row_i)
                w.click(540, (t_line[0] + t_line[1]) / 2.0)
                u_toggled = wait_for(
                    lambda: (u := w.ui())
                    and any(t.lstrip().startswith("[x] C ") for t in u["panel"]["body"])
                    and u
                )
                assert reader.selection_text("CLIPBOARD") == ed_copied_str

                # Drag across `What to send` .. `To` in `Send to session` -> selects & auto-copies to CLIPBOARD!
                slines = u_toggled["layout"]["lines"]
                sy0 = int((slines[0][0] + slines[0][1]) / 2.0)
                sy1 = int((slines[3][0] + slines[3][1]) / 2.0)
                subprocess.run(
                    ["xdotool", "mousemove", "--window", wid, "560", str(sy0), "mousedown", "1", "mousemove", "--window", wid, "560", str(sy1), "mouseup", "1"],
                    env=w.env,
                    check=True,
                )
                u_send_sel = wait_for(lambda: (u := w.ui()) and u["selection"] and "What to send" in u["selection"] and u)
                wait_for(lambda: reader.selection_text("CLIPBOARD") == u_send_sel["selection"] or None)
                assert reader.selection_owner("PRIMARY") == 0
                w.key("Escape")
                wait_for(lambda: w.ui()["panel"] is None)

                # 4. `Accounts` panel (`Step::Accounts`) drag auto-copy:
                u_chat2 = wait_for(
                    lambda: (u := w.ui())
                    and any(b["id"] == "accounts" and b["rect"] is not None for b in u["toolbar"]["buttons"])
                    and u
                )
                acc_btn = next(b for b in u_chat2["toolbar"]["buttons"] if b["id"] == "accounts")
                w.click(*_centre(acc_btn["rect"]))
                u_acc = wait_for(
                    lambda: (u := w.ui())
                    and u["panel"] is not None
                    and u["panel"]["title"].startswith("Accounts")
                    and len(u["layout"]["lines"]) >= 2
                    and u
                )
                alines = u_acc["layout"]["lines"]
                ay0 = int((alines[0][0] + alines[0][1]) / 2.0)
                ay1 = int((alines[1][0] + alines[1][1]) / 2.0)
                subprocess.run(
                    ["xdotool", "mousemove", "--window", wid, "560", str(ay0), "mousedown", "1", "mousemove", "--window", wid, "560", str(ay1), "mouseup", "1"],
                    env=w.env,
                    check=True,
                )
                u_acc_sel = wait_for(lambda: (u := w.ui()) and u["selection"] and "CopyAcc" in u["selection"] and u)
                wait_for(lambda: reader.selection_text("CLIPBOARD") == u_acc_sel["selection"] or None)
                w.key("Escape")
                wait_for(lambda: w.ui()["panel"] is None)
            finally:
                reader.close()


def test_directory_sidebar_history_back_forward_and_bookmarks_persist_across_gui_and_host_restart():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        state_dir = root / "state"
        ws_dir = root / "ws"
        dir_alpha = ws_dir / "alpha"
        dir_beta = ws_dir / "beta"
        dir_gamma = ws_dir / "gamma"
        for d in (dir_alpha, dir_beta, dir_gamma):
            d.mkdir(mode=0o700, parents=True, exist_ok=True)
        (dir_alpha / "a.txt").write_text("alpha\n", encoding="utf-8")
        for idx in range(36):
            (dir_beta / f"file_{idx:02d}.txt").write_text(f"beta {idx}\n", encoding="utf-8")
        (dir_gamma / "g.txt").write_text("gamma\n", encoding="utf-8")

        host_args = _dual_args(root, state_dir)

        with ProductWindow(host_binary=LICO, host_args=host_args, host_sessions=0) as w:
            conn = Conn(w.socket)
            acc = account(conn, "NavAcc", "claude")
            s_ws = conn.call(op="session.create", kind="claude", account=acc, mode="auto", cwd=str(ws_dir), name="NavChat")["session"]

            w.proc.stdin.write("load\n")
            w.proc.stdin.flush()
            u0 = wait_for(lambda: (u := w.ui()) and len(u["tabs"]["items"]) == 1 and u)
            w.click(*_tab_click_xy(u0["tabs"]["items"][0]))
            u_ws = wait_for(
                lambda: (u := w.ui())
                and u["selected"] == s_ws
                and u["sidebar"]["cwd"] == str(ws_dir.resolve())
                and u["sidebar"]["loading"] is None
                and u
            )

            # Navigate: ws -> alpha -> (parent ws) -> beta (p.1) -> beta (p.2) -> gamma (p.1)
            ent_alpha = next(e for e in u_ws["sidebar"]["entries"] if e["name"] == "alpha")
            w.click(*_centre(ent_alpha["rect"]))
            u_a = wait_for(lambda: (u := w.ui()) and u["sidebar"]["cwd"] == str(dir_alpha.resolve()) and u["sidebar"]["loading"] is None and u)
            assert u_a["sidebar"]["can_back"] is True and u_a["sidebar"]["can_forward"] is False

            w.click(*_centre(u_a["sidebar"]["parent_rect"]))
            u_ws2 = wait_for(lambda: (u := w.ui()) and u["sidebar"]["cwd"] == str(ws_dir.resolve()) and u["sidebar"]["loading"] is None and u)
            ent_beta = next(e for e in u_ws2["sidebar"]["entries"] if e["name"] == "beta")
            w.click(*_centre(ent_beta["rect"]))
            u_b1 = wait_for(lambda: (u := w.ui()) and u["sidebar"]["cwd"] == str(dir_beta.resolve()) and u["sidebar"]["loading"] is None and u)
            assert u_b1["sidebar"]["total_pages"] >= 2 and u_b1["sidebar"]["page"] == 1

            # Advance to beta Page 2 and bookmark `beta (p.2)` via `☆`!
            w.click(*_centre(u_b1["sidebar"]["next_rect"]))
            u_b2 = wait_for(lambda: (u := w.ui()) and u["sidebar"]["page"] == 2 and u)
            assert u_b2["sidebar"]["bookmarked"] is False
            w.click(*_centre(u_b2["sidebar"]["bookmark_toggle_rect"]))
            u_b2_bm = wait_for(lambda: (u := w.ui()) and u["sidebar"]["bookmarked"] is True and len(u["sidebar"]["bookmarks"]) == 1 and u)
            assert u_b2_bm["sidebar"]["bookmarks"][0]["label"] == "beta (p.2)"
            assert u_b2_bm["sidebar"]["bookmarks"][0]["page"] == 2

            # Go parent -> gamma (p.1) and bookmark `gamma` too!
            w.click(*_centre(u_b2_bm["sidebar"]["parent_rect"]))
            u_ws3 = wait_for(lambda: (u := w.ui()) and u["sidebar"]["cwd"] == str(ws_dir.resolve()) and u["sidebar"]["loading"] is None and u)
            ent_gamma = next(e for e in u_ws3["sidebar"]["entries"] if e["name"] == "gamma")
            w.click(*_centre(ent_gamma["rect"]))
            u_g = wait_for(lambda: (u := w.ui()) and u["sidebar"]["cwd"] == str(dir_gamma.resolve()) and u["sidebar"]["loading"] is None and u)
            w.click(*_centre(u_g["sidebar"]["bookmark_toggle_rect"]))
            u_g_bm = wait_for(lambda: (u := w.ui()) and len(u["sidebar"]["bookmarks"]) == 2 and u)

            # Step Back (`<`) twice: gamma -> ws -> beta (p.2), then once more -> beta (p.1)!
            w.click(*_centre(u_g_bm["sidebar"]["back_rect"]))
            wait_for(lambda: (u := w.ui()) and u["sidebar"]["cwd"] == str(ws_dir.resolve()) and u["sidebar"]["loading"] is None and u)
            w.click(*_centre(w.ui()["sidebar"]["back_rect"]))
            u_back_b2 = wait_for(
                lambda: (u := w.ui())
                and u["sidebar"]["cwd"] == str(dir_beta.resolve())
                and u["sidebar"]["loading"] is None
                and u["sidebar"]["page"] == 2
                and u
            )
            assert u_back_b2["sidebar"]["can_back"] is True and u_back_b2["sidebar"]["can_forward"] is True

            # Test `Alt+Left` (back to beta p.1) and `Alt+Right` (forward to beta p.2)!
            subprocess.run(["xdotool", "key", "alt+Left"], env=w.env, check=True)
            wait_for(lambda: w.ui()["sidebar"]["page"] == 1)
            subprocess.run(["xdotool", "key", "alt+Right"], env=w.env, check=True)
            wait_for(lambda: w.ui()["sidebar"]["page"] == 2)

            # Verify protocol schema conformance of `status` settings with `dir_history` + `dir_bookmarks`!
            st = wait_for(
                lambda: (s := conn.call(op="status"))
                and "dir_history" in s["settings"]
                and len(s["settings"].get("dir_bookmarks", [])) == 2
                and s
            )
            assert v1.validate_response("status", st) == []

        # Restart BOTH `lico host` and `lico gui` against the same `state_dir`!
        with ProductWindow(host_binary=LICO, host_args=host_args, host_sessions=0) as w2:
            w2.proc.stdin.write("load\n")
            w2.proc.stdin.flush()
            u_restored = wait_for(
                lambda: (u := w2.ui())
                and u["sidebar"]["cwd"] == str(dir_beta.resolve())
                and u["sidebar"]["loading"] is None
                and u["sidebar"]["page"] == 2
                and u
            )
            # Both Back stack (`beta p.1`, ...) AND Forward stack (`ws`, `gamma`) survived the full restart!
            assert u_restored["sidebar"]["can_back"] is True
            assert u_restored["sidebar"]["can_forward"] is True
            assert u_restored["sidebar"]["bookmarked"] is True
            assert len(u_restored["sidebar"]["bookmarks"]) == 2

            # Step Forward (`>`) twice after restart -> reaches `gamma`!
            w2.click(*_centre(u_restored["sidebar"]["forward_rect"]))
            wait_for(lambda: (u := w2.ui()) and u["sidebar"]["cwd"] == str(ws_dir.resolve()) and u["sidebar"]["loading"] is None and u)
            w2.click(*_centre(w2.ui()["sidebar"]["forward_rect"]))
            u_fwd_g = wait_for(lambda: (u := w2.ui()) and u["sidebar"]["cwd"] == str(dir_gamma.resolve()) and u["sidebar"]["loading"] is None and u)
            assert u_fwd_g["sidebar"]["can_forward"] is False

            # Open `Marks (2) v` popup, click `beta (p.2)` -> jumps straight to `beta` Page 2!
            w2.click(*_centre(u_fwd_g["sidebar"]["bookmarks_btn_rect"]))
            u_pop = wait_for(lambda: (u := w2.ui()) and u["sidebar"]["bookmarks_popup_open"] is True and u)
            bm_beta = next(b for b in u_pop["sidebar"]["bookmarks"] if b["label"] == "beta (p.2)")
            w2.click(*_centre(bm_beta["rect"]))
            u_jumped = wait_for(
                lambda: (u := w2.ui())
                and u["sidebar"]["bookmarks_popup_open"] is False
                and u["sidebar"]["cwd"] == str(dir_beta.resolve())
                and u["sidebar"]["loading"] is None
                and u["sidebar"]["page"] == 2
                and u
            )

            # Open `Marks (2) v` popup again and click `×` (`remove_rect`) on `gamma` -> deletes & persists!
            w2.click(*_centre(u_jumped["sidebar"]["bookmarks_btn_rect"]))
            u_pop2 = wait_for(lambda: (u := w2.ui()) and u["sidebar"]["bookmarks_popup_open"] is True and u)
            bm_gamma = next(b for b in u_pop2["sidebar"]["bookmarks"] if b["label"] == "gamma")
            w2.click(*_centre(bm_gamma["remove_rect"]))
            wait_for(lambda: len(w2.ui()["sidebar"]["bookmarks"]) == 1)
