"""Reference mapping from Claude Code stream-json messages to host records and approvals.

The specification the owned host's Claude adapter is tested against. The message shapes come from
the Claude Code documentation, not from a recording (see the capability record's unverified list).
A host record is one UTF-8 line; provider output is untrusted display data.
"""

from __future__ import annotations

import json
from collections import Counter
from dataclasses import dataclass, field

from .codex_mapping import MAX_PROMPT, MAX_RECORD, sanitize

MAX_RESULT_LINES = 20
PERMISSION_SUBTYPE = "can_use_tool"          # assumed, not documented: listed as unverified


@dataclass
class Mapped:
    records: list[str] = field(default_factory=list)
    approvals: list[dict] = field(default_factory=list)          # request_id and prompt, in order
    ignored: Counter = field(default_factory=Counter)


def _emit(out: Mapped, line: str) -> None:
    out.records.append(sanitize(line).encode()[:MAX_RECORD].decode(errors="ignore"))


def approval_prompt(tool: str, tool_input: dict) -> str:
    if tool == "Bash" and isinstance(tool_input.get("command"), str):
        text = "run: " + tool_input["command"]
    else:
        text = f"{tool}: " + json.dumps(tool_input, ensure_ascii=False, separators=(",", ":"), sort_keys=True)
    text = sanitize(text)
    return text if len(text) <= MAX_PROMPT else text[: MAX_PROMPT - 1] + "…"


def _text_of(content) -> str:
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        return "\n".join(b.get("text", "") for b in content if isinstance(b, dict) and b.get("type") == "text")
    return ""


def map_stream(messages: list[dict]) -> Mapped:
    out = Mapped()
    for message in messages:
        kind = message.get("type")
        if kind == "system" and message.get("subtype") == "init":
            _emit(out, "[session started] model=" + str(message.get("model", ""))[:60])
        elif kind == "assistant":
            for block in message.get("message", {}).get("content", []):
                if block.get("type") == "text":
                    for line in block.get("text", "").split("\n"):
                        _emit(out, line.rstrip("\r"))
                elif block.get("type") == "tool_use":
                    tool_input = block.get("input", {})
                    if block.get("name") == "Bash" and isinstance(tool_input.get("command"), str):
                        _emit(out, "$ " + tool_input["command"])
                    else:
                        _emit(out, "[tool] " + str(block.get("name", ""))[:60])
        elif kind == "user":
            for block in message.get("message", {}).get("content", []) if isinstance(message.get("message", {}).get("content"), list) else []:
                if block.get("type") == "tool_result":
                    if block.get("is_error"):
                        _emit(out, "[tool error]")
                    for line in _text_of(block.get("content")).split("\n")[:MAX_RESULT_LINES]:
                        _emit(out, "> " + line.rstrip("\r"))
        elif kind == "result":
            if message.get("is_error") or message.get("subtype") != "success":
                _emit(out, "[turn failed] " + str(message.get("subtype", "")))
                for error in message.get("errors", []) or []:
                    _emit(out, "[error] " + str(error))
            else:
                _emit(out, "[turn completed]")
        elif kind == "control_request":
            request = message.get("request", {})
            if request.get("subtype") == PERMISSION_SUBTYPE:
                out.approvals.append({"request_id": message["request_id"], "prompt": approval_prompt(request.get("tool_name", ""), request.get("input", {}))})
            else:
                out.ignored["control_request:" + str(request.get("subtype"))] += 1
        else:
            out.ignored[str(kind)] += 1
    return out
