"""Reference mapping from Codex app-server messages to host records and approvals.

This is the specification the owned host's adapter is tested against: given the same
message sequence it must produce the same records, approvals and refusals, in order.
A host record is one UTF-8 line. Provider output is untrusted display data, so control
characters and terminal escape sequences are removed before a record is formed, and a line of agent text that begins like a record of the host
(`>> `, `$ `, `> `, a bracketed note) gets `[agent] ` in front, once. The rules are those of hostd/src/codex.rs and leads.rs; the lines and records of
tests/fixtures/provider_text.json are what both are tested against.
"""

from __future__ import annotations

import re
from collections import Counter
from dataclasses import dataclass, field

MAX_PROMPT = 200
MAX_RECORD = 4096
_CSI = re.compile(r"\x1b\[[0-9;?]*[ -/]*[@-~]")
_OSC = re.compile(r"\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)")
_ESC = re.compile(r"\x1b[@-_]")
# controls, and the characters that draw nothing or reorder what is drawn (a lead behind a zero-width space, a tag or a bidi override); U+200C and U+200D stay
_CONTROL = re.compile("[\x00-\x08\x0b\x0c\x0e-\x1f\x7f-\x9f\u061c\u200b\u200e\u200f\u2028\u2029\u202a-\u202e\u2060-\u2064\u2066-\u2069\ufeff\U000e0000-\U000e007f]")
TAG = "[agent] "
LEADS = (">>", "$", ">")                                     # a lead counts when a space or the end of the record follows it
NOTES = ("session started", "session closed", "provider reconnected", "provider disconnected", "tool", "tool error", "error", "patch", "input refused",
         "account changed", "account issue")
NOTE_STEMS = ("turn ", "exit ", "log recovered:")
_IGNORABLE = re.compile("[\u00ad\u034f\u061c\u115f\u1160\u17b4\u17b5\u180b-\u180f\u200b-\u200f\u2028-\u202e\u2060-\u206f\u3164\ufe00-\ufe0f\ufeff\uffa0"
                        "\ufff0-\ufff8\U0001bca0-\U0001bca3\U0001d173-\U0001d17a\U000e0000-\U000e0fff]")   # the default-ignorable characters
_WHITE = "\t\n\x0b\x0c\r \x85\xa0\u1680\u2000\u2001\u2002\u2003\u2004\u2005\u2006\u2007\u2008\u2009\u200a\u2028\u2029\u202f\u205f\u3000"
_FOLDED = re.compile(r"[0-9]+ more line|a long line is cut")   # the echo's markers

APPROVAL_METHODS = ("item/commandExecution/requestApproval", "item/fileChange/requestApproval")
REFUSED_METHODS = ("item/permissions/requestApproval", "item/tool/call", "item/tool/requestUserInput", "account/chatgptAuthTokens/refresh",
                   "attestation/generate", "mcpServer/elicitation/request", "execCommandApproval", "applyPatchApproval")


def sanitize(text: str) -> str:
    """Remove terminal escape sequences and control characters (a tab becomes a space)."""
    text = _OSC.sub("", text)
    text = _CSI.sub("", text)
    text = _ESC.sub("", text)
    return _CONTROL.sub("", text.replace("\t", " ").replace("\n", " ").replace("\r", " "))


def reserved(record: str) -> bool:
    """Whether a record begins like something the host writes, judged on a skeleton: nothing that draws nothing, no white space in front."""
    skeleton = _IGNORABLE.sub("", record).lstrip(_WHITE)
    closed = lambda rest: rest == "" or rest.startswith(" ")
    if any(skeleton.startswith(lead) and closed(skeleton[len(lead):]) for lead in LEADS):
        return True
    if not skeleton.startswith("[") or "]" not in skeleton:
        return False
    note, after = skeleton[1:].split("]", 1)
    return closed(after) and (note in NOTES or note.startswith(NOTE_STEMS) or _FOLDED.match(note) is not None)


def agent_record(line: str) -> str:
    """One line of agent text as a record: cleaned, tagged when it begins like a record of the host, then bounded (the tag counts)."""
    clean = sanitize(line)
    return (TAG + clean if reserved(clean) else clean).encode()[:MAX_RECORD].decode(errors="ignore")


@dataclass
class Mapped:
    records: list[str] = field(default_factory=list)
    approvals: list[dict] = field(default_factory=list)          # pending when the sequence ends
    responses: list[dict] = field(default_factory=list)          # what the adapter sends back, in order
    ignored: Counter = field(default_factory=Counter)


def _emit(out: Mapped, line: str) -> None:
    line = sanitize(line)
    out.records.append(line.encode()[:MAX_RECORD].decode(errors="ignore"))


def _emit_agent(out: Mapped, line: str) -> None:
    out.records.append(agent_record(line))


class Mapper:
    def __init__(self) -> None:
        self.out = Mapped()
        self.buffers: dict[str, str] = {}

    def _delta(self, item: str, delta: str) -> None:
        text = self.buffers.get(item, "") + delta
        *lines, rest = text.split("\n")
        for line in lines:
            _emit_agent(self.out, line.rstrip("\r"))
        self.buffers[item] = rest

    def _flush(self, item: str) -> None:
        rest = self.buffers.pop(item, "")
        if rest:
            _emit_agent(self.out, rest.rstrip("\r"))

    def notification(self, method: str, params: dict) -> None:
        if method in ("item/agentMessage/delta", "item/commandExecution/outputDelta"):
            self._delta(params["itemId"], params["delta"])
        elif method == "item/started":
            item = params["item"]
            if item["type"] == "commandExecution":
                _emit(self.out, "$ " + item["command"])
            elif item["type"] == "fileChange":
                _emit(self.out, f"[patch] {len(item.get('changes', []))} change(s)")
        elif method == "item/completed":
            item = params["item"]
            self._flush(item["id"])
            if item["type"] == "commandExecution":
                code = item.get("exitCode")
                _emit(self.out, f"[exit {code}]" if code is not None else "[exit unknown]")
        elif method == "turn/started":
            _emit(self.out, "[turn started]")
        elif method == "turn/completed":
            _emit(self.out, f"[turn {params['turn']['status']}]")
        elif method == "error":
            _emit(self.out, "[error] " + str(params["error"].get("message", "")))
        elif method == "serverRequest/resolved":
            self.out.approvals = [a for a in self.out.approvals if a["request_id"] != params["requestId"]]
        elif method in ("thread/started", "thread/status/changed"):
            self.out.ignored[method] += 1  # state only: counted, no record
        else:
            self.out.ignored[method] += 1

    def server_request(self, request_id, method: str, params: dict) -> None:
        if method in APPROVAL_METHODS:
            if method.endswith("commandExecution/requestApproval"):
                prompt = "run: " + str(params.get("command", "a command"))
            else:
                prompt = "change files" + (": " + params["reason"] if params.get("reason") else "")
            self.out.approvals.append({"request_id": request_id, "method": method, "prompt": sanitize(prompt)[:MAX_PROMPT]})
        else:
            # tokens, tools, attestations and elicitations are not supplied by this client
            self.out.responses.append({"id": request_id, "error": {"code": -32601, "message": "unsupported by this client"}})
            self.out.ignored["refused:" + method] += 1

    def answer(self, request_id, decision: str) -> None:
        """A host approval answer becomes the provider's decision for exactly that request."""
        pending = [a for a in self.out.approvals if a["request_id"] == request_id]
        if not pending:
            raise KeyError("no such pending approval")  # stale or forged: nothing is sent
        self.out.responses.append({"id": request_id, "result": {"decision": "accept" if decision == "allow" else "decline"}})
        self.out.approvals = [a for a in self.out.approvals if a["request_id"] != request_id]

    def feed(self, steps: list[dict]) -> Mapped:
        """Run a recorded sequence: provider messages and the user's approval answers, in order."""
        for step in steps:
            if step["dir"] == "server" and "method" in step["msg"] and "id" in step["msg"]:
                self.server_request(step["msg"]["id"], step["msg"]["method"], step["msg"].get("params", {}))
            elif step["dir"] == "server" and "method" in step["msg"]:
                self.notification(step["msg"]["method"], step["msg"].get("params", {}))
            elif step["dir"] == "user":
                self.answer(step["request_id"], step["answer"])
        for item in list(self.buffers):
            self._flush(item)
        return self.out
