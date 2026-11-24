"""M24-P05b: what the person sees and can do about an agent that drives their panes, in the real window with a real host. An agent's `pane` call (the tool connection of a real session) shows in the
header of the terminal that is in front for a few seconds, with the one key that stops it; the key (`Ctrl+Alt+Esc`) works with the terminal in front and never reaches the shell, switches the
control off at the host (the tool is gone from the list and a call is refused) and the menu `More > Settings` switches it on again. The agent is the mocked Claude of the other window tests."""

from __future__ import annotations

from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.gui_helpers import menu_click
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_icon_toolbar_wizard import _ready, _rig
from tests.test_terminal_tab import terminal_from_the_icon
from tests.test_tool_connection import config_of, rpc

CUE = "(Ctrl+Alt+Esc stops agents)"


def _window(root: Path):
    kwargs, made, _ = _rig(root, [("Work", "claude")])
    rc = root / "rc"
    rc.write_text("PS1='lico$ '\nPS2='> '\n")
    kwargs["env_extra"].update({"SHELL": "/bin/sh", "ENV": str(rc)})
    return ProductWindow(**kwargs), made


def _pane(config, **arguments):
    status, body = rpc(config, "tools/call", {"name": "pane", "arguments": arguments})
    return status, body


def _tools(config) -> list[str]:
    return [t["name"] for t in rpc(config, "tools/list", {})[1]["result"]["tools"]]


def _screen(w: ProductWindow) -> str:
    return "\n".join(w.ui()["terminal"]["lines"] or [])


def test_an_agents_typing_shows_in_the_header_the_stop_key_works_with_the_terminal_in_front_and_the_menu_turns_it_on_again():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        window, made = _window(root)
        with window as w:
            conn = Conn(w.socket)
            _ready(w)
            made_session = conn.call(op="session.create", kind="claude", account=made["Work"], cwd=str(root))
            assert made_session["ok"] == 1, made_session
            sid = made_session["session"]
            found = next(Path(w.directory).rglob(f"{sid}.mcp.json"))
            config = config_of(found.parent.parent, sid)
            terminal_from_the_icon(w)
            wait_for(lambda: "lico$" in _screen(w))
            tab = w.ui()["terminal"]["active"]
            assert "stops agents" not in w.ui()["terminal"]["header"], "no agent has acted"

            # looking is not acting: no cue; typing is, and the header of the terminal in front says who and what, and the key that stops it
            status, body = _pane(config, action="read", tab=tab)
            assert status == 200 and not body["result"]["isError"], body
            assert "stops agents" not in w.ui()["terminal"]["header"]
            status, body = _pane(config, action="type", tab=tab, text="cat -v")
            assert status == 200 and not body["result"]["isError"], body
            _pane(config, action="key", tab=tab, key="Return")
            header = wait_for(lambda: (h := w.ui()["terminal"]["header"]) and CUE in h and h)
            assert "pressed a key" in header and header.startswith("terminal | "), header
            wait_for(lambda: "cat -v" in _screen(w))
            assert w.ui()["settings"]["pane_cues"][0]["tab"] == tab

            # the stop key, with the terminal in front: it is the window's, never the shell's (`cat -v` would show a stray Esc as `^[`)
            w.key("ctrl+alt+Escape")
            wait_for(lambda: conn.call(op="status")["settings"]["agent_panes"] is False)
            u = wait_for(lambda: (x := w.ui()) and x["settings"]["agent_panes"] is False and x)
            assert "stops agents" not in u["terminal"]["header"] and u["settings"]["pane_cues"] == [], "the cue goes with the stop"
            assert "can no longer type or click" in u["header"] and "More > Settings" in u["header"], u["header"]
            w.type_text("x")
            w.key("Return")
            screen = wait_for(lambda: (s := _screen(w)) and s.count("x") >= 2 and s)
            assert "^[" not in screen, f"the stop key reached the shell: {screen!r}"

            # off at the host: the tool is not offered and a call is not a tool
            assert "pane" not in _tools(config) and "computer" in _tools(config)
            status, body = _pane(config, action="type", tab=tab, text="NOT TYPED")
            assert status == 200 and body["error"]["message"] == "unknown tool", body

            # the menu says it is off and turns it on again, and the tool is back
            menu_click(w, 4, "Agent pane control: off")
            wait_for(lambda: conn.call(op="status")["settings"]["agent_panes"] is True)
            wait_for(lambda: "pane" in _tools(config))
            wait_for(lambda: w.ui()["settings"]["agent_panes"] is True)
            status, body = _pane(config, action="type", tab=tab, text="again")
            assert status == 200 and not body["result"]["isError"], body
            wait_for(lambda: "again" in _screen(w))
