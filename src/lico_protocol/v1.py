"""Protocol v1 schemas, a small validator and the operation inventory.

Transport: one JSON object per line over an owner-only local socket. Every message
carries ``v`` (1). A request carries ``op`` and may carry ``id`` (a client string
of at most 64 characters that the reply echoes). A reply carries ``ok`` (1 or 0).
Events carry ``ev`` and no ``ok``. Unknown fields are rejected in requests and are
a conformance failure in replies. ``ok`` stays an integer so the earlier
prototype clients keep working.
"""

from __future__ import annotations

PROTOCOL = 1
MAX_REQUEST = 4096
ERRORS = ("bad_request", "unknown_op", "unknown_session", "stale_generation", "version_unsupported", "too_large",
          "session_exited", "session_disconnected", "unknown_request", "unknown_approval", "unknown_view", "busy",
          "unknown_account")

# Every operation of the protocol and where it is implemented. "oracle" operations
# exist in the reference prototype and in every conformance candidate; "host"
# operations are implemented by the owned host only; the others arrive in later
# steps of the phase.
INVENTORY = {
    "hello": {"state": "oracle", "mutates": False},
    "status": {"state": "oracle", "mutates": False},
    "snapshot": {"state": "oracle", "mutates": False},
    "subscribe": {"state": "oracle", "mutates": True},
    "unsubscribe": {"state": "oracle", "mutates": True},
    "session.create": {"state": "host", "mutates": True},
    "session.list": {"state": "host", "mutates": False},
    "session.resume": {"state": "host", "mutates": True},
    "session.purge": {"state": "host", "mutates": True},
    "input": {"state": "host", "mutates": True},
    "input.status": {"state": "host", "mutates": False},
    "approval.answer": {"state": "host", "mutates": True},
    "view.attach": {"state": "host", "mutates": True},
    "view.detach": {"state": "host", "mutates": True},
    "resources": {"state": "host", "mutates": False},
    "account.create": {"state": "host", "mutates": True},
    "account.list": {"state": "host", "mutates": False},
    "account.default": {"state": "host", "mutates": True},
    "session.rebind": {"state": "host", "mutates": True},
}
ORACLE_OPS = tuple(op for op, meta in INVENTORY.items() if meta["state"] == "oracle")

INT, STR, BOOL = "integer", "string", "boolean"


def _obj(required: dict, optional: dict | None = None) -> dict:
    return {"type": "object", "required": required, "optional": optional or {}}


COMMON_REPLY_OPTIONAL = {"id": {"type": STR, "max_length": 64}}
_V = {"type": INT, "enum": [PROTOCOL]}
_OK = {"type": INT, "enum": [1]}

SNAPSHOT_BODY = {
    "type": "object",
    "required": {"session": {"type": STR}, "generation": {"type": INT, "minimum": 1}, "seq": {"type": INT, "minimum": -1},
                 "lines": {"type": "array", "items": {"type": "array", "length": 2}}},
    "optional": {"text": {"type": BOOL}},
}

REPLIES = {
    "hello": _obj({"v": _V, "ok": _OK, "protocol": {"type": INT, "enum": [PROTOCOL]}, "host": {"type": STR},
                   "max_request": {"type": INT}, "retain": {"type": INT, "minimum": 1},
                   "ops": {"type": "array", "items": {"type": STR}}}, COMMON_REPLY_OPTIONAL),
    "status": _obj({"v": _V, "ok": _OK, "pid": {"type": INT}, "sessions": {"type": "array", "items": {
        "type": "object",
        "required": {"session": {"type": STR}, "generation": {"type": INT, "minimum": 1}, "seq": {"type": INT, "minimum": -1},
                     "producing": {"type": BOOL}, "subscribers": {"type": INT, "minimum": 0},
                     "bytes_total": {"type": INT, "minimum": 0}},
        "optional": {"oldest": {"type": INT, "minimum": 0}, "state": {"type": STR, "enum": ["running", "exited", "disconnected"]},
                     "account": {"type": STR}, "credential_generation": {"type": INT, "minimum": 1},
                     "approvals": {"type": "array", "items": {"type": "object", "required": {
                         "approval_id": {"type": INT, "minimum": 1}, "prompt": {"type": STR, "max_length": 200}},
                         "optional": {}}}}}}},
                  COMMON_REPLY_OPTIONAL),
    "session.create": _obj({"v": _V, "ok": _OK, "session": {"type": STR}, "generation": {"type": INT, "minimum": 1}},
                           COMMON_REPLY_OPTIONAL),
    "input": _obj({"v": _V, "ok": _OK, "session": {"type": STR}, "request_id": {"type": STR},
                   "state": {"type": STR, "enum": ["accepted", "submitted", "observed", "failed"]}, "duplicate": {"type": BOOL}},
                  COMMON_REPLY_OPTIONAL),
    "input.status": _obj({"v": _V, "ok": _OK, "session": {"type": STR}, "request_id": {"type": STR},
                          "state": {"type": STR, "enum": ["accepted", "submitted", "observed", "failed"]}}, COMMON_REPLY_OPTIONAL),
    "approval.answer": _obj({"v": _V, "ok": _OK, "session": {"type": STR}, "approval_id": {"type": INT, "minimum": 1},
                             "state": {"type": STR, "enum": ["answered"]}}, COMMON_REPLY_OPTIONAL),
    "view.attach": _obj({"v": _V, "ok": _OK, "view": {"type": STR}, "session": {"type": STR},
                         "generation": {"type": INT, "minimum": 1}, "gap": {"type": BOOL}, "from": {"type": INT, "minimum": 0},
                         "snapshot": SNAPSHOT_BODY}, {**COMMON_REPLY_OPTIONAL, "missed": {"type": [INT, "null"]}}),
    "view.detach": _obj({"v": _V, "ok": _OK, "view": {"type": STR}, "session": {"type": STR},
                         "seq": {"type": INT, "minimum": -1}}, COMMON_REPLY_OPTIONAL),
    "resources": _obj({"v": _V, "ok": _OK, "pid": {"type": INT}, "uptime_s": {"type": INT, "minimum": 0},
                       "rss_kb": {"type": INT, "minimum": 0}, "pss_kb": {"type": INT, "minimum": 0},
                       "threads": {"type": INT, "minimum": 1}, "sessions": {"type": INT, "minimum": 0},
                       "running_sessions": {"type": INT, "minimum": 0}, "subscribers": {"type": INT, "minimum": 0},
                       "views": {"type": INT, "minimum": 0}, "log_bytes": {"type": INT, "minimum": 0},
                       "agents": {"type": "array", "items": {"type": "object", "required": {
                           "session": {"type": STR}, "pid": {"type": INT}, "rss_kb": {"type": INT, "minimum": 0}},
                           "optional": {}}}}, COMMON_REPLY_OPTIONAL),
    "session.list": _obj({"v": _V, "ok": _OK, "sessions": {"type": "array", "items": {
        "type": "object",
        "required": {"session": {"type": STR}, "generation": {"type": INT, "minimum": 1},
                     "state": {"type": STR, "enum": ["running", "exited", "disconnected"]}, "seq": {"type": INT, "minimum": -1}},
        "optional": {}}}}, COMMON_REPLY_OPTIONAL),
    "session.resume": _obj({"v": _V, "ok": _OK, "session": {"type": STR}, "generation": {"type": INT, "minimum": 1}},
                           COMMON_REPLY_OPTIONAL),
    "session.purge": _obj({"v": _V, "ok": _OK, "session": {"type": STR}}, COMMON_REPLY_OPTIONAL),
    "session.rebind": _obj({"v": _V, "ok": _OK, "session": {"type": STR}, "generation": {"type": INT, "minimum": 1}},
                           COMMON_REPLY_OPTIONAL),
    "account.create": _obj({"v": _V, "ok": _OK, "account": {"type": STR}, "provider": {"type": STR, "enum": ["codex", "claude"]},
                            "label": {"type": STR, "max_length": 40}}, COMMON_REPLY_OPTIONAL),
    "account.list": _obj({"v": _V, "ok": _OK, "accounts": {"type": "array", "items": {
        "type": "object",
        "required": {"account": {"type": STR}, "provider": {"type": STR, "enum": ["codex", "claude"]},
                     "label": {"type": STR, "max_length": 40}, "default": {"type": BOOL},
                     "credential_generation": {"type": INT, "minimum": 1}},
        "optional": {}}}}, COMMON_REPLY_OPTIONAL),
    "account.default": _obj({"v": _V, "ok": _OK, "account": {"type": STR}}, COMMON_REPLY_OPTIONAL),
    "snapshot": _obj({"v": _V, "ok": _OK, **SNAPSHOT_BODY["required"]}, COMMON_REPLY_OPTIONAL),
    "subscribe": _obj({"v": _V, "ok": _OK, "session": {"type": STR}, "generation": {"type": INT, "minimum": 1},
                       "gap": {"type": BOOL}, "from": {"type": INT, "minimum": 0}},
                      {**COMMON_REPLY_OPTIONAL, "snapshot": SNAPSHOT_BODY, "missed": {"type": [INT, "null"]}}),
    "unsubscribe": _obj({"v": _V, "ok": _OK, "session": {"type": STR}, "seq": {"type": INT, "minimum": -1}},
                        COMMON_REPLY_OPTIONAL),
}
ERROR_REPLY = _obj({"v": _V, "ok": {"type": INT, "enum": [0]}, "error": {"type": STR, "enum": list(ERRORS)}},
                   COMMON_REPLY_OPTIONAL)
EVENTS = {
    "rec": _obj({"v": _V, "ev": {"type": STR, "enum": ["rec"]}, "session": {"type": STR},
                 "generation": {"type": INT, "minimum": 1}, "seq": {"type": INT, "minimum": 0}, "data": {"type": STR}},
                {"text": {"type": STR}}),
    "overflow": _obj({"v": _V, "ev": {"type": STR, "enum": ["overflow"]}, "session": {"type": STR}}),
    "input": _obj({"v": _V, "ev": {"type": STR, "enum": ["input"]}, "session": {"type": STR},
                   "request_id": {"type": STR}, "state": {"type": STR, "enum": ["submitted", "observed", "failed"]}}),
    "approval": _obj({"v": _V, "ev": {"type": STR, "enum": ["approval"]}, "session": {"type": STR},
                      "approval_id": {"type": INT, "minimum": 1}, "prompt": {"type": STR, "max_length": 200}}),
}

_COMMON_REQ = {"v": _V, "id": {"type": STR, "max_length": 64}}
REQUESTS = {
    "hello": _obj({}, _COMMON_REQ),
    "status": _obj({}, _COMMON_REQ),
    "snapshot": _obj({"session": {"type": STR, "max_length": 64}},
                     {**_COMMON_REQ, "generation": {"type": INT, "minimum": 1}}),
    "subscribe": _obj({"session": {"type": STR, "max_length": 64}, "after": {"type": INT, "minimum": -1}},
                      {**_COMMON_REQ, "generation": {"type": INT, "minimum": 1}, "snapshot": {"type": BOOL}}),
    "unsubscribe": _obj({"session": {"type": STR, "max_length": 64}}, _COMMON_REQ),
    "session.resume": _obj({"session": {"type": STR, "max_length": 64}, "generation": {"type": INT, "minimum": 1}}, _COMMON_REQ),
    "session.purge": _obj({"session": {"type": STR, "max_length": 64}, "generation": {"type": INT, "minimum": 1}}, _COMMON_REQ),
    "session.create": _obj({"kind": {"type": STR, "enum": ["synthetic", "codex"]}},
                           {**_COMMON_REQ, "rate": {"type": INT, "minimum": 0, "maximum": 10000},
                            "history": {"type": INT, "minimum": 0, "maximum": 100000},
                            "payload": {"type": INT, "minimum": 1, "maximum": 4096},
                            "account": {"type": STR, "max_length": 64}}),
    "session.rebind": _obj({"session": {"type": STR, "max_length": 64}, "generation": {"type": INT, "minimum": 1},
                            "account": {"type": STR, "max_length": 64}}, _COMMON_REQ),
    "account.create": _obj({"provider": {"type": STR, "enum": ["codex", "claude"]},
                            "label": {"type": STR, "min_length": 1, "max_length": 40, "no_control": True}}, _COMMON_REQ),
    "account.list": _obj({}, _COMMON_REQ),
    "account.default": _obj({"account": {"type": STR, "max_length": 64}}, _COMMON_REQ),
    "session.list": _obj({}, _COMMON_REQ),
    "input": _obj({"session": {"type": STR, "max_length": 64}, "generation": {"type": INT, "minimum": 1},
                   "request_id": {"type": STR, "min_length": 1, "max_length": 64, "charset": "id"},
                   "text": {"type": STR, "min_length": 1, "max_bytes": 512, "no_control": True}}, _COMMON_REQ),
    "input.status": _obj({"session": {"type": STR, "max_length": 64},
                          "request_id": {"type": STR, "min_length": 1, "max_length": 64, "charset": "id"}},
                         _COMMON_REQ),
    "approval.answer": _obj({"session": {"type": STR, "max_length": 64}, "generation": {"type": INT, "minimum": 1},
                             "approval_id": {"type": INT, "minimum": 1}, "answer": {"type": STR, "enum": ["allow", "deny"]}},
                            _COMMON_REQ),
    "view.attach": _obj({"session": {"type": STR, "max_length": 64}, "kind": {"type": STR, "enum": ["gui", "cli"]}},
                        {**_COMMON_REQ, "generation": {"type": INT, "minimum": 1}, "after": {"type": INT, "minimum": -1}}),
    "view.detach": _obj({"view": {"type": STR, "max_length": 64}}, _COMMON_REQ),
    "resources": _obj({}, _COMMON_REQ),
}


def _is(value, kind) -> bool:
    if kind == "null":
        return value is None
    if kind == INT:
        return isinstance(value, int) and not isinstance(value, bool)
    if kind == STR:
        return isinstance(value, str)
    if kind == BOOL:
        return isinstance(value, bool)
    if kind == "array":
        return isinstance(value, list)
    if kind == "object":
        return isinstance(value, dict)
    return False


def _check(value, spec: dict, path: str, errors: list[str]) -> None:
    kinds = spec["type"] if isinstance(spec["type"], list) else [spec["type"]]
    if not any(_is(value, k) for k in kinds):
        errors.append(f"{path}: expected {kinds}")
        return
    if "enum" in spec and value not in spec["enum"]:
        errors.append(f"{path}: not one of {spec['enum']}")
    if isinstance(value, int) and not isinstance(value, bool) and "minimum" in spec and value < spec["minimum"]:
        errors.append(f"{path}: below {spec['minimum']}")
    if isinstance(value, int) and not isinstance(value, bool) and "maximum" in spec and value > spec["maximum"]:
        errors.append(f"{path}: above {spec['maximum']}")
    if isinstance(value, str) and "max_length" in spec and len(value) > spec["max_length"]:
        errors.append(f"{path}: longer than {spec['max_length']}")
    if isinstance(value, str) and "max_bytes" in spec and len(value.encode()) > spec["max_bytes"]:
        errors.append(f"{path}: longer than {spec['max_bytes']} bytes")
    if isinstance(value, str) and spec.get("min_length", 0) > len(value):
        errors.append(f"{path}: shorter than {spec['min_length']}")
    if isinstance(value, str) and spec.get("charset") == "id" and not all(c.isascii() and (c.isalnum() or c in "-_") for c in value):
        errors.append(f"{path}: only letters, digits, dash and underscore")
    if isinstance(value, str) and spec.get("no_control") and any(ord(c) < 32 or ord(c) == 127 for c in value):
        errors.append(f"{path}: control characters")
    if isinstance(value, list):
        if "length" in spec and len(value) != spec["length"]:
            errors.append(f"{path}: length {len(value)}")
        for index, item in enumerate(value):
            if "items" in spec:
                _check(item, spec["items"], f"{path}[{index}]", errors)
    if isinstance(value, dict) and spec["type"] == "object":
        validate_object(value, spec, path, errors)


def validate_object(value: dict, spec: dict, path: str, errors: list[str]) -> None:
    required, optional = spec.get("required", {}), spec.get("optional", {})
    for key, sub in required.items():
        if key not in value:
            errors.append(f"{path}.{key}: missing")
        else:
            _check(value[key], sub, f"{path}.{key}", errors)
    for key, item in value.items():
        if key in required:
            continue
        if key in optional:
            _check(item, optional[key], f"{path}.{key}", errors)
        else:
            errors.append(f"{path}.{key}: unknown field")


def validate_request(op: str, message: dict) -> list[str]:
    errors: list[str] = []
    body = {k: v for k, v in message.items() if k != "op"}
    validate_object(body, REQUESTS[op], "$", errors)
    return errors


def validate_response(op: str, message: dict) -> list[str]:
    errors: list[str] = []
    validate_object(message, REPLIES[op], "$", errors)
    return errors


def validate_error(message: dict) -> list[str]:
    errors: list[str] = []
    validate_object(message, ERROR_REPLY, "$", errors)
    return errors


def validate_event(message: dict) -> list[str]:
    kind = message.get("ev")
    if kind not in EVENTS:
        return [f"$.ev: unknown event {kind!r}"]
    errors: list[str] = []
    validate_object(message, EVENTS[kind], "$", errors)
    return errors
