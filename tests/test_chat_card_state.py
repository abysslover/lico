"""M22-P04c in the window: a Chats card shows the state of its chat as a picture at the right end of its second line. A chat that is open in a tab is a disc (green while its memory is fine, orange
from the warning level, red from the limit: the colours of the tab list's figure and the same limits), a chat whose tab is closed is a ring, a chat that is not open has none. The card has the
provider as a picture (no letter), the title on the first line with the time at its right end, no word of state; the hint row says what the card says, with the figure. A private Xvfb, the
simulator that holds memory on request, synthetic homes: nothing of the real machine is looked at."""

from __future__ import annotations

import os
import re
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.account_helpers import account
from tests.test_activity import begin
from tests.test_claude_adapter import status_of
from tests.test_chats_pagination_and_resume import _records
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_new_session_screen import claude_args
from tests.test_running_elsewhere_window import _centre

SCREENS = Path(os.environ["LICO_M22_SCREENS"]) if os.environ.get("LICO_M22_SCREENS") else None


def shot(w, name):
    if SCREENS:
        SCREENS.mkdir(parents=True, exist_ok=True)
        w.png(SCREENS / name)


def run(mib, check):
    """A window with an open chat whose agent holds `mib` MiB, a chat whose tab was closed and the limits 40 and 60 MB (the tests of the memory guard use the same file)."""
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = (root / "proj-card").resolve()
        proj.mkdir(mode=0o700)
        c_args, _rec = claude_args(root)
        made = {}

        def setup(w: ProductWindow) -> None:
            conn = Conn(w.socket)
            work = account(conn, "ClaudeWork", "claude")
            heavy = conn.call(op="session.create", kind="claude", account=work, mode="auto", cwd=str(proj), name="Heavy")
            shut = conn.call(op="session.create", kind="claude", account=work, mode="auto", cwd=str(proj), name="Shut")
            assert heavy["ok"] == 1 and shut["ok"] == 1, (heavy, shut)
            begin(conn, shut["session"], shut["generation"], "initial turn")
            wait_for(lambda: "[turn completed]" in _records(root / "logs", shut["session"]))
            assert conn.call(op="session.close", session=shut["session"], generation=shut["generation"])["ok"] == 1
            wait_for(lambda: status_of(conn, shut["session"])["state"] == "disconnected")
            made.update(heavy=heavy["session"], generation=heavy["generation"], shut=shut["session"])
            setting = Path(w.directory) / "state" / "lico" / "memory.json"
            setting.write_text('{"warn_mb": 40, "limit_mb": 60}')
            setting.chmod(0o600)

        with ProductWindow(host_binary=LICO, host_args=[*c_args, "--log-dir", str(root / "logs")], env_extra={"LANG": "C.UTF-8"}, before_window=setup) as w:
            w.proc.stdin.write("load\n")
            w.proc.stdin.flush()
            wait_for(lambda: (x := w.ui()) and x["sidebar"]["shown"] is True and x)
            conn = Conn(w.socket)
            if mib:
                begin(conn, made["heavy"], made["generation"], f"hold:{mib}:0:0")
                wait_for(lambda: (m := status_of(conn, made["heavy"]).get("memory")) and m.get("max_rss_kb", 0) >= (mib + 20) * 1024 * 0.95 and m)
            w.click(*_centre(w.ui()["sidebar"]["chats_tab_rect"]))
            check(w, made)


def card_of(w, sid, memory=None):
    """The card of the chat that is open as `sid`, once it says `memory` (any level when none is asked for)."""
    def find():
        s = w.ui()["sidebar"]
        card = next((e for e in s.get("chats_entries", []) if e["open_session"] == sid), None)
        return card if card and s["chats_loading"] is False and (memory is None or card["memory"] == memory) else None
    return wait_for(find, timeout=40)


def test_an_open_chat_is_a_green_disc_a_closed_one_a_ring_and_the_card_has_a_picture_a_time_and_no_word_of_state():
    def check(w, made):
        card = card_of(w, made["heavy"], "ok")
        assert (card["state"], card["icon"], card["memory"]) == ("open", "claude", "ok"), card
        assert re.fullmatch(r"now|[0-4]m", card["age"]) and card["age_class"] == "fresh", card
        assert card["folder_shown"].endswith("proj-card"), card
        gone = card_of(w, made["shut"])
        assert (gone["state"], gone["memory"]) == ("closed", None), gone
        # the parts do not overlap: the picture is left of the text, the title stops before the time, the state's picture is on the second line under the time
        r, ic, ti, tm, fo, gl = (card[k] for k in ("rect", "icon_rect", "title_rect", "time_rect", "folder_rect", "glyph_rect"))
        assert ic[2] <= ti[0] and ti[2] <= tm[0] and fo[2] <= gl[0] and ti[3] <= fo[1] and tm[2] == gl[2], card
        assert all(r[0] <= p[0] and p[2] <= r[2] and r[1] <= p[1] and p[3] <= r[3] for p in (ic, ti, tm, fo, gl)), card
        # a click on a card still switches to its open tab and says nothing about words that are gone
        text = " ".join(str(v) for v in (card["title_shown"], card["folder_shown"], card["age"]))
        assert not re.search(r"\[(open|closed|ended|elsewhere)\]|written|[CX]·", text), text
        shot(w, "1_green_disc_and_ring.png")
    run(0, check)


def test_the_disc_turns_orange_from_the_warning_level_and_the_hint_row_says_how_much_it_holds():
    def check(w, made):
        card = card_of(w, made["heavy"], "orange")
        assert (card["state"], card["memory"]) == ("open", "orange"), card
        w.move(*_centre(card["rect"]))
        hint = wait_for(lambda: (h := w.ui()["hint"]) and "memory high" in h and h)
        assert re.search(r"open, \d+ MiB, memory high", hint) and "proj-card" in hint, hint
        shot(w, "2_orange_disc.png")
    run(22, check)


def test_the_disc_turns_red_from_the_limit_and_the_card_follows_the_tab_list_not_a_second_reading():
    def check(w, made):
        card = card_of(w, made["heavy"], "red")
        assert (card["state"], card["memory"]) == ("open", "red"), card
        tab = wait_for(lambda: next((t for t in w.ui()["tabs"]["items"] if t["session"] == made["heavy"]), None))
        w.move(*_centre(card["rect"]))
        hint = wait_for(lambda: (h := w.ui()["hint"]) and "over the limit" in h and h)
        assert "memory over the limit" in hint, hint
        assert tab, "the tab of the same session is in the tab list"
        shot(w, "3_red_disc.png")
    run(45, check)
