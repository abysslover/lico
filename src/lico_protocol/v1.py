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
ERRORS = ("bad_request", "unknown_op", "unknown_session", "stale_generation", "version_unsupported", "too_large")

# Every operation of the protocol and where it is implemented. "oracle" operations
# exist in the reference prototype and in every conformance candidate; the others
# are defined here and arrive in later steps of the phase.
INVENTORY = {
    "hello": {"state": "oracle", "mutates": False},
    "status": {"state": "oracle", "mutates": False},
    "snapshot": {"state": "oracle", "mutates": False},
    "subscribe": {"state": "oracle", "mutates": True},
    "unsubscribe": {"state": "oracle", "mutates": True},
    "session.create": {"state": "planned S02", "mutates": True},
    "session.list": {"state": "planned S02", "mutates": False},
    "input": {"state": "planned S03", "mutates": True},
    "approval.answer": {"state": "planned S03", "mutates": True},
    "view.attach": {"state": "planned S03", "mutates": True},
    "view.detach": {"state": "planned S03", "mutates": True},
    "resources": {"state": "planned S03", "mutates": False},
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
    "optional": {},
}

REPLIES = {
    "hello": _obj({"v": _V, "ok": _OK, "protocol": {"type": INT, "enum": [PROTOCOL]}, "host": {"type": STR},
                   "max_request": {"type": INT}, "retain": {"type": INT, "minimum": 1},
                   "ops": {"type": "array", "items": {"type": STR}}}, COMMON_REPLY_OPTIONAL),
    "status": _obj({"v": _V, "ok": _OK, "pid": {"type": INT}, "sessions": {"type": "array", "items": {
        "type": "object",
        "required": {"session": {"type": STR}, "generation": {"type": INT, "minimum": 1}, "seq": {"type": INT, "minimum": -1},
                     "producing": {"type": BOOL}, "subscribers": {"type": INT, "minimum": 0},
                     "bytes_total": {"type": INT, "minimum": 0}}, "optional": {}}}}, COMMON_REPLY_OPTIONAL),
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
                 "generation": {"type": INT, "minimum": 1}, "seq": {"type": INT, "minimum": 0}, "data": {"type": STR}}),
    "overflow": _obj({"v": _V, "ev": {"type": STR, "enum": ["overflow"]}, "session": {"type": STR}}),
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
    if isinstance(value, str) and "max_length" in spec and len(value) > spec["max_length"]:
        errors.append(f"{path}: longer than {spec['max_length']}")
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
