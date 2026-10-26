import json
import os
import random
import sys
import time
from pathlib import Path

from lico_measure.p02 import ProtoConn as Conn, read_log, start
from lico_measure.providers import generate_schema, message_errors
from lico_measure.rig import REPO, private_dir

SIM = REPO / "src" / "host" / "adapters" / "codex" / "simulator.py"
FIXTURE = REPO / "tests" / "fixtures" / "codex_app_server" / "turn_with_approval.json"


def codex_host(directory, sim_args=()):
    received = os.path.join(directory, "received.jsonl")
    extra = ["--codex", sys.executable, "--codex-arg", "-I", "--codex-arg", str(SIM), "--codex-arg", "--received",
             "--codex-arg", received, "--codex-arg", "--fixture", "--codex-arg", str(FIXTURE)]
    for arg in sim_args:
        extra += ["--codex-arg", str(arg)]
    host, log = start(directory, extra=extra, sessions=0, producing=0, history=0)
    return host, log, Path(received)


def new_session(host):
    conn = Conn(host.socket)
    made = conn.call(op="session.create", kind="codex")
    assert made["ok"] == 1, made
    return conn, made["session"], made["generation"]


def sent(received):
    """Every message the provider received from the host, parsed."""
    if not received.exists():
        return []
    return [json.loads(line) for line in received.read_text().split("\n")[:-1] if line.strip().startswith("{")]       # only finished lines: a long one is half there for a moment


def texts(log, sid):
    return [p.decode() for _, p in read_log(log, sid)]


def wait_for(check, timeout=10.0):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        value = check()
        if value:
            return value
        time.sleep(0.05)
    raise AssertionError("condition not reached")


def test_input_becomes_exactly_one_turn_and_an_ambiguous_one_is_never_resent():
    with private_dir() as directory:
        host, log, received = codex_host(directory)
        try:
            conn, sid, generation = new_session(host)
            first = conn.call(op="input", session=sid, generation=generation, request_id="r-1", text="hello 한글 안녕")
            assert first["ok"] == 1 and first["duplicate"] is False
            wait_for(lambda: "echo: hello 한글 안녕" in texts(log, sid))
            turns = [m for m in sent(received) if m.get("method") == "turn/start"]
            assert len(turns) == 1, turns
            assert turns[0]["params"]["input"] == [{"type": "text", "text": "hello 한글 안녕", "text_elements": []}]
            assert conn.call(op="input.status", session=sid, request_id="r-1")["state"] in ("submitted", "observed")
            wait_for(lambda: conn.call(op="input.status", session=sid, request_id="r-1")["state"] == "observed")
            # replies lost and the same request sent again: still one turn each
            rng = random.Random(3)
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
            wait_for(lambda: all(f"echo: t{k}" in texts(log, sid) or f"steer: t{k}" in texts(log, sid) for k in range(30)))
            time.sleep(0.5)                                           # a late duplicate would show up now
            seen = texts(log, sid)
            # each input was applied by the provider exactly once: as a turn of its own, or as a steer of
            # the turn that was running (a steer the provider refused falls back to a turn, never both)
            for k in range(30):
                assert seen.count(f"echo: t{k}") + seen.count(f"steer: t{k}") == 1, k
            # an input while a turn is running steers that turn instead of starting another
            conn.call(op="input", session=sid, generation=generation, request_id="slow-1", text="slow:6")
            wait_for(lambda: "tick 0" in texts(log, sid))
            conn.call(op="input", session=sid, generation=generation, request_id="steer-1", text="mid-turn")
            wait_for(lambda: "steer: mid-turn" in texts(log, sid))
            messages = sent(received)
            mid = [m for m in messages if m.get("method") in ("turn/start", "turn/steer") and m["params"]["input"][0]["text"] == "mid-turn"]
            assert [m["method"] for m in mid] == ["turn/steer"] and mid[0]["params"]["expectedTurnId"].startswith("tu-")
            assert "echo: mid-turn" not in texts(log, sid)
        finally:
            host.stop()


def test_approval_round_trip_and_a_stale_or_forged_approval_is_refused():
    with private_dir() as directory:
        host, log, received = codex_host(directory)
        try:
            conn, sid, generation = new_session(host)
            watcher = Conn(host.socket)
            watcher.call(op="subscribe", session=sid, after=-1, snapshot=True)
            conn.call(op="input", session=sid, generation=generation, request_id="fx", text="fixture")

            def pending():
                entry = [s for s in conn.call(op="status")["sessions"] if s["session"] == sid][0]
                return entry.get("approvals", [])

            first = wait_for(pending)
            assert first[0]["prompt"] == "run: ls -la"
            number = first[0]["approval_id"]
            # forged, stale and wrong-session answers send nothing to the provider
            assert conn.call(op="approval.answer", session=sid, generation=generation, approval_id=number + 50, answer="allow")["error"] == "unknown_approval"
            assert conn.call(op="approval.answer", session=sid, generation=generation + 1, approval_id=number, answer="allow")["error"] == "stale_generation"
            assert not [m for m in sent(received) if m.get("id") in (7, 11) and "result" in m]
            ok = conn.call(op="approval.answer", session=sid, generation=generation, approval_id=number, answer="allow")
            assert ok["ok"] == 1 and ok["state"] == "answered"
            # the same answer again is refused: it is never sent twice
            assert conn.call(op="approval.answer", session=sid, generation=generation, approval_id=number, answer="allow")["error"] == "unknown_approval"
            second = wait_for(lambda: [a for a in pending() if a["approval_id"] != number])
            assert second[0]["prompt"].startswith("change files")
            assert conn.call(op="approval.answer", session=sid, generation=generation, approval_id=second[0]["approval_id"], answer="deny")["ok"] == 1
            wait_for(lambda: "[turn completed]" in texts(log, sid))
            replies = [m for m in sent(received) if "result" in m or "error" in m]
            assert replies.count({"id": 7, "result": {"decision": "accept"}}) == 1
            assert replies.count({"id": 11, "result": {"decision": "decline"}}) == 1
            assert replies.count({"id": 9, "error": {"code": -32601, "message": "unsupported by this client"}}) == 1
            assert len([m for m in replies if m.get("id") in (7, 9, 11)]) == 3
            # the records are exactly what the reference mapping gives for the same recorded turn
            assert texts(log, sid) == [
                ">> fixture", "[turn started]", "안녕하세요. 작업을 시작합니다.", "두 번째 줄 é 漢字 🙂", "hidden controls and tab done",
                "$ ls -la", "total 0", "file-한글.txt", "[exit 0]", "[error] network unavailable", "[turn completed]",
            ]
            # viewers were told about both approvals as events
            events = []
            watcher.sock.settimeout(0.5)
            try:
                while True:
                    events.append(watcher.read())
            except Exception:
                pass
            assert len([e for e in events if e.get("ev") == "approval"]) == 2
        finally:
            host.stop()


def test_provider_exit_is_disconnected_not_idle():
    with private_dir() as directory:
        host, log, received = codex_host(directory)
        try:
            conn, sid, generation = new_session(host)

            def pending():
                return [s for s in conn.call(op="status")["sessions"] if s["session"] == sid][0].get("approvals", [])

            conn.call(op="input", session=sid, generation=generation, request_id="fx", text="fixture")
            first_ids = []
            for _ in range(2):
                found = wait_for(pending)
                first_ids.append(found[0]["approval_id"])
                assert conn.call(op="approval.answer", session=sid, generation=generation, approval_id=found[0]["approval_id"], answer="allow")["ok"] == 1
            wait_for(lambda: conn.call(op="input.status", session=sid, request_id="fx")["state"] == "observed" and "[turn completed]" in texts(log, sid))
            conn.call(op="input", session=sid, generation=generation, request_id="boom", text="crash")

            def state():
                return [s for s in conn.call(op="status")["sessions"] if s["session"] == sid][0]["state"]

            wait_for(lambda: state() == "disconnected")
            assert "[provider disconnected]" in texts(log, sid)
            assert conn.call(op="input", session=sid, generation=generation, request_id="after", text="x")["error"] == "session_disconnected"
            # the input that was in flight is neither resent nor forgotten
            assert conn.call(op="input.status", session=sid, request_id="boom")["state"] in ("submitted", "observed")
            resumed = conn.call(op="session.resume", session=sid, generation=generation)
            assert resumed["ok"] == 1 and resumed["generation"] == generation + 1
            assert state() == "running"
            # the old target is stale now; the new generation works
            assert conn.call(op="input", session=sid, generation=generation, request_id="old", text="x")["error"] == "stale_generation"
            assert conn.call(op="input", session=sid, generation=resumed["generation"], request_id="new", text="안녕")["ok"] == 1
            wait_for(lambda: "echo: 안녕" in texts(log, sid))
            # an approval id is never reused after a restart: the old number cannot approve a new request
            conn.call(op="input", session=sid, generation=resumed["generation"], request_id="fx2", text="fixture")
            again = wait_for(pending)
            assert again[0]["approval_id"] > max(first_ids), (first_ids, again)
            assert conn.call(op="approval.answer", session=sid, generation=resumed["generation"], approval_id=first_ids[0], answer="allow")["error"] == "unknown_approval"
            messages = sent(received)
            assert len([m for m in messages if m.get("method") == "turn/start" and m["params"]["input"][0]["text"] == "crash"]) == 1
            assert [m for m in messages if m.get("method") == "thread/resume"], "the thread was not resumed"
            assert "[provider reconnected]" in texts(log, sid)
        finally:
            host.stop()


# What a mode lets Codex do. A thread always starts and resumes `read-only`: a thread whose sandbox can write makes Codex trust the folder it was given and
# load that folder's own `.codex/config.toml` (MCP servers, notify, hooks, providers) as the owner's own settings (Codex 0.164.0). The turn-level override
# does not do that, so the mode lives in the turns.
TURN_RULES = {
    "auto": ("never", {"type": "dangerFullAccess"}),
    "default": ("untrusted", {"type": "readOnly"}),
}


def test_a_codex_thread_never_starts_or_resumes_with_a_writable_sandbox_and_every_turn_carries_the_policy_of_its_mode():
    with private_dir() as scratch:
        schema = Path(scratch) / "schema"
        generate_schema(Path(scratch) / "home", schema)
        for mode, (policy, sandbox) in TURN_RULES.items():
            with private_dir() as directory:
                host, log, received = codex_host(directory)
                try:
                    conn = Conn(host.socket)
                    made = conn.call(op="session.create", kind="codex", mode=mode)
                    sid, generation = made["session"], made["generation"]
                    for k in range(12):                                   # some of these steer a running turn, some start one, and a refused steer starts one
                        assert conn.call(op="input", session=sid, generation=generation, request_id=f"{mode}-{k}", text=f"t{k}")["ok"] == 1
                    wait_for(lambda: all(f"echo: t{k}" in texts(log, sid) or f"steer: t{k}" in texts(log, sid) for k in range(12)))
                    assert conn.call(op="session.close", session=sid, generation=generation)["ok"] == 1
                    time.sleep(0.5)                                       # the closed process's reader ends a moment after the close; this test is about what the resume sends
                    resumed = conn.call(op="session.resume", session=sid, generation=generation)
                    assert resumed["ok"] == 1, resumed
                    assert conn.call(op="input", session=sid, generation=resumed["generation"], request_id=f"{mode}-after", text="after")["ok"] == 1
                    wait_for(lambda: "echo: after" in texts(log, sid))
                    messages = sent(received)
                    threads = [m for m in messages if m.get("method") in ("thread/start", "thread/resume")]
                    turns = [m for m in messages if m.get("method") == "turn/start"]
                    assert [m["method"] for m in threads] == ["thread/start", "thread/resume"], mode
                    assert len(turns) >= 2, mode
                    for m in threads:
                        assert (m["params"]["approvalPolicy"], m["params"]["sandbox"]) == (policy, "read-only"), (mode, m["method"], m["params"])
                    for m in turns:
                        assert (m["params"]["approvalPolicy"], m["params"]["sandboxPolicy"]) == (policy, sandbox), (mode, m["params"])
                    for m in threads + turns:                              # and the provider's own schema accepts what is sent
                        assert message_errors(schema, "ClientRequest.json", m["method"], m["params"]) == [], (m["method"], m["params"])
                finally:
                    host.stop()


def _hwm_kb(pid):
    for line in open(f"/proc/{pid}/status"):
        if line.startswith("VmHWM:"):
            return int(line.split()[1])


def test_adapter_survives_malformed_oversize_and_hostile_provider_messages():
    canary = "CANARY-5c1e9f0a7b"
    with private_dir() as directory:
        host, log, received = codex_host(directory)
        try:
            conn, sid, generation = new_session(host)
            base = _hwm_kb(host.pid)

            def run(name, text, marker):
                assert conn.call(op="input", session=sid, generation=generation, request_id=name, text=text)["ok"] == 1
                wait_for(lambda: marker in texts(log, sid), timeout=60)

            run("bad", "bad", "still alive")                # not JSON, a message without fields, a 3 MB line
            run("endless", "endless", "endless survived")   # forty megabytes and no end of line
            run("hostile", "hostile", "hostile survived")
            run("canary", canary, "echo: " + canary)
            entry = [s for s in conn.call(op="status")["sessions"] if s["session"] == sid][0]
            assert entry["state"] == "running"               # the host did not drop the provider session
            records = texts(log, sid)
            assert all(len(r.encode()) <= 4096 for r in records)
            assert not any(c < " " or c == "\x7f" or c in "\u2028\u2029" for r in records for c in r)
            # line breaks inside a command cannot forge status records: it stays one record, and no
            # record says the provider disconnected (the log keeps only the newest records)
            assert "$ echo [turn completed] [turn started][provider disconnected]" in records, [r for r in records if r.startswith("$")]
            assert "[provider disconnected]" not in records
            # approvals stay bounded, prompts are short and plain, a repeated request id is one approval
            approvals = entry.get("approvals", [])
            assert len(approvals) == 64 and all(len(a["prompt"]) <= 200 and not any(c < " " for c in a["prompt"]) for a in approvals)
            assert all(a["prompt"].endswith("…") for a in approvals), "a cut prompt says so"
            # an oversize request id is never echoed back to the provider
            assert not any(isinstance(m.get("id"), str) and len(m["id"]) > 64 for m in sent(received))
            replies = {m["id"]: m for m in sent(received) if "id" in m and "error" in m}
            assert all(replies[1000 + k]["error"]["code"] for k in range(64, 300)), "approvals beyond the bound are refused to the provider"
            assert not any(1000 + k in replies for k in range(64))
            assert replies[5000]["error"]["code"] == -32601
            # memory grew by a small constant, not with the 70 MB the provider sent
            assert _hwm_kb(host.pid) - base < 24 * 1024, (base, _hwm_kb(host.pid))
            # the canary reached the session log and the provider only: not argv, not stderr, not any other file
            for pid in [host.pid] + [int(p) for p in os.listdir("/proc") if p.isdigit() and _parent(int(p)) == host.pid]:
                assert canary.encode() not in open(f"/proc/{pid}/cmdline", "rb").read()
            holders = sorted(f.suffix for f in Path(directory).rglob("*") if f.is_file() and canary.encode() in f.read_bytes())
            assert holders == [".jsonl", ".seg"], holders      # the provider's own received log and the session log
        finally:
            host.stop()


def _parent(pid):
    try:
        return int(open(f"/proc/{pid}/stat").read().rsplit(")", 1)[1].split()[1])
    except (OSError, ValueError):
        return -1
