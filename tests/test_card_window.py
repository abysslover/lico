"""M02-P03-S04b in the product window: a send that an agent asks for is a card on the row of the agent's session; V shows the whole text it would send, page by page,
Y and N answer it, and S allows the later sends to that session too. A create (S04c) is a card too, with no S: a session is made once for each yes. The agent here is
the simulator's program; its call is made with the config file it was given."""
import json
import re
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir

from tests.test_claude_adapter import new_claude, records
from tests.test_codex_adapter import wait_for
from tests.test_new_session_screen import claude_args
from tests.test_session_actions_window import panel, row_centre
from tests.test_tool_connection import config_of, rpc

HEADER = re.compile(r">> \[from session 'writer' \(s[0-9a-f]{6}\) \| \d\d:\d\d \| message \| [^\]]+\]")
PAGE = 16 * 1024


def ask(directory, caller, to, text):
    """The agent's call of `send`, answered at once with the card it left."""
    status, body = rpc(config_of(Path(directory) / "state" / "lico", caller), "tools/call", {"name": "send", "arguments": {"to": to, "text": text}})
    assert status == 200, (status, body)
    return json.loads(body["result"]["content"][0]["text"])


def ask_create(directory, caller, prompt, name):
    """The agent's call of `create`, answered at once with the card it left."""
    status, body = rpc(config_of(Path(directory) / "state" / "lico", caller), "tools/call", {"name": "create", "arguments": {"prompt": prompt, "name": name}})
    assert status == 200, (status, body)
    return json.loads(body["result"]["content"][0]["text"])


def test_the_text_behind_a_card_is_read_page_by_page_in_the_window_and_y_s_and_n_answer_it_for_that_card():
    with private_dir() as directory:
        args, _ = claude_args(directory)
        log = Path(directory) / "log"
        made = {}

        def sessions(w):
            conn = Conn(w.socket)
            made["writer"], _ = new_claude(conn, name="writer")
            made["docs"], _ = new_claude(conn, name="docs")
            made["notes"], _ = new_claude(conn, name="notes")

        text = "".join(f"row {i:04d} {'x' * 30}\n" for i in range(1000))                              # 41 000 bytes: three pages
        with ProductWindow(host_binary=LICO, host_args=[*args, "--log-dir", str(log)], before_window=sessions) as w:
            writer, docs, notes = made["writer"], made["docs"], made["notes"]
            wait_for(lambda: len(w.ui()["rows"]) == 3)
            w.click(*row_centre(w, writer))
            assert w.ui()["selected"] == writer
            pending = ask(w.directory, writer, docs, text)
            assert pending["status"] == "pending", pending
            number = pending["approval_id"]
            state = wait_for(lambda: w.ui()["approval_bar"] and w.ui())
            assert state["approval_bar"].startswith(f"approval {number}: send to 'docs'"), state["approval_bar"]
            w.move(700, 400)                                                                           # off the list: over a row, the hint line shows its memory instead of the keys
            state = wait_for(lambda: w.ui() if w.ui()["hint"].startswith("Y allow") else None)
            assert state["hint"].startswith("Y allow  N deny  S allow for the session  V read it all"), state["hint"]
            w.key("v")
            shown = panel(w)
            assert shown["title"] == f"Request {number}" and shown["body"][1] == f"bytes 0 to {PAGE} of {len(text)} (page 1 of 3)", shown["body"]
            assert "row 0000" in shown["body"][3] and "row 0001" in shown["body"][4], shown["body"]
            w.key("Next")                                                                              # Page Down: fourteen lines on
            assert "row 0014" in panel(w)["body"][3], panel(w)["body"]
            w.key("End")                                                                               # the last page, at its end
            last = panel(w)
            assert last["body"][1] == f"bytes {2 * PAGE} to {len(text)} of {len(text)} (page 3 of 3)" and "row 0999" in "\n".join(last["body"]), last["body"]
            w.key("Home")
            assert panel(w)["body"][1].startswith("bytes 0 to ") and "row 0000" in panel(w)["body"][3]
            w.key("Escape")
            assert panel(w) is None and w.ui()["approval_bar"], "leaving the view leaves the request asked"
            w.key("v")
            w.key("y")                                                                                 # the answer to the request that was read
            state = w.ui()
            assert state["panel"] is None and not state["approval_bar"] and "Allowed: the text was sent." in state["header"], state["header"]
            wait_for(lambda: [r for r in records(log, docs) if HEADER.fullmatch(r)])
            shown = records(log, docs)                                                                 # more than the inline limit: the receiver is told it is a file, with the first lines
            assert next(r for r in shown if HEADER.fullmatch(r)).endswith("| message | 39.1 KiB, 1000 lines]") and ">>   row 0000 " + "x" * 30 in shown, shown

            pending = ask(w.directory, writer, docs, "the second")                                       # S: this one and the later ones to that session
            wait_for(lambda: w.ui()["approval_bar"])
            w.key("s")
            assert "Allowed for the session" in w.ui()["header"] and not w.ui()["approval_bar"], w.ui()["header"]
            wait_for(lambda: ">> the second" in records(log, docs))
            wait_for(lambda: "[turn completed]" in records(log, docs))
            assert ask(w.directory, writer, docs, "the third") == {"sent": True, "to": docs, "delivery": "inline", "size": 9, "receiver": "idle"}
            wait_for(lambda: ">> the third" in records(log, docs))
            assert not w.ui()["approval_bar"]

            ask(w.directory, writer, notes, "the fourth")                                              # N, for another receiver (the allowance is for docs only): nothing is sent
            wait_for(lambda: w.ui()["approval_bar"])
            w.key("n")
            assert "Declined: nothing was sent." in w.ui()["header"] and not w.ui()["approval_bar"], w.ui()["header"]
            wait_for(lambda: [r for r in records(log, writer) if re.fullmatch(r">> \[from lico \| \d\d:\d\d \| approval \d+\]", r)][-1:] and len([r for r in records(log, writer) if r.startswith(">> [from lico | ")]) == 3)
            assert ">> the fourth" not in records(log, notes), "a request that was denied sends nothing"

            dirty = "\x1b[31mred\x1b[0m\u202eevil\u200b\r\nsecond\U000e0041 line"                       # the window drops some of this on its own; the host has cleaned it before the card
            number = ask(w.directory, writer, notes, dirty)["approval_id"]
            state = wait_for(lambda: w.ui()["approval_bar"] and w.ui())
            assert state["approval_bar"].startswith(f"approval {number}: send to 'notes'") and "19 B, 2 lines | 12 hidden characters removed | \"redevil\"" in state["approval_bar"], state["approval_bar"]
            w.key("v")
            shown = "\n".join(panel(w)["body"])
            assert "\nredevil\nsecond line\n" in shown and "[31m" not in shown and "\x1b" not in shown, shown
            w.key("y")
            wait_for(lambda: ">> second line" in records(log, notes))
            assert ">> redevil" in records(log, notes) and not any(c in r for r in records(log, notes) for c in "\x1b\u202e\u200b\r"), records(log, notes)

            number = ask_create(w.directory, writer, "write the notes down", "scribe")["approval_id"]                  # a create: the same keys but S, and a yes makes one session
            state = wait_for(lambda: w.ui()["approval_bar"] and w.ui())
            assert state["approval_bar"].startswith(f"approval {number}: create session 'scribe' ") and "mode: asks for approval" in state["approval_bar"], state["approval_bar"]
            state = wait_for(lambda: w.ui() if w.ui()["hint"].startswith("Y allow") else None)
            assert state["hint"].startswith("Y allow  N deny  V read it all") and "S allow" not in state["hint"], state["hint"]
            assert len(w.ui()["rows"]) == 3, "nothing is made until the answer"
            w.key("y")
            assert "Allowed: the session was made and is working on its first prompt." in w.ui()["header"], w.ui()["header"]
            wait_for(lambda: len(w.ui()["rows"]) == 4)
            assert "scribe - made by writer" in [r.get("name") for r in Conn(w.socket).call(op="status")["sessions"]]
