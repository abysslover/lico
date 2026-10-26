import json
import os
import random
import re
import sys
import time
from pathlib import Path

from lico_measure.keyring_rig import PrivateKeyring
from lico_measure.p04 import _received_lines
from lico_measure.p02 import HOSTD_DIR, ProtoConn as Conn, read_log, start
from lico_measure.rig import REPO, private_dir
from lico_protocol.claude_mapping import map_stream

from tests.test_codex_adapter import agent_pid, finished_lines, let_the_old_helper_write, wait_for
from tests.test_credential_store import cred

SIM = REPO / "src" / "host" / "adapters" / "claude" / "simulator.py"
FIXTURE = REPO / "tests" / "fixtures" / "claude_stream" / "turn_with_permission.json"
CAPABILITIES = REPO / "src" / "host" / "adapters" / "claude" / "capabilities.json"


def claude_host(directory, env_overrides=None):
    received = Path(directory) / "claude_received.txt"
    extra = ["--claude", sys.executable, "--claude-arg", "-I", "--claude-arg", str(SIM), "--claude-arg", "--received", "--claude-arg", str(received),
             "--claude-arg", "--fixture", "--claude-arg", str(FIXTURE), "--cred-bin", str(HOSTD_DIR / "lico-cred"), "--cred-dir", str(Path(directory) / "credentials")]
    host, log = start(directory, extra=extra, sessions=0, producing=0, history=0, retain=100000, env_overrides=env_overrides)
    return host, log, received


def lines_of(received):
    """The lines the provider has finished writing: a long one is half there for a moment, and is not yet a line (and the half may end inside a character)."""
    return finished_lines(received)


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
            sid, generation = new_claude(conn, mode="default")
            conn.call(op="input", session=sid, generation=generation, request_id="fx", text="fixture")
            for want in expected.approvals:
                pending = wait_for(lambda: status_of(conn, sid).get("approvals"))
                assert [p["prompt"] for p in pending] == [want["prompt"]]
                assert conn.call(op="approval.answer", session=sid, generation=generation, approval_id=pending[0]["approval_id"], answer="allow")["ok"] == 1
            wait_for(lambda: "[turn completed]" in records(log, sid))
            # the host's records equal the reference mapping, in order, and nothing else is recorded
            assert records(log, sid) == [">> fixture"] + expected.records, records(log, sid)
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
            sid, generation = new_claude(conn, mode="default")
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
            # the provider gets a private broker for its own key and no part of the keyring's bus
            provider_env = dict(item.split("=", 1) for item in Path(f"/proc/{pid}/environ").read_bytes().decode().split("\0") if "=" in item)
            assert "DBUS_SESSION_BUS_ADDRESS" not in provider_env and "XDG_RUNTIME_DIR" not in provider_env
            assert " broker --name lico-broker-" in Path(f"/proc/{pid}/cmdline").read_text().replace("\0", " ")
            for path in [p for root in (directory, keyring.root) for p in Path(root).rglob("*") if p.is_file()]:
                assert secret.encode() not in path.read_bytes(), path
        finally:
            host.stop()


def _hwm_kb(pid):
    for line in open(f"/proc/{pid}/status"):
        if line.startswith("VmHWM:"):
            return int(line.split()[1])


def test_claude_adapter_survives_malformed_oversize_and_hostile_provider_messages():
    canary = "CANARY-claude-2f9a61c4"
    with private_dir() as directory:
        host, log, received = claude_host(directory)
        try:
            conn = Conn(host.socket)
            sid, generation = new_claude(conn)
            base = _hwm_kb(host.pid)

            def run(name, text, marker):
                assert conn.call(op="input", session=sid, generation=generation, request_id=name, text=text)["ok"] == 1
                wait_for(lambda: marker in records(log, sid), timeout=60)

            run("bad", "bad", "still alive")                  # not JSON, a field of the wrong type, a forged status in a command, a 3 MB line
            run("hostile", "hostile", "hostile survived")
            run("canary", canary, "echo: " + canary)
            entry = status_of(conn, sid)
            assert entry["state"] == "running"                # the host did not drop the provider session
            shown = records(log, sid)
            assert all(len(r.encode()) <= 4096 for r in shown) and not any(c < " " or c == "\x7f" or c in "\u2028\u2029" for r in shown for c in r)
            # line breaks inside a command cannot forge status records
            assert "$ echo [turn completed] [turn started][provider disconnected]" in shown and "[provider disconnected]" not in shown
            # approvals stay bounded, prompts are short, plain and say when they are cut, a repeated id is one approval
            approvals = entry.get("approvals", [])
            assert len(approvals) == 64 and all(len(a["prompt"]) <= 200 and a["prompt"].endswith("…") and not any(c < " " for c in a["prompt"]) for a in approvals)
            answers = [json.loads(l) for l in lines_of(received) if l.startswith("{") and json.loads(l).get("type") == "control_response"]
            by_id = {a["response"]["request_id"]: a["response"] for a in answers}
            assert not any(f"req-{k}" in by_id for k in range(64))                         # the pending ones wait for a person
            assert all(by_id[f"req-{k}"]["subtype"] == "error" for k in range(64, 300))     # the rest are refused to the provider, not held
            assert by_id["unknown-1"]["subtype"] == "error" and by_id["huge-1"]["subtype"] == "error"
            assert not any(len(a["response"]["request_id"]) > 64 for a in answers)         # an oversize request id is never echoed
            # memory grew by a small constant, not with what the provider sent
            assert _hwm_kb(host.pid) - base < 24 * 1024, (base, _hwm_kb(host.pid))
            # the canary reached the session log and the provider only
            holders = sorted(f.suffix for f in Path(directory).rglob("*") if f.is_file() and canary.encode() in f.read_bytes())
            assert holders == [".seg", ".txt"], holders
        finally:
            host.stop()


def test_runtime_reuse_never_merges_accounts_or_tool_contexts():
    from tests.test_accounts import account, environ_of
    from tests.test_codex_adapter import codex_host, sent

    with private_dir() as directory:
        host, log, received = claude_host(directory)
        try:
            conn = Conn(host.socket)
            ca, cb = account(conn, "Work", "claude"), account(conn, "Personal", "claude")
            plan = [("claude", ca), ("claude", ca), ("claude", cb), ("claude", cb)]
            sessions = [(new_claude(conn, account=acc)[0], acc) for _, acc in plan]
            pids = {a["session"]: a["pid"] for a in conn.call(op="resources")["agents"]}
            assert len({pids[s] for s, _ in sessions}) == 4                                     # no process serves two sessions
            workspaces = {s: os.readlink(f"/proc/{pids[s]}/cwd") for s, _ in sessions}
            assert len(set(workspaces.values())) == 4 and all(w.endswith("/workspaces/" + s) for s, w in workspaces.items())   # one private workspace each
            envs = {s: environ_of(host, conn, s) for s, _ in sessions}
            for s, acc in sessions:
                assert f"/accounts/{acc}/" in envs[s]["CLAUDE_CONFIG_DIR"] and envs[s]["HOME"].startswith(str(Path(directory) / "accounts" / acc))
            for (s1, a1) in sessions:
                for (s2, a2) in sessions:
                    if s1 != s2:
                        assert (a1 == a2) == (envs[s1]["CLAUDE_CONFIG_DIR"] == envs[s2]["CLAUDE_CONFIG_DIR"])      # a home is shared only by the same account
                        assert envs[s1]["HOME"] != envs[s2]["HOME"] or a1 == a2
            # the permission context is the same for all sessions of a default start (owner rule of 2026-10-25: permissions skipped, the prompt tool always on)
            for argv in argv_of(received):
                assert "bypassPermissions" == argv[argv.index("--permission-mode") + 1] and argv[argv.index("--permission-prompt-tool") + 1] == "stdio"
        finally:
            host.stop()
    # the same for the Codex provider: each session's thread is started with its own workspace and a read-only sandbox (the turns carry the mode)
    with private_dir() as codex_dir:
        host, log, received = codex_host(codex_dir)
        try:
            conn = Conn(host.socket)
            ca, cb = account(conn, "Work"), account(conn, "Personal")
            made = [conn.call(op="session.create", kind="codex", account=acc) for acc in (ca, cb, ca)]
            starts = [m for m in sent(received) if m.get("method") == "thread/start"]
            assert len(starts) == 3 and len({m["params"]["cwd"] for m in starts}) == 3
            assert all(m["params"]["approvalPolicy"] == "never" and m["params"]["sandbox"] == "read-only" for m in starts)
            pids = {a["session"]: a["pid"] for a in conn.call(op="resources")["agents"]}
            assert len({pids[m["session"]] for m in made}) == 3
        finally:
            host.stop()


def test_the_reader_of_a_closed_provider_cannot_tear_down_the_session_that_was_resumed_after_it():
    """F10: `linger:10` leaves a helper outside the process group that holds the provider's output open for a second, so the
    closed provider's reader ends after the resume, as it does when a close is followed at once by a resume. It is wall clock: the helper
    lives a second from the start of the turn, so in a run where the close and the resume take longer than that the old reader ends first
    and this test cannot fail."""
    with private_dir() as directory:
        host, log, received = claude_host(directory)
        try:
            conn = Conn(host.socket)
            sid, generation = new_claude(conn)
            assert conn.call(op="input", session=sid, generation=generation, request_id="l-1", text="linger:10")["ok"] == 1
            wait_for(lambda: "echo: linger:10" in records(log, sid) and "[turn completed]" in records(log, sid))
            assert conn.call(op="session.close", session=sid, generation=generation)["ok"] == 1
            resumed = conn.call(op="session.resume", session=sid, generation=generation)        # at once, no wait
            assert resumed["ok"] == 1, resumed
            time.sleep(1.6)                                                                      # the helper has ended by now, and the old reader with it
            assert status_of(conn, sid)["state"] == "running", status_of(conn, sid)["state"]
            assert "[provider disconnected]" not in records(log, sid)
            assert conn.call(op="input", session=sid, generation=resumed["generation"], request_id="l-2", text="after")["ok"] == 1
            wait_for(lambda: "echo: after" in records(log, sid))
        finally:
            host.stop()


def test_a_helper_that_outlives_a_closed_provider_writes_nothing_into_the_closed_session():
    """V1: after `session.close` a forged answer and a forged permission request from the helper are read and ignored."""
    with private_dir() as directory:
        host, log, received = claude_host(directory)
        try:
            conn = Conn(host.socket)
            sid, generation = new_claude(conn)
            assert conn.call(op="input", session=sid, generation=generation, request_id="l-1", text="linger:300:inject")["ok"] == 1
            wait_for(lambda: "echo: linger:300:inject" in records(log, sid) and "[turn completed]" in records(log, sid))
            old = agent_pid(conn, sid)
            assert conn.call(op="session.close", session=sid, generation=generation)["ok"] == 1
            let_the_old_helper_write(received, old)
            assert not any("STALE-FORGED" in record for record in records(log, sid)), records(log, sid)
            assert status_of(conn, sid)["state"] == "disconnected" and not status_of(conn, sid).get("approvals"), status_of(conn, sid)
        finally:
            host.stop()


def test_a_helper_that_outlives_a_provider_replaced_by_a_resume_cannot_write_into_the_new_session_and_the_new_agent_goes_on():
    """V1: the forged lines of the old provider's helper add no record, no approval and no sign of life to the session that was resumed at once after the
    close; the new agent's own turn, which is silent while the helper writes, still ends normally (the guard does not swallow what is live)."""
    with private_dir() as directory:
        host, log, received = claude_host(directory)
        try:
            conn = Conn(host.socket)
            sid, generation = new_claude(conn)
            assert conn.call(op="input", session=sid, generation=generation, request_id="l-1", text="linger:300:inject")["ok"] == 1
            wait_for(lambda: "echo: linger:300:inject" in records(log, sid) and "[turn completed]" in records(log, sid))
            old = agent_pid(conn, sid)
            assert conn.call(op="session.close", session=sid, generation=generation)["ok"] == 1
            resumed = conn.call(op="session.resume", session=sid, generation=generation)         # at once, no wait
            assert resumed["ok"] == 1, resumed
            assert conn.call(op="input", session=sid, generation=resumed["generation"], request_id="s-1", text="stall")["ok"] == 1
            wait_for(lambda: "stalling" in records(log, sid))
            heard = status_of(conn, sid)["last_event_at"]
            wait_for(lambda: int(time.time()) > heard)                           # the host counts seconds: a write that counted as a sign of life would move the number
            let_the_old_helper_write(received, old)
            assert not any("STALE-FORGED" in record for record in records(log, sid)), records(log, sid)
            entry = status_of(conn, sid)
            assert not entry.get("approvals") and entry["last_event_at"] == heard and entry["activity"] == "working", entry
            Path(f"{received}.end").touch()
            wait_for(lambda: "done thinking" in records(log, sid))
            wait_for(lambda: status_of(conn, sid)["activity"] == "idle")
        finally:
            host.stop()


def test_lines_of_a_file_that_is_being_written_stops_at_the_last_finished_line_even_when_it_ends_inside_a_character(tmp_path):
    """V5: a read between two writes of a long line can end after the first two bytes of a three-byte character (seen in `test_folders` at random)."""
    path = tmp_path / "claude_received.txt"
    path.write_bytes('{"type": "user", "text": "한"}\n'.encode("utf-8") + '{"type": "user", "text": "한'.encode("utf-8")[:-1])
    assert lines_of(path) == ['{"type": "user", "text": "한"}'] and sent_users(path) == [{"type": "user", "text": "한"}]
    assert lines_of(tmp_path / "none") == []


def test_the_measurement_reads_the_claude_providers_file_up_to_the_last_finished_line_even_when_it_ends_inside_a_character(tmp_path):
    """N11: src/lico_measure/p04.py reads the received file after a sleep while the simulator can still be writing it, and json.loads each line that starts with "{"."""
    path = tmp_path / "claude_received.txt"
    path.write_bytes('{"type": "user", "text": "한"}\n'.encode("utf-8") + '{"type": "user", "text": "한'.encode("utf-8")[:-1])
    assert _received_lines(path) == ['{"type": "user", "text": "한"}']
    assert _received_lines(tmp_path / "none") == []
