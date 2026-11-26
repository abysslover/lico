"""Reference mapping from Agent Client Protocol messages (`agy acp`) to host records and approvals.

The specification the owned host's `agy` adapter is tested against. What was observed with the installed program (a private empty home, no sign-in, no turn) is the
`initialize` answer, the answer of `session/new` and the error codes in `src/host/adapters/agy/capabilities.json`; every shape after that (the updates of a turn, a
permission request, a stop reason) is from the ACP documentation and is on that record's unverified list. A host record is one UTF-8 line; provider output is
untrusted display data.

A sequence is a list of `{"dir": "host" | "agent", "msg": {...}}`: `host` is what the host sent (the requests it makes), `agent` is what `agy acp` sent.
"""

from __future__ import annotations

import re
from collections import Counter
from dataclasses import dataclass, field

from .codex_mapping import MAX_PROMPT, MAX_RECORD, agent_record, sanitize

MAX_RESULT_LINES = 20
MAX_PATCHES = 8
MAX_NAME = 60
MAX_ID = 128
MAX_OPTIONS = 8
MAX_PENDING = 8                    # approvals waiting at once; a request over this is cancelled at once
MAX_TOOLS = 256                    # tool calls followed at once; one more is not followed
MAX_PENDING_LINE = 4 * MAX_RECORD  # characters of a line that has not ended; the line is cut here
OPTION_KINDS = ("allow_once", "allow_always", "reject_once", "reject_always")
AUTH_REQUIRED = -32000             # the code the ACP documentation gives for "authenticate first"
SIGN_IN = re.compile(r"auth|sign[ -]?in|log[ -]?in|credential|api[ -]?key", re.I)
SIGN_IN_WORDS = "[error] agy is not signed in: open a Terminal tab, run agy, sign in, then send the message again"
STOPS = {"end_turn": "[turn completed]", "cancelled": "[turn stopped]"}
SILENT_UPDATES = ("agent_thought_chunk", "user_message_chunk", "plan", "available_commands_update", "usage_update", "current_mode_update", "config_option_update")


@dataclass
class Mapped:
    records: list[str] = field(default_factory=list)
    approvals: list[dict] = field(default_factory=list)       # pending when the sequence ends: request_id, prompt, options
    responses: list[dict] = field(default_factory=list)       # what the adapter sends back on its own, in order (a refusal, a cancelled outcome)
    ignored: Counter = field(default_factory=Counter)


def _emit(out: Mapped, line: str) -> None:
    out.records.append(sanitize(line).encode()[:MAX_RECORD].decode(errors="ignore"))


def _emit_agent(out: Mapped, line: str) -> None:
    out.records.append(agent_record(line))


def _clip(text, limit: int) -> str:
    return sanitize(text if isinstance(text, str) else "")[:limit]


def request_id_ok(value) -> bool:
    """A JSON-RPC id the host can echo back exactly: an integer (not a boolean) or a short string."""
    return (isinstance(value, int) and not isinstance(value, bool)) or (isinstance(value, str) and 0 < len(value) <= MAX_ID)


def valid_options(options) -> list[dict]:
    """The options of a permission request that the host understands: an id and one of the four kinds, at most eight, in the order given."""
    kept = []
    for option in options if isinstance(options, list) else []:
        if isinstance(option, dict) and isinstance(option.get("optionId"), str) and 0 < len(option["optionId"]) <= MAX_ID and option.get("kind") in OPTION_KINDS:
            kept.append({"optionId": option["optionId"], "kind": option["kind"], "name": _clip(option.get("name"), MAX_NAME)})
        if len(kept) >= MAX_OPTIONS:
            break
    return kept


def approval_prompt(tool_call) -> str:
    """What the person is asked: the command for a command, the title otherwise."""
    call = tool_call if isinstance(tool_call, dict) else {}
    raw = call.get("rawInput")
    if call.get("kind") == "execute" and isinstance(raw, dict) and isinstance(raw.get("command"), str):
        text = "run: " + sanitize(raw["command"])
    else:
        text = sanitize(call["title"]) if isinstance(call.get("title"), str) and call["title"].strip() else sanitize(call["kind"]) if isinstance(call.get("kind"), str) else "tool"
    return text if len(text) <= MAX_PROMPT else text[: MAX_PROMPT - 1] + "…"


def answer(approval: dict, allow: bool) -> dict:
    """The response to a permission request: exactly the narrowest option the person's answer means (`allow_once` for allow, `reject_once` for deny), and `cancelled` when the
    agent did not offer it: an answer is never widened to `allow_always` or `reject_always`."""
    want = "allow_once" if allow else "reject_once"
    chosen = next((o["optionId"] for o in approval["options"] if o["kind"] == want), None)
    outcome = {"outcome": "selected", "optionId": chosen} if chosen is not None else {"outcome": "cancelled"}
    return {"jsonrpc": "2.0", "id": approval["request_id"], "result": {"outcome": outcome}}


def _error_records(error) -> list[str]:
    message = _clip(error.get("message") if isinstance(error, dict) else None, 200)
    lines = ["[turn failed]", "[error] " + (message or "no message")]
    code = error.get("code") if isinstance(error, dict) else None
    if code == AUTH_REQUIRED or SIGN_IN.search(message):
        lines.append(SIGN_IN_WORDS)
    return lines


class _Mapper:
    def __init__(self) -> None:
        self.out = Mapped()
        self.buffer = ""
        self.methods: dict = {}                # host request id -> method, until the answer comes
        self.loading: set = set()              # host requests of session/load that have not been answered: what they replay is not copied
        self.tools: dict[str, bool] = {}       # tool call id -> its result has been recorded
        self.pending: set = set()              # agent request ids of permission requests waiting for the person

    def flush(self) -> None:
        if self.buffer:
            _emit_agent(self.out, self.buffer)
            self.buffer = ""

    def text(self, piece: str) -> None:
        self.buffer += piece
        while "\n" in self.buffer:
            line, self.buffer = self.buffer.split("\n", 1)
            _emit_agent(self.out, line.rstrip("\r"))
        if len(self.buffer) > MAX_PENDING_LINE:
            self.flush()

    def update(self, update) -> None:
        kind = update.get("sessionUpdate") if isinstance(update, dict) else None
        if self.loading:
            self.out.ignored["replayed"] += 1
        elif kind == "agent_message_chunk":
            content = update.get("content")
            if isinstance(content, dict) and content.get("type") == "text" and isinstance(content.get("text"), str):
                self.text(content["text"])
            else:
                self.out.ignored["agent_message_chunk:not_text"] += 1
        elif kind == "tool_call":
            self.tool_call(update)
        elif kind == "tool_call_update":
            self.tool_update(update)
        elif kind in SILENT_UPDATES:
            self.out.ignored[kind] += 1
        else:
            self.out.ignored["update:" + _clip(kind, MAX_NAME)] += 1

    def tool_call(self, update: dict) -> None:
        call_id = update.get("toolCallId")
        if not isinstance(call_id, str) or not 0 < len(call_id) <= MAX_ID:
            self.out.ignored["tool_call:bad_id"] += 1
            return
        if call_id in self.tools:
            return self.tool_update(update)
        if len(self.tools) >= MAX_TOOLS:
            self.out.ignored["tool_call:too_many"] += 1
            return
        self.flush()
        self.tools[call_id] = False
        raw = update.get("rawInput")
        if update.get("kind") == "execute" and isinstance(raw, dict) and isinstance(raw.get("command"), str):
            _emit(self.out, "$ " + raw["command"])
        else:
            name = update.get("title") if isinstance(update.get("title"), str) and update["title"].strip() else update.get("kind")
            _emit(self.out, "[tool] " + _clip(name, MAX_NAME))
        self.tool_update(update)

    def tool_update(self, update: dict) -> None:
        call_id = update.get("toolCallId")
        status = update.get("status")
        if not isinstance(call_id, str) or call_id not in self.tools or self.tools[call_id] or status not in ("completed", "failed"):
            return
        self.flush()
        self.tools[call_id] = True
        if status == "failed":
            _emit(self.out, "[tool error]")
        patches = 0
        shown = 0
        for block in update.get("content") if isinstance(update.get("content"), list) else []:
            if not isinstance(block, dict):
                continue
            inner = block.get("content")
            if block.get("type") == "content" and isinstance(inner, dict) and inner.get("type") == "text" and isinstance(inner.get("text"), str):
                for line in inner["text"].split("\n"):
                    if shown >= MAX_RESULT_LINES:
                        break
                    _emit(self.out, "> " + line.rstrip("\r"))
                    shown += 1
            elif block.get("type") == "diff" and patches < MAX_PATCHES:
                _emit(self.out, "[patch] " + _clip(block.get("path"), 120))
                patches += 1

    def permission(self, request_id, params) -> None:
        call = params.get("toolCall") if isinstance(params, dict) else None
        options = valid_options(params.get("options") if isinstance(params, dict) else None)
        if not request_id_ok(request_id) or request_id in self.pending:
            self.out.ignored["permission:bad_or_repeated_id"] += 1       # a repeated id is one approval, never two
            return
        self.flush()
        approval = {"request_id": request_id, "prompt": approval_prompt(call), "options": options}
        if len(self.pending) >= MAX_PENDING or not any(o["kind"] == "allow_once" for o in options):
            self.out.ignored["permission:refused"] += 1
            _emit(self.out, "[tool error]")
            self.out.responses.append(answer(approval, False))             # cancelled: nothing the person could allow once
            return
        self.pending.add(request_id)
        self.out.approvals.append(approval)

    def agent_message(self, message: dict) -> None:
        method, ident = message.get("method"), message.get("id")
        if isinstance(method, str):
            if method == "session/update":
                params = message.get("params")
                self.update(params.get("update") if isinstance(params, dict) else None)
            elif method == "session/request_permission" and ident is not None:
                self.permission(ident, message.get("params"))
            elif ident is not None and request_id_ok(ident):
                self.out.ignored["refused:" + _clip(method, MAX_NAME)] += 1   # fs/*, terminal/* and the rest: the host offered none of them
                self.out.responses.append({"jsonrpc": "2.0", "id": ident, "error": {"code": -32601, "message": "Method not found"}})
            else:
                self.out.ignored["notification:" + _clip(method, MAX_NAME)] += 1
            return
        if ident is None or "result" not in message and "error" not in message:
            self.out.ignored["unknown"] += 1
            return
        self.response(ident, message)

    def response(self, ident, message: dict) -> None:
        method = self.methods.pop(ident, None) if request_id_ok(ident) else None
        result, error = message.get("result"), message.get("error")
        self.loading.discard(ident)
        if method is None:
            self.out.ignored["response:unknown_request"] += 1
        elif method == "session/prompt":
            self.flush()
            if isinstance(error, dict):
                for line in _error_records(error):
                    _emit(self.out, line)
            else:
                reason = result.get("stopReason") if isinstance(result, dict) else None
                _emit(self.out, STOPS.get(reason) or "[turn failed] " + (_clip(reason, MAX_NAME) or "no stop reason"))
        elif isinstance(error, dict):
            self.flush()
            _emit(self.out, "[error] " + (_clip(error.get("message"), 200) or "no message"))
            if error.get("code") == AUTH_REQUIRED or SIGN_IN.search(_clip(error.get("message"), 200)):
                _emit(self.out, SIGN_IN_WORDS)
        elif method == "session/new" and isinstance(result, dict) and isinstance(result.get("sessionId"), str):
            models = result.get("models")
            model = _clip(models.get("currentModelId"), MAX_NAME) if isinstance(models, dict) else ""
            _emit(self.out, "[session started] model=" + model if model else "[session started]")
        elif method in ("session/load", "session/resume"):
            _emit(self.out, "[provider reconnected]")
        elif method == "initialize":
            pass                                                           # the adapter reads the capabilities itself; nothing is shown
        else:
            self.out.ignored["response:" + method] += 1

    def host_message(self, message: dict) -> None:
        method, ident = message.get("method"), message.get("id")
        if isinstance(method, str) and ident is not None and request_id_ok(ident):
            self.methods[ident] = method
            if method == "session/load":
                self.loading.add(ident)
        elif "result" in message and ident in self.pending:
            self.pending.discard(ident)                                    # the host answered a permission request

    def run(self, stream: list[dict]) -> Mapped:
        for item in stream:
            (self.host_message if item.get("dir") == "host" else self.agent_message)(item["msg"])
        self.flush()
        return self.out


def map_stream(stream: list[dict]) -> Mapped:
    return _Mapper().run(stream)
