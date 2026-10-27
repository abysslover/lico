"""Opt-in check of the tool connection's approvals (P03 S04) against the REAL `claude` program and its sign-in: a few short haiku turns, never part of the suite.

Rows (a private Lico host on a private socket and a private state folder; the owner's socket is never touched):
  send   `send` from session A to session B, `--tries` times (3): the call answers `pending`; pass = A's turn ends and A did not call `sessions`, `send` or
         `create` after the answer; when the script says yes through the owner operation `approval.answer`, B is handed the message and answers, the notice reaches
         A as an input, and a second turn of A starts from it and answers it.
  long   ONE try of the case where the notice reaches A while A writes a long answer with no tool (about 600 words are asked for after the call); pass = the answer is
         whole, and the notice is handled after it (a turn of A that answers the notice).
  create ONE try of `create`: the script says yes, the new session (made on the model and effort of A) reads its first prompt and answers, A gets the notice.
  handover  after the first send try: the conversation of A is handed to a session of the Codex simulator (no model turn) and the head of the stored file is read.
  gone   a session whose conversation the program no longer has: after its first turn the program's record file is moved away, the session is closed, and Enter is
         pressed in the product window with a text in the box. What the host's history shows, how long until the row is `disconnected`, what the window shows.
         This row uses the program the machine has on PATH; the others use the pinned version.

The sign-in is the one of the home this script runs under (the program finds it itself; this script never reads it). The program's environment is PATH, LANG and HOME only.
A budget of 16 turns is counted from the records (`[turn completed]`/`[turn failed]` of the sessions the check made); a row is not started when it would go over.
Allowance figures (5 h and 7 d, in percent) are read from the status-line file the status line writes, before and after; nothing else of that file is printed.

    PYTHONPATH=src python3 s04_check.py --lico PATH_TO_LICO --out RESULT.json [--rows send,long,create,gone,gone0] [--tries 3] [--claude PATH] [--claude-latest PATH] [--status-files GLOB] [--shots DIR]
    (--sim: the Claude program is the protocol simulator, no model call, for a dry run of the script itself)
"""
import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import uuid
import time
from pathlib import Path

REPO = Path(__file__).resolve().parents[4]
sys.path[:0] = [str(REPO / "src"), str(REPO)]
from lico_measure.p02 import ProtoConn, read_log  # noqa: E402

BUDGET = 16
SIM = False
STATUS_FILES: list[str] = []     # the one glob of `--status-files`; empty: no allowance figures
SCALE = 1.0                      # the waits are multiplied by this; a dry run with the simulator (which never calls a tool) makes them short
TOOLS = ("mcp__lico__sessions", "mcp__lico__send", "mcp__lico__create")
FINISHED = ("[turn completed]", "[turn failed]", "[turn interrupted]")
CODEX_SIM = REPO / "src" / "host" / "adapters" / "codex" / "simulator.py"
CODEX_FIXTURE = REPO / "tests" / "fixtures" / "codex_app_server" / "turn_with_approval.json"
CLAUDE_SIM = REPO / "src" / "host" / "adapters" / "claude" / "simulator.py"
CLAUDE_FIXTURE = REPO / "tests" / "fixtures" / "claude_stream" / "turn_with_permission.json"
PONG = "Reply with the single word pong."
TEMPS: list[str] = []
RESULT: dict = {"rows": {}}


def say(*parts):
    print(*parts, flush=True)


def temp(prefix):
    path = Path(tempfile.mkdtemp(prefix=prefix))          # short: a unix socket path has a length limit
    TEMPS.append(str(path))
    return path


def wait_for(check, seconds, step=0.3):
    until = time.monotonic() + seconds * SCALE
    while time.monotonic() < until:
        got = check()
        if got:
            return got
        time.sleep(step)
    return None


def allowance():
    """The 5 h and 7 d figures of the latest status-line file (`--status-files`); only those two numbers (and how old the file is) leave this function."""
    files = sorted(Path(STATUS_FILES[0]).expanduser().parent.glob(Path(STATUS_FILES[0]).name), key=lambda p: p.stat().st_mtime) if STATUS_FILES else []
    if not files:
        return None
    try:
        limits = json.loads(files[-1].read_text()).get("rate_limits") or {}
    except (OSError, ValueError):
        return None
    return {"five_hour": (limits.get("five_hour") or {}).get("used_percentage"), "seven_day": (limits.get("seven_day") or {}).get("used_percentage"),
            "read_at": time.strftime("%H:%M:%S"), "file_age_s": int(time.time() - files[-1].stat().st_mtime)}


def program_env():
    return {"PATH": os.environ["PATH"], "LANG": os.environ.get("LANG", "C.UTF-8"), "HOME": os.environ["HOME"]}


class Ctx:
    """A private host and what the rows need of it."""

    def __init__(self, lico, claude_args, socket=None, state=None, work=None):
        self.lico = lico
        self.sessions: list[str] = []
        self.denied = 0
        self.work = work or temp("s4c-w")
        self.sock_dir = temp("s4c-s")
        self.socket = socket or str(self.sock_dir / "s")
        self.state = Path(state or self.work / "state")
        self.log = self.work / "log"
        received = self.work / "codex_received.jsonl"
        codex = ["--codex", sys.executable, "--codex-arg", "-I", "--codex-arg", str(CODEX_SIM), "--codex-arg", "--received", "--codex-arg", str(received),
                 "--codex-arg", "--fixture", "--codex-arg", str(CODEX_FIXTURE)]
        argv = [lico, "host", "--socket", self.socket, "--state-dir", str(self.state), "--log-dir", str(self.log), "--sessions", "0", "--producing", "0", *claude_args, *codex]
        self.proc = subprocess.Popen(argv, env=program_env(), stdout=subprocess.DEVNULL, stderr=open(self.work / "host.err", "wb"), cwd=self.work)
        self.conn = None
        for _ in range(100):                           # not scaled: a host needs its time (and a socket file left by an earlier host is there before the new one listens)
            try:
                self.conn = ProtoConn(self.socket, timeout=60.0)
                break
            except OSError:
                time.sleep(0.15)
        if self.conn is None:
            raise SystemExit("the private host did not come up")

    def stop(self):
        try:
            subprocess.run([self.lico, "--socket", self.socket, "host", "stop", "--force"], env=program_env(), capture_output=True, timeout=30)
            self.proc.wait(timeout=10)
        except Exception:
            self.proc.kill()
            self.proc.wait()

    def create(self, name, kind="claude", **extra):
        made = self.conn.call(op="session.create", kind=kind, name=name, **extra)
        if made.get("ok") != 1:
            raise SystemExit(f"session.create failed: {json.dumps(made)[:200]}")
        if kind == "claude":                          # only the sessions of the real program cost model turns
            self.sessions.append(made["session"])
        return made["session"], made["generation"]

    def row(self, sid):
        return next(s for s in self.conn.call(op="status")["sessions"] if s["session"] == sid)

    def records(self, sid):
        return [p.decode(errors="replace") for _, p in read_log(self.log, sid)]

    def turns(self):
        """The turns the check has paid for: the finished turns of every session it made."""
        return sum(1 for sid in self.sessions for r in self.records(sid) if r in FINISHED)

    def service(self):
        """The program's own questions (they carry no `tool`) are denied and counted, as nobody is watching; the host's own cards are the check's to answer."""
        for row in self.conn.call(op="status")["sessions"]:
            for card in row.get("approvals") or []:
                if "tool" not in card:
                    self.conn.call(op="approval.answer", session=row["session"], generation=row["generation"], approval_id=card["approval_id"], answer="deny")
                    self.denied += 1

    def wait(self, check, seconds):
        until = time.monotonic() + seconds * SCALE
        while time.monotonic() < until:
            self.service()
            got = check()
            if got:
                return got
            time.sleep(0.4)
        return None

    def say_to(self, sid, generation, text, tag):
        reply = self.conn.call(op="input", session=sid, generation=generation, request_id=tag, text=text)
        if reply.get("ok") != 1:
            raise SystemExit(f"input refused: {json.dumps(reply)[:200]}")

    def close(self, *pairs):
        """The programs of a finished try are ended; the rows and the records stay."""
        for sid, generation in pairs:
            self.conn.call(op="session.close", session=sid, generation=generation)


def finished(records, start=0):
    return sum(1 for r in records[start:] if r in FINISHED)


def tools_in(records):
    return [r[len("[tool] "):] for r in records if r.startswith("[tool] ")]


def pending_index(records):
    return next((i for i, r in enumerate(records) if r.startswith("> ") and re.search(r'"status"\s*:\s*"pending"', r)), None)


def polled(records, cut):
    """Did the model call a tool of Lico after the answer `pending` and before its turn ended? (A call before it, refused for its arguments, is not that.)
    It is false too when the answer `pending` is not in the records at all; a verdict that says "did not poll" asks for `pending_seen` as well."""
    answer = pending_index(records)
    return answer is not None and any(t in TOOLS for t in tools_in(records[answer + 1:cut]))


def notice_index(records):
    return next((i for i, r in enumerate(records) if r.startswith(">> [from lico | ")), None)


def agent_lines(records, start=0, stop=None):
    return [r for r in records[start:stop] if r and not r.startswith((">", "[", "$"))]


def words(lines):
    return sum(len(line.split()) for line in lines)


def card_of(ctx, sid, tool, seconds=30):
    return ctx.wait(lambda: next((c for c in (ctx.row(sid).get("approvals") or []) if c.get("tool") == tool), None), seconds)


def allow(ctx, sid, generation, card):
    return ctx.conn.call(op="approval.answer", session=sid, generation=generation, approval_id=card["approval_id"], answer="allow")


def pair(ctx, n):
    a = ctx.create(f"caller-{n}", mode="default", model="haiku")
    b = ctx.create(f"receiver-{n}", mode="default", model="haiku")
    return a, b


def room(ctx, needed, what):
    if ctx.turns() + needed > BUDGET:
        say(f"SKIP {what}: {ctx.turns()} turns used, {needed} more would go over {BUDGET}")
        return False
    return True


def try_send(ctx, n, long_answer=False, handover=False):
    """One `send` from A to B. Returns the facts of the try (all text is the check's own prompts and the model's short answers)."""
    (a, ga), (b, gb) = pair(ctx, n)
    prompt = f"Send the message '{PONG}' to the session {b} with the tool mcp__lico__send. Use no other tool."
    if long_answer:
        prompt += " When the tool has answered, write about 600 words on how a library keeps its catalogue in order, in plain paragraphs, with no tool."
    ctx.say_to(a, ga, prompt, f"s04-{n}")
    facts = {"try": n, "kind": "long" if long_answer else "send", "caller": a[:7], "receiver": b[:7]}
    card = card_of(ctx, a, "send", 120)
    facts["card"] = card["prompt"] if card else None
    if not card:
        facts["verdict"] = "the model never called send (no card in 120 s)"
        facts["a_records"] = ctx.records(a)
        ctx.close((a, ga), (b, gb))
        return facts
    if long_answer:
        ctx.wait(lambda: pending_index(ctx.records(a)) is not None, 60)         # the answer reached the model: now the notice goes while it writes
        said = allow(ctx, a, ga, card)
    else:
        ended = ctx.wait(lambda: finished(ctx.records(a)) >= 1, 90)
        facts["turn_ended"] = bool(ended)
        said = None
    first = ctx.records(a)
    cut = next((i for i, r in enumerate(first) if r in FINISHED), len(first))
    facts["tools_in_first_turn"] = tools_in(first[:cut])
    facts["polled"] = polled(first, cut)
    facts["pending_seen"] = pending_index(first) is not None
    if not long_answer:
        said = allow(ctx, a, ga, card)
    facts["answer_op"] = {k: said.get(k) for k in ("ok", "state", "outcome", "error")}
    got_b = ctx.wait(lambda: (lambda r: finished(r) >= 1 and any(x.startswith(">> [from session ") for x in r))(ctx.records(b)), 120)
    facts["receiver_answered"] = bool(got_b)
    ctx.wait(lambda: notice_index(ctx.records(a)) is not None and finished(ctx.records(a)) >= 2, 120)
    after = ctx.records(a)
    n_at = notice_index(after)
    facts["notice_at"] = n_at
    facts["notice_record"] = after[n_at:n_at + 3] if n_at is not None else None
    facts["pending_record"] = after[pending_index(after)] if pending_index(after) is not None else None
    facts["a_turns_finished"] = finished(after)
    facts["b_turns_finished"] = finished(ctx.records(b))
    facts["b_text"] = agent_lines(ctx.records(b))[:3]
    if n_at is not None:
        first_done = next((i for i, r in enumerate(after) if r in FINISHED), len(after))
        facts["notice_before_first_turn_end"] = n_at < first_done
        facts["second_turn_text"] = agent_lines(after, n_at)[:3]
        facts["second_turn_answers"] = finished(after, n_at) >= 1 and bool(agent_lines(after, n_at))
        p = pending_index(after)
        essay = agent_lines(after, (p or 0) + 1, n_at if n_at < first_done else first_done)
        facts["words_before_notice"] = words(essay)
        facts["words_in_first_turn"] = words(agent_lines(after, (p or 0) + 1, first_done))
    if long_answer:
        facts["verdict"] = ("held" if facts.get("second_turn_answers") and facts.get("words_in_first_turn", 0) >= 450 and facts.get("notice_before_first_turn_end") else "check the records")
    else:
        facts["verdict"] = "held" if (facts.get("turn_ended") and facts["pending_seen"] and not facts["polled"] and facts["receiver_answered"] and facts.get("second_turn_answers")) else "did not hold"
    if handover:
        facts["handover"] = hand_over(ctx, a)
    ctx.close((a, ga), (b, gb))
    return facts


def hand_over(ctx, sender):
    """The conversation of `sender` to a session of the Codex simulator, and the head of the stored file (first 12 lines)."""
    target, _ = ctx.create("handover-target", kind="codex")
    reply = ctx.conn.call(op="transfer.send", **{"from": sender, "to": target, "kind": "conversation"})
    found = {"reply": reply}
    files = sorted((ctx.state / "handoff" / target).rglob("*conversation.md")) if (ctx.state / "handoff" / target).exists() else []
    if files:
        text = files[0].read_text()
        body = text.partition("\n\n")[2]                         # after the lines Lico wrote about the file (from, to, when, kind, size)
        found["head"] = body.splitlines()[:12]
        found["whole_lines"] = len(text.splitlines())
        found["head_lines_to_blank"] = body.partition("\n\n")[0].splitlines()
        found["has"] = {key: (key in text) for key in ("Compaction: none found", "Not shown:", "Skipped", "record_format")}
        found["thinking_lines_in_head"] = [line for line in found["head"] if "thinking" in line]
    return found


def row_create(ctx):
    a, ga = ctx.create("caller-c", mode="default", model="haiku")
    prompt = "Make a new session with the tool mcp__lico__create. Its first prompt is: Reply with the single word ok. Use no other tool."
    before = {s["session"] for s in ctx.conn.call(op="status")["sessions"]}
    ctx.say_to(a, ga, prompt, "s04-create")
    facts = {"kind": "create", "caller": a[:7]}
    card = card_of(ctx, a, "create", 120)
    facts["card"] = card["prompt"] if card else None
    if not card:
        facts["verdict"] = "the model never called create (no card in 120 s)"
        facts["a_records"] = ctx.records(a)
        return facts
    ended = ctx.wait(lambda: finished(ctx.records(a)) >= 1, 90)
    first = ctx.records(a)
    cut = next((i for i, r in enumerate(first) if r in FINISHED), len(first))
    facts.update(turn_ended=bool(ended), tools_in_first_turn=tools_in(first[:cut]), polled=polled(first, cut), pending_seen=pending_index(first) is not None)
    said = allow(ctx, a, ga, card)
    facts["answer_op"] = {k: said.get(k) for k in ("ok", "state", "outcome", "error")}
    child = ctx.wait(lambda: next((s for s in ctx.conn.call(op="status")["sessions"] if s["session"] not in before), None), 60)
    facts["child"] = bool(child)
    if child:
        ctx.sessions.append(child["session"])
        facts["child_name"] = child.get("name")
        ctx.wait(lambda: finished(ctx.records(child["session"])) >= 1, 120)
        recs = ctx.records(child["session"])
        facts["child_first_prompt_seen"] = any(r.startswith(">> [from session ") for r in recs)
        facts["child_started"] = next((r for r in recs if r.startswith("[session started]")), None)
        facts["child_text"] = agent_lines(recs)[:3]
    ctx.wait(lambda: notice_index(ctx.records(a)) is not None and finished(ctx.records(a)) >= 2, 120)
    after = ctx.records(a)
    n_at = notice_index(after)
    facts["notice_record"] = after[n_at:n_at + 3] if n_at is not None else None
    facts["pending_record"] = after[pending_index(after)] if pending_index(after) is not None else None
    facts["second_turn_text"] = agent_lines(after, n_at)[:3] if n_at is not None else None
    facts["verdict"] = "held" if (facts["turn_ended"] and facts["pending_seen"] and not facts["polled"] and facts.get("child_first_prompt_seen") and n_at is not None and finished(after) >= 2) else "did not hold"
    if child:
        ctx.close((child["session"], child["generation"]))
    ctx.close((a, ga))
    return facts


def row_gone(lico, claude_args, turn=True, shots=None):
    """A session whose conversation the program no longer has, in the product window. `turn`: the session has its first (real) turn and the program's record file is then moved
    away; without it (no model call) the session never had a turn, the host is stopped, the registry is given an id the program has no record of, and the host is started again:
    the program is asked to resume a conversation it does not have, which is the same thing for it."""
    from lico_measure.captures import ProductWindow
    from tests.test_session_actions_window import row_centre

    kept, found = {}, {"kind": "gone" if turn else "gone0"}

    def before(w):
        (Path(w.runtime) / "lico").mkdir(mode=0o700, exist_ok=True)
        ctx = Ctx(lico, claude_args, socket=w.socket, state=Path(w.directory) / "state" / "lico")
        kept["ctx"] = ctx
        sid, gen = ctx.create("gone-1", mode="default", model="haiku")
        kept["sid"], kept["gen"] = sid, gen
        if not turn:
            ctx.stop()                                 # a session with no conversation yet cannot be closed; a stopped host leaves it in the registry
            registry_file = ctx.state / "registry" / "sessions.json"
            document = json.loads(registry_file.read_text())
            for entry in document["sessions"]:
                if entry["session"] == sid:
                    entry["provider_session"] = str(uuid.uuid4())
            registry_file.write_text(json.dumps(document))
            ctx = Ctx(lico, claude_args, socket=w.socket, state=Path(w.directory) / "state" / "lico", work=ctx.work)
            kept["ctx"] = ctx
            if not ctx.wait(lambda: ctx.row(sid)["state"] == "disconnected", 30):
                raise SystemExit("the session did not come back disconnected")
            kept["old_generation"] = ctx.row(sid)["generation"]
            return
        ctx.say_to(sid, gen, "Reply with the single word ok.", "s04-gone-1")
        if not ctx.wait(lambda: finished(ctx.records(sid)) >= 1, 120):
            raise SystemExit("the first turn of the gone row did not finish")
        registry = json.loads((ctx.state / "registry" / "sessions.json").read_text())["sessions"]
        provider_id = next(e["provider_session"] for e in registry if e["session"] == sid)
        found["conversation_handover"] = hand_over(ctx, sid)               # while the record is there: what the reader does with this program's version
        records = list((Path.home() / ".claude" / "projects").glob(f"*/{provider_id}.jsonl"))
        found["record_files_found"] = len(records)
        if len(records) != 1 and not SIM:
            raise SystemExit(f"expected one record file of the program, found {len(records)}")
        if records:
            shutil.move(str(records[0]), str(Path(w.directory) / "moved_record.jsonl"))
        ctx.conn.call(op="session.close", session=sid, generation=gen)
        if not ctx.wait(lambda: ctx.row(sid)["state"] == "disconnected", 30):
            raise SystemExit("the session did not become disconnected")
        kept["old_generation"] = ctx.row(sid)["generation"]

    with ProductWindow(host_binary=None, before_window=before) as w:
        ctx, sid = kept["ctx"], kept["sid"]
        try:
            w.settle(w.frames(), 1.0)
            w.click(*row_centre(w, sid))
            found["selected"] = w.ui()["selected"] == sid
            found["hint_before"] = w.ui()["hint"]
            before_records = len(ctx.records(sid))
            w.type_text("second question")
            started = time.monotonic()
            w.key("Return")
            timeline, last = [], None
            while time.monotonic() - started < 45:
                row = ctx.row(sid)
                now = (row["state"], row["generation"])
                if now != last:
                    timeline.append({"t": round(time.monotonic() - started, 2), "state": row["state"], "generation": row["generation"]})
                    last = now
                if row["state"] == "disconnected" and row["generation"] > kept["old_generation"] and time.monotonic() - started > 1.0:
                    break
                time.sleep(0.2)
            found["timeline"] = timeline
            found["seconds_until_disconnected_again"] = round(time.monotonic() - started, 2) if last and last[0] == "disconnected" and last[1] > kept["old_generation"] else None
            time.sleep(2.0)
            state = w.ui()
            found["window_after_2s"] = {"hint": state["hint"], "editor_text": state["editor"]["text"], "editor_note": state["editor"]["note"], "selected": state["selected"],
                                        "row": [{k: r[k] for k in ("state", "state_text", "activity", "generation")} for r in state["rows"] if r["session"] == sid],
                                        "header": state["header"], "activity_line": state["activity_line"], "lines_buffered": state["lines_buffered"]}
            if shots:
                w.png(Path(shots) / f"s04_window_{'gone' if turn else 'gone0'}.png")
            time.sleep(8.0)
            state = w.ui()
            found["window_after_10s"] = {"hint": state["hint"], "editor_text": state["editor"]["text"], "editor_note": state["editor"]["note"]}
            found["history_after_enter"] = ctx.records(sid)[before_records:]
            found["row_at_end"] = {k: ctx.row(sid).get(k) for k in ("state", "generation")}
        finally:
            ctx.stop()
    return found


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--lico", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--rows", default="send,long,create,gone", help="gone0: the gone row without a model turn (see row_gone)")
    parser.add_argument("--shots", help="a folder for a screenshot of the window at the end of the gone rows")
    parser.add_argument("--tries", type=int, default=3)
    parser.add_argument("--status-files", help="a glob of the JSON files a status line writes (its `rate_limits` has `five_hour` and `seven_day` with `used_percentage`); without it no allowance figures are read")
    parser.add_argument("--claude", help="the program for the send, long and create rows (default: the pinned version under the program's own versions folder)")
    parser.add_argument("--claude-latest", help="the program for the gone row (default: claude on PATH)")
    parser.add_argument("--sim", action="store_true", help="the Claude simulator instead of the program: a dry run of the script, no model call")
    args = parser.parse_args()
    rows = args.rows.split(",")
    STATUS_FILES[:] = [args.status_files] if args.status_files else []
    lico = str(Path(args.lico).resolve())
    if args.sim:
        global SIM, SCALE
        SIM, SCALE = True, 0.05
        pinned = latest = ["--claude", sys.executable, "--claude-arg", "-I", "--claude-arg", str(CLAUDE_SIM), "--claude-arg", "--fixture", "--claude-arg", str(CLAUDE_FIXTURE)]
        versions = {"pinned": "simulator", "latest": "simulator"}
    else:
        pinned_path = args.claude or str(Path.home() / ".local" / "share" / "claude" / "versions" / json.loads((REPO / "src/host/adapters/claude/capabilities.json").read_text())["version"]["cli"])
        latest_path = args.claude_latest or shutil.which("claude")
        for p in (pinned_path, latest_path):
            if not p or not os.path.exists(p):
                raise SystemExit(f"the program was not found: {p}")
        pinned, latest = ["--claude", pinned_path], ["--claude", latest_path]
        versions = {k: subprocess.run([p, "--version"], capture_output=True, text=True, timeout=30).stdout.strip() for k, p in (("pinned", pinned_path), ("latest", latest_path))}
    RESULT["program_versions"] = versions
    RESULT["allowance_before"] = allowance()
    say("versions", versions, "allowance before", RESULT["allowance_before"])
    used = 0
    try:
        if any(r in rows for r in ("send", "long", "create")):
            ctx = Ctx(lico, pinned)
            try:
                if "send" in rows:
                    for n in range(1, args.tries + 1):
                        if room(ctx, 3, f"send try {n}"):
                            facts = try_send(ctx, n, handover=(n == 1))
                            RESULT["rows"][f"send-{n}"] = facts
                            say(f"send try {n}: {facts['verdict']}  (turns used {ctx.turns()})")
                if "long" in rows and room(ctx, 3, "long try"):
                    facts = try_send(ctx, 9, long_answer=True)
                    RESULT["rows"]["long"] = facts
                    say(f"long try: {facts['verdict']}  (turns used {ctx.turns()})")
                if "create" in rows and room(ctx, 3, "create try"):
                    facts = row_create(ctx)
                    RESULT["rows"]["create"] = facts
                    say(f"create try: {facts['verdict']}  (turns used {ctx.turns()})")
                used += ctx.turns()
                RESULT["program_questions_denied"] = ctx.denied
                RESULT["host_err_audit_lines"] = [l for l in (ctx.work / "host.err").read_text().splitlines() if l.startswith("lico-host: call ") or l.startswith("lico-host: request ")]
            finally:
                ctx.stop()
        if "gone" in rows and used + 1 <= BUDGET:
            RESULT["rows"]["gone"] = row_gone(lico, latest, turn=True, shots=args.shots)
            say("gone row done")
            used += 1
        if "gone0" in rows:
            RESULT["rows"]["gone0"] = row_gone(lico, latest, turn=False, shots=args.shots)
            say("gone row without a model turn done")
    finally:
        RESULT["turns_used_counted"] = used
        RESULT["allowance_after"] = allowance()
        Path(args.out).write_text(json.dumps(RESULT, indent=1, ensure_ascii=False))
        say("allowance after", RESULT["allowance_after"], "turns counted", used)
        for path in TEMPS:
            shutil.rmtree(path, ignore_errors=True)


if __name__ == "__main__":
    main()
