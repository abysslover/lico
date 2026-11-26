"""M25-P06: an `agy` session is a peer like a Claude or a Codex one: Send to, a Call Room, an `@name` paragraph of its answer, tried with the simulator of `agy acp` (never the real program).

`agy` has no tools of Lico's (D4: its tool servers are empty), so what it receives is a prompt and what it says is the reply: a room's envelope tells it so, the hint after an inline `@name` of the person
tells it to begin a paragraph with the name, and `dispatch_assistant_mentions` sends that paragraph. What the real program does with these prompts is not verified (the owner's hands-on check, gate G3).

M25-P09 (the owner's words of 2026-11-26): handing over an `agy` conversation tells the receiver the conversation's number, not its text.
"""

from lico_measure.p02 import ProtoConn as Conn
from lico_measure.rig import private_dir

from tests.test_agy_session import agy_host, folder_in, new_agy, records, say, sent
from tests.test_codex_adapter import wait_for
from tests.test_recovery import registered


def named(conn, folder, name):
    """A new `agy` session with a name the others can call it by."""
    sid, generation = new_agy(conn, folder)
    assert conn.call(op="session.rename", session=sid, name=name)["ok"] == 1
    return sid, generation


def said(log, sid):
    return "\n".join(records(log, sid))


def prompts(received):
    """The texts the program was given, in order."""
    return [m["params"]["prompt"][0]["text"] for m in sent(received, "session/prompt")]


def test_a_text_sent_to_an_agy_session_is_its_prompt_and_a_room_between_two_agy_sessions_brings_the_reply_back_to_the_asker():
    with private_dir() as directory:
        folder = folder_in(directory)
        host, log, received = agy_host(directory)
        try:
            conn = Conn(host.socket)
            ann, _ = named(conn, folder, "Ann")
            bee, _ = named(conn, folder, "Bee")
            # Send to: a person's text to a running agy session is one prompt of that session, said as sent
            assert conn.call(op="transfer.send", to=bee, kind="text", text="a note for bee")["ok"] == 1
            wait_for(lambda: "a note for bee" in said(log, bee) and any("a note for bee" in p for p in prompts(received)))
            # a room: Ann asks Bee; Bee's answer to that turn is the reply, recorded in the room and delivered to Ann
            started = conn.call(**{"op": "call.start", "from": ann, "to": bee, "text": "hello bee"})
            assert started["ok"] == 1, started
            wait_for(lambda: "hello bee" in said(log, bee) and "OWNER-AUTHORIZED" in said(log, bee))
            call = started["call"]["call_id"]
            wait_for(lambda: any(e["kind"] == "peer_reply" and e["provider"] == "agy" for e in conn.call(op="call.get", call=call)["call"]["entries"]))
            room = conn.call(op="call.get", call=call)["call"]
            assert (room["provider_a"], room["provider_b"]) == ("agy", "agy"), "the room knows them as agy, not as a synthetic session"
            wait_for(lambda: "from 'Bee'" in said(log, ann))       # the reply came back to the asker's own session
        finally:
            host.stop()


def test_an_inline_mention_tells_an_agy_session_to_begin_a_paragraph_with_the_name_and_such_a_paragraph_of_its_answer_is_sent():
    with private_dir() as directory:
        folder = folder_in(directory)
        host, log, received = agy_host(directory)
        try:
            conn = Conn(host.socket)
            ann, generation = named(conn, folder, "Ann")
            bee, _ = named(conn, folder, "Bee")
            say(conn, ann, generation, "r1", "please relay:\n@Bee wave hello")        # the simulator echoes the prompt: its answer has a paragraph that begins with @Bee
            wait_for(lambda: any("wave hello" in p for p in prompts(received)[1:]) or "wave hello" in said(log, bee))
            given = [p for p in prompts(received) if "please relay" in p]
            assert given and "mcp__lico__send" not in given[0], "agy has no tool to call: " + given[0]
            assert "begins with @Bee at the start of the line" in given[0], given[0]
            wait_for(lambda: "wave hello" in said(log, bee))        # the paragraph reached Bee
            wait_for(lambda: "from 'Bee'" in said(log, ann))        # and Bee's answer to that turn came back to Ann: the send began a call
        finally:
            host.stop()


def test_handing_over_an_agy_conversation_gives_the_receiver_its_number_and_never_its_text():
    with private_dir() as directory:
        folder = folder_in(directory)
        host, log, received = agy_host(directory)
        try:
            conn = Conn(host.socket)
            ann, generation = named(conn, folder, "Ann")
            bee, _ = named(conn, folder, "Bee")
            say(conn, ann, generation, "r1", "a sentence of the conversation")
            wait_for(lambda: "echo: a sentence of the conversation" in said(log, ann))
            number = wait_for(lambda: registered(directory).get(ann, {}).get("provider_session"))
            wait_for(lambda: "[session started]" in said(log, bee))                    # the receiver is up (a start under load is slow)
            handed = conn.call(op="transfer.send", to=bee, kind="conversation", **{"from": ann})
            assert handed["ok"] == 1 and handed["delivery"] == "inline" and handed["kind"] == "conversation", handed
            wait_for(lambda: any(number in p for p in prompts(received)))
            [note] = [p for p in prompts(received) if number in p]
            assert note.startswith("[from session 'Ann' (") and " | conversation | " in note, note
            assert "only the number" in note and "a sentence of the conversation" not in note, note
            assert "a sentence of the conversation" not in said(log, bee), "nothing of the conversation reached Bee's transcript either"
            # a session that has not been spoken to has its number too (the program's answer to session/new): it can be handed over the other way
            assert conn.call(op="transfer.send", to=ann, kind="conversation", **{"from": bee})["ok"] == 1
        finally:
            host.stop()
