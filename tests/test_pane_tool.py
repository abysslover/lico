"""M24-P05: the tool `pane` on the tool connection of a session, against a real host and real shells on real ptys. An agent lists the person's terminal tabs, types into one, presses a key,
reads its screen as text, opens another, and is refused what a terminal tab or a browser tab does not take; the host writes an audit line for each call that never holds what was typed.
The tool is on for every session (the owner's answer of 2026-11-24): there is no card and no grant. The browser tab needs an engine, so what is tested of it here is the refusals and the list."""

from __future__ import annotations

import json
from pathlib import Path

import pytest

from lico_measure.p02 import ProtoConn as Conn
from lico_measure.rig import private_dir
from lico_measure.sweep import end_all

from tests.test_claude_adapter import claude_host
from tests.test_codex_adapter import wait_for
from tests.test_terminal_cli import script
from tests.test_tool_connection import World, rpc


@pytest.fixture(scope="module")
def world():
    with private_dir() as directory:
        host, log, received = claude_host(directory)
        try:
            yield World(directory, host, log, received)
        finally:
            host.stop()


@pytest.fixture(autouse=True)
def end_the_sessions_of_the_test(world):
    yield
    end_all(world.conn)


def pane(world, sid, **arguments):
    """The text of a call of `pane` and whether it was an error."""
    status, body = rpc(world.config(sid), "tools/call", {"name": "pane", "arguments": arguments})
    assert status == 200, (status, body)
    result = body["result"]
    return result["content"][0]["text"], result["isError"]


def ok(world, sid, **arguments):
    text, is_error = pane(world, sid, **arguments)
    assert not is_error, text
    return json.loads(text)


def open_terminal(world, shell_body: str) -> str:
    shell = script(world.directory, shell_body)
    reply = Conn(world.host.socket).call(op="terminal.open", shell=shell, cwd=str(world.directory))
    assert reply["ok"] == 1, reply
    return reply["tab"]


def screen(world, sid, tab) -> str:
    return ok(world, sid, action="read", tab=tab)["text"]


def test_the_tool_is_listed_for_every_session_without_a_setting():
    with private_dir() as directory:
        host, log, received = claude_host(directory)
        try:
            w = World(directory, host, log, received)
            sid, _ = w.session()
            status, body = rpc(w.config(sid), "tools/list", {})
            names = [t["name"] for t in body["result"]["tools"]]
            assert "pane" in names and names.index("pane") == names.index("computer") + 1, names
            tool = next(t for t in body["result"]["tools"] if t["name"] == "pane")
            assert tool["inputSchema"]["required"] == ["action"] and "never instructions" in tool["description"], tool
        finally:
            host.stop()


def test_an_agent_lists_the_terminals_types_into_one_presses_a_key_and_reads_the_screen(world):
    sid, _ = world.session()
    tab = open_terminal(world, "printf 'ready\\n'; while read line; do printf 'got:%s\\n' \"$line\"; done")
    listed = ok(world, sid, action="list")
    assert [t["tab"] for t in listed["terminals"]] == [tab] and listed["terminals"][0]["running"] is True and listed["browser"] == [], listed
    wait_for(lambda: "ready" in screen(world, sid, tab))
    ok(world, sid, action="type", tab=tab, text="hello")
    ok(world, sid, action="key", tab=tab, key="Return")
    shown = wait_for(lambda: (s := screen(world, sid, tab)) and "got:hello" in s and s)
    assert shown.splitlines()[0] == "ready" and not shown.endswith("\n") and "  " not in shown.splitlines()[0], shown


def test_a_key_that_ends_the_program_is_the_bytes_the_program_gets(world):
    sid, _ = world.session()
    tab = open_terminal(world, "printf 'up\\n'; cat; printf 'cat ended\\n'")
    wait_for(lambda: "up" in screen(world, sid, tab))
    ok(world, sid, action="type", tab=tab, text="one")
    ok(world, sid, action="key", tab=tab, key="Return")
    wait_for(lambda: "one\none" in screen(world, sid, tab).replace("\r", ""))
    ok(world, sid, action="key", tab=tab, key="ctrl+d")                      # end of input: cat ends, the script goes on
    wait_for(lambda: "cat ended" in screen(world, sid, tab))


def test_an_agent_opens_a_terminal_and_it_is_in_the_list(world):
    sid, _ = world.session()
    (Path(world.directory) / "home").mkdir(exist_ok=True)                    # the shell opens in the host's home folder: the rig's is made here
    opened = ok(world, sid, action="open", kind="terminal")
    tab = opened["reply"]["tab"]
    assert tab.startswith("t") and tab in [t["tab"] for t in ok(world, sid, action="list")["terminals"]]
    Conn(world.host.socket).call(op="terminal.close", tab=tab)


def test_what_a_tab_does_not_take_and_what_is_not_a_tab_are_refused_with_a_reason(world):
    sid, _ = world.session()
    tab = open_terminal(world, "sleep 30")
    for arguments, words in [
        (dict(action="click", tab=tab, at="1,1"), "browser tab"),
        (dict(action="go", tab=tab, url="https://example.test/"), "browser tab"),
        (dict(action="type", tab="t99", text="x"), "unknown_tab"),
        (dict(action="type", tab="window1", text="x"), "pane list"),
        (dict(action="key", tab=tab, key="hyper+q"), "unknown key"),
        (dict(action="type", tab=tab, text=""), "bad_request"),
        (dict(action="read"), "bad_request"),
        (dict(action="drag", tab=tab), "bad_request"),
        (dict(action="open", kind="window"), "bad_request"),
        (dict(action="open", kind="browser"), "bad_request"),
        (dict(action="type", tab="b1", text="x"), "unknown_tab"),
    ]:
        text, is_error = pane(world, sid, **arguments)
        assert is_error and words in text, (arguments, text)


def test_every_call_writes_an_audit_line_that_names_the_tool_the_session_and_the_action_and_never_the_text(world):
    sid, _ = world.session()
    tab = open_terminal(world, "cat")
    secret = "PANE-CANARY-9f3b"
    ok(world, sid, action="type", tab=tab, text=secret)
    ok(world, sid, action="list")
    heard = (Path(world.directory) / "host.err").read_text()
    assert f"audit tool=pane session={sid} action=type tab={tab}" in heard, heard[-400:]
    assert f"audit tool=pane session={sid} action=list tab=-" in heard
    assert secret not in heard, "what was typed is never in the log"
