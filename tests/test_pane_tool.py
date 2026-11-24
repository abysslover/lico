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


def cues(world, *tabs):
    """What the host's `status` says agents did lately in the panes (of the tabs named: the host of this module is shared and a cue lasts eight seconds)."""
    return [c for c in Conn(world.host.socket).call(op="status")["pane_cues"] if not tabs or c["tab"] in tabs]


def test_a_type_or_a_key_leaves_a_cue_for_the_persons_window_and_list_read_or_a_refused_call_leave_none(world):
    sid, _ = world.session()
    tab = open_terminal(world, "cat")
    assert cues(world, tab) == []
    ok(world, sid, action="list")
    ok(world, sid, action="read", tab=tab)
    assert cues(world, tab) == [], "looking is not acting"
    ok(world, sid, action="type", tab=tab, text="hi")
    [cue] = cues(world, tab)
    assert (cue["tab"], cue["did"]) == (tab, "typed") and 0 <= cue["ms"] < 8000 and 0 < len(cue["by"]) <= 24, cue
    ok(world, sid, action="key", tab=tab, key="Return")
    [cue] = cues(world, tab)
    assert (cue["tab"], cue["did"]) == (tab, "pressed a key"), "the last thing in the tab is what the header says"
    text, is_error = pane(world, sid, action="key", tab=tab, key="Hyper+Nothing")
    assert is_error, text
    [cue] = cues(world, tab)
    assert cue["did"] == "pressed a key", "a refused call did nothing, so it leaves nothing"
    other = open_terminal(world, "cat")
    ok(world, sid, action="type", tab=other, text="x")
    assert sorted(c["tab"] for c in cues(world, tab, other)) == sorted([tab, other]), "each tab has its own"


def test_the_switch_off_takes_the_tool_out_of_the_list_and_refuses_a_call_and_on_brings_it_back_without_a_restart(world):
    sid, _ = world.session()
    tab = open_terminal(world, "cat")
    names = lambda: [t["name"] for t in rpc(world.config(sid), "tools/list", {})[1]["result"]["tools"]]
    assert "pane" in names() and Conn(world.host.socket).call(op="status")["settings"]["agent_panes"] is True, "on is the default"
    set_ = Conn(world.host.socket).call(op="settings.set", agent_panes=False)
    assert set_["ok"] == 1 and set_["settings"]["agent_panes"] is False, set_
    assert "pane" not in names() and "computer" in names(), "only that tool goes"
    status, body = rpc(world.config(sid), "tools/call", {"name": "pane", "arguments": {"action": "type", "tab": tab, "text": "x"}})
    assert status == 200 and body["error"]["message"] == "unknown tool", body
    assert "x" not in screen_of(world, tab), "nothing was typed"
    assert Conn(world.host.socket).call(op="status")["settings"]["agent_panes"] is False
    again = Conn(world.host.socket).call(op="settings.set", agent_panes=True)
    assert again["settings"]["agent_panes"] is True
    assert "pane" in names()
    ok(world, sid, action="type", tab=tab, text="back")
    wait_for(lambda: "back" in screen_of(world, tab))


def test_a_pause_takes_the_tool_out_of_the_list_for_a_time_and_a_resume_or_the_end_of_it_brings_it_back(world):
    """M24-P07: the stop key's pause, over the host's own socket: the same reading of the file as the switch."""
    sid, _ = world.session()
    tab = open_terminal(world, "cat")
    names = lambda: [t["name"] for t in rpc(world.config(sid), "tools/list", {})[1]["result"]["tools"]]
    paused = Conn(world.host.socket).call(op="settings.set", agent_panes_pause=True)
    assert paused["ok"] == 1 and paused["settings"]["agent_panes"] is False and 0 < paused["settings"]["agent_panes_paused"] <= 300, paused
    assert "pane" not in names() and "computer" in names(), "only that tool goes"
    status, body = rpc(world.config(sid), "tools/call", {"name": "pane", "arguments": {"action": "type", "tab": tab, "text": "x"}})
    assert status == 200 and body["error"]["message"] == "unknown tool", body
    assert "x" not in screen_of(world, tab), "nothing was typed"
    status = Conn(world.host.socket).call(op="status")["settings"]
    assert status["agent_panes"] is False and status["agent_panes_paused"] > 0
    resumed = Conn(world.host.socket).call(op="settings.set", agent_panes_pause=False)
    assert resumed["ok"] == 1 and resumed["settings"]["agent_panes"] is True and "agent_panes_paused" not in resumed["settings"], resumed
    assert "pane" in names()
    ok(world, sid, action="type", tab=tab, text="back")
    wait_for(lambda: "back" in screen_of(world, tab))
    # off for good wins over a pause and tells nothing of one
    Conn(world.host.socket).call(op="settings.set", agent_panes=False)
    off = Conn(world.host.socket).call(op="settings.set", agent_panes_pause=True)
    assert off["settings"]["agent_panes"] is False and "agent_panes_paused" not in off["settings"], off
    Conn(world.host.socket).call(op="settings.set", agent_panes=True)
    assert "pane" in names(), "switched on: on, with no pause waiting in the file"


def screen_of(world, tab) -> str:
    reply = Conn(world.host.socket).call(op="terminal.watch", tab=tab, seq=0)
    return "".join(piece[0] for line in reply["lines"] for piece in line)


def test_the_switch_is_a_boolean_and_the_command_line_takes_on_and_off(world):
    for bad in ("off", 0, None, "false"):
        reply = Conn(world.host.socket).call(op="settings.set", agent_panes=bad)
        assert reply["ok"] == 0, (bad, reply)
    from tests.test_everyday_comfort import cli_run
    assert cli_run(world.host.socket, "settings", "--agent-panes", "off").returncode == 0
    assert json.loads(cli_run(world.host.socket, "settings").stdout)["settings"]["agent_panes"] is False
    assert cli_run(world.host.socket, "settings", "--agent-panes", "maybe").returncode != 0
    assert cli_run(world.host.socket, "settings", "--agent-panes", "on").returncode == 0
    assert json.loads(cli_run(world.host.socket, "settings").stdout)["settings"]["agent_panes"] is True
