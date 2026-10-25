import json
import time
from pathlib import Path

from lico_measure.keyring_rig import PrivateKeyring
from lico_measure.p02 import ProtoConn as Conn
from lico_measure.rig import private_dir
from lico_protocol.claude_mapping import map_stream

from tests.account_helpers import account, add_credential
from tests.test_claude_adapter import argv_of, claude_host, lines_of, new_claude, records, status_of
from tests.test_codex_adapter import wait_for
from tests.test_credential_store import cred


def test_account_failure_never_switches_account_or_paid_mode():
    with PrivateKeyring() as keyring, private_dir() as directory:
        host, log, received = claude_host(directory, env_overrides=keyring.env)
        try:
            conn = Conn(host.socket)
            a, b = account(conn, "Work", "claude"), account(conn, "Personal", "claude")
            add_credential(conn, keyring, directory, a, "canary-key-work-3c71")
            sid, generation = new_claude(conn)                       # the default account, a
            assert status_of(conn, sid)["account"] == a
            conn.call(op="input", session=sid, generation=generation, request_id="f", text="authfail")
            wait_for(lambda: "[turn failed] error_during_execution" in records(log, sid))
            failure = map_stream([{"type": "system", "subtype": "api_retry", "error": "authentication_failed"},
                                  {"type": "result", "subtype": "error_during_execution", "is_error": True, "api_error_status": 401, "errors": ["authentication failed"]}])
            assert records(log, sid) == [">> authfail", "[session started] model=simulator"] + failure.records
            # the failure is shown against that account and that session, and nothing else
            listed = {e["account"]: e for e in conn.call(op="account.list")["accounts"]}
            assert listed[a]["issue"] == "authentication_failed" and listed[a]["issue_sessions"] == [sid] and "issue" not in listed[b]
            assert status_of(conn, sid)["account_issue"] == "authentication_failed" and status_of(conn, sid)["account"] == a
            assert listed[a]["usage"] is None and listed[b]["usage"] is None            # unknown usage is not zero
            # no silent switch: the default stays, a new session without an account still takes it, a restart keeps the binding and the mode
            assert listed[a]["default"] is True and listed[b]["default"] is False
            again, _ = new_claude(conn)
            assert status_of(conn, again)["account"] == a
            conn.call(op="input", session=sid, generation=generation, request_id="boom", text="crash")
            wait_for(lambda: status_of(conn, sid)["state"] == "disconnected")
            assert conn.call(op="session.resume", session=sid, generation=generation)["ok"] == 1
            assert status_of(conn, sid)["account"] == a
            argvs = wait_for(lambda: argv_of(received) if len(argv_of(received)) >= 3 else None)
            first = [x for x in argvs if "--settings" in x][0]
            restarted = [x for x in argvs if "--resume" in x][0]
            import re
            helper = lambda argv: re.sub(r"lico-broker-\w+", "lico-broker-N", json.loads(argv[argv.index("--settings") + 1])["apiKeyHelper"])   # the broker's name is new for each process
            assert helper(restarted) == helper(first)      # the same key helper: no other key, no other mode
            for argv in argvs:
                assert "--dangerously-skip-permissions" not in argv and "ANTHROPIC_API_KEY" not in " ".join(argv)
            # a successful turn clears the issue, and only then
            conn.call(op="input", session=sid, generation=generation + 1, request_id="ok", text="hello")
            wait_for(lambda: "echo: hello" in records(log, sid))
            wait_for(lambda: "issue" not in {e["account"]: e for e in conn.call(op="account.list")["accounts"]}[a])
            # a provider that has had accounts never runs unbound again, even when none is left
            for acc in (a, b):
                assert conn.call(op="account.remove", account=acc, sessions="end")["ok"] == 1
            assert conn.call(op="account.list")["accounts"] == [] and conn.call(op="session.create", kind="claude")["error"] == "unknown_account"
        finally:
            host.stop()


def test_removal_names_affected_sessions_and_leaves_other_accounts_histories_and_running_work():
    with PrivateKeyring() as keyring, private_dir() as directory:
        host, log, received = claude_host(directory, env_overrides=keyring.env)
        try:
            conn = Conn(host.socket)
            a, b, c = account(conn, "Work", "claude"), account(conn, "Personal", "claude"), account(conn, "Spare", "claude")
            refs = {name: add_credential(conn, keyring, directory, acc, f"canary-key-{name}-8e20") for name, acc in (("a", a), ("b", b), ("c", c))}
            s_a1, g_a1 = new_claude(conn, account=a)
            s_a2, g_a2 = new_claude(conn, account=a)
            s_b, g_b = new_claude(conn, account=b)
            s_c, g_c = new_claude(conn, account=c)
            conn.call(op="input", session=s_a1, generation=g_a1, request_id="slow", text="slow:40")       # work in progress under account a
            conn.call(op="input", session=s_b, generation=g_b, request_id="slow", text="slow:40")
            conn.call(op="input", session=s_a2, generation=g_a2, request_id="hist", text="history of a2")
            wait_for(lambda: "echo: history of a2" in records(log, s_a2))
            b_home = Path(directory) / "accounts" / b
            (b_home / "claude" / "marker.txt").write_text("b")
            # the affected sessions are named before anything is removed, and an action must be chosen
            affected = conn.call(op="account.affected", account=a)
            assert sorted(s["session"] for s in affected["sessions"]) == sorted([s_a1, s_a2]) and all(s["state"] == "running" for s in affected["sessions"])
            assert conn.call(op="account.remove", account=a)["error"] == "bad_request"
            assert conn.call(op="account.remove", account="a" + "0" * 12, sessions="detach")["error"] == "unknown_account"
            # detach: the work goes on, the histories stay; the account and its key go; revoking the login at the provider is a different thing
            removed = conn.call(op="account.remove", account=a, sessions="detach")
            assert removed["ok"] == 1 and removed["metadata_removed"] is True and removed["credential"] == "removed"
            assert removed["remote_revocation"] == "not_performed" and removed["home"] == "kept" and sorted(removed["affected"]) == sorted([s_a1, s_a2])
            from tests.test_credential_store import cred
            data = Path(directory) / "credentials"
            assert cred(keyring.env, data, "helper", "--ref", refs["a"]).returncode == 4                      # a's key is gone
            assert cred(keyring.env, data, "helper", "--ref", refs["b"]).stdout.strip() == "canary-key-b-8e20"  # b's and c's are not
            assert cred(keyring.env, data, "helper", "--ref", refs["c"]).returncode == 0
            assert {e["account"] for e in conn.call(op="account.list")["accounts"]} == {b, c}
            assert (Path(directory) / "accounts" / a).exists()
            ticks = lambda sid: len([r for r in records(log, sid) if r.startswith("tick ")])
            before = ticks(s_a1)
            wait_for(lambda: ticks(s_a1) > before + 3)                                                          # running work was not stopped
            assert status_of(conn, s_a1)["state"] == "running" and "echo: history of a2" in records(log, s_a2) and ticks(s_b) > 0
            assert (b_home / "claude" / "marker.txt").read_text() == "b"
            # no silent switch: with a's default gone, a new session without an account is refused until an owner picks one
            assert conn.call(op="session.create", kind="claude")["error"] == "unknown_account"
            assert conn.call(op="account.default", account=b)["ok"] == 1
            assert status_of(conn, new_claude(conn)[0])["account"] == b
            # end: the account's sessions are purged with it, its home goes, nothing of another account is touched
            gone = conn.call(op="account.remove", account=c, sessions="end")
            assert gone["ok"] == 1 and gone["affected"] == [s_c] and gone["home"] == "removed"
            assert conn.call(op="snapshot", session=s_c)["error"] == "unknown_session" and not (Path(directory) / "accounts" / c).exists()
            assert status_of(conn, s_b)["state"] == "running" and (b_home / "claude" / "marker.txt").exists()
            # replacing an account's key removes the old one from the keyring
            replaced = add_credential(conn, keyring, directory, b, "canary-key-b2-5f19")
            assert cred(keyring.env, data, "helper", "--ref", refs["b"]).returncode == 4
            assert cred(keyring.env, data, "helper", "--ref", replaced).stdout.strip() == "canary-key-b2-5f19"
            # when the keyring cannot be reached nothing is removed, and the answer says why
            keyring.lock()
            before = conn.call(op="account.list")["accounts"]
            assert conn.call(op="account.remove", account=b, sessions="detach")["error"] == "credential_store_unavailable"
            assert conn.call(op="account.list")["accounts"] == before and status_of(conn, s_b)["state"] == "running"
        finally:
            host.stop()


def test_a_corrupt_accounts_file_stops_the_host_instead_of_running_sessions_unbound():
    import subprocess

    from lico_measure.p02 import HOSTD

    with private_dir() as directory:
        accounts = Path(directory) / "accounts"
        accounts.mkdir(mode=0o700)
        (accounts / "accounts.json").write_text("{not json")
        done = subprocess.run([str(HOSTD), "--socket", str(Path(directory) / "host.sock"), "--sessions", "0", "--producing", "0"],
                              capture_output=True, text=True, timeout=15)
        assert done.returncode != 0 and "accounts.json" in done.stderr and not (Path(directory) / "host.sock").exists()
