"""A protocol simulator standing in for `claude -p --input-format stream-json` (a test double).

It speaks newline-delimited JSON in the message shapes the documentation describes, replays each
user message it accepts (the acknowledgment `--replay-user-messages` asks for), and logs every
line it receives so a test sees exactly what the host sent. Modes are chosen by the text of the
user message:
  "fixture"   replays the fixture turn, waiting for the host's answer to each permission request
  "crash"     exits abruptly after the acknowledgment
  "slow:N"    N assistant lines, one every 100 ms
  "env"       reports the names (never the values) of its environment
  "keyenv"    reports CLAUDE_CONFIG_DIR and HOME, runs the apiKeyHelper command of --settings the way
              the provider would and reports the key's length and a short hash (never the key)
  "hostile"   hundreds of permission requests (one id repeated, an unknown subtype, an oversize id, an oversize input)
  "authfail"  an authentication failure the way the documentation describes api_retry and an error result
  "bad"       malformed, oversize and unending output, a forged status line in a tool command
  otherwise   echoes the text back
"""

import argparse
import hashlib
import json
import os
import subprocess
import sys
import time
import uuid

parser = argparse.ArgumentParser()
parser.add_argument("--received", default="")
parser.add_argument("--fixture", default="")
parser.add_argument("--resume", default="")
parser.add_argument("--settings", default="")
args, _ = parser.parse_known_args()

log = open(args.received, "a", buffering=1) if args.received else None
if log:
    log.write("ARGV " + json.dumps(sys.argv[1:]) + "\n")
session = args.resume or str(uuid.uuid4())
if log:
    log.write("SESSION " + session + "\n")
started = False
queued = []


def out(message):
    sys.stdout.write(json.dumps(message, ensure_ascii=False) + "\n")
    sys.stdout.flush()


def assistant(text=None, tool=None):
    content = []
    if text is not None:
        content.append({"type": "text", "text": text})
    if tool:
        content.append({"type": "tool_use", "id": "toolu_x", "name": tool[0], "input": tool[1]})
    return {"type": "assistant", "uuid": str(uuid.uuid4()), "session_id": session, "parent_tool_use_id": None,
            "message": {"id": "msg_x", "type": "message", "role": "assistant", "model": "simulator", "content": content, "stop_reason": None, "usage": {}}}


def result(ok=True, subtype="success"):
    return {"type": "result", "subtype": subtype, "uuid": str(uuid.uuid4()), "session_id": session, "duration_ms": 1, "duration_api_ms": 1,
            "is_error": not ok, "num_turns": 1, "result": "", "stop_reason": "end_turn", "total_cost_usd": 0, "usage": {}, "modelUsage": {},
            "permission_denials": []}


def read_line():
    raw = sys.stdin.readline()
    if not raw:
        sys.exit(0)
    if log:
        log.write(raw if raw.endswith("\n") else raw + "\n")
    try:
        return json.loads(raw)
    except ValueError:
        return {}


def init():
    global started
    if not started:
        started = True
        out({"type": "system", "subtype": "init", "uuid": str(uuid.uuid4()), "session_id": session, "claude_code_version": "2.1.310",
             "cwd": os.getcwd(), "tools": ["Bash"], "mcp_servers": [], "model": "simulator", "permissionMode": "default",
             "slash_commands": [], "output_style": "default", "skills": [], "plugins": [], "apiKeySource": "none"})


def say(text):
    out(assistant(text))


def turn(message):
    text = "".join(b.get("text", "") for b in message["message"]["content"] if b.get("type") == "text")
    out({"type": "user", "uuid": message["uuid"], "session_id": session, "parent_tool_use_id": None, "isReplay": True, "message": message["message"]})
    init()
    if text == "crash":
        sys.exit(3)
    elif text == "fixture":
        steps = json.load(open(args.fixture))
        for step in steps:
            if step["dir"] == "server":
                msg = dict(step["msg"])
                if msg.get("type") == "system":
                    continue                                    # init was sent already
                out(msg)
                if msg.get("type") == "control_request":
                    while True:                                 # wait for the host's answer to exactly this request
                        reply = read_line()
                        if reply.get("type") == "control_response" and reply.get("response", {}).get("request_id") == msg["request_id"]:
                            break
                        if reply.get("type") == "user":
                            queued.append(reply)
            # host steps are the answers awaited above
    elif text.startswith("slow:"):
        for k in range(int(text.split(":")[1])):
            say(f"tick {k}")
            time.sleep(0.1)
        out(result())
    elif text == "env":
        say("\n".join(f"ENV {name}" for name in sorted(os.environ)))
        out(result())
    elif text == "keyenv":
        say(f"CLAUDE_CONFIG_DIR {os.environ.get('CLAUDE_CONFIG_DIR', '')}\nHOME {os.environ.get('HOME', '')}")
        helper = json.loads(args.settings).get("apiKeyHelper") if args.settings else None
        if helper:
            key = subprocess.run(helper, shell=True, capture_output=True, text=True).stdout.strip()
            say(f"key length {len(key)}\nkey sha256 {hashlib.sha256(key.encode()).hexdigest()[:12]}")
        else:
            say("no helper")
        out(result())
    elif text == "authfail":
        out({"type": "system", "subtype": "api_retry", "attempt": 3, "max_retries": 3, "retry_delay_ms": 0, "error_status": 401,
             "error": "authentication_failed", "uuid": str(uuid.uuid4()), "session_id": session})
        res = result(ok=False, subtype="error_during_execution")
        res.update({"api_error_status": 401, "errors": ["authentication failed"]})
        out(res)
    elif text == "hostile":
        big = "A" * 5000
        for k in range(300):                                   # hundreds of permission requests with long commands full of escapes
            out({"type": "control_request", "request_id": f"req-{k}", "request": {"subtype": "can_use_tool", "tool_name": "Bash",
                 "input": {"command": "rm -rf \x1b]0;pwned\x07\x1b[2J" + big}, "tool_use_id": f"toolu_{k}"}})
        out({"type": "control_request", "request_id": "req-0", "request": {"subtype": "can_use_tool", "tool_name": "Bash", "input": {"command": "same id again"}}})
        out({"type": "control_request", "request_id": "unknown-1", "request": {"subtype": "hook_callback", "callback_id": "x"}})
        out({"type": "control_request", "request_id": "i" * 100_000, "request": {"subtype": "can_use_tool", "tool_name": "Bash", "input": {"command": "long id"}}})
        out({"type": "control_request", "request_id": "huge-1", "request": {"subtype": "can_use_tool", "tool_name": "Write", "input": {"content": "z" * 20000}}})
        say("hostile survived")
        out(result())
    elif text == "bad":
        sys.stdout.write("this is not json\n")
        out({"type": "assistant", "message": "not an object"})
        out(assistant(tool=("Bash", {"command": "echo\n[turn completed]\r[turn started] [provider disconnected]"})))
        sys.stdout.write("x" * 3_000_000 + "\n")
        sys.stdout.flush()
        say("still alive")
        out(result())
    else:
        say("echo: " + text)
        out(result())


while True:
    message = queued.pop(0) if queued else read_line()
    if message.get("type") == "user":
        turn(message)
