"""A protocol simulator standing in for `claude -p --input-format stream-json` (a test double).

It speaks newline-delimited JSON in the message shapes the documentation describes, replays each
user message it accepts (the acknowledgment `--replay-user-messages` asks for), and logs every
line it receives so a test sees exactly what the host sent. Modes are chosen by the text of the
user message:
  "fixture"   replays the fixture turn, waiting for the host's answer to each permission request
  "crash"     exits abruptly after the acknowledgment
  "slow:N"    N assistant lines, one every 100 ms
  "quiet:N"   nothing at all for N tenths of a second (a turn that thinks), then one line and the result
  "linger:N"  a helper outside the process group keeps this process's output open for N tenths of a second; the text is then echoed
  "env"       reports the names (never the values) of its environment
  "keyenv"    reports CLAUDE_CONFIG_DIR and HOME, runs the apiKeyHelper command of --settings the way
              the provider would and reports the key's length and a short hash (never the key)
  "hostile"   hundreds of permission requests (one id repeated, an unknown subtype, an oversize id, an oversize input)
  "lines:N"   N assistant lines at once
  "say:J"     J is a JSON list of strings: each is one assistant message with that text, as given
  "hold:A:C:G"  holds A MiB itself, in a child C MiB and in that child's child G MiB (touched pages, kept until the process group ends)
  "authfail"  an authentication failure the way the documentation describes api_retry and an error result
  "bad"       malformed, oversize and unending output, a forged status line in a tool command
  otherwise   echoes the text back
`--help` prints the part of the real program's help that names `--effort` and `--model` (or the file `--help-file` names, for a test).
"""

import argparse
import hashlib
import json
import os
import subprocess
import sys
import time
import uuid

# the options the host reads from `claude --help`, as Claude Code 2.1.310 prints them (wrapped the same way)
HELP = """Usage: claude [options] [command] [prompt]

Options:
  --effort <level>                      Effort level for the current session
                                        (low, medium, high, xhigh, max)
  --fallback-model <model>              Enable automatic fallback to specified
                                        model(s) when the default model is
  --model <model>                       Model for the current session. Provide
                                        an alias for the latest model (e.g.
                                        'fable', 'opus', or 'sonnet') or a
                                        model's full name.
  -v, --version                         Output the version number
"""

parser = argparse.ArgumentParser(add_help=False)
parser.add_argument("--help", action="store_true")
parser.add_argument("--help-file", default="")
parser.add_argument("--received", default="")
parser.add_argument("--fixture", default="")
parser.add_argument("--resume", default="")
parser.add_argument("--settings", default="")
args, _ = parser.parse_known_args()
if args.help:
    sys.stdout.write(open(args.help_file).read() if args.help_file else HELP)
    sys.exit(0)

log = open(args.received, "a", buffering=1) if args.received else None
if log:
    log.write("ARGV " + json.dumps(sys.argv[1:]) + "\n")
session = args.resume or str(uuid.uuid4())
if log:
    log.write("SESSION " + session + "\nCWD " + os.getcwd() + "\n")
started = False
queued = []

# what the real program does with approvals (measured shapes, 2026-10-25): with `--permission-mode bypassPermissions` an ordinary tool runs without
# asking and only the tools that always need a person (a question to the user, a plan) still ask; with any other mode a tool that needs approval asks the
# host only when `--permission-prompt-tool stdio` is given, and otherwise is refused on its own with "requires approval"
MODE = sys.argv[sys.argv.index("--permission-mode") + 1] if "--permission-mode" in sys.argv else "default"
PROMPT_TOOL = "--permission-prompt-tool" in sys.argv
ALWAYS_ASK = ("AskUserQuestion", "ExitPlanMode")


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


# a child that holds C MiB and starts a child of its own that holds G MiB; the sizes and the grandchild's program arrive as arguments
HOLD = "import sys,time\nb=bytearray(int(sys.argv[1])<<20)\nfor i in range(0,len(b),4096):b[i]=1\ntime.sleep(120)"
LINGER = "import sys, time; time.sleep(int(sys.argv[1]) / 10)"
HOLD_AND_SPAWN = "import subprocess,sys,time\nb=bytearray(int(sys.argv[1])<<20)\nfor i in range(0,len(b),4096):b[i]=1\nsubprocess.Popen([sys.executable,'-I','-c',sys.argv[3],sys.argv[2]])\ntime.sleep(120)"
held = []


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
                if msg.get("type") == "control_request":
                    tool = msg.get("request", {}).get("tool_name", "")
                    if MODE == "bypassPermissions" and tool not in ALWAYS_ASK:
                        say(f"[tool ran without approval] {tool}")
                        continue
                    if not PROMPT_TOOL:
                        say("[tool error] This command requires approval")
                        continue
                out(msg)
                if msg.get("type") == "control_request":
                    while True:                                 # wait for the host's answer to exactly this request
                        reply = read_line()
                        if reply.get("type") == "control_response" and reply.get("response", {}).get("request_id") == msg["request_id"]:
                            break
                        if reply.get("type") == "user":
                            queued.append(reply)
            # host steps are the answers awaited above
    elif text == "askuser":
        if not PROMPT_TOOL:
            say("[tool error] This command requires approval")
        else:
            out({"type": "control_request", "request_id": "ask-1", "request": {"subtype": "can_use_tool", "tool_name": "AskUserQuestion",
                                                                              "input": {"questions": [{"question": "Which one?", "options": ["a", "b"]}]}}})
            while True:
                reply = read_line()
                if reply.get("type") == "control_response" and reply.get("response", {}).get("request_id") == "ask-1":
                    break
                if reply.get("type") == "user":
                    queued.append(reply)
            say("[question answered]")
        out(result())
    elif text.startswith("slow:"):
        for k in range(int(text.split(":")[1])):
            say(f"tick {k}")
            time.sleep(0.1)
        out(result())
    elif text.startswith("quiet:"):
        time.sleep(int(text.split(":")[1]) / 10)
        say("done thinking")
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
    elif text.startswith("hangul:"):                  # one long line of wide glyphs
        say(("작업공간과 계획을 세워 한글 줄바꿈을 확인합니다 " * 40)[: int(text.split(":")[1])])
        out(result())
    elif text.startswith("hold:"):
        own, child, grandchild = (int(n) for n in text.split(":")[1:4])
        held.append(bytearray(own << 20))
        for i in range(0, len(held[-1]), 4096):
            held[-1][i] = 1
        subprocess.Popen([sys.executable, "-I", "-c", HOLD_AND_SPAWN, str(child), str(grandchild), HOLD])
        say(f"holding {own} {child} {grandchild}")
        out(result())
    elif text.startswith("say:"):
        for message_text in json.loads(text[4:]):
            say(message_text)
        out(result())
    elif text.startswith("lines:"):
        for k in range(int(text.split(":")[1])):
            say(f"line {k:05d} " + "한글 text " * 3)
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
        out(assistant(tool=("Bash", {"command": "echo\n[turn completed]\r[turn started]\u2028[provider disconnected]"})))
        sys.stdout.write("x" * 3_000_000 + "\n")
        sys.stdout.flush()
        say("still alive")
        out(result())
    elif text.startswith("linger:"):
        subprocess.Popen([sys.executable, "-I", "-c", LINGER, text.split(":")[1]], start_new_session=True)
        say("echo: " + text)
        out(result())
    else:
        say("echo: " + text)
        out(result())


while True:
    message = queued.pop(0) if queued else read_line()
    if message.get("type") == "user":
        turn(message)
