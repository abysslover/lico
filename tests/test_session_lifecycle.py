"""M02-P02-S05 host side: closing a session ends its process and keeps it, deleting it removes it, each while a turn runs, and neither
touches the folder the person chose. Both are reachable by the command line."""
import json
import subprocess
from pathlib import Path

from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir

from tests.test_activity import begin
from tests.test_claude_adapter import claude_host, new_claude, records, status_of
from tests.test_codex_adapter import wait_for
from tests.test_recovery import registered


def working_in(directory, conn, name):
    """A claude session in a folder of its own with a sentinel file in it, in the middle of a turn: (project, session, generation, pid)."""
    project = Path(directory).resolve() / name
    project.mkdir()
    (project / "sentinel.txt").write_text("mine")
    sid, generation = new_claude(conn, cwd=str(project))
    begin(conn, sid, generation, "slow:80")                                     # a line every 100 ms for eight seconds
    wait_for(lambda: status_of(conn, sid).get("activity") == "working")
    pid = [a["pid"] for a in conn.call(op="resources")["agents"] if a["session"] == sid][0]
    return project, sid, generation, pid


def gone(pid):
    return wait_for(lambda: not Path(f"/proc/{pid}").exists() or None)


def test_closing_a_session_in_the_middle_of_a_turn_ends_the_process_keeps_the_session_and_it_resumes():
    with private_dir() as directory:
        host, log, received = claude_host(directory)

        def run(*args):
            return subprocess.run([str(LICO), "--socket", host.socket, *args], capture_output=True, timeout=20, text=True)
        try:
            conn = Conn(host.socket)
            project, sid, generation, pid = working_in(directory, conn, "project")
            other, other_generation = new_claude(conn)
            assert conn.call(op="session.close", session=sid, generation=generation + 1)["error"] == "stale_generation"
            assert status_of(conn, sid)["state"] == "running"
            closed = run("session", "close", sid)                              # the command line asks for the same op
            assert closed.returncode == 0 and json.loads(closed.stdout)["generation"] == generation, closed.stderr
            gone(pid)
            entry = status_of(conn, sid)
            assert (entry["state"], entry["generation"], entry["cwd"], "activity" in entry) == ("disconnected", generation, str(project), False)
            assert records(log, sid)[-1] == "[session closed]" and (project / "sentinel.txt").read_text() == "mine"
            assert registered(directory)[sid]["generation"] == generation
            assert conn.call(op="session.close", session=sid, generation=generation)["ok"] == 1          # closing it again changes nothing
            assert status_of(conn, other)["state"] == "running"                                           # nothing of another session is touched
            assert conn.call(op="session.close", session="s" + "0" * 16, generation=1)["error"] == "unknown_session"
            wait_for(lambda: registered(directory).get(sid, {}).get("provider_session"))
            resumed = conn.call(op="session.resume", session=sid, generation=generation)
            assert resumed["ok"] == 1 and resumed["generation"] == generation + 1
            assert status_of(conn, sid)["state"] == "running"
        finally:
            host.stop()


def test_deleting_a_session_in_the_middle_of_a_turn_stops_the_turn_removes_the_record_and_leaves_the_chosen_folder_alone():
    with private_dir() as directory:
        host, log, received = claude_host(directory)

        def run(*args):
            return subprocess.run([str(LICO), "--socket", host.socket, *args], capture_output=True, timeout=20, text=True)
        try:
            conn = Conn(host.socket)
            project, sid, generation, pid = working_in(directory, conn, "project")
            other, other_generation = new_claude(conn)
            assert run("session", "delete", sid).returncode == 0
            gone(pid)
            assert sid not in [e["session"] for e in conn.call(op="status")["sessions"]] and sid not in registered(directory)
            assert records(log, sid) == [] and (project / "sentinel.txt").read_text() == "mine" and project.is_dir()
            assert status_of(conn, other)["state"] == "running"
            assert conn.call(op="session.purge", session=sid, generation=generation)["error"] == "unknown_session"
        finally:
            host.stop()
