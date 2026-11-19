"""M23-P03: the terminal tab in the window, against a real host, a real shell on a real pty and a real window on a private Xvfb. The shell is `/bin/sh` with a prompt of its own (`lico$ `),
so the screen the window shows can be read from the window's dump (`terminal.lines`, the plain text of the grid). A screenshot of each state is kept for the owner under the folder named by
`LICO_P03_SCREENS`. What the Rust tests prove inside the window (the keys, the queue, the watch generation) is not repeated; here the doors are: the menu entry and the wizard open a tab, a key
typed in the window reaches the shell and what the shell prints comes back to the grid, the grid is the size of the pty, a selection is copied and a paste is typed, the history scrolls, and a
tab closes by its cross, by its key and by the shell's end."""

from __future__ import annotations

import os
import subprocess
import time
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.gui_helpers import centre, menu_click_when_enabled
from tests.test_codex_adapter import usual_umask, wait_for

SCREENS = Path(os.environ["LICO_P03_SCREENS"]) if os.environ.get("LICO_P03_SCREENS") else None


def shot(w: ProductWindow, name: str) -> None:
    if SCREENS:
        SCREENS.mkdir(parents=True, exist_ok=True)
        w.png(SCREENS / name)


def rig(root: Path, **more) -> dict:
    """The keywords of a window whose host runs `/bin/sh` with the prompt `lico$ ` (the shell reads the file named by `ENV`, the prompt is the one place the test does not leave to the person's rc files)."""
    rc = root / "rc"
    rc.write_text("PS1='lico$ '\nPS2='> '\n")
    return dict(host_binary=LICO, env_extra={"SHELL": "/bin/sh", "ENV": str(rc), "LANG": "C.UTF-8", **more.pop("env", {})}, **more)


def term(w: ProductWindow) -> dict:
    return w.ui()["terminal"]


def text(w: ProductWindow) -> str:
    return "\n".join(term(w)["lines"] or [])


def tab(w: ProductWindow, tab_id: str = "t1") -> dict:
    return next(t for t in term(w)["tabs"] if t["id"] == tab_id)


def open_by_menu(w: ProductWindow) -> dict:
    """File, Session, View, Help: the Session menu's `New terminal tab` opens a terminal and shows it; returns the state once the shell has printed its prompt."""
    menu_click_when_enabled(w, 1, "New terminal tab")
    return wait_for(lambda: (t := term(w)) and t["shown"] and "lico$" in "\n".join(t["lines"] or []) and t)


def run(w: ProductWindow, line: str, expect: str, timeout: float = 10.0) -> str:
    """Types a command line and waits until the screen says `expect`; returns the screen."""
    w.type_text(line)
    w.key("Return")
    return wait_for(lambda: (s := text(w)) and expect in s and s, timeout)


def cell_point(w: ProductWindow, row: int, col: int) -> tuple[float, float]:
    """The middle of the cell (row, col) of the grid, in window pixels."""
    x, y, _, _ = term(w)["rect"]
    cw, ch = term(w)["cell"]
    return x + (col + 0.5) * cw, y + (row + 0.5) * ch


def drag(w: ProductWindow, start: tuple[float, float], end: tuple[float, float]) -> None:
    wid = w.window_id()
    since = w.frames()
    subprocess.run(["xdotool", "mousemove", "--window", wid, str(int(start[0])), str(int(start[1])), "mousedown", "1", "mousemove", "--window", wid, str(int(end[0] - 8)), str(int(end[1])),
                    "mousemove", "--window", wid, str(int(end[0])), str(int(end[1])), "mouseup", "1"], env=w.env, check=True)
    w.settle(since, 0.3)


def clipboard(w: ProductWindow) -> str:
    return subprocess.run(["xclip", "-selection", "clipboard", "-o"], env=w.env, capture_output=True, text=True, check=True).stdout


def set_clipboard(w: ProductWindow, value: str) -> None:
    subprocess.run(["xclip", "-selection", "clipboard"], env=w.env, input=value, text=True, check=True, timeout=5)


def test_the_session_menu_opens_a_terminal_whose_shell_answers_what_is_typed_and_whose_grid_is_the_size_of_the_pty():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        work = root.resolve() / "work"
        work.mkdir()

        def before(w: ProductWindow) -> None:
            assert Conn(w.socket).call(op="settings.set", last_cwd=str(work), last_view="start")["ok"] == 1

        with ProductWindow(**rig(root, before_window=before)) as w:
            conn = Conn(w.socket)
            t = open_by_menu(w)
            home = str(work)                                       # the folder Directories shows (M23-P03c): the terminal starts there, not at the home
            assert t["active"] == "t1" and t["listed"] and [x["id"] for x in t["tabs"]] == ["t1"], t
            assert t["header"] == f"terminal | shell | {home} | not fenced: it runs as you", t["header"]
            assert t["status"].startswith("Ctrl+Shift+C copy") and t["status"].endswith("Ctrl+Shift+W close") and len(t["status"]) < 100, t["status"]   # one row of the default window: it does not wrap
            assert t["tabs"][0]["caption"] == ">_ work" and t["tabs"][0]["running"] and t["tabs"][0]["cwd"] == home, t["tabs"][0]
            shot(w, "1_terminal_opened.png")

            # the shell is the person's, started by the host in the folder it was asked for; what is typed comes back from the shell, never from the window
            assert "42" in run(w, "echo hi $((6*7))", "hi 42")
            assert home in run(w, "pwd", Path(home).name + "\n").replace("\n", ""), "the shell starts in the folder the host was asked for"
            rows = conn.call(op="terminal.list")["terminals"]
            assert [r["tab"] for r in rows] == ["t1"] and rows[0]["cwd"] == home, rows

            # the grid is the size of the pty: `stty size` says rows and columns, and the host's own list says the same
            t = term(w)
            cols, grid_rows = t["grid"]
            screen = run(w, "stty size", f"{grid_rows} {cols}")
            assert f"{grid_rows} {cols}" in screen, screen
            assert (rows := conn.call(op="terminal.list")["terminals"])[0]["cols"] == cols and rows[0]["rows"] == grid_rows, rows
            assert cols >= 40 and grid_rows >= 10, "the grid is whole cells of the area"
            shot(w, "2_typed_and_stty.png")

            # the window is made smaller: the grid follows it, the host is told, and the shell's own idea of its size changes with it (the pty's size, the hang-up-free `SIGWINCH` path)
            subprocess.run(["xdotool", "windowsize", w.window_id(), "800", "520"], env=w.env, check=True)
            small = wait_for(lambda: (t := term(w)) and t["grid"][0] < cols and t["grid"][1] < grid_rows and t)
            new_cols, new_rows = small["grid"]
            wait_for(lambda: tab(w)["cols"] == new_cols and tab(w)["rows"] == new_rows)
            screen = run(w, "stty size", f"{new_rows} {new_cols}")
            assert (rows := conn.call(op="terminal.list")["terminals"])[0]["cols"] == new_cols and rows[0]["rows"] == new_rows, rows
            shot(w, "2b_window_made_smaller.png")

            # the keys of the shell are the shell's: Ctrl+C ends what runs, and what the person did not type never reaches it
            w.type_text("sleep 30")
            w.key("Return")
            time.sleep(0.4)
            w.key("ctrl+c")
            wait_for(lambda: text(w).rstrip().endswith("lico$"))
            assert wait_for(lambda: term(w)["queued"] == 0 and not term(w)["inflight"] and term(w)), "nothing waits to be sent"


def test_the_grid_follows_the_application_cursor_keys_the_bracketed_paste_and_the_wide_text():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        with ProductWindow(**rig(root)) as w:
            open_by_menu(w)
            # a program that turned on the application cursor keys gets `ESC O A` for Up, not `ESC [ A`; `cat -v` shows the bytes it reads
            w.type_text(r"printf '\033[?1h'; cat -v")
            w.key("Return")
            wait_for(lambda: tab(w)["modes"]["app_cursor"])
            w.key("Up")
            wait_for(lambda: "^[OA" in text(w))
            w.key("ctrl+c")
            wait_for(lambda: text(w).rstrip().endswith("lico$"))
            w.type_text(r"printf '\033[?1l'")
            w.key("Return")
            wait_for(lambda: not tab(w)["modes"]["app_cursor"])

            # a program that asked for the bracketed paste gets the paste between the two marks and the lines of the paste are not Enter
            w.type_text(r"printf '\033[?2004h'; cat -v")
            w.key("Return")
            wait_for(lambda: tab(w)["modes"]["bracketed_paste"])
            set_clipboard(w, "one\ntwo")
            w.key("ctrl+shift+v")
            screen = wait_for(lambda: (s := text(w)) and "^[[200~one" in s and "two^[[201~" in s and s)
            assert "^[[200~one" in screen, screen
            w.key("ctrl+c")
            shot(w, "3_application_keys_and_paste.png")

            # wide text: a Hangul word is two cells a letter and comes back whole
            w.type_text("echo ")
            set_clipboard(w, "안녕 한글")
            w.key("ctrl+shift+v")
            w.key("Return")
            wait_for(lambda: any(line.strip() == "안녕 한글" for line in text(w).splitlines()))
            shot(w, "4_wide_text.png")


def test_a_selection_is_copied_with_ctrl_shift_c_a_paste_is_typed_and_the_history_scrolls_with_shift_page_up():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        with ProductWindow(**rig(root)) as w:
            open_by_menu(w)
            screen = run(w, "echo alpha beta gamma", "alpha beta gamma\n")
            row = next(i for i, line in enumerate(screen.splitlines()) if line.strip() == "alpha beta gamma")
            col = screen.splitlines()[row].index("alpha")
            # drag over `alpha beta`: the selection is highlighted (the window's dump says its anchor and focus) and Ctrl+Shift+C puts the selected text on the clipboard
            drag(w, cell_point(w, row, col), cell_point(w, row, col + len("alpha beta") - 1))
            sel = wait_for(lambda: term(w)["selection"] and term(w))["selection"]
            assert sel["point"] is False, sel
            shot(w, "5_selection.png")
            set_clipboard(w, "before")
            w.key("ctrl+shift+c")
            wait_for(lambda: clipboard(w) == "alpha beta")
            # the same words dragged from the right to the left copy the same text: the ends are put in order before the text is asked for
            set_clipboard(w, "before again")
            drag(w, cell_point(w, row, col + len("alpha beta") - 1), cell_point(w, row, col))
            wait_for(lambda: (s := term(w)["selection"]) and s["point"] is False)
            w.key("ctrl+shift+c")
            wait_for(lambda: clipboard(w) == "alpha beta")
            # a click with no drag clears the selection and sends nothing to the shell
            w.click(*cell_point(w, row, col))
            wait_for(lambda: not term(w)["selection"] or term(w)["selection"]["point"])

            # a paste is typed into the shell as text, and the line is not run until the person presses Enter
            set_clipboard(w, "echo pasted")
            w.key("ctrl+shift+v")
            wait_for(lambda: text(w).rstrip().endswith("lico$ echo pasted"))
            w.key("Return")
            wait_for(lambda: text(w).rstrip().splitlines()[-2:-1] == ["pasted"] or "pasted\nlico$" in text(w))

            # the history: two hundred lines are more than the grid; Shift+PgUp moves back, the status row says how far, typing comes back to the bottom
            run(w, "seq 1 200", "200\n")
            bottom = term(w)
            assert bottom["tabs"][0]["scroll"] == 0 and (bottom["tabs"][0]["total"] or 0) > bottom["grid"][1]
            w.key("shift+Prior")
            hist = wait_for(lambda: (t := term(w)) and t["tabs"][0]["scroll"] > 0 and t)
            assert hist["status"].startswith("history: ") and "Shift+End returns" in hist["status"], hist["status"]
            assert "200" not in "\n".join(hist["lines"][:1]) and hist["lines"] != bottom["lines"], "an older part of the output is shown"
            shot(w, "6_history.png")
            w.click(*cell_point(w, 2, 2), button=4)            # a wheel step up goes further back
            wait_for(lambda: term(w)["tabs"][0]["scroll"] > hist["tabs"][0]["scroll"])
            w.key("shift+End")
            wait_for(lambda: term(w)["tabs"][0]["scroll"] == 0 and term(w)["status"].startswith("Ctrl+Shift+C copy"))
            w.key("shift+Prior")
            wait_for(lambda: term(w)["tabs"][0]["scroll"] > 0)
            w.type_text("x")                                    # typing returns to the bottom, and the key is the shell's
            wait_for(lambda: term(w)["tabs"][0]["scroll"] == 0 and text(w).rstrip().endswith("lico$ x"))


def test_a_terminal_tab_closes_by_its_cross_by_ctrl_shift_w_and_by_the_end_of_its_shell_and_the_tabs_switch_with_the_page():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        with ProductWindow(**rig(root)) as w:
            conn = Conn(w.socket)
            open_by_menu(w)
            run(w, "echo first-tab", "first-tab\n")
            # a second tab from the menu: both are in the row, the new one is shown, and the first one keeps its screen on the host
            menu_click_when_enabled(w, 1, "New terminal tab")
            two = wait_for(lambda: (t := term(w)) and t["active"] == "t2" and "lico$" in "\n".join(t["lines"] or []) and t)
            assert [x["id"] for x in two["tabs"]] == ["t1", "t2"] and "first-tab" not in text(w)
            assert all(x["rect"] and x["close_rect"] for x in two["tabs"]), two["tabs"]
            shot(w, "7_two_tabs.png")
            w.click(*centre(two["tabs"][0]["rect"]))
            first = wait_for(lambda: (t := term(w)) and t["active"] == "t1" and "first-tab" in "\n".join(t["lines"] or []) and t)
            assert first["shown"], first
            w.click(*centre(first["tabs"][1]["rect"]))
            wait_for(lambda: term(w)["active"] == "t2" and "first-tab" not in text(w))

            # the cross closes the tab and its shell: the host's list loses it
            w.click(*centre(term(w)["tabs"][1]["close_rect"]))
            wait_for(lambda: [x["id"] for x in term(w)["tabs"]] == ["t1"])
            wait_for(lambda: [r["tab"] for r in conn.call(op="terminal.list")["terminals"]] == ["t1"])

            # the first tab is shown again (a page of its own kind is always one of them), Ctrl+Shift+W closes it
            wait_for(lambda: term(w)["active"] == "t1" or term(w)["active"] is None)
            w.click(*centre(term(w)["tabs"][0]["rect"]))
            wait_for(lambda: term(w)["shown"])
            w.key("ctrl+shift+w")
            wait_for(lambda: term(w)["tabs"] == [])
            assert conn.call(op="terminal.list")["terminals"] == [], "the shell ended with its tab"

            # a shell that ends takes its tab with it, and the host's terminal with it (the host keeps an ended one until it is closed): with `exit` nothing is said,
            # with a failure the header's line says how it ended
            open_by_menu(w)
            w.type_text("exit")
            w.key("Return")
            wait_for(lambda: term(w)["tabs"] == [] and not term(w)["shown"])
            wait_for(lambda: conn.call(op="terminal.list")["terminals"] == [])
            assert "shell ended" not in (w.ui()["header"] or ""), w.ui()["header"]
            open_by_menu(w)
            w.type_text("exit 3")
            w.key("Return")
            wait_for(lambda: term(w)["tabs"] == [] and not term(w)["shown"])
            wait_for(lambda: conn.call(op="terminal.list")["terminals"] == [])
            wait_for(lambda: "The terminal's shell ended (exit 3)." in (w.ui()["header"] or ""))
            shot(w, "8_shell_ended_tab_gone.png")


def test_a_terminal_that_the_host_already_has_is_adopted_by_the_window_and_the_browser_and_the_terminal_do_not_share_the_page():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)

        def before(w: ProductWindow) -> None:
            # a terminal opened on the socket before the window exists, as `lico terminal open` does
            work = root / "work"
            work.mkdir()
            reply = Conn(w.socket).call(op="terminal.open", cwd=str(work), cols=80, rows=24)
            assert reply["tab"] == "t1", reply
            Conn(w.socket).call(op="terminal.input", tab="t1", text="echo adopted-from-the-host\n")

        with ProductWindow(**rig(root, before_window=before)) as w:
            adopted = wait_for(lambda: (t := term(w)) and t["listed"] and [x["id"] for x in t["tabs"]] == ["t1"] and t)
            assert adopted["shown"] is False and adopted["active"] is None, "adopting adds the tab to the row and shows nothing"
            assert adopted["tabs"][0]["cwd"] == str(root / "work")
            w.click(*centre(wait_for(lambda: term(w)["tabs"][0]["rect"] and term(w))["tabs"][0]["rect"]))
            shown = wait_for(lambda: (t := term(w)) and t["shown"] and "adopted-from-the-host" in "\n".join(t["lines"] or []) and t)
            assert shown["header"].startswith("terminal | ") and str(root / "work") in shown["header"], shown["header"]
            shot(w, "9_adopted.png")

            # the browser's prompt takes the page: the terminal is not shown, keeps its tab and its screen, and the tab brings it back
            w.click(*centre(next(b for b in w.ui()["toolbar"]["buttons"] if b["id"] == "new_browser")["rect"]))
            wait_for(lambda: not term(w)["shown"] and w.ui()["browser"]["shown"])
            assert [x["id"] for x in term(w)["tabs"]] == ["t1"] and term(w)["active"] is None
            w.click(*centre(term(w)["tabs"][0]["rect"]))
            back = wait_for(lambda: (t := term(w)) and t["shown"] and "adopted-from-the-host" in "\n".join(t["lines"] or []) and t)
            assert not w.ui()["browser"]["shown"], "one page at a time"
            assert back["tabs"][0]["seq"] >= shown["tabs"][0]["seq"]


def test_a_shell_that_ended_while_no_window_looked_is_closed_in_the_host_and_gets_no_tab():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)

        def before(w: ProductWindow) -> None:
            conn = Conn(w.socket)
            assert conn.call(op="terminal.open", cols=80, rows=24)["tab"] == "t1"
            conn.call(op="terminal.input", tab="t1", text="exit\n")
            wait_for(lambda: [r["running"] for r in conn.call(op="terminal.list")["terminals"]] == [False])      # the host keeps it, ended, until somebody closes it

        with ProductWindow(**rig(root, before_window=before)) as w:
            wait_for(lambda: (t := term(w)) and t["listed"])
            wait_for(lambda: Conn(w.socket).call(op="terminal.list")["terminals"] == [])      # the window gave the host's place back
            assert term(w)["tabs"] == [] and not term(w)["shown"], "an ended shell is not adopted"


def new_session_button(w: ProductWindow) -> None:
    """The plus of the toolbar (Ctrl+N is the shell's while a terminal is shown)."""
    w.proc.stdin.write("load\n")
    w.proc.stdin.flush()
    wait_for(lambda: (x := w.ui()) and x["toolbar"]["shown"] and x["toolbar"]["buttons"][0]["rect"])
    w.click(*centre(next(b for b in w.ui()["toolbar"]["buttons"] if b["id"] == "new_session")["rect"]))


def test_the_fourth_icon_opens_a_terminal_at_once_in_the_current_folder_with_no_step():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        project = root.resolve() / "project"
        project.mkdir()

        def before(w: ProductWindow) -> None:
            assert Conn(w.socket).call(op="settings.set", last_cwd=str(project), last_view="files")["ok"] == 1

        with ProductWindow(**rig(root, before_window=before)) as w:
            new_session_button(w)
            first = wait_for(lambda: (p := w.ui()["panel"]) and p["form"] and p["form"]["tiles"] and all(t["rect"] for t in p["form"]["tiles"]) and p)
            tiles = first["form"]["tiles"]
            assert [t["provider"] for t in tiles] == ["claude", "agy", "codex", "local", "terminal"] and tiles[4]["enabled"], tiles
            w.click(*centre(tiles[4]["rect"]))
            started = wait_for(lambda: (u := w.ui()) and u["panel"] is None and u["terminal"]["shown"] and u["terminal"]["header"] and "lico$" in "\n".join(u["terminal"]["lines"] or []) and u["terminal"])
            assert str(project) in started["header"] and started["tabs"][0]["cwd"] == str(project), "the folder Directories is on: " + started["header"]
            assert str(project) in run(w, "pwd", str(project) + "\n")
            shot(w, "10_terminal_from_the_icon.png")

    # the kept folder is gone: Directories falls back to a folder that exists, and the terminal starts where Directories is (M23-P03c), not a refusal
    with usual_umask(), private_dir() as directory:
        root = Path(directory)

        def before(w: ProductWindow) -> None:
            gone = root / "gone"
            gone.mkdir()
            assert Conn(w.socket).call(op="settings.set", last_cwd=str(gone), last_view="files")["ok"] == 1      # the host keeps only a folder that exists
            gone.rmdir()                                                                                          # and it is gone when the window starts

        with ProductWindow(**rig(root, before_window=before)) as w:
            wait_for(lambda: (s := w.ui()["sidebar"]) and s["cwd"] and s["loading"] is None and s["load_count"] > 0)
            new_session_button(w)
            wait_for(lambda: (p := w.ui()["panel"]) and p["form"] and p["form"]["tiles"] and all(t["rect"] for t in p["form"]["tiles"]))
            w.click(*centre(w.ui()["panel"]["form"]["tiles"][4]["rect"]))
            started = wait_for(lambda: (u := w.ui()) and u["panel"] is None and u["terminal"]["shown"] and u["terminal"]["tabs"] and u["terminal"])
            shown = w.ui()["sidebar"]["cwd"]
            assert shown and Path(shown).is_dir() and started["tabs"][0]["cwd"] == shown, (shown, started["tabs"])


def terminal_from_the_icon(w: ProductWindow) -> dict:
    """The toolbar's New session icon and then the Terminal picture: the shell is opened at once; returns the terminal's state when it is shown."""
    new_session_button(w)
    wait_for(lambda: (p := w.ui()["panel"]) and p["form"] and p["form"]["tiles"] and all(t["rect"] for t in p["form"]["tiles"]))
    w.click(*centre(w.ui()["panel"]["form"]["tiles"][4]["rect"]))
    return wait_for(lambda: (u := w.ui()) and u["panel"] is None and u["terminal"]["shown"] and u["terminal"]["tabs"] and u["terminal"])


def test_a_terminal_starts_in_the_folder_directories_shows_also_when_the_person_moved_there_and_nothing_was_kept():
    """The owner's report (2026-11-19): a terminal always opened at home. What the window keeps as the last folder (`last_cwd`) follows a session or the Files screen only, so a person who
    only moves in Directories left it empty or old, and the start folder fell back to the home. The folder Directories shows decides, in both of these cases."""
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        with ProductWindow(**rig(root)) as w:                       # no folder was kept: the sidebar starts where the host puts it
            shown = wait_for(lambda: (s := w.ui()["sidebar"]) and s["cwd"] and s["loading"] is None and s["load_count"] > 0 and s["cwd"])
            home = str(Path(w.directory) / "home")
            assert shown != home, "the test needs a folder that is not the home"
            started = terminal_from_the_icon(w)
            assert started["tabs"][0]["cwd"] == shown, (shown, started["tabs"])

    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        project = root.resolve() / "project"
        (project / "inner").mkdir(parents=True)

        def before(w: ProductWindow) -> None:
            assert Conn(w.socket).call(op="settings.set", last_cwd=str(project), last_view="start")["ok"] == 1

        with ProductWindow(**rig(root, before_window=before)) as w:
            side = wait_for(lambda: (s := w.ui()["sidebar"]) and s["cwd"] == str(project) and s["loading"] is None and any(e["name"] == "inner" and e["rect"] for e in s["entries"]) and s)
            w.click(*centre(next(e for e in side["entries"] if e["name"] == "inner")["rect"]))
            wait_for(lambda: (s := w.ui()["sidebar"]) and s["cwd"] == str(project / "inner") and s["loading"] is None)
            started = terminal_from_the_icon(w)
            assert started["tabs"][0]["cwd"] == str(project / "inner"), "the folder the person moved to, not the one kept from before: " + str(started["tabs"])


def test_a_terminal_starts_in_the_folder_on_the_left_also_when_a_session_is_in_front():
    """The owner's report (2026-11-19, after M23-P03c): however the start folder is decided on the left, the shell starts elsewhere. A session in front won over the folder Directories shows;
    the folder on the left decides now, and the session's own folder is used only when the left shows none of this machine's."""
    from tests.account_helpers import account
    from tests.gui_helpers import open_form
    from tests.test_new_session_screen import claude_args

    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        project = root.resolve() / "project"
        (project / "inner").mkdir(parents=True)
        args, _received = claude_args(directory)

        def work_account(w: ProductWindow) -> None:
            account(Conn(w.socket), "Work", "claude")
            assert Conn(w.socket).call(op="settings.set", last_cwd=str(project), last_view="start")["ok"] == 1

        with ProductWindow(**rig(root, host_args=args, before_window=work_account)) as w:
            open_form(w)
            w.key("ctrl+u")                                          # the folder row starts from the folder that was kept
            w.type_text(str(project))
            w.key("Return")
            wait_for(lambda: (u := w.ui())["panel"] is None and len(u["rows"]) == 1)
            side = wait_for(lambda: (s := w.ui()["sidebar"]) and s["cwd"] == str(project) and s["loading"] is None and any(e["name"] == "inner" and e["rect"] for e in s["entries"]) and s)
            w.click(*centre(next(e for e in side["entries"] if e["name"] == "inner")["rect"]))
            wait_for(lambda: (s := w.ui()["sidebar"]) and s["cwd"] == str(project / "inner") and s["loading"] is None)
            started = terminal_from_the_icon(w)
            assert started["tabs"][0]["cwd"] == str(project / "inner"), "the folder on the left, not the session's " + str(project) + ": " + str(started["tabs"])


def test_korean_composes_in_the_terminal_and_the_shell_gets_the_committed_text_only():
    from lico_measure.ime import SENTENCE_KEYS

    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        with ProductWindow(**rig(root), ime=True) as w:
            open_by_menu(w)
            time.sleep(0.5)
            w.type_text("echo ")
            w.key("shift+space")                                # the engine starts in Latin mode: its trigger key switches to Hangul
            time.sleep(0.3)
            w.key("--delay", "60", *SENTENCE_KEYS)
            wait_for(lambda: any("안녕하세요" in line and line.startswith("lico$ echo") for line in text(w).splitlines()))
            shot(w, "11_korean_composition.png")
            w.key("Return")
            wait_for(lambda: any(line.strip().startswith("안녕하세요") and not line.startswith("lico$") for line in text(w).splitlines()))


def test_a_tab_shows_the_picture_of_the_agent_that_runs_in_it_and_the_text_again_when_it_ends():
    """M23-P04a: a program named `agy` (a script of that name that prints and waits: what the host sees is the foreground group of the pty and its command line) runs in the shell of a
    real window; the host names it, the window draws its picture where the text `>_` was, and when the program is ended with Ctrl+C the tab is as a plain terminal's again. Claude and
    Codex are named by the same rule (the Rust tests); the host's own answers are `terminal.list` and `terminal.watch`."""
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        with ProductWindow(**rig(root)) as w:
            conn = Conn(w.socket)
            open_by_menu(w)
            plain = tab(w)
            assert plain["agent"] is None and plain["picture_rect"] is None, plain
            run(w, f"printf '#!/bin/sh\\necho agent-up\\nsleep 30\\n' > {root}/agy; chmod +x {root}/agy; echo $((40+2))", "\n42\n")
            w.type_text(f"{root}/agy")
            w.key("Return")
            wait_for(lambda: "agent-up" in text(w))
            named = wait_for(lambda: (t := tab(w)) and t["agent"] == "agy" and t["picture_rect"] and t)
            # the host says the same in its list; the picture is inked in the tab (bright pixels in its rectangle), at the left of the tab, left of where the text begins
            assert [r["agent"] for r in conn.call(op="terminal.list")["terminals"]] == ["agy"]
            left, top, right, bottom = (int(v) for v in named["picture_rect"])
            assert named["rect"][0] < left and right - left >= 8 and bottom - top >= 8, named
            assert w.bright(left, top, right, bottom, 200) > 20, "the picture is drawn"
            shot(w, "8_agent_in_a_tab.png")
            # Ctrl+C ends the script (the foreground group of an interactive shell): the prompt is back and the tab is a plain terminal's
            w.key("ctrl+c")
            wait_for(lambda: (t := tab(w)) and t["agent"] is None and t["picture_rect"] is None and text(w).rstrip().endswith("lico$"))
            assert [r["agent"] for r in conn.call(op="terminal.list")["terminals"]] == [None]

