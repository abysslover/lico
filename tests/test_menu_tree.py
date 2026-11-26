"""M22-P04a, M24-P04: the menus are trees and New session offers three modes. After the diet of M24-P04 the four menus hold only what the owner kept on show and the menu `More` holds the rest as
groups: it opens with its groups closed (a heading opens and closes in place, by a click and by the keys),
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

MORE = 4
MORE_GROUPS = ["Browser pages", "This session", "Tools", "Show", "Text size", "Settings"]
SESSION_SHOWN = ["New Bot session", "New browser tab", "Permission mode:", "Send to session...", "Hand over conversation..."]

# every entry that is hidden (M24-P04, the owner's classification of 2026-11-24: twenty-one; M24-P05b: twenty-two with the switch of the agents' control of the panes; M25-P09: twenty-three with the start of the answers), by the start of its label
MORE_ENTRIES = [
    "New local browser tab", "Reopen closed tab", "Clear remembered pages", "Start screen", "Resume session", "Rebind account...", "Rename session", "Delete session", "Find Local LLM servers...",
    "Virtual display:", "Group sessions:", "Internal events:", "Text size: larger", "Text size: smaller", "Text size: 14 px", "Capture screen to input", "Agent clipboard write:", "Agent pane control:", "Desktop notifications:", "Start answers folded:",
    "Use existing Claude Code login", "Use existing Codex login", "About Lico",
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


def test_the_four_menus_show_a_few_entries_and_more_opens_with_closed_groups_and_a_heading_opens_in_place_and_no_hidden_entry_is_lost():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        c_args, setup = _rig(root)
        with ProductWindow(host_binary=LICO, host_args=[*c_args, "--log-dir", str(root / "logs")], env_extra={"LANG": "C.UTF-8"}, before_window=setup) as w:
            w.proc.stdin.write("load\n")
            w.proc.stdin.flush()
            wait_for(lambda: (x := w.ui()) and x["toolbar"]["shown"] and x["sidebar"]["shown"] and x)

            # the four menus have no group: File, Session, View and Help show only what was kept on show
            file_menu = _open(w, 0)
            assert [i["label"] for i in file_menu["items"]] == ["New session", "Accounts"] and file_menu["groups"] == [], file_menu["items"]
            _close(w)
            session = _open(w, 1)
            assert [i["label"].split(":")[0] for i in session["items"]] == [p.split(":")[0] for p in SESSION_SHOWN] and session["groups"] == [], [i["label"] for i in session["items"]]
            assert session["items"][0]["label"] == "New Bot session" and session["selected"] == 0
            _close(w)
            assert [i["label"] for i in _open(w, 2)["items"]] == ["Working directory files"]
            _close(w)
            assert [i["label"] for i in _open(w, 3)["items"]] == ["Keys"]
            _close(w)

            # More opens with its groups closed and the entries that stay on the dropdown
            more = _open(w, MORE)
            heads = [i["label"] for i in more["items"] if i["kind"] == "group"]
            assert heads == MORE_GROUPS and [g["label"] for g in more["groups"]] == MORE_GROUPS, heads
            assert all(i["depth"] == 0 and (i["open"] is False if i["kind"] == "group" else True) for i in more["items"]), more["items"]
            assert len(more["items"]) <= 12, [i["label"] for i in more["items"]]

            # a click on a heading opens it in place: its entries follow it, indented, and the menu stays open
            group = next(i for i in more["items"] if i["label"] == "This session")
            w.click(*_centre(group["rect"]))
            opened = wait_for(lambda: (m := w.ui()["menu"]) and m["open"] == MORE and any(i["label"] == "Rename session" for i in m["items"]) and all(b["rect"] for b in m["items"]) and m)
            labels = [i["label"] for i in opened["items"]]
            at = labels.index("This session")
            assert labels[at + 1:at + 5] == ["Resume session", "Rebind account...", "Rename session", "Delete session"], labels
            assert [i["depth"] for i in opened["items"][at:at + 5]] == [0, 1, 1, 1, 1] and opened["items"][at]["open"] is True
            assert opened["items"][at + 1]["group"] == "This session" and opened["items"][at + 1]["rect"][0] >= opened["items"][at]["rect"][0], opened["items"][at:at + 3]
            assert opened["items"][at + 3]["enabled"] is False and opened["items"][at + 3]["reason"], "with no session open the entry says why it is greyed"
            w.click(*_centre(opened["items"][at]["rect"]))
            wait_for(lambda: not any(i["label"] == "Rename session" for i in w.ui()["menu"]["items"]))
            assert w.ui()["menu"]["open"] == MORE
            _close(w)

            # nothing hidden is lost: with every group open each of the twenty-three entries is there
            _open(w, MORE)
            shown = _open_every_group(w, MORE)
            assert [p for p in MORE_ENTRIES if not any(label.startswith(p) for label in shown)] == [], shown
            assert len(shown) == 23, shown
            _close(w)
            # the dropdown opens closed again
            again = _open(w, MORE)
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
            w.key("alt+m")
            session = wait_for(lambda: (m := w.ui()["menu"]) and m["open"] == MORE and m["items"] and m)
            tools = next(i for i, row in enumerate(session["items"]) if row["label"] == "Tools")
            for _ in range(32):  # Down moves over the rows that can act (a heading always can) until the selection is on Tools
                if w.ui()["menu"]["selected"] == tools:
                    break
                w.key("Down")
                time.sleep(0.15)
            assert w.ui()["menu"]["selected"] == tools
            w.key("Right")  # Right on a closed heading opens it (it does not go to the next menu)
            opened = wait_for(lambda: (m := w.ui()["menu"]) and m["open"] == MORE and m["items"][tools]["open"] is True and m)
            assert opened["selected"] == tools and opened["items"][tools + 1]["label"] == "Find Local LLM servers...", [i["label"] for i in opened["items"]]
            w.key("Down")
            wait_for(lambda: w.ui()["menu"]["selected"] == tools + 1)
            w.key("Left")  # Left on an entry of an open group closes the group and goes back to its heading
            closed = wait_for(lambda: (m := w.ui()["menu"]) and m["open"] == MORE and m["items"][tools]["open"] is False and m)
            assert closed["selected"] == tools
            w.key("Left")  # Left on a closed heading goes to the menu before (Help)
            wait_for(lambda: w.ui()["menu"]["open"] == 3)
            w.key("Right")
            wait_for(lambda: w.ui()["menu"]["open"] == MORE)
            for _ in range(32):
                if w.ui()["menu"]["selected"] == tools:
                    break
                w.key("Down")
                time.sleep(0.15)
            w.key("Return")  # Enter on a heading opens it
            wait_for(lambda: (m := w.ui()["menu"]) and m["open"] == MORE and m["items"][tools]["open"] is True)
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
