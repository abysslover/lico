"""M02-P03-H2 in the product window: Enter in the box of a session that is `disconnected` reconnects it and sends the text once, the way Ctrl+R reconnects it and sends nothing.

A real host with each simulator (Claude and Codex). The window asks the host for `session.resume` with the generation of the row, keeps the text, and sends it as ONE `input` with the
generation the host answered. What reached the provider is read from the durable log and from what the simulator received; what the person sees is read from the window's dump
(`hint`, the editor's `text` and `note`). The cases the window cannot reach with a simulator (every refusal code, a second Enter, another session opened meanwhile) are unit tests
in `gui/reconnect_tests.rs`."""
import sys
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir

from tests.test_claude_adapter import argv_of, new_claude, records, sent_users, status_of, user_text
from tests.test_codex_adapter import FIXTURE as CODEX_FIXTURE, SIM as CODEX_SIM, sent, wait_for
from tests.test_new_session_screen import claude_args
from tests.test_session_actions_window import row_centre

HINT = "Disconnected: Enter reconnects and sends, Ctrl+R reconnects"
ENDED_NOTE = "Not sent: the session could not be reconnected: the host could not take it just now. The text is back in the box."


def codex_args(directory):
    received = Path(directory) / "codex_received.jsonl"
    args = ["--codex", sys.executable, "--codex-arg", "-I", "--codex-arg", str(CODEX_SIM), "--codex-arg", "--received", "--codex-arg", str(received),
            "--codex-arg", "--fixture", "--codex-arg", str(CODEX_FIXTURE)]
    return args, received


def make_codex(conn):
    made = conn.call(op="session.create", kind="codex")
    assert made["ok"] == 1, made
    return made["session"], made["generation"]


def in_order(shown, *wanted):
    """Each of `wanted` is in `shown` exactly once, and they come in this order."""
    places = []
    for item in wanted:
        assert shown.count(item) == 1, (item, shown)
        places.append(shown.index(item))
    assert places == sorted(places), (wanted, shown)


def closed_after_a_turn(w, log, make, first):
    """A session that had one turn and was closed: its row says `disconnected`. The setup of every test here."""
    conn = Conn(w.socket)
    sid, generation = make(conn)
    assert conn.call(op="input", session=sid, generation=generation, request_id="first", text=first)["ok"] == 1
    wait_for(lambda: f"echo: {first}" in records(log, sid) and "[turn completed]" in records(log, sid))
    assert conn.call(op="session.close", session=sid, generation=generation)["ok"] == 1
    wait_for(lambda: status_of(conn, sid)["state"] == "disconnected")
    return sid, generation


def open_row(w, sid):
    wait_for(lambda: len(w.ui()["rows"]) == 1)
    w.click(*row_centre(w, sid))
    assert w.ui()["selected"] == sid


def enter_reconnects_and_sends_once(args, log, make, first, second):
    made = {}

    def before(w):
        made["sid"], made["generation"] = closed_after_a_turn(w, log, make, first)

    with ProductWindow(host_binary=LICO, host_args=[*args, "--log-dir", str(log)], before_window=before) as w:
        conn = Conn(w.socket)
        sid, old = made["sid"], made["generation"]
        open_row(w, sid)
        assert w.ui()["hint"].startswith(HINT), w.ui()["hint"]
        w.type_text(second)
        w.key("Return")
        wait_for(lambda: f"echo: {second}" in records(log, sid))
        row = status_of(conn, sid)
        assert row["state"] == "running" and row["generation"] > old, row
        state = w.ui()
        assert state["editor"]["text"] == "" and not state["hint"].startswith("Disconnected"), (state["editor"], state["hint"])
        assert state["editor"]["note"] is None, state["editor"]
        return sid, records(log, sid)


def test_enter_in_a_disconnected_claude_session_reconnects_it_and_sends_the_text_once_after_the_old_turn():
    with private_dir() as directory:
        args, received = claude_args(directory)
        log = Path(directory) / "log"
        _, shown = enter_reconnects_and_sends_once(args, log, new_claude, "first question", "second question")
        in_order(shown, ">> first question", "echo: first question", "[session closed]", "[provider reconnected]", ">> second question", "echo: second question")
        assert [user_text(m) for m in sent_users(received)] == ["first question", "second question"]          # the text reached the program once
        assert len(argv_of(received)) == 2, "one start and one resume, no second resume"


def test_enter_in_a_disconnected_codex_session_reconnects_it_and_sends_the_text_once_after_the_old_turn():
    with private_dir() as directory:
        args, received = codex_args(directory)
        log = Path(directory) / "log"
        _, shown = enter_reconnects_and_sends_once(args, log, make_codex, "first question", "second question")
        in_order(shown, ">> first question", "echo: first question", "[session closed]", "[provider reconnected]", ">> second question", "echo: second question")
        starts = [m for m in sent(received) if m.get("method") == "turn/start"]
        assert [m["params"]["input"][0]["text"] for m in starts] == ["first question", "second question"]
        assert len([m for m in sent(received) if m.get("method") == "thread/resume"]) == 1


def test_a_codex_session_whose_provider_cannot_start_again_gives_the_text_back_with_a_sentence_and_stays_disconnected():
    with private_dir() as directory:
        args, received = codex_args(directory)
        log = Path(directory) / "log"
        made = {}

        def before(w):
            made["sid"], made["generation"] = closed_after_a_turn(w, log, make_codex, "first question")
            Path(str(received) + ".die").write_text("")                                                     # the simulator exits as soon as it is started again

        with ProductWindow(host_binary=LICO, host_args=[*args, "--log-dir", str(log)], before_window=before) as w:
            conn = Conn(w.socket)
            sid, old = made["sid"], made["generation"]
            open_row(w, sid)
            w.type_text("second question")
            w.key("Return")
            state = wait_for(lambda: w.ui() if w.ui()["editor"]["text"] == "second question" and w.ui()["editor"]["note"] else None)
            assert state["editor"]["note"] == ENDED_NOTE, state["editor"]
            assert state["hint"].startswith(HINT), state["hint"]                                          # nothing changed: Enter tries again, Ctrl+R too
            row = status_of(conn, sid)
            assert row["state"] == "disconnected", row
            assert ">> second question" not in records(log, sid)
            assert [m for m in sent(received) if m.get("method") == "turn/start"][-1]["params"]["input"][0]["text"] == "first question"


def test_a_claude_session_whose_program_ends_at_once_is_resumed_once_and_not_a_second_time_by_itself():
    """What the host does with a program that dies right after the resume: Claude's start has no handshake, so the host takes the input before it knows. The text is
    echoed once, the session goes back to `disconnected`, and nothing resumes it again but the person."""
    with private_dir() as directory:
        args, received = claude_args(directory)
        log = Path(directory) / "log"
        made = {}

        def before(w):
            made["sid"], made["generation"] = closed_after_a_turn(w, log, new_claude, "first question")
            Path(str(received) + ".die").write_text("")

        with ProductWindow(host_binary=LICO, host_args=[*args, "--log-dir", str(log)], before_window=before) as w:
            conn = Conn(w.socket)
            sid = made["sid"]
            open_row(w, sid)
            w.type_text("second question")
            w.key("Return")
            wait_for(lambda: ">> second question" in records(log, sid) and status_of(conn, sid)["state"] == "disconnected")
            wait_for(lambda: "[provider disconnected]" in records(log, sid))
            shown = records(log, sid)
            assert shown.count(">> second question") == 1 and not [r for r in shown if r == "echo: second question"], shown
            assert len(argv_of(received)) == 2, "the window did not resume a second time"
            assert w.ui()["hint"].startswith(HINT)
