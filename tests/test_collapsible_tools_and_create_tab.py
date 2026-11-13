"""M09: clean `[call done]` / clipped `[call done` stripping, default-collapsed click-to-expand
tool & command cards with distinct indigo-violet palette, and cross-provider `mcp__lico__create`
(`Claude` <-> `Codex`, optional `prompt`, optional `scope`) opening visible tabs in `lico gui` (`0.2.5`)."""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.account_helpers import account
from tests.test_claude_adapter import FIXTURE as CLAUDE_FIXTURE, SIM as CLAUDE_SIM, records
from tests.test_codex_adapter import FIXTURE as CODEX_FIXTURE, SIM as CODEX_SIM, usual_umask, wait_for
from tests.test_first_view import PixelReader


def _centre(rect: list[float]) -> tuple[float, float]:
    return (rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0


def _distinct_bands(u: dict) -> list[list[float]]:
    out: list[list[float]] = []
    for band in u.get("layout", {}).get("lines", []):
        if not out or out[-1][2] != band[2]:
            out.append(band)
    return out


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


def test_clean_call_done_collapsible_indigo_tool_cards_and_cross_provider_create_tabs():
    cap_env = os.environ.get("LICO_CAPTURE_M09_DIR", "").strip()
    cap_dir = Path(cap_env) if cap_env else None
    if cap_dir is not None:
        cap_dir.mkdir(mode=0o700, parents=True, exist_ok=True)

    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        state_dir = root / "state"
        log_dir = state_dir / "log"
        work = root / "work"
        work.mkdir(mode=0o700, parents=True, exist_ok=True)
        host_args = _dual_args(root, state_dir)

        with ProductWindow(host_binary=LICO, host_args=host_args, host_sessions=0) as w:
            reader = PixelReader(w.xvfb.display)
            try:
                conn = Conn(w.socket)
                acc_claude = account(conn, "Claude-Main", "claude")
                acc_codex = account(conn, "Codex-Main", "codex")

                c_made = conn.call(
                    op="session.create", kind="claude", account=acc_claude, mode="auto", cwd=str(work), name="Claude-L"
                )
                assert c_made["ok"] == 1
                s_claude, g_claude = c_made["session"], c_made["generation"]

                x_made = conn.call(
                    op="session.create", kind="codex", account=acc_codex, mode="auto", cwd=str(work), name="Codex-L"
                )
                assert x_made["ok"] == 1
                s_codex, g_codex = x_made["session"], x_made["generation"]

                w.proc.stdin.write("load\n")
                w.proc.stdin.flush()
                u0 = wait_for(lambda: (u := w.ui()) and len(u["tabs"]["items"]) == 2 and u)
                w.click(*_tab_click_xy(next(t for t in u0["tabs"]["items"] if t["session"] == s_claude)))
                wait_for(lambda: w.ui()["selected"] == s_claude)

                # 1. Clean `[call done]` and 201-char `LINE_CHARS`-clipped `[call done` stripping:
                #    (a) Authorize Call Room `(Claude-L, Codex-L)` and have `Codex-L` send the 201-char Korean weather
                #        line ending in `[call done]` via `mcp__lico__send`: Call Room settles (`active == False`),
                #        no `[call done` appears in Call Room entries, and `Codex-L` folds `[tool] mcp__lico__send` to `▸ ...`.
                started = conn.call(op="call.start", **{"from": s_claude, "to": s_codex})
                assert started["ok"] == 1
                call_id = started["call"]["call_id"]

                weather_201 = (
                    "오늘 서울 날씨(2026년 11월 13일, 한국시간): 조회 시점 현재 서울 흐림, 약 12°C입니다. "
                    "오늘 낮 예보는 약 14~17°C, 밤에는 약 10~11°C이며 대체로 흐리겠습니다. "
                    "출처: 날씨 조회 도구(web weather)의 서울 실시간·시간별 예보. "
                    "조회 결과에 원문 URL이 제공되지 않아 출처 링크는 첨부할 수 없습니다. [call done]"
                )
                assert len(weather_201) == 201  # clips at 200 chars (`LINE_CHARS`) to trailing `[call done`
                send_arg_json = json.dumps({"to": "AT_PLACEHOLDER", "text": weather_201}, ensure_ascii=False).replace(
                    "AT_PLACEHOLDER", r"\u0040Claude-L"
                )
                assert conn.call(
                    op="input",
                    session=s_codex,
                    generation=g_codex,
                    request_id="x-weather",
                    text=f"tool:send {send_arg_json}",
                )["ok"] == 1
                wait_for(lambda: conn.call(op="call.get", call=call_id)["call"]["active"] is False)
                cshow = conn.call(op="call.get", call=call_id)["call"]
                assert not any("[call done" in e["text"].lower() for e in cshow["entries"]), cshow["entries"]

                # Switch to `Codex-L` tab and verify `[tool] mcp__lico__send` is folded (`▸ [tool] mcp__lico__send`)
                # and the `[peer reply]` card has zero `[call done` text:
                w.click(*_tab_click_xy(next(t for t in w.ui()["tabs"]["items"] if t["session"] == s_codex)))
                u_codex = wait_for(
                    lambda: (u := w.ui())
                    and u["selected"] == s_codex
                    and any(ln.startswith("\u25b8 [tool] mcp__lico__send") for ln in u["shown_lines"])
                    and any("오늘 서울 날씨" in ln for ln in u["shown_lines"])
                    and u
                )
                assert not any("[call done" in ln.lower() for ln in u_codex["shown_lines"]), u_codex["shown_lines"]

                #    (b) Switch back to `Claude-L` and emit the 201-char line directly (`say:[weather_201]`), which
                #        `LINE_CHARS` (200) clips to `... [call done` in `.seg`, and verify GUI hides `[call done` cleanly!
                w.click(*_tab_click_xy(next(t for t in u_codex["tabs"]["items"] if t["session"] == s_claude)))
                wait_for(lambda: w.ui()["selected"] == s_claude)
                assert conn.call(
                    op="input",
                    session=s_claude,
                    generation=g_claude,
                    request_id="c-weather-201",
                    text=f"say:{json.dumps([weather_201], ensure_ascii=False)}",
                )["ok"] == 1
                u_clean = wait_for(
                    lambda: (u := w.ui())
                    and any("오늘 서울 날씨" in ln for ln in u["shown_lines"])
                    and u
                )
                assert not any("[call done" in ln.lower() for ln in u_clean["shown_lines"]), u_clean["shown_lines"]

                # 2. Default-collapsed (`▸ ...`) tool/command cards + distinct indigo-violet palette + click expand/collapse:
                #    Run `fixture` on `Claude-L` (emits `$ ls -la` + output + normal prose).
                assert conn.call(op="input", session=s_claude, generation=g_claude, request_id="c-fix", text="fixture")["ok"] == 1
                u_folded = wait_for(
                    lambda: (u := w.ui())
                    and any(ln.startswith("\u25b8 $ ls -la") for ln in u["shown_lines"])
                    and any("Done" in ln for ln in u["shown_lines"])
                    and len(_distinct_bands(u)) == len(u["shown_lines"])
                    and u
                )
                assert not any("total 8" in ln for ln in u_folded["shown_lines"]), "command output hidden while folded"

                folded_bands = _distinct_bands(u_folded)
                pill_idx = next(i for i, ln in enumerate(u_folded["shown_lines"]) if ln.startswith("\u25b8 $ ls -la"))
                prose_idx = next(i for i, ln in enumerate(u_folded["shown_lines"]) if "I will list the files." in ln)
                pill_band = folded_bands[pill_idx]
                prose_band = folded_bands[prose_idx]
                sample_x = int(u_folded["layout"]["list_w"] + 460)
                pill_rgb = reader.pixel(sample_x, int(pill_band[0] + 2))
                prose_rgb = reader.pixel(sample_x, int(prose_band[0] + 2))
                assert pill_rgb == 0x161426, (hex(pill_rgb), hex(prose_rgb))
                assert pill_rgb != prose_rgb

                if cap_dir is not None:
                    w.png(cap_dir / "01_clean_reply_and_collapsed_tool_pill.png")

                # Click the `▸ $ ls -la` pill to expand it -> `▾ $ ls -la` + distinct indigo-violet body rows!
                click_x = u_folded["layout"]["list_w"] + 90.0
                click_y = (pill_band[0] + pill_band[1]) / 2.0
                w.click(click_x, click_y)
                u_expanded = wait_for(
                    lambda: (u := w.ui())
                    and any(ln.startswith("\u25be $ ls -la") for ln in u["shown_lines"])
                    and len(u["shown_lines"]) > len(u_folded["shown_lines"])
                    and len(_distinct_bands(u)) == len(u["shown_lines"])
                    and u
                )
                exp_bands = _distinct_bands(u_expanded)
                head_idx = next(i for i, ln in enumerate(u_expanded["shown_lines"]) if ln.startswith("\u25be $ ls -la"))
                head_band = exp_bands[head_idx]
                body_band = exp_bands[head_idx + 1]
                assert reader.pixel(sample_x, int(head_band[0] + 2)) == 0x1D1934
                assert reader.pixel(sample_x, int(body_band[0] + 2)) == 0x121020

                if cap_dir is not None:
                    w.png(cap_dir / "02_expanded_indigo_violet_tool_card.png")

                # Click the `▾ $ ls -la` header again -> collapses back to `▸ $ ls -la`!
                w.click(click_x, (head_band[0] + head_band[1]) / 2.0)
                wait_for(
                    lambda: (u := w.ui())
                    and any(ln.startswith("\u25b8 $ ls -la") for ln in u["shown_lines"])
                    and len(u["shown_lines"]) == len(u_folded["shown_lines"])
                    and u
                )

                # 3. Cross-provider `mcp__lico__create`:
                #    (a) `Claude-L` opens a new empty `Codex` tab (`kind="codex"`, no `prompt`) -> modal -> Allow -> new tab!
                w.proc.stdin.write("clock freeze\n")
                w.proc.stdin.flush()
                assert conn.call(
                    op="input",
                    session=s_claude,
                    generation=g_claude,
                    request_id="c-create-codex",
                    text='tool:create {"kind": "codex", "name": "Codex-New"}',
                )["ok"] == 1
                wait_for(
                    lambda: (u := w.ui())
                    and "create codex session 'Codex-New'" in (u.get("approval_bar") or "")
                    and u["card_seen"] is True
                    and u
                )
                w.proc.stdin.write("clock advance 900\n")
                w.proc.stdin.flush()
                u_modal_c2x = wait_for(lambda: (u := w.ui()) and u["card_ready"] is True and u)
                if cap_dir is not None:
                    w.png(cap_dir / "03_cross_provider_create_approval_modal.png")
                w.click(*_centre(u_modal_c2x["layout"]["allow"]))
                w.proc.stdin.write("clock run\n")
                w.proc.stdin.flush()

                u_tabs3 = wait_for(
                    lambda: (u := w.ui())
                    and any("Codex-New" in t["label"] for t in u["tabs"]["items"])
                    and u
                )
                codex_new_row = next(r for r in conn.call(op="status")["sessions"] if "Codex-New" in r.get("name", ""))
                assert codex_new_row["provider"] == "codex" and codex_new_row["account"] == acc_codex

                #    (b) `Codex-L` opens a new Global `Claude` tab WITH initial prompt (`kind="claude"`, `scope="global"`)!
                w.proc.stdin.write("clock freeze\n")
                w.proc.stdin.flush()
                assert conn.call(
                    op="input",
                    session=s_codex,
                    generation=g_codex,
                    request_id="x-create-claude",
                    text='tool:create {"kind": "claude", "scope": "global", "name": "Claude-New", "prompt": "새 Claude 글로벌 대화창 시작"}',
                )["ok"] == 1
                wait_for(
                    lambda: (u := w.ui())
                    and "create claude session 'Claude-New' in global scope" in (u.get("approval_bar") or "")
                    and u["card_seen"] is True
                    and u
                )
                w.proc.stdin.write("clock advance 900\n")
                w.proc.stdin.flush()
                u_modal_x2c = wait_for(lambda: (u := w.ui()) and u["card_ready"] is True and u)
                w.click(*_centre(u_modal_x2c["layout"]["allow"]))
                w.proc.stdin.write("clock run\n")
                w.proc.stdin.flush()

                u_tabs4 = wait_for(
                    lambda: (u := w.ui())
                    and any("Claude-New" in t["label"] and t["global"] is True for t in u["tabs"]["items"])
                    and u
                )
                claude_new_tab = next(t for t in u_tabs4["tabs"]["items"] if "Claude-New" in t["label"])
                w.click(*_tab_click_xy(claude_new_tab))
                u_claude_new = wait_for(
                    lambda: (u := w.ui())
                    and u["selected"] == claude_new_tab["session"]
                    and any("새 Claude 글로벌 대화창 시작" in ln for ln in u["shown_lines"])
                    and u
                )
                assert len(u_claude_new["tabs"]["items"]) == 4

                if cap_dir is not None:
                    w.png(cap_dir / "04_new_codex_and_global_claude_tabs_open.png")
            finally:
                reader.close()
