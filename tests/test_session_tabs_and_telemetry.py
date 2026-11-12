"""M07-P01: maximized product window launch, dedicated Global general sessions,
multi-session horizontal tab bar, and color-coded per-session telemetry bar
(Model/Effort, live Tokens in/out, 5h & Weekly quota with reset countdown, and CWD/Global scope)."""

from __future__ import annotations

import subprocess
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir
from lico_protocol.v1 import validate_response
from tests.account_helpers import account
from tests.test_claude_adapter import status_of
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_new_session_screen import claude_args
from tests.test_reconnect_window import codex_args


def _centre(rect: list[float]) -> tuple[float, float]:
    return (rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0


def _reload(w: ProductWindow) -> None:
    w.proc.stdin.write("load\n")
    w.proc.stdin.flush()


def _fast_key(w: ProductWindow, *keys: str) -> None:
    subprocess.run(["xdotool", "key", *keys], env=w.env, check=True)


def test_maximized_launch_global_sessions_top_tabs_and_live_telemetry_bar():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = root / "workbench-repo"
        proj.mkdir(mode=0o700, parents=True, exist_ok=True)
        (proj / "README.md").write_text("# Workbench Repo\n", encoding="utf-8")
        log_dir = root / "logs"
        c_args, _ = claude_args(root)
        x_args, _ = codex_args(root)
        made: dict[str, str] = {}

        def setup(w: ProductWindow) -> None:
            conn = Conn(w.socket)
            a_claude = account(conn, "ClaudeMain", "claude")
            a_codex = account(conn, "CodexMain", "codex")
            made["a_claude"] = a_claude
            made["a_codex"] = a_codex

            opts = conn.call(op="session.options")
            effort = opts["efforts"][-1] if opts.get("efforts") else None
            create_claude = {
                "op": "session.create",
                "kind": "claude",
                "account": a_claude,
                "mode": "auto",
                "cwd": str(proj),
                "name": "Pri-Lico",
            }
            if effort:
                create_claude["effort"] = effort
            s_claude = conn.call(**create_claude)
            assert s_claude["ok"] == 1, s_claude

            s_codex = conn.call(
                op="session.create",
                kind="codex",
                account=a_codex,
                mode="auto",
                cwd=str(proj),
                name="CodexWorker",
            )
            assert s_codex["ok"] == 1, s_codex
            made["s_claude"] = s_claude["session"]
            made["g_claude"] = str(s_claude["generation"])
            made["s_codex"] = s_codex["session"]
            made["g_codex"] = str(s_codex["generation"])
            made["effort"] = effort or ""

        with ProductWindow(
            host_binary=LICO,
            host_args=[*c_args, *x_args, "--log-dir", str(log_dir)],
            env_extra={"LANG": "C.UTF-8"},
            before_window=setup,
        ) as w:
            conn = Conn(w.socket)
            s_claude = made["s_claude"]
            s_codex = made["s_codex"]
            _reload(w)

            # 1. Maximized launch requested by default on `lico gui` + top tab bar visible
            u0 = wait_for(
                lambda: (u := w.ui())
                and u["tabs"]["shown"] is True
                and len(u["tabs"]["items"]) == 2
                and u
            )
            assert u0["layout"]["maximized_requested"] is True
            st0 = conn.call(op="status")
            assert validate_response("status", st0) == []
            st0_by_id = {s["session"]: s for s in st0["sessions"]}
            assert "tokens" not in st0_by_id[s_claude]

            # 2. Create a dedicated Global general session via `+G` tab-bar button
            new_g_rect = u0["tabs"]["new_global_rect"]
            assert new_g_rect is not None
            w.click(*_centre(new_g_rect))
            uf = wait_for(
                lambda: (u := w.ui())
                and u["panel"]
                and u["panel"]["title"] == "New session"
                and u["panel"]["form"]["focus"] == "Model"
                and u
            )
            assert uf["panel"]["form"]["folder"] == ""
            assert any("Global general session" in line for line in uf["panel"]["body"])
            assert uf["telemetry"]["shown"] is False  # hidden while modal panel owns prompt row

            # Press Enter directly from Model row to launch Global session
            _fast_key(w, "Return")
            ug = wait_for(
                lambda: (u := w.ui())
                and u["panel"] is None
                and len(u["tabs"]["items"]) == 3
                and u["telemetry"]["shown"] is True
                and u["telemetry"]["global"] is True
                and u
            )
            s_global = ug["selected"]
            assert s_global not in (s_claude, s_codex)
            global_tab = next(t for t in ug["tabs"]["items"] if t["session"] == s_global)
            assert global_tab["global"] is True and global_tab["label"].startswith("G\u00b7")
            assert "Global" in ug["telemetry"]["cwd_text"]
            assert ug["telemetry"]["tokens_text"] == "Tok: in:-- out:--"

            # 3. Emit live token + quota events on Claude & Codex sessions and verify telemetry bar
            assert conn.call(
                op="input",
                session=s_claude,
                generation=int(made["g_claude"]),
                request_id="tok-c1",
                text="tokens:1200:85:94500",
            )["ok"] == 1
            wait_for(lambda: status_of(conn, s_claude)["activity"] == "idle")
            assert conn.call(
                op="input",
                session=s_claude,
                generation=int(made["g_claude"]),
                request_id="rl-c1",
                text="ratelimit:0.22:0.07",
            )["ok"] == 1
            wait_for(
                lambda: status_of(conn, s_claude)["activity"] == "idle"
                and status_of(conn, s_claude).get("account_usage", {}).get("weekly_remaining_pct") == 78
            )

            assert conn.call(
                op="input",
                session=s_codex,
                generation=int(made["g_codex"]),
                request_id="tok-x1",
                text="tokens:2510000:4200",
            )["ok"] == 1
            wait_for(lambda: status_of(conn, s_codex)["activity"] == "idle")
            assert conn.call(
                op="input",
                session=s_codex,
                generation=int(made["g_codex"]),
                request_id="rl-x1",
                text="ratelimit:92:15",
            )["ok"] == 1
            wait_for(
                lambda: status_of(conn, s_codex)["activity"] == "idle"
                and status_of(conn, s_codex).get("account_usage", {}).get("weekly_remaining_pct") == 8
            )

            st1 = conn.call(op="status")
            assert validate_response("status", st1) == []
            st1_by_id = {s["session"]: s for s in st1["sessions"]}
            assert st1_by_id[s_claude]["tokens"] == {"input": 95700, "output": 85}
            assert st1_by_id[s_codex]["tokens"] == {"input": 2510000, "output": 4200}

            # Open About panel, then click Claude tab directly -> closes panel & switches to Claude!
            w.key("alt+h")
            mh = wait_for(lambda: (u := w.ui()) and u["menu"]["open"] == 3 and u["menu"]["items"] and u)
            about_item = next(i for i in mh["menu"]["items"] if "About" in i["label"])
            w.click(*_centre(about_item["rect"]))
            wait_for(lambda: (u := w.ui()) and u["panel"] and u["panel"]["title"] == "About Lico")

            claude_tab = next(t for t in w.ui()["tabs"]["items"] if t["session"] == s_claude)
            w.click(*_centre(claude_tab["rect"]))
            uc = wait_for(
                lambda: (u := w.ui())
                and u["panel"] is None
                and u["selected"] == s_claude
                and u["telemetry"]["shown"] is True
                and "95.7k" in u["telemetry"]["tokens_text"]
                and u
            )
            assert uc["telemetry"]["tokens"] == {"input": 95700, "output": 85}
            assert uc["telemetry"]["tokens_text"] == "Tok: in:95.7k out:85"
            assert "5h:93%" in uc["telemetry"]["quota_text"] and "Wk:78%" in uc["telemetry"]["quota_text"]
            assert uc["telemetry"]["quota_level"] == "ok"
            assert uc["telemetry"]["cwd_text"] == str(proj)
            if made["effort"]:
                assert f"({made['effort'].capitalize()})" in uc["telemetry"]["model_text"]

            # Switch to Codex tab and check critical quota level + M-scale token formatting
            codex_tab = next(t for t in uc["tabs"]["items"] if t["session"] == s_codex)
            w.click(*_centre(codex_tab["rect"]))
            ux = wait_for(
                lambda: (u := w.ui())
                and u["selected"] == s_codex
                and "2.51M" in u["telemetry"]["tokens_text"]
                and u
            )
            assert ux["telemetry"]["tokens_text"] == "Tok: in:2.51M out:4.2k"
            assert "Wk:8%" in ux["telemetry"]["quota_text"]
            assert ux["telemetry"]["quota_level"] == "critical"

            # Clicking CWD segment on telemetry bar opens Working directory files at `proj`
            assert ux["telemetry"]["cwd_rect"] is not None
            w.click(*_centre(ux["telemetry"]["cwd_rect"]))
            ufiles = wait_for(
                lambda: (u := w.ui())
                and u["panel"]
                and u["panel"]["files"]
                and u["panel"]["files"]["cwd"] == str(proj)
                and u
            )
            assert any(e["name"] == "README.md" for e in ufiles["panel"]["files"]["entries"])

            # Clicking `×` on Global tab closes that running session cleanly
            gtab = next(t for t in ufiles["tabs"]["items"] if t["session"] == s_global)
            w.click(*_centre(gtab["close_rect"]))
            wait_for(lambda: status_of(conn, s_global)["state"] == "disconnected")
