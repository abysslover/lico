"""M25-P04: an agy conversation comes back as a session, tried with the simulator of `agy acp` (never the real program).

A host that was killed leaves the session disconnected with the conversation's id, and a resume asks the program for that conversation and never for a new one; a conversation
that the program (or the Chats list) names by its id opens a session in its folder; one session holds a conversation at a time; an id the program does not know ends in plain words
and never in a new conversation put in its place. What the real program does with these requests is not verified (the owner's hands-on check, gate G3).
"""

import json
from pathlib import Path

from lico_measure.p02 import ProtoConn as Conn
from lico_measure.rig import private_dir

from tests.test_agy_session import agy_host, folder_in, lines_of, new_agy, records, refused, say, sent, started_as, status_of
from tests.test_codex_adapter import wait_for
from tests.test_recovery import hard_stop, registered

CONVERSATION = "3f2a9c1e-7b4d-4e55-9a60-1d2c3b4a5f66"
OTHER = "8c1d0e47-2a9b-4f63-b5d8-0e7f6a5b4c33"
EARLIER = [{"role": "user", "text": "earlier question"}, {"role": "agent", "text": "earlier answer"}]


def opened(received):
    """What the host asked of the program to open a conversation, in order."""
    return [(m["method"], m["params"].get("sessionId")) for m in lines_of(received) if m.get("method") in ("session/new", "session/resume", "session/load")]


def test_a_killed_host_leaves_the_agy_session_disconnected_with_its_history_and_a_resume_asks_for_the_same_conversation():
    with private_dir() as directory:
        folder = folder_in(directory)
        host, log, received = agy_host(directory)
        try:
            conn = Conn(host.socket)
            sid, generation = new_agy(conn, folder, mode="default")
            say(conn, sid, generation, "a", "remember")
            wait_for(lambda: "echo: remember" in records(log, sid))
            wait_for(lambda: registered(directory).get(sid, {}).get("provider_session"))
            first = registered(directory)[sid]["provider_session"]
            remembered = conn.call(op="session.options")["last"]
            assert remembered["kind"] == "agy" and remembered["mode"] == "default" and remembered["cwd"] == str(folder), remembered      # what the form offers again is how it was started
            assert opened(received) == [("session/new", None)] and sent(received, "session/prompt")[0]["params"]["sessionId"] == first
        finally:
            hard_stop(host)
        host, log, received = agy_host(directory)
        try:
            conn = Conn(host.socket)
            entry = status_of(conn, sid)
            assert entry["state"] == "disconnected" and entry["generation"] == generation and entry["mode"] == "default"
            assert ">> remember" in records(log, sid) and "echo: remember" in records(log, sid), "the history is there"
            assert len(started_as(received)) == 1, "the restart itself starts no program"
            resumed = conn.call(op="session.resume", session=sid, generation=generation)
            assert resumed["ok"] == 1 and resumed["generation"] == generation + 1, resumed
            wait_for(lambda: "[provider reconnected]" in records(log, sid))
            say(conn, sid, resumed["generation"], "b", "back")
            wait_for(lambda: "echo: back" in records(log, sid))
            assert opened(received) == [("session/new", None), ("session/resume", first)], "the same conversation, never a new one in its place"
            assert sent(received, "session/prompt")[-1]["params"]["sessionId"] == first
            assert records(log, sid).count("echo: remember") == 1, "what a resume says is not copied twice"
        finally:
            host.stop()


def test_a_conversation_named_by_its_id_opens_a_session_in_its_folder_and_one_session_holds_it_at_a_time():
    with private_dir() as directory:
        folder = folder_in(directory)
        Path(directory, "agy_received.txt.sessions").write_text(json.dumps({CONVERSATION: EARLIER, OTHER: EARLIER}), encoding="utf-8")       # the program knows this conversation from an earlier run
        host, log, received = agy_host(directory)
        try:
            conn = Conn(host.socket)
            made = conn.call(op="session.create", kind="agy", cwd=str(folder), resume_id=CONVERSATION)
            assert made["ok"] == 1 and "already_open" not in made, made
            sid, generation = made["session"], made["generation"]
            wait_for(lambda: opened(received) == [("session/resume", CONVERSATION)])
            say(conn, sid, generation, "a", "again")
            wait_for(lambda: "echo: again" in records(log, sid))
            assert sent(received, "session/prompt")[0]["params"]["sessionId"] == CONVERSATION
            assert started_as(received)[0]["cwd"] == str(folder)
            assert "earlier answer" not in " ".join(records(log, sid)), "what the program already said is the program's, not copied into this session"
            # the conversation is held by this session: asking again shows it, and nothing is started
            again = conn.call(op="session.create", kind="agy", cwd=str(folder), resume_id=CONVERSATION)
            assert again["ok"] == 1 and again["session"] == sid and again["already_open"] is True and again["resumed"] is True
            assert len(started_as(received)) == 1
            # another conversation is another session, and the first still holds its own
            other = conn.call(op="session.create", kind="agy", cwd=str(folder), resume_id=OTHER)
            assert other["ok"] == 1 and other["session"] != sid and "already_open" not in other, other
            wait_for(lambda: opened(received) == [("session/resume", CONVERSATION), ("session/resume", OTHER)])
            # a disconnected holder is resumed in place, under a new generation, and no second session appears
            say(conn, sid, generation, "boom", "crash")
            wait_for(lambda: status_of(conn, sid)["state"] == "disconnected")
            back = conn.call(op="session.create", kind="agy", cwd=str(folder), resume_id=CONVERSATION)
            assert back["ok"] == 1 and back["session"] == sid and back["generation"] == generation + 1 and back["already_open"] is True
            wait_for(lambda: len(started_as(received)) == 3)
            assert sorted(s["session"] for s in conn.call(op="status")["sessions"]) == sorted([sid, other["session"]]), "no third session appeared"
            assert opened(received) == [("session/resume", CONVERSATION), ("session/resume", OTHER), ("session/resume", CONVERSATION)]
        finally:
            host.stop()


def test_an_id_that_cannot_be_right_is_refused_and_one_the_program_does_not_know_ends_in_words_and_never_in_a_new_conversation():
    with private_dir() as directory:
        folder = folder_in(directory)
        host, log, received = agy_host(directory)
        try:
            conn = Conn(host.socket)
            for bad in ("../x", "a b", "x" * 100, "", "-" + CONVERSATION):       # the last one passes the request's own check and is the provider id check's to refuse
                assert refused(conn.call(op="session.create", kind="agy", cwd=str(folder), resume_id=bad)), bad
            assert started_as(received) == [] and conn.call(op="status")["sessions"] == []
            local = dict(kind="local", endpoint="127.0.0.1:9", api="ollama", model="m")      # a complete request of a kind with no conversation of its own, except for the id
            assert conn.call(op="session.create", resume_id=CONVERSATION, **local).get("error") == "option_unavailable"
            made = conn.call(op="session.create", kind="agy", cwd=str(folder), resume_id=CONVERSATION)       # the program has never heard of it
            assert made["ok"] == 1
            sid = made["session"]
            wait_for(lambda: "[error] agy could not bring this conversation back; start a new session" in records(log, sid))
            wait_for(lambda: status_of(conn, sid)["state"] == "disconnected")
            assert [m for m, _ in opened(received)] == ["session/resume", "session/load"], "resumed, then loaded, and never a new conversation"
            assert sent(received, "session/prompt") == []
        finally:
            host.stop()
