import json
import os
import sys
import time
from pathlib import Path

from lico_measure.p02 import ProtoConn as Conn, start
from lico_measure.rig import REPO, private_dir

from tests.test_account_helpers import account
from tests.test_codex_adapter import wait_for

FAKE = REPO / "tests" / "fixtures" / "login" / "fake_login.py"
URL = "https://login.example.invalid/authorize?state=CANARYURLSTATE7"


def login_host(directory, mode, *more):
    extra = ["--login-bin", sys.executable, "--login-arg", "-I", "--login-arg", str(FAKE), "--login-arg", mode, *more]
    return start(directory, extra=extra, sessions=0, producing=0, history=0)


def login_status(conn, who, **more):
    return conn.call(op="account.login.status", account=who, **more)


def alive(pid):
    return Path(f"/proc/{pid}").exists() and "Z" not in (Path(f"/proc/{pid}/stat").read_text().split(") ")[1][:1] if Path(f"/proc/{pid}/stat").exists() else "Z")


def home_of(directory, who):
    return Path(directory) / "accounts" / who if (Path(directory) / "accounts" / who).exists() else next((Path(directory)).glob(f"**/accounts/{who}"))


def test_login_runs_in_the_accounts_directory_and_nothing_of_it_reaches_log_registry_or_arguments():
    with private_dir() as directory:
        host, log = login_host(directory, "code")
        try:
            conn = Conn(host.socket)
            who = account(conn, "Work")
            assert conn.call(op="account.login.start", account=who)["ok"] == 1
            shown = {}
            wait_for(lambda: shown.update(login_status(conn, who)) or shown.get("needs_code"))
            assert shown["url"] == URL and shown["state"] == "code"            # the escape codes and the prose around the URL are gone
            assert conn.call(op="account.login.input", account=who, code="CANARYCODE9")["ok"] == 1
            wait_for(lambda: login_status(conn, who)["state"] == "succeeded")
            assert login_status(conn, who)["exit_code"] == 0
            account_dir = home_of(directory, who)
            seen = json.loads((account_dir / "home" / "seen.json").read_text())
            assert (account_dir / "codex" / "auth.json").exists() and (account_dir / "home" / "received_code").read_text() == "CANARYCODE9"
            assert set(seen["env"]) <= {"HOME", "CODEX_HOME", "NO_COLOR", "PATH", "LANG", "PWD", "SHLVL", "_", "LC_CTYPE"}, seen["env"]   # no display, no bus, none of the host's variables
            assert seen["cwd"] == str(account_dir / "home") and seen["words"] == ["login"]
            # the code and the URL's state are in no sink of the host: log, registry, standard error, its command line
            files = [p for d in ("log", "registry") for p in (Path(directory) / d).rglob("*") if p.is_file()] + [Path(directory) / "host.err"]
            for path in files:
                data = path.read_bytes()
                assert b"CANARYCODE9" not in data and b"CANARYURLSTATE7" not in data, path
            assert b"CANARY" not in Path(f"/proc/{host.pid}/cmdline").read_bytes() and b"CANARY" not in Path(f"/proc/{host.pid}/environ").read_bytes()
            assert b"CANARYCODE9" not in json.dumps(conn.call(op="status")).encode()
            # a code is refused when none was asked for
            assert conn.call(op="account.login.input", account=who, code="late")["error"] == "no_code_expected"
        finally:
            host.stop()


def test_one_login_per_account_and_cancel_or_timeout_ends_the_process():
    with private_dir() as directory:
        host, log = login_host(directory, "hang", "--login-timeout", "3")
        try:
            conn = Conn(host.socket)
            a, b = account(conn, "One"), account(conn, "Two")
            assert conn.call(op="account.login.start", account=a)["ok"] == 1
            assert conn.call(op="account.login.start", account=a)["error"] == "busy"          # one login per account at a time
            assert conn.call(op="account.login.start", account=b)["ok"] == 1                  # another account is independent
            wait_for(lambda: (home_of(directory, a) / "home" / "pid").exists() and (home_of(directory, b) / "home" / "pid").exists())
            pid_a = int((home_of(directory, a) / "home" / "pid").read_text())
            assert conn.call(op="account.login.cancel", account=a)["ok"] == 1
            wait_for(lambda: login_status(conn, a)["state"] == "cancelled")
            wait_for(lambda: not alive(pid_a))                                                # the process is gone, not just marked
            pid_b = int((home_of(directory, b) / "home" / "pid").read_text())
            wait_for(lambda: login_status(conn, b)["state"] == "timed_out", timeout=10)       # the other one runs out of time
            wait_for(lambda: not alive(pid_b))
            assert conn.call(op="account.login.start", account=a)["ok"] == 1                  # and a finished login can be started again
            assert conn.call(op="account.login.start", account="a" + "0" * 12)["error"] == "unknown_account"
            pass
        finally:
            host.stop()


def test_the_command_line_signs_an_account_in_with_a_pasted_code():
    import subprocess

    from lico_measure.p02 import LICO, cli

    with private_dir() as directory:
        host, log = login_host(directory, "code")
        try:
            conn = Conn(host.socket)
            who = account(conn, "Work")
            done = subprocess.run([str(LICO), "--socket", host.socket, "account", "login", who],
                                  input="CANARYCODE9\n", capture_output=True, text=True, timeout=30)
            assert done.returncode == 0 and URL in done.stderr and "signed in" in done.stderr, (done.stdout, done.stderr)
            assert "CANARYCODE9" not in done.stdout + done.stderr                      # the code is not echoed back
            assert "signed_in" in json.loads(cli(host, "account", "auth", who).stdout)       # the provider's own status command answers, not a guess
        finally:
            host.stop()
