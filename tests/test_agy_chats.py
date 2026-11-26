"""M23-P04b and M25-P05: the conversations of `agy` are cards in Chats, and a click on one brings it back as a session of Lico's. `agy` keeps `~/.gemini/antigravity-cli/history.jsonl`; the test host
and the window have a private HOME (`<run folder>/home`), so these tests write a synthetic file there and never read the owner's. The program the host runs is the simulator of `agy acp`."""

from __future__ import annotations

import json
import os
import time
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.gui_helpers import centre
from tests.test_agy_session import sent, simulator_args, started_as
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_terminal_tab import rig, shot

A = "11111111-aaaa-4bbb-8ccc-000000000001"
B = "22222222-aaaa-4bbb-8ccc-000000000002"
C = "33333333-aaaa-4bbb-8ccc-000000000003"


def history_file(root: Path) -> Path:
    return root / "home" / ".gemini" / "antigravity-cli" / "history.jsonl"


def write_history(root: Path, lines: list, mode: int = 0o600) -> Path:
    """The file `agy` keeps, with `lines` (a dict becomes a JSON line, a string stays as it is)."""
    file = history_file(root)
    file.parent.mkdir(parents=True, exist_ok=True)
    file.write_text("\n".join(x if isinstance(x, str) else json.dumps(x) for x in lines) + "\n")
    file.chmod(mode)
    return file


def prompt(text: str, millis: int, folder: str, conversation: str | None, **more) -> dict:
    entry = {"display": text, "timestamp": millis, "workspace": folder, **more}
    if conversation:
        entry["conversationId"] = conversation
    return entry


def test_the_agy_conversations_join_the_list_of_this_machine_when_the_window_asks_and_in_no_other_list():
    from lico_measure.p02 import start
    from lico_protocol import v1
    from tests.account_helpers import account
    from tests.test_chats_pagination_and_resume import _seed_claude_jsonl
    from tests.test_claude_adapter import argv_of  # noqa: F401  (the rig's helper module)
    from tests.test_new_session_screen import claude_args

    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = (root / "proj-alpha").resolve()
        proj.mkdir(mode=0o700, parents=True)
        c_args, _rec = claude_args(root)
        host, _log_dir = start(directory, extra=["--text", "--state-dir", str(root / "state"), *c_args], sessions=0, producing=0)
        try:
            conn = Conn(host.socket)
            acc = account(conn, "LocalClaude", "claude")
            home = root / "state" / "accounts" / acc / "claude"
            _seed_claude_jsonl(home, "proj-alpha", "cl-a1", str(proj), "alpha question", "answer", 1_760_000_030)
            file = write_history(root, [
                prompt("first of A", 1_760_000_010_000, "/work/one", A),
                prompt("/model", 1_760_000_011_000, "/work/one", A, type="slash_command"),
                prompt("beta conversation", 1_760_000_050_000, "/work/two/", B),
                prompt("later of A", 1_760_000_060_000, "/work/one/inner", A),
                prompt("no id yet", 1_760_000_070_000, "/work/one", None),
                prompt("not a token", 1_760_000_071_000, "/work/one", "../x"),
                prompt("old one", 1_759_000_000_000, "/work/three", C),
            ])

            def ask(**extra):
                reply = conn.call(op="session.chats", all=True, **extra)
                assert reply["ok"] == 1, reply
                return reply

            def ids(reply):
                return [it["provider_session"] for it in reply["items"]]

            plain = ask()
            assert ids(plain) == ["cl-a1"] and plain["total"] == 1, "no `agy`: what it was"
            both = ask(agy=True)
            assert v1.validate_request("session.chats", {"op": "session.chats", "all": True, "agy": True}) == []
            assert v1.validate_response("session.chats", both) == [], both
            assert ids(both) == [A, B, "cl-a1", C], "one list, newest first: A was last asked at 1_760_000_060"
            assert both["total"] == 4 and both["has_more"] is False
            by = {it["provider_session"]: it for it in both["items"]}
            a = by[A]
            assert (a["provider"], a["title"], a["cwd"], a["updated_at"], a["account"], a["global"], a["open_session"], a["ssh_host"]) == ("agy", "first of A", "/work/one/inner", 1_760_000_060, None, False, None, None), a
            assert (by[B]["title"], by[B]["cwd"]) == ("beta conversation", "/work/two"), "the folder without its trailing slash"
            # the page boundary: a walk of two pages of two loses nothing
            assert [i for off in (0, 2) for i in ids(ask(agy=True, offset=off, limit=2))] == ids(both)
            # the query matches the title and the id; the provider's own name is in every card of it
            assert ids(ask(agy=True, query="beta")) == [B]
            assert ids(ask(agy=True, query="22222222")) == [B]
            assert ids(ask(agy=True, query="agy")) == [A, B, C]
            # an account has no `agy` conversations; the other scopes ignore the field; a false flag is no flag
            assert ids(ask(agy=True, account=acc)) == ["cl-a1"]
            assert conn.call(op="session.chats", cwd=str(proj), agy=True)["total"] == 1
            assert conn.call(**{"op": "session.chats", "global": True, "agy": True})["total"] == 0, "the Home scope has no conversation of agy (and the claude chat is in a folder of its own)"
            assert ask(agy=False)["total"] == 1
            assert conn.call(op="session.chats", all=True, agy="yes")["ok"] != 1, "the field is a boolean"
            # a file that is not the owner's alone, a damaged one and a missing one are an empty list of conversations
            file.chmod(0o640)
            assert ask(agy=True)["total"] == 1
            file.chmod(0o600)
            file.write_text("{damaged")
            assert ask(agy=True)["total"] == 1
            file.unlink()
            assert ask(agy=True)["total"] == 1
            # a change of the file is seen at the next ask (the host does not keep what it read)
            write_history(root, [prompt("brand new", 1_760_000_099_000, "/work/new", B)])
            assert ids(ask(agy=True)) == [B, "cl-a1"]
        finally:
            host.stop()


def agy_cards(w: ProductWindow) -> list[dict]:
    """The cards of `agy` conversations in the Chats tab, once its list has come (the Chats tab is opened first: a switch to it fetches again)."""
    s = w.ui()["sidebar"]
    if s["tab"] != 1:
        w.click(*centre(s["chats_tab_rect"]))
        time.sleep(0.2)
    return wait_for(lambda: (s := w.ui()["sidebar"]) and s["chats_loading"] is False and s["chats_load_count"] > 0 and [e for e in s["chats_entries"] if e["provider"] == "agy"] or None)


def card_of(w: ProductWindow, conversation: str) -> dict:
    return next(c for c in agy_cards(w) if c["provider_session"] == conversation)


def test_a_click_on_an_agy_card_resumes_the_conversation_as_a_session_that_asks_and_a_second_click_shows_that_session():
    """M23-P04b and M25-P05 in the real window: the cards of two conversations (one folder exists, one does not) and a click that brings the conversation back as a session of Lico's, asking before
    it acts, in the conversation's folder (a workspace of the host's own when the folder is gone); the same card again shows that session and starts nothing. The host of the window runs the simulator."""
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        proj = (root / "proj").resolve()
        proj.mkdir()
        received = root / "agy_received.txt"
        Path(f"{received}.sessions").write_text(json.dumps({A: [{"role": "user", "text": "resume me"}], B: [{"role": "user", "text": "folder is gone"}]}), encoding="utf-8")
        with ProductWindow(**rig(root, host_args=simulator_args(received))) as w:
            conn = Conn(w.socket)
            write_history(Path(w.directory), [prompt("resume me", 1_760_000_010_000, str(proj), A), prompt("folder is gone", 1_760_000_020_000, "/nonexistent-folder-for-agy", B)])
            cards = agy_cards(w)
            assert [c["provider_session"] for c in cards] == [B, A], "newest first"
            assert all(c["icon"] == "agy" and c["state"] == "none" and c["open_session"] is None for c in cards), cards
            assert [c["title"] for c in cards] == ["folder is gone", "resume me"]
            shot(w, "9_agy_cards.png")
            # a click: a session in the conversation's folder, resumed with that conversation, asking before it acts
            w.click(*centre(card_of(w, A)["rect"]))
            wait_for(lambda: (u := w.ui())["panel"] is None and len(u["rows"]) == 1)
            [made] = conn.call(op="status")["sessions"]
            assert made["provider"] == "agy" and made["mode"] == "default", made
            wait_for(lambda: [m["params"]["sessionId"] for m in sent(received, "session/resume")] == [A])
            assert sent(received, "session/new") == [], "never a new conversation in its place"
            assert os.path.realpath(wait_for(lambda: started_as(received))[0]["cwd"]) == str(proj)
            assert w.ui()["terminal"]["tabs"] == [] and conn.call(op="terminal.list")["terminals"] == [], "a session of Lico's, no terminal"
            shot(w, "10_agy_conversation_as_a_session.png")
            # a conversation whose folder is gone is brought back in a workspace of the host's own
            w.click(*centre(card_of(w, B)["rect"]))
            wait_for(lambda: len(conn.call(op="status")["sessions"]) == 2)
            wait_for(lambda: len(started_as(received)) == 2)
            assert "/workspaces/" in started_as(received)[1]["cwd"]
            # the same card again: the session that holds it is shown, no third is started
            w.click(*centre(card_of(w, A)["rect"]))
            wait_for(lambda: w.ui()["selected"] == made["session"])
            assert len(conn.call(op="status")["sessions"]) == 2 and len(started_as(received)) == 2
