"""M05-P02: top-anchored panel layout, large-directory listing (`<= 10,000`), and page/wheel navigation."""

from __future__ import annotations

import subprocess
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.account_helpers import account
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_new_session_screen import claude_args


def _tool_btn(ui: dict, btn_id: str) -> dict:
    return next(b for b in ui["toolbar"]["buttons"] if b["id"] == btn_id)


def _centre(rect: list[float]) -> tuple[float, float]:
    return (rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0


def _fast_key(w: ProductWindow, *keys: str) -> None:
    subprocess.run(["xdotool", "key", *keys], env=w.env, check=True)


def _wheel(w: ProductWindow, x: float, y: float, button: int, clicks: int = 1) -> None:
    wid = w.window_id()
    subprocess.run(
        ["xdotool", "mousemove", "--window", wid, str(int(x)), str(int(y)), "click", "--repeat", str(clicks), "--delay", "20", str(button)],
        env=w.env,
        check=True,
    )


def test_large_directory_pagination_toolbar_keys_wheel_and_top_row_visibility():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        big_dir = root / "big_workspace"
        big_dir.mkdir(parents=True)

        # 1. Over 500 entries + a very long filename to ensure zero line-wrap clipping + a multi-page text file
        long_name = "0000_extremely_long_filename_" + ("segment_" * 20) + "end.txt"
        (big_dir / long_name).write_text("\n".join(f"preview line {i:03d}" for i in range(1, 95)) + "\n", encoding="utf-8")
        for i in range(1, 560):
            (big_dir / f"file_{i:04d}.txt").write_text(f"body {i}\n", encoding="utf-8")

        args, _ = claude_args(directory)

        def setup(w: ProductWindow) -> None:
            conn = Conn(w.socket)
            account(conn, "Work", "claude")
            # Also verify folder.list returns all 560 entries without truncating at 500, and set 20px font first to stress top-row anchoring!
            listed = conn.call(op="folder.list", cwd=str(big_dir))
            assert listed["ok"] == 1 and listed["truncated"] is False
            assert len(listed["entries"]) == 560
            conn.call(op="settings.set", last_cwd=str(big_dir), last_view="files", font_px=20)

        with ProductWindow(
            host_binary=LICO,
            host_args=args,
            env_extra={"LICO_NO_AUTO_OPEN": "1"},
            before_window=setup,
        ) as w:
            # Restores directly into big_dir at font_px=20; top row (raw index 0 = '[+ New session...]') MUST be visible!
            u1 = wait_for(
                lambda: (u := w.ui())["panel"] is not None
                and u["panel"]["files"] is not None
                and u["panel"]["files"]["entry_count"] == 560
                and all(b["rect"] for b in u["toolbar"]["buttons"])
                and u
            )
            f1 = u1["panel"]["files"]
            assert f1["page"] == 1
            assert f1["total_pages"] > 10
            assert u1["visible"][0] == 0, f"top row clipped at font_px=20: visible={u1['visible']}"
            assert any(l[2] == 0 for l in u1["layout"]["lines"]), "line 0 hit-box missing"
            assert _tool_btn(u1, "prev_page")["enabled"] is False
            assert _tool_btn(u1, "next_page")["enabled"] is True
            assert f"Page 1 / {f1['total_pages']}" in "\n".join(u1["panel"]["body"])

            # Toolbar 'Next >' advances to Page 2; '< Prev' becomes enabled
            w.click(*_centre(_tool_btn(u1, "next_page")["rect"]))
            u2 = wait_for(lambda: (u := w.ui())["panel"]["files"]["page"] == 2 and u)
            assert _tool_btn(u2, "prev_page")["enabled"] is True
            assert u2["visible"][0] == 0

            # Keyboard ']' -> Page 3, Page_Down -> Page 4, '[' -> Page 3, '< Prev' toolbar -> Page 2, Page_Up -> Page 1
            _fast_key(w, "bracketright")
            wait_for(lambda: w.ui()["panel"]["files"]["page"] == 3)
            _fast_key(w, "Page_Down")
            wait_for(lambda: w.ui()["panel"]["files"]["page"] == 4)
            _fast_key(w, "bracketleft")
            u3 = wait_for(lambda: (u := w.ui())["panel"]["files"]["page"] == 3 and u)
            w.click(*_centre(_tool_btn(u3, "prev_page")["rect"]))
            wait_for(lambda: w.ui()["panel"]["files"]["page"] == 2)
            _fast_key(w, "Page_Up")
            u_p1 = wait_for(lambda: (u := w.ui())["panel"]["files"]["page"] == 1 and u)

            # Mouse wheel down (button 5) moves cursor down; wheel up (button 4) moves cursor back up
            c0 = u_p1["panel"]["files"]["cursor"]
            _wheel(w, 600.0, 260.0, 5, clicks=2)
            u_wd = wait_for(lambda: (u := w.ui())["panel"]["files"]["cursor"] > c0 and u)
            _wheel(w, 600.0, 260.0, 4, clicks=2)
            u_wu = wait_for(lambda: (u := w.ui())["panel"]["files"]["cursor"] == 0 and u)

            # Open the multi-page file (0000_extremely_long_filename_...) and paginate its preview via Next > and wheel!
            file_line = next(i for i, line in enumerate(u_wu["panel"]["body"]) if "0000_extremely_long_filename_" in line)
            y0, y1, _ = next(l for l in u_wu["layout"]["lines"] if l[2] == file_line)
            w.click(520.0, (y0 + y1) / 2.0)
            u_pv1 = wait_for(lambda: (u := w.ui())["panel"]["files"]["mode"] == "preview" and u)
            assert u_pv1["panel"]["files"]["page"] == 1
            assert u_pv1["panel"]["files"]["total_pages"] >= 3
            assert u_pv1["visible"][0] == 0
            assert any("preview line 001" in line for line in u_pv1["panel"]["body"])

            w.click(*_centre(_tool_btn(u_pv1, "next_page")["rect"]))
            u_pv2 = wait_for(lambda: (u := w.ui())["panel"]["files"]["page"] == 2 and u)
            assert u_pv2["panel"]["files"]["preview"]["top"] > 0
            _wheel(w, 600.0, 260.0, 5, clicks=2)
            wait_for(lambda: w.ui()["panel"]["files"]["preview"]["top"] > u_pv2["panel"]["files"]["preview"]["top"])
