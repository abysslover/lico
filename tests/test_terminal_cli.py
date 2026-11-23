"""M23-P02: `lico terminal ...` through the real executables (a real host, the real command, a real shell on a real pty). What the Rust tests prove inside the host (the emulator's
screen, the signals, the descriptors) is not repeated; here the doors are: the command line builds what the socket takes, the socket answers it, `POST /v1` refuses every terminal
operation (a terminal is a shell that runs as the person), and a host that stops takes its shells with it."""

from __future__ import annotations

import json
import os
import signal
import stat
import subprocess
import time
from pathlib import Path

from lico_measure.p02 import LICO, ProtoConn as Conn, start
from lico_measure.rig import private_dir

from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_host_control import _http

ENV = {"PATH": "/usr/bin:/bin", "LANG": "C.UTF-8", "HOME": "/CANARY-no-home"}


def term(host, *args, cwd=None, timeout=30):
    """`lico terminal ...`: (exit code, standard output, standard error)."""
    done = subprocess.run([str(LICO), "--socket", host.socket, "terminal", *args], env=ENV, capture_output=True, text=True, timeout=timeout, cwd=cwd, stdin=subprocess.DEVNULL)
    return done.returncode, done.stdout, done.stderr


def term_json(host, *args, cwd=None):
    code, out, err = term(host, *args, cwd=cwd)
    assert code == 0, (code, out, err)
    return json.loads(out)


def script(directory: str, body: str) -> str:
    path = Path(directory) / "shell.sh"
    path.write_text(f"#!/bin/sh\n{body}\n")
    path.chmod(0o755)
    return str(path)


def alive(pid: int) -> bool:
    try:
        stat_line = Path(f"/proc/{pid}/stat").read_text()
    except (FileNotFoundError, ProcessLookupError):                 # the process ended between the open and the read
        return False
    return not stat_line.rsplit(") ", 1)[1].startswith("Z")


def host_in(directory: str, ignoring=(), env=None):
    """A host with its own folder; `ignoring`: signals the host is started ignoring (what `nohup` does to the hang-up, and what an ignored signal passes on through `exec`); `env`: variables
    the host process is started with besides its usual ones."""
    token_file = os.path.join(directory, "http.token")
    old = {s: signal.signal(s, signal.SIG_IGN) for s in ignoring}
    try:
        with usual_umask():
            host, _ = start(directory, extra=["--http-port", "auto", "--http-token-file", token_file], sessions=0, producing=0, history=0, retain=1000, env_overrides=env)
    finally:
        for s, handler in old.items():
            signal.signal(s, handler)
    return host, Path(token_file).read_text().strip()


def test_the_command_line_opens_types_shows_resizes_and_closes_a_terminal():
    with private_dir() as directory:
        host, _ = host_in(directory)
        try:
            shell = script(directory, "printf 'hello\\n'; read line; printf 'got:%s\\n' \"$line\"; stty size; read more; exit 5")
            work = Path(directory) / "work"
            work.mkdir()
            opened = term_json(host, "open", "--shell", shell, "--size", "100x30", cwd=work)
            assert opened["ok"] == 1 and opened["tab"] == "t1" and (opened["cols"], opened["rows"]) == (100, 30), opened
            assert opened["cwd"] == str(work.resolve()), "no --cwd: the folder the command runs in"
            tab = opened["tab"]
            wait_for(lambda: "hello" in term(host, "show", tab, "--text")[1])
            assert term_json(host, "send", tab, "abc", "--enter")["written"] == 4
            shown = wait_for(lambda: (t := term(host, "show", tab, "--text")[1]) and "got:abc" in t and t)
            assert "hello\nabc\ngot:abc\n" in shown, shown
            assert "30 100\n" in shown, "the pty has the size the command asked for"
            listed = term_json(host, "list")
            row = listed["terminals"][0]
            assert listed["max_terminals"] == 8 and row["tab"] == tab and row["running"] is True and row["pid"] > 1, listed
            assert term_json(host, "resize", tab, "90x20")["cols"] == 90
            screen = term_json(host, "show", tab)
            assert (screen["cols"], screen["rows"]) == (90, 20) and len(screen["lines"]) == 20 and screen["modes"]["alt_screen"] is False, screen["modes"]
            term(host, "send", tab, "", "--enter")
            wait_for(lambda: term_json(host, "list")["terminals"][0]["running"] is False)
            assert term_json(host, "list")["terminals"][0]["exit"] == 5
            code, out, _ = term(host, "send", tab, "late")
            assert code == 2 and json.loads(out)["error"] == "terminal_ended"
            assert term_json(host, "close", tab)["closed"] is True
            assert term_json(host, "list")["terminals"] == []
            code, out, _ = term(host, "show", tab)
            assert code == 2 and json.loads(out)["error"] == "unknown_tab"
        finally:
            host.stop()


def test_usage_errors_and_the_hosts_refusals_come_back_as_exit_codes():
    with private_dir() as directory:
        host, _ = host_in(directory)
        try:
            for bad in (["open", "--size", "80"], ["open", "--bogus"], ["close"], ["send", "t1"], ["show", "t1", "--scroll"], ["nothing"], []):
                code, out, err = term(host, *bad)
                assert code == 1 and out == "" and "terminal open" in err, (bad, code, out, err)
            code, out, _ = term(host, "open", "--cwd", str(Path(directory) / "nothing-here"))
            assert code == 2 and json.loads(out)["error"] == "folder_missing"
            code, out, _ = term(host, "open", "--shell", str(Path(directory) / "no-shell"))
            assert code == 2 and json.loads(out)["error"] == "shell_missing"
            code, out, _ = term(host, "open", "--size", "501x24")
            assert code == 2 and json.loads(out)["error"] == "bad_request"
            assert term_json(host, "list")["terminals"] == [], "nothing was started"
        finally:
            host.stop()


def test_the_http_door_refuses_every_terminal_operation_and_the_socket_takes_them():
    with private_dir() as directory:
        host, token = host_in(directory)
        try:
            auth = {"Authorization": f"Bearer {token}", "Content-Type": "application/json"}
            caps = Conn(host.socket).call(op="capabilities")
            shell = script(directory, "exec sleep 30")
            valid = {
                "terminal.open": {"cwd": directory, "shell": shell},
                "terminal.list": {},
                "terminal.close": {"tab": "t1"},
                "terminal.input": {"tab": "t1", "text": "x"},
                "terminal.resize": {"tab": "t1", "cols": 80, "rows": 24},
                "terminal.watch": {"tab": "t1"},
                "terminal.text": {"tab": "t1", "from": [0, 0], "to": [0, 1]},
            }
            for op, fields in valid.items():
                assert op in caps["ops"] and op in caps["socket_only_ops"], op
                status, body = _http(host.http_port, "POST", "/v1", auth, json.dumps({"op": op, **fields}).encode())
                assert body.get("ok") != 1 and body.get("error") == "bad_request", (op, status, body)
            assert term_json(host, "list")["terminals"] == [], "the refused door started nothing"
            conn = Conn(host.socket)
            opened = conn.call(op="terminal.open", cwd=directory, shell=shell)
            assert opened["ok"] == 1 and opened["tab"] == "t1", opened
            assert conn.call(op="terminal.input", tab="t1", data="aGk=")["written"] == 2
            conn.close()
        finally:
            host.stop()


def test_a_host_that_stops_takes_its_shells_and_their_children_with_it():
    with private_dir() as directory:
        host, _ = host_in(directory)
        marker = Path(directory) / "child.pid"
        shell = script(directory, f"sleep 300 &\necho $! > {marker}\nwait")
        try:
            tab = term_json(host, "open", "--shell", shell)["tab"]
            child = int(wait_for(lambda: marker.exists() and marker.read_text().strip() or None))
            pid = term_json(host, "list")["terminals"][0]["pid"]
            assert alive(child) and alive(pid)
        finally:
            host.stop()                                         # the host is killed, not asked: the parent-death signal ends the shell, and the shell's hang-up its child
        deadline = time.time() + 5
        while time.time() < deadline and (alive(child) or alive(pid)):
            time.sleep(0.05)
        assert not alive(pid), "the shell outlived its host"
        assert not alive(child), "what the shell started outlived its host"
        assert tab == "t1"


def test_a_shell_that_ignores_the_hangup_still_does_not_outlive_its_host():
    with private_dir() as directory:
        host, _ = host_in(directory)
        marker = Path(directory) / "child.pid"
        shell = script(directory, f"trap '' HUP\nsleep 300 &\necho $! > {marker}\nwait")
        child = pid = 0
        try:
            term_json(host, "open", "--shell", shell)
            child = int(wait_for(lambda: marker.exists() and marker.read_text().strip() or None))
            pid = term_json(host, "list")["terminals"][0]["pid"]
            assert alive(pid)
        finally:
            host.stop()
        # the pty hangs up when the host goes, and this shell ignores the hang-up: only the parent-death signal ends it
        deadline = time.time() + 5
        while time.time() < deadline and alive(pid):
            time.sleep(0.05)
        try:
            assert not alive(pid), "the shell outlived its host"
        finally:
            if child and alive(child):
                os.kill(child, 9)                               # what it started and that also ignores the hang-up is the known limit: it is not the host's to find


def test_a_shell_starts_with_the_default_action_for_the_signals_its_host_ignores():
    """A host run under `nohup` ignores the hang-up, and an ignored signal stays ignored through `exec`: without a reset the shell could not trap it and what it starts would
    outlive its terminal. The shell reads the masks of ignored signals (bit n-1 is signal n) of itself and of its parent, the host."""
    with private_dir() as directory:
        host, _ = host_in(directory, ignoring=(signal.SIGHUP, signal.SIGINT, signal.SIGQUIT))
        try:
            mask = "sed -n 's/^SigIgn:[[:space:]]*//p'"
            shell = script(directory, f"printf 'own=%s parent=%s\\n' \"$({mask} /proc/$$/status)\" \"$({mask} /proc/$PPID/status)\"; exit 0")
            tab = term_json(host, "open", "--shell", shell)["tab"]
            shown = wait_for(lambda: (t := term(host, "show", tab, "--text")[1]) and "parent=" in t and t)
        finally:
            host.stop()
    own, parent = (int(shown.split(f"{name}=")[1].split()[0], 16) for name in ("own", "parent"))
    hang_up, interrupt, quit_ = 1 << (signal.SIGHUP - 1), 1 << (signal.SIGINT - 1), 1 << (signal.SIGQUIT - 1)
    assert parent & hang_up, f"the premise: the host was started ignoring the hang-up ({parent:x})"
    assert own & (hang_up | interrupt | quit_) == 0, f"the shell was started ignoring signals ({own:x})"


def test_a_shell_is_not_given_the_hosts_secrets_but_is_given_what_a_person_needs():
    """M23-P09: the shell gets a short list of names. The host is started with canary variables that look like what a launcher or a person's keys put in an environment; none of them
    may reach the shell, and `HOME`, `PATH` and the locale still do. The test prints only the names it asks about, never the environment."""
    canary = "P09-CANARY-NOT-A-SECRET"
    secret_names = ["ANTHROPIC_API_KEY", "OPENAI_API_KEY", "HF_TOKEN", "AWS_SECRET_ACCESS_KEY", "ORCA_AGENT_HOOK_TOKEN", "LICO_TOKEN", "P09_PROBE", "https_proxy"]
    with private_dir() as directory:
        host, _ = host_in(directory, env={**{name: canary for name in secret_names}, "LANG": "C.UTF-8"})
        try:
            asked = " ".join(secret_names)
            body = (f"for n in {asked}; do eval \"v=\\${{$n-unset}}\"; printf '%s=%s\\n' \"$n\" \"$v\"; done; "
                    "printf 'home=%s path=%s lang=%s\\n' \"${HOME:+set}\" \"${PATH:+set}\" \"${LANG:+set}\"; printf 'end\\n'; read hold")
            tab = term_json(host, "open", "--shell", script(directory, body))["tab"]
            shown = wait_for(lambda: (t := term(host, "show", tab, "--text")[1]) and "\nend" in t and t)
        finally:
            host.stop()
    assert canary not in shown, "a canary variable of the host reached the shell"
    for name in secret_names:
        assert f"{name}=unset" in shown, f"{name} is not unset in the shell:\n{shown}"
    assert "home=set path=set lang=set" in shown, shown
