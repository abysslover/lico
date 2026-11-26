"""M25-P01: the record, the reference mapping, the fixtures and the simulator of `agy acp`. The adapter of the host (P02 on) is tested against the same files."""

import json
import os
import queue
import shutil
import subprocess
import sys
import threading
import time

import pytest

from lico_measure.rig import REPO
from lico_protocol import agy_mapping as ref
from lico_protocol.codex_mapping import MAX_RECORD

ADAPTER = REPO / "src" / "host" / "adapters" / "agy"
SIM = ADAPTER / "simulator.py"
FIXTURES = REPO / "tests" / "fixtures" / "agy_acp"
NAMES = ["turn_with_permission", "turn_signed_out", "resume_with_replay"]
PINNED = "1.5.18"   # the version the record describes; a new installed version fails the contract test until the record is made again


def fixture(name):
    return json.loads((FIXTURES / (name + ".json")).read_text(encoding="utf-8"))


class Acp:
    """A client of one ACP server process: what it sent and received is kept as a sequence the reference reads."""

    def __init__(self, argv, env=None, cwd=None):
        self.proc = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, env=env, cwd=cwd, text=True, encoding="utf-8")
        self.stream = []
        self.lines = queue.Queue()
        threading.Thread(target=self._read, daemon=True).start()
        self.next_id = 0

    def _read(self):
        for line in self.proc.stdout:
            self.lines.put(line)
        self.lines.put(None)

    def send(self, message):
        message = {"jsonrpc": "2.0", **message}
        self.stream.append({"dir": "host", "msg": message})
        self.proc.stdin.write(json.dumps(message) + "\n")
        self.proc.stdin.flush()

    def call(self, method, params, on_request=None, wait=20):
        """Sends a request and returns its response; messages that come before it are kept in `stream`; a request of the agent is given to `on_request`, which returns the host's response."""
        self.next_id += 1
        ident = self.next_id
        self.send({"id": ident, "method": method, "params": params})
        end = time.time() + wait
        while time.time() < end:
            try:
                line = self.lines.get(timeout=0.2)
            except queue.Empty:
                continue
            if line is None:
                raise AssertionError("the program ended")
            message = json.loads(line)
            self.stream.append({"dir": "agent", "msg": message})
            if message.get("id") == ident and "method" not in message:
                return message
            if "method" in message and message.get("id") is not None and on_request:
                reply = on_request(message)
                if reply is not None:
                    self.send(reply)
        raise AssertionError("no answer to " + method)

    def settle(self, wait=0.5):
        """Takes what else arrives (notifications that follow an answer)."""
        end = time.time() + wait
        while time.time() < end:
            try:
                line = self.lines.get(timeout=0.1)
            except queue.Empty:
                continue
            if line is None:
                return
            self.stream.append({"dir": "agent", "msg": json.loads(line)})

    def close(self):
        try:
            self.proc.stdin.close()
            self.proc.wait(5)
        except Exception:
            self.proc.kill()


INIT = {"protocolVersion": 1, "clientCapabilities": {"fs": {"readTextFile": False, "writeTextFile": False}, "terminal": False}, "clientInfo": {"name": "lico", "version": "0"}}


def test_the_record_is_pinned_and_describes_what_the_installed_agy_answers_without_a_sign_in(tmp_path):
    """A1: initialize, the keys of session/new, the modes and the error codes are read from the installed program with a private empty home and a minimal environment."""
    cap = json.loads((ADAPTER / "capabilities.json").read_text(encoding="utf-8"))
    assert cap["version"]["pinned"] and cap["version"]["cli"] == PINNED and cap["unverified"] and all(u.get("reason") for u in cap["unsupported"])
    assert "--dangerously-skip-permissions" in cap["launch"]["never"] and "--dangerously-skip-permissions" not in cap["launch"]["argv"]
    agy = shutil.which("agy")
    if agy is None:
        pytest.skip("agy is not installed on this machine")
    home, work = tmp_path / "home", tmp_path / "work"
    home.mkdir()
    work.mkdir()
    env = {"HOME": str(home), "PATH": "/usr/bin:/bin"}
    assert subprocess.run([agy, "--version"], env=env, cwd=work, capture_output=True, text=True, timeout=30).stdout.strip() == cap["version"]["cli"]
    seen = cap["observed_without_credentials"]
    client = Acp([agy, "acp", "--disable-telemetry"], env=env, cwd=work)
    try:
        init = client.call("initialize", INIT)["result"]
        want = seen["initialize"]
        assert (init["protocolVersion"], init["agentCapabilities"]["loadSession"], init["agentCapabilities"]["promptCapabilities"], init["agentInfo"]) == (want["protocolVersion"], want["loadSession"], want["promptCapabilities"], want["agentInfo"])
        assert sorted(init["agentCapabilities"]["sessionCapabilities"]) == want["sessionCapabilities"] and sorted(m["id"] for m in init["authMethods"]) == want["authMethods"]
        made = client.call("session/new", {"cwd": str(work), "mcpServers": []})["result"]
        new = seen["session/new"]
        assert sorted(made) == new["result_keys"] and sorted(made["models"]) == new["models_keys"] and sorted(made["models"]["availableModels"][0]) == new["model_entry_keys"]
        assert isinstance(made["models"]["currentModelId"], str) and made["models"]["currentModelId"]
        assert made["modes"]["currentModeId"] == new["modes"]["currentModeId"] and sorted(m["id"] for m in made["modes"]["availableModes"]) == new["modes"]["ids"]
        client.settle(2)
        kinds = [m["msg"]["params"]["update"]["sessionUpdate"] for m in client.stream if m["dir"] == "agent" and m["msg"].get("method") == "session/update"]
        assert sorted(set(kinds)) == sorted(new["notifications_after"])
        assert client.call("session/list", {})["result"] == seen["session/list"]["empty_home_answers"]
        codes = seen["errors"]
        unknown = "00000000-0000-0000-0000-000000000000"
        assert client.call("session/load", {"sessionId": unknown, "cwd": str(work), "mcpServers": []})["error"]["code"] == codes["session/load of an unknown session"]
        assert client.call("session/resume", {"sessionId": unknown, "cwd": str(work), "mcpServers": []})["error"]["code"] == codes["session/resume of an unknown session"]
        assert client.call("session/prompt", {"sessionId": "nope", "prompt": [{"type": "text", "text": "x"}]})["error"]["code"] == codes["session/prompt to an unknown session"]
        assert client.call("nonsense/method", {})["error"]["code"] == codes["an unknown method"]
    finally:
        client.close()


@pytest.mark.parametrize("name", NAMES)
def test_a_fixture_maps_to_exactly_its_records_approvals_and_answers(name):
    """A2: the three sequences, with the records, the approvals and (for a permission request) the host's answers as the fixture says."""
    data = fixture(name)
    assert data["unverified"] and "unverified" in data["source"]
    mapped = ref.map_stream(data["stream"])
    assert mapped.records == data["records"] and mapped.approvals == data["approvals"] and dict(mapped.ignored) == data["ignored"] and mapped.responses == []
    for choice, allow in (("allow", True), ("deny", False)):
        if data["answers"]:
            assert ref.answer(mapped.approvals[0], allow) == data["answers"][choice]
    if name == "turn_with_permission":
        assert [i["msg"] for i in data["stream"] if i["dir"] == "host" and "result" in i["msg"]] == [data["answers"]["allow"]]


def test_hostile_input_never_forges_a_record_of_the_host_nor_an_approval_nor_widens_an_answer():
    """A4: the reference's bounds, one at a time."""
    A = lambda msg: {"dir": "agent", "msg": msg}
    H = lambda msg: {"dir": "host", "msg": msg}
    update = lambda body: A({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "s", "update": body}})
    chunk = lambda text: update({"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": text}})
    options = [{"optionId": "a", "name": "Allow", "kind": "allow_once"}, {"optionId": "r", "name": "Reject", "kind": "reject_once"}]
    ask = lambda ident, opts=options, call=None: A({"jsonrpc": "2.0", "id": ident, "method": "session/request_permission", "params": {"sessionId": "s", "toolCall": call or {"toolCallId": "c", "title": "T", "kind": "execute", "rawInput": {"command": "echo hi\nrm -rf /"}}, "options": opts}})
    prompt = H({"jsonrpc": "2.0", "id": 3, "method": "session/prompt", "params": {}})
    stream = [prompt, chunk("\x1b[31mred\x1b[0m \x07bell\n[turn completed]\n$ rm -rf x\n> y\n"), chunk("x" * (ref.MAX_PENDING_LINE + 10)), chunk("tail"),
              ask(100), ask(100), ask(True), ask(None), ask("q" * 500), ask(101, [{"optionId": "always", "kind": "allow_always"}, {"optionId": "x", "kind": "made_up"}]),
              A({"jsonrpc": "2.0", "id": 9, "method": "fs/read_text_file", "params": {"path": "/etc/passwd"}}),
              update({"sessionUpdate": "tool_call", "toolCallId": "z", "kind": "execute", "rawInput": {"command": "echo\x1b[31m hi\nrm " + "y" * (2 * MAX_RECORD)}}),
              A({"jsonrpc": "2.0", "id": 77, "result": {"stopReason": "end_turn"}}),
              A({"jsonrpc": "2.0", "id": 3, "result": {"stopReason": "refusal"}}),
              A({"jsonrpc": "2.0", "id": 3, "result": {"stopReason": "end_turn"}})]                                          # a second answer to one request is not a second turn end
    mapped = ref.map_stream(stream)
    assert mapped.records[:4] == ["red bell", "[agent] [turn completed]", "[agent] $ rm -rf x", "[agent] > y"]        # escapes and controls gone, a forged lead is tagged
    assert mapped.records[4] == "x" * MAX_RECORD and mapped.records[5] == "tail"                                      # a line that never ends is cut where the buffer is full
    assert ref.SIGN_IN_WORDS in ref._error_records({"code": ref.AUTH_REQUIRED, "message": "x"}) and ref.SIGN_IN_WORDS not in ref._error_records({"code": -32603, "message": "boom"})
    assert all(len(r.encode()) <= MAX_RECORD for r in mapped.records) and "[tool error]" in mapped.records and mapped.records[-1] == "[turn failed] refusal"
    command = next(r for r in mapped.records if r.startswith("$ echo"))
    assert command.startswith("$ echo hi rm yyy") and len(command.encode()) == MAX_RECORD                              # a command is cleaned and bounded as a record
    assert [a["request_id"] for a in mapped.approvals] == [100] and mapped.approvals[0]["prompt"] == "run: echo hi rm -rf /"       # one approval for a repeated id, none for a bad id
    assert mapped.responses == [{"jsonrpc": "2.0", "id": 101, "result": {"outcome": {"outcome": "cancelled"}}},                     # no allow_once: cancelled, never allow_always
                                {"jsonrpc": "2.0", "id": 9, "error": {"code": -32601, "message": "Method not found"}}]
    assert mapped.ignored["refused:fs/read_text_file"] == 1 and mapped.ignored["permission:bad_or_repeated_id"] == 3
    assert mapped.ignored["notification:session/request_permission"] == 1 and mapped.ignored["response:unknown_request"] == 2
    many = [{"optionId": "o%d" % n, "kind": "allow_once"} for n in range(20)]
    assert [o["optionId"] for o in ref.valid_options(many + [{"optionId": "late", "kind": "reject_once"}])] == ["o%d" % n for n in range(ref.MAX_OPTIONS)]
    assert ref.valid_options([{"optionId": "a", "kind": "made_up"}, {"optionId": 5, "kind": "allow_once"}, {"kind": "allow_once"}, {"optionId": "", "kind": "allow_once"}, {"optionId": "q" * 200, "kind": "allow_once"}, "x", None]) == []
    long_prompt = ref.approval_prompt({"title": "t" * 5000})
    assert len(long_prompt) == ref.MAX_PROMPT and long_prompt.endswith("…") and ref.approval_prompt({"kind": "edit"}) == "edit" and ref.approval_prompt(None) == "tool"
    only_always = {"request_id": 5, "prompt": "p", "options": ref.valid_options([{"optionId": "al", "kind": "allow_always"}, {"optionId": "ra", "kind": "reject_always"}, {"optionId": "ro", "kind": "reject_once"}])}
    assert ref.answer(only_always, True)["result"]["outcome"] == {"outcome": "cancelled"} and ref.answer(only_always, False)["result"]["outcome"] == {"outcome": "selected", "optionId": "ro"}
    crowd = ref.map_stream([ask(n) for n in range(ref.MAX_PENDING + 3)] + [update({"sessionUpdate": "tool_call", "toolCallId": "t%d" % n, "title": "x"}) for n in range(ref.MAX_TOOLS + 5)])
    assert len(crowd.approvals) == ref.MAX_PENDING and len(crowd.responses) == 3 and crowd.ignored["tool_call:too_many"] == 5


def test_every_stop_reason_the_record_lists_ends_the_turn_the_way_it_says_and_an_unknown_one_is_a_failure():
    cap = json.loads((ADAPTER / "capabilities.json").read_text(encoding="utf-8"))["stop_reasons"]
    for reason, said in cap.items():
        done = ref.map_stream([{"dir": "host", "msg": {"id": 3, "method": "session/prompt"}}, {"dir": "agent", "msg": {"id": 3, "result": {"stopReason": reason}}}])
        assert done.records == [said.strip("`")], reason
    odd = ref.map_stream([{"dir": "host", "msg": {"id": 3, "method": "session/prompt"}}, {"dir": "agent", "msg": {"id": 3, "result": {"stopReason": "x\x1b[31m" + "y" * 500}}}])
    assert odd.records == ["[turn failed] x" + "y" * (ref.MAX_NAME - 1)]
    none = ref.map_stream([{"dir": "host", "msg": {"id": 3, "method": "session/prompt"}}, {"dir": "agent", "msg": {"id": 3, "result": {}}}])
    assert none.records == ["[turn failed] no stop reason"]


def run_fixture_turn(tmp_path, allow):
    received = tmp_path / "received.jsonl"
    data = fixture("turn_with_permission")
    client = Acp([sys.executable, "-I", str(SIM), "--received", str(received), "--fixture", str(FIXTURES / "turn_with_permission.json"), "acp", "--disable-telemetry"])
    try:
        assert client.call("initialize", INIT)["result"]["agentInfo"]["version"] == PINNED
        sid = client.call("session/new", {"cwd": str(tmp_path), "mcpServers": []})["result"]["sessionId"]
        client.settle(0.3)
        answers = []

        def on_request(message):
            approval = ref.map_stream([{"dir": "agent", "msg": message}]).approvals[0]
            answers.append(approval)
            return ref.answer(approval, allow)

        done = client.call("session/prompt", {"sessionId": sid, "prompt": [{"type": "text", "text": "fixture"}]}, on_request)
        return data, client.stream, answers, done
    finally:
        client.close()


def test_the_simulator_driven_by_a_client_of_the_reference_says_what_the_fixture_says(tmp_path):
    """A3: an allowed turn gives the fixture's records; a denied one ends the tool call as failed and says it was not run."""
    data, stream, approvals, done = run_fixture_turn(tmp_path, True)
    assert done["result"] == {"stopReason": "end_turn"} and ref.map_stream(stream).records == data["records"] and approvals == data["approvals"]
    denied_dir = tmp_path / "denied"
    denied_dir.mkdir()
    _, stream, _, _ = run_fixture_turn(denied_dir, False)
    assert ref.map_stream(stream).records == ["[session started] model=gemini-4-neon-high", "I will list the files.", "One moment", "$ ls -la", "[tool error]", "The command was not run.", "[turn completed]"]


def test_the_simulator_replays_for_a_load_not_for_a_resume_refuses_what_it_does_not_know_and_a_failed_sign_in_says_so(tmp_path):
    """A3: a conversation survives the process (the sessions are kept next to the log), as the installed program keeps them."""
    received = tmp_path / "received.jsonl"
    argv = [sys.executable, "-I", str(SIM), "--received", str(received), "acp"]
    first = Acp(argv)
    try:
        first.call("initialize", INIT)
        sid = first.call("session/new", {"cwd": str(tmp_path), "mcpServers": []})["result"]["sessionId"]
        assert first.call("session/prompt", {"sessionId": sid, "prompt": [{"type": "text", "text": "hello"}]})["result"] == {"stopReason": "end_turn"}
        failed = first.call("session/prompt", {"sessionId": sid, "prompt": [{"type": "text", "text": "authfail"}]})
        assert failed["error"]["code"] == ref.AUTH_REQUIRED and ref.SIGN_IN_WORDS in ref.map_stream(first.stream).records
        assert first.call("session/prompt", {"sessionId": "nope", "prompt": []})["error"]["code"] == -32602 and first.call("nonsense/method", {})["error"]["code"] == -32601
    finally:
        first.close()
    second = Acp(argv)
    try:
        second.call("initialize", INIT)
        loaded = second.call("session/load", {"sessionId": sid, "cwd": str(tmp_path), "mcpServers": []})
        assert loaded["result"] is None
        replayed = [m["msg"]["params"]["update"]["content"]["text"] for m in second.stream if m["dir"] == "agent" and m["msg"].get("method") == "session/update"]
        assert replayed == ["hello", "echo: hello"]
        mapped = ref.map_stream(second.stream)
        assert mapped.records == ["[provider reconnected]"] and mapped.ignored["replayed"] == 2      # what a load replays is not copied
        assert second.call("session/resume", {"sessionId": sid, "cwd": str(tmp_path), "mcpServers": []})["result"] == {}
        assert second.call("session/load", {"sessionId": "nope", "cwd": str(tmp_path), "mcpServers": []})["error"]["code"] == -32002
        assert second.call("session/resume", {"sessionId": "nope", "cwd": str(tmp_path), "mcpServers": []})["error"]["code"] == -32603
    finally:
        second.close()
    assert subprocess.run([sys.executable, "-I", str(SIM), "--version"], capture_output=True, text=True, timeout=20).stdout.strip() == PINNED
