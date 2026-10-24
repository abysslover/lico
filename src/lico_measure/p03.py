"""M01-P03 scenarios: the provider adapter against the protocol simulator, and provider evidence.

Everything here runs against the simulator, a test double. It is never a result of the real
provider; the real turn is run by the owner with its own script.
"""

from __future__ import annotations

import json
import os
import random
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
    host, log = start(directory, extra=extra, sessions=0, producing=0, history=0)
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
            reconnects = refused = lost_replies = 0
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
            for k in range(50):
                reply = control.call(op="input", session="s" + "0" * 16, generation=1, request_id=f"foreign-{k}", text="foreign-target")
                refused += reply.get("error") == "unknown_session"
            _until(lambda: all(_settled(control, s["id"], f"in-{inputs - 1 - j:05d}") for j, s in enumerate(reversed(sessions))))
            time.sleep(1.5)
            # what the provider received and what the session recorded, both read independently of the host's replies
            heard = [m for m in _provider_messages(received) if m.get("method") in ("turn/start", "turn/steer")]
            heard_texts = [m["params"]["input"][0]["text"] for m in heard]
            records = {s["id"]: [p.decode() for _, p in read_log(log, s["id"])] for s in sessions}
            wanted = [t for texts in expected.values() for t in texts]
            result = {
                "inputs": inputs, "sessions": 2, "provider_restarts": reconnects, "retried_after_lost_reply": lost_replies,
                "stale_and_foreign_attempts": 150, "refused_correctly": refused,
                "sent_to_provider_zero_times": sum(1 for t in wanted if heard_texts.count(t) == 0),
                "sent_to_provider_more_than_once": sum(1 for t in wanted if heard_texts.count(t) > 1),
                "refused_inputs_that_reached_the_provider": sum(1 for t in heard_texts if "target" in t),
                "misrouted": sum(1 for sid, texts in expected.items() for t in texts
                                 if not any(r.endswith(t) for r in records[sid]) or any(r.endswith(t) for o, rs in records.items() if o != sid for r in rs)),
                "unobserved_inputs": 0,
            }
            result["unobserved_inputs"] = sum(1 for k in range(0, inputs, 7)
                                              if control.call(op="input.status", session=sessions[k % 2]["id"], request_id=f"in-{k:05d}").get("state") not in ("observed", "submitted"))
            result["approvals"] = _approvals(control, host, received, sessions, turns_with_approvals)
            return {"measurements": [], "observations": result}
        finally:
            host.stop()


def _state(control: ProtoConn, sid: str) -> str:
    return [s for s in control.call(op="status")["sessions"] if s["session"] == sid][0]["state"]


def _settled(control: ProtoConn, sid: str, request_id: str) -> bool:
    state = control.call(op="input.status", session=sid, request_id=request_id).get("state")
    return state in ("observed", "failed")


def _approvals(control: ProtoConn, host, received: Path, sessions: list[dict], turns: int) -> dict:
    """Fixture turns raise two approvals each. Answers are sent with forged, stale and repeated attempts around them."""
    answered = forged_refused = stale_refused = repeat_refused = 0
    base = len([m for m in _provider_messages(received) if "result" in m and m.get("id") in (7, 11)])
    for k in range(turns):
        s = sessions[k % 2]
        reply = control.call(op="input", session=s["id"], generation=s["generation"], request_id=f"fx-{k}", text="fixture")
        assert reply["ok"] == 1, reply
        for _ in range(2):
            prompt = _until(lambda: [a for a in _pending(control, s["id"])])
            number = prompt[0]["approval_id"]
            forged_refused += control.call(op="approval.answer", session=s["id"], generation=s["generation"], approval_id=number + 1000, answer="allow").get("error") == "unknown_approval"
            stale_refused += control.call(op="approval.answer", session=s["id"], generation=s["generation"] + 50, approval_id=number, answer="allow").get("error") == "stale_generation"
            ok = control.call(op="approval.answer", session=s["id"], generation=s["generation"], approval_id=number, answer="allow" if (k % 2) else "deny")
            answered += ok.get("ok") == 1
            repeat_refused += control.call(op="approval.answer", session=s["id"], generation=s["generation"], approval_id=number, answer="allow").get("error") == "unknown_approval"
        _until(lambda: _settled(control, s["id"], f"fx-{k}") and not _pending(control, s["id"]))
    time.sleep(1.0)
    replies = [m for m in _provider_messages(received) if "result" in m and m.get("id") in (7, 11)]
    return {
        "fixture_turns": turns, "approvals_raised": turns * 2, "answered": answered, "provider_responses": len(replies) - base,
        "forged_refused": forged_refused, "stale_refused": stale_refused, "repeat_refused": repeat_refused,
    }


def _pending(control: ProtoConn, sid: str) -> list:
    return [s for s in control.call(op="status")["sessions"] if s["session"] == sid][0].get("approvals", [])


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


def hostile_adapter_check() -> dict:
    """Run the adapter's hostile test and report its outcome and duration."""
    began = time.monotonic()
    result = subprocess.run([sys.executable, "-m", "pytest", "-q", "-p", "no:cacheprovider", str(REPO / "tests" / "test_codex_adapter.py"),
                             "-k", "hostile"], capture_output=True, text=True, cwd=REPO,
                            env=dict(os.environ, PYTHONPATH=str(REPO / "src") + os.pathsep + str(REPO)))
    return {"passed": result.returncode == 0, "summary": (result.stdout.strip().splitlines() or [""])[-1],
            "seconds": round(time.monotonic() - began, 1)}


def collect_p03() -> dict:
    return {"adapter_integrity": adapter_integrity(), "hostile_adapter": hostile_adapter_check(),
            "provider_evidence": provider_evidence()}
