"""M24-P10a: the right-click menu of the Directories list, against a real host (the shell is `/bin/sh`) and a real window on a private Xvfb. The owner's words (2026-11-25): a right click in the
left Directories list opens a menu; Open terminal is its first line (the one used most), New folder makes a directory. What the Rust tests prove inside the window (the order and the greying of the
lines, the target of a row, the placement) is not repeated; here the doors are: the right button over a folder row, a file row and the empty space, what the menu holds, a click on a line, the
keys, a click away, and what lands on the disk and in the terminal."""

from __future__ import annotations

from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.gui_helpers import centre
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_terminal_tab import rig

LINES = ["Open terminal", "New folder"]


def start(w: ProductWindow, project: Path) -> dict:
    """The window's Directories list on `project`, listed, with its rows drawn; returns the sidebar's state."""
    return wait_for(lambda: (s := w.ui()["sidebar"]) and s["cwd"] == str(project) and s["loading"] is None and s["load_count"] > 0 and all(e["rect"] for e in s["entries"]) and s["entries"] and s)


def row(side: dict, name: str) -> dict:
    return next(e for e in side["entries"] if e["name"] == name)


def menu(w: ProductWindow) -> dict:
    return w.ui()["sidebar"]["dirmenu"]


def open_menu_on(w: ProductWindow, point: tuple[float, float]) -> dict:
    w.click(*point, button=3)
    return wait_for(lambda: (m := menu(w)) and m["open"] and all(i["rect"] for i in m["items"]) and m)


def empty_space(side: dict) -> tuple[float, float]:
    """A point of the list area below the last row, where nothing is drawn."""
    x0, y0, x1, y1 = side["list_rect"]
    last = max(e["rect"][3] for e in side["entries"])
    assert last + 12 < y1, "the test needs some empty room under the rows: " + str((last, y1))
    return (x0 + x1) / 2, (last + y1) / 2 - 4


def project_tree(root: Path) -> Path:
    project = root.resolve() / "project"
    (project / "inner").mkdir(parents=True)
    (project / "note.txt").write_text("a note\n")
    return project


def keep_folder(project: Path):
    def before(w: ProductWindow) -> None:
        assert Conn(w.socket).call(op="settings.set", last_cwd=str(project), last_view="start")["ok"] == 1
    return before


def test_a_right_click_opens_the_menu_with_open_terminal_first_and_a_click_on_it_opens_a_shell_in_the_clicked_folder():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        project = project_tree(root)
        with ProductWindow(**rig(root, before_window=keep_folder(project))) as w:
            side = start(w, project)
            assert side["dirmenu"]["open"] is False and side["list_rect"], side["dirmenu"]

            m = open_menu_on(w, centre(row(side, "inner")["rect"]))              # a folder row: the folder itself
            assert [i["label"] for i in m["items"]] == LINES, m["items"]
            assert all(i["enabled"] and i["reason"] is None and i["rect"] for i in m["items"]), m["items"]
            assert m["folder"] == str(project / "inner") and m["row"] == ["inner", "dir"], m
            assert m["items"][0]["rect"][3] <= m["items"][1]["rect"][1] + 1, "Open terminal is above New folder"
            assert m["selected"] == 0, "the first usable line is the one Enter runs"
            sx0, sy0, sx1, sy1 = side["list_rect"]
            x0, y0, x1, y1 = m["rect"]
            assert x0 >= 0 and x1 <= sx1 + 1 and y0 >= sy0 - 40 and y1 <= w.ui()["layout"]["window_size"][1], ("inside the left column and the window", m["rect"], side["list_rect"])

            w.click(*centre(m["items"][0]["rect"]))
            started = wait_for(lambda: (u := w.ui()) and u["terminal"]["shown"] and u["terminal"]["tabs"] and u["terminal"])
            assert started["tabs"][0]["cwd"] == str(project / "inner"), started["tabs"]
            assert menu(w)["open"] is False, "a click on a line closes the menu"


def test_a_file_row_and_the_empty_space_mean_the_folder_the_list_shows():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        project = project_tree(root)
        with ProductWindow(**rig(root, before_window=keep_folder(project))) as w:
            side = start(w, project)
            m = open_menu_on(w, centre(row(side, "note.txt")["rect"]))
            assert m["folder"] == str(project) and m["row"] == ["note.txt", "file"], m
            w.key("Escape")
            wait_for(lambda: menu(w)["open"] is False)
            m = open_menu_on(w, empty_space(side))
            assert m["folder"] == str(project) and m["row"] is None, m
            w.click(*centre(m["items"][0]["rect"]))
            started = wait_for(lambda: (u := w.ui()) and u["terminal"]["shown"] and u["terminal"]["tabs"] and u["terminal"])
            assert started["tabs"][0]["cwd"] == str(project), started["tabs"]


def test_the_keys_move_over_the_lines_enter_runs_and_escape_or_a_click_away_close_without_doing_anything():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        project = project_tree(root)
        with ProductWindow(**rig(root, before_window=keep_folder(project))) as w:
            side = start(w, project)
            open_menu_on(w, centre(row(side, "inner")["rect"]))
            w.key("Down")
            assert menu(w)["selected"] == 1
            w.key("Down")
            assert menu(w)["selected"] == 1, "it stops at the last line"
            w.key("Up")
            w.key("Up")
            assert menu(w)["selected"] == 0
            w.key("Escape")
            assert menu(w)["open"] is False and w.ui()["panel"] is None and not w.ui()["terminal"]["tabs"], "Escape closes and does nothing else"

            open_menu_on(w, centre(row(side, "inner")["rect"]))
            w.click(*empty_space(side))                                          # a click away: the menu closes and the click is not passed on
            assert menu(w)["open"] is False
            assert w.ui()["sidebar"]["cwd"] == str(project), "the click away did not enter a folder"

            open_menu_on(w, centre(row(side, "note.txt")["rect"]))
            w.click(*centre(row(side, "inner")["rect"]))                         # a click on a row while the menu is open only closes it
            assert menu(w)["open"] is False and w.ui()["sidebar"]["cwd"] == str(project), "the row under a click away is not entered"

            m = open_menu_on(w, centre(row(side, "inner")["rect"]))              # the pointer over a line selects it
            w.move(*centre(m["items"][1]["rect"]))
            assert menu(w)["selected"] == 1, menu(w)
            w.move(*centre(m["items"][0]["rect"]))
            assert menu(w)["selected"] == 0, menu(w)
            w.key("Escape")

            open_menu_on(w, centre(row(side, "inner")["rect"]))
            w.key("Return")                                                      # Enter runs the selected line: Open terminal
            started = wait_for(lambda: (u := w.ui()) and u["terminal"]["shown"] and u["terminal"]["tabs"] and u["terminal"])
            assert started["tabs"][0]["cwd"] == str(project / "inner"), started["tabs"]


def test_a_right_click_at_the_right_edge_keeps_the_whole_menu_inside_the_left_column_and_the_window():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        project = project_tree(root)
        with ProductWindow(**rig(root, before_window=keep_folder(project))) as w:
            side = start(w, project)
            x0, y0, x1, y1 = side["list_rect"]
            m = open_menu_on(w, (x1 - 2, (y0 + y1) / 2))
            mx0, my0, mx1, my1 = m["rect"]
            assert mx1 - mx0 > 20 and mx0 >= 0 and mx1 <= x1 + 1, ("the menu is moved left to stay in the column", m["rect"], side["list_rect"])
            assert my1 <= w.ui()["layout"]["window_size"][1], "and above the bottom of the window"
            assert all(i["rect"][0] >= mx0 and i["rect"][2] <= mx1 for i in m["items"]), "the lines are inside the menu"
            w.click(*centre(m["items"][1]["rect"]))                              # the moved menu still takes its clicks
            assert wait_for(lambda: (p := w.ui()["panel"]) and p["text_entry"] and p), "New folder was reached"


def test_a_right_click_over_the_header_opens_no_menu():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        project = project_tree(root)
        with ProductWindow(**rig(root, before_window=keep_folder(project))) as w:
            side = start(w, project)
            w.click(*centre(side["dir_tab_rect"]), button=3)
            assert menu(w)["open"] is False, "the tab is no part of the list"
            w.click(*centre(side["filter_rect"]), button=3)
            assert menu(w)["open"] is False, "the filter box is no part of the list"
            w.click(*centre(row(side, "inner")["rect"]), button=2)
            assert menu(w)["open"] is False, "only the right button opens the menu"
            assert w.ui()["sidebar"]["cwd"] == str(project)


def test_new_folder_asks_a_name_makes_the_folder_in_the_target_and_refuses_a_name_that_is_taken():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        project = project_tree(root)
        with ProductWindow(**rig(root, before_window=keep_folder(project))) as w:
            side = start(w, project)
            m = open_menu_on(w, empty_space(side))
            w.click(*centre(m["items"][1]["rect"]))
            panel = wait_for(lambda: (p := w.ui()["panel"]) and p["text_entry"] and p)
            assert "New folder" in panel["title"] and str(project) in "\n".join(panel["body"]), panel
            assert menu(w)["open"] is False, "the question replaces the menu"

            w.type_text("inner")                                                 # a name that is taken
            w.key("Return")
            panel = wait_for(lambda: (p := w.ui()["panel"]) and "taken" in "\n".join(p["body"]) and p)
            assert panel["input_len"] == len("inner"), "what was typed stays to be corrected"
            assert [p.name for p in sorted(project.iterdir())] == ["inner", "note.txt"], "nothing was made"

            w.key("ctrl+u")
            w.type_text("made here")
            w.key("Return")
            wait_for(lambda: w.ui()["panel"] is None)
            assert (project / "made here").is_dir(), sorted(p.name for p in project.iterdir())
            side = wait_for(lambda: (s := w.ui()["sidebar"]) and any(e["name"] == "made here" and e["kind"] == "dir" for e in s["entries"]) and s)
            assert side["cwd"] == str(project), "the list went on showing the folder, now with the new one"

            m = open_menu_on(w, centre(row(side, "inner")["rect"]))              # on a folder row: the new folder is made inside it
            w.click(*centre(m["items"][1]["rect"]))
            wait_for(lambda: (p := w.ui()["panel"]) and p["text_entry"])
            w.type_text("deeper")
            w.key("Return")
            wait_for(lambda: w.ui()["panel"] is None)
            assert (project / "inner" / "deeper").is_dir() and not (project / "deeper").exists()

            m = open_menu_on(w, empty_space(w.ui()["sidebar"]))                  # Escape on the question makes nothing
            w.click(*centre(m["items"][1]["rect"]))
            wait_for(lambda: (p := w.ui()["panel"]) and p["text_entry"])
            w.type_text("never")
            w.key("Escape")
            wait_for(lambda: w.ui()["panel"] is None)
            assert not (project / "never").exists()


def test_the_lines_are_greyed_with_their_reason_on_an_open_screen_and_a_greyed_line_does_nothing():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        project = project_tree(root)
        with ProductWindow(**rig(root, before_window=keep_folder(project))) as w:
            side = start(w, project)
            m = open_menu_on(w, empty_space(side))
            w.click(*centre(m["items"][1]["rect"]))                              # the New folder question is a screen now
            wait_for(lambda: (p := w.ui()["panel"]) and p["text_entry"])
            w.type_text("abc")
            side = w.ui()["sidebar"]
            m = open_menu_on(w, empty_space(side))
            assert [(i["label"], i["enabled"], i["reason"]) for i in m["items"]] == [("Open terminal", True, None), ("New folder", False, "close the open screen first")], m["items"]
            assert m["selected"] == 0
            w.click(*centre(m["items"][1]["rect"]))                              # a greyed line is not run, and the menu closes
            assert menu(w)["open"] is False
            assert w.ui()["panel"]["text_entry"] and w.ui()["panel"]["input_len"] == 3, "the question is still the one that was open, with what was typed in it"
