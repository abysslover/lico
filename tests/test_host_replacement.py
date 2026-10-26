"""The window brings the host to its own build and keeps the sessions (M02-P01-S04): an older host is stopped and replaced by itself, the sessions that
were alive are resumed under a new generation, and a disconnected session can be resumed from the window. The provider is the codex simulator behind a
`codex` script on the PATH of host and window, so the new host that the window starts uses it too."""

import json
import os
import sys
import time
from pathlib import Path

from lico_measure.captures import ProductWindow
from lico_measure.p02 import LICO, ProtoConn as Conn, start
from lico_measure.rig import private_dir

from tests.test_codex_adapter import FIXTURE, SIM, wait_for
from tests.test_stale_host import alive, fnv, older_build, status as status_now

NEW_LINE = "Restarted the host (new build); resumed {n} sessions."


def fake_codex(directory):
    """A directory with a `codex` that is the simulator."""
    folder = Path(directory) / "bin"
    folder.mkdir()
    script = folder / "codex"
    script.write_text(f'#!/bin/sh\nexec {sys.executable} -I {SIM} --received {directory}/received.jsonl --fixture {FIXTURE} "$@"\n')
    script.chmod(0o755)
    return str(folder)


def status(w):
    """The host's status; empty while no host answers (between the old host's end and the new one's start)."""
    try:
        return status_now(w)
    except ValueError:
        return {}


def sessions_of(w):
    return {s["session"]: s for s in status(w)["sessions"]}


def registered(w):
    try:
        return {e["session"]: e for e in json.loads((Path(w.directory) / "state" / "lico" / "registry" / "sessions.json").read_text())["sessions"]}
    except (OSError, ValueError, KeyError):
        return {}


def make_sessions(count, crashed=0):
    """Before the window starts: `count` running codex sessions on the first host, and then `crashed` more whose provider has gone."""
    def hook(w):
        conn = Conn(w.socket)
        w.made = [conn.call(op="session.create", kind="codex") for _ in range(count)]
        w.crashed = [conn.call(op="session.create", kind="codex") for _ in range(crashed)]
        for m in w.crashed:
            conn.call(op="input", session=m["session"], generation=m["generation"], request_id="c-1", text="crash")
        wait_for(lambda: all(registered(w).get(m["session"], {}).get("provider_session") for m in w.made + w.crashed), timeout=15)
        wait_for(lambda: all(sessions_of(w)[m["session"]]["state"] == "disconnected" for m in w.crashed), timeout=15)
    return hook


def test_a_window_of_a_newer_build_replaces_the_host_by_itself_and_resumes_the_sessions_that_were_alive():
    with private_dir() as directory:
        def hook(w):
            make_sessions(2, crashed=1)(w)
            w.providers_before = children_of(w.old_pid)                              # the old host's provider processes, by pid
            assert len(w.providers_before) >= 2
        with ProductWindow(host_binary=older_build(directory), path_prefix=fake_codex(directory), before_window=hook) as w:
            old = w.old_pid
            wait_for(lambda: status(w).get("pid") != old and status(w).get("build") == fnv(LICO), timeout=30)
            assert not alive(old)                                                    # asked over its socket, not killed by name
            alive_ids = [m["session"] for m in w.made]
            wait_for(lambda: all(sessions_of(w).get(i, {}).get("state") == "running" for i in alive_ids), timeout=30)
            found = sessions_of(w)
            for m in w.made:
                assert found[m["session"]]["generation"] == m["generation"] + 1      # a new generation: everything aimed at the old run is stale
            gone = w.crashed[0]
            assert found[gone["session"]]["state"] == "disconnected" and found[gone["session"]]["generation"] == gone["generation"]   # it stays as it was
            assert NEW_LINE.format(n=2) in w.ui()["header"]
            # no provider of the old host is left; the new host's providers are exactly those of the two resumed sessions
            wait_for(lambda: not any(alive(pid) for pid in w.providers_before), timeout=10)
            assert len(children_of(status(w)["pid"])) == 2
            # the resumed sessions work, and their history carries on
            conn = Conn(w.socket)
            for m in w.made:
                conn.call(op="input", session=m["session"], generation=m["generation"] + 1, request_id="after", text="after the update")
            wait_for(lambda: all(any("echo: after the update" in line[1] for line in conn.call(op="snapshot", session=m["session"])["lines"]) for m in w.made), timeout=15)


def test_the_stop_that_names_a_build_only_stops_that_build():
    with private_dir() as directory:
        host, log = start(directory, sessions=0, producing=0, history=0)
        try:
            conn = Conn(host.socket)
            build = conn.call(op="status")["build"]
            refused = conn.call(op="host.stop", force=True, build="0000000000000000")
            assert refused["ok"] == 0 and refused["error"] == "build_changed"
            time.sleep(0.6)
            assert conn.call(op="status")["build"] == build                          # the host that was asked is the one that is there: it was not stopped
            assert conn.call(op="host.stop", force=True, build=build)["ok"] == 1
            wait_for(lambda: host.proc.poll() is not None, timeout=10)
        finally:
            host.stop()


def test_a_disconnected_session_is_resumed_from_the_window_by_the_menu_entry_and_by_ctrl_r():
    with private_dir() as directory:
        with ProductWindow(host_binary=LICO, path_prefix=fake_codex(directory), before_window=make_sessions(1, crashed=1)) as w:
            running, gone = w.made[0], w.crashed[0]
            wait_for(lambda: len(w.ui()["rows"]) == 2, timeout=15)
            w.key("alt+1")
            w.key("alt+s")
            entry = next(i for i in w.ui()["menu"]["items"] if "Resume session" in i["label"])
            assert entry["key"] == "Ctrl+R" and entry["enabled"] is False and entry["reason"] == "the session is running"
            w.key("Escape")
            order = [r["session"] for r in w.ui()["rows"]]
            w.key(f"alt+{order.index(gone['session']) + 1}")
            w.key("alt+s")
            entry = next(i for i in w.ui()["menu"]["items"] if "Resume session" in i["label"])
            assert entry["enabled"] is True and not entry.get("reason")
            w.key("Escape")
            w.key("ctrl+r")
            wait_for(lambda: sessions_of(w)[gone["session"]]["state"] == "running", timeout=15)
            assert sessions_of(w)[gone["session"]]["generation"] == gone["generation"] + 1
            assert sessions_of(w)[running["session"]]["generation"] == running["generation"]        # the other one was not touched
            w.key("alt+s")
            entry = next(i for i in w.ui()["menu"]["items"] if "Resume session" in i["label"])
            assert entry["enabled"] is False and entry["reason"] == "the session is running"


def header_has(w, text):
    return text in w.ui()["header"]


def start_turn(count, seconds):
    """Before the window: `count` idle codex sessions and then a turn of about `seconds` seconds in the first one."""
    def hook(w):
        make_sessions(count)(w)
        first = w.made[0]
        Conn(w.socket).call(op="input", session=first["session"], generation=first["generation"], request_id="t-1", text=f"slow:{int(seconds * 10)}")
        wait_for(lambda: sessions_of(w)[first["session"]].get("activity") == "working", timeout=15)
    return hook


def test_a_running_turn_is_waited_for_and_then_the_host_is_replaced_without_cutting_it():
    with private_dir() as directory:
        env = {"LICO_UPDATE_TURN_WAIT_MS": "30000"}
        with ProductWindow(host_binary=older_build(directory), path_prefix=fake_codex(directory), before_window=start_turn(2, 6), env_extra=env) as w:
            old = w.old_pid
            wait_for(lambda: header_has(w, "after the running turn"), timeout=10)
            assert alive(old) and status(w)["pid"] == old                              # nothing is cut while the turn runs
            wait_for(lambda: status(w).get("pid") != old, timeout=30)
            wait_for(lambda: header_has(w, NEW_LINE.format(n=2)), timeout=30)
            assert "interrupted" not in w.ui()["header"]                                # the turn had ended by then: nothing was lost


def test_a_turn_that_does_not_end_is_cut_after_the_limit_and_the_line_counts_it():
    with private_dir() as directory:
        env = {"LICO_UPDATE_TURN_WAIT_MS": "2000"}
        with ProductWindow(host_binary=older_build(directory), path_prefix=fake_codex(directory), before_window=start_turn(1, 120), env_extra=env) as w:
            old = w.old_pid
            wait_for(lambda: status(w).get("pid") != old and status(w).get("build") == fnv(LICO), timeout=30)
            wait_for(lambda: header_has(w, "Restarted the host (new build); resumed 1 session; 1 turn was interrupted."), timeout=30)
            found = sessions_of(w)[w.made[0]["session"]]
            assert found["state"] == "running" and found["generation"] == w.made[0]["generation"] + 1 and found.get("activity") == "idle"   # back, without the turn


def test_an_approval_that_was_waiting_is_not_kept_and_the_line_says_so():
    with private_dir() as directory:
        def hook(w):
            make_sessions(1)(w)
            first = w.made[0]
            Conn(w.socket).call(op="input", session=first["session"], generation=first["generation"], request_id="a-1", text="fixture")
            wait_for(lambda: sessions_of(w)[first["session"]].get("approvals"), timeout=15)
        env = {"LICO_UPDATE_TURN_WAIT_MS": "2000"}
        with ProductWindow(host_binary=older_build(directory), path_prefix=fake_codex(directory), before_window=hook, env_extra=env) as w:
            old = w.old_pid
            wait_for(lambda: status(w).get("pid") != old and status(w).get("build") == fnv(LICO), timeout=30)
            wait_for(lambda: header_has(w, "Restarted the host (new build); resumed 1 session; 1 turn was interrupted; 1 approval waiting for an answer was not kept."), timeout=30)
            assert not sessions_of(w)[w.made[0]["session"]].get("approvals")                       # the provider does not ask again after a resume (measured with codex and claude)


LOGIN = ["--login-bin", sys.executable, "--login-arg", "-I", "--login-arg", str(Path(__file__).parent / "fixtures" / "login" / "fake_login.py"), "--login-arg", "code"]


def sign_in_started(w):
    conn = Conn(w.socket)
    w.account = conn.call(op="account.create", provider="codex", label="Work")["account"]
    assert conn.call(op="account.login.start", account=w.account)["ok"] == 1
    wait_for(lambda: conn.call(op="account.login.status", account=w.account).get("needs_code"), timeout=15)


def test_a_sign_in_in_progress_keeps_the_host_and_the_line_says_why():
    with private_dir() as directory:
        env = {"LICO_UPDATE_LOGIN_WAIT_MS": "2500"}
        with ProductWindow(host_binary=older_build(directory), host_args=LOGIN, before_window=sign_in_started, env_extra=env) as w:
            old = w.old_pid
            wait_for(lambda: header_has(w, "a sign-in was still in progress"), timeout=15)
            header = w.ui()["header"]
            assert "was not replaced" in header and "lico host stop" in header           # 014: the stop line only when the replacement could not be done, and why
            assert alive(old) and status(w)["pid"] == old                                # the sign-in was never ended
            assert Conn(w.socket).call(op="account.login.status", account=w.account)["state"] == "code"


def test_a_sign_in_that_ends_during_the_wait_lets_the_replacement_go_on():
    with private_dir() as directory:
        env = {"LICO_UPDATE_LOGIN_WAIT_MS": "30000"}
        with ProductWindow(host_binary=older_build(directory), host_args=LOGIN, before_window=sign_in_started, env_extra=env) as w:
            old = w.old_pid
            wait_for(lambda: header_has(w, "after the sign-in in progress"), timeout=15)
            assert alive(old)
            assert Conn(w.socket).call(op="account.login.input", account=w.account, code="CANARYCODE9")["ok"] == 1
            wait_for(lambda: status(w).get("pid") != old and status(w).get("build") == fnv(LICO), timeout=30)
            wait_for(lambda: header_has(w, NEW_LINE.format(n=0)), timeout=15)


def fake_host(directory):
    folder = Path(directory) / "fake"
    folder.mkdir()
    script = folder / "host"
    script.write_text(f'#!/bin/sh\nexec {sys.executable} -I {Path(__file__).parent / "fixtures" / "old_host" / "fake_host.py"} "$@"\n')
    script.chmod(0o755)
    return script


import pytest


@pytest.mark.parametrize("how", ["host.stop", "sigterm"])
def test_the_helpers_of_a_provider_that_died_with_its_old_host_are_ended_too(how):
    """The real old build's providers die with their host (parent-death signal) before the window looks, so only the helpers in the dead leader's group are left."""
    with private_dir() as directory:
        note = Path(directory) / "provider.pids"
        env = {"FAKE_HOST_BUILD": "00000000000000aa", "FAKE_HOST_PROVIDER": str(note)}
        if how == "host.stop":
            env["FAKE_HOST_STOP"] = "exit"                                                                   # answers ok and exits at once; without it host.stop is an unknown op: SIGTERM by pid
        found = {}

        def note_pids(w):
            wait_for(lambda: note.exists() and len(note.read_text().split()) == 2, timeout=10)
            found["child"], found["helper"] = (int(n) for n in note.read_text().split())
            assert alive(found["child"]) and alive(found["helper"])
        try:
            with ProductWindow(host_binary=fake_host(directory), env_extra=env, before_window=note_pids) as w:
                old = w.old_pid
                wait_for(lambda: status(w).get("build") == fnv(LICO), timeout=30)
                wait_for(lambda: not alive(found["child"]) and not alive(found["helper"]), timeout=10)
                assert not alive(old) and alive(os.getpid()) and w.alive()                                       # only the old host's own processes are gone
                wait_for(lambda: header_has(w, NEW_LINE.format(n=0)), timeout=15)
                assert "could not be ended" not in w.ui()["header"]
        finally:
            for pid in found.values():
                if alive(pid):
                    os.kill(pid, 9)


def test_a_host_too_old_for_host_stop_is_ended_by_the_pid_its_status_named_and_replaced():
    with private_dir() as directory:
        child = Path(directory) / "child.pid"
        env = {"FAKE_HOST_BUILD": "none", "FAKE_HOST_CHILD": str(child)}                                         # no build id, host.stop is an unknown op
        def note_child(w):                                                                                       # before the window starts and replaces the host
            w.leftover = int(child.read_text())
            assert alive(w.leftover)
        with ProductWindow(host_binary=fake_host(directory), env_extra=env, before_window=note_child) as w:
            old, leftover = w.old_pid, w.leftover
            wait_for(lambda: not alive(old), timeout=15)
            wait_for(lambda: status(w).get("build") == fnv(LICO), timeout=20)
            wait_for(lambda: header_has(w, NEW_LINE.format(n=0)), timeout=15)
            assert not alive(leftover)                                                                         # what the old host left running is ended too, by its pid


def children_of(pid):
    found = []
    for entry in Path("/proc").iterdir():
        try:
            if entry.name.isdigit() and (entry / "stat").read_text().split(") ")[1].split()[1] == str(pid):
                found.append(int(entry.name))
        except (OSError, IndexError):
            pass
    return found


def test_stopping_a_host_ends_its_providers_and_what_they_started_instead_of_leaving_them_to_finish_their_turn():
    with private_dir() as directory:
        tool = Path(directory) / "tool.pid"
        extra = ["--codex", sys.executable, "--codex-arg", "-I", "--codex-arg", str(SIM), "--codex-arg", "--received", "--codex-arg", f"{directory}/received.jsonl",
                 "--codex-arg", "--fixture", "--codex-arg", str(FIXTURE), "--codex-arg", "--survive-eof",         # a provider that stays up after its input ends
                 "--codex-arg", "--grandchild", "--codex-arg", str(tool)]                                         # and a process it started, which the parent-death signal does not reach
        host, log = start(directory, extra=extra, sessions=0, producing=0, history=0)
        started = None
        try:
            conn = Conn(host.socket)
            conn.call(op="session.create", kind="codex")
            wait_for(lambda: tool.exists() and tool.read_text(), timeout=10)
            started = int(tool.read_text())
            providers = children_of(host.pid)
            assert len(providers) == 1 and alive(providers[0]) and alive(started)
            assert conn.call(op="host.stop", force=True)["ok"] == 1
            wait_for(lambda: host.proc.poll() is not None, timeout=10)
            wait_for(lambda: not alive(providers[0]) and not alive(started), timeout=5)                        # not left behind to meet a resumed session
        finally:
            host.stop()
            for pid in children_of(host.pid) + ([started] if started and alive(started) else []):
                os.kill(pid, 9)


def test_a_host_that_ends_without_an_answer_is_not_called_replaced_by_another_window():
    with private_dir() as directory:
        env = {"FAKE_HOST_BUILD": "00000000000000aa", "FAKE_HOST_STOP": "die"}                                  # it ends when asked to stop, without a word
        with ProductWindow(host_binary=fake_host(directory), env_extra=env) as w:
            wait_for(lambda: status(w).get("build") == fnv(LICO), timeout=30)
            wait_for(lambda: header_has(w, "The host ended and a new one answers (new build)."), timeout=15)
            assert "another window" not in w.ui()["header"]                                                      # nobody else was seen to do it


def test_a_host_that_another_window_replaced_at_once_is_still_a_host_this_window_ended():
    """The window that stopped the old host must not need to see the socket empty: a new host (another window's) can answer in the same instant."""
    with private_dir() as directory:
        env = {"FAKE_HOST_BUILD": "00000000000000aa", "FAKE_HOST_STOP": "handover", "FAKE_HOST_SUCCESSOR": fnv(LICO), "FAKE_HOST_LIFETIME": "60"}
        with ProductWindow(host_binary=fake_host(directory), env_extra=env) as w:
            old = w.old_pid
            try:
                wait_for(lambda: status(w).get("build") == fnv(LICO), timeout=30)
                try:
                    wait_for(lambda: header_has(w, NEW_LINE.format(n=0)), timeout=15)                              # this window stopped the old host: it is the one that restarts
                except AssertionError:
                    raise AssertionError(f"the old host is gone and a new one answers, but this window did not take it as its own restart: header={w.ui()['header']!r}") from None
                assert status(w)["pid"] != old and not alive(old)
            finally:
                if status(w).get("pid") not in (None, old):
                    os.kill(status(w)["pid"], 9)


def test_a_bad_request_to_session_create_says_the_host_is_an_older_build():
    with private_dir() as directory:
        env = {"FAKE_HOST_BUILD": fnv(LICO), "FAKE_HOST_ERROR": "bad_request"}                                  # the same build, so the window leaves it alone
        with ProductWindow(host_binary=fake_host(directory), env_extra=env) as w:
            w.key("ctrl+n")                                                                                    # the one account of the stand-in host is chosen
            w.key("Return")                                                                                    # Enter on the folder row starts
            panel = w.ui()["panel"]
            assert any("The session was not started. The host does not understand this request: it is an older build." in line for line in panel["body"]), panel
            assert not any("bad_request" in line for line in panel["body"])


def host_processes(w):
    """Hosts of this window's socket that are alive, found by their command line (our own socket path is unique to this test)."""
    found = []
    for entry in Path("/proc").iterdir():
        try:
            words = (entry / "cmdline").read_bytes().split(b"\0")
            state = (entry / "stat").read_text().split(") ")[1][0]
        except (OSError, IndexError):
            continue
        if entry.name.isdigit() and state != "Z" and b"host" in words and w.socket.encode() in words:
            found.append(int(entry.name))
    return found


class Second:
    """A second `lico gui` on the first window's display, host and state, with the same `ui` dump as the first one."""

    def __init__(self, w):
        import queue
        import subprocess
        import threading
        self.trace = Path(w.directory) / "second.trace"
        self.proc = subprocess.Popen([str(LICO), "gui"], env={**w.env, "LICO_UPDATE_TRACE": str(self.trace)}, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        self.events = queue.Queue()

        def read():
            for line in self.proc.stdout:
                try:
                    self.events.put(json.loads(line))
                except ValueError:
                    pass
        threading.Thread(target=read, daemon=True).start()

    def header(self):
        import queue
        try:
            self.proc.stdin.write("ui\n")
            self.proc.stdin.flush()
            end = time.monotonic() + 5
            while time.monotonic() < end:
                try:
                    event = self.events.get(timeout=0.5)
                except queue.Empty:
                    continue
                if event.get("ev") == "ui":
                    return event["header"]
        except (OSError, ValueError):
            return "(the window cannot be asked: it has ended)"
        return "(no ui dump within 5 s)"


def explain(w, second):
    """What the next failure needs to be understood without a rerun: the host, the sessions, both headers, the processes of this socket."""
    try:
        host = status(w)
        sessions = {k: (v.get("state"), v.get("generation"), v.get("activity")) for k, v in sessions_of(w).items()} if host else {}
        host = {key: host.get(key) for key in ("pid", "build", "logins_running")}
    except Exception as why:                                                                  # a diagnosis never hides the failure it describes
        host, sessions = f"(status failed: {why!r})", {}
    try:
        first = w.ui()["header"]
    except Exception as why:
        first = f"(no ui dump: {why!r})"
    def trace(path):
        try:
            return Path(path).read_text()
        except OSError:
            return "(no trace)\n"
    return (f"host={host}\nsessions={sessions}\nfirst window header={first!r}\nsecond window header={second.header()!r}\n"
            f"host processes of this socket={host_processes(w)} old host alive={alive(w.old_pid)} window build={fnv(LICO)}\n"
            f"--- first window trace\n{trace(second.first)}--- second window trace\n{trace(second.trace)}")


def reached(what, condition, timeout, w, second):
    try:
        wait_for(condition, timeout=timeout)
    except AssertionError:
        raise AssertionError(f"not reached within {timeout} s: {what}\n{explain(w, second)}") from None


def test_two_windows_at_once_make_one_host_and_one_set_of_resumes():
    other = []

    def hook(w):
        start_turn(2, 4)(w)
        other.append(Second(w))                                                                 # started in the same instant as the first window
        other[0].first = Path(w.env["LICO_UPDATE_TRACE"])
    with private_dir() as directory:
        env = {"LICO_UPDATE_TURN_WAIT_MS": "30000", "LICO_UPDATE_TRACE": str(Path(directory) / "first.trace")}
        try:
            with ProductWindow(host_binary=older_build(directory), path_prefix=fake_codex(directory), before_window=hook, env_extra=env) as w:
                old, second = w.old_pid, other[0]
                reached("the host is the new build", lambda: status(w).get("pid") != old and status(w).get("build") == fnv(LICO), 40, w, second)
                alive_ids = [m["session"] for m in w.made]
                reached("every session is running again", lambda: all(sessions_of(w).get(i, {}).get("state") == "running" for i in alive_ids), 40, w, second)
                time.sleep(2)                                                                   # a second set of resumes would show by now
                found = sessions_of(w)
                for m in w.made:
                    assert found[m["session"]]["generation"] == m["generation"] + 1, f"resumed more than once\n{explain(w, second)}"   # once, by the window that stopped the old host
                assert not alive(old) and len(host_processes(w)) == 1, f"the old host is alive or there is not one host\n{explain(w, second)}"
                kinds = {"restarted": NEW_LINE.format(n=2), "replaced by another window": "replaced by another window", "host ended": "The host ended and a new one answers"}
                reached("a header says what happened", lambda: any(header_has(w, text) for text in kinds.values()), 20, w, second)
                print("FIRST_WINDOW_HEADER", [kind for kind, text in kinds.items() if header_has(w, text)], "second:", second.header())      # tallied by the loaded runs (pytest -s)
        finally:
            for p in other:
                p.proc.terminate()
                p.proc.wait(timeout=10)


def test_a_window_that_is_already_open_follows_a_host_that_another_window_replaced_and_starts_nothing():
    import subprocess
    with private_dir() as directory:
        newer = older_build(directory)                                              # another build than window A: window B runs it and replaces A's host
        with ProductWindow(host_binary=LICO, path_prefix=fake_codex(directory), before_window=make_sessions(2)) as a:
            h1 = a.old_pid
            wait_for(lambda: len(a.ui()["rows"]) == 2, timeout=15)
            assert status(a)["pid"] == h1 and "Restarted" not in a.ui()["header"]      # same build: window A did nothing at its start
            b = subprocess.Popen([str(newer), "gui"], env=a.env, stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, text=True)
            try:
                wait_for(lambda: status(a).get("pid") not in (None, h1) and status(a).get("build") == fnv(newer), timeout=40)
                h2 = status(a)["pid"]
                ids = [m["session"] for m in a.made]
                wait_for(lambda: all(sessions_of(a).get(i, {}).get("state") == "running" for i in ids), timeout=40)
                # A is attached to the new host: its own list shows the new generations (+1), which only H2 reports
                wait_for(lambda: sorted(r["generation"] for r in a.ui()["rows"]) == sorted(m["generation"] + 1 for m in a.made), timeout=20)
                assert "The host was replaced" in a.ui()["header"]
                time.sleep(6)                                                          # window A has a different build than H2 and must leave it alone
                assert status(a)["pid"] == h2 and not alive(h1) and len(host_processes(a)) == 1
                assert a.alive() and a.proc.poll() is None
                for m in a.made:
                    found = sessions_of(a)[m["session"]]
                    assert found["generation"] == m["generation"] + 1                  # resumed once, by window B only
            finally:
                b.terminate()
                b.wait(timeout=10)
