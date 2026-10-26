"""M02-P03-S06 in the product window: the draft in the box of one session is sent to another by Ctrl+Shift+S (the Session menu shows the entry and its key); the panel lists the other sessions by
their numbers, the person picks one by its number, and the receiver's history shows the host's header in front of the text. S05: the menu entry "Hand over conversation..." opens the same panel with the whole conversation chosen."""
import json
import re
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir

from tests.test_claude_adapter import new_claude, records
from tests.test_codex_adapter import wait_for
from tests.test_new_session_screen import claude_args
from tests.test_recovery import registered
from tests.test_session_actions_window import panel, row_centre

DRAFT = "please check the login test"


def centre(rect):                                                                                 # not taken from test_menu_bar: a change there must not select this test
    return (rect[0] + rect[2]) / 2, (rect[1] + rect[3]) / 2


def menu(w):
    return w.ui()["menu"]


def item(m, text):
    return next(i for i in m["items"] if text in i["label"])


HEADER = re.compile(r">> \[from session 'api-server' \((s[0-9a-f]{6})\) \| \d\d:\d\d \| message \| 27 B\]")


def test_a_draft_goes_by_the_chord_to_the_session_picked_by_its_number_and_the_receiver_history_shows_the_hosts_header():
    with private_dir() as directory:
        args, _ = claude_args(directory)
        log = Path(directory) / "log"
        made = {}

        def sessions(w):
            conn = Conn(w.socket)
            made["api"], _ = new_claude(conn, name="api-server")
            made["docs"], _ = new_claude(conn, name="docs")
            made["notes"], _ = new_claude(conn, name="notes")

        with ProductWindow(host_binary=LICO, host_args=[*args, "--log-dir", str(log)], before_window=sessions) as w:
            api, docs, notes = made["api"], made["docs"], made["notes"]
            wait_for(lambda: len(w.ui()["rows"]) == 3)
            w.click(*row_centre(w, api))
            assert w.ui()["selected"] == api
            w.type_text(DRAFT)
            w.click(*centre(menu(w)["titles"][1]["rect"]))                                        # Session
            entry = item(menu(w), "Send to session")
            assert entry["enabled"] and entry["key"] == "Ctrl+Shift+S", entry
            w.key("Escape")                                                                       # the menu shows the entry and its key; the key itself opens the panel
            assert menu(w)["open"] is None and panel(w) is None
            w.key("ctrl+shift+s")
            asked = panel(w)
            assert asked and asked["title"] == "Send to session", asked
            body = asked["body"]
            line = next(l for l in body if "docs" in l)
            number = re.search(r"\[ \] (\d)\s+docs", line)
            assert number, body                                                                   # nobody is chosen yet: there are two to choose from
            assert any("Draft text" in l and "(27 B)" in l for l in body), body
            w.key("Return")                                                                       # no receiver: the panel stays and says so
            assert panel(w)["title"] == "Send to session" and any("Pick the session" in l for l in panel(w)["body"]), panel(w)
            w.key(number.group(1))
            line = next(l for l in panel(w)["body"] if "docs" in l)
            assert "[x]" in line, line
            w.key("Return")
            state = w.ui()
            assert state["panel"] is None and state["input"] == "" and "Sent to 'docs' (inline, 27 B)" in state["header"], state["header"]
            wait_for(lambda: [r for r in records(log, docs) if HEADER.fullmatch(r)])
            shown = [r for r in records(log, docs) if r.startswith(">> ")]
            assert [r for r in shown if HEADER.fullmatch(r)] and f">> {DRAFT}" in shown, shown
            assert not [r for r in records(log, api) if r.startswith(">> ")] and not [r for r in records(log, notes) if r.startswith(">> ")]


CONVERSATION = re.compile(r">> \[from session 'api-server' \((s[0-9a-f]{6})\) \| \d\d:\d\d \| conversation \| \d+ B, \d+ lines?\]")


def test_the_whole_conversation_is_handed_over_from_the_session_menu_and_the_receiver_gets_a_file_the_header_names_without_a_path():
    with private_dir() as directory:
        args, _ = claude_args(directory)
        log = Path(directory) / "log"
        provider_home = Path(directory) / "claude_home"
        made = {}

        def sessions(w):
            conn = Conn(w.socket)
            made["api"], generation = new_claude(conn, name="api-server")
            made["docs"], _ = new_claude(conn, name="docs")
            assert conn.call(op="input", session=made["api"], generation=generation, request_id="first", text="hi")["ok"] == 1     # the program tells its own session id with its first turn
            wait_for(lambda: "[turn completed]" in records(log, made["api"]))
            state = Path(w.directory) / "state" / "lico"
            made["provider_id"] = wait_for(lambda: registered(state).get(made["api"], {}).get("provider_session"))

        with ProductWindow(host_binary=LICO, host_args=[*args, "--log-dir", str(log)], before_window=sessions, env_extra={"CLAUDE_CONFIG_DIR": str(provider_home)}) as w:
            api, docs = made["api"], made["docs"]
            record = provider_home / "projects" / "-work-proj" / f"{made['provider_id']}.jsonl"
            record.parent.mkdir(parents=True)
            record.write_text("".join(json.dumps(m) + "\n" for m in (
                {"type": "user", "version": "2.1.310", "timestamp": "2026-10-27T04:12:03.000Z", "message": {"role": "user", "content": "how many files?"}},
                {"type": "assistant", "version": "2.1.310", "timestamp": "2026-10-27T04:12:09.000Z", "message": {"role": "assistant", "content": [{"type": "text", "text": "Twelve."}]}})))
            wait_for(lambda: len(w.ui()["rows"]) == 2)
            w.click(*row_centre(w, api))
            assert w.ui()["selected"] == api
            w.click(*centre(menu(w)["titles"][1]["rect"]))                                        # Session
            entry = item(menu(w), "Hand over conversation")
            assert entry["enabled"], entry
            w.click(*centre(entry["rect"]))
            asked = panel(w)
            assert asked and asked["title"] == "Send to session", asked
            body = asked["body"]
            assert any("[x] C  Whole conversation" in l for l in body), body                    # the menu entry opens the panel with the whole conversation chosen
            assert any("as a file. The host reads the program's record" in l for l in body) and any("Careful" in l for l in body), body
            assert any("[x]" in l and "docs" in l for l in body), body                          # one other session: there is nothing to choose
            go = next(l for l in body if l.startswith("Enter  "))
            assert go == "Enter  Hand over to 'docs'", go
            w.key("Return")
            state = w.ui()
            assert state["panel"] is None and "Handed over the conversation to 'docs' (as a file," in state["header"], state["header"]
            shown = wait_for(lambda: [r for r in records(log, docs) if CONVERSATION.fullmatch(r)])
            assert shown and "/" not in shown[0] and "handoff" not in shown[0], shown            # the header says where it is from and how big it is, and no path
            assert [r for r in records(log, api) if r.startswith(">> ")] == [">> hi"]
