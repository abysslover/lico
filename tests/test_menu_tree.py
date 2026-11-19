"""M22-P04a: the menus are trees and New session offers three modes. The Session and View dropdowns open with their groups closed (a heading opens and closes in place, by a click and by the keys),
no entry of any menu is lost, and the New session form shows Claude, Codex and Local LLM on one row; choosing Local LLM makes the one look for model servers on this machine at once and lists what it
found, while opening the form and choosing Codex look at nothing. A private Xvfb, synthetic homes, mocked providers, a fake server on a loopback port listed by a process table the test wrote."""

from __future__ import annotations

import time
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.account_helpers import account
from tests.gui_helpers import open_form
from tests.llm_fixtures import FakeServer, openai_models, write_proc
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_new_session_screen import claude_args
from tests.test_running_elsewhere_window import _centre

SESSION_GROUPS = ["Browser pages", "Go to", "This session", "Share", "Tools", "Approvals", "Edit"]
VIEW_GROUPS = ["Find", "Scroll", "Show", "Text size", "Settings"]

# every entry the menus had before the trees, by the start of its label (nothing was removed)
SESSION_ENTRIES = [
    "New Home session", "New browser tab", "New local browser tab", "New terminal tab", "Reopen closed tab", "Clear remembered pages", "Next session", "Previous session", "Start screen", "Resume session", "Rebind account...", "Permission mode:", "Rename session",
    "Close session", "Delete session", "Send to session...", "Hand over conversation...", "Find Local LLM servers...", "Allow approval", "Deny approval", "Copy selected", "Cut selection", "Paste",
    "Virtual display:", "Stop real-screen control",
]
VIEW_ENTRIES = [
    "Working directory files", "Left sidebar:", "Hidden files:", "Find in conversation", "Search all sessions", "Older output", "Newer output", "Newest output", "Group sessions:",
    "Internal events:", "Text size: larger", "Text size: smaller", "Text size: 14 px", "Capture screen to input", "Agent clipboard write:", "Desktop notifications:",
]


def _open(w: ProductWindow, index: int) -> dict:
    w.click(*_centre(w.ui()["menu"]["titles"][index]["rect"]))
    return wait_for(lambda: (m := w.ui()["menu"]) and m["open"] == index and m["items"] and all(b["rect"] for b in m["items"]) and m)


def _close(w: ProductWindow) -> None:
    w.key("Escape")
    wait_for(lambda: w.ui()["menu"]["open"] is None)


def _open_every_group(w: ProductWindow, index: int) -> list[str]:
    """Clicks each closed heading of the open dropdown in turn (the rows move under each click) and returns the labels of every entry then shown."""
    while True:
        m = wait_for(lambda: (x := w.ui()["menu"]) and x["open"] == index and all(b["rect"] for b in x["items"]) and x)
        closed = next((i for i in m["items"] if i["kind"] == "group" and i["open"] is False), None)
        if closed is None:
            return [i["label"] for i in m["items"] if i["kind"] == "entry"]
        w.click(*_centre(closed["rect"]))
        wait_for(lambda: next(i for i in w.ui()["menu"]["items"] if i["kind"] == "group" and i["label"] == closed["label"])["open"] is True)


def _rig(root: Path):
    c_args, _rec = claude_args(root)

    def setup(w: ProductWindow) -> None:
        account(Conn(w.socket), "ClaudeWork", "claude")

    return c_args, setup


def test_the_session_and_view_dropdowns_open_with_closed_groups_and_a_heading_opens_in_place_and_no_entry_is_lost():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        c_args, setup = _rig(root)
        with ProductWindow(host_binary=LICO, host_args=[*c_args, "--log-dir", str(root / "logs")], env_extra={"LANG": "C.UTF-8"}, before_window=setup) as w:
            w.proc.stdin.write("load\n")
            w.proc.stdin.flush()
            wait_for(lambda: (x := w.ui()) and x["toolbar"]["shown"] and x["sidebar"]["shown"] and x)

            # File and Help have no group; Session and View open with their groups closed and the entries that stay on the dropdown
            file_menu = _open(w, 0)
            assert [i["label"] for i in file_menu["items"]] == ["New session", "New session options...", "Accounts", "Quit"] and file_menu["groups"] == [], file_menu["items"]
            _close(w)
            session = _open(w, 1)
            heads = [i["label"] for i in session["items"] if i["kind"] == "group"]
            assert heads == SESSION_GROUPS and [g["label"] for g in session["groups"]] == SESSION_GROUPS, heads
            assert all(i["depth"] == 0 and (i["open"] is False if i["kind"] == "group" else True) for i in session["items"]), session["items"]
            assert len(session["items"]) <= 12, [i["label"] for i in session["items"]]  # was 35 rows
            assert session["items"][0]["label"] == "New Home session" and session["selected"] == 0

            # a click on a heading opens it in place: its entries follow it, indented, and the menu stays open
            share = next(i for i in session["items"] if i["label"] == "Share")
            w.click(*_centre(share["rect"]))
            opened = wait_for(lambda: (m := w.ui()["menu"]) and m["open"] == 1 and any(i["label"] == "Send to session..." for i in m["items"]) and all(b["rect"] for b in m["items"]) and m)
            labels = [i["label"] for i in opened["items"]]
            at = labels.index("Share")
            assert labels[at + 1:at + 3] == ["Send to session...", "Hand over conversation..."], labels
            assert [i["depth"] for i in opened["items"][at:at + 3]] == [0, 1, 1] and opened["items"][at]["open"] is True
            assert opened["items"][at + 1]["group"] == "Share" and opened["items"][at + 1]["rect"][0] >= opened["items"][at]["rect"][0], opened["items"][at:at + 3]
            assert opened["items"][at + 1]["enabled"] is False and opened["items"][at + 1]["reason"], "with no session open the entry says why it is greyed"
            w.click(*_centre(opened["items"][at]["rect"]))
            wait_for(lambda: not any(i["label"] == "Send to session..." for i in w.ui()["menu"]["items"]))
            assert w.ui()["menu"]["open"] == 1
            _close(w)

            # nothing is lost: with every group open each entry the old menus had is there
            _open(w, 1)
            shown = _open_every_group(w, 1)
            assert [p for p in SESSION_ENTRIES if not any(label.startswith(p) for label in shown)] == [], shown
            _close(w)
            view = _open(w, 2)
            assert [i["label"] for i in view["items"] if i["kind"] == "group"] == VIEW_GROUPS, view["items"]
            shown = _open_every_group(w, 2)
            assert [p for p in VIEW_ENTRIES if not any(label.startswith(p) for label in shown)] == [], shown
            _close(w)
            # the dropdown opens closed again
            again = _open(w, 1)
            assert all(i["open"] is False for i in again["items"] if i["kind"] == "group")
            _close(w)


def test_the_keys_open_and_close_a_group_and_run_an_entry_in_it():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        c_args, setup = _rig(root)
        with ProductWindow(host_binary=LICO, host_args=[*c_args, "--log-dir", str(root / "logs")], env_extra={"LANG": "C.UTF-8"}, before_window=setup) as w:
            w.proc.stdin.write("load\n")
            w.proc.stdin.flush()
            wait_for(lambda: (x := w.ui()) and x["toolbar"]["shown"] and x["sidebar"]["shown"] and x)
            w.key("alt+s")
            session = wait_for(lambda: (m := w.ui()["menu"]) and m["open"] == 1 and m["items"] and m)
            tools = next(i for i, row in enumerate(session["items"]) if row["label"] == "Tools")
            for _ in range(32):  # Down moves over the rows that can act (a heading always can) until the selection is on Tools
                if w.ui()["menu"]["selected"] == tools:
                    break
                w.key("Down")
                time.sleep(0.15)
            assert w.ui()["menu"]["selected"] == tools
            w.key("Right")  # Right on a closed heading opens it (it does not go to the next menu)
            opened = wait_for(lambda: (m := w.ui()["menu"]) and m["open"] == 1 and m["items"][tools]["open"] is True and m)
            assert opened["selected"] == tools and opened["items"][tools + 1]["label"] == "Find Local LLM servers...", [i["label"] for i in opened["items"]]
            w.key("Down")
            wait_for(lambda: w.ui()["menu"]["selected"] == tools + 1)
            w.key("Left")  # Left on an entry of an open group closes the group and goes back to its heading
            closed = wait_for(lambda: (m := w.ui()["menu"]) and m["open"] == 1 and m["items"][tools]["open"] is False and m)
            assert closed["selected"] == tools
            w.key("Left")  # Left on a closed heading goes to the menu before (File)
            wait_for(lambda: w.ui()["menu"]["open"] == 0)
            w.key("Right")
            wait_for(lambda: w.ui()["menu"]["open"] == 1)
            for _ in range(32):
                if w.ui()["menu"]["selected"] == tools:
                    break
                w.key("Down")
                time.sleep(0.15)
            w.key("Return")  # Enter on a heading opens it
            wait_for(lambda: (m := w.ui()["menu"]) and m["open"] == 1 and m["items"][tools]["open"] is True)
            w.key("Down")
            wait_for(lambda: w.ui()["menu"]["selected"] == tools + 1)
            w.key("Return")  # Enter on an entry runs it: the dialog that looks for servers opens and the menu closes
            wait_for(lambda: w.ui()["menu"]["open"] is None and (d := w.ui()["sidebar"]["llm_find"]) and d["open"])


def test_new_session_shows_three_modes_and_choosing_local_llm_looks_at_once_and_lists_the_servers():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        server = FakeServer(openai_models(("tiny-llama", "qwen-small")))
        try:
            write_proc(root / "fake_proc", [{"port": server.port, "address": "127.0.0.1", "inode": 10}], {7: ("server-process", [10])})
            c_args, setup = _rig(root)
            with ProductWindow(host_binary=LICO, host_args=[*c_args, "--llm-proc-root", str(root / "fake_proc"), "--log-dir", str(root / "logs")], env_extra={"LANG": "C.UTF-8"},
                               before_window=setup) as w:
                w.proc.stdin.write("load\n")
                w.proc.stdin.flush()
                u = wait_for(lambda: (x := w.ui()) and x["toolbar"]["shown"] and x["sidebar"]["shown"] and x)
                open_form(w)  # M22-P04b: the plus opens the wizard (three icons); the one-page form, with the Provider row, is File > New session options...
                form = wait_for(lambda: (x := w.ui()) and x["panel"] and x["panel"]["title"] == "New session" and x["panel"]["form"] and x)
                row = form["panel"]["body"][0]
                assert "[Claude]" in row and "Codex" in row and "Local LLM" in row and "[Codex]" not in row, row
                time.sleep(0.8)
                assert server.connections == 0, "opening the form looks at nothing"

                # the Provider row is two rows above the folder, where the focus starts: Codex first (nothing looked at), then Local LLM
                w.key("Up")
                w.key("Up")
                wait_for(lambda: w.ui()["panel"]["form"]["focus"] == "Provider")
                w.key("Right")
                codex = wait_for(lambda: (x := w.ui()) and x["panel"]["form"]["provider"] == "codex" and x)
                assert "[Codex]" in codex["panel"]["body"][0]
                time.sleep(0.8)
                assert server.connections == 0, "choosing Codex looks at nothing"
                w.key("Right")
                found = wait_for(lambda: (x := w.ui()) and x["panel"]["form"]["provider"] == "local" and x["panel"]["form"]["scanned"] and x)
                f = found["panel"]["form"]
                assert server.connections > 0, "choosing Local LLM looked at this machine"
                assert len(f["servers"]) == 1 and "openai" in f["servers"][0], f["servers"]
                assert f["local_model"] == "tiny-llama" and f["problem"] is None, f
                body = found["panel"]["body"]
                assert "[Local LLM]" in body[0], body[0]
                head = next(i for i, line in enumerate(body) if line.startswith("Model servers on this machine"))
                assert body[head + 1].startswith("> 127.0.0.1:") and "openai" in body[head + 1], body
                assert any("Found 1 server" in line for line in body), body
        finally:
            server.close()
