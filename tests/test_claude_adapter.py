import json
import os
import random
import re
import sys
import time
from pathlib import Path

from lico_measure.keyring_rig import PrivateKeyring
from lico_measure.p02 import HOSTD_DIR, ProtoConn as Conn, read_log, start
from lico_measure.rig import REPO, private_dir
from lico_protocol.claude_mapping import map_stream

from tests.test_codex_adapter import wait_for
from tests.test_credential_store import cred

SIM = REPO / "src" / "host" / "adapters" / "claude" / "simulator.py"
FIXTURE = REPO / "tests" / "fixtures" / "claude_stream" / "turn_with_permission.json"
CAPABILITIES = REPO / "src" / "host" / "adapters" / "claude" / "capabilities.json"


def claude_host(directory, env_overrides=None):
    received = Path(directory) / "claude_received.txt"
    extra = ["--claude", sys.executable, "--claude-arg", "-I", "--claude-arg", str(SIM), "--claude-arg", "--received", "--claude-arg", str(received),
             "--claude-arg", "--fixture", "--claude-arg", str(FIXTURE), "--cred-bin", str(HOSTD_DIR / "lico-cred"), "--cred-dir", str(Path(directory) / "credentials")]
    host, log = start(directory, extra=extra, sessions=0, producing=0, history=0, env_overrides=env_overrides)
    return host, log, received


def lines_of(received):
    return received.read_text().splitlines() if received.exists() else []


def argv_of(received):
    return [json.loads(l[5:]) for l in lines_of(received) if l.startswith("ARGV ")]


def sent_users(received):
    """The user messages the provider received from the host, parsed."""
    out = []
    for line in lines_of(received):
        if line.startswith("{"):
            message = json.loads(line)
            if message.get("type") == "user":
                out.append(message)
    return out


def user_text(message):
    return "".join(b.get("text", "") for b in message["message"]["content"])


def records(log, sid):
    return [p.decode() for _, p in read_log(log, sid)]


def status_of(conn, sid):
    return [s for s in conn.call(op="status")["sessions"] if s["session"] == sid][0]


def new_claude(conn, **extra):
    made = conn.call(op="session.create", kind="claude", **extra)
    assert made["ok"] == 1, made
    return made["session"], made["generation"]


def test_claude_adapter_pins_the_version_and_marks_unverified_and_maps_fixtures_in_order():
    cap = json.loads(CAPABILITIES.read_text())
    assert cap["provider"] == "claude" and cap["version"]["pinned"] and re.fullmatch(r"\d+\.\d+\.\d+", cap["version"]["cli"])
    assert cap["unverified"] and any("subtype" in item for item in cap["unverified"])           # what the documentation does not say is listed
    assert all(item.get("reason") for item in cap["unsupported"]) and cap["version"]["fixtures"].startswith("built from the documentation")
    steps = json.loads(FIXTURE.read_text())
    expected = map_stream([s["msg"] for s in steps if s["dir"] == "server"])
    assert len(expected.approvals) == 2 and not expected.ignored
    with private_dir() as directory:
        host, log, received = claude_host(directory)
        try:
            conn = Conn(host.socket)
            sid, generation = new_claude(conn)
            conn.call(op="input", session=sid, generation=generation, request_id="fx", text="fixture")
            for want in expected.approvals:
                pending = wait_for(lambda: status_of(conn, sid).get("approvals"))
                assert [p["prompt"] for p in pending] == [want["prompt"]]
                assert conn.call(op="approval.answer", session=sid, generation=generation, approval_id=pending[0]["approval_id"], answer="allow")["ok"] == 1
            wait_for(lambda: "[turn completed]" in records(log, sid))
            # the host's records equal the reference mapping, in order, and nothing else is recorded
            assert records(log, sid) == expected.records, records(log, sid)
            # each permission request got exactly one answer, for its own request id
            answers = [json.loads(l) for l in lines_of(received) if l.startswith("{") and json.loads(l).get("type") == "control_response"]
            assert [a["response"]["request_id"] for a in answers] == [w["request_id"] for w in expected.approvals]
            assert all(a["response"]["subtype"] == "success" and a["response"]["response"]["behavior"] == "allow" for a in answers)
        finally:
            host.stop()


def test_claude_input_and_approval_reach_the_provider_exactly_once_and_a_stale_target_is_refused():
    secret = "canary-key-claude-account-5d83"
    with PrivateKeyring() as keyring, private_dir() as directory:
        host, log, received = claude_host(directory, env_overrides=keyring.env)
        try:
            conn = Conn(host.socket)
            sid, generation = new_claude(conn)
            # one input is one user message, whatever happens to the reply
            rng = random.Random(5)
            for k in range(30):
                probe = Conn(host.socket)
                probe.send(op="input", session=sid, generation=generation, request_id=f"x-{k}", text=f"t{k}")
                if rng.random() < 0.5:
                    probe.close()
                    again = Conn(host.socket)
                    assert again.call(op="input", session=sid, generation=generation, request_id=f"x-{k}", text=f"t{k}")["ok"] == 1
                    again.close()
                else:
                    probe.read()
                    probe.close()
            wait_for(lambda: all(f"echo: t{k}" in records(log, sid) for k in range(30)))
            time.sleep(0.5)
            heard = [user_text(m) for m in sent_users(received)]
            assert all(heard.count(f"t{k}") == 1 and records(log, sid).count(f"echo: t{k}") == 1 for k in range(30))
            wait_for(lambda: conn.call(op="input.status", session=sid, request_id="x-29")["state"] == "observed")   # the replay of its own uuid acknowledged it
            # forged, stale and repeated approval answers send nothing; a real one is answered once
            conn.call(op="input", session=sid, generation=generation, request_id="fx", text="fixture")
            pending = wait_for(lambda: status_of(conn, sid).get("approvals"))
            number = pending[0]["approval_id"]
            assert conn.call(op="approval.answer", session=sid, generation=generation, approval_id=number + 50, answer="allow")["error"] == "unknown_approval"
            assert conn.call(op="approval.answer", session=sid, generation=generation + 1, approval_id=number, answer="allow")["error"] == "stale_generation"
            responses = lambda: [json.loads(l) for l in lines_of(received) if l.startswith("{") and json.loads(l).get("type") == "control_response"]
            assert responses() == []
            assert conn.call(op="approval.answer", session=sid, generation=generation, approval_id=number, answer="deny")["ok"] == 1
            assert conn.call(op="approval.answer", session=sid, generation=generation, approval_id=number, answer="allow")["error"] == "unknown_approval"
            wait_for(lambda: len(responses()) == 1)
            assert responses()[0]["response"]["request_id"] == "req-7" and responses()[0]["response"]["response"]["behavior"] == "deny"
            # a provider that vanishes is disconnected, never idle; resume asks the provider to resume the same session, under a new generation
            second = wait_for(lambda: status_of(conn, sid).get("approvals"))
            conn.call(op="approval.answer", session=sid, generation=generation, approval_id=second[0]["approval_id"], answer="allow")
            wait_for(lambda: "[turn completed]" in records(log, sid))
            first_session = [l.split()[1] for l in lines_of(received) if l.startswith("SESSION ")][0]
            conn.call(op="input", session=sid, generation=generation, request_id="boom", text="crash")
            wait_for(lambda: status_of(conn, sid)["state"] == "disconnected")
            assert conn.call(op="input", session=sid, generation=generation, request_id="after", text="x")["error"] == "session_disconnected"
            resumed = conn.call(op="session.resume", session=sid, generation=generation)
            assert resumed["ok"] == 1 and resumed["generation"] == generation + 1
            argv_lines = lambda: [json.loads(l[5:]) for l in lines_of(received) if l.startswith("ARGV ")]
            argvs = wait_for(lambda: argv_lines() if len(argv_lines()) == 2 else None)       # the new process logs its arguments when it starts
            assert "--resume" not in argvs[0] and argvs[1][argvs[1].index("--resume") + 1] == first_session
            assert conn.call(op="input", session=sid, generation=generation, request_id="old", text="x")["error"] == "stale_generation"
            assert len([m for m in sent_users(received) if user_text(m) == "crash"]) == 1
            # an account's key reaches the provider only through the key helper that reads the keyring
            account = conn.call(op="account.create", provider="claude", label="Key account")["account"]
            data = Path(directory) / "credentials"
            added = cred(keyring.env, data, "add", "--provider", "claude", "--account", account, "--kind", "api_key", "--label", "key ending 83", stdin=secret + "\n")
            assert added.returncode == 0, added.stderr
            reference = json.loads(added.stdout)["ref"]
            assert conn.call(op="account.credential", account=account, credential_ref=reference)["ok"] == 1
            assert conn.call(op="account.credential", account=account, credential_ref="0" * 32)["error"] == "bad_request"      # a reference the store does not know
            keyed, kgen = new_claude(conn, account=account)
            conn.call(op="input", session=keyed, generation=kgen, request_id="k", text="keyenv")
            wait_for(lambda: any(r.startswith("key sha256") for r in records(log, keyed)))
            import hashlib
            shown = records(log, keyed)
            assert f"key length {len(secret)}" in shown and f"key sha256 {hashlib.sha256(secret.encode()).hexdigest()[:12]}" in shown
            assert any(r.startswith("CLAUDE_CONFIG_DIR ") and f"/accounts/{account}/" in r for r in shown)
            pid = [a["pid"] for a in conn.call(op="resources")["agents"] if a["session"] == keyed][0]
            for source in (f"/proc/{pid}/cmdline", f"/proc/{pid}/environ"):
                assert secret.encode() not in Path(source).read_bytes()
            assert "apiKeyHelper" in Path(f"/proc/{pid}/cmdline").read_text()
            for path in [p for root in (directory, keyring.root) for p in Path(root).rglob("*") if p.is_file()]:
                assert secret.encode() not in path.read_bytes(), path
        finally:
            host.stop()
