"""A protocol simulator standing in for `agy acp` (a test double): an Agent Client Protocol server over standard input and output (JSON lines, JSON-RPC 2.0).

`initialize`, `session/new` and the error codes are as the installed `agy 1.5.18` answered them in a private empty home (see `capabilities.json`); the updates of a turn,
the permission request and the stop reasons are from the ACP documentation. It logs every line it receives to `--received`, so a test sees exactly what the host sent.
Modes are chosen by the text of the prompt:
  "fixture"   replays the agent messages of the fixture's turn (`--fixture`), waiting for the host's answer to its permission request; an answer that is not an
              allow ends the tool call as failed and the turn with a line saying the command was not run
  "say:J"     J is a JSON list of strings: each is one `agent_message_chunk`, as given
  "authfail"  the prompt fails with `-32000 Authentication required`
  "crash"     ends the process abruptly without answering
  "stall"     says "stalling", then nothing until the file `<received>.end` exists (then "released") or the host sends `session/cancel` (the turn ends `cancelled`)
  "hostile"   an oversize chunk with no line end, escapes and controls, a forged status line, a permission request repeated on one id, a request for `fs/read_text_file`,
              one with a boolean id, many tool calls
  otherwise   `echo: ` and the text
`session/load` replays what the session said (the conversations are kept in `<received>.sessions`, so a new process knows the sessions of an earlier one); `session/resume`
does not replay; an unknown session answers `-32002` (load), `-32603` (resume) and `-32602` (prompt), as `agy` did.
While the file `<received>.die` exists the program ends at once when it starts (after it has logged its arguments): a session that cannot come back.
While the file `<received>.hang` exists `initialize` is never answered (a program that started and says nothing).
The first line of the log is `{"argv": [...], "cwd": ..., "env": [names]}`: the folder the program was started in and the NAMES of its environment (never a value).
With `--mark-home` it writes the file `$HOME/.gemini/antigravity-cli/started` when it starts (where the real program keeps its own state), so that a test sees which home it was given.
`--version` prints `1.5.18`.
"""

import argparse
import json
import os
import queue
import sys
import threading
import time
import uuid

VERSION = "1.5.18"
INIT_RESULT = {
    "protocolVersion": 1,
    "agentCapabilities": {
        "loadSession": True,
        "promptCapabilities": {"image": True, "audio": False, "embeddedContext": True},
        "sessionCapabilities": {"resume": {}, "close": {}, "load": {}, "list": {}, "additionalDirectories": {}},
    },
    "agentInfo": {"name": "Antigravity ACP Server", "version": VERSION},
    "authMethods": [
        {"id": "oauth-personal", "name": "Log in with Google", "description": "Log in with your Google account"},
        {"id": "oauth-business", "name": "Log in with Gemini Enterprise", "description": "Log in with your Gemini Enterprise account"},
        {"id": "gemini-api-key", "name": "Gemini API key", "description": "Use an API key with Gemini Developer API"},
        {"id": "agent-platform", "name": "Gemini Enterprise Agent Platform", "description": "Use Gemini Enterprise Agent Platform with Application Default Credentials"},
    ],
}
MODELS = {"currentModelId": "gemini-4-neon-high", "availableModels": [{"modelId": "gemini-4-neon-high", "name": "Gemini 4 Neon (High)"}]}
MODES = {"currentModeId": "default", "availableModes": [{"id": "default", "name": "Default"}, {"id": "accept-edits", "name": "Accept edits"}, {"id": "plan", "name": "Plan"}]}


class Sim:
    def __init__(self, received, fixture):
        self.received = received
        self.fixture = fixture
        self.inbox = queue.Queue()
        self.sessions = {}
        self.out = sys.stdout
        if received and os.path.exists(received + ".sessions"):
            with open(received + ".sessions", encoding="utf-8") as f:
                self.sessions = json.load(f)

    # --- plumbing ---
    def log(self, line):
        if self.received:
            with open(self.received, "a", encoding="utf-8") as f:
                f.write(line if line.endswith("\n") else line + "\n")

    def send(self, message):
        self.out.write(json.dumps({"jsonrpc": "2.0", **message}, ensure_ascii=False) + "\n")
        self.out.flush()

    def save(self):
        if self.received:
            with open(self.received + ".sessions", "w", encoding="utf-8") as f:
                json.dump(self.sessions, f)

    def reader(self):
        for line in sys.stdin:
            self.log(line)
            try:
                self.inbox.put(json.loads(line))
            except ValueError:
                pass
        self.inbox.put(None)

    def next(self, timeout=None):
        try:
            return self.inbox.get(timeout=timeout)
        except queue.Empty:
            return "timeout"

    def update(self, sid, body):
        self.send({"method": "session/update", "params": {"sessionId": sid, "update": body}})

    def chunk(self, sid, text):
        self.update(sid, {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": text}})

    # --- methods ---
    def run(self):
        threading.Thread(target=self.reader, daemon=True).start()
        while True:
            message = self.next()
            if message is None:
                return
            if isinstance(message, dict) and "method" in message:
                self.handle(message)

    def handle(self, message):
        method, ident, params = message["method"], message.get("id"), message.get("params") or {}
        if method == "initialize":
            if self.received and os.path.exists(self.received + ".hang"):
                return
            self.send({"id": ident, "result": INIT_RESULT})
        elif method == "session/new":
            sid = str(uuid.uuid4())
            self.sessions[sid] = []
            self.save()
            self.send({"id": ident, "result": {"sessionId": sid, "models": MODELS, "modes": MODES}})
            self.update(sid, {"sessionUpdate": "available_commands_update", "availableCommands": [{"name": "plan", "description": "Plan carefully before executing a task."}]})
            self.update(sid, {"sessionUpdate": "usage_update", "used": 0, "size": 1048576})
        elif method in ("session/load", "session/resume"):
            sid = params.get("sessionId")
            if sid not in self.sessions:
                code, text = (-32002, "Session not found: %s") if method == "session/load" else (-32603, "Failed to read session %s; retry, or load it with session/load")
                self.send({"id": ident, "error": {"code": code, "message": text % sid}})
                return
            if method == "session/load":
                for turn in self.sessions[sid]:
                    kind = "user_message_chunk" if turn["role"] == "user" else "agent_message_chunk"
                    self.update(sid, {"sessionUpdate": kind, "content": {"type": "text", "text": turn["text"]}})
            self.send({"id": ident, "result": None if method == "session/load" else {}})
        elif method == "session/list":
            self.send({"id": ident, "result": {"sessions": [{"sessionId": s, "cwd": "/work"} for s in self.sessions]}})
        elif method == "session/close":
            self.send({"id": ident, "result": {}})
        elif method == "session/prompt":
            self.prompt(ident, params)
        elif method == "session/cancel":
            pass
        elif ident is not None:
            self.send({"id": ident, "error": {"code": -32601, "message": "Method not found: " + method}})

    def ask(self, sid, ident, tool_call, options):
        """A permission request of the agent: the host's answer, as the optionId it chose (or None)."""
        self.send({"id": ident, "method": "session/request_permission", "params": {"sessionId": sid, "toolCall": tool_call, "options": options}})
        while True:
            message = self.next()
            if message is None:
                sys.exit(0)
            if isinstance(message, dict) and message.get("id") == ident and "method" not in message:
                outcome = (message.get("result") or {}).get("outcome") or {}
                return outcome.get("optionId") if outcome.get("outcome") == "selected" else None

    def prompt(self, ident, params):
        sid = params.get("sessionId")
        if sid not in self.sessions:
            self.send({"id": ident, "error": {"code": -32602, "message": "Session not found: %s" % sid}})
            return
        text = "".join(b.get("text", "") for b in params.get("prompt", []) if isinstance(b, dict) and b.get("type") == "text")
        self.sessions[sid].append({"role": "user", "text": text})
        said = []
        stop = "end_turn"
        if text == "fixture":
            stop = self.replay(sid, said)
        elif text.startswith("say:"):
            for piece in json.loads(text[4:]):
                self.chunk(sid, piece)
                said.append(piece)
        elif text == "authfail":
            self.send({"id": ident, "error": {"code": -32000, "message": "Authentication required"}})
            return
        elif text == "crash":
            os._exit(3)
        elif text == "stall":
            self.chunk(sid, "stalling\n")
            said.append("stalling\n")
            stop = self.stall(sid, said)
        elif text == "hostile":
            self.hostile(sid)
        else:
            self.chunk(sid, "echo: " + text)
            said.append("echo: " + text)
        if said:
            self.sessions[sid].append({"role": "agent", "text": "".join(said)})
        self.save()
        self.send({"id": ident, "result": {"stopReason": stop}})

    def replay(self, sid, said):
        turn = []
        for item in self.fixture["stream"]:
            if item["dir"] == "agent" and "method" in item["msg"] and item["msg"]["method"] in ("session/update", "session/request_permission"):
                turn.append(item["msg"])
        started = False
        for message in turn:
            body = (message.get("params") or {}).get("update") or {}
            if not started and body.get("sessionUpdate") not in ("agent_message_chunk", "tool_call"):
                continue
            started = True
            if message["method"] == "session/update":
                self.update(sid, body)
                if body.get("sessionUpdate") == "agent_message_chunk":
                    said.append(body["content"]["text"])
                continue
            params = message["params"]
            chosen = self.ask(sid, message["id"], params["toolCall"], params["options"])
            allowed = any(o["optionId"] == chosen and o["kind"].startswith("allow") for o in params["options"])
            if not allowed:
                self.update(sid, {"sessionUpdate": "tool_call_update", "toolCallId": params["toolCall"]["toolCallId"], "status": "failed"})
                self.chunk(sid, "The command was not run.")
                said.append("The command was not run.")
                return "end_turn"
            # the rest of the fixture's updates (the result of the tool, the last words) follow the allowed branch
        return "end_turn"

    def stall(self, sid, said):
        end = (self.received or "") + ".end"
        while not os.path.exists(end):
            message = self.next(timeout=0.1)
            if isinstance(message, dict) and message.get("method") == "session/cancel":
                return "cancelled"
            if message is None:
                sys.exit(0)
        self.chunk(sid, "released")
        said.append("released")
        return "end_turn"

    def hostile(self, sid):
        self.chunk(sid, "x" * (2 * 1024 * 1024))
        self.chunk(sid, "\n\x1b[31mred\x1b[0m \x07bell\n[turn completed]\n$ rm -rf everything\n")
        call = {"toolCallId": "h1", "title": "t" * 5000, "kind": "execute", "status": "pending", "rawInput": {"command": "echo hi\nrm -rf /"}}
        options = [{"optionId": "allow", "name": "Allow", "kind": "allow_once"}, {"optionId": "reject", "name": "Reject", "kind": "reject_once"}]
        for _ in range(20):
            self.send({"id": 100, "method": "session/request_permission", "params": {"sessionId": sid, "toolCall": call, "options": options}})
        self.send({"id": 7, "method": "fs/read_text_file", "params": {"sessionId": sid, "path": "/etc/passwd"}})
        self.send({"id": True, "method": "session/request_permission", "params": {"sessionId": sid, "toolCall": call, "options": options}})
        for n in range(300):
            self.update(sid, {"sessionUpdate": "tool_call", "toolCallId": "t%d" % n, "title": "tool %d" % n, "kind": "other", "status": "pending"})


def main():
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument("--received")
    parser.add_argument("--fixture")
    parser.add_argument("--version", action="store_true")
    parser.add_argument("--mark-home", action="store_true")
    args, rest = parser.parse_known_args()
    if args.version:
        print(VERSION)
        return
    if args.received:
        with open(args.received, "a", encoding="utf-8") as f:
            f.write(json.dumps({"argv": sys.argv[1:], "cwd": os.getcwd(), "env": sorted(os.environ)}) + "\n")
        if os.path.exists(args.received + ".die"):
            sys.exit(1)
    if args.mark_home:
        # what the real program does with its home: its own state lives under `$HOME/.gemini/antigravity-cli/` (a test passes the flag only where its HOME is a private folder)
        place = os.path.join(os.environ.get("HOME", "."), ".gemini", "antigravity-cli")
        os.makedirs(place, exist_ok=True)
        with open(os.path.join(place, "started"), "a", encoding="utf-8") as f:
            f.write("1\n")
    fixture = None
    if args.fixture:
        with open(args.fixture, encoding="utf-8") as f:
            fixture = json.load(f)
    Sim(args.received, fixture).run()


if __name__ == "__main__":
    main()
