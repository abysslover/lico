"""P02 host side: a long multi-line input, and sessions that run in a folder the user picked."""
import json
import subprocess
from pathlib import Path

from lico_measure.p02 import LICO, ProtoConn as Conn
from lico_measure.rig import private_dir
from lico_protocol import v1

from tests.test_claude_adapter import argv_of, claude_host, lines_of, new_claude, sent_users, status_of, user_text
from tests.test_codex_adapter import codex_host, new_session, sent, wait_for
from tests.test_recovery import hard_stop, registered


def long_text(size):
    """Lines with tabs, accents and an astral character (a 12-byte escape in JSON), cut to `size` bytes."""
    text, n = "", 0
    while len(text.encode()) < size:
        text += f"line {n}\tcafé 한글 \U0001F600\n"
        n += 1
    while len(text.encode()) > size:
        text = text[:-1]
    return text


def test_a_long_multi_line_input_reaches_both_providers_byte_for_byte_and_one_byte_over_the_limit_is_refused():
    text = long_text(20 * 1024)
    with private_dir() as directory:
        host, log, received = claude_host(directory)
        try:
            conn = Conn(host.socket)
            assert conn.call(op="hello")["max_request"] == v1.MAX_REQUEST
            sid, generation = new_claude(conn, mode="default")
            assert conn.call(op="input", session=sid, generation=generation, request_id="long-1", text=text)["ok"] == 1
            wait_for(lambda: sent_users(received))
            assert [user_text(m) for m in sent_users(received)] == [text]
            # the largest text is accepted whole, one byte more is refused with the connection still usable
            largest = long_text(v1.MAX_INPUT_TEXT)
            assert conn.call(op="input", session=sid, generation=generation, request_id="long-2", text=largest)["ok"] == 1
            refused = conn.call(op="input", session=sid, generation=generation, request_id="long-3", text=largest + "x")
            assert refused["ok"] == 0 and refused["error"] == "bad_request"
            assert conn.call(op="input", session=sid, generation=generation, request_id="long-4", text="a\x01b")["error"] == "bad_request"
            assert conn.call(op="input", session=sid, generation=generation, request_id="long-5", text="a\rb")["error"] == "bad_request"
            assert conn.call(op="status")["ok"] == 1
        finally:
            host.stop()
    with private_dir() as directory:
        host, log, received = codex_host(directory)
        try:
            conn, sid, generation = new_session(host)
            assert conn.call(op="input", session=sid, generation=generation, request_id="long-1", text=text)["ok"] == 1
            turns = wait_for(lambda: [m for m in sent(received) if m.get("method") == "turn/start"])
            assert [t["params"]["input"][0]["text"] for t in turns] == [text]
        finally:
            host.stop()


def test_a_session_runs_in_the_folder_the_person_picked_keeps_it_across_a_restart_and_a_delete_never_touches_it():
    with private_dir() as directory:
        base = Path(directory).resolve()
        project = base / "project"
        project.mkdir()
        (project / "notes.txt").write_text("mine")
        host, log, received = claude_host(directory)
        try:
            conn = Conn(host.socket)
            options = conn.call(op="session.options")            # what the new-session screen offers, from the installed program's own help
            assert (options["models"], options["issue"], options["recent"]) == (["fable", "opus", "sonnet"], None, [])
            assert options["efforts"] == ["low", "medium", "high", "xhigh", "max"]
            sid, generation = new_claude(conn, cwd=str(project), name=" api 서버 ", model="opus", effort="high")
            entry = status_of(conn, sid)
            assert (entry["scratch"], entry["cwd"], entry["name"], entry["chosen_model"], entry["chosen_effort"]) == (False, str(project), "api 서버", "opus", "high")
            wait_for(lambda: f"CWD {project}" in lines_of(received))
            argv = argv_of(received)[-1]
            assert argv[argv.index("--model") + 1] == "opus" and argv[argv.index("--effort") + 1] == "high"
            assert conn.call(op="session.options")["recent"] == [str(project)]
            # a session without a folder runs in a scratch workspace of its own and says so
            scratch_sid, scratch_generation = new_claude(conn)
            scratch = base / "workspaces" / scratch_sid
            assert status_of(conn, scratch_sid)["scratch"] is True and "cwd" not in status_of(conn, scratch_sid) and scratch.is_dir()
            wait_for(lambda: f"CWD {scratch}" in lines_of(received))
            assert "--model" not in argv_of(received)[-1]
            # a name can be given, changed and taken away; nothing the agent sees changes
            assert conn.call(op="session.rename", session=sid, name="renamed")["name"] == "renamed"
            assert status_of(conn, sid)["name"] == "renamed"
            assert conn.call(op="session.rename", session=sid, name="  ")["name"] is None and "name" not in status_of(conn, sid)
            assert conn.call(op="session.rename", session=sid, name="back")["ok"] == 1
            assert conn.call(op="session.rename", session="s" + "0" * 16, name="x")["error"] == "unknown_session"
            # deleting the scratch session removes its workspace; deleting the one in the folder removes nothing of the folder
            assert conn.call(op="session.purge", session=scratch_sid, generation=scratch_generation)["ok"] == 1
            assert not scratch.exists()
            assert conn.call(op="input", session=sid, generation=generation, request_id="first", text="hello")["ok"] == 1
            wait_for(lambda: registered(directory).get(sid, {}).get("provider_session"))         # the program names its session once it has heard from the host
            stored = registered(directory)[sid]
            assert (stored["scratch"], stored["cwd"], stored["name"], stored["chosen_model"], stored["chosen_effort"]) == (False, str(project), "back", "opus", "high")
        finally:
            hard_stop(host)
        host, log, received = claude_host(directory)
        try:
            conn = Conn(host.socket)
            entry = status_of(conn, sid)                           # back after a restart with what the person chose, not running
            assert (entry["state"], entry["cwd"], entry["name"], entry["chosen_model"], entry["chosen_effort"]) == ("disconnected", str(project), "back", "opus", "high")
            # the folder is gone: the resume is refused with the reason and the session stays as it was
            project.rename(base / "moved")
            assert conn.call(op="session.resume", session=sid, generation=generation)["error"] == "folder_missing"
            assert status_of(conn, sid)["state"] == "disconnected" and status_of(conn, sid)["generation"] == generation
            (base / "moved").rename(project)
            spawned = len(argv_of(received))                       # the log is the same file as before the restart
            resumed = conn.call(op="session.resume", session=sid, generation=generation)
            assert resumed["ok"] == 1
            wait_for(lambda: len(argv_of(received)) > spawned)
            argv = argv_of(received)[-1]
            assert argv[argv.index("--resume") + 1] and argv[argv.index("--model") + 1] == "opus" and lines_of(received).count(f"CWD {project}") == 2
            assert conn.call(op="session.purge", session=sid, generation=resumed["generation"])["ok"] == 1
            assert (project / "notes.txt").read_text() == "mine"
        finally:
            host.stop()
    with private_dir() as directory:                              # the Codex app-server gets the folder in thread/start
        project = Path(directory).resolve() / "project"
        project.mkdir()
        host, log, received = codex_host(directory)
        try:
            conn = Conn(host.socket)
            made = conn.call(op="session.create", kind="codex", cwd=str(project))
            assert made["ok"] == 1, made
            starts = wait_for(lambda: [m for m in sent(received) if m.get("method") == "thread/start"])
            assert [m["params"]["cwd"] for m in starts] == [str(project)]
        finally:
            host.stop()


def test_a_folder_that_is_not_safe_or_not_there_gives_a_reason_and_no_session_and_a_delete_never_reaches_outside_the_workspace():
    with private_dir() as directory:
        base = Path(directory).resolve()
        host, log, received = claude_host(directory)
        try:
            (base / "a_file").write_text("x")
            (base / "link_into_workspaces").symlink_to(base / "workspaces")
            (base / "workspaces" / "s0123456789abcdef").mkdir()
            conn = Conn(host.socket)
            refused = {"relative/folder": "folder_not_absolute", str(base / "missing"): "folder_missing", str(base / "a_file"): "folder_not_directory",
                       str(base / "workspaces"): "folder_protected", str(base / "link_into_workspaces"): "folder_protected",
                       str(base / "workspaces" / "s0123456789abcdef"): "folder_protected", str(base / "registry"): "folder_protected", str(base / "log"): "folder_protected"}
            for folder, reason in refused.items():
                assert conn.call(op="session.create", kind="claude", cwd=folder)["error"] == reason, folder
            for field, value in [("cwd", ""), ("cwd", "/tmp/a\nb"), ("name", "two\nlines"), ("name", "x" * 61), ("model", "o pus"), ("model", "-x"), ("effort", "High")]:
                assert conn.call(op="session.create", kind="claude", **{field: value})["error"] == "bad_request", (field, value)
            assert conn.call(op="session.create", kind="claude", effort="turbo")["error"] == "option_unavailable"       # not one the program lists
            assert conn.call(op="session.create", kind="synthetic", cwd=str(base))["error"] == "option_unavailable"
            assert conn.call(op="session.create", kind="codex", model="opus")["error"] == "option_unavailable"
            assert conn.call(op="status")["sessions"] == [] and sorted(p.name for p in (base / "workspaces").iterdir()) == ["s0123456789abcdef"]
            # a folder that CONTAINS the host's own directories is a legitimate choice, and deleting its session removes nothing in it
            made = conn.call(op="session.create", kind="claude", cwd=str(base))
            assert made["ok"] == 1, made
            assert conn.call(op="session.purge", session=made["session"], generation=made["generation"])["ok"] == 1
            assert (base / "a_file").exists() and (base / "workspaces" / "s0123456789abcdef").is_dir() and (base / "registry").is_dir()
        finally:
            host.stop()


def test_the_command_line_makes_a_session_in_a_folder_renames_it_and_sends_a_long_text_from_standard_input():
    with private_dir() as directory:
        base = Path(directory).resolve()
        (base / "project").mkdir()
        host, log, received = claude_host(directory)

        def run(*args, **kw):
            return subprocess.run([str(LICO), "--socket", host.socket, *args], capture_output=True, timeout=20, **{"text": True, **kw})
        try:
            made = run("session", "create", "--kind", "claude", "--cwd", "project", "--name", "cli", cwd=base)      # a relative folder is relative to where the command runs
            assert made.returncode == 0, made.stderr
            sid = json.loads(made.stdout)["session"]
            conn = Conn(host.socket)
            assert status_of(conn, sid)["cwd"] == str(base / "project")
            assert json.loads(run("session", "rename", sid, "second").stdout)["name"] == "second"
            assert json.loads(run("session", "rename", sid).stdout)["name"] is None
            assert json.loads(run("session", "options").stdout)["recent"] == [str(base / "project")]
            # 64 KiB through standard input arrives whole; 200 KiB is refused with the reason and nothing is sent
            text = long_text(v1.MAX_INPUT_TEXT)
            assert run("session", "send", sid, "-", input=text).returncode == 0
            wait_for(lambda: sent_users(received))
            assert [user_text(m) for m in sent_users(received)] == [text]
            too_long = run("session", "send", sid, "-", input=long_text(200 * 1024))
            assert too_long.returncode == 1 and "longer than 64 KiB" in too_long.stderr
            assert len(sent_users(received)) == 1 and conn.call(op="status")["ok"] == 1
            # Windows and old Mac line ends become LF; the limit is applied to the LF text, so 64 KiB of LF text sent with CRLF still goes
            assert run("session", "send", sid, "-", input=b"one\r\ntwo\rthree\r\n", text=False).returncode == 0
            crlf = long_text(v1.MAX_INPUT_TEXT).replace("\n", "\r\n").encode()
            assert len(crlf) > v1.MAX_INPUT_TEXT and run("session", "send", sid, "-", input=crlf, text=False).returncode == 0
            wait_for(lambda: len(sent_users(received)) == 3)
            assert [user_text(m) for m in sent_users(received)][1:] == ["one\ntwo\nthree\n", long_text(v1.MAX_INPUT_TEXT)]
        finally:
            host.stop()
