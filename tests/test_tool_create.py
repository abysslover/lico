"""S04c: the tool `create` on the tool connection, the person's approval of it, and the limits an agent meets (AC-04).

A host with the Claude simulator. A call of `create` makes nothing: it leaves a card on the row of the caller, and the session is made, once, when the card is
answered with the owner operation the window uses (`approval.answer`). The tests read what the new program was started with, what it was handed and what the host
logged, from the durable log, the file the simulator writes and the host's own log."""
import json
import re
from pathlib import Path

import pytest

from lico_measure.p02 import ProtoConn
from lico_measure.rig import private_dir
from lico_protocol import v1

from tests.test_claude_adapter import lines_of, records, status_of
from tests.test_codex_adapter import wait_for
from tests.test_tool_connection import World, rpc
from tests.test_transfer import check_lookalikes, lookalike_text
from tests.test_tool_send import CLEAN, DIRTY, HEADER, NOTICE, PLAIN_HEADER, decide, host_of, notices, received_with

STARTS = "starts at once on the account's allowance"
DEFAULT_MODEL = "model: the program's default"


@pytest.fixture(scope="module")
def world():
    with private_dir() as directory:
        host, log, received = host_of(directory)
        try:
            yield World(directory, host, log, received)
        finally:
            host.stop()


@pytest.fixture(autouse=True)
def tidy(world):
    """What the agents made in one test is not there for the next: the caps count every session that agents made and that is running."""
    yield
    for row in rows(world):
        if "made by" in row.get("name", ""):
            world.conn.call(op="session.purge", session=row["session"], generation=row["generation"])


def call(config, tool, **arguments):
    """A call of a tool (`send` has no argument called `tool`, and `create` has one called `name`, so the tool is not passed by name)."""
    status, body = rpc(config, "tools/call", {"name": tool, "arguments": arguments})
    assert status == 200, (status, body)
    result = body["result"]
    return result["isError"], result["content"][0]["text"]


def rows(world):
    return world.conn.call(op="status")["sessions"]


def running_made(world, by=None):
    """The sessions that agents made and that run. `by` is the name of the caller (the name of a new session ends with it)."""
    return [r for r in rows(world) if "made by" in r.get("name", "") and r["state"] == "running" and (by is None or r["name"].endswith(f"made by {by}"))]


def wants(world, caller, **arguments):
    """A call of `create` that has to wait for the person: its approval id."""
    failed, answer = call(world.config(caller), "create", **arguments)
    reply = json.loads(answer)
    assert not failed and reply["status"] == "pending" and sorted(reply) == ["approval_id", "message", "status"], answer
    return reply["approval_id"]


def made(world, caller, generation, **arguments):
    """A create and the person's yes: the id and the generation of the new session."""
    before = {r["session"] for r in rows(world)}
    number = wants(world, caller, **arguments)
    done = decide(world, caller, generation, number, "allow")
    assert done["ok"] == 1 and done["outcome"] == "done" and done["approval_id"] == number, done
    [new] = [r for r in rows(world) if r["session"] not in before]
    return new["session"], new["generation"]


def program_of(world, sid):
    """The argument list the program of `sid` was started with and the folder it started in (the simulator writes both)."""
    path = str(Path(world.directory) / "tools" / f"{sid}.mcp.json")
    lines = lines_of(world.received)
    for k, line in enumerate(lines):
        if line.startswith("ARGV ") and path in json.loads(line[5:]):
            return json.loads(line[5:]), (lines[k + 2][4:] if k + 2 < len(lines) and lines[k + 2].startswith("CWD ") else None)
    return None


def shown(folder):
    """A folder as the card shows it: cut from its start to 40 characters."""
    return folder if len(folder) <= 40 else "\u2026" + folder[-39:]


def err(world):
    return (Path(world.directory) / "host.err").read_text().splitlines()


def test_the_list_names_create_with_a_prompt_and_a_name_and_no_other_argument(world):
    sid, _ = world.session()
    status, body = rpc(world.config(sid), "tools/list", {})
    tools = {t["name"]: t for t in body["result"]["tools"]}
    assert status == 200 and list(tools) == ["sessions", "send", "create", "history", "read_handoff", "capture", "clipboard_read"]
    schema = tools["create"]["inputSchema"]
    assert schema["required"] == ["prompt"] and sorted(schema["properties"]) == ["name", "prompt"], schema      # no folder, mode, kind, account or `from`
    assert len(json.dumps(body["result"], separators=(",", ":"))) <= 4096


def test_a_create_waits_for_the_person_and_one_yes_makes_one_session_in_the_folder_of_the_caller_in_the_mode_that_asks_and_tells_the_caller(world):
    project = Path(world.directory) / "project"
    project.mkdir()
    caller, generation = world.session(name="writer", cwd=str(project))
    bystander, _ = world.session(name="bystander")
    folder = status_of(world.conn, caller)["cwd"]
    before = {r["session"] for r in rows(world)}
    prompt = "fix the tests"      # the card has 200 characters; what the head leaves is the start of the prompt (the whole of it is a keypress away in the window)
    number = wants(world, caller, prompt=prompt, name="reviewer", cwd="/etc", mode="auto", kind="codex", account="other", model="m", effort="high", **{"from": bystander})
    card = status_of(world.conn, caller)["approvals"]
    assert card == [{"approval_id": number, "tool": "create", "prompt": f"create session 'reviewer' in '{shown(folder)}' | mode: asks for approval | {DEFAULT_MODEL} | {STARTS} | {len(prompt)} B | \"{prompt}\""}], card
    assert {r["session"] for r in rows(world)} == before, "nothing is made until the card is answered"
    assert status_of(world.conn, bystander).get("approvals") is None, "what the caller said about itself was not heard"
    assert v1.validate_response("status", world.conn.call(op="status")) == []
    done = decide(world, caller, generation, number, "allow")
    assert done["ok"] == 1 and (done["state"], done["outcome"]) == ("answered", "done") and "reason" not in done, done
    [new] = [r for r in rows(world) if r["session"] not in before]
    assert new["name"] == "reviewer - made by writer" and (new["provider"], new["mode"], new["cwd"], new["state"]) == ("claude", "default", folder, "running"), new
    argv, started_in = wait_for(lambda: program_of(world, new["session"]))
    assert argv[argv.index("--permission-mode") + 1] == "default" and started_in == folder, (argv, started_in)      # the caller runs in `auto`; its mode is not handed on
    assert "codex" not in " ".join(argv) and "other" not in argv and "--model" not in argv, argv
    head = re.compile(HEADER.format(name="writer", id7=caller[:7], size=f"{len(prompt)} B"))
    wait_for(lambda: [r for r in records(world.log, new["session"]) if head.fullmatch(r)])
    assert f">> {prompt}" in records(world.log, new["session"]), "the first prompt came the way a send comes"
    told = wait_for(lambda: [r for r in records(world.log, caller) if r.startswith(">> approved and done: created session ")])
    match = re.fullmatch(rf">> approved and done: created session ({new['session']}) 'reviewer - made by writer' in '{re.escape(folder)}', mode: asks for approval; "
                         rf"the first prompt \({len(prompt)} B\) was delivered \(inline\)\. .*", told[0])
    assert match, told
    assert re.fullmatch(NOTICE.format(number=number), notices(world.log, caller)[0])
    assert decide(world, caller, generation, number, "allow")["error"] == "unknown_approval", "a decided card makes no second session"
    assert len(running_made(world)) == 1 and not notices(world.log, bystander)
    assert f"lico-host: call session={caller} tool=create result=asked" in err(world)
    assert f"lico-host: call session={caller} tool=create result=created" in err(world)
    assert f"lico-host: request session={caller} tool=create approval={number} state=asked size={len(prompt)}" in err(world)
    assert f"lico-host: request session={caller} tool=create approval={number} state=done size={len(prompt)}" in err(world)
    assert prompt not in " ".join(err(world)), "the host's log says who and how much, never the prompt"


def test_a_session_without_a_folder_makes_a_session_without_one_and_the_name_may_be_left_out(world):
    caller, generation = world.session(name="scratchy")
    number = wants(world, caller, prompt="hello")
    assert status_of(world.conn, caller)["approvals"][0]["prompt"] == f"create session in a scratch folder | mode: asks for approval | {DEFAULT_MODEL} | {STARTS} | 5 B | \"hello\""
    assert decide(world, caller, generation, number, "allow")["outcome"] == "done"
    [new] = running_made(world)
    assert new["name"] == "made by scratchy" and new.get("scratch") is True and "cwd" not in new, new


def test_the_card_says_the_model_and_the_effort_of_the_caller_and_the_program_of_the_new_session_is_started_with_them(world):
    caller, generation = world.session(name="chooser", model="opus", effort="high")
    number = wants(world, caller, prompt="hello")
    assert status_of(world.conn, caller)["approvals"][0]["prompt"] == f"create session in a scratch folder | mode: asks for approval | model: opus, effort: high | {STARTS} | 5 B | \"hello\""
    assert decide(world, caller, generation, number, "allow")["outcome"] == "done"
    [new] = running_made(world, by="chooser")
    argv, _ = wait_for(lambda: program_of(world, new["session"]))
    assert argv[argv.index("--model") + 1] == "opus" and argv[argv.index("--effort") + 1] == "high", argv
    other, other_generation = world.session(name="plain", effort="low")
    number = wants(world, other, prompt="hello")
    assert " | model: the program's default, effort: low | " in status_of(world.conn, other)["approvals"][0]["prompt"]
    assert decide(world, other, other_generation, number, "allow")["outcome"] == "done"
    [again] = running_made(world, by="plain")
    argv, _ = wait_for(lambda: program_of(world, again["session"]))
    assert "--model" not in argv and argv[argv.index("--effort") + 1] == "low", argv      # a choice left to the program stays left to it


def test_a_no_makes_nothing_and_a_yes_for_the_session_cannot_be_given_to_a_create(world):
    caller, generation = world.session(name="writer")
    number = wants(world, caller, prompt="do not make it")
    for answer in ("allow", "deny"):
        refused = decide(world, caller, generation, number, answer, keep=True)
        assert refused["error"] == "bad_request", (answer, refused)
    assert [a["approval_id"] for a in status_of(world.conn, caller)["approvals"]] == [number], "the card is still there"
    done = decide(world, caller, generation, number, "deny")
    assert done["ok"] == 1 and (done["outcome"], "reason" in done) == ("declined", False), done
    told = wait_for(lambda: [r for r in records(world.log, caller) if r.startswith(">> declined: ")])
    assert "creation of a new session" in told[0] and not running_made(world)
    assert "allowed" not in status_of(world.conn, caller), "and nothing is remembered"
    again = wants(world, caller, prompt="do not make it")
    assert again != number, "so it asks again"


def test_a_create_without_a_prompt_or_with_a_name_that_is_not_plain_is_refused_and_leaves_no_card(world):
    caller, _ = world.session(name="writer")
    for arguments in ({}, {"prompt": ""}, {"prompt": 5}, {"prompt": "p", "name": 5}, {"prompt": "p", "name": "a\nb"}, {"prompt": "p", "name": "x" * 37},
                      {"prompt": "p", "name": "rev\u202eiewer"}, {"prompt": "p", "name": "rev\u00adiewer"}, {"prompt": "p", "name": "rev\u3164iewer"}, {"prompt": "p", "name": "rev\u001b[31miewer"}, {"prompt": "\u200b"}, {"prompt": "\u001b[0m\U000e0041"}):
        failed, answer = call(world.config(caller), "create", **arguments)
        assert failed and answer.startswith("bad_request: create needs `prompt`"), (arguments, answer)
    assert status_of(world.conn, caller).get("approvals") is None and not running_made(world)


def test_the_characters_of_a_name_that_would_end_the_quote_of_the_card_become_underscores_in_the_card_and_in_the_name_of_the_session(world):
    caller, generation = world.session(name="writer")
    number = wants(world, caller, prompt="p", name="x' in '/etc | mode: auto | [(a)]")
    card = status_of(world.conn, caller)["approvals"][0]["prompt"]
    assert card.startswith("create session 'x_ in _/etc _ mode: auto _ __a__' in a scratch folder | mode: asks for approval | "), card
    assert decide(world, caller, generation, number, "allow")["outcome"] == "done"
    assert running_made(world)[0]["name"] == "x_ in _/etc _ mode: auto _ __a__ - made by writer"


def test_the_prompt_is_cleaned_once_so_the_card_the_pages_the_program_and_the_history_hold_the_same_string(world):
    caller, generation = world.session(name="writer")
    tag = f"c{caller}"
    clean = f"{tag} {CLEAN}"
    number = wants(world, caller, prompt=f"{tag} {DIRTY}")
    card = status_of(world.conn, caller)["approvals"][0]["prompt"]
    head, start = card.split(f"| {len(clean)} B, 2 lines | 12 hidden characters removed | \"")      # the head leaves the card room for the start of the line only
    assert f"{tag} redevil".startswith(start.removesuffix("\"").removesuffix("\u2026")) and not any(c in card for c in "\x1b\u202e\u200b\r\n"), card
    page = world.conn.call(op="approval.text", session=caller, generation=generation, approval_id=number, offset=0)
    assert (page["text"], page["total"]) == (clean, len(clean)), page
    assert decide(world, caller, generation, number, "allow")["outcome"] == "done"
    got = wait_for(lambda: received_with(world, tag))
    header, body = got[0].split("\n", 1)
    assert len(got) == 1 and re.fullmatch(PLAIN_HEADER.format(size=len(clean)), header) and body == clean, got
    [new] = running_made(world)
    wait_for(lambda: ">> second line" in records(world.log, new["session"]))
    assert not any(c in r for r in records(world.log, new["session"]) for c in "\x1b\u202e\u200b\r")


def test_header_look_alikes_in_the_first_prompt_of_a_new_session_are_defused_in_what_its_program_is_handed(world):
    caller, generation = world.session(name="writer")
    tag = f"d{caller}"
    made(world, caller, generation, prompt=lookalike_text(tag))
    head, body = wait_for(lambda: received_with(world, tag))[0].split("\n", 1)
    assert re.fullmatch(PLAIN_HEADER.format(size=len(lookalike_text(tag).encode())), head), head
    check_lookalikes(body, tag)


def test_a_long_prompt_is_stored_whole_in_the_folder_of_the_new_session_and_the_card_shows_the_size_and_the_start(world):
    caller, generation = world.session(name="writer")
    text = "0123456789abcdef" * 700                                                                               # 11200 bytes: a file, as a long send is
    number = wants(world, caller, prompt=text)
    assert len(status_of(world.conn, caller)["approvals"][0]["prompt"]) <= 200
    assert decide(world, caller, generation, number, "allow")["outcome"] == "done"
    [new] = running_made(world)
    wait_for(lambda: [r for r in records(world.log, new["session"]) if r.startswith(">> [from session ")])
    stored = [p for p in (Path(world.directory) / "handoff" / new["session"]).rglob("*") if p.is_file()]
    assert len(stored) == 1 and stored[0].read_text().endswith("\n\n" + text), "the whole prompt, nothing cut, in the folder of the new session only"
    assert not [p for p in (Path(world.directory) / "handoff" / caller).rglob("*") if p.is_file()], "and nothing in the folder of the caller"
    assert wait_for(lambda: any(r.startswith(">> approved and done: created session ") and "(file)" in r for r in records(world.log, caller)))


def test_four_sessions_made_by_one_caller_stop_the_fifth_at_the_call_and_at_the_yes_and_a_closed_or_deleted_one_gives_its_slot_back(world):
    caller, generation = world.session(name="boss")
    numbers = [wants(world, caller, prompt=f"job {k}") for k in range(5)]                                         # five cards at once: nothing is made yet, so no call sees a limit
    for number in numbers[:4]:
        assert decide(world, caller, generation, number, "allow")["outcome"] == "done"
    last = decide(world, caller, generation, numbers[4], "allow")
    assert last["ok"] == 1 and (last["outcome"], last["reason"]) == ("not_done", "send_limit"), last
    told = wait_for(lambda: [r for r in records(world.log, caller) if r.startswith(">> not done: send_limit: ")])
    assert "4 sessions that you made are running; the limit is 4 for one session and 8 in all" in told[0] and "close one" in told[0], told
    assert len(running_made(world, "boss")) == 4
    failed, answer = call(world.config(caller), "create", prompt="the sixth")
    assert failed and answer.startswith("send_limit: 4 sessions that you made are running"), answer
    assert status_of(world.conn, caller).get("approvals") is None, "a refused call leaves no card"
    first, second = running_made(world, "boss")[:2]
    wait_for(lambda: "[turn completed]" in records(world.log, first["session"]))                                   # a close needs the program's own session id, which comes with its first turn
    assert world.conn.call(op="session.close", session=first["session"], generation=first["generation"])["ok"] == 1
    assert len(running_made(world, "boss")) == 3
    assert made(world, caller, generation, prompt="after the close")
    assert call(world.config(caller), "create", prompt="and again")[0] is True
    assert world.conn.call(op="session.purge", session=second["session"], generation=second["generation"])["ok"] == 1
    assert made(world, caller, generation, prompt="after the delete")
    assert len(running_made(world, "boss")) == 4


def test_eight_sessions_that_agents_made_stop_a_ninth_for_whoever_asks_and_a_card_asked_before_is_stopped_at_its_yes(world):
    first, first_generation = world.session(name="alpha")
    second, second_generation = world.session(name="beta")
    third, third_generation = world.session(name="gamma")
    waiting = wants(world, third, prompt="asked early")
    for caller, generation in ((first, first_generation), (second, second_generation)):
        for k in range(4):
            made(world, caller, generation, prompt=f"job {k}")
    assert len(running_made(world)) == 8
    failed, answer = call(world.config(third), "create", prompt="the ninth")
    assert failed and answer.startswith("send_limit: 8 sessions that agents made are running; the limit is 8 in all and 4 for one session"), answer
    stopped = decide(world, third, third_generation, waiting, "allow")
    assert (stopped["outcome"], stopped["reason"]) == ("not_done", "send_limit"), stopped
    assert len(running_made(world)) == 8 and not running_made(world, "gamma")


def test_a_caller_that_ended_before_the_answer_creates_nothing(world):
    caller, generation = world.session(name="writer")
    number = wants(world, caller, prompt="too late")
    before = {r["session"] for r in rows(world)}
    assert world.conn.call(op="session.purge", session=caller, generation=generation)["ok"] == 1
    assert decide(world, caller, generation, number, "allow")["error"] == "unknown_session"
    assert {r["session"] for r in rows(world)} == before - {caller} and not running_made(world)


def test_a_closed_caller_cannot_be_answered_until_it_is_back_and_then_one_yes_makes_one_session(world):
    caller, generation = world.session(name="closing")
    world.first_turn(caller, generation)
    number = wants(world, caller, prompt="asked before the close")
    assert world.conn.call(op="session.close", session=caller, generation=generation)["ok"] == 1
    assert decide(world, caller, generation, number, "allow")["error"] == "session_disconnected" and not running_made(world)
    resumed = world.conn.call(op="session.resume", session=caller, generation=generation)
    assert resumed["ok"] == 1
    assert decide(world, caller, resumed.get("generation", generation), number, "allow")["outcome"] == "done"
    assert len(running_made(world)) == 1


def test_a_create_is_a_step_on_the_chain_the_new_session_makes_the_next_and_the_fifth_is_refused_without_a_card(world):
    root, root_generation = world.session(name="root")
    chain = [(root, root_generation)]
    for step in range(4):
        caller, generation = chain[-1]
        chain.append(made(world, caller, generation, prompt=f"step {step + 1}"))
    last, last_generation = chain[-1]
    failed, answer = call(world.config(last), "create", prompt="the fifth step")
    assert failed and answer.startswith("chain_depth: this would be step 5 in a row") and "limit is 4" in answer and "person" in answer, answer
    assert status_of(world.conn, last).get("approvals") is None
    failed, answer = call(world.config(last), "send", to=root, text="a send is the fifth step too")
    assert failed and answer.startswith("chain_depth: "), answer
    assert world.conn.call(op="input", session=last, generation=last_generation, request_id="p-1", text="carry on")["ok"] == 1        # a person writes to the last one
    assert made(world, last, last_generation, prompt="after the person")


def test_the_program_in_the_session_calls_create_with_the_credential_it_read_at_its_start_and_is_told_the_decision_as_its_next_input(world):
    caller, generation = world.session(name="writer")
    assert world.conn.call(op="input", session=caller, generation=generation, request_id="t-1", text='tool:create {"prompt": "from the program", "name": "helper"}')["ok"] == 1
    wait_for(lambda: "[turn completed]" in records(world.log, caller))
    said = next(r for r in records(world.log, caller) if r.startswith("tool "))
    assert said.startswith("tool 200 ") and '"isError":false' in said and "pending" in said and "Nothing was made yet" in said, said
    number = status_of(world.conn, caller)["approvals"][0]["approval_id"]
    assert decide(world, caller, generation, number, "allow")["outcome"] == "done"
    [new] = running_made(world)
    assert new["name"] == "helper - made by writer"
    wait_for(lambda: [r for r in records(world.log, new["session"]) if re.fullmatch(HEADER.format(name="writer", id7=caller[:7], size="16 B"), r)])
    wait_for(lambda: records(world.log, caller).count("[turn completed]") == 2)


def test_the_new_session_is_an_ordinary_row_the_person_can_rename_close_resume_and_delete(world):
    caller, generation = world.session(name="writer")
    sid, new_generation = made(world, caller, generation, prompt="work")
    wait_for(lambda: "[turn completed]" in records(world.log, sid))
    assert world.conn.call(op="session.rename", session=sid, name="mine now")["ok"] == 1
    assert status_of(world.conn, sid)["name"] == "mine now"
    assert world.conn.call(op="session.close", session=sid, generation=new_generation)["ok"] == 1
    resumed = world.conn.call(op="session.resume", session=sid, generation=new_generation)
    assert resumed["ok"] == 1 and status_of(world.conn, sid)["state"] == "running"
    assert world.conn.call(op="session.purge", session=sid, generation=resumed.get("generation", new_generation))["ok"] == 1


def test_the_flag_that_once_let_a_test_send_without_the_question_does_not_let_a_create_through():
    with private_dir() as directory:
        host, log, received = host_of(directory, "--tool-send-without-approval")
        try:
            world = World(directory, host, log, received)
            caller, generation = world.session(name="writer")
            before = {r["session"] for r in rows(world)}
            number = wants(world, caller, prompt="hello")
            assert {r["session"] for r in rows(world)} == before, "an unknown argument changes nothing: the question is asked"
            assert decide(world, caller, generation, number, "allow")["outcome"] == "done"
        finally:
            host.stop()
