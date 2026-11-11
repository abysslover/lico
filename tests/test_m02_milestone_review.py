"""M02-P07: memory pass (`MB-02`, `MB-06`) and full-milestone review (`R1`..`R5`) regression checks."""
import os
import struct
import subprocess
import zlib
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.account_helpers import account
from tests.test_claude_adapter import new_claude, records, status_of
from tests.test_codex_adapter import wait_for
from tests.test_everyday_comfort import fast_key, init_git_repo, reload_status
from tests.test_new_session_screen import claude_args
from tests.test_picture_paste import Owners
from tests.test_session_actions_window import row_centre


def rss_anon_kib(pid: int) -> int:
    for line in Path(f"/proc/{pid}/status").read_text().splitlines():
        if line.startswith("RssAnon:"):
            return int(line.split()[1])
    return 0


def uncompressed_png(width: int, height: int) -> bytes:
    """Build a valid RGB PNG whose compressed IDAT payload is ~2.3 MiB of incompressible random bytes."""
    row_len = 1 + width * 3
    raw = bytearray(os.urandom(row_len * height))
    for y in range(height):
        raw[y * row_len] = 0
    idat = zlib.compress(bytes(raw), level=0)

    def chunk(tag: bytes, body: bytes) -> bytes:
        return struct.pack(">I", len(body)) + tag + body + struct.pack(">I", zlib.crc32(tag + body) & 0xFFFFFFFF)

    ihdr = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", idat) + chunk(b"IEND", b"")


def test_m02_milestone_review_labels_search_cause_and_heap_trim():
    png = uncompressed_png(960, 800)
    assert 2_100_000 <= len(png) <= 2_900_000, len(png)

    with private_dir() as directory:
        root = Path(directory).resolve()
        repo = init_git_repo(root / "myrepo")
        log_dir = root / "logs"
        args, _ = claude_args(directory)
        made = {}

        def setup(w):
            conn = Conn(w.socket)
            acc = account(conn, "Work", "claude")
            made["acc"] = acc
            wt_reply = conn.call(op="session.create", kind="claude", account=acc, cwd=str(repo), worktree_branch="feat/wt-unnamed")
            assert wt_reply.get("ok") == 1, wt_reply
            made["wt"] = wt_reply["session"]
            made["peer"], _ = new_claude(conn, account=acc, mode="auto", name="peer")

        with ProductWindow(host_binary=LICO, host_args=[*args, "--log-dir", str(log_dir)], before_window=setup) as w, Owners(w.xvfb.display, root) as owners:
            conn = Conn(w.socket)
            sid_wt, sid_peer = made["wt"], made["peer"]

            # R3 (before prompt): GUI list row shows 'myrepo' (and '(wt)'), never the raw session id prefix
            reload_status(w)
            ui_pre = wait_for(lambda: (u := w.ui()) and len(u["rows"]) == 2 and u)
            assert "myrepo" in ui_pre["list_text"] and "(wt)" in ui_pre["list_text"] and sid_wt[:7] not in ui_pre["list_text"], ui_pre["list_text"]

            assert conn.call(op="input", session=sid_wt, generation=1, request_id="seed-wt", text="worktree_unique_needle")["ok"] == 1
            wait_for(lambda: status_of(conn, sid_wt).get("activity") == "idle" and status_of(conn, sid_wt).get("seq", 0) >= 2 or None)

            # R3: session.search and transfer.send both call the unnamed worktree session 'myrepo', not its raw worktree directory id
            sr = conn.call(op="session.search", query="worktree_unique_needle")
            assert sr.get("ok") == 1 and sr["total"] >= 1, sr
            assert all(m["name"] == "myrepo" for m in sr["matches"] if m["session"] == sid_wt), sr

            tr = conn.call(op="transfer.send", **{"from": sid_wt, "to": sid_peer, "kind": "text", "text": "hello from unnamed wt"})
            assert tr.get("ok") == 1, tr
            wait_for(lambda: any("[from session 'myrepo'" in ln for ln in records(log_dir, sid_peer)) or None)

            reload_status(w)
            ui0 = wait_for(lambda: (u := w.ui()) and any(r["session"] == sid_wt and r["auto_title"] == "worktree_unique_needle" for r in u["rows"]) and u)
            # R3 (after prompt): derived auto_title replaces generic folder name while keeping '(wt)', never raw session id prefix
            assert "worktree\u2026" in ui0["list_text"] and "(wt)" in ui0["list_text"] and sid_wt[:7] not in ui0["list_text"], ui0["list_text"]

            # Select peer session
            w.click(*row_centre(w, sid_peer))
            wait_for(lambda: w.ui()["selected"] == sid_peer or None)
            causes_before = dict(w.ui()["frame"]["causes"])
            assert "search" in causes_before

            # R1: opening and closing Ctrl+F attributes full repaint to Cause::Search, not Cause::Selection
            fast_key(w, "ctrl+f")
            ui_search = wait_for(lambda: (u := w.ui()) and u["search"] is not None and u)
            assert ui_search["frame"]["causes"]["search"] > causes_before["search"], ui_search["frame"]["causes"]
            assert ui_search["frame"]["causes"]["selection"] == causes_before["selection"], ui_search["frame"]["causes"]
            fast_key(w, "Escape")
            wait_for(lambda: (w.ui()["search"] is None) or None)

            # R5: selecting conversation lines and pressing plain Ctrl+C copies them to CLIPBOARD and Ctrl+V pastes them
            lines = wait_for(lambda: (ls := w.ui()["layout"]["lines"]) and len(ls) >= 2 and ls)
            y0, y1 = (lines[0][0] + lines[0][1]) / 2, (lines[1][0] + lines[1][1]) / 2
            wid = w.window_id()
            subprocess.run(["xdotool", "mousemove", "--window", wid, "500", str(int(y0)), "mousedown", "1", "mousemove", "--window", wid, "500", str(int(y1)), "mouseup", "1"], env=w.env, check=True)
            sel_text = wait_for(lambda: w.ui()["selection"] or None)
            copied_before = w.ui()["copied"]
            fast_key(w, "ctrl+c")
            wait_for(lambda: w.ui()["copied"] > copied_before or None)
            clip_out = subprocess.run(["xclip", "-o", "-selection", "clipboard"], env={"DISPLAY": w.xvfb.display, "PATH": "/usr/bin:/bin"}, capture_output=True, text=True, check=True).stdout
            assert clip_out == sel_text and "Copied " in w.ui()["header"], (clip_out, sel_text, w.ui()["header"])
            fast_key(w, "ctrl+v")
            wait_for(lambda: w.ui()["editor"]["text"] == sel_text or None)
            fast_key(w, "ctrl+u")
            wait_for(lambda: w.ui()["editor"]["text"] == "" or None)
            fast_key(w, "Escape")
            wait_for(lambda: w.ui()["selection"] is None or None)

            # R4 / MB-06: pasting a ~2.3 MiB PNG chip increases GUI RssAnon, and Escape reclaims it immediately via malloc_trim(0)
            anon_before = rss_anon_kib(w.proc.pid)
            owners.png(png)
            fast_key(w, "ctrl+v")
            wait_for(lambda: w.ui()["editor"]["picture"] or None)
            owners.nothing()
            anon_peak = rss_anon_kib(w.proc.pid)
            assert anon_peak - anon_before >= 1800, (anon_before, anon_peak)

            fast_key(w, "Escape")
            wait_for(lambda: (w.ui()["editor"]["picture"] is None) or None)
            anon_after = rss_anon_kib(w.proc.pid)
            assert anon_peak - anon_after >= 1500, (anon_before, anon_peak, anon_after)

            # MB-02: Escape with empty box hides/detaches the view cleanly
            fast_key(w, "Escape")
            wait_for(lambda: (not w.ui()["selected"]) or None)
