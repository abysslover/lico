"""M25-P02: an agy session in the host, tried with the simulator of `agy acp` (`src/host/adapters/agy/simulator.py`), never the real program.

What the unit tests of `agy::` cannot see is here: the process (its folder, its arguments, the names of its environment), the threads, the session record, the states of an input,
the refused permission request reaching the program as an answer, the sign-in words, a program that dies or says nothing, and nothing left behind by a refused start.
The real program's turns are not verified (the record of what was observed is `src/host/adapters/agy/capabilities.json`): the owner's hands-on check is gate G3.
"""

import json
import os
import sys
from pathlib import Path

from lico_measure.p02 import HOSTD_DIR, ProtoConn as Conn, read_log, start
from lico_measure.rig import REPO, private_dir

from tests.test_codex_adapter import agent_pid, usual_umask, wait_for

SIM = REPO / "src" / "host" / "adapters" / "agy" / "simulator.py"
FIXTURE = REPO / "tests" / "fixtures" / "agy_acp" / "turn_with_permission.json"
SIGNED_OUT = "[error] agy is not signed in: open a Terminal tab, run agy, sign in, then send the message again"
NOT_ASKED = "> not run: this version of Lico does not pass the question to you yet"
KEPT = {"PATH", "LANG", "HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME", "XDG_CACHE_HOME", "LC_CTYPE"}   # LC_CTYPE is what the interpreter itself adds


def agy_host(directory, env_overrides=None, program=None):
    received = Path(directory) / "agy_received.txt"
    if program is None:
        extra = ["--agy", sys.executable, "--agy-arg", "-I", "--agy-arg", str(SIM), "--agy-arg", "--received", "--agy-arg", str(received),
                 "--agy-arg", "--fixture", "--agy-arg", str(FIXTURE)]
    else:
        extra = ["--agy", program]
    with usual_umask():
        host, log = start(directory, extra=extra, sessions=0, producing=0, history=0, retain=100000, env_overrides=env_overrides)
    return host, log, received


def lines_of(received):
    """What the program received, as lines: the first is its own log of how it was started, the others are what the host sent."""
    try:
        return [json.loads(l) for l in Path(received).read_text(encoding="utf-8").splitlines() if l.startswith("{")]
    except FileNotFoundError:
        return []


def started_as(received):
    return [m for m in lines_of(received) if "argv" in m]


def sent(received, method):
    return [m for m in lines_of(received) if m.get("method") == method]


def answers(received):
    """What the host answered to the requests of the program (a message with a result and no method)."""
    return [m for m in lines_of(received) if "method" not in m and "argv" not in m and "result" in m and m.get("id") not in (1, 2)]


def records(log, sid):
    return [p.decode() for _, p in read_log(log, sid)]


def status_of(conn, sid):
    return [s for s in conn.call(op="status")["sessions"] if s["session"] == sid][0]


def new_agy(conn, folder, **extra):
    made = conn.call(op="session.create", kind="agy", cwd=str(folder), **extra)
    assert made["ok"] == 1, made
    return made["session"], made["generation"]


def say(conn, sid, generation, request_id, text):
    reply = conn.call(op="input", session=sid, generation=generation, request_id=request_id, text=text)
    assert reply["ok"] == 1, reply
    return reply


def refused(reply):
    """A reply that says no, with a reason (the value of `ok` in such a reply is the protocol's own)."""
    return reply.get("ok") != 1 and bool(reply.get("error"))


def folder_in(directory):
    folder = Path(directory) / "work"
    folder.mkdir()
    return folder


def test_a_session_runs_in_its_folder_with_the_fixed_arguments_and_a_cleaned_environment_and_two_inputs_are_two_turns_in_order():
    with private_dir() as directory:
        folder = folder_in(directory)
        host, log, received = agy_host(directory, env_overrides={"LICO_CANARY_TOKEN": "canary-agy-5c41"})
        try:
            conn = Conn(host.socket)
            sid, generation = new_agy(conn, folder)
            first = wait_for(lambda: started_as(received))[0]
            assert first["argv"][-2:] == ["acp", "--disable-telemetry"] and "--dangerously-skip-permissions" not in first["argv"]
            assert os.path.realpath(first["cwd"]) == os.path.realpath(folder)
            assert set(first["env"]) <= KEPT, sorted(set(first["env"]) - KEPT)       # the names only: the host's own variables, the keyring's and the tokens never reach it
            assert "LICO_CANARY_TOKEN" not in first["env"]
            # two inputs, the second while the first may still be in flight: one prompt each, in order, nothing sent before the conversation was opened
            say(conn, sid, generation, "one", "alpha")
            say(conn, sid, generation, "two", "beta")
            wait_for(lambda: "echo: beta" in records(log, sid))
            shown = [r for r in records(log, sid) if r.startswith((">>", "echo"))]
            # the person's words are written when they are typed, the program's when it says them: the second input was typed while the first was answered
            assert [r for r in shown if r.startswith(">>")] == [">> alpha", ">> beta"] and [r for r in shown if r.startswith("echo")] == ["echo: alpha", "echo: beta"], shown
            assert shown.index("echo: alpha") < shown.index("echo: beta") and records(log, sid).count("[turn completed]") >= 1
            methods = [m["method"] for m in lines_of(received) if "method" in m]
            assert methods[:2] == ["initialize", "session/new"] and methods.count("initialize") == 1 and methods.count("session/new") == 1
            assert methods.count("session/prompt") == 2
            prompts = [m["params"]["prompt"][0]["text"] for m in sent(received, "session/prompt")]
            assert prompts == ["alpha", "beta"]
            # the session is the host's own record: a provider, the model the program named, idle once the turns ended, an input that went through
            entry = wait_for(lambda: (e := status_of(conn, sid)) and e.get("activity") == "idle" and e)
            assert entry["provider"] == "agy" and entry["state"] == "running" and entry["model"] == "gemini-4-neon-high"
            assert wait_for(lambda: conn.call(op="input.status", session=sid, request_id="two")["state"] in ("observed", "submitted"))
            # the same request id again is the same input: nothing more reaches the program
            again = say(conn, sid, generation, "one", "alpha")
            assert again["duplicate"] is True
            assert len(sent(received, "session/prompt")) == 2
            # the process works in the folder the person chose, and is the host's child in its own group
            pid = agent_pid(conn, sid)
            assert os.path.realpath(os.readlink(f"/proc/{pid}/cwd")) == os.path.realpath(folder)
            assert os.getpgid(pid) == pid
        finally:
            host.stop()


def test_a_permission_request_is_refused_to_the_program_and_never_held_and_the_words_say_so():
    with private_dir() as directory:
        folder = folder_in(directory)
        host, log, received = agy_host(directory)
        try:
            conn = Conn(host.socket)
            sid, generation = new_agy(conn, folder)
            say(conn, sid, generation, "fx", "fixture")
            wait_for(lambda: NOT_ASKED in records(log, sid))
            wait_for(lambda: any(r == "The command was not run." for r in records(log, sid)))
            assert wait_for(lambda: status_of(conn, sid).get("activity") == "idle")
            assert not status_of(conn, sid).get("approvals")                        # nothing waits for a person: this version does not ask
            given = answers(received)
            assert len(given) == 1 and given[0]["id"] == 100
            outcome = given[0]["result"]["outcome"]
            assert outcome["outcome"] == "selected" and outcome["optionId"] == "reject"      # the narrowest refusal of the options the program offered
            assert records(log, sid).index("[tool error]") < records(log, sid).index(NOT_ASKED)       # the failed call, then why
            # the program is not held: another turn works
            say(conn, sid, generation, "next", "after")
            wait_for(lambda: "echo: after" in records(log, sid))
        finally:
            host.stop()


def test_a_signed_out_program_says_so_in_words_and_the_session_stays_to_be_tried_again_and_a_stalled_turn_is_not_a_hang():
    with private_dir() as directory:
        folder = folder_in(directory)
        host, log, received = agy_host(directory)
        try:
            conn = Conn(host.socket)
            sid, generation = new_agy(conn, folder)
            say(conn, sid, generation, "out", "authfail")
            wait_for(lambda: SIGNED_OUT in records(log, sid))
            entry = wait_for(lambda: (e := status_of(conn, sid)) and e.get("activity") == "idle" and e)
            assert entry["state"] == "running"                                       # the words are the answer; the person signs in and sends again
            assert wait_for(lambda: conn.call(op="input.status", session=sid, request_id="out")["state"] == "failed")
            say(conn, sid, generation, "again", "hello")
            wait_for(lambda: "echo: hello" in records(log, sid))
            # a turn that says nothing for a while is working, not lost: the input is not failed, and the end of the turn comes when the program says so
            say(conn, sid, generation, "slow", "stall")
            wait_for(lambda: "stalling" in records(log, sid))
            assert status_of(conn, sid)["activity"] == "working"
            Path(f"{received}.end").touch()
            wait_for(lambda: "released" in records(log, sid))
            assert wait_for(lambda: status_of(conn, sid).get("activity") == "idle")
        finally:
            host.stop()


def test_a_program_that_dies_leaves_a_disconnected_session_that_resumes_the_same_conversation_and_one_that_cannot_stays_disconnected():
    with private_dir() as directory:
        folder = folder_in(directory)
        host, log, received = agy_host(directory)
        try:
            conn = Conn(host.socket)
            sid, generation = new_agy(conn, folder)
            say(conn, sid, generation, "a", "remember")
            wait_for(lambda: "echo: remember" in records(log, sid))
            first = sent(received, "session/prompt")[0]["params"]["sessionId"]
            say(conn, sid, generation, "boom", "crash")
            wait_for(lambda: status_of(conn, sid)["state"] == "disconnected")
            assert "[provider disconnected]" in records(log, sid)
            assert conn.call(op="input", session=sid, generation=generation, request_id="late", text="x")["error"] == "session_disconnected"
            assert [m for m in sent(received, "session/prompt") if m["params"]["prompt"][0]["text"] == "crash"] != []
            # resume: a new process, under a new generation, asked for the same conversation and never for a new one in its place
            resumed = conn.call(op="session.resume", session=sid, generation=generation)
            assert resumed["ok"] == 1 and resumed["generation"] == generation + 1
            wait_for(lambda: len(started_as(received)) == 2)
            say(conn, sid, resumed["generation"], "b", "back")
            wait_for(lambda: "echo: back" in records(log, sid))
            opened = [m for m in lines_of(received) if m.get("method") in ("session/new", "session/resume", "session/load")]
            assert [m["method"] for m in opened] == ["session/new", "session/resume"] and opened[1]["params"]["sessionId"] == first
            assert sent(received, "session/prompt")[-1]["params"]["sessionId"] == first
            # a program that cannot come back (it ends at once): the session is disconnected again and says why, with nothing new started in its place
            say(conn, sid, resumed["generation"], "c", "crash")
            wait_for(lambda: status_of(conn, sid)["state"] == "disconnected")
            Path(f"{received}.die").touch()
            again = conn.call(op="session.resume", session=sid, generation=resumed["generation"])
            assert again["ok"] == 1
            wait_for(lambda: status_of(conn, sid)["state"] == "disconnected" and len(started_as(received)) == 3)
            assert len([m for m in lines_of(received) if m.get("method") == "session/new"]) == 1
        finally:
            host.stop()


def test_closing_or_deleting_a_session_ends_its_process_even_when_the_program_never_answered_and_a_refused_start_leaves_nothing():
    with private_dir() as directory:
        folder = folder_in(directory)
        Path(f"{directory}/agy_received.txt.hang").touch()                    # the program starts and never answers `initialize`
        host, log, received = agy_host(directory)
        try:
            conn = Conn(host.socket)
            sid, generation = new_agy(conn, folder)
            reply = say(conn, sid, generation, "early", "hello")              # accepted and held: nothing is sent before the program has answered
            assert reply["state"] == "accepted"
            pid = agent_pid(conn, sid)
            wait_for(lambda: sent(received, "initialize"))
            assert not sent(received, "session/new") and not sent(received, "session/prompt")
            closed = conn.call(op="session.close", session=sid, generation=generation)
            assert closed["ok"] == 1
            wait_for(lambda: not Path(f"/proc/{pid}").exists())               # ended at once, not at the deadline of the answer it waited for
            assert not sent(received, "session/prompt")
            purged = conn.call(op="session.purge", session=sid, generation=generation)
            assert purged["ok"] == 1
            assert sid not in [s["session"] for s in conn.call(op="status")["sessions"]] and records(log, sid) == []
        finally:
            host.stop()
    with private_dir() as directory:
        folder = folder_in(directory)
        host, log, received = agy_host(directory, program=str(Path(directory) / "no-such-agy"))       # a program that is not there
        try:
            conn = Conn(host.socket)
            made = conn.call(op="session.create", kind="agy", cwd=str(folder))
            assert refused(made) and made["error"] == "busy"
            assert conn.call(op="status")["sessions"] == []                                  # no session, no log, no scratch folder left
            assert not list(Path(log).glob("*.seg")) if Path(log).exists() else True
            assert not (Path(directory) / "workspaces").exists() or list((Path(directory) / "workspaces").iterdir()) == []
        finally:
            host.stop()


def test_a_session_of_agy_needs_a_folder_that_is_there_when_one_is_named_and_takes_no_account_model_effort_or_worktree():
    with private_dir() as directory:
        folder = folder_in(directory)
        (Path(directory) / "a_file").write_text("x")
        host, log, received = agy_host(directory)
        try:
            conn = Conn(host.socket)
            assert refused(conn.call(op="session.create", kind="agy", cwd=str(Path(directory) / "missing")))
            assert refused(conn.call(op="session.create", kind="agy", cwd=str(Path(directory) / "a_file")))        # a file is not a folder
            assert refused(conn.call(op="session.create", kind="agy", cwd="relative/path"))
            for extra in ({"model": "opus"}, {"effort": "high"}, {"worktree_branch": "topic"}, {"resume_id": "abc"}):
                refused_reply = conn.call(op="session.create", kind="agy", cwd=str(folder), **extra)
                assert refused_reply.get("error") in ("option_unavailable", "bad_request"), (extra, refused_reply)
            assert refused(conn.call(op="session.create", kind="agy", cwd=str(folder), account="someone"))
            assert conn.call(op="status")["sessions"] == []                                                         # every refusal left nothing
            assert sent(received, "initialize") == []                                                              # and started nothing
            # with no folder named the program works in the scratch workspace every session without one is given, and it goes with the session
            made = conn.call(op="session.create", kind="agy")
            assert made["ok"] == 1, made
            workspace = os.readlink(f"/proc/{agent_pid(conn, made['session'])}/cwd")
            assert workspace.endswith("/workspaces/" + made["session"]) and os.path.isdir(workspace)
            assert conn.call(op="session.purge", session=made["session"], generation=made["generation"])["ok"] == 1
            wait_for(lambda: not os.path.exists(workspace))
        finally:
            host.stop()
