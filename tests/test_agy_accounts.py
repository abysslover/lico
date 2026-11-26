"""M25-P11: `agy` is a provider of accounts, tried with the simulator of `agy acp` and a double of its sign-in (`tests/fixtures/login/fake_agy_login.py`), never the real program.

An account of agy is a private home (the program keeps its own state under `$HOME/.gemini/`, so a home of its own is what tells two accounts apart); its sign-in is the program's own
(`agy acp --login`: an address, then a code pasted in) and only that; a session of agy takes the account it was asked for, else the default one, else (no account of agy at all) the person's own
home as before, and it keeps its account when it is resumed. What cannot be tried here is the real Google sign-in and where the real program keeps its token afterwards: the owner's hands-on check
(gate G3).
"""

import json
import sys
from pathlib import Path

from lico_measure.p02 import ProtoConn as Conn, start
from lico_measure.rig import REPO, files_under, private_dir

from tests.account_helpers import account
from tests.test_agy_session import KEPT, folder_in, lines_of, new_agy, simulator_args, started_as, status_of
from tests.test_codex_adapter import usual_umask, wait_for
from tests.test_recovery import hard_stop, registered

FAKE = REPO / "tests" / "fixtures" / "login" / "fake_agy_login.py"
URL = "https://accounts.example.invalid/o/oauth2/auth?state=CANARYURLSTATE7"
CONVERSATION = "3f2a9c1e-7b4d-4e55-9a60-1d2c3b4a5f66"


def host_with(directory, mode="code", *more):
    """A host whose `agy` is the simulator (which marks the home it was given) and whose sign-ins are the double's; its own home is `<directory>/home` and it has a variable of the XDG kind."""
    received = Path(directory) / "agy_received.txt"
    extra = [*simulator_args(received), "--agy-arg", "--mark-home", "--login-bin", sys.executable, "--login-arg", "-I", "--login-arg", str(FAKE), "--login-arg", mode, *more]
    with usual_umask():
        host, log = start(directory, extra=extra, sessions=0, producing=0, history=0, retain=100000, env_overrides={"XDG_CONFIG_HOME": str(Path(directory) / "xdg")})
    return host, log, received


def home_of(directory, who):
    return Path(directory) / "accounts" / who / "home"


def marks(place):
    """How many times the program was started in a home (`--mark-home`)."""
    mark = Path(place) / ".gemini" / "antigravity-cli" / "started"
    return len(mark.read_text().split()) if mark.exists() else 0


def login_status(conn, who):
    return conn.call(op="account.login.status", account=who)


def test_an_account_of_agy_is_a_private_home_with_no_ssh_and_signs_in_through_the_programs_own_login_and_nothing_of_it_is_kept():
    with private_dir() as directory:
        host, log, _received = host_with(directory)
        try:
            conn = Conn(host.socket)
            who = account(conn, "Google", provider="agy")
            listed = [a for a in conn.call(op="account.list")["accounts"] if a["account"] == who][0]
            assert listed["provider"] == "agy" and listed["ssh_host"] is None and listed["default"] is True, "the first account of agy is its default"
            assert (home_of(directory, who).stat().st_mode & 0o777) == 0o700
            refused = conn.call(op="account.create", provider="agy", label="Far", ssh_host="gpu-box")
            assert refused.get("error") == "bad_request", "the program is local, as an agy session is"
            assert conn.call(op="account.create", provider="other", label="No").get("error") == "bad_request"
            assert conn.call(op="account.login.start", account=who, method="device").get("error") == "bad_request", "one way only: the program's own sign-in"

            assert conn.call(op="account.login.start", account=who)["ok"] == 1
            shown = {}
            wait_for(lambda: shown.update(login_status(conn, who)) or shown.get("needs_code"))
            assert shown["url"] == URL and shown["state"] == "code", "the address above the prompt is the sign-in address and the prompt without a newline is a request for a code"
            assert conn.call(op="account.login.input", account=who, code="CANARYCODE9")["ok"] == 1
            wait_for(lambda: login_status(conn, who)["state"] == "succeeded")
            home = home_of(directory, who)
            seen = json.loads((home / "seen.json").read_text())
            assert seen["words"] == ["acp", "--login", "--disable-telemetry"] and seen["cwd"] == str(home)
            assert set(seen["env"]) <= {"HOME", "NO_COLOR", "BROWSER", "PATH", "LANG", "PWD", "SHLVL", "_", "LC_CTYPE"}, seen["env"]   # no display, no bus, no variable of another provider, none of the host's
            assert (home / ".gemini" / "antigravity-cli" / "signed_in").exists(), "what the program keeps lands in the account's home, not in the person's own"
            assert not (Path(directory) / "home" / ".gemini").exists()
            for path, data in files_under(Path(directory) / "log", Path(directory) / "registry", Path(directory) / "host.err"):
                assert b"CANARYCODE9" not in data and b"CANARYURLSTATE7" not in data, path
            assert b"CANARY" not in Path(f"/proc/{host.pid}/cmdline").read_bytes()
            assert conn.call(op="account.login.input", account=who, code="late")["error"] == "no_code_expected"
            # removing the account removes the home with what the program kept in it
            assert conn.call(op="account.remove", account=who, sessions="end")["ok"] == 1
            assert not home.exists()
        finally:
            host.stop()


def test_a_session_of_agy_takes_the_account_it_was_asked_for_else_the_default_one_else_the_persons_own_home_and_keeps_it_when_resumed():
    with private_dir() as directory:
        folder = folder_in(directory)
        own_home = Path(directory) / "home"
        Path(directory, "agy_received.txt.sessions").write_text(json.dumps({CONVERSATION: [{"role": "user", "text": "earlier"}, {"role": "agent", "text": "answer"}]}), encoding="utf-8")
        host, log, received = host_with(directory)
        try:
            conn = Conn(host.socket)
            # no account of agy at all: the person's own home, the names of the person's own environment, no account on the session (as before)
            alone, _ = new_agy(conn, folder)
            first = wait_for(lambda: started_as(received))[0]
            assert set(first["env"]) <= KEPT and "XDG_CONFIG_HOME" in first["env"], first["env"]
            assert marks(own_home) == 1 and "account" not in status_of(conn, alone)

            one, two = account(conn, "One", provider="agy"), account(conn, "Two", provider="agy")
            # none asked for: the default (the first), in its home and with no XDG name of the person's
            sid, generation = new_agy(conn, folder)
            assert status_of(conn, sid)["account"] == one
            wait_for(lambda: marks(home_of(directory, one)) == 1)
            env = started_as(received)[1]["env"]
            assert set(env) <= {"PATH", "LANG", "HOME", "LC_CTYPE"} and "XDG_CONFIG_HOME" not in env, env
            assert marks(own_home) == 1, "the person's own home was not used again"
            # the account asked for
            other, _ = new_agy(conn, folder, account=two)
            assert status_of(conn, other)["account"] == two
            wait_for(lambda: marks(home_of(directory, two)) == 1)
            # an account of another provider, or none that exists, is no account of agy
            claude = account(conn, "Not agy", provider="claude")
            for wrong in (claude, "someone"):
                refused = conn.call(op="session.create", kind="agy", cwd=str(folder), account=wrong)
                assert refused.get("error") == "unknown_account", (wrong, refused)
            # a conversation of the person's own (the Chats list reads only that home) is brought back there, with no account, unless an account is named
            own = conn.call(op="session.create", kind="agy", cwd=str(folder), resume_id=CONVERSATION)
            assert own["ok"] == 1 and "account" not in status_of(conn, own["session"]), own
            wait_for(lambda: marks(own_home) == 2)
            # the default may be moved, and a provider that has accounts and no default is never run unbound
            assert conn.call(op="account.default", account=two)["ok"] == 1
            third, _ = new_agy(conn, folder)
            assert status_of(conn, third)["account"] == two
            assert conn.call(op="account.remove", account=two, sessions="end")["ok"] == 1
            lost = conn.call(op="session.create", kind="agy", cwd=str(folder))
            assert lost.get("error") == "unknown_account", "accounts of agy exist and none is the default: no fall-back to the person's own home"
            # an account's session that was hard-stopped comes back in the same home
            wait_for(lambda: registered(directory).get(sid, {}).get("provider_session"))
            before = marks(home_of(directory, one))
        finally:
            hard_stop(host)
        host, log, received = host_with(directory)
        try:
            conn = Conn(host.socket)
            entry = status_of(conn, sid)
            assert entry["state"] == "disconnected" and entry["account"] == one
            # the account of a session is the one it started with: no switch (`session.rebind` is for Claude and Codex)
            moved = conn.call(op="session.rebind", session=sid, generation=generation, account=one)
            assert moved.get("error") == "unknown_account" and status_of(conn, sid)["generation"] == generation and status_of(conn, sid)["state"] == "disconnected", moved
            resumed = conn.call(op="session.resume", session=sid, generation=generation)
            assert resumed["ok"] == 1, resumed
            wait_for(lambda: marks(home_of(directory, one)) == before + 1)
            assert marks(own_home) == 2, "the resume did not move to the person's own home"
            assert [m for m in lines_of(received) if m.get("method") == "session/resume"], "the same conversation is asked for"
        finally:
            host.stop()
