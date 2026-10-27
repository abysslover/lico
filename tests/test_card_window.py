"""M02-P03-S04b in the product window: a send that an agent asks for is a card on the row of the agent's session; V shows the whole text it would send, page by page,
Y and N answer it once it has been on screen for a moment, and Shift+S allows the later sends to that session too. A create (S04c) is a card too, with no S: a session is made once for each yes. The agent here is
the simulator's program; its call is made with the config file it was given."""
import json
import re
from pathlib import Path

from lico_measure.captures import GRAY_THRESHOLD, ProductWindow
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


def ready(w):
    """The card has been on screen for the dwell (here the window's own clock runs): from now a bare key is an answer."""
    return wait_for(lambda: w.ui() if w.ui()["card_ready"] else None)


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
            assert state["hint"].startswith("Y allow  N deny  Shift+S allow for the session  V read it all"), state["hint"]
            ready(w)
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
            ready(w)
            w.key("v")
            w.key("y")                                                                                 # the answer to the request that was read
            state = w.ui()
            assert state["panel"] is None and not state["approval_bar"] and "Allowed: the text was sent." in state["header"], state["header"]
            wait_for(lambda: [r for r in records(log, docs) if HEADER.fullmatch(r)])
            shown = records(log, docs)                                                                 # more than the inline limit: the receiver is told it is a file, with the first lines
            assert next(r for r in shown if HEADER.fullmatch(r)).endswith("| message | 39.1 KiB, 1000 lines]") and ">>   row 0000 " + "x" * 30 in shown, shown

            pending = ask(w.directory, writer, docs, "the second")                                       # S: this one and the later ones to that session
            wait_for(lambda: w.ui()["approval_bar"])
            ready(w)
            w.key("s")
            assert w.ui()["input"] == "s" and w.ui()["approval_bar"], "a small s is typing"
            w.key("BackSpace")
            w.key("shift+s")
            assert "Allowed for the session" in w.ui()["header"] and not w.ui()["approval_bar"], w.ui()["header"]
            wait_for(lambda: ">> the second" in records(log, docs))
            wait_for(lambda: "[turn completed]" in records(log, docs))
            assert ask(w.directory, writer, docs, "the third") == {"sent": True, "to": docs, "delivery": "inline", "size": 9, "receiver": "idle"}
            wait_for(lambda: ">> the third" in records(log, docs))
            assert not w.ui()["approval_bar"]

            ask(w.directory, writer, notes, "the fourth")                                              # N, for another receiver (the allowance is for docs only): nothing is sent
            wait_for(lambda: w.ui()["approval_bar"])
            ready(w)
            w.key("n")
            assert "Declined: nothing was sent." in w.ui()["header"] and not w.ui()["approval_bar"], w.ui()["header"]
            wait_for(lambda: [r for r in records(log, writer) if re.fullmatch(r">> \[from lico \| \d\d:\d\d \| approval \d+\]", r)][-1:] and len([r for r in records(log, writer) if r.startswith(">> [from lico | ")]) == 3)
            assert ">> the fourth" not in records(log, notes), "a request that was denied sends nothing"

            dirty = "\x1b[31mred\x1b[0m\u202eevil\u200b\r\nsecond\U000e0041 line"                       # the window drops some of this on its own; the host has cleaned it before the card
            number = ask(w.directory, writer, notes, dirty)["approval_id"]
            state = wait_for(lambda: w.ui()["approval_bar"] and w.ui())
            assert state["approval_bar"].startswith(f"approval {number}: send to 'notes'") and "19 B, 2 lines | 12 hidden characters removed | \"redevil\"" in state["approval_bar"], state["approval_bar"]
            ready(w)
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
            ready(w)
            w.key("y")
            assert "Allowed: the session was made and is working on its first prompt." in w.ui()["header"], w.ui()["header"]
            wait_for(lambda: len(w.ui()["rows"]) == 4)
            assert "scribe - made by writer" in [r.get("name") for r in Conn(w.socket).call(op="status")["sessions"]]

            ask(w.directory, writer, notes, "the fifth")                                               # S in the view is the capital too: a small s there keeps nothing
            wait_for(lambda: w.ui()["approval_bar"])
            ready(w)
            w.key("v")
            assert panel(w) is not None
            w.key("s")
            assert panel(w) is not None and w.ui()["approval_bar"] and ">> the fifth" not in records(log, notes)
            w.key("shift+s")
            assert "Allowed for the session" in w.ui()["header"] and not w.ui()["approval_bar"], w.ui()["header"]
            wait_for(lambda: ">> the fifth" in records(log, notes))


def clock(w, *words):
    """The window's test clock: `freeze` stops the instant that the dwell of a card counts to, `advance MS` moves it on, nothing lets it run."""
    w.proc.stdin.write(" ".join(["clock", *words]) + "\n")
    w.proc.stdin.flush()


def centre(box):
    return (box[0] + box[2]) / 2, (box[1] + box[3]) / 2


def fill_of(w, name):
    """The gray of one pixel inside a button, at its left edge half way down, where its label does not reach."""
    box, (data, width, _) = w.ui()["layout"][name], w.gray()
    return data[int((box[1] + box[3]) / 2) * width + int(box[0]) + 2]


def steady_fill(w, name):
    """The gray of a button's fill once two captures in a row agree: a capture can be of the frame before the one the flag speaks of."""
    seen = [fill_of(w, name)]

    def agreed():
        seen.append(fill_of(w, name))
        return seen[-1] == seen[-2]

    wait_for(agreed)
    return seen[-1]


def test_a_key_typed_while_a_card_comes_up_is_typing_and_a_key_after_the_dwell_is_the_answer():
    with private_dir() as directory:
        args, _ = claude_args(directory)
        log = Path(directory) / "log"
        made = {}

        def sessions(w):
            conn = Conn(w.socket)
            made["writer"], _ = new_claude(conn, name="writer")
            made["docs"], _ = new_claude(conn, name="docs")

        with ProductWindow(host_binary=LICO, host_args=[*args, "--log-dir", str(log)], before_window=sessions) as w:
            writer, docs = made["writer"], made["docs"]
            wait_for(lambda: len(w.ui()["rows"]) == 2)
            w.click(*row_centre(w, writer))
            assert w.ui()["selected"] == writer
            clock(w, "freeze")                                                                         # the dwell is counted on a clock that stands still
            assert ask(w.directory, writer, docs, "the first")["status"] == "pending"
            wait_for(lambda: w.ui()["approval_bar"] and w.ui()["card_seen"])                           # a frame has drawn the card: the count starts there
            assert not w.ui()["card_ready"]
            assert w.ui()["layout"]["dim"], "the buttons are drawn dimmed while the card may not be answered"
            w.click(*centre(w.ui()["layout"]["allow"]))                                                # a click is no answer before the dwell, as a key is none
            assert w.ui()["approval_bar"] and not w.ui()["card_ready"], w.ui()["header"]
            w.key("y")                                                                                 # typed at once: the person was writing, the card came up under the key
            state = w.ui()
            assert state["input"] == "y" and state["approval_bar"] and not state["card_ready"], state["input"]
            w.key("BackSpace")
            clock(w, "advance", "799")
            w.key("n")
            state = w.ui()
            assert state["input"] == "n" and state["approval_bar"] and not state["card_ready"], state["input"]
            w.key("BackSpace")
            clock(w, "advance", "1")
            assert wait_for(lambda: w.ui()["card_ready"] and w.ui()["input"] == "")
            w.key("y")                                                                                 # the box is empty and the card has been there for the dwell
            state = w.ui()
            assert not state["approval_bar"] and "Allowed: the text was sent." in state["header"], state["header"]
            wait_for(lambda: ">> the first" in records(log, docs))

            assert ask(w.directory, writer, docs, "the second")["status"] == "pending"                  # a panel over the card hides it: when it is shown again the count starts again
            wait_for(lambda: w.ui()["approval_bar"] and w.ui()["card_seen"])
            clock(w, "advance", "800")
            assert w.ui()["card_ready"]
            w.key("ctrl+a")
            assert panel(w) is not None
            wait_for(lambda: not w.ui()["card_seen"])
            w.key("Escape")
            wait_for(lambda: panel(w) is None and w.ui()["card_seen"])
            assert not w.ui()["card_ready"]
            clock(w, "advance", "799")
            assert not w.ui()["card_ready"]
            clock(w, "advance", "1")
            assert wait_for(lambda: w.ui()["card_ready"])
            w.key("n")
            assert "Declined: nothing was sent." in w.ui()["header"], w.ui()["header"]

            assert ask(w.directory, writer, docs, "the third")["status"] == "pending"                   # the buttons wake with the keys: a click is the answer once the dwell is over
            wait_for(lambda: w.ui()["approval_bar"] and w.ui()["card_seen"])
            clock(w, "advance", "799")
            assert wait_for(lambda: w.ui()["layout"]["dim"])
            dimmed = steady_fill(w, "allow")                                                            # the flag is not all: the buttons are drawn in the dimmed colour
            w.click(*centre(w.ui()["layout"]["deny"]))
            assert w.ui()["approval_bar"] and not w.ui()["card_ready"], w.ui()["header"]
            clock(w, "advance", "1")
            assert wait_for(lambda: w.ui()["card_ready"] and not w.ui()["layout"]["dim"])
            assert wait_for(lambda: fill_of(w, "allow") >= dimmed + 20), (dimmed, fill_of(w, "allow"))     # and in the awake one, clearly lighter, once the dwell is over
            w.click(*centre(w.ui()["layout"]["allow"]))
            assert wait_for(lambda: not w.ui()["approval_bar"] and "Allowed: the text was sent." in w.ui()["header"])
            wait_for(lambda: ">> the third" in records(log, docs))


def lit(w):
    """The lit pixels inside the Allow and Deny buttons: their labels, and whatever of the card's text runs under them."""
    layout, (data, width, _) = w.ui()["layout"], w.gray()
    return [sum(1 for y in range(int(r[1]), int(r[3])) for x in range(int(r[0]), int(r[2])) if data[y * width + x] > GRAY_THRESHOLD) for r in (layout["allow"], layout["deny"])]


def test_the_text_of_a_card_never_runs_under_the_allow_and_deny_buttons_whatever_its_length():
    with private_dir() as directory:
        args, _ = claude_args(directory)
        made = {}

        def sessions(w):
            conn = Conn(w.socket)
            made["writer"], made["generation"] = new_claude(conn, name="writer")
            made["docs"], _ = new_claude(conn, name="docs")

        with ProductWindow(host_binary=LICO, host_args=args, before_window=sessions) as w:
            writer, docs, host = made["writer"], made["docs"], Conn(w.socket)
            wait_for(lambda: len(w.ui()["rows"]) == 2)
            w.click(*row_centre(w, writer))
            assert w.ui()["selected"] == writer
            w.move(700, 400)

            def shown(call):
                """The card is on screen: the pixels of the buttons, the length of the bar. The host declines it (no key, so no wait for the dwell)."""
                assert call["status"] == "pending", call
                wait_for(lambda: w.ui()["approval_bar"])
                found, length = lit(w), len(w.ui()["approval_bar"])
                assert host.call(op="approval.answer", session=writer, generation=made["generation"], approval_id=call["approval_id"], answer="deny")["ok"] == 1
                wait_for(lambda: not w.ui()["approval_bar"])
                return found, length

            labels, _ = shown(ask(w.directory, writer, docs, "x"))                                    # a card of one row: the buttons hold their labels and nothing else
            assert all(labels) and labels[0] != labels[1], labels
            lengths = []
            for size in (20, 40, 60, 80, 100, 120):                                                    # the last row of the card ends at every place along the line, one row and two
                found, length = shown(ask(w.directory, writer, docs, "y" * size))
                lengths.append(length)
                assert found == labels, (size, length, found, labels)
            found, length = shown(ask_create(w.directory, writer, "write the release notes", "release-notes"))   # the longest card of the host: its own words alone fill a row and a half
            assert length > 180 and found == labels, (length, found, labels)
            assert max(lengths) - min(lengths) > 90, lengths
