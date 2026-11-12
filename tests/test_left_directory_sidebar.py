"""M07-P02: two-tab left workspace sidebar (`[ Directories ]` | `[ Past ]`),
active working-directory & Global highlighting, async session-switch sync,
`-> active` jump-back, in-memory filename filter, `.dot` toggle, pagination,
inline UTF-8 preview (`Esc` straight back to chat), external opener, and `Ctrl+B` collapse."""

from __future__ import annotations

import stat
import subprocess
import time
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.account_helpers import account
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_new_session_screen import claude_args


def _centre(rect: list[float]) -> tuple[float, float]:
    return (rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0


def _tab_click_xy(tab: dict) -> tuple[float, float]:
    return (tab["rect"][0] + tab["close_rect"][0]) / 2.0, (tab["rect"][1] + tab["rect"][3]) / 2.0


def _fast_key(w: ProductWindow, *keys: str) -> None:
    subprocess.run(["xdotool", "key", *keys], env=w.env, check=True)


def _fast_type(w: ProductWindow, text: str) -> None:
    subprocess.run(["xdotool", "type", "--delay", "5", text], env=w.env, check=True)


def test_left_two_tab_directory_sidebar_highlight_filter_preview_and_collapse():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj_a = root / "proj-alpha"
        proj_b = root / "proj-beta"
        nested_b = proj_b / "nested"
        proj_a.mkdir(mode=0o700, parents=True, exist_ok=True)
        nested_b.mkdir(mode=0o700, parents=True, exist_ok=True)

        (proj_a / "alpha_readme.md").write_text("# Alpha Project\nFirst repo.\n", encoding="utf-8")
        (proj_b / ".env.example").write_text("PORT=8080\n", encoding="utf-8")
        (proj_b / "notes.md").write_text("# Beta Notes\nInline sidebar preview check.\n", encoding="utf-8")
        (proj_b / "report.html").write_text("<!doctype html><h1>Beta Report</h1>\n", encoding="utf-8")
        for idx in range(28):
            (proj_b / f"item_{idx:02d}.txt").write_text(f"row {idx}\n", encoding="utf-8")
        (nested_b / "deep.txt").write_text("deep file\n", encoding="utf-8")

        opened_log = root / "opened_targets.log"
        opener = root / "record_open.sh"
        opener.write_text(f"#!/bin/sh\nprintf '%s\\n' \"$1\" >> '{opened_log}'\n", encoding="utf-8")
        opener.chmod(opener.stat().st_mode | stat.S_IXUSR)

        log_dir = root / "logs"
        args, _ = claude_args(root)
        made: dict[str, str] = {}

        def setup(w: ProductWindow) -> None:
            conn = Conn(w.socket)
            acc = account(conn, "Work", "claude")
            made["acc"] = acc
            sa = conn.call(op="session.create", kind="claude", account=acc, mode="auto", cwd=str(proj_a), name="AlphaChat")
            sb = conn.call(op="session.create", kind="claude", account=acc, mode="auto", cwd=str(proj_b), name="BetaChat")
            sg = conn.call(op="session.create", kind="claude", account=acc, mode="auto", name="QuickAsk")
            assert sa["ok"] == 1 and sb["ok"] == 1 and sg["ok"] == 1, (sa, sb, sg)
            made["s_a"] = sa["session"]
            made["s_b"] = sb["session"]
            made["s_g"] = sg["session"]

        with ProductWindow(
            host_binary=LICO,
            host_args=[*args, "--log-dir", str(log_dir)],
            env_extra={"LICO_OPEN_CMD": str(opener), "LANG": "C.UTF-8"},
            before_window=setup,
        ) as w:
            s_a, s_b, s_g = made["s_a"], made["s_b"], made["s_g"]
            w.proc.stdin.write("load\n")
            w.proc.stdin.flush()

            # 1. Default 2-tab Left Sidebar (`[ Directories ]` | `[ Past ]`) is visible & open
            u0 = wait_for(
                lambda: (u := w.ui())
                and u["sidebar"]["shown"] is True
                and len(u["tabs"]["items"]) == 3
                and u
            )
            assert u0["sidebar"]["collapsed"] is False
            assert u0["sidebar"]["tab"] == "directories"
            assert u0["sidebar"]["dir_tab_rect"] is not None
            assert u0["sidebar"]["past_tab_rect"] is not None
            assert u0["layout"]["list_w"] >= 330

            # 2. Selecting AlphaChat highlights proj_a; switching to BetaChat auto-syncs & highlights proj_b
            tab_a = next(t for t in u0["tabs"]["items"] if t["session"] == s_a)
            w.click(*_tab_click_xy(tab_a))
            ua = wait_for(
                lambda: (u := w.ui())
                and u["selected"] == s_a
                and u["sidebar"]["cwd"] == str(proj_a.resolve())
                and u["sidebar"]["loading"] is None
                and u
            )
            assert ua["sidebar"]["active_cwd_highlighted"] is True
            assert ua["sidebar"]["global_highlighted"] is False
            assert [e["name"] for e in ua["sidebar"]["entries"]] == ["alpha_readme.md"]

            tab_b = next(t for t in ua["tabs"]["items"] if t["session"] == s_b)
            w.click(*_tab_click_xy(tab_b))
            ub = wait_for(
                lambda: (u := w.ui())
                and u["selected"] == s_b
                and u["sidebar"]["cwd"] == str(proj_b.resolve())
                and u["sidebar"]["loading"] is None
                and u
            )
            assert ub["sidebar"]["active_cwd_highlighted"] is True
            assert ub["sidebar"]["total_pages"] >= 2

            # 3. Switching to Global session highlights `★ Global (general)` while keeping proj_b browsable
            tab_g = next(t for t in ub["tabs"]["items"] if t["session"] == s_g)
            w.click(*_tab_click_xy(tab_g))
            ug = wait_for(lambda: (u := w.ui()) and u["selected"] == s_g and u["sidebar"]["global_highlighted"] is True and u)
            assert ug["sidebar"]["global_scope"] is True
            assert ug["sidebar"]["cwd"] == str(proj_b.resolve())

            # Switch back to BetaChat
            tab_b2 = next(t for t in ug["tabs"]["items"] if t["session"] == s_b)
            w.click(*_tab_click_xy(tab_b2))
            ub2 = wait_for(lambda: (u := w.ui()) and u["selected"] == s_b and u["sidebar"]["active_cwd_highlighted"] is True and u)

            # 4. Pagination (`>` and `<`) across proj_b entries
            assert ub2["sidebar"]["page"] == 1
            w.click(*_centre(ub2["sidebar"]["next_rect"]))
            ub_p2 = wait_for(lambda: (u := w.ui()) and u["sidebar"]["page"] == 2 and u)
            w.click(*_centre(ub_p2["sidebar"]["prev_rect"]))
            ub_p1 = wait_for(lambda: (u := w.ui()) and u["sidebar"]["page"] == 1 and u)

            # 5. Navigate into `nested/` -> `active_cwd_highlighted` turns False, `can_jump_active` turns True, `-> active` jumps back
            nested_entry = next(e for e in ub_p1["sidebar"]["entries"] if e["name"] == "nested")
            w.click(*_centre(nested_entry["rect"]))
            u_nested = wait_for(
                lambda: (u := w.ui())
                and u["sidebar"]["cwd"] == str(nested_b.resolve())
                and u["sidebar"]["loading"] is None
                and u
            )
            assert u_nested["sidebar"]["active_cwd_highlighted"] is False
            assert u_nested["sidebar"]["can_jump_active"] is True
            assert u_nested["sidebar"]["jump_active_rect"] is not None
            w.click(*_centre(u_nested["sidebar"]["jump_active_rect"]))
            ub_back = wait_for(
                lambda: (u := w.ui())
                and u["sidebar"]["cwd"] == str(proj_b.resolve())
                and u["sidebar"]["active_cwd_highlighted"] is True
                and u
            )

            # 6. `.dot` toggle reveals `.env.example`, and explicit `R` refresh updates without background timers
            assert ".env.example" not in [e["name"] for e in ub_back["sidebar"]["entries"]]
            w.click(*_centre(ub_back["sidebar"]["dotfiles_rect"]))
            u_dot = wait_for(
                lambda: (u := w.ui())
                and any(e["name"] == ".env.example" for e in u["sidebar"]["entries"])
                and u
            )
            loads_before = u_dot["sidebar"]["load_count"]
            (proj_b / "later.txt").write_text("added on disk\n", encoding="utf-8")
            time.sleep(0.5)
            assert w.ui()["sidebar"]["load_count"] == loads_before
            w.click(*_centre(u_dot["sidebar"]["refresh_rect"]))
            u_ref = wait_for(lambda: (u := w.ui()) and u["sidebar"]["refresh_count"] >= 1 and u)
            assert u_ref["sidebar"]["total_entries"] >= 33

            # 7. Instant in-memory filename filter (`notes` -> `notes.md`, then clear) + inline UTF-8 preview (`Esc` straight to chat)
            w.click(*_centre(u_ref["sidebar"]["filter_rect"]))
            wait_for(lambda: w.ui()["sidebar"]["filter_focused"] is True)
            _fast_type(w, "notes")
            u_flt = wait_for(lambda: (u := w.ui()) and u["sidebar"]["filter"] == "notes" and u)
            assert [e["name"] for e in u_flt["sidebar"]["entries"]] == ["notes.md"]
            w.click(*_centre(u_flt["sidebar"]["entries"][0]["rect"]))
            u_prev = wait_for(
                lambda: (u := w.ui())
                and u["panel"] is not None
                and u["panel"]["files"] is not None
                and u["panel"]["files"]["mode"] == "preview"
                and u
            )
            assert any("Inline sidebar preview check." in line for line in u_prev["panel"]["body"])
            _fast_key(w, "Escape")
            u_chat_again = wait_for(lambda: (u := w.ui()) and u["panel"] is None and u)
            assert u_chat_again["selected"] == s_b

            # Clear filter and click `report.html` -> opens externally via `LICO_OPEN_CMD` without leaving chat
            w.click(*_centre(u_chat_again["sidebar"]["filter_clear_rect"]))
            u_cleared = wait_for(lambda: (u := w.ui()) and u["sidebar"]["filter"] == "" and u)
            w.click(*_centre(u_cleared["sidebar"]["filter_rect"]))
            wait_for(lambda: w.ui()["sidebar"]["filter_focused"] is True)
            _fast_type(w, "report.html")
            u_html_flt = wait_for(lambda: (u := w.ui()) and [e["name"] for e in u["sidebar"]["entries"]] == ["report.html"] and u)
            w.click(*_centre(u_html_flt["sidebar"]["entries"][0]["rect"]))
            expected_html = str((proj_b / "report.html").resolve())
            wait_for(lambda: opened_log.exists() and expected_html in opened_log.read_text(encoding="utf-8"))
            assert w.ui()["panel"] is None
            w.click(*_centre(w.ui()["sidebar"]["filter_clear_rect"]))
            wait_for(lambda: w.ui()["sidebar"]["filter"] == "")

            # 8. Switch to `[ Past ]` tab and back, then `Ctrl+B` collapse & re-expand
            w.click(*_centre(w.ui()["sidebar"]["past_tab_rect"]))
            u_past = wait_for(lambda: (u := w.ui()) and u["sidebar"]["tab"] == "past" and u)
            w.click(*_centre(u_past["sidebar"]["dir_tab_rect"]))
            wait_for(lambda: w.ui()["sidebar"]["tab"] == "directories")

            _fast_key(w, "ctrl+b")
            u_col = wait_for(lambda: (u := w.ui()) and u["sidebar"]["collapsed"] is True and u)
            assert u_col["layout"]["list_w"] == 0
            _fast_key(w, "ctrl+b")
            u_exp = wait_for(lambda: (u := w.ui()) and u["sidebar"]["collapsed"] is False and u)
            assert u_exp["layout"]["list_w"] >= 330
