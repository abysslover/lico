"""M01-P03 scenarios: the provider adapter against the protocol simulator, and provider evidence.

Everything here runs against the simulator, a test double. It is never a result of the real
provider; the real turn is run by the owner with its own script.
"""

from __future__ import annotations

import json
import os
import random
import signal
import subprocess
import sys
import time
from pathlib import Path

from .p02 import ProtoConn, read_log, start
from .rig import REPO, private_dir

SIMULATOR = REPO / "src" / "host" / "adapters" / "codex" / "simulator.py"
FIXTURE = REPO / "tests" / "fixtures" / "codex_app_server" / "turn_with_approval.json"
CAPABILITIES = REPO / "src" / "host" / "adapters" / "codex" / "capabilities.json"


def _simulated_host(directory: str) -> tuple:
    received = Path(directory) / "received.jsonl"
    extra = ["--codex", sys.executable, "--codex-arg", "-I", "--codex-arg", str(SIMULATOR), "--codex-arg", "--received",
             "--codex-arg", str(received), "--codex-arg", "--fixture", "--codex-arg", str(FIXTURE)]
    host, log = start(directory, extra=extra, sessions=0, producing=0, history=0, retain=100000)   # nothing pruned while counting
    return host, log, received


def _provider_messages(received: Path) -> list[dict]:
    if not received.exists():
        return []
    return [json.loads(line) for line in received.read_text().splitlines() if line.strip().startswith("{")]


def _until(check, seconds: float = 30.0):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        value = check()
        if value:
            return value
        time.sleep(0.05)
    raise TimeoutError("condition not reached")


def adapter_integrity(inputs: int = 1000, turns_with_approvals: int = 50, reconnect_every: int = 100) -> dict:
    """Inputs and approvals through the adapter with lost replies, stale and forged targets and
    provider restarts: each input reaches the provider once and only in its own session, each
    approval is answered once and only for its own request."""
    rng = random.Random(23)
    with private_dir() as directory:
        host, log, received = _simulated_host(directory)
        try:
            control = ProtoConn(host.socket)
            sessions = []
            for _ in range(2):
                made = control.call(op="session.create", kind="codex")
                assert made["ok"] == 1, made
                sessions.append({"id": made["session"], "generation": made["generation"]})
            expected: dict[str, list[str]] = {s["id"]: [] for s in sessions}
            reconnects = refused = lost_replies = attempts = 0
            for k in range(inputs):
                s = sessions[k % 2]
                if k and k % reconnect_every == 0:
                    # the provider process of this session vanishes; the session says so and resumes under a new generation
                    reply = control.call(op="input", session=s["id"], generation=s["generation"], request_id=f"crash-{k}", text="crash")
                    assert reply["ok"] == 1, reply
                    _until(lambda: _state(control, s["id"]) == "disconnected")
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
                    _until(lambda: _settled(control, s["id"], rid))     # no flooding of one provider; also gives its turn time
            for k in range(100):
                s = sessions[k % 2]
                reply = control.call(op="input", session=s["id"], generation=s["generation"] + 50, request_id=f"stale-{k}", text="stale-target")
                refused += reply.get("error") == "stale_generation"
                attempts += 1
            for k in range(50):
                reply = control.call(op="input", session="s" + "0" * 16, generation=1, request_id=f"foreign-{k}", text="foreign-target")
                refused += reply.get("error") == "unknown_session"
                attempts += 1
            _until(lambda: all(_settled(control, s["id"], f"in-{inputs - 1 - j:05d}") for j, s in enumerate(reversed(sessions))))
            time.sleep(1.5)
            # what the provider received and what the session recorded, both read independently of the host's replies
            heard = [m for m in _provider_messages(received) if m.get("method") in ("turn/start", "turn/steer")]
            heard_texts = [m["params"]["input"][0]["text"] for m in heard]
            records = {s["id"]: [p.decode() for _, p in read_log(log, s["id"])] for s in sessions}
            wanted = [t for texts in expected.values() for t in texts]
            # the provider applies an input as a turn ("echo:") or as a steer of the running turn ("steer:");
            # a steer it refuses is followed by a turn, so one text can be sent twice and applied once
            applied = {t: sum(r in (f"echo: {t}", f"steer: {t}") for rs in records.values() for r in rs) for t in wanted}
            sent_twice = {t: [m["method"] for m in heard if m["params"]["input"][0]["text"] == t] for t in wanted if heard_texts.count(t) > 1}
            result = {
                "inputs": inputs, "sessions": 2, "provider_restarts": reconnects, "retried_after_lost_reply": lost_replies,
                "stale_and_foreign_attempts": attempts, "refused_correctly": refused,
                "applied_zero_times": sum(1 for t in wanted if applied[t] == 0),
                "applied_more_than_once": sum(1 for t in wanted if applied[t] > 1),
                "sent_to_provider_zero_times": sum(1 for t in wanted if heard_texts.count(t) == 0),
                "sent_to_provider_more_than_once": len(sent_twice),
                "sent_more_than_once_detail": sent_twice,
                "refused_inputs_that_reached_the_provider": sum(1 for t in heard_texts if "target" in t),
                "misrouted": sum(1 for sid, texts in expected.items() for t in texts
                                 if not any(r.endswith(t) for r in records[sid]) or any(r.endswith(t) for o, rs in records.items() if o != sid for r in rs)),
            }
            # the final state of every input, not a sample; a provider that vanished may leave an input unresolved
            tally: dict[str, int] = {}
            for k in range(inputs):
                state = control.call(op="input.status", session=sessions[k % 2]["id"], request_id=f"in-{k:05d}").get("state", "unknown")
                tally[state] = tally.get(state, 0) + 1
            result["input_states"] = tally
            result["approvals"] = _approvals(control, log, received, sessions, turns_with_approvals)
            return {"measurements": [], "observations": result}
        finally:
            host.stop()


def _completed(log: Path, sid: str) -> int:
    return sum(1 for _, p in read_log(log, sid) if p == b"[turn completed]")


def _state(control: ProtoConn, sid: str) -> str:
    return [s for s in control.call(op="status")["sessions"] if s["session"] == sid][0]["state"]


def _settled(control: ProtoConn, sid: str, request_id: str) -> bool:
    state = control.call(op="input.status", session=sid, request_id=request_id).get("state")
    return state in ("observed", "failed")


def _approvals(control: ProtoConn, log: Path, received: Path, sessions: list[dict], turns: int) -> dict:
    """Fixture turns raise two approvals each. Answers are sent with forged, stale and repeated attempts around them."""
    answered = forged_refused = stale_refused = repeat_refused = raised = 0
    base = len([m for m in _provider_messages(received) if "result" in m and m.get("id") in (7, 11)])
    for k in range(turns):
        s = sessions[k % 2]
        completed_before = _completed(log, s["id"])
        reply = control.call(op="input", session=s["id"], generation=s["generation"], request_id=f"fx-{k}", text="fixture")
        assert reply["ok"] == 1, reply
        for _ in range(2):
            prompt = _until(lambda: [a for a in _pending(control, s["id"])])
            number = prompt[0]["approval_id"]
            raised += 1
            forged_refused += control.call(op="approval.answer", session=s["id"], generation=s["generation"], approval_id=number + 1000, answer="allow").get("error") == "unknown_approval"
            stale_refused += control.call(op="approval.answer", session=s["id"], generation=s["generation"] + 50, approval_id=number, answer="allow").get("error") == "stale_generation"
            ok = control.call(op="approval.answer", session=s["id"], generation=s["generation"], approval_id=number, answer="allow" if (k % 2) else "deny")
            answered += ok.get("ok") == 1
            repeat_refused += control.call(op="approval.answer", session=s["id"], generation=s["generation"], approval_id=number, answer="allow").get("error") == "unknown_approval"
        _until(lambda: _completed(log, s["id"]) > completed_before and not _pending(control, s["id"]))
    time.sleep(1.0)
    replies = [m for m in _provider_messages(received) if "result" in m and m.get("id") in (7, 11)]
    decisions = [m["result"].get("decision") for m in replies[base:]]
    return {
        "fixture_turns": turns, "approvals_raised": raised, "answered": answered, "provider_responses": len(replies) - base,
        "provider_decisions": {d: decisions.count(d) for d in set(decisions)},
        "forged_refused": forged_refused, "stale_refused": stale_refused, "repeat_refused": repeat_refused,
    }


def _pending(control: ProtoConn, sid: str) -> list:
    return [s for s in control.call(op="status")["sessions"] if s["session"] == sid][0].get("approvals", [])


def simulated_session_demo() -> dict:
    """One simulated session end to end, as a watcher sees it: an input, a turn with two approvals
    (one answered after a forged and a stale attempt), a vanished provider and the resumed session."""
    events: list[dict] = []
    with private_dir() as directory:
        host, log, _received = _simulated_host(directory)
        try:
            control, watcher = ProtoConn(host.socket), ProtoConn(host.socket, timeout=1.0)
            made = control.call(op="session.create", kind="codex")
            sid, generation = made["session"], made["generation"]
            watcher.call(op="subscribe", session=sid, after=-1, snapshot=True)

            def collect() -> None:
                while True:
                    try:
                        events.append(watcher.read())
                    except (OSError, EOFError):
                        return

            def step(label: str, reply: dict) -> None:
                events.append({"step": label, "reply": reply})

            step("input fixture", control.call(op="input", session=sid, generation=generation, request_id="demo-1", text="fixture"))
            for _ in range(2):
                pending = _until(lambda: _pending(control, sid))
                number = pending[0]["approval_id"]
                step("approval forged", control.call(op="approval.answer", session=sid, generation=generation, approval_id=number + 1000, answer="allow"))
                step("approval stale generation", control.call(op="approval.answer", session=sid, generation=generation + 1, approval_id=number, answer="allow"))
                step("approval answered", control.call(op="approval.answer", session=sid, generation=generation, approval_id=number, answer="allow"))
                step("approval answered again", control.call(op="approval.answer", session=sid, generation=generation, approval_id=number, answer="allow"))
            _until(lambda: _completed(log, sid) >= 1 and not _pending(control, sid))   # the turn is over: a new input starts a turn, not a steer
            step("input crash", control.call(op="input", session=sid, generation=generation, request_id="demo-2", text="crash"))
            _until(lambda: _state(control, sid) == "disconnected")
            step("input while disconnected", control.call(op="input", session=sid, generation=generation, request_id="demo-3", text="lost?"))
            resumed = control.call(op="session.resume", session=sid, generation=generation)
            step("session.resume", resumed)
            step("old generation after resume", control.call(op="input", session=sid, generation=generation, request_id="demo-4", text="old target"))
            step("input after resume", control.call(op="input", session=sid, generation=resumed["generation"], request_id="demo-5", text="resumed 한글 ok"))
            time.sleep(1.0)
            collect()
            return {"events": events, "session_log": [p.decode() for _, p in read_log(log, sid)]}
        finally:
            host.stop()


def provider_evidence() -> dict:
    """What is pinned and what is not: copied from the capability record, never inferred."""
    record = json.loads(CAPABILITIES.read_text())
    return {
        "provider": record["provider"], "version": record["version"], "protocol_schema": record["protocol_schema"],
        "observed_without_credentials": record["observed_without_credentials"],
        "counts": {"client_requests": len(record["client_requests"]), "server_requests": len(record["server_requests"]),
                   "notifications": len(record["notifications"])},
        "unsupported": record["unsupported"], "unverified": record["unverified"],
        "this_session_ran_a_real_provider_turn": False,
    }


def provider_idle_memory(launches: int = 3) -> dict:
    """The real provider program started and initialized in a private empty home: no sign-in, no
    thread, no turn. Its memory is the provider's, kept apart from Lico's budget."""
    import shutil

    from .procscope import collect_scopes
    from .providers import _private_env
    from .report import build_measurement

    binary = shutil.which("codex")
    if not binary:
        return {"measurements": [], "observations": {"unverified": "the codex program is not installed"}}
    rss, pss, processes = [], [], []
    for _ in range(launches):
        with private_dir() as directory:
            home = Path(directory) / "provider"
            proc = subprocess.Popen([binary, "app-server"], env=_private_env(home), stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                    stderr=subprocess.DEVNULL, text=True,
                                    start_new_session=True)       # its own process group: one exact group to stop
            try:
                init = {"id": 1, "method": "initialize", "params": {"clientInfo": {"name": "lico", "title": "Lico", "version": "0.0.0"}, "capabilities": {}}}
                proc.stdin.write(json.dumps(init) + "\n")
                proc.stdin.flush()
                assert json.loads(proc.stdout.readline()).get("id") == 1
                proc.stdin.write(json.dumps({"method": "initialized"}) + "\n")
                proc.stdin.flush()
                time.sleep(3.0)
                for _ in range(10):                                  # a helper that starts and ends while sampled is read again
                    scope = collect_scopes({"provider": proc.pid})["provider"]
                    if not scope.unreadable:
                        break
                    time.sleep(1.0)
                else:
                    raise RuntimeError("a provider process could not be read")
                processes.append(len(scope.processes))
                rss.append(scope.rss_kb / 1024.0)
                pss.append(scope.pss_kb / 1024.0)
            finally:
                os.killpg(proc.pid, signal.SIGKILL)               # the program and every helper it started
                proc.wait()
    import hashlib

    from .providers import installed_version

    with private_dir() as scratch:
        version = installed_version(Path(scratch))          # asked of the binary that ran, not of the capability record
    digest = hashlib.sha256(Path(binary).resolve().read_bytes()).hexdigest()
    where = {"candidate": "codex app-server " + version + " (not signed in)",
             "workload": "initialized, no thread, no turn", "process_scope": "provider process tree", "sample_unit": "independent launch"}
    return {"measurements": [build_measurement("provider_idle_rss_mib", where, rss), build_measurement("provider_idle_pss_mib", where, pss)],
            "observations": {"version_reported_by_the_binary": version, "binary_sha256": digest, "processes_in_tree": processes, "note": "an idle provider, not signed in; a provider with a conversation holds more"}}


def real_turn_dry_run() -> dict:
    """The owner's real-turn command run against the simulator: it must finish its turns, deny nothing
    unasked and leave an owner-only transcript. This is the script's own check, not a provider result."""
    from . import real_turn

    with private_dir() as directory:
        out = Path(directory) / "dry"
        result = real_turn.run(out, 1, "fixture", str(SIMULATOR))     # the fixture turn asks for two approvals
        mode = oct((out / "transcript.txt").stat().st_mode & 0o777)
    return {"simulated": result["simulated"], "turns": [t.get("finished") for t in result["turns"]], "records": result["records"],
            "approvals_denied": result["approvals_denied"], "transcript_mode": mode}


def hostile_adapter_check() -> dict:
    """Run the adapter's hostile test and report its outcome and duration."""
    began = time.monotonic()
    result = subprocess.run([sys.executable, "-m", "pytest", "-q", "-p", "no:cacheprovider", str(REPO / "tests" / "test_codex_adapter.py"),
                             "-k", "hostile"], capture_output=True, text=True, cwd=REPO,
                            env=dict(os.environ, PYTHONPATH=str(REPO / "src") + os.pathsep + str(REPO)))
    lines = result.stdout.strip().splitlines()
    return {"passed": result.returncode == 0, "summary": (lines or [""])[-1], "seconds": round(time.monotonic() - began, 1),
            "failure": [l for l in lines if l.startswith("E ")][:8] if result.returncode else []}


def collect_p03() -> dict:
    return {"adapter_integrity": adapter_integrity(), "hostile_adapter": hostile_adapter_check(),
            "provider_evidence": provider_evidence(), "provider_memory": provider_idle_memory(),
            "real_turn_dry_run": real_turn_dry_run(), "simulated_session": simulated_session_demo()}
