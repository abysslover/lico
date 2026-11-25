"""M24-P10a, P10b and P10c: the right-click menu of the Directories list, against a real host (the shell is `/bin/sh`) and a real window on a private Xvfb. The owner's words (2026-11-25): a right
click in the left Directories list opens a menu; Open terminal is its first line (the one used most), then New folder, Rename, Delete, Extract and Compress. What the Rust tests prove inside the
window (the order and the greying of the lines, the target of a row, the placement, the vetting of an archive) is not repeated; here the doors are: the right button over a folder row, a file row
and the empty space, what the menu holds, a click on a line, the keys, a click away, and what lands on the disk and in the terminal."""

from __future__ import annotations

import os
import subprocess
import tarfile
import zipfile
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.gui_helpers import centre
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_terminal_tab import rig

LINES = ["Open terminal", "New folder", "Rename", "Delete", "Extract", "Compress"]


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
            assert [(i["enabled"], i["reason"]) for i in m["items"]] == [(True, None)] * 4 + [(False, "not an archive"), (True, None)], m["items"]
            assert all(i["rect"] for i in m["items"]), m["items"]
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
            w.key("Down")
            assert menu(w)["selected"] == 3
            w.key("Down")
            assert menu(w)["selected"] == 5, "Extract is greyed on a folder: the selection steps over it to Compress"
            w.key("Down")
            assert menu(w)["selected"] == 5, "it stops at the last line"
            for _ in range(4):
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
            shut = "close the open screen first"
            assert [(i["label"], i["enabled"], i["reason"]) for i in m["items"]] == [("Open terminal", True, None), ("New folder", False, shut), ("Rename", False, shut), ("Delete", False, shut), ("Extract", False, "point at a file or folder"), ("Compress", False, "point at a file or folder")], m["items"]
            assert m["selected"] == 0
            w.click(*centre(m["items"][1]["rect"]))                              # a greyed line is not run, and the menu closes
            assert menu(w)["open"] is False
            assert w.ui()["panel"]["text_entry"] and w.ui()["panel"]["input_len"] == 3, "the question is still the one that was open, with what was typed in it"


def test_rename_and_delete_are_greyed_on_the_empty_space_with_their_reason_and_a_click_on_one_does_nothing():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        project = project_tree(root)
        with ProductWindow(**rig(root, before_window=keep_folder(project))) as w:
            side = start(w, project)
            m = open_menu_on(w, empty_space(side))
            point = "point at a file or folder"
            assert [(i["label"], i["enabled"], i["reason"]) for i in m["items"]] == [("Open terminal", True, None), ("New folder", True, None), ("Rename", False, point), ("Delete", False, point), ("Extract", False, point), ("Compress", False, point)], m["items"]
            assert m["selected"] == 0
            w.click(*centre(m["items"][2]["rect"]))
            assert menu(w)["open"] is False and w.ui()["panel"] is None, "a greyed line asks nothing"
            assert sorted(p.name for p in project.iterdir()) == ["inner", "note.txt"]


def test_rename_asks_the_new_name_with_the_old_one_on_the_line_and_never_replaces():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        project = project_tree(root)
        (project / "inner" / "child.txt").write_text("child\n")
        with ProductWindow(**rig(root, before_window=keep_folder(project))) as w:
            side = start(w, project)
            m = open_menu_on(w, centre(row(side, "note.txt")["rect"]))
            assert all(i["enabled"] for i in m["items"][:4]), m["items"]
            w.click(*centre(m["items"][2]["rect"]))
            panel = wait_for(lambda: (p := w.ui()["panel"]) and p["text_entry"] and p)
            assert "Rename" in panel["title"] and "note.txt" in "\n".join(panel["body"]), panel
            assert panel["input_len"] == len("note.txt"), "the old name is on the line"
            assert menu(w)["open"] is False

            w.key("ctrl+u")                                                      # a name that is taken: refused, nothing replaced, what was typed stays
            w.type_text("inner")
            w.key("Return")
            panel = wait_for(lambda: (p := w.ui()["panel"]) and "taken" in "\n".join(p["body"]) and p)
            assert panel["input_len"] == len("inner")
            assert sorted(p.name for p in project.iterdir()) == ["inner", "note.txt"] and (project / "inner" / "child.txt").is_file()

            w.key("ctrl+u")                                                      # the same name: said
            w.type_text("note.txt")
            w.key("Return")
            wait_for(lambda: (p := w.ui()["panel"]) and "its name already" in "\n".join(p["body"]))

            w.key("ctrl+u")                                                      # a good name, in Korean
            w.type_text("메모")
            w.key("Return")
            wait_for(lambda: w.ui()["panel"] is None)
            assert (project / "메모").read_text() == "a note\n" and not (project / "note.txt").exists()
            side = wait_for(lambda: (s := w.ui()["sidebar"]) and any(e["name"] == "메모" for e in s["entries"]) and not any(e["name"] == "note.txt" for e in s["entries"]) and s)

            m = open_menu_on(w, centre(row(side, "inner")["rect"]))              # a folder goes with what is in it; Escape renames nothing
            w.click(*centre(m["items"][2]["rect"]))
            wait_for(lambda: (p := w.ui()["panel"]) and p["text_entry"])
            w.key("ctrl+u")
            w.type_text("never")
            w.key("Escape")
            wait_for(lambda: w.ui()["panel"] is None)
            assert (project / "inner").is_dir() and not (project / "never").exists()
            m = open_menu_on(w, centre(row(side, "inner")["rect"]))
            w.click(*centre(m["items"][2]["rect"]))
            wait_for(lambda: (p := w.ui()["panel"]) and p["text_entry"])
            w.key("ctrl+u")
            w.type_text("outer")
            w.key("Return")
            wait_for(lambda: w.ui()["panel"] is None)
            assert (project / "outer" / "child.txt").read_text() == "child\n" and not (project / "inner").exists()


def test_delete_asks_what_goes_n_keeps_it_y_removes_it_and_a_link_goes_as_a_link():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        project = project_tree(root)
        outside = root.resolve() / "outside"
        outside.mkdir()
        (outside / "precious.txt").write_text("precious\n")
        (project / "inner" / "deep.txt").write_text("deep\n")
        (project / "inner" / "to-outside").symlink_to(outside)
        (project / "pointer").symlink_to(outside)
        with ProductWindow(**rig(root, before_window=keep_folder(project))) as w:
            side = start(w, project)
            m = open_menu_on(w, centre(row(side, "note.txt")["rect"]))
            w.click(*centre(m["items"][3]["rect"]))
            panel = wait_for(lambda: (p := w.ui()["panel"]) and not p["text_entry"] and "Delete" in p["title"] and p)
            body = "\n".join(panel["body"])
            assert "note.txt" in body and "a file of 7 bytes" in body and "cannot be undone" in body, body
            assert "The file is removed for good." in body, body
            assert any(line.startswith("Y") for line in panel["body"]) and any(line.startswith("N") for line in panel["body"]), panel["body"]
            w.key("n")                                                           # N keeps it
            wait_for(lambda: w.ui()["panel"] is None)
            assert (project / "note.txt").is_file()

            m = open_menu_on(w, centre(row(side, "note.txt")["rect"]))
            w.click(*centre(m["items"][3]["rect"]))
            wait_for(lambda: (p := w.ui()["panel"]) and "Delete" in p["title"])
            w.key("Escape")                                                      # Escape keeps it too
            wait_for(lambda: w.ui()["panel"] is None)
            assert (project / "note.txt").is_file()

            m = open_menu_on(w, centre(row(side, "note.txt")["rect"]))
            w.click(*centre(m["items"][3]["rect"]))
            wait_for(lambda: (p := w.ui()["panel"]) and "Delete" in p["title"])
            w.key("y")                                                           # Y removes it and the list shows what is left
            wait_for(lambda: w.ui()["panel"] is None)
            assert not (project / "note.txt").exists()
            side = wait_for(lambda: (s := w.ui()["sidebar"]) and not any(e["name"] == "note.txt" for e in s["entries"]) and s)

            m = open_menu_on(w, centre(row(side, "pointer")["rect"]))            # a link to a folder: only the link goes
            w.click(*centre(m["items"][3]["rect"]))
            panel = wait_for(lambda: (p := w.ui()["panel"]) and "Delete" in p["title"] and p)
            assert "a link to" in "\n".join(panel["body"]) and "Only the link" in "\n".join(panel["body"]), panel["body"]
            w.key("y")
            wait_for(lambda: w.ui()["panel"] is None)
            assert not (project / "pointer").exists() and (outside / "precious.txt").is_file()
            side = wait_for(lambda: (s := w.ui()["sidebar"]) and not any(e["name"] == "pointer" for e in s["entries"]) and s)

            m = open_menu_on(w, centre(row(side, "inner")["rect"]))              # a folder with a link in it: the folder goes, the link goes as a link
            w.click(*centre(m["items"][3]["rect"]))
            panel = wait_for(lambda: (p := w.ui()["panel"]) and "Delete" in p["title"] and p)
            assert "a folder with 2 items" in "\n".join(panel["body"]), panel["body"]
            assert "The folder and all that is in it are removed for good" in "\n".join(panel["body"]), panel["body"]
            w.key("y")
            wait_for(lambda: w.ui()["panel"] is None)
            assert not (project / "inner").exists()
            assert (outside / "precious.txt").read_text() == "precious\n", "what the link inside pointed to is not touched"


def test_the_home_folder_is_protected_with_its_reason_and_the_menu_still_opens_a_terminal_there():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        people = root.resolve() / "people"
        home = people / "me"
        (home / "work").mkdir(parents=True)
        with ProductWindow(**rig(root, before_window=keep_folder(people), env={"HOME": str(home)})) as w:
            side = start(w, people)
            m = open_menu_on(w, centre(row(side, "me")["rect"]))
            assert [(i["label"], i["enabled"], i["reason"]) for i in m["items"]] == [("Open terminal", True, None), ("New folder", True, None), ("Rename", False, "protected"), ("Delete", False, "protected"), ("Extract", False, "not an archive"), ("Compress", True, None)], m["items"]
            w.click(*centre(m["items"][3]["rect"]))
            assert w.ui()["panel"] is None and home.is_dir() and (home / "work").is_dir(), "a protected row asks nothing and loses nothing"


def flash_of(w: ProductWindow, start_of: str) -> str:
    """The one-line notice of the window once it begins with `start_of` (the answer of an archive)."""
    return wait_for(lambda: (f := menu(w)["flash"]) and f.startswith(start_of) and f)


def unzip_names(archive: Path) -> list[str]:
    """What the system's own `unzip` lists (it reads a Korean name that a plain zip reader garbles)."""
    return subprocess.run(["unzip", "-Z1", str(archive)], check=True, capture_output=True, text=True).stdout.splitlines()


def test_compress_makes_a_zip_beside_the_row_never_replaces_one_and_the_list_shows_it():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        project = project_tree(root)
        (project / "inner" / "자료.txt").write_text("한글\n")
        (project / "inner" / "sub").mkdir()
        (project / "inner" / "sub" / "deep.txt").write_text("deep\n")
        with ProductWindow(**rig(root, before_window=keep_folder(project))) as w:
            side = start(w, project)
            m = open_menu_on(w, centre(row(side, "inner")["rect"]))
            assert m["items"][5]["label"] == "Compress" and m["items"][5]["enabled"], m["items"]
            w.click(*centre(m["items"][5]["rect"]))
            assert flash_of(w, "Compressed:") == "Compressed: inner.zip"
            names = unzip_names(project / "inner.zip")
            assert {"inner/", "inner/sub/", "inner/sub/deep.txt", "inner/자료.txt"} <= set(names), names
            assert not [p.name for p in project.iterdir() if p.name.startswith(".")], "no half-made file is left behind"
            side = wait_for(lambda: (s := w.ui()["sidebar"]) and any(e["name"] == "inner.zip" and e["kind"] == "file" for e in s["entries"]) and s)
            assert menu(w)["archive_busy"] is False

            first = (project / "inner.zip").read_bytes()
            m = open_menu_on(w, centre(row(side, "inner")["rect"]))              # a second one is a new name; the first is the same bytes
            w.click(*centre(m["items"][5]["rect"]))
            assert flash_of(w, "Compressed:") == "Compressed: inner 2.zip"
            assert (project / "inner.zip").read_bytes() == first and (project / "inner 2.zip").is_file()

            side = wait_for(lambda: (s := w.ui()["sidebar"]) and any(e["name"] == "note.txt" for e in s["entries"]) and s)
            m = open_menu_on(w, centre(row(side, "note.txt")["rect"]))           # a file is compressed under its whole name
            w.click(*centre(m["items"][5]["rect"]))
            assert flash_of(w, "Compressed:") == "Compressed: note.txt.zip"
            assert unzip_names(project / "note.txt.zip") == ["note.txt"]

            m = open_menu_on(w, empty_space(w.ui()["sidebar"]))                  # nothing to compress on the empty space
            assert (m["items"][5]["enabled"], m["items"][5]["reason"]) == (False, "point at a file or folder")


def test_extract_opens_a_zip_and_a_tar_gz_into_new_folders_and_a_taken_name_gets_a_number():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        project = project_tree(root)
        with zipfile.ZipFile(project / "bundle.zip", "w") as z:
            z.writestr("docs/a.txt", "alpha\n")
            z.writestr("docs/b.txt", "beta\n")
            z.writestr("top.txt", "top\n")
            z.writestr(zipfile.ZipInfo("empty/"), "")
        (project / "fake.zip").mkdir()
        (project / "alias.zip").symlink_to(project / "bundle.zip")
        payload = root.resolve() / "payload.txt"
        payload.write_text("tar payload\n")
        with tarfile.open(project / "data.tar.gz", "w:gz") as t:
            t.add(payload, arcname="data/payload.txt")
        with ProductWindow(**rig(root, before_window=keep_folder(project))) as w:
            side = start(w, project)
            m = open_menu_on(w, centre(row(side, "bundle.zip")["rect"]))
            assert (m["items"][4]["label"], m["items"][4]["enabled"], m["items"][4]["reason"]) == ("Extract", True, None), m["items"]
            w.click(*centre(m["items"][4]["rect"]))
            assert flash_of(w, "Extracted:") == "Extracted: bundle"
            assert (project / "bundle" / "docs" / "a.txt").read_text() == "alpha\n" and (project / "bundle" / "top.txt").read_text() == "top\n"
            assert (project / "bundle" / "empty").is_dir()
            side = wait_for(lambda: (s := w.ui()["sidebar"]) and any(e["name"] == "bundle" and e["kind"] == "dir" for e in s["entries"]) and s)

            m = open_menu_on(w, centre(row(side, "bundle.zip")["rect"]))         # again: nothing is replaced, the next free name
            w.click(*centre(m["items"][4]["rect"]))
            assert flash_of(w, "Extracted:") == "Extracted: bundle 2"
            assert (project / "bundle 2" / "top.txt").is_file() and (project / "bundle" / "top.txt").is_file()

            side = wait_for(lambda: (s := w.ui()["sidebar"]) and any(e["name"] == "data.tar.gz" for e in s["entries"]) and s)
            m = open_menu_on(w, centre(row(side, "data.tar.gz")["rect"]))
            w.click(*centre(m["items"][4]["rect"]))
            assert flash_of(w, "Extracted:") == "Extracted: data"
            assert (project / "data" / "data" / "payload.txt").read_text() == "tar payload\n"

            m = open_menu_on(w, centre(row(w.ui()["sidebar"], "note.txt")["rect"]))   # a plain file is no archive: greyed with its reason, a click does nothing
            assert (m["items"][4]["enabled"], m["items"][4]["reason"]) == (False, "not an archive"), m["items"]
            before = sorted(p.name for p in project.iterdir())
            w.click(*centre(m["items"][4]["rect"]))
            assert menu(w)["open"] is False and sorted(p.name for p in project.iterdir()) == before

            side = wait_for(lambda: (s := w.ui()["sidebar"]) and all(e["rect"] for e in s["entries"]) and "fake.zip" in [e["name"] for e in s["entries"]] and s)
            m = open_menu_on(w, centre(row(side, "fake.zip")["rect"]))           # a folder named like an archive is no archive
            assert (m["items"][4]["enabled"], m["items"][4]["reason"]) == (False, "not an archive"), m["items"]
            w.key("Escape")
            m = open_menu_on(w, centre(row(side, "alias.zip")["rect"]))          # a link to an archive file is listed as a file and read like it
            assert m["items"][4]["enabled"], m["items"]
            w.click(*centre(m["items"][4]["rect"]))
            assert flash_of(w, "Extracted:") == "Extracted: alias"
            assert (project / "alias" / "top.txt").read_text() == "top\n"


def test_an_archive_that_goes_up_starts_at_the_root_or_holds_a_link_is_refused_and_nothing_is_written():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        project = project_tree(root)
        with zipfile.ZipFile(project / "up.zip", "w") as z:
            z.writestr("ok.txt", "fine\n")
            z.writestr("../escaped.txt", "out\n")
        with zipfile.ZipFile(project / "abs.zip", "w") as z:
            z.writestr(zipfile.ZipInfo("/escaped-abs.txt"), "out\n")
        with zipfile.ZipFile(project / "link.zip", "w") as z:
            link = zipfile.ZipInfo("shortcut")
            link.create_system = 3
            link.external_attr = 0o120777 << 16
            z.writestr(link, "/etc")
        with tarfile.open(project / "link.tar", "w") as t:
            info = tarfile.TarInfo("shortcut")
            info.type = tarfile.SYMTYPE
            info.linkname = "/etc"
            t.addfile(info)
        with ProductWindow(**rig(root, before_window=keep_folder(project))) as w:
            start(w, project)
            for name, word in [("up.zip", "goes up"), ("abs.zip", "root"), ("link.zip", "link"), ("link.tar", "link")]:
                side = wait_for(lambda: (s := w.ui()["sidebar"]) and all(e["rect"] for e in s["entries"]) and s)
                m = open_menu_on(w, centre(row(side, name)["rect"]))
                w.click(*centre(m["items"][4]["rect"]))
                why = flash_of(w, "Not extracted:")
                assert word in why, (name, why)
                wait_for(lambda: menu(w)["archive_busy"] is False)
            assert sorted(p.name for p in project.iterdir()) == ["abs.zip", "inner", "link.tar", "link.zip", "note.txt", "up.zip"], "no folder was made"
            assert not (project / "escaped.txt").exists() and not (root / "escaped.txt").exists() and not Path("/escaped-abs.txt").exists()


def test_a_second_archive_waits_for_the_first_and_the_other_lines_do_not():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        project = project_tree(root)
        (project / "heavy").mkdir()
        with open(project / "heavy" / "random.bin", "wb") as out:
            for _ in range(240):
                out.write(os.urandom(1 << 20))                                   # about four seconds of zip on this machine
        with ProductWindow(**rig(root, before_window=keep_folder(project))) as w:
            side = start(w, project)
            m = open_menu_on(w, centre(row(side, "heavy")["rect"]))
            w.click(*centre(m["items"][5]["rect"]))
            wait_for(lambda: menu(w)["archive_busy"] is True)
            assert menu(w)["flash"] == "Compressing heavy...", "the start is said while it runs"
            m = open_menu_on(w, centre(row(side, "note.txt")["rect"]))
            busy = "an archive is being made"
            assert [(i["label"], i["enabled"], i["reason"]) for i in m["items"][4:]] == [("Extract", False, busy), ("Compress", False, busy)], m["items"]
            assert all(i["enabled"] for i in m["items"][:4]), "the rest of the menu does not wait"
            w.click(*centre(m["items"][5]["rect"]))                              # a greyed line says why and starts nothing
            assert menu(w)["flash"] == "Compress: an archive is being made."
            assert flash_of(w, "Compressed:") == "Compressed: heavy.zip"
            assert menu(w)["archive_busy"] is False and not (project / "note.txt.zip").exists(), "only the first was made"
            assert sorted(p.name for p in project.iterdir() if p.name.endswith(".zip")) == ["heavy.zip"]
