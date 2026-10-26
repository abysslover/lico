"""M02-P02-S05 in the product window: a working session is closed (after a question), comes back, is renamed, and is deleted (after a question)
with the folder the person chose left alone; a scratch session is deleted with its workspace; a double click on a row asks for a name."""
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir

from tests.test_claude_adapter import new_claude, status_of
from tests.test_codex_adapter import wait_for
from tests.test_new_session_screen import claude_args


def panel(w):
    return w.ui()["panel"]


def row_centre(w, sid):
    top, bottom, _ = next(r for r in w.ui()["layout"]["rows"] if r[2] == sid)
    return 100, (top + bottom) / 2


def test_a_working_session_is_closed_resumed_renamed_and_deleted_from_the_window_and_the_chosen_folder_stays():
    with private_dir() as directory:
        project = Path(directory).resolve() / "project"
        project.mkdir()
        (project / "sentinel.txt").write_text("mine")
        args, received = claude_args(directory)
        made = {}

        def sessions(w):
            conn = Conn(w.socket)
            made["a"], made["a_generation"] = new_claude(conn, cwd=str(project), name="api")
            made["b"], _ = new_claude(conn)                                          # a scratch workspace of its own
            conn.call(op="input", session=made["a"], generation=made["a_generation"], request_id="t-1", text="slow:300")

        with ProductWindow(host_binary=LICO, host_args=args, before_window=sessions) as w:
            conn = Conn(w.socket)
            a, b = made["a"], made["b"]
            wait_for(lambda: len(w.ui()["rows"]) == 2 and status_of(conn, a).get("activity") == "working")
            w.click(*row_centre(w, a))
            pid = [x["pid"] for x in conn.call(op="resources")["agents"] if x["session"] == a][0]
            # a working session is not closed without a question; N keeps it, Y stops it and keeps the session
            w.key("ctrl+shift+w")
            asked = panel(w)
            assert asked["title"] == "Close session" and any("Stop the work" in line for line in asked["body"]), asked
            w.key("n")
            assert panel(w) is None and status_of(conn, a)["state"] == "running"
            w.key("ctrl+shift+w", "y")
            assert panel(w) is None and "Session closed" in w.ui()["header"]
            wait_for(lambda: status_of(conn, a)["state"] == "disconnected" and not Path(f"/proc/{pid}").exists())
            assert (project / "sentinel.txt").read_text() == "mine"
            w.key("ctrl+shift+w")                                                     # nothing to close any more: the window says why
            assert panel(w) is None and "already closed" in w.ui()["header"]
            # F2 takes the present name into the line; Enter keeps the new one in the host
            w.key("F2")
            line = panel(w)
            assert line["title"] == "Rename session" and line["text_entry"] and line["input"] == "api", line
            w.key("ctrl+u")
            w.type_text("backend")
            w.key("Return")
            assert panel(w) is None and status_of(conn, a)["name"] == "backend"
            wait_for(lambda: "backend" in w.ui()["list_text"])
            # the closed session comes back with the same folder, and is working again when it is deleted
            w.key("ctrl+r")
            wait_for(lambda: status_of(conn, a)["state"] == "running")
            generation = status_of(conn, a)["generation"]
            conn.call(op="input", session=a, generation=generation, request_id="t-2", text="slow:300")
            wait_for(lambda: status_of(conn, a).get("activity") == "working")
            pid = [x["pid"] for x in conn.call(op="resources")["agents"] if x["session"] == a][0]
            w.type_text("a draft of the deleted")
            w.key("ctrl+Delete")
            asked = panel(w)
            assert asked["title"] == "Delete session" and any(f"The folder {project} is not touched" in line for line in asked["body"]), asked
            w.key("y")
            assert panel(w) is None and "Session deleted" in w.ui()["header"] and w.ui()["selected"] == ""
            wait_for(lambda: not Path(f"/proc/{pid}").exists() and a not in [s["session"] for s in conn.call(op="status")["sessions"]])
            assert (project / "sentinel.txt").read_text() == "mine" and project.is_dir()
            # a double click on the other row opens it and asks for its name; Esc leaves it as it was; the draft of the deleted one is nowhere
            w.click(*row_centre(w, b), times=2)
            assert panel(w)["title"] == "Rename session" and w.ui()["selected"] == b
            w.key("Escape")
            assert panel(w) is None and w.ui()["input"] == "" and "name" not in status_of(conn, b)
            scratch = [p for p in Path(w.directory).rglob(b) if p.is_dir()]
            assert len(scratch) == 1
            w.key("ctrl+Delete", "y")
            wait_for(lambda: conn.call(op="status")["sessions"] == [])
            assert not scratch[0].exists() and (project / "sentinel.txt").read_text() == "mine"
