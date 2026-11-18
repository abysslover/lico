"""M22-P04b: the account icon and the new-session wizard in the real window. The toolbar starts with the account icon (lit while the host says there is no account); the plus and the house are
never greyed, because Local LLM needs no account. The plus opens three icons (Claude, Codex, Local LLM, drawn by the window, the ones with no account grey) and nothing else; each icon goes on to
a step of its own (the account and the folder, or the server; then the options and the start), Esc goes a step back, and the session that starts is the one the one-page form starts for the same
answers (still at File > New session options...). A private Xvfb, synthetic homes, mocked providers, a fake server on a loopback port listed by a process table the test wrote."""

from __future__ import annotations

import time
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.account_helpers import account
from tests.gui_helpers import centre, menu_click
from tests.llm_fixtures import FakeServer, openai_models, write_proc
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_new_session_screen import claude_args


def _rig(root: Path, accounts: list[tuple[str, str]], extra: tuple[str, ...] = ()):
    """What a window needs: the host's arguments, the ids of the accounts made before the window starts (by label), the file the mocked Claude records into."""
    c_args, received = claude_args(root)
    made: dict[str, str] = {}

    def setup(w: ProductWindow) -> None:
        for label, provider in accounts:
            made[label] = account(Conn(w.socket), label, provider)

    return dict(host_binary=LICO, host_args=[*c_args, *extra, "--log-dir", str(root / "logs")], env_extra={"LANG": "C.UTF-8"}, before_window=setup), made, received


def _ready(w: ProductWindow) -> dict:
    w.proc.stdin.write("load\n")
    w.proc.stdin.flush()
    return wait_for(lambda: (x := w.ui()) and x["toolbar"]["shown"] and x["sidebar"]["shown"] and x["toolbar"]["buttons"][0]["rect"] and x)


def _button(w: ProductWindow, which: str) -> dict:
    return next(b for b in w.ui()["toolbar"]["buttons"] if b["id"] == which)


def _panel(w: ProductWindow, until=lambda p: True) -> dict:
    return wait_for(lambda: (x := w.ui()) and x["panel"] and until(x["panel"]) and x["panel"])


def _step(w: ProductWindow, page: int) -> dict:
    """The panel once the wizard is on step `page` (0, 1 or 2) and, on the first step, the three icons have been drawn."""
    return _panel(w, lambda p: p["form"] and p["form"]["wizard"] and p["form"]["wizard"]["page"] == page and (page > 0 or all(t["rect"] for t in p["form"]["tiles"])))


def _gray(w: ProductWindow, rect: list[float], inset: int = 4) -> int:
    """The grey level of the fill of a box, read inside its edge (the ring of the focus is outside it)."""
    data, width, _ = w.gray()
    return data[int(rect[1] + inset) * width + int(rect[0] + inset)]


def _ink(w: ProductWindow, rect: list[float], threshold: int) -> int:
    return w.bright(int(rect[0]), int(rect[1]), int(rect[2]), int(rect[3]), threshold)


def test_the_account_icon_leads_the_toolbar_it_is_lit_while_there_is_no_account_and_the_plus_and_the_house_are_never_greyed():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        kwargs, _made, _received = _rig(root, [])
        with ProductWindow(**kwargs) as w:
            _ready(w)
            buttons = wait_for(lambda: (b := w.ui()["toolbar"]["buttons"]) and b[0]["active"] and b)  # the host has answered: there is no account
            assert [b["id"] for b in buttons] == ["accounts", "new_session", "new_home", "new_browser"], [b["id"] for b in buttons]
            assert [b["icon"] for b in buttons] == ["account", "plus", "house", "browser"]
            assert buttons[0]["label"].startswith("Accounts: none yet") and [b["active"] for b in buttons] == [True, False, False, False]
            assert all(b["enabled"] and b["reason"] is None and b["rect"] for b in buttons), "the plus and the house are not greyed: Local LLM needs no account"
            assert all(a["rect"][2] <= b["rect"][0] for a, b in zip(buttons, buttons[1:])), "left to right, the account first"
            # drawn: the lit icon's box is not the colour of the one beside it
            assert _gray(w, buttons[0]["rect"], 2) > _gray(w, buttons[1]["rect"], 2), "the account icon is lit"

            # the icon opens the Accounts screen
            w.click(*centre(buttons[0]["rect"]))
            _panel(w, lambda p: p["title"] == "Accounts")
            w.key("Escape")
            wait_for(lambda: w.ui()["panel"] is None)

            # an account was made: the icon is not lit any more and says Accounts
            account(Conn(w.socket), "ClaudeWork", "claude")
            w.proc.stdin.write("load\n")
            w.proc.stdin.flush()
            after = wait_for(lambda: (b := w.ui()["toolbar"]["buttons"]) and b[0]["active"] is False and b)
            assert after[0]["label"] == "Accounts" and all(b["enabled"] for b in after), after


def test_the_plus_shows_three_icons_one_is_grey_without_an_account_and_each_step_asks_one_thing_until_the_session_starts():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        project = root.resolve() / "project"
        project.mkdir()
        kwargs, made, _received = _rig(root, [("Work", "claude")])
        with ProductWindow(**kwargs) as w:
            conn = Conn(w.socket)
            _ready(w)
            w.click(*centre(_button(w, "new_session")["rect"]))

            # step one: three icons and nothing else; Codex has no account here, so it is grey, and Local LLM never is
            first = _step(w, 0)
            form = first["form"]
            assert first["title"] == "New session - step 1 of 3 - Choose" and first["text_entry"] is False
            assert [t["provider"] for t in form["tiles"]] == ["claude", "codex", "local"] and [t["enabled"] for t in form["tiles"]] == [True, False, True]
            assert [t["icon"] for t in form["tiles"]] == ["claude", "codex", "local"] and [t["word"] for t in form["tiles"]] == ["Claude", "Codex", "Local LLM"]
            said = "\n".join(first["body"])
            assert not any(word in said for word in ("Provider", "Account", "Folder", "Name", "Model", "Effort", "Start session")), first["body"]
            assert "Codex needs an account first" in said, first["body"]
            rects = [t["rect"] for t in form["tiles"]]
            assert len({(round(r[2] - r[0]), round(r[3] - r[1])) for r in rects}) == 1 and rects[0][2] - rects[0][0] >= 40 and all(a[2] < b[0] for a, b in zip(rects, rects[1:])), rects
            claude, codex, local = rects
            assert _ink(w, claude, 180) > 150 and _ink(w, local, 180) > 150, "the icons are drawn"
            assert _ink(w, codex, 180) == 0 and _ink(w, codex, 90) > 100, "the icon with no account is drawn dim"
            assert form["tiles"][0]["focus"] is True and _gray(w, claude) > _gray(w, local) > _gray(w, codex), "the focus is lit, an icon that can be chosen is not, a grey one is darker still"

            # the pointer lights an icon, and the word under the icons is the one the pointer is on
            w.move(*centre(local))
            hovered = _step(w, 0)["form"]["tiles"]
            assert [t["hover"] for t in hovered] == [False, False, True] and _gray(w, local) == _gray(w, claude), hovered
            assert "Local LLM" in _step(w, 0)["body"], "the word of the icon the pointer is on"

            # a click on a grey icon takes nothing: it says why
            w.click(*centre(codex))
            grey = _step(w, 0)
            assert grey["form"]["focus"] == "Pick(1)" and any("Codex needs an account" in line for line in grey["body"]) and grey["form"]["wizard"]["page"] == 0

            # Esc on the first step closes the screen; the keys do what they say on the icons: Right and Left move, Enter chooses
            w.key("Escape")
            wait_for(lambda: w.ui()["panel"] is None)
            w.key("ctrl+n")
            assert _step(w, 0)["form"]["focus"] == "Pick(0)"
            w.key("Right")
            _panel(w, lambda p: p["form"]["focus"] == "Pick(1)")
            w.key("Left")
            _panel(w, lambda p: p["form"]["focus"] == "Pick(0)")
            w.key("Return")

            # step two: the account and the folder (what the form's rows say), Back and Next
            second = _step(w, 1)
            assert second["title"] == "New session - step 2 of 3 - Account and folder" and second["text_entry"] and second["form"]["focus"] == "Folder"
            assert (second["form"]["provider"], second["form"]["account"]) == ("claude", made["Work"])
            assert [line.lstrip("> ").split()[0] for line in second["body"][:5] if line.strip()] == ["Account", "Folder", "Sign", "Back", "Next"], second["body"]
            assert not any(line.lstrip("> ").startswith(("Name", "Model", "Effort", "Permission")) for line in second["body"]), "the options are the next step"
            w.type_text(str(project))
            w.key("Return")  # Enter is Next

            # step three: the options and the start; Esc goes back with what was typed kept, and the name is asked here
            third = _step(w, 2)
            assert third["title"] == "New session - step 3 of 3 - Options and start" and third["form"]["focus"] == "Name" and third["form"]["folder"] == str(project)
            assert any(line.lstrip("> ").startswith("Start session") for line in third["body"]) and any(line.lstrip("> ").startswith("Back") for line in third["body"])
            w.key("Escape")
            back = _step(w, 1)
            assert back["input"] == str(project) and back["form"]["focus"] == "Folder", "the folder is on the input line again"
            w.key("Escape")
            _step(w, 0)
            assert conn.call(op="status")["sessions"] == [], "choosing creates nothing"
            w.key("Return")
            _step(w, 1)
            w.key("Return")
            _step(w, 2)
            w.type_text("api")
            w.key("Return")
            state = wait_for(lambda: (u := w.ui())["panel"] is None and len(u["rows"]) == 1 and u)
            entry = conn.call(op="status")["sessions"][0]
            assert (entry["cwd"], entry["name"], entry["account"], entry["provider"]) == (str(project), "api", made["Work"], "claude"), entry
            assert state["selected"] == entry["session"]

            # the one-page form is still there, with every row, in the File menu
            menu_click(w, 0, "New session options")
            plain = _panel(w, lambda p: p["form"] is not None)
            assert plain["title"] == "New session" and plain["form"]["wizard"] is None and plain["form"]["tiles"] is None, plain["title"]
            rows = [line.lstrip("> ").split()[0] for line in plain["body"] if line.strip()]
            assert [r for r in rows if r != "recent"][:4] == ["Provider", "Account", "Folder", "Name"], plain["body"]  # a recent folder (the one just used) is a row of its own
            w.key("Escape")
            wait_for(lambda: w.ui()["panel"] is None)


def test_local_llm_needs_no_account_and_looks_at_this_machine_as_soon_as_its_icon_is_chosen():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        server = FakeServer(openai_models(("tiny-llama", "qwen-small")))
        try:
            write_proc(root / "fake_proc", [{"port": server.port, "address": "127.0.0.1", "inode": 10}], {7: ("server-process", [10])})
            kwargs, _made, _received = _rig(root, [], ("--llm-proc-root", str(root / "fake_proc")))
            with ProductWindow(**kwargs) as w:
                conn = Conn(w.socket)
                _ready(w)
                w.click(*centre(_button(w, "new_session")["rect"]))
                first = _step(w, 0)
                assert [t["enabled"] for t in first["form"]["tiles"]] == [False, False, True], "no account: Claude and Codex are grey, Local LLM is not"
                assert first["form"]["focus"] == "Pick(2)", "and it has the focus"
                assert any("Claude and Codex need an account first" in line for line in first["body"]), first["body"]
                time.sleep(0.8)
                assert server.connections == 0, "the icons look at nothing"

                w.click(*centre(first["form"]["tiles"][2]["rect"]))
                second = _step(w, 1)
                wait_for(lambda: w.ui()["panel"]["form"]["scanned"])
                form = w.ui()["panel"]["form"]
                assert server.connections > 0, "choosing the Local LLM icon looked at this machine"
                assert second["title"] == "New session - step 2 of 3 - Local LLM server" and len(form["servers"]) == 1 and "openai" in form["servers"][0], form["servers"]
                assert form["focus"] == "Server" and form["local_model"] == "tiny-llama" and form["problem"] is None, form
                body = w.ui()["panel"]["body"]
                assert any(line.startswith("> 127.0.0.1:") and "openai" in line for line in body) and any("Found 1 server" in line for line in body), body

                w.key("Return")  # Next
                third = _step(w, 2)
                assert third["title"] == "New session - step 3 of 3 - Options and start" and not any(line.lstrip("> ").startswith(("Folder", "Account")) for line in third["body"]), third["body"]
                w.key("Return")  # Enter on the name row starts
                wait_for(lambda: conn.call(op="status")["sessions"])
                entry = conn.call(op="status")["sessions"][0]
                assert entry["provider"] == "local", entry
        finally:
            server.close()


def test_the_house_opens_the_same_wizard_for_a_home_session_with_no_folder_and_starts_one():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        kwargs, made, _received = _rig(root, [("Work", "claude")])
        with ProductWindow(**kwargs) as w:
            conn = Conn(w.socket)
            _ready(w)
            w.click(*centre(_button(w, "new_home")["rect"]))
            first = _step(w, 0)
            assert first["title"] == "New Home session - step 1 of 3 - Choose" and first["form"]["wizard"]["home"] is True
            w.click(*centre(first["form"]["tiles"][0]["rect"]))
            second = _step(w, 1)
            assert second["title"] == "New Home session - step 2 of 3 - Account" and second["form"]["focus"] == "Account" and second["text_entry"] is False, second["title"]
            assert not any(line.lstrip("> ").startswith(("Folder", "Recent")) for line in second["body"]), second["body"]
            assert any("Home session" in line and "scratch workspace" in line for line in second["body"]), second["body"]
            w.key("Return")  # Next
            _step(w, 2)
            w.key("Return")  # Enter on the name row starts
            home = wait_for(lambda: (x := w.ui()) and x["panel"] is None and len(x["tabs"]["items"]) == 1 and x["telemetry"]["shown"] and x)
            tab = home["tabs"]["items"][0]
            assert tab["global"] is True and tab["label"].startswith("H·"), tab
            entry = conn.call(op="status")["sessions"][0]
            assert entry["account"] == made["Work"] and entry["provider"] == "claude", entry
