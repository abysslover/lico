"""M04-P01: direct left-sidebar session switching from open panels (`About Lico`, `Keys`, `Rename`),
derived session titles (`auto_title`), active-first reverse-chronological sorting (`last_active_at`),
working directory context in header (`dir: ...`), and clickable primary-action Toolbar."""

from __future__ import annotations

import subprocess
import time
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir
from lico_protocol import v1
from tests.account_helpers import account
from tests.test_claude_adapter import new_claude, status_of
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_new_session_screen import claude_args


def _centre(rect: list[float]) -> tuple[float, float]:
    return (rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0


def _fast_key(w: ProductWindow, *keys: str) -> None:
    subprocess.run(["xdotool", "key", *keys], env=w.env, check=True)


def _fast_type(w: ProductWindow, text: str) -> None:
    subprocess.run(["xdotool", "type", "--delay", "2", text], env=w.env, check=True)


def _reload(w: ProductWindow) -> None:
    w.proc.stdin.write("load\n")
    w.proc.stdin.flush()


def _row_centre(w: ProductWindow, sid: str) -> tuple[float, float]:
    u = wait_for(lambda: (u := w.ui()) and any(r[2] == sid for r in u["layout"]["rows"]) and u)
    y0, y1, _ = next(r for r in u["layout"]["rows"] if r[2] == sid)
    return 100.0, (y0 + y1) / 2.0


def _tool_btn(u: dict, btn_id: str) -> dict:
    return next(b for b in u["toolbar"]["buttons"] if b["id"] == btn_id)


def test_panel_session_clicks_auto_titles_active_recency_sorting_and_toolbar():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = root / "alpha-proj"
        proj.mkdir(parents=True, exist_ok=True)
        log_dir = root / "logs"
        args, _received = claude_args(root)
        made: dict[str, str] = {}

        def setup(w: ProductWindow) -> None:
            conn = Conn(w.socket)
            acc = account(conn, "Work", "claude")
            made["acc"] = acc
            # Closed session created first (send 1 prompt so provider_session exists for close + Reconnect)
            s_closed, g_closed = new_claude(conn, account=acc, mode="auto", name="archived-run")
            assert conn.call(op="input", session=s_closed, generation=g_closed, request_id="c0", text="seed")["ok"] == 1
            wait_for(lambda: status_of(conn, s_closed)["activity"] == "idle" and status_of(conn, s_closed)["seq"] >= 2)
            assert conn.call(op="session.close", session=s_closed, generation=g_closed)["ok"] == 1
            wait_for(lambda: status_of(conn, s_closed)["state"] == "disconnected")
            made["closed"] = s_closed

            # Older active session (named)
            s_old, _ = new_claude(conn, account=acc, mode="auto", name="older-task")
            made["old"] = s_old

            # Newer unnamed active session in alpha-proj folder
            r_new = conn.call(op="session.create", kind="claude", account=acc, mode="auto", cwd=str(proj))
            assert r_new["ok"] == 1, r_new
            made["new"] = r_new["session"]

        with ProductWindow(host_binary=LICO, host_args=[*args, "--log-dir", str(log_dir)], before_window=setup) as w:
            conn = Conn(w.socket)
            s_closed, s_old, s_new = made["closed"], made["old"], made["new"]

            st0 = conn.call(op="status")
            assert v1.validate_response("status", st0) == []

            # Before first prompt, unnamed s_new falls back to folder basename 'alpha-proj', and running sessions precede closed
            _reload(w)
            u0 = wait_for(lambda: (u := w.ui()) and len(u["layout"]["rows"]) == 3 and u)
            order0 = [r[2] for r in u0["layout"]["rows"]]
            assert order0[-1] == s_closed, order0  # running sessions always sort ahead of disconnected
            assert set(order0[:2]) == {s_old, s_new}
            new_row0 = next(r for r in u0["rows"] if r["session"] == s_new)
            assert new_row0["display_name"] == "alpha-proj" and new_row0["auto_title"] is None

            # Wait so Unix second advances, then send first prompt to unnamed s_new -> auto_title derived & s_new sorts #1!
            time.sleep(1.1)
            assert conn.call(op="input", session=s_new, generation=1, request_id="p1", text="Fix sidebar click from About")["ok"] == 1
            wait_for(lambda: status_of(conn, s_new)["activity"] == "idle" and status_of(conn, s_new)["seq"] >= 2)
            _reload(w)
            u1 = wait_for(
                lambda: (u := w.ui())
                and next(r for r in u["rows"] if r["session"] == s_new)["auto_title"] == "Fix sidebar click from About"
                and u
            )
            order1 = [r[2] for r in u1["layout"]["rows"]]
            assert order1 == [s_new, s_old, s_closed], order1
            assert "Fix side\u2026" in u1["list_text"] and s_new[:7] not in u1["list_text"], u1["list_text"]

            # 1. Open Help > About Lico via menu click, then click s_new in left sidebar -> About closes & s_new opens immediately!
            m_help = u1["menu"]["titles"][3]["rect"]
            w.click(*_centre(m_help))
            u_menu = wait_for(lambda: (u := w.ui())["menu"]["open"] == 3 and all(i["rect"] for i in u["menu"]["items"]) and u)
            about_rect = next(i["rect"] for i in u_menu["menu"]["items"] if "About Lico" in i["label"])
            w.click(*_centre(about_rect))
            wait_for(lambda: (p := w.ui()["panel"]) is not None and p["title"] == "About Lico")

            w.click(*_row_centre(w, s_new))
            u_entered = wait_for(lambda: (u := w.ui())["panel"] is None and u["selected"] == s_new and u)
            assert "Fix sidebar click from About" in u_entered["header"], u_entered["header"]
            assert "dir: " in u_entered["header"] and u_entered["header"].endswith("/alpha-proj"), u_entered["header"]

            # 2. Open Help > Keyboard shortcuts (Alt+H, Return), press Ctrl+Tab -> panel closes and switches to s_old!
            _fast_key(w, "alt+h")
            wait_for(lambda: w.ui()["menu"]["open"] == 3)
            _fast_key(w, "Return")
            wait_for(lambda: (p := w.ui()["panel"]) is not None and p["title"] == "Keys")
            _fast_key(w, "ctrl+Tab")
            wait_for(lambda: (u := w.ui())["panel"] is None and u["selected"] == s_old)

            # 3. Draft preservation when clicking a session row while Rename is open
            _fast_type(w, "keep-old-draft")
            wait_for(lambda: w.ui()["input"] == "keep-old-draft")
            _fast_key(w, "F2")
            wait_for(lambda: (p := w.ui()["panel"]) is not None and "Rename" in p["title"])
            _fast_type(w, "discard-me")
            w.click(*_row_centre(w, s_new))
            u_sw = wait_for(lambda: (u := w.ui())["panel"] is None and u["selected"] == s_new and u)
            assert u_sw["input"] == ""
            w.click(*_row_centre(w, s_old))
            u_back = wait_for(lambda: (u := w.ui())["selected"] == s_old and u)
            assert u_back["input"] == "keep-old-draft"
            assert next(r for r in u_back["rows"] if r["session"] == s_old)["display_name"] == "older-task"

            # 4. Activity recency re-ordering: send prompt to s_old -> s_old jumps to #1 ahead of s_new, closed stays #3!
            time.sleep(1.1)
            assert conn.call(op="input", session=s_old, generation=1, request_id="p2", text="Bump older task")["ok"] == 1
            wait_for(lambda: status_of(conn, s_old)["activity"] == "idle" and status_of(conn, s_old)["seq"] >= 2)
            _reload(w)
            u_reordered = wait_for(lambda: (u := w.ui()) and [r[2] for r in u["layout"]["rows"]][0] == s_old and u)
            assert [r[2] for r in u_reordered["layout"]["rows"]] == [s_old, s_new, s_closed]

            # 5. Toolbar geometry & direct mouse click actions
            tb = u_reordered["toolbar"]
            assert tb["shown"] is True
            ids = [b["id"] for b in tb["buttons"]]
            assert ids == ["new_session", "accounts", "files", "close", "rename", "toggle_mode", "toggle_group", "find"]
            rects = [b["rect"] for b in tb["buttons"]]
            assert all(r is not None and 0 <= r[0] < r[2] <= 1200 and r[1] > 18 and r[3] < 60 for r in rects), rects
            assert all(a[2] <= b[0] for a, b in zip(rects, rects[1:])), rects

            # Click 'Group: acct' -> toggles to 'Group: dir' (workspace grouping)
            w.click(*_centre(_tool_btn(u_reordered, "toggle_group")["rect"]))
            u_grp = wait_for(lambda: (u := w.ui())["settings"]["group_by"] == "workspace" and u)
            assert _tool_btn(u_grp, "toggle_group")["label"] == "Group: dir"
            # Toggle back to account grouping via toolbar click
            w.click(*_centre(_tool_btn(u_grp, "toggle_group")["rect"]))
            u_acct = wait_for(lambda: (u := w.ui())["settings"]["group_by"] == "account" and u)
            assert _tool_btn(u_acct, "toggle_group")["label"] == "Group: acct"

            # Click 'Mode: auto' -> toggles session permission mode to 'asks for approval' ('Mode: ask')
            w.click(*_centre(_tool_btn(u_acct, "toggle_mode")["rect"]))
            u_mode = wait_for(lambda: (u := w.ui()) and "mode: asks for approval" in u["header"] and u)
            assert _tool_btn(u_mode, "toggle_mode")["label"] == "Mode: ask"

            # Click 'Find' -> opens in-session search bar; Escape closes it
            w.click(*_centre(_tool_btn(u_mode, "find")["rect"]))
            wait_for(lambda: w.ui()["search"] is not None)
            _fast_key(w, "Escape")
            wait_for(lambda: w.ui()["search"] is None)

            # Click 'Accounts' -> opens Accounts panel; click '+ New' -> switches directly to New session panel
            w.click(*_centre(_tool_btn(w.ui(), "accounts")["rect"]))
            wait_for(lambda: (p := w.ui()["panel"]) is not None and p["title"] == "Accounts")
            w.click(*_centre(_tool_btn(w.ui(), "new_session")["rect"]))
            wait_for(lambda: (p := w.ui()["panel"]) is not None and p["title"] == "New session")

            # Click disconnected s_closed in sidebar while New session is open -> closes panel, selects s_closed, toolbar offers 'Reconnect'!
            w.click(*_row_centre(w, s_closed))
            u_closed_sel = wait_for(lambda: (u := w.ui())["panel"] is None and u["selected"] == s_closed and u)
            resume_btn = _tool_btn(u_closed_sel, "resume")
            assert resume_btn["label"] == "Reconnect" and resume_btn["enabled"] is True
            w.click(*_centre(resume_btn["rect"]))
            wait_for(lambda: status_of(conn, s_closed)["state"] == "running")
            _reload(w)
            wait_for(lambda: any(b["id"] == "close" for b in w.ui()["toolbar"]["buttons"]))
