import json
import re
import time
from pathlib import Path

from lico_measure.keyring_rig import PrivateKeyring
from lico_measure.p02 import ProtoConn as Conn
from lico_measure.rig import private_dir

from tests.test_codex_adapter import codex_host, texts, wait_for
from tests.test_credential_store import cred


def value(log, sid, name, after=0):
    lines = [t for t in texts(log, sid) if t.startswith(name + " ")]
    return lines[-1][len(name) + 1:] if len(lines) > after else None


def account(conn, label, provider="codex"):
    made = conn.call(op="account.create", provider=provider, label=label)
    assert made["ok"] == 1, made
    assert re.fullmatch(r"a[0-9a-f]{12}", made["account"])
    return made["account"]


def status_of(conn, sid):
    return [s for s in conn.call(op="status")["sessions"] if s["session"] == sid][0]


def environ_of(host, conn, sid):
    pid = [a["pid"] for a in conn.call(op="resources")["agents"] if a["session"] == sid][0]
    return dict(item.split("=", 1) for item in Path(f"/proc/{pid}/environ").read_bytes().decode().split("\0") if "=" in item)


def test_two_accounts_of_one_provider_share_no_slot_file_or_process_state():
    with PrivateKeyring() as keyring, private_dir() as directory:
        host, log, received = codex_host(directory)
        try:
            conn = Conn(host.socket)
            a, b = account(conn, "Work"), account(conn, "Personal")
            assert a != b
            sessions = {}
            for name, acc in (("a", a), ("b", b)):
                made = conn.call(op="session.create", kind="codex", account=acc)
                assert made["ok"] == 1, made
                sessions[name] = (made["session"], made["generation"])
            homes = {}
            for name, (sid, generation) in sessions.items():
                conn.call(op="input", session=sid, generation=generation, request_id=f"h-{name}", text="home")
                wait_for(lambda: value(log, sid, "CODEX_HOME"))
                homes[name] = (value(log, sid, "HOME"), value(log, sid, "CODEX_HOME"))
            accounts_dir = Path(directory) / "accounts"
            for name, acc in (("a", a), ("b", b)):
                home, codex_home = (Path(p) for p in homes[name])
                assert accounts_dir / acc in home.parents and accounts_dir / acc in codex_home.parents, (acc, home)
                assert oct(home.stat().st_mode & 0o777) == "0o700" and oct(codex_home.stat().st_mode & 0o777) == "0o700"
            assert homes["a"][0] != homes["b"][0] and homes["a"][1] != homes["b"][1]
            import os
            assert homes["a"][0] != os.environ.get("HOME") and homes["a"][1] != os.environ.get("CODEX_HOME")
            # a login file a provider keeps in its own home is in that home only
            for name, marker in (("a", "login-of-a"), ("b", "login-of-b")):
                sid, generation = sessions[name]
                conn.call(op="input", session=sid, generation=generation, request_id=f"w-{name}", text=f"write:{marker}")
                wait_for(lambda: value(log, sid, "wrote") is not None or "wrote" in texts(log, sid))
            written = {str(p.relative_to(accounts_dir)): p.read_text() for p in accounts_dir.rglob("auth.json")}
            assert written == {f"{a}/{Path(homes['a'][1]).name}/auth.json": "login-of-a", f"{b}/{Path(homes['b'][1]).name}/auth.json": "login-of-b"}, written
            # each provider process holds only its own account's home
            for name, (sid, _) in sessions.items():
                env = environ_of(host, conn, sid)
                assert (env["HOME"], env["CODEX_HOME"]) == homes[name]
            # no shared keyring slot: credentials are filed under the account id, so each is read back apart
            data = Path(directory) / "credentials"
            for acc, secret in ((a, "canary-key-account-a-91"), (b, "canary-key-account-b-47")):
                done = cred(keyring.env, data, "add", "--provider", "codex", "--account", acc, "--kind", "api_key", "--label", "key ending " + secret[-2:], stdin=secret + "\n")
                assert done.returncode == 0, done.stderr
            entries = json.loads(cred(keyring.env, data, "list").stdout)
            assert {e["account"] for e in entries} == {a, b} and len({e["ref"] for e in entries}) == 2
            for entry, secret in zip(sorted(entries, key=lambda e: e["account"] != a), ("canary-key-account-a-91", "canary-key-account-b-47")):
                assert cred(keyring.env, data, "helper", "--ref", entry["ref"]).stdout.strip() == secret
        finally:
            host.stop()


def test_a_running_session_keeps_its_binding_when_the_default_changes_and_an_account_change_during_a_turn_is_refused():
    with private_dir() as directory:
        host, log, received = codex_host(directory)
        try:
            conn = Conn(host.socket)
            a, b, other = account(conn, "Work"), account(conn, "Personal"), account(conn, "Other tool", provider="claude")
            listed = {e["account"]: e for e in conn.call(op="account.list")["accounts"]}
            assert listed[a]["default"] is True and listed[b]["default"] is False and listed[other]["default"] is True   # a provider's first account is its default
            first = conn.call(op="session.create", kind="codex")
            s1, g1 = first["session"], first["generation"]
            assert status_of(conn, s1)["account"] == a
            env_before = environ_of(host, conn, s1)
            assert conn.call(op="account.default", account=b)["ok"] == 1
            second = conn.call(op="session.create", kind="codex")
            s2 = second["session"]
            assert status_of(conn, s2)["account"] == b and status_of(conn, s1)["account"] == a      # the default only affects new sessions
            assert environ_of(host, conn, s1)["CODEX_HOME"] == env_before["CODEX_HOME"] != environ_of(host, conn, s2)["CODEX_HOME"]
            # an account that does not exist, or belongs to another provider, is refused before anything starts
            before = len(conn.call(op="status")["sessions"])
            assert conn.call(op="session.create", kind="codex", account="a" + "0" * 12)["error"] == "unknown_account"
            assert conn.call(op="session.create", kind="codex", account=other)["error"] == "unknown_account"
            assert len(conn.call(op="status")["sessions"]) == before
            # during a turn, and while merely idle and connected, a rebind is refused and the binding stays
            conn.call(op="input", session=s1, generation=g1, request_id="slow", text="slow:30")
            wait_for(lambda: "tick 0" in texts(log, s1))
            pid_before = [x["pid"] for x in conn.call(op="resources")["agents"] if x["session"] == s1]
            assert conn.call(op="session.rebind", session=s1, generation=g1, account=b)["error"] == "busy"
            # a refused rebind touches nothing: the session still runs, with the same provider process and thread
            assert status_of(conn, s1)["state"] == "running" and status_of(conn, s1)["generation"] == g1
            assert [x["pid"] for x in conn.call(op="resources")["agents"] if x["session"] == s1] == pid_before
            conn.call(op="input", session=s2, generation=second["generation"], request_id="quiet", text="hello")
            assert conn.call(op="session.rebind", session=s2, generation=second["generation"], account=a)["error"] == "busy"
            assert status_of(conn, s1)["account"] == a and status_of(conn, s2)["account"] == b
            assert conn.call(op="session.rebind", session=s1, generation=g1, account="a" + "0" * 12)["error"] == "unknown_account"
            # the explicit boundary: a session whose provider is gone can be moved, as a new thread under a new generation
            third = conn.call(op="session.create", kind="codex")
            s3, g3 = third["session"], third["generation"]
            conn.call(op="input", session=s3, generation=g3, request_id="boom", text="crash")
            wait_for(lambda: status_of(conn, s3)["state"] == "disconnected")
            moved = conn.call(op="session.rebind", session=s3, generation=g3, account=a)
            assert moved["ok"] == 1 and moved["generation"] == g3 + 1, moved
            assert status_of(conn, s3)["account"] == a and status_of(conn, s3)["state"] == "running"
            assert conn.call(op="input", session=s3, generation=g3, request_id="old", text="x")["error"] == "stale_generation"
            conn.call(op="input", session=s3, generation=moved["generation"], request_id="home", text="home")
            wait_for(lambda: value(log, s3, "CODEX_HOME"))
            assert f"/accounts/{a}/" in value(log, s3, "CODEX_HOME")
        finally:
            host.stop()
