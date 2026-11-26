"""M25-P02 and P03: an agy session in the host, tried with the simulator of `agy acp` (`src/host/adapters/agy/simulator.py`), never the real program.

What the unit tests of `agy::` cannot see is here: the process (its folder, its arguments, the names of its environment), the threads, the session record, the states of an input,
the permission request as an approval of the session (asked in the mode `default`, answered by the host in `auto`) and the answer reaching the program as one option of one request, the sign-in words, a program that dies or says nothing, and nothing left behind by a refused start.
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
KEPT = {"PATH", "LANG", "HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME", "XDG_CACHE_HOME", "LC_CTYPE"}   # LC_CTYPE is what the interpreter itself adds


def simulator_args(received):
    """The arguments that make a host (the test host or the window's own) run the simulator as its `agy`; what it receives is written to `received`."""
    return ["--agy", sys.executable, "--agy-arg", "-I", "--agy-arg", str(SIM), "--agy-arg", "--received", "--agy-arg", str(received), "--agy-arg", "--fixture", "--agy-arg", str(FIXTURE)]


def agy_host(directory, env_overrides=None, program=None):
    received = Path(directory) / "agy_received.txt"
    if program is None:
        extra = simulator_args(received)
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


def approvals_of(conn, sid):
    return status_of(conn, sid).get("approvals", [])


def answer(conn, sid, generation, number, what):
    return conn.call(op="approval.answer", session=sid, generation=generation, approval_id=number, answer=what)


def events_seen(watcher):
    seen = []
    watcher.sock.settimeout(0.5)
    try:
        while True:
            seen.append(watcher.read())
    except Exception:
        pass
    return seen


def test_in_the_mode_default_a_permission_request_is_an_approval_and_the_answer_is_one_option_of_that_one_request():
    with private_dir() as directory:
        folder = folder_in(directory)
        host, log, received = agy_host(directory)
        try:
            conn = Conn(host.socket)
            sid, generation = new_agy(conn, folder, mode="default")
            watcher = Conn(host.socket)
            watcher.call(op="subscribe", session=sid, after=-1, snapshot=True)
            say(conn, sid, generation, "fx", "fixture")
            first = wait_for(lambda: approvals_of(conn, sid))
            assert first[0]["prompt"] == "run: ls -la"
            number = first[0]["approval_id"]
            assert answers(received) == [], "nothing is answered before the person does"
            # forged, stale and repeated answers send nothing to the program
            assert answer(conn, sid, generation, number + 50, "allow")["error"] == "unknown_approval"
            assert answer(conn, sid, generation + 1, number, "allow")["error"] == "stale_generation"
            assert answers(received) == []
            done = answer(conn, sid, generation, number, "allow")
            assert done["ok"] == 1 and done["state"] == "answered"
            assert answer(conn, sid, generation, number, "allow")["error"] == "unknown_approval"
            wait_for(lambda: "[turn completed]" in records(log, sid))
            assert "Done: a.txt and b.txt." in records(log, sid)
            given = answers(received)
            assert len(given) == 1 and given[0]["id"] == 100
            assert given[0]["result"]["outcome"] == {"outcome": "selected", "optionId": "allow"}      # the one-shot yes of the options the program offered, never an `always`
            assert wait_for(lambda: status_of(conn, sid).get("activity") == "idle") and not approvals_of(conn, sid)
            # a second turn: the number is new, and a no is the one-shot no
            say(conn, sid, generation, "fx2", "fixture")
            second = wait_for(lambda: approvals_of(conn, sid))
            assert second[0]["approval_id"] > number
            assert answer(conn, sid, generation, second[0]["approval_id"], "deny")["ok"] == 1
            wait_for(lambda: "The command was not run." in records(log, sid))
            assert records(log, sid).count("The command was not run.") == 1       # (the program's tool call id repeats in this fixture, so no second failure line is drawn)
            assert answers(received)[-1]["result"]["outcome"] == {"outcome": "selected", "optionId": "reject"}
            assert wait_for(lambda: status_of(conn, sid).get("activity") == "idle")
            assert len([e for e in events_seen(watcher) if e.get("ev") == "approval"]) == 2, "the viewers were told of both"
        finally:
            host.stop()


def test_in_the_mode_auto_the_host_answers_the_one_shot_yes_and_says_so_and_a_switch_to_default_makes_it_ask_again():
    with private_dir() as directory:
        folder = folder_in(directory)
        host, log, received = agy_host(directory)
        try:
            conn = Conn(host.socket)
            sid, generation = new_agy(conn, folder, mode="auto")
            say(conn, sid, generation, "fx", "fixture")
            wait_for(lambda: "[turn completed]" in records(log, sid))
            lines = records(log, sid)
            assert lines.index("[auto-allowed] run: ls -la") < lines.index("Done: a.txt and b.txt.")      # said before the program went on
            given = answers(received)
            assert len(given) == 1 and given[0]["result"]["outcome"] == {"outcome": "selected", "optionId": "allow"}
            assert not approvals_of(conn, sid), "nobody was asked"
            # the person's switch: the next request is asked, and the switch is refused while one is waiting
            assert wait_for(lambda: status_of(conn, sid).get("activity") == "idle")
            switched = conn.call(op="session.mode", session=sid, generation=generation, mode="default")
            assert switched["ok"] == 1 and switched["mode"] == "default" and switched["generation"] == generation      # the program is not restarted
            say(conn, sid, generation, "fx2", "fixture")
            asked = wait_for(lambda: approvals_of(conn, sid))
            assert conn.call(op="session.mode", session=sid, generation=generation, mode="auto")["error"] == "busy"
            assert answer(conn, sid, generation, asked[0]["approval_id"], "allow")["ok"] == 1
            wait_for(lambda: records(log, sid).count("[turn completed]") == 2)
            assert records(log, sid).count("[auto-allowed] run: ls -la") == 1
        finally:
            host.stop()


def test_a_repeated_or_unclean_request_is_one_approval_at_most_and_a_request_the_turn_left_unanswered_goes_with_the_turn_and_so_does_the_program():
    with private_dir() as directory:
        folder = folder_in(directory)
        host, log, received = agy_host(directory)
        try:
            conn = Conn(host.socket)
            sid, generation = new_agy(conn, folder, mode="default")
            watcher = Conn(host.socket)
            watcher.call(op="subscribe", session=sid, after=-1, snapshot=True)
            say(conn, sid, generation, "bad", "hostile")           # 20 requests on one id, one with a boolean id, a read of a file, 300 tool calls; the turn ends at once
            wait_for(lambda: "[turn completed]" in records(log, sid))
            assert wait_for(lambda: status_of(conn, sid).get("activity") == "idle")
            assert len([e for e in events_seen(watcher) if e.get("ev") == "approval"]) == 1
            assert not approvals_of(conn, sid), "the turn ended with the request unanswered: it is over"
            assert answer(conn, sid, generation, 1, "allow")["error"] == "unknown_approval"
            replies = [m for m in lines_of(received) if "method" not in m and "argv" not in m and m.get("id") not in (1, 2, 3)]
            assert [m["id"] for m in replies] == [7] and replies[0]["error"]["code"] == -32601, "only the read of a file was answered, and refused"
            # a request that waits for the person when the program ends is gone with it, and its answer is refused
            say(conn, sid, generation, "fx", "fixture")
            waiting = wait_for(lambda: approvals_of(conn, sid))
            os.kill(agent_pid(conn, sid), 9)
            wait_for(lambda: status_of(conn, sid)["state"] == "disconnected")
            assert not approvals_of(conn, sid)
            assert refused(answer(conn, sid, generation, waiting[0]["approval_id"], "allow"))
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
            for extra in ({"model": "opus"}, {"effort": "high"}, {"worktree_branch": "topic"}, {"resume_id": "not a valid id"}):       # (a valid conversation id is taken since P04: tests/test_agy_resume.py)
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
