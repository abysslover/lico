"""M23-P04c: the `agy` picture of New session, in the real window. The window's PATH of these tests is `/usr/bin:/bin` and the home is the run's own, so whether `agy` is found is the test's
to decide: a script named `agy` (it prints how many arguments it was given and the folder it runs in, then sleeps) is put where the test wants it."""

from __future__ import annotations

from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.gui_helpers import centre
from tests.test_agy_chats import flat
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_terminal_tab import new_session_button, rig, shot, tab, term, walk_then_choose_home

FAKE = '#!/bin/sh\necho "agy-up $# args in $(pwd)"\nsleep 60\n'


def fake_agy(folder: Path, body: str = FAKE, mode: int = 0o755) -> Path:
    folder.mkdir(parents=True, exist_ok=True)
    program = folder / "agy"
    program.write_text(body)
    program.chmod(mode)
    return program


def tiles(w: ProductWindow) -> list[dict]:
    """The pictures of the first step of New session, once they are drawn."""
    new_session_button(w)
    return wait_for(lambda: (p := w.ui()["panel"]) and p["form"] and p["form"]["tiles"] and all(t["rect"] for t in p["form"]["tiles"]) and p["form"]["tiles"])


def agy_tile(w: ProductWindow) -> dict:
    return next(t for t in tiles(w) if t["provider"] == "agy")


def test_the_agy_picture_is_second_starts_plain_agy_in_a_terminal_in_the_folder_on_the_left_and_the_tab_shows_its_picture():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        project = root.resolve() / "project"
        project.mkdir()
        fake_agy(root / "bin")

        def before(w: ProductWindow) -> None:
            assert Conn(w.socket).call(op="settings.set", last_cwd=str(project), last_view="files")["ok"] == 1

        with ProductWindow(**rig(root, path_prefix=str(root / "bin"), before_window=before)) as w:
            all_tiles = tiles(w)
            assert [t["provider"] for t in all_tiles] == ["claude", "agy", "codex", "local", "terminal"], "the owner's order: Claude, agy, Codex, Local LLM, Terminal"
            assert [t["word"] for t in all_tiles][1] == "agy" and all_tiles[1]["icon"] == "agy" and all_tiles[1]["enabled"] is True, all_tiles[1]
            sides = {(round(t["rect"][2] - t["rect"][0]), round(t["rect"][3] - t["rect"][1])) for t in all_tiles}
            assert len(sides) == 1 and next(iter(sides))[0] >= 40, "five squares of one size"
            assert w.ui()["panel"]["form"]["focus"] == "Pick(3)", "no account: Local LLM has the focus at first, never agy (it is no provider of a session)"
            assert "agy is not installed" not in "\n".join(w.ui()["panel"]["body"]), "found: nothing grey is said of it"
            # the pictures are their own: with the ring on the terminal, a lit agy and a lit Local LLM are drawn with other pixels
            w.key("Right")
            wait_for(lambda: w.ui()["panel"]["form"]["focus"] == "Pick(4)")
            data, width, _ = w.gray()

            def inside(tile: dict) -> list:
                x0, y0 = int(tile["rect"][0]) + 8, int(tile["rect"][1]) + 8
                return [data[(y0 + j) * width + x0:(y0 + j) * width + x0 + 64] for j in range(64)]

            assert inside(all_tiles[1]) != inside(all_tiles[3]), "the agy picture is not the Local LLM one"
            assert any(any(v > 150 for v in row) for row in inside(all_tiles[1])), "and it is drawn (lit)"
            shot(w, "11_new_session_five_icons.png")
            w.click(*centre(all_tiles[1]["rect"]))
            started = wait_for(lambda: (u := w.ui()) and u["panel"] is None and u["terminal"]["shown"] and u["terminal"]["tabs"] and u["terminal"])
            assert started["tabs"][0]["cwd"] == str(project), "the folder Directories is on, as the Terminal picture does"
            wait_for(lambda: f"agy-up 0 args in {project}" in flat(w))
            first = wait_for(lambda: (t := tab(w, "t1")) and t["agent"] == "agy" and t["conversation"] is None and t["picture_rect"] and t)  # (the picture is drawn by the frame after the one that knows the agent: under a loaded machine the dump came between the two)
            assert first["picture_rect"], "P04a: the tab shows the agy picture"
            assert len(term(w)["tabs"]) == 1 and len(Conn(w.socket).call(op="terminal.list")["terminals"]) == 1
            shot(w, "12_agy_started_from_new_session.png")


def test_agy_starts_in_the_folder_the_left_lists_also_when_the_scope_is_home():
    """The owner's report (2026-11-20): the folder agy ran in had nothing to do with the work directory. Its own log says where it ran; here the fake prints it."""
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        project = root.resolve() / "project"
        (project / "inner").mkdir(parents=True)
        fake_agy(root / "bin")

        def before(w: ProductWindow) -> None:
            assert Conn(w.socket).call(op="settings.set", last_cwd=str(project), last_view="start")["ok"] == 1

        with ProductWindow(**rig(root, path_prefix=str(root / "bin"), before_window=before)) as w:
            listed = walk_then_choose_home(w, project)
            w.click(*centre(agy_tile(w)["rect"]))
            started = wait_for(lambda: (u := w.ui()) and u["panel"] is None and u["terminal"]["shown"] and u["terminal"]["tabs"] and u["terminal"])
            assert started["tabs"][0]["cwd"] == str(listed), "the folder the left lists, not the one kept (" + str(project) + "): " + str(started["tabs"])
            wait_for(lambda: f"agy-up 0 args in {listed}" in flat(w))


def test_agy_in_the_home_bin_is_found_and_a_file_that_cannot_run_is_not_and_a_grey_picture_says_why_and_opens_nothing():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        fake_agy(root / "bin", "not a program\n", 0o644)   # on the PATH, but not executable
        (root / "bin2").mkdir()
        (root / "bin2" / "agy").mkdir()                     # and a folder of that name
        with ProductWindow(**rig(root, path_prefix=f"{root / 'bin'}:{root / 'bin2'}")) as w:
            home = Path(w.directory) / "home"
            grey = agy_tile(w)
            assert grey["enabled"] is False, "a file that is not executable and a folder are no program"
            assert "agy is not installed on this machine" in "\n".join(w.ui()["panel"]["body"])
            w.click(*centre(grey["rect"]))
            wait_for(lambda: w.ui()["panel"]["form"]["focus"] == "Pick(1)")
            assert w.ui()["terminal"]["tabs"] == [] and w.ui()["panel"]["form"]["wizard"]["page"] == 0, "nothing opens"
            w.key("Escape")
            wait_for(lambda: w.ui()["panel"] is None)
            # `agy` installs itself in the person's ~/.local/bin, which a launcher's PATH may not have: it is looked for there too, each time New session opens
            fake_agy(home / ".local" / "bin")
            assert agy_tile(w)["enabled"] is True
            w.key("Escape")
            wait_for(lambda: w.ui()["panel"] is None)
            (home / ".local" / "bin" / "agy").unlink()
            assert agy_tile(w)["enabled"] is False, "and again grey when it is gone"
