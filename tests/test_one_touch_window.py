"""M24-P02: one touch in the real window. A click (or Enter) on the Claude picture of the new-session wizard starts the session at once, in the folder the left side lists, with the
account the picture's provider has; Shift is the old way into the second and third step. The host is real, the agent is the mocked Claude of the other window tests."""

from __future__ import annotations

from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import ProtoConn as Conn
from lico_measure.rig import private_dir
from tests.gui_helpers import centre
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_icon_toolbar_wizard import _button, _ready, _rig, _shift_click, _step


def step_title(panel):
    """The wizard's step as the state says it (the title of the panel is only the name of what is made)."""
    return (((panel or {}).get("form") or {}).get("wizard") or {}).get("title", "")


def _window(root: Path, project: Path):
    """The window of the rig with one Claude account, whose last folder is `project` (so that the left lists it)."""
    kwargs, made, _received = _rig(root, [("Work", "claude")])
    first = kwargs["before_window"]

    def before(w: ProductWindow) -> None:
        first(w)
        assert Conn(w.socket).call(op="settings.set", last_cwd=str(project), last_view="start")["ok"] == 1

    kwargs["before_window"] = before
    return ProductWindow(**kwargs), made


def _walk_left_into_inner(w: ProductWindow, project: Path) -> None:
    """Directories lists `project`; a click on `inner` makes the left list it. The folder the window kept (`last_cwd`) stays `project`: moving about in Directories does not change it."""
    side = wait_for(lambda: (s := w.ui()["sidebar"]) and s["cwd"] == str(project) and s["loading"] is None and any(e["name"] == "inner" and e["rect"] for e in s["entries"]) and s)
    w.click(*centre(next(e for e in side["entries"] if e["name"] == "inner")["rect"]))
    wait_for(lambda: (s := w.ui()["sidebar"]) and s["cwd"] == str(project / "inner") and s["loading"] is None)


def test_a_click_on_the_claude_picture_starts_the_session_at_once_in_the_folder_on_the_left():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        project = root.resolve() / "project"
        (project / "inner").mkdir(parents=True)
        window, made = _window(root, project)
        with window as w:
            conn = Conn(w.socket)
            _ready(w)
            _walk_left_into_inner(w, project)
            w.click(*centre(_button(w, "new_session")["rect"]))
            w.click(*centre(_step(w, 0)["form"]["tiles"][0]["rect"]))                  # Claude
            state = wait_for(lambda: (u := w.ui())["panel"] is None and len(u["rows"]) == 1 and u)
            entry = conn.call(op="status")["sessions"][0]
            assert (entry["cwd"], entry["account"], entry["provider"]) == (str(project / "inner"), made["Work"], "claude"), entry
            assert state["selected"] == entry["session"], "the new session is the one shown"


def test_enter_on_the_focused_picture_starts_the_session_at_once_too():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        project = root.resolve() / "project"
        (project / "inner").mkdir(parents=True)
        window, made = _window(root, project)
        with window as w:
            conn = Conn(w.socket)
            _ready(w)
            w.key("ctrl+n")
            assert _step(w, 0)["form"]["focus"] == "Pick(0)"
            w.key("Return")
            wait_for(lambda: (u := w.ui())["panel"] is None and len(u["rows"]) == 1)
            entry = conn.call(op="status")["sessions"][0]
            assert (entry["account"], entry["provider"]) == (made["Work"], "claude"), entry


def test_shift_with_the_click_is_the_old_way_the_second_step_and_nothing_is_started():
    with usual_umask(), private_dir() as directory:
        root = Path(directory)
        project = root.resolve() / "project"
        project.mkdir()
        window, _made = _window(root, project)
        with window as w:
            conn = Conn(w.socket)
            _ready(w)
            w.click(*centre(_button(w, "new_session")["rect"]))
            _shift_click(w, *centre(_step(w, 0)["form"]["tiles"][0]["rect"]))
            second = _step(w, 1)
            assert step_title(second) == "New session - step 2 of 3 - Account and folder", second["title"]
            assert conn.call(op="status")["sessions"] == [], "choosing with Shift starts nothing"
