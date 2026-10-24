"""M01-P04 scenarios and measurements: the credential store, accounts and the second provider.

Everything runs against a throwaway private Secret Service with canary values; no real
keyring, account, login or provider is used here.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import time
from pathlib import Path

from .keyring_rig import PrivateKeyring
from .p02 import HOSTD_DIR
from .report import build_measurement
from .rig import private_dir

CRED = HOSTD_DIR / "lico-cred"
CANARY = "canary-key-measure-4d2e81b7"


def _run(env: dict, data: str, *args: str, stdin: str | None = None) -> tuple[subprocess.CompletedProcess, float, float]:
    """Run lico-cred; return the result and the wall time in ms (the third value is unused)."""
    began = time.monotonic()
    proc = subprocess.Popen([str(CRED), "--data-dir", data, *args], env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, text=True)
    out, err = proc.communicate(stdin)
    elapsed = (time.monotonic() - began) * 1000
    return subprocess.CompletedProcess(proc.args, proc.returncode, out, err), elapsed, 0.0


def _peak_rss_mib(env: dict, data: str, *args: str, stdin: str | None = None) -> float:
    """The peak resident size of one lico-cred process. GNU time forks from a tiny process, so the
    figure is not inflated by the size of this Python process (a child's peak includes its parent's
    image until it executes)."""
    with private_dir() as scratch:
        report = Path(scratch) / "peak"
        subprocess.run(["/usr/bin/time", "-f", "%M", "-o", str(report), str(CRED), "--data-dir", data, *args], env=env,
                       input=stdin or "", capture_output=True, text=True)
        return int(report.read_text().split()[-1]) / 1024.0


def credential_store_costs(reads: int = 100) -> dict:
    """What the hand-written Secret Service client costs, with the keyring unlocked, locked and absent."""
    measurements, observations = [], {"binary_bytes": CRED.stat().st_size}
    scope = {"candidate": "lico-cred (hand-written D-Bus client, no new dependency)", "process_scope": "one lico-cred process",
             "output": "canary secret in a throwaway keyring"}
    with PrivateKeyring() as keyring, private_dir() as data:
        added, _, _ = _run(keyring.env, data, "add", "--provider", "claude", "--account", "a-1", "--kind", "api_key", "--label", "key ending b7", stdin=CANARY + "\n")
        reference = json.loads(added.stdout)["ref"]
        reads_ms, adds_ms = [], []
        for k in range(reads):
            done, elapsed, _ = _run(keyring.env, data, "helper", "--ref", reference)
            assert done.returncode == 0 and done.stdout.strip() == CANARY
            reads_ms.append(elapsed)
        for k in range(20):
            done, elapsed, _ = _run(keyring.env, data, "add", "--provider", "claude", "--account", f"a-{k}", "--kind", "api_key", "--label", "key ending", stdin=CANARY + "\n")
            assert done.returncode == 0
            adds_ms.append(elapsed)
        peaks = [_peak_rss_mib(keyring.env, data, "helper", "--ref", reference) for _ in range(10)]
        measurements += [
            build_measurement("credential_read_ms", dict(scope, workload="helper read, keyring unlocked, one process per read"), reads_ms),
            build_measurement("credential_write_ms", dict(scope, workload="add, keyring unlocked"), adds_ms),
            build_measurement("credential_process_peak_rss_mib", dict(scope, workload="one helper read"), peaks),
        ]
        keyring.lock()
        closed_ms = []
        for _ in range(20):
            done, elapsed, _ = _run(keyring.env, data, "helper", "--ref", reference)
            assert done.returncode == 3 and done.stdout == ""
            closed_ms.append(elapsed)
        measurements.append(build_measurement("credential_fail_closed_ms", dict(scope, workload="read while the keyring is locked"), closed_ms))
    with PrivateKeyring(daemon=False) as keyring, private_dir() as data:
        env = {k: v for k, v in keyring.env.items() if k != "DBUS_SESSION_BUS_ADDRESS"}
        absent_ms = []
        for _ in range(20):
            done, elapsed, _ = _run(env, data, "status")
            assert done.returncode == 3
            absent_ms.append(elapsed)
        measurements.append(build_measurement("credential_fail_closed_ms", dict(scope, workload="status with no session bus"), absent_ms))
    return {"measurements": measurements, "observations": observations}


def real_turn_dry_run() -> dict:
    """The owner's real-turn command for Claude Code run against its simulator: it must finish its turn,
    deny both permission requests and leave an owner-only transcript. The script's own check, not a provider result."""
    from . import real_turn
    from .p03 import SIMULATOR  # noqa: F401  (the Codex simulator is used by the P03 dry run)
    from .rig import REPO

    simulator = REPO / "src" / "host" / "adapters" / "claude" / "simulator.py"
    with private_dir() as directory:
        out = Path(directory) / "dry"
        result = real_turn.run(out, 1, "fixture", str(simulator), provider="claude")
        mode = oct((out / "transcript.txt").stat().st_mode & 0o777)
    return {"simulated": result["simulated"], "provider": result["provider"], "turns": [t.get("finished") for t in result["turns"]],
            "records": result["records"], "approvals_denied": result["approvals_denied"], "transcript_mode": mode}


# ------------------------------------------------------------------ the Claude adapter against its simulator
def _claude_host(directory: str, env_overrides: dict | None = None) -> tuple:
    from .p02 import start
    from .rig import REPO

    simulator = REPO / "src" / "host" / "adapters" / "claude" / "simulator.py"
    fixture = REPO / "tests" / "fixtures" / "claude_stream" / "turn_with_permission.json"
    received = Path(directory) / "claude_received.txt"
    extra = ["--claude", sys.executable, "--claude-arg", "-I", "--claude-arg", str(simulator), "--claude-arg", "--received", "--claude-arg", str(received),
             "--claude-arg", "--fixture", "--claude-arg", str(fixture), "--cred-bin", str(CRED), "--cred-dir", str(Path(directory) / "credentials")]
    host, log = start(directory, extra=extra, sessions=0, producing=0, history=0, retain=100000, env_overrides=env_overrides)
    return host, log, received


def _received_lines(received: Path) -> list[str]:
    return received.read_text().splitlines() if received.exists() else []


def _until(check, seconds: float = 30.0):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        value = check()
        if value:
            return value
        time.sleep(0.05)
    raise TimeoutError("condition not reached")


def _status(control, sid: str) -> dict:
    return [s for s in control.call(op="status")["sessions"] if s["session"] == sid][0]


def claude_integrity(inputs: int = 1000, turns_with_approvals: int = 50, reconnect_every: int = 100) -> dict:
    """The P03 adapter scenario for the second provider: inputs and approvals with lost replies, stale and forged
    targets and provider restarts, two accounts; counted from the provider's own received log and the durable
    session log, not from replies. Everything is simulated."""
    import random

    from .p02 import ProtoConn, read_log

    rng = random.Random(29)
    with private_dir() as directory:
        host, log, received = _claude_host(directory)
        try:
            control = ProtoConn(host.socket)
            accounts = [control.call(op="account.create", provider="claude", label=f"Account {k}")["account"] for k in range(2)]
            sessions = []
            for account in accounts:
                made = control.call(op="session.create", kind="claude", account=account)
                assert made["ok"] == 1, made
                sessions.append({"id": made["session"], "generation": made["generation"], "account": account})
            expected: dict[str, list[str]] = {s["id"]: [] for s in sessions}
            reconnects = lost_replies = refused = attempts = 0

            def completed(sid: str) -> int:
                return sum(1 for _, p in read_log(log, sid) if p == b"[turn completed]")

            def observed(sid: str, rid: str) -> bool:
                return control.call(op="input.status", session=sid, request_id=rid).get("state") in ("observed", "failed")

            for k in range(inputs):
                s = sessions[k % 2]
                if k and k % reconnect_every == 0:
                    control.call(op="input", session=s["id"], generation=s["generation"], request_id=f"crash-{k}", text="crash")
                    _until(lambda: _status(control, s["id"])["state"] == "disconnected")
                    assert control.call(op="input", session=s["id"], generation=s["generation"], request_id=f"down-{k}", text="down")["ok"] == 0
                    resumed = control.call(op="session.resume", session=s["id"], generation=s["generation"])
                    assert resumed["ok"] == 1, resumed
                    s["generation"] = resumed["generation"]
                    reconnects += 1
                rid, text = f"in-{k:05d}", f"{k % 2}:text-{k:05d}"
                conn = ProtoConn(host.socket)
                conn.send(op="input", session=s["id"], generation=s["generation"], request_id=rid, text=text)
                if rng.random() < 0.5:
                    conn.close()                       # the reply is lost, so the sender sends the same request again
                    retry = ProtoConn(host.socket)
                    retry.call(op="input", session=s["id"], generation=s["generation"], request_id=rid, text=text)
                    retry.close()
                    lost_replies += 1
                else:
                    conn.read()
                    conn.close()
                expected[s["id"]].append(text)
                if k % 10 == 9:
                    _until(lambda: observed(s["id"], rid))
            for k in range(100):
                s = sessions[k % 2]
                reply = control.call(op="input", session=s["id"], generation=s["generation"] + 50, request_id=f"stale-{k}", text="stale-target")
                refused += reply.get("error") == "stale_generation"
                attempts += 1
            for k in range(50):
                reply = control.call(op="input", session="s" + "0" * 16, generation=1, request_id=f"foreign-{k}", text="foreign-target")
                refused += reply.get("error") == "unknown_session"
                attempts += 1
            _until(lambda: all(observed(s["id"], f"in-{inputs - 2 + j:05d}") for j, s in enumerate(sessions)))
            time.sleep(1.0)
            heard = []
            for line in _received_lines(received):
                if line.startswith("{"):
                    message = json.loads(line)
                    if message.get("type") == "user":
                        heard.append("".join(b.get("text", "") for b in message["message"]["content"]))
            records = {s["id"]: [p.decode() for _, p in read_log(log, s["id"])] for s in sessions}
            wanted = [t for texts in expected.values() for t in texts]
            applied = {t: sum(r == f"echo: {t}" for rs in records.values() for r in rs) for t in wanted}
            result = {
                "inputs": inputs, "sessions": 2, "provider_restarts": reconnects, "retried_after_lost_reply": lost_replies,
                "stale_and_foreign_attempts": attempts, "refused_correctly": refused,
                "sent_to_provider_zero_times": sum(1 for t in wanted if heard.count(t) == 0),
                "sent_to_provider_more_than_once": sum(1 for t in wanted if heard.count(t) > 1),
                "applied_zero_times": sum(1 for t in wanted if applied[t] == 0), "applied_more_than_once": sum(1 for t in wanted if applied[t] > 1),
                "refused_inputs_that_reached_the_provider": sum(1 for t in heard if "target" in t),
                "misrouted": sum(1 for sid, texts in expected.items() for t in texts
                                 if f"echo: {t}" not in records[sid] or any(f"echo: {t}" in rs for o, rs in records.items() if o != sid)),
            }
            tally: dict[str, int] = {}
            for k in range(inputs):
                state = control.call(op="input.status", session=sessions[k % 2]["id"], request_id=f"in-{k:05d}").get("state", "unknown")
                tally[state] = tally.get(state, 0) + 1
            result["input_states"] = tally
            raised = answered = forged = stale = repeated = 0
            for k in range(turns_with_approvals):
                s = sessions[k % 2]
                done_before = completed(s["id"])
                assert control.call(op="input", session=s["id"], generation=s["generation"], request_id=f"fx-{k}", text="fixture")["ok"] == 1
                for _ in range(2):
                    pending = _until(lambda: _status(control, s["id"]).get("approvals"))
                    number = pending[0]["approval_id"]
                    raised += 1
                    forged += control.call(op="approval.answer", session=s["id"], generation=s["generation"], approval_id=number + 1000, answer="allow").get("error") == "unknown_approval"
                    stale += control.call(op="approval.answer", session=s["id"], generation=s["generation"] + 50, approval_id=number, answer="allow").get("error") == "stale_generation"
                    answered += control.call(op="approval.answer", session=s["id"], generation=s["generation"], approval_id=number, answer="allow" if k % 2 else "deny").get("ok") == 1
                    repeated += control.call(op="approval.answer", session=s["id"], generation=s["generation"], approval_id=number, answer="allow").get("error") == "unknown_approval"
                _until(lambda: completed(s["id"]) > done_before and not _status(control, s["id"]).get("approvals"))
            time.sleep(0.5)
            responses = [json.loads(l)["response"] for l in _received_lines(received) if l.startswith("{") and json.loads(l).get("type") == "control_response"]
            result["approvals"] = {
                "fixture_turns": turns_with_approvals, "approvals_raised": raised, "answered": answered, "provider_responses": len(responses),
                "provider_decisions": {d: sum(1 for r in responses if r["response"].get("behavior") == d) for d in ("allow", "deny")},
                "forged_refused": forged, "stale_refused": stale, "repeat_refused": repeated,
            }
            return {"measurements": [], "observations": result}
        finally:
            host.stop()


# ------------------------------------------------------------------ secrets in sinks
def secret_sink_scan() -> dict:
    """A canary key in the throwaway keyring, attached to an account and used by a provider session: every non-secret
    sink is searched for it (files of the run and of the keyring, command lines and environments of every process of
    the run, protocol replies, the host's standard error, the credential list), and the scanner is first shown to find
    a canary planted in a file, so an empty result means something."""
    from .p02 import ProtoConn, read_log
    from .procscope import tree_pids

    canary = "canary-key-sink-scan-7a2c19e4"
    with PrivateKeyring() as keyring, private_dir() as directory:
        host, log, received = _claude_host(directory, env_overrides=keyring.env)
        try:
            control = ProtoConn(host.socket)
            account = control.call(op="account.create", provider="claude", label="Scan")["account"]
            done = subprocess.run([str(CRED), "--data-dir", str(Path(directory) / "credentials"), "add", "--provider", "claude", "--account", account,
                                   "--kind", "api_key", "--label", "key ending 19e4"], env=keyring.env, input=canary + "\n", capture_output=True, text=True)
            reference = json.loads(done.stdout)["ref"]
            control.call(op="account.credential", account=account, credential_ref=reference)
            made = control.call(op="session.create", kind="claude", account=account)
            sid, generation = made["session"], made["generation"]
            control.call(op="input", session=sid, generation=generation, request_id="k", text="keyenv")
            _until(lambda: any(p.startswith(b"key sha256") for _, p in read_log(log, sid)))
            replies = json.dumps([control.call(op="status"), control.call(op="account.list"), control.call(op="account.affected", account=account),
                                  control.call(op="resources"), control.call(op="session.list")])
            listing = subprocess.run([str(CRED), "--data-dir", str(Path(directory) / "credentials"), "list"], env=keyring.env, capture_output=True, text=True).stdout
            sinks: dict[str, list[bytes]] = {"protocol replies": [replies.encode()], "credential list": [listing.encode()]}
            files = [p for root in (directory, keyring.root) for p in Path(root).rglob("*") if p.is_file()]
            sinks["files of the run and of the keyring"] = [p.read_bytes() for p in files]
            pids = set()
            for root in [host.pid, *keyring.pids()]:
                pids.update(tree_pids(root))
            commands, environments = [], []
            for pid in pids:
                try:
                    commands.append(Path(f"/proc/{pid}/cmdline").read_bytes())
                    environments.append(Path(f"/proc/{pid}/environ").read_bytes())
                except OSError:
                    pass
            sinks["command lines of the run's processes"] = commands
            sinks["environments of the run's processes"] = environments
            planted = Path(directory) / "planted.txt"
            planted.write_text("a canary planted to prove the scanner can see: " + canary)
            detects = canary.encode() in planted.read_bytes()
            planted.unlink()
            result = {"scanner_finds_a_planted_canary": detects,
                      "sinks": {name: {"items": len(items), "occurrences": sum(item.count(canary.encode()) for item in items)} for name, items in sinks.items()},
                      "processes_scanned": len(pids)}
            result["occurrences_total"] = sum(v["occurrences"] for v in result["sinks"].values())
            return {"measurements": [], "observations": result}
        finally:
            host.stop()


# ------------------------------------------------------------------ host memory with accounts
def accounts_host_memory(repetitions: int = 3) -> dict:
    """Host memory with many accounts and with provider sessions bound to them (Claude simulator sessions); the
    provider processes are reported apart."""
    from .p02 import ProtoConn, _host_rss
    from .report import build_measurement

    rows = {"0 accounts, 0 sessions": (0, 0), "10 accounts, 0 sessions": (10, 0), "100 accounts, 0 sessions": (100, 0), "10 accounts, 20 sessions": (10, 20)}
    measurements = []
    for label, (accounts, sessions) in rows.items():
        rss, pss, agents = [], [], []
        for _ in range(repetitions):
            with private_dir() as directory:
                host, log, received = _claude_host(directory)
                try:
                    control = ProtoConn(host.socket)
                    ids = [control.call(op="account.create", provider="claude", label=f"Account {k}")["account"] for k in range(accounts)]
                    for k in range(sessions):
                        assert control.call(op="session.create", kind="claude", account=ids[k % len(ids)])["ok"] == 1
                    time.sleep(2.0)
                    r, p = _host_rss(host)
                    rss.append(r), pss.append(p)
                    agents.append(sum(a["rss_kb"] for a in control.call(op="resources")["agents"]) / 1024.0)
                finally:
                    host.stop()
        scope = {"candidate": "host:hostd", "workload": label, "process_scope": "host process", "output": "accounts and simulated Claude sessions"}
        measurements += [build_measurement("host_rss_mib", scope, rss), build_measurement("host_pss_mib", scope, pss),
                         build_measurement("agents_total_rss_mib", dict(scope, process_scope="provider simulator processes (Python test doubles, not the real provider)"), agents)]
    return {"measurements": measurements, "observations": {}}


# ------------------------------------------------------------------ what a person sees
def transcripts() -> dict:
    """Command-line transcripts of the credential store in each keyring state, and an account failure and a removal as
    the protocol shows them. Canary values only."""
    from .p02 import ProtoConn, read_log

    out: dict = {"credential_store": {}}

    def show(env, data, *args, stdin=None):
        done = subprocess.run([str(CRED), "--data-dir", data, *args], env=env, input=stdin, capture_output=True, text=True)
        return {"command": "lico-cred " + " ".join(args), "exit": done.returncode, "stdout": done.stdout.strip(), "stderr": done.stderr.strip()}

    with private_dir() as data:
        with PrivateKeyring() as keyring:
            out["credential_store"]["ready"] = [show(keyring.env, data, "status"),
                                                show(keyring.env, data, "add", "--provider", "claude", "--account", "a-demo", "--kind", "api_key", "--label", "key ending demo", stdin="canary-key-demo-0001\n"),
                                                show(keyring.env, data, "list")]
            keyring.lock()
            out["credential_store"]["locked"] = [show(keyring.env, data, "status"), show(keyring.env, data, "add", "--provider", "claude", "--account", "a-demo2", "--kind", "api_key", "--label", "key ending demo", stdin="canary-key-demo-0002\n")]
        with PrivateKeyring(unlock=False) as keyring:
            out["credential_store"]["no_default_keyring"] = [show(keyring.env, data, "status")]
        with PrivateKeyring(daemon=False) as keyring:
            out["credential_store"]["no_secret_service"] = [show(keyring.env, data, "status")]
            env = {k: v for k, v in keyring.env.items() if k != "DBUS_SESSION_BUS_ADDRESS"}
            out["credential_store"]["no_session_bus"] = [show(env, data, "status")]
    with PrivateKeyring() as keyring, private_dir() as directory:
        host, log, received = _claude_host(directory, env_overrides=keyring.env)
        try:
            control = ProtoConn(host.socket)
            a = control.call(op="account.create", provider="claude", label="Work")["account"]
            b = control.call(op="account.create", provider="claude", label="Personal")["account"]
            for acc in (a, b):
                done = subprocess.run([str(CRED), "--data-dir", str(Path(directory) / "credentials"), "add", "--provider", "claude", "--account", acc, "--kind", "api_key", "--label", "key ending demo"],
                                      env=keyring.env, input=f"canary-key-{acc}\n", capture_output=True, text=True)
                control.call(op="account.credential", account=acc, credential_ref=json.loads(done.stdout)["ref"])
            first = control.call(op="session.create", kind="claude")
            control.call(op="input", session=first["session"], generation=first["generation"], request_id="f", text="authfail")
            _until(lambda: any(p.startswith(b"[turn failed]") for _, p in read_log(log, first["session"])))
            time.sleep(0.3)
            control.call(op="account.default", account=b)
            second = control.call(op="session.create", kind="claude")
            control.call(op="input", session=second["session"], generation=second["generation"], request_id="s", text="slow:30")
            out["account_failure"] = {"account.list": control.call(op="account.list")["accounts"],
                                      "status": [_status(control, first["session"]), _status(control, second["session"])],
                                      "records_of_the_failed_session": [p.decode() for _, p in read_log(log, first["session"])]}
            # the removal shown is that of the default account, whose session is still working
            out["removal"] = {"account.affected": control.call(op="account.affected", account=b),
                              "account.remove without a choice": control.call(op="account.remove", account=b),
                              "account.remove detach": control.call(op="account.remove", account=b, sessions="detach"),
                              "session.create after the default was removed": control.call(op="session.create", kind="claude"),
                              "account.list after": control.call(op="account.list")["accounts"],
                              "status after": [_status(control, first["session"]), _status(control, second["session"])]}
        finally:
            host.stop()
    return out


def hostile_adapter_checks() -> dict:
    """Run both adapters' hostile tests (and the runtime reuse test) and report outcome and duration."""
    from .rig import REPO

    results = {}
    for name, test_file, selector in (("codex", "tests/test_codex_adapter.py", "hostile"), ("claude", "tests/test_claude_adapter.py", "hostile or reuse")):
        began = time.monotonic()
        done = subprocess.run([sys.executable, "-m", "pytest", "-q", "-p", "no:cacheprovider", str(REPO / test_file), "-k", selector], capture_output=True,
                              text=True, cwd=REPO, env=dict(os.environ, PYTHONPATH=str(REPO / "src") + os.pathsep + str(REPO)))
        lines = done.stdout.strip().splitlines()
        results[name] = {"passed": done.returncode == 0, "summary": (lines or [""])[-1], "seconds": round(time.monotonic() - began, 1),
                         "failure": [l for l in lines if l.startswith("E ")][:8] if done.returncode else []}
    return results


def capability_records() -> dict:
    from .rig import REPO

    out = {}
    for name in ("codex", "claude"):
        record = json.loads((REPO / "src" / "host" / "adapters" / name / "capabilities.json").read_text())
        out[name] = {"version": record["version"], "unsupported": record["unsupported"], "unverified": record["unverified"],
                     "this_session_ran_a_real_provider_turn": False}
    return out


def collect_p04() -> dict:
    from . import real_turn  # noqa: F401
    from .p03 import real_turn_dry_run as codex_dry_run

    return {"credential_store_costs": credential_store_costs(), "secret_sink_scan": secret_sink_scan(), "claude_integrity": claude_integrity(),
            "accounts_host_memory": accounts_host_memory(), "transcripts": transcripts(), "hostile_adapters": hostile_adapter_checks(),
            "capability_records": capability_records(),
            "real_turn_dry_runs": {"codex": codex_dry_run(), "claude": real_turn_dry_run()}}
