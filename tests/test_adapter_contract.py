import contextlib
import copy
import json
import signal
import tempfile
from pathlib import Path

import pytest

from lico_measure import providers
from lico_measure.providers import (
    CAPABILITIES, generate_schema, installed_version, message_errors, problems, schema_inventory,
)
from lico_measure.rig import REPO
from lico_protocol.codex_mapping import Mapper

FIXTURES = REPO / "tests" / "fixtures" / "codex_app_server"


PINNED = "0.167.0"      # the version the capability record describes; a new installed version must fail this test until re-recorded


def test_adapter_capabilities_pin_the_version_and_mark_unverified():
    with tempfile.TemporaryDirectory(dir=REPO / "private" / "scratch") as scratch:
        home = Path(scratch)
        installed = installed_version(home)
        generate_schema(home, home / "schema")
        inventory = schema_inventory(home / "schema")
    cap = json.loads(CAPABILITIES.read_text())
    # the recorded data describes exactly the installed provider
    assert problems(cap, installed, inventory) == []
    assert cap["version"]["pinned"] and cap["version"]["cli"] == installed == PINNED
    assert set(cap["server_requests"]) == set(inventory["server_requests"])
    assert cap["unverified"] and all(item.get("reason") for item in cap["unsupported"])
    # the recorded unauthenticated handshake agrees with what the capability data says was observed
    handshake = json.loads((FIXTURES / "handshake_unauthenticated.json").read_text())
    account = [s["msg"]["result"] for s in handshake if s["dir"] == "server" and s["msg"].get("id") == 2][0]
    assert account["account"] is None and account["requiresOpenaiAuth"] is cap["observed_without_credentials"]["account/read"]["requiresOpenaiAuth"]
    # and the check is not lax: each way of going stale or hiding a gap is reported
    stale = copy.deepcopy(cap)
    stale["version"]["cli"] = "0.0.1"
    assert any("not the pinned" in p for p in problems(stale, installed, inventory))
    unpinned = copy.deepcopy(cap)
    unpinned["version"]["pinned"] = False
    assert any("not pinned" in p for p in problems(unpinned, installed, inventory))
    missing = copy.deepcopy(cap)
    missing["server_requests"].pop("item/tool/call")
    assert any("item/tool/call" in p for p in problems(missing, installed, inventory))
    unknown = copy.deepcopy(cap)
    unknown["client_requests"]["mapped"].append("thread/invented")
    assert any("thread/invented" in p for p in problems(unknown, installed, inventory))
    hidden = copy.deepcopy(cap)
    hidden["unverified"] = []
    assert any("unverified" in p for p in problems(hidden, installed, inventory))
    drifted = copy.deepcopy(cap)
    drifted["protocol_schema"]["sha256"]["ClientRequest.json"] = "0" * 64
    assert any("schema differs" in p for p in problems(drifted, installed, inventory))


def test_fixture_events_map_to_host_records_in_order():
    steps = json.loads((FIXTURES / "turn_with_approval.json").read_text())
    # the recorded messages are valid for the installed provider's own schema
    with tempfile.TemporaryDirectory(dir=REPO / "private" / "scratch") as scratch:
        home = Path(scratch)
        generate_schema(home, home / "schema")
        for step in steps:
            if step["dir"] == "server":
                message = step["msg"]
                file = "ServerRequest.json" if "id" in message else "ServerNotification.json"
                assert message_errors(home / "schema", file, message["method"], message.get("params", {})) == [], message["method"]
    result = Mapper().feed(steps)
    # what the user sees, in the order it happened: text, wide and combining characters kept,
    # escape sequences and control characters gone, commands and exit codes as lines
    assert result.records == [
        "[turn started]",
        "안녕하세요. 작업을 시작합니다.",
        "두 번째 줄 e\u0301 漢字 🙂",   # the combining accent is kept as sent, not normalised
        "hidden controls and tab done",
        "$ ls -la",
        "total 0",
        "file-한글.txt",
        "[exit 0]",
        "[error] network unavailable",
        "[turn completed]",
    ]
    # an approval answer becomes the provider's decision for exactly that request, nothing else is sent,
    # and a request the client cannot serve is answered with an error instead of being dropped
    assert result.responses == [
        {"id": 7, "result": {"decision": "accept"}},
        {"id": 9, "error": {"code": -32601, "message": "unsupported by this client"}},
        {"id": 11, "result": {"decision": "decline"}},
    ]
    assert result.approvals == []
    assert result.ignored["thread/tokenUsage/updated"] == 1 and result.ignored["refused:account/chatgptAuthTokens/refresh"] == 1
    # a stale or forged answer sends nothing
    mapper = Mapper()
    mapper.feed(steps[:3])
    try:
        mapper.answer(99, "allow")
        raise AssertionError("a forged approval answer was accepted")
    except KeyError:
        pass
    assert mapper.out.responses == []



@contextlib.contextmanager
def sigint_not_ignored():
    """A shell that starts a job in the background hands SIGINT on as ignored, and a child that ignores it cannot end itself by it, as the real provider was ended."""
    before = signal.signal(signal.SIGINT, signal.SIG_DFL)
    try:
        yield
    finally:
        signal.signal(signal.SIGINT, before)


def fake_codex(tmp_path, monkeypatch, name, body):
    """A stand-in for the codex command that counts its starts and then does what body says (n is the number of this start). Returns the folder to use as the home and the count of starts."""
    folder = tmp_path / name
    folder.mkdir()
    counts, script = folder / "starts", folder / "codex"
    script.write_text(f'#!/bin/sh\necho x >> "{counts}"\nn=$(wc -l < "{counts}")\n{body}\n')
    script.chmod(0o755)
    monkeypatch.setattr(providers, "codex_binary", lambda: str(script))
    return folder, lambda: len(counts.read_text().split())


SCHEMA_JOB = 'mkdir -p "$4" && echo "{}" > "$4/ClientRequest.json"'      # codex app-server generate-json-schema --out DIR
ENDED_BY_SIGINT_ON_THE_FIRST_START = 'if [ "$n" = 1 ]; then kill -INT $$; fi'


def test_a_schema_start_that_a_signal_ended_is_made_once_more_and_any_other_failure_is_reported_at_once_with_its_code_and_the_end_of_stderr(tmp_path, monkeypatch):
    with sigint_not_ignored():
        folder, starts = fake_codex(tmp_path, monkeypatch, "stopped_once", f"{ENDED_BY_SIGINT_ON_THE_FIRST_START}\n{SCHEMA_JOB}")
        generate_schema(folder / "home", folder / "schema")
        assert starts() == 2 and (folder / "schema" / "ClientRequest.json").is_file()           # the second start did the job
        folder, starts = fake_codex(tmp_path, monkeypatch, "failed", 'echo "no space left on device" >&2; exit 1')
        with pytest.raises(RuntimeError) as failure:
            generate_schema(folder / "home", folder / "schema")
        assert starts() == 1                                                                    # a code of 1 is an answer, not a stop
        assert "code 1" in str(failure.value) and "no space left on device" in str(failure.value)
        folder, starts = fake_codex(tmp_path, monkeypatch, "stopped_twice", "kill -INT $$")
        with pytest.raises(RuntimeError) as failure:
            generate_schema(folder / "home", folder / "schema")
        assert starts() == 2 and "code -2" in str(failure.value) and "2 start(s)" in str(failure.value)      # stopped twice: no third start


def test_the_message_of_a_failed_start_keeps_the_last_300_characters_of_stderr_and_not_the_first_ones(tmp_path, monkeypatch):
    folder, _ = fake_codex(tmp_path, monkeypatch, "long_stderr", f'echo "FIRST{"x" * 600}LAST" >&2; exit 3')
    with pytest.raises(RuntimeError) as failure:
        generate_schema(folder / "home", folder / "schema")
    assert "code 3" in str(failure.value) and "LAST" in str(failure.value) and "FIRST" not in str(failure.value)
    assert "x" * 296 in str(failure.value) and "x" * 297 not in str(failure.value)             # 300 characters of the tail: 296 x and LAST


def test_the_version_start_follows_the_same_rule_as_the_schema_start(tmp_path, monkeypatch):
    with sigint_not_ignored():
        folder, starts = fake_codex(tmp_path, monkeypatch, "stopped_once", f'{ENDED_BY_SIGINT_ON_THE_FIRST_START}\necho "codex-cli 9.9.9"')
        assert installed_version(folder / "home") == "9.9.9" and starts() == 2
        folder, starts = fake_codex(tmp_path, monkeypatch, "failed", 'echo "bad config" >&2; exit 1')
        with pytest.raises(RuntimeError) as failure:
            installed_version(folder / "home")
        assert starts() == 1 and "code 1" in str(failure.value) and "bad config" in str(failure.value)
        folder, starts = fake_codex(tmp_path, monkeypatch, "stopped_twice", "kill -INT $$")
        with pytest.raises(RuntimeError) as failure:
            installed_version(folder / "home")
        assert starts() == 2 and "code -2" in str(failure.value)
