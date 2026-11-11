"""M02-P06: git worktree sessions (`WT-01`, `WT-02`, `SF-07`), background session attention and
desktop notifications (`NT-01`, `NT-02`), and on-demand conversation search (`SR-01`, `SR-02`)."""
import json
import os
import stat
import subprocess
import sys
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import HOSTD_DIR, LICO, ProtoConn as Conn, start
from lico_measure.rig import private_dir
from lico_protocol import v1
from tests.account_helpers import account
from tests.test_claude_adapter import FIXTURE, SIM, lines_of, new_claude, status_of
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_new_session_screen import claude_args


def init_git_repo(path: Path) -> Path:
    path.mkdir(parents=True, exist_ok=True)
    git_env = {"PATH": "/usr/bin:/bin", "LANG": "C.UTF-8", "GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@e",
               "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@e"}
    subprocess.run(["git", "-C", str(path), "init", "-b", "main"], env=git_env, check=True, capture_output=True)
    (path / "README.txt").write_text("tracked\n")
    subprocess.run(["git", "-C", str(path), "add", "README.txt"], env=git_env, check=True, capture_output=True)
    subprocess.run(["git", "-C", str(path), "commit", "-m", "init"], env=git_env, check=True, capture_output=True)
    return path.resolve()


def cli_run(socket: str, *args: str):
    env = dict(os.environ, LICO_SOCKET=socket)
    return subprocess.run([str(LICO), *args], env=env, capture_output=True, text=True, timeout=15)


def window_lines(w: ProductWindow) -> list[str]:
    w.proc.stdin.write("lines\n")
    w.proc.stdin.flush()
    while True:
        ev = w.events.get(timeout=5.0)
        if ev.get("ev") == "lines":
            return ev["lines"]


def reload_status(w: ProductWindow) -> None:
    w.proc.stdin.write("load\n")
    w.proc.stdin.flush()


def start_claude_host(directory: str, env_overrides=None):
    received = Path(directory) / "claude_received.txt"
    extra = [
        "--claude", sys.executable, "--claude-arg", "-I", "--claude-arg", str(SIM),
        "--claude-arg", "--received", "--claude-arg", str(received),
        "--claude-arg", "--fixture", "--claude-arg", str(FIXTURE),
        "--cred-bin", str(HOSTD_DIR / "lico-cred"), "--cred-dir", str(Path(directory) / "credentials"),
    ]
    with usual_umask():
        host, log = start(directory, extra=extra, sessions=0, producing=0, history=0, retain=100000, env_overrides=env_overrides)
    return host, log, received


def test_worktree_lifecycle_cli_desktop_notifications_and_parent_repo_safety():
    with private_dir() as directory:
        root = Path(directory).resolve()
        repo = init_git_repo(root / "repo")
        (repo / "parent_untracked.txt").write_text("keep-me\n")
        plain = root / "plain_dir"
        plain.mkdir()
        notify_log = root / "notify.log"
        stub = root / "notify_stub.sh"
        stub.write_text(f"#!/bin/sh\nprintf '%s\\t%s\\t%s\\n' \"$1\" \"$2\" \"$3\" >> '{notify_log}'\n")
        stub.chmod(0o700)

        host, _, received = start_claude_host(directory, env_overrides={"LICO_NOTIFY_BIN": str(stub)})
        try:
            conn = Conn(host.socket)
            acc = account(conn, "Work", "claude")

            # Protocol v1 validation on folder.git
            fg_yes = conn.call(op="folder.git", cwd=str(repo))
            fg_no = conn.call(op="folder.git", cwd=str(plain))
            assert v1.validate_response("folder.git", fg_yes) == [] and fg_yes["is_git"] is True and fg_yes["repo"] == str(repo)
            assert v1.validate_response("folder.git", fg_no) == [] and fg_no["is_git"] is False

            # Refusals: non-git folder, invalid branch, existing branch
            assert conn.call(op="session.create", kind="claude", account=acc, cwd=str(plain), worktree_branch="feat/a")["error"] == "not_git_repo"
            assert conn.call(op="session.create", kind="claude", account=acc, cwd=str(repo), worktree_branch="main")["error"] == "branch_exists"
            assert conn.call(op="session.create", kind="claude", account=acc, cwd=str(repo), worktree_branch="-bad")["error"] == "bad_request"

            # CLI worktree creation
            created = cli_run(host.socket, "session", "create", "--kind", "claude", "--account", acc, "--cwd", str(repo), "--worktree", "feat/cli-wt", "--name", "cli-wt")
            assert created.returncode == 0, created.stdout + created.stderr
            sid = json.loads(created.stdout)["session"]
            wt_dir = Path(directory).resolve() / "workspaces" / "worktrees" / sid
            assert wt_dir.is_dir()
            assert stat.S_IMODE(wt_dir.parent.stat().st_mode) == 0o700
            wait_for(lambda: f"CWD {wt_dir}" in lines_of(received))

            st = conn.call(op="status")
            assert v1.validate_response("status", st) == []
            entry = next(s for s in st["sessions"] if s["session"] == sid)
            assert entry["worktree"] == {"repo": str(repo), "branch": "feat/cli-wt", "exists": True}
            opts = conn.call(op="session.options")
            assert opts["recent"] == [str(repo)] and opts["last"]["cwd"] == str(repo)

            # First turn so session can be closed cleanly later
            assert conn.call(op="input", session=sid, generation=1, request_id="r1", text="hello")["ok"] == 1
            wait_for(lambda: status_of(conn, sid)["activity"] == "idle" and status_of(conn, sid).get("finished_seq", -1) >= 0)

            # Dirty worktree refuses removal without --force-dirty, leaves process and files intact
            (wt_dir / "dirty.txt").write_text("uncommitted\n")
            refused_close = cli_run(host.socket, "session", "close", sid, "--remove-worktree")
            assert refused_close.returncode == 2 and "worktree_dirty" in refused_close.stdout
            assert status_of(conn, sid)["state"] == "running" and (wt_dir / "dirty.txt").exists()

            # Close keeping worktree succeeds and preserves dirty file
            kept_close = cli_run(host.socket, "session", "close", sid, "--keep-worktree")
            assert kept_close.returncode == 0, kept_close.stdout + kept_close.stderr
            assert status_of(conn, sid)["state"] == "disconnected" and (wt_dir / "dirty.txt").exists()

            # Delete without --force-dirty refuses; with --force-dirty removes worktree, keeps branch and parent repo untouched
            refused_del = cli_run(host.socket, "session", "delete", sid, "--remove-worktree")
            assert refused_del.returncode == 2 and "worktree_dirty" in refused_del.stdout
            forced_del = cli_run(host.socket, "session", "delete", sid, "--remove-worktree", "--force-dirty")
            assert forced_del.returncode == 0, forced_del.stdout + forced_del.stderr
            assert not wt_dir.exists()
            branch_check = subprocess.run(["git", "-C", str(repo), "rev-parse", "--verify", "refs/heads/feat/cli-wt"], capture_output=True, text=True)
            assert branch_check.returncode == 0
            porcelain = subprocess.run(["git", "-C", str(repo), "status", "--porcelain"], capture_output=True, text=True, check=True).stdout
            assert (repo / "parent_untracked.txt").read_text() == "keep-me\n" and "?? parent_untracked.txt" in porcelain

            # Desktop notifications (no GUI attached) + CLI settings toggle (`NT-02`, `SF-02`, `SF-03`)
            nsid, ngen = new_claude(conn, account=acc, mode="default", name="worker-alpha")

            def read_notifications():
                return [line.split("\t") for line in notify_log.read_text().splitlines() if line.strip()] if notify_log.exists() else []

            canary = "SUPER_SECRET_CANARY_991"
            assert conn.call(op="input", session=nsid, generation=ngen, request_id="n1", text=canary)["ok"] == 1
            notes = wait_for(lambda: [r for r in read_notifications() if "worker-alpha" in r[-1]], timeout=5)
            assert notes[-1][-2:] == ["Lico", "'worker-alpha' finished"]
            assert canary not in notify_log.read_text()

            # Attaching a GUI view suppresses desktop notifications
            att = conn.call(op="view.attach", session=nsid, kind="gui")
            assert att["ok"] == 1
            before = len(read_notifications())
            assert conn.call(op="input", session=nsid, generation=ngen, request_id="n2", text="quiet-turn")["ok"] == 1
            wait_for(lambda: status_of(conn, nsid)["finished_seq"] >= 4)
            assert len(read_notifications()) == before
            assert conn.call(op="view.detach", view=att["view"])["ok"] == 1

            # Turning notifications off via CLI persists 0600 settings.json and suppresses notifications even without GUI
            off = cli_run(host.socket, "settings", "--notifications", "off")
            assert off.returncode == 0
            shown = json.loads(cli_run(host.socket, "settings").stdout)
            assert shown["settings"]["notifications"] is False
            settings_file = root / "settings.json"
            assert stat.S_IMODE(settings_file.stat().st_mode) == 0o600
            assert conn.call(op="input", session=nsid, generation=ngen, request_id="n3", text="muted-turn")["ok"] == 1
            wait_for(lambda: status_of(conn, nsid)["finished_seq"] >= 6)
            assert len(read_notifications()) == before

            # Re-enabling notifications fires on approval needed too
            assert cli_run(host.socket, "settings", "--notifications", "on").returncode == 0
            assert conn.call(op="input", session=nsid, generation=ngen, request_id="n4", text="fixture")["ok"] == 1
            wait_for(lambda: len(read_notifications()) > before, timeout=5)
            assert read_notifications()[-1][-2:] == ["Lico", "'worker-alpha' needs approval"]
        finally:
            host.stop()


def test_product_window_worktree_attention_marks_and_conversation_search():
    with private_dir() as directory:
        root = Path(directory).resolve()
        repo = init_git_repo(root / "repo")
        (repo / "keep_me.txt").write_text("untouched\n")
        args, received = claude_args(directory)
        made = {}

        def setup_account(w):
            made["account"] = account(Conn(w.socket), "Work", "claude")

        with ProductWindow(host_binary=LICO, host_args=args, before_window=setup_account) as w:
            conn = Conn(w.socket)

            # 1. GUI worktree start + dirty-worktree confirmation (`WT-01`, `WT-02`)
            w.key("ctrl+n")
            w.type_text(str(repo))
            w.key("Tab")  # checks folder.git and focuses Worktree row
            form = wait_for(lambda: (f := w.ui()["panel"]["form"])["is_git"] and f)
            assert form["focus"] == "Worktree" and form["worktree"] is False
            w.key("space")
            assert w.ui()["panel"]["form"]["worktree"] is True
            w.key("Tab")
            assert w.ui()["panel"]["form"]["focus"] == "Branch"
            w.type_text("feat/gui-wt")
            w.key("Tab")
            w.type_text("gui-wt")
            w.key("Return")
            ui = wait_for(lambda: (u := w.ui())["panel"] is None and len(u["rows"]) == 1 and u)
            wt_sid = ui["selected"]
            assert "(wt)" in ui["list_text"] and ui["rows"][0]["worktree"]["branch"] == "feat/gui-wt"
            wt_dir = Path(w.directory).resolve() / "state" / "lico" / "workspaces" / "worktrees" / wt_sid
            wait_for(lambda: wt_dir.is_dir())

            # Run one quick turn so close is allowed, dirty the worktree, then Ctrl+Shift+W -> R -> D
            assert conn.call(op="input", session=wt_sid, generation=1, request_id="w1", text="hi")["ok"] == 1
            wait_for(lambda: status_of(conn, wt_sid)["activity"] == "idle")
            (wt_dir / "uncommitted.txt").write_text("dirty\n")
            w.key("ctrl+shift+w")
            panel = wait_for(lambda: w.ui()["panel"])
            assert panel["title"] == "Close worktree session"
            w.key("r")
            dirty_panel = wait_for(lambda: (p := w.ui()["panel"]) and p["title"] == "Uncommitted changes in worktree" and p)
            assert any(line.startswith("D  ") for line in dirty_panel["body"])
            w.key("d")
            wait_for(lambda: w.ui()["panel"] is None and not wt_dir.exists())
            assert (repo / "keep_me.txt").read_text() == "untouched\n"
            assert subprocess.run(["git", "-C", str(repo), "rev-parse", "--verify", "refs/heads/feat/gui-wt"], capture_output=True).returncode == 0

            # 2. Background attention (`NT-01`) and View menu notification toggle (`NT-02`)
            s_front, g_front = new_claude(conn, account=made["account"], mode="auto", name="front")
            s_back, g_back = new_claude(conn, account=made["account"], mode="auto", name="back")
            wait_for(lambda: len(w.ui()["rows"]) == 3)
            w.key("alt+2")  # switch to s_front (1 is closed gui-wt, 2 is front, 3 is back)
            wait_for(lambda: w.ui()["selected"] == s_front)

            # `stall` in s_back writes `>> stall` echo + `"stalling"` WITHOUT finishing the turn -> NOT unread!
            end_stall = Path(str(received) + ".end")
            if end_stall.exists():
                end_stall.unlink()
            assert conn.call(op="input", session=s_back, generation=g_back, request_id="b1", text="stall")["ok"] == 1
            wait_for(lambda: status_of(conn, s_back)["seq"] >= 1 and status_of(conn, s_back)["activity"] == "working")
            reload_status(w)
            mid_ui = wait_for(lambda: (u := w.ui()) and next(r for r in u["rows"] if r["session"] == s_back)["activity"] == "working" and u)
            back_row = next(r for r in mid_ui["rows"] if r["session"] == s_back)
            assert back_row["unread"] is False and "\u2022" not in mid_ui["list_text"] and "[finished]" not in w.title()

            # Let s_back finish its turn -> unread dot `•` and `[finished]` X11 title appear!
            end_stall.write_text("ok\n")
            attn_ui = wait_for(lambda: (u := w.ui()) and next(r for r in u["rows"] if r["session"] == s_back)["unread"] and u)
            assert "\u20223 back" in attn_ui["list_text"], attn_ui["list_text"]
            title_now = w.title()
            assert title_now.startswith("Lico ") and title_now.endswith(" [finished]"), title_now

            # Switching to s_back clears unread mark and restores base window title
            w.key("alt+3")
            cleared_ui = wait_for(lambda: (u := w.ui())["selected"] == s_back and not next(r for r in u["rows"] if r["session"] == s_back)["unread"] and u)
            assert "\u2022" not in cleared_ui["list_text"] and not w.title().endswith(" [finished]")

            # View menu toggles Desktop notifications off
            w.key("alt+v")
            menu_items = w.ui()["menu"]["items"]
            notif_idx = next(i for i, item in enumerate(menu_items) if "Desktop notifications: on" in item["label"])
            for _ in range(notif_idx):
                w.key("Down")
            w.key("Return")
            wait_for(lambda: w.ui()["settings"]["notifications"] is False)

            # 3. Conversation search (`Ctrl+F`) & cross-session search (`Ctrl+Shift+F`) (`SR-01`, `SR-02`)
            many = ["ANCIENT_NEEDLE_42"] + [f"padding line {i}" for i in range(55)]
            assert conn.call(op="input", session=s_front, generation=g_front, request_id="f1", text="say:" + json.dumps(many))["ok"] == 1
            assert conn.call(op="input", session=s_back, generation=g_back, request_id="b2", text="CROSS_SESSION_77")["ok"] == 1
            wait_for(lambda: status_of(conn, s_front)["seq"] >= 56 and status_of(conn, s_back)["activity"] == "idle")

            # Protocol v1 search validation + CLI `lico search`
            sr = conn.call(op="session.search", query="CROSS_SESSION_77")
            assert v1.validate_response("session.search", sr) == [] and sr["total"] >= 1
            cli_sr = cli_run(w.socket, "search", "ANCIENT_NEEDLE_42")
            assert cli_sr.returncode == 0 and "ANCIENT_NEEDLE_42" in cli_sr.stdout

            # Open s_front, type a draft, press Ctrl+F, search ANCIENT_NEEDLE_42 (scrolled > 50 lines back!)
            w.key("alt+2")
            wait_for(lambda: w.ui()["selected"] == s_front)
            w.type_text("keep my draft")
            w.key("ctrl+f")
            wait_for(lambda: (s := w.ui()["search"]) is not None and s["scope"] == "session")
            w.type_text("ANCIENT_NEEDLE_42")
            found_ui = wait_for(lambda: (u := w.ui())["search"]["count"] >= 1 and u["scroll"] > 0 and u)
            assert "ANCIENT_NEEDLE_42" in "\n".join(window_lines(w))
            assert found_ui["input"] == "keep my draft"
            w.key("Escape")
            assert w.ui()["search"] is None and w.ui()["input"] == "keep my draft"

            # Press Ctrl+Shift+F from s_front and jump directly to CROSS_SESSION_77 in s_back
            w.key("ctrl+shift+f")
            wait_for(lambda: (s := w.ui()["search"]) is not None and s["scope"] == "all")
            w.type_text("CROSS_SESSION_77")
            jumped = wait_for(lambda: (u := w.ui())["selected"] == s_back and u["search"]["count"] >= 1 and u)
            assert jumped["search"]["current"]["session"] == s_back
            assert "CROSS_SESSION_77" in "\n".join(window_lines(w))
