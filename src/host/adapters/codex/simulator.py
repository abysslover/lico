"""A protocol simulator standing in for `codex app-server` (a test double, not the provider).

It speaks JSON lines on standard input and output with the message shapes of the
provider's documented, schema-valid protocol, and logs every message it receives so a
test can see exactly what the host sent. Modes are chosen by the text of a turn:
  "fixture"   replays the recorded turn (waiting for the host's approval answers)
  "crash"     exits abruptly after acknowledging the turn
  "slow:N"    N output lines, one every 100 ms, so a turn is still running
  "bad"       malformed, oversize and endless messages
  "endless"   forty megabytes of one line that never ends, then a normal message
  "hostile"   unending partial lines, thousands of item ids, hundreds of approval requests (one id repeated), an unknown server request
  "escape"    terminal escapes (clear screen, clipboard write, hyperlink, title, backspaces, bidi override, bell) and a very long line
  "env"       reports the names (never the values) of its environment, its working directory and its mode
  otherwise   echoes the text back, character for character
"""

import argparse
import json
import os
import queue
import sys
import threading
import time

parser = argparse.ArgumentParser()
parser.add_argument("--received", default="")
parser.add_argument("--fixture", default="")
args, _ = parser.parse_known_args()

lines: "queue.Queue[str | None]" = queue.Queue()
threading.Thread(target=lambda: [lines.put(l) for l in sys.stdin] + [lines.put(None)], daemon=True).start()
log = open(args.received, "a", buffering=1) if args.received else None
turn_counter = 0
active = {"turn": None, "thread": "th-1"}
scheduled = []          # (time, message)
waiting = {}            # request id -> remaining fixture steps after it is answered
replay = []


def out(message):
    sys.stdout.write(json.dumps(message, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def notify(method, params):
    out({"method": method, "params": params})


def thread_object(thread_id):
    return {"id": thread_id, "cliVersion": "0.163.0", "createdAt": 1, "cwd": "/work/session", "ephemeral": True,
            "modelProvider": "simulator", "preview": "", "projectId": None, "sessionId": "se-1", "source": "appServer",
            "status": {"type": "idle"}, "turns": [], "updatedAt": 1}


def advance():
    """Emit recorded server messages until one needs the host's answer."""
    while replay:
        step = replay.pop(0)
        if step["dir"] == "user":
            if step["request_id"] in waiting:      # the answer has not arrived yet: stop here
                replay.insert(0, step)
                return
            continue                               # it already arrived
        message = dict(step["msg"])
        out(message)
        if "id" in message and message.get("method") in ("item/commandExecution/requestApproval", "item/fileChange/requestApproval"):
            waiting[message["id"]] = True          # an approval: wait for the host's answer
            return


def handle(message):
    global turn_counter
    method, rid, params = message.get("method"), message.get("id"), message.get("params", {})
    if method is None:                         # a response to one of our requests
        if rid in waiting:
            waiting.pop(rid)
            advance()
        return
    if method == "initialize":
        out({"id": rid, "result": {"userAgent": "simulator/0.163.0", "codexHome": "<private>", "platformFamily": "unix", "platformOs": "linux"}})
    elif method == "initialized":
        return
    elif method in ("thread/start", "thread/resume"):
        tid = params.get("threadId", "th-1")
        active["thread"] = tid
        out({"id": rid, "result": {"thread": thread_object(tid)}})
        if method == "thread/start":
            notify("thread/started", {"thread": thread_object(tid)})
    elif method == "turn/start":
        turn_counter += 1
        tid = f"tu-{turn_counter}"
        active["turn"] = tid
        text = "".join(i.get("text", "") for i in params.get("input", []))
        out({"id": rid, "result": {"turn": {"id": tid, "items": [], "status": "inProgress"}}})
        notify("turn/started", {"threadId": active["thread"], "turn": {"id": tid, "items": [], "status": "inProgress"}})
        if text == "crash":
            sys.exit(3)
        elif text == "fixture":
            replay.extend(json.load(open(args.fixture)))
            replay[:] = [s for s in replay if not (s["dir"] == "server" and s["msg"].get("method") in ("thread/started", "turn/started"))]
            advance()
        elif text.startswith("slow:"):
            count = int(text.split(":")[1])
            now = time.monotonic()
            for k in range(count):
                scheduled.append((now + 0.1 * (k + 1), ("delta", f"tick {k}\n")))
            scheduled.append((now + 0.1 * (count + 1), ("done", None)))
        elif text == "escape":
            esc, bel = chr(27), chr(7)
            body = ("before\n" + esc + "[2J" + esc + "[H" + "cleared?\n" + esc + "]52;c;aGVsbG8=" + bel + "clip?\n"
                    + esc + "]8;;http://example.invalid" + esc + "\\link" + esc + "]8;;" + esc + "\\\n"
                    + "back" + chr(8) * 4 + "spoof\n" + "\u202etxet\u202c\n" + bel + "bell\n" + esc + "]0;pwned" + bel + "title\n"
                    + "x" * 10000 + "\nvisible after escapes\n")
            notify("item/agentMessage/delta", {"threadId": active["thread"], "turnId": tid, "itemId": "esc", "delta": body})
            notify("turn/completed", {"threadId": active["thread"], "turn": {"id": tid, "items": [], "status": "completed"}})
            active["turn"] = None
        elif text == "env":
            iid = f"env-{turn_counter}"
            lines_out = [f"ENV {name}" for name in sorted(os.environ)] + [f"CWD {os.getcwd()}", f"CWDMODE {oct(os.stat('.').st_mode & 0o777)[2:]}"]
            notify("item/agentMessage/delta", {"threadId": active["thread"], "turnId": tid, "itemId": iid, "delta": "\n".join(lines_out) + "\n"})
            notify("turn/completed", {"threadId": active["thread"], "turn": {"id": tid, "items": [], "status": "completed"}})
            active["turn"] = None
        elif text == "endless":
            for _ in range(40):
                sys.stdout.write("y" * 1_000_000)
                sys.stdout.flush()
            sys.stdout.write("\n")
            notify("item/agentMessage/delta", {"threadId": active["thread"], "turnId": tid, "itemId": "e", "delta": "endless survived\n"})
            notify("turn/completed", {"threadId": active["thread"], "turn": {"id": tid, "items": [], "status": "completed"}})
            active["turn"] = None
        elif text == "hostile":
            th = active["thread"]
            for _ in range(300):                                   # 30 MB in one item that never gets a newline
                notify("item/agentMessage/delta", {"threadId": th, "turnId": tid, "itemId": "long", "delta": "z" * 100_000})
            for k in range(3000):                                  # many items, each with a partial line
                notify("item/agentMessage/delta", {"threadId": th, "turnId": tid, "itemId": f"many-{k}", "delta": "partial"})
            command = "rm -rf " + chr(27) + "]0;pwned" + chr(7) + chr(27) + "[2J" + "A" * 100_000
            for k in range(300):
                out({"id": 1000 + k, "method": "item/commandExecution/requestApproval", "params": {"threadId": th, "turnId": tid, "itemId": f"c{k}", "command": command}})
            out({"id": 1000, "method": "item/commandExecution/requestApproval", "params": {"threadId": th, "turnId": tid, "itemId": "again", "command": "same id again"}})
            out({"id": 5000, "method": "item/tool/call", "params": {"threadId": th, "turnId": tid, "tool": "anything"}})
            notify("item/agentMessage/delta", {"threadId": th, "turnId": tid, "itemId": "end", "delta": "hostile survived\n"})
            notify("turn/completed", {"threadId": th, "turn": {"id": tid, "items": [], "status": "completed"}})
            active["turn"] = None
        elif text == "bad":
            sys.stdout.write("this is not json\n")
            sys.stdout.write(json.dumps({"method": "item/agentMessage/delta", "params": {"itemId": "x"}}) + "\n")
            sys.stdout.write("x" * 3_000_000 + "\n")
            sys.stdout.flush()
            notify("item/agentMessage/delta", {"threadId": active["thread"], "turnId": tid, "itemId": "it", "delta": "still alive\n"})
            notify("turn/completed", {"threadId": active["thread"], "turn": {"id": tid, "items": [], "status": "completed"}})
        else:
            iid = f"it-{turn_counter}"
            notify("item/started", {"threadId": active["thread"], "turnId": tid, "startedAtMs": 1, "item": {"id": iid, "type": "agentMessage", "text": ""}})
            notify("item/agentMessage/delta", {"threadId": active["thread"], "turnId": tid, "itemId": iid, "delta": "echo: " + text + "\n"})
            notify("item/completed", {"threadId": active["thread"], "turnId": tid, "completedAtMs": 2, "item": {"id": iid, "type": "agentMessage", "text": text}})
            notify("turn/completed", {"threadId": active["thread"], "turn": {"id": tid, "items": [], "status": "completed"}})
            active["turn"] = None
    elif method == "turn/steer":
        if params.get("expectedTurnId") == active["turn"]:
            text = "".join(i.get("text", "") for i in params.get("input", []))
            out({"id": rid, "result": {"turnId": active["turn"]}})
            notify("item/agentMessage/delta", {"threadId": active["thread"], "turnId": active["turn"], "itemId": "steer", "delta": "steer: " + text + "\n"})
        else:
            out({"id": rid, "error": {"code": -32000, "message": "no matching active turn"}})
    elif method == "turn/interrupt":
        out({"id": rid, "result": {}})
        notify("turn/completed", {"threadId": active["thread"], "turn": {"id": active["turn"], "items": [], "status": "interrupted"}})
        scheduled.clear()
        active["turn"] = None
    elif rid is not None:
        out({"id": rid, "error": {"code": -32601, "message": "unknown method"}})


while True:
    timeout = None
    if scheduled:
        timeout = max(0.0, min(t for t, _ in scheduled) - time.monotonic())
    try:
        raw = lines.get(timeout=timeout)
    except queue.Empty:
        raw = ""
    if raw is None:
        break
    if raw:
        if log:
            log.write(raw if raw.endswith("\n") else raw + "\n")
        try:
            handle(json.loads(raw))
        except ValueError:
            pass
    now = time.monotonic()
    for item in [s for s in scheduled if s[0] <= now]:
        scheduled.remove(item)
        kind, text = item[1]
        if kind == "delta":
            notify("item/agentMessage/delta", {"threadId": active["thread"], "turnId": active["turn"], "itemId": "slow", "delta": text})
        elif active["turn"]:
            notify("turn/completed", {"threadId": active["thread"], "turn": {"id": active["turn"], "items": [], "status": "completed"}})
            active["turn"] = None
