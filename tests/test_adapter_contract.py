import copy
import json
import tempfile
from pathlib import Path

from lico_measure.providers import (
    CAPABILITIES, generate_schema, installed_version, message_errors, problems, schema_inventory,
)
from lico_measure.rig import REPO
from lico_protocol.codex_mapping import Mapper

FIXTURES = REPO / "tests" / "fixtures" / "codex_app_server"


PINNED = "0.164.0"      # the version the capability record describes; a new installed version must fail this test until re-recorded


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
