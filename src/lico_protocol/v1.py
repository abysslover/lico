"""Protocol v1 schemas, a small validator and the operation inventory.

Transport: one JSON object per line over an owner-only local socket. Every message
carries ``v`` (1). A request carries ``op`` and may carry ``id`` (a client string
of at most 64 characters that the reply echoes). A reply carries ``ok`` (1 or 0).
Events carry ``ev`` and no ``ok``. Unknown fields are rejected in requests and are
a conformance failure in replies. ``ok`` stays an integer so the earlier
prototype clients keep working.
"""

from __future__ import annotations

from .codex_mapping import shows_nothing

PROTOCOL = 1
MAX_INPUT_TEXT = 64 * 1024     # bytes of one `input`: newline and tab allowed, no other control character
MAX_PATH = 1024                # characters of a folder path a session may be created in
MAX_NAME = 60                  # characters of a session name
MAX_REQUEST = 4 * MAX_INPUT_TEXT     # the text JSON-escaped (up to 3 times its size) and the rest of the line
ERRORS = ("bad_request", "unknown_op", "unknown_session", "stale_generation", "version_unsupported", "too_large",
          "session_exited", "session_disconnected", "unknown_request", "unknown_approval", "unknown_view", "busy",
          "unknown_account", "credential_store_unavailable", "build_changed", "stopping",
          "folder_not_absolute", "folder_missing", "folder_not_directory", "folder_unreadable", "folder_protected", "option_unavailable",
          "no_conversation_yet", "chain_depth", "expired", "record_not_found", "record_ambiguous", "record_format", "settings_failed",
          "xvfb_unavailable", "no_display",
          "worktree_needs_folder", "not_git_repo", "invalid_branch", "branch_exists", "worktree_failed", "worktree_dirty")

# Every operation of the protocol and where it is implemented. "oracle" operations
# exist in the reference prototype and in every conformance candidate; "host"
# operations are implemented by the owned host only; the others arrive in later
# steps of the phase.
INVENTORY = {
    "hello": {"state": "oracle", "mutates": False},
    "capabilities": {"state": "host", "mutates": False},
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
    "approval.text": {"state": "host", "mutates": False},
    "view.attach": {"state": "host", "mutates": True},
    "view.detach": {"state": "host", "mutates": True},
    "resources": {"state": "host", "mutates": False},
    "account.create": {"state": "host", "mutates": True},
    "account.rename": {"state": "host", "mutates": True},
    "account.list": {"state": "host", "mutates": False},
    "account.default": {"state": "host", "mutates": True},
    "session.rebind": {"state": "host", "mutates": True},
    "session.mode": {"state": "host", "mutates": True},
    "account.credential": {"state": "host", "mutates": True},
    "account.affected": {"state": "host", "mutates": False},
    "account.remove": {"state": "host", "mutates": True},
    "history": {"state": "host", "mutates": False},
    "host.stop": {"state": "host", "mutates": True},
    "session.options": {"state": "host", "mutates": False},
    "session.rename": {"state": "host", "mutates": True},
    "session.close": {"state": "host", "mutates": True},
    "transfer.send": {"state": "host", "mutates": True},
    "settings.set": {"state": "host", "mutates": True},
    "folder.git": {"state": "host", "mutates": False},
    "session.search": {"state": "host", "mutates": False},
    "display.ensure": {"state": "host", "mutates": True},
    "display.status": {"state": "host", "mutates": False},
    "display.stop": {"state": "host", "mutates": True},
    "real_screen.stop": {"state": "host", "mutates": True},
}
ORACLE_OPS = tuple(op for op, meta in INVENTORY.items() if meta["state"] == "oracle")

INT, STR, BOOL = "integer", "string", "boolean"


def _obj(required: dict, optional: dict | None = None) -> dict:
    return {"type": "object", "required": required, "optional": optional or {}}


# what an agent and all its descendants hold of their own (RssAnon + RssShmem), their VmRSS, and how many processes that is; kB
MEMORY = {"type": "object", "required": {"private_kb": {"type": INT, "minimum": 0}, "rss_kb": {"type": INT, "minimum": 0}, "processes": {"type": INT, "minimum": 1}},
          "optional": {}}

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
    "capabilities": _obj({
        "v": _V,
        "ok": _OK,
        "protocol": {"type": INT, "enum": [PROTOCOL]},
        "version": {"type": STR, "min_length": 1},
        "commit": {"type": STR, "min_length": 1},
        "build": {"type": STR, "min_length": 1},
        "ops": {"type": "array", "items": {"type": STR}},
        "socket_only_ops": {"type": "array", "items": {"type": STR}},
        "http_routes": {"type": "array", "items": {"type": STR}},
        "mcp_tools": {"type": "array", "items": {"type": STR}},
        "limits": _obj({
            "max_request": {"type": INT, "minimum": 1},
            "max_input_text": {"type": INT, "minimum": 1},
            "max_sessions": {"type": INT, "minimum": 1},
            "retain": {"type": INT, "minimum": 1},
            "font_px_min": {"type": INT, "minimum": 1},
            "font_px_max": {"type": INT, "minimum": 1},
        }),
    }, COMMON_REPLY_OPTIONAL),
    "status": _obj({"v": _V, "ok": _OK, "pid": {"type": INT}, "sessions": {"type": "array", "items": {
        "type": "object",
        "required": {"session": {"type": STR}, "generation": {"type": INT, "minimum": 1}, "seq": {"type": INT, "minimum": -1},
                     "producing": {"type": BOOL}, "subscribers": {"type": INT, "minimum": 0},
                     "bytes_total": {"type": INT, "minimum": 0}},
        "optional": {"oldest": {"type": INT, "minimum": 0}, "state": {"type": STR, "enum": ["running", "exited", "disconnected"]},
                     "account": {"type": STR}, "provider": {"type": STR, "enum": ["codex", "claude"]}, "model": {"type": STR, "max_length": 60}, "mode": {"type": STR, "enum": ["auto", "default"]}, "activity": {"type": STR, "enum": ["working", "idle"]}, "account_issue": {"type": STR}, "state_at": {"type": INT, "minimum": 0}, "busy_since": {"type": INT, "minimum": 0}, "last_event_at": {"type": INT, "minimum": 0}, "stalled": {"type": BOOL}, "credential_generation": {"type": INT, "minimum": 1},
                     "scratch": {"type": BOOL}, "cwd": {"type": STR, "max_length": MAX_PATH}, "name": {"type": STR, "max_length": MAX_NAME},
                     "finished_seq": {"type": INT, "minimum": -1},
                     "worktree": _obj({"repo": {"type": STR, "max_length": MAX_PATH}, "branch": {"type": STR, "max_length": 80}, "exists": {"type": BOOL}}),
                     "memory": MEMORY,   # of a running session, from the second status call on
                     "virtual_display": {"type": STR, "max_length": 16},
                     "chosen_model": {"type": STR, "max_length": 60}, "chosen_effort": {"type": STR, "max_length": 16},
                     "approvals": {"type": "array", "items": {"type": "object", "required": {
                         "approval_id": {"type": INT, "minimum": 1}, "prompt": {"type": STR, "max_length": 200}},
                         "optional": {"tool": {"type": STR, "enum": ["send", "create", "history", "capture", "clipboard_read", "clipboard_write", "computer"]}}}},    # a question the host asks for an agent's request; the program's own has none
                     "allowed": {"type": "array", "items": {"type": "object", "required": {"tool": {"type": STR, "enum": ["send", "history"]}, "to": {"type": STR, "max_length": 64}}}}}}}},    # what the person said yes to for as long as the session lives
                  {**COMMON_REPLY_OPTIONAL, "build": {"type": STR, "max_length": 32}, "logins_running": {"type": INT, "minimum": 0}, "host_memory": MEMORY,
                   "settings": {"type": "object", "required": {"clipboard_write": {"type": BOOL}, "notifications": {"type": BOOL}, "group_by": {"type": STR, "enum": ["account", "workspace"]}, "font_px": {"type": INT, "minimum": 12, "maximum": 20}}, "optional": {}},
                   "real_screen": {"type": "array", "items": _obj({"session": {"type": STR}, "name": {"type": STR, "max_length": MAX_NAME}, "seconds_left": {"type": INT, "minimum": 1}})}}),
    "session.create": _obj({"v": _V, "ok": _OK, "session": {"type": STR}, "generation": {"type": INT, "minimum": 1}},
                           COMMON_REPLY_OPTIONAL),
    "session.rename": _obj({"v": _V, "ok": _OK, "session": {"type": STR}, "name": {"type": [STR, "null"], "max_length": MAX_NAME}}, COMMON_REPLY_OPTIONAL),
    "session.options": _obj({"v": _V, "ok": _OK, "models": {"type": "array", "items": {"type": STR}}, "efforts": {"type": "array", "items": {"type": STR}},
                             "issue": {"type": [STR, "null"]}, "recent": {"type": "array", "items": {"type": STR, "max_length": MAX_PATH}}},
                            {**COMMON_REPLY_OPTIONAL, "last": {"type": "object", "required": {}, "optional": {   # how the last session was started; an older host has none
                                "kind": {"type": STR, "enum": ["codex", "claude"]}, "account": {"type": STR, "max_length": 64},
                                "mode": {"type": STR, "enum": ["auto", "default"]}, "cwd": {"type": STR, "max_length": MAX_PATH},
                                "model": {"type": STR, "max_length": 60}, "effort": {"type": STR, "max_length": 16}}}}),
    "input": _obj({"v": _V, "ok": _OK, "session": {"type": STR}, "request_id": {"type": STR},
                   "state": {"type": STR, "enum": ["accepted", "submitted", "observed", "failed"]}, "duplicate": {"type": BOOL}},
                  COMMON_REPLY_OPTIONAL),
    "input.status": _obj({"v": _V, "ok": _OK, "session": {"type": STR}, "request_id": {"type": STR},
                          "state": {"type": STR, "enum": ["accepted", "submitted", "observed", "failed"]}}, COMMON_REPLY_OPTIONAL),
    "approval.answer": _obj({"v": _V, "ok": _OK, "session": {"type": STR}, "approval_id": {"type": INT, "minimum": 1},
                             "state": {"type": STR, "enum": ["answered"]}},
                            {**COMMON_REPLY_OPTIONAL, "outcome": {"type": STR, "enum": ["done", "declined", "not_done"]},   # only of an agent's request: what the answer led to
                             "reason": {"type": STR, "max_length": 64}}),                                                    # why it was not done: the name of the refusal
    "approval.text": _obj({"v": _V, "ok": _OK, "session": {"type": STR}, "approval_id": {"type": INT, "minimum": 1}, "text": {"type": STR, "max_length": 16384},
                           "offset": {"type": INT, "minimum": 0}, "next": {"type": [INT, "null"], "minimum": 1}, "total": {"type": INT, "minimum": 0}}, COMMON_REPLY_OPTIONAL),
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
    "session.close": _obj({"v": _V, "ok": _OK, "session": {"type": STR}, "generation": {"type": INT, "minimum": 1}}, COMMON_REPLY_OPTIONAL),
    "session.rebind": _obj({"v": _V, "ok": _OK, "session": {"type": STR}, "generation": {"type": INT, "minimum": 1}},
                           COMMON_REPLY_OPTIONAL),
    "session.mode": _obj({"v": _V, "ok": _OK, "session": {"type": STR}, "generation": {"type": INT, "minimum": 1},
                          "mode": {"type": STR, "enum": ["auto", "default"]}},
                         COMMON_REPLY_OPTIONAL),
    "transfer.send": _obj({"v": _V, "ok": _OK, "to": {"type": STR}, "request_id": {"type": STR}, "kind": {"type": STR, "enum": ["text", "lines", "conversation"]},
                           "delivery": {"type": STR, "enum": ["inline", "file"]}, "size": {"type": INT, "minimum": 1},
                           "receiver": {"type": STR, "enum": ["idle", "busy", "waiting_for_approval"]}, "hops": {"type": INT, "minimum": 0}},
                          COMMON_REPLY_OPTIONAL),
    "account.create": _obj({"v": _V, "ok": _OK, "account": {"type": STR}, "provider": {"type": STR, "enum": ["codex", "claude"]},
                            "label": {"type": STR, "max_length": 40}}, COMMON_REPLY_OPTIONAL),
    "account.rename": _obj({"v": _V, "ok": _OK, "account": {"type": STR}, "provider": {"type": STR, "enum": ["codex", "claude"]},
                            "label": {"type": STR, "max_length": 40}}, COMMON_REPLY_OPTIONAL),
    "account.list": _obj({"v": _V, "ok": _OK, "accounts": {"type": "array", "items": {
        "type": "object",
        "required": {"account": {"type": STR}, "provider": {"type": STR, "enum": ["codex", "claude"]},
                     "label": {"type": STR, "max_length": 40}, "default": {"type": BOOL},
                     "credential_generation": {"type": INT, "minimum": 1}, "has_credential": {"type": BOOL},
                     "usage": {"type": "null"}},
        "optional": {"issue": {"type": STR}, "issue_sessions": {"type": "array", "items": {"type": STR}}}}}}, COMMON_REPLY_OPTIONAL),
    "account.affected": _obj({"v": _V, "ok": _OK, "account": {"type": STR}, "sessions": {"type": "array", "items": {
        "type": "object", "required": {"session": {"type": STR}, "state": {"type": STR, "enum": ["running", "exited", "disconnected"]}},
        "optional": {}}}}, COMMON_REPLY_OPTIONAL),
    "account.remove": _obj({"v": _V, "ok": _OK, "account": {"type": STR}, "metadata_removed": {"type": BOOL},
                            "credential": {"type": STR, "enum": ["removed", "none"]}, "remote_revocation": {"type": STR, "enum": ["not_performed"]},
                            "home": {"type": STR, "enum": ["kept", "removed", "none"]}, "affected": {"type": "array", "items": {"type": STR}}},
                           COMMON_REPLY_OPTIONAL),
    "account.default": _obj({"v": _V, "ok": _OK, "account": {"type": STR}}, COMMON_REPLY_OPTIONAL),
    "history": _obj({"v": _V, "ok": _OK, "session": {"type": STR}, "lines": {"type": "array", "items": {"type": "array", "length": 2}},
                     "oldest": {"type": INT, "minimum": 0}}, COMMON_REPLY_OPTIONAL),
    "account.credential": _obj({"v": _V, "ok": _OK, "account": {"type": STR}}, COMMON_REPLY_OPTIONAL),
    "snapshot": _obj({"v": _V, "ok": _OK, **SNAPSHOT_BODY["required"]}, COMMON_REPLY_OPTIONAL),
    "subscribe": _obj({"v": _V, "ok": _OK, "session": {"type": STR}, "generation": {"type": INT, "minimum": 1},
                       "gap": {"type": BOOL}, "from": {"type": INT, "minimum": 0}},
                      {**COMMON_REPLY_OPTIONAL, "snapshot": SNAPSHOT_BODY, "missed": {"type": [INT, "null"]}}),
    "unsubscribe": _obj({"v": _V, "ok": _OK, "session": {"type": STR}, "seq": {"type": INT, "minimum": -1}},
                        COMMON_REPLY_OPTIONAL),
    "settings.set": _obj({"v": _V, "ok": _OK, "settings": {"type": "object", "required": {"clipboard_write": {"type": BOOL}, "notifications": {"type": BOOL}, "group_by": {"type": STR, "enum": ["account", "workspace"]}, "font_px": {"type": INT, "minimum": 12, "maximum": 20}}, "optional": {}}},
                         COMMON_REPLY_OPTIONAL),
    "folder.git": _obj({"v": _V, "ok": _OK, "is_git": {"type": BOOL}, "repo": {"type": [STR, "null"], "max_length": MAX_PATH}},
                       {**COMMON_REPLY_OPTIONAL, "reason": {"type": STR, "max_length": 32}}),
    "session.search": _obj({"v": _V, "ok": _OK, "query": {"type": STR, "max_length": 256}, "scope": {"type": STR, "enum": ["session", "all"]},
                            "matches": {"type": "array", "items": _obj({"session": {"type": STR}, "name": {"type": STR, "max_length": MAX_NAME},
                                                                        "seq": {"type": INT, "minimum": 0}, "text": {"type": STR, "max_length": 240}})},
                            "total": {"type": INT, "minimum": 0}, "truncated": {"type": BOOL}},
                           COMMON_REPLY_OPTIONAL),
    "display.ensure": _obj({"v": _V, "ok": _OK, "session": {"type": STR}, "display": {"type": STR, "max_length": 16}, "geometry": {"type": STR, "max_length": 32}},
                           COMMON_REPLY_OPTIONAL),
    "display.status": _obj({"v": _V, "ok": _OK, "session": {"type": STR}, "running": {"type": BOOL}, "display": {"type": [STR, "null"], "max_length": 16}},
                           {**COMMON_REPLY_OPTIONAL, "geometry": {"type": STR, "max_length": 32}, "memory": MEMORY}),
    "display.stop": _obj({"v": _V, "ok": _OK, "session": {"type": STR}, "stopped": {"type": BOOL}}, COMMON_REPLY_OPTIONAL),
    "real_screen.stop": _obj({"v": _V, "ok": _OK, "stopped": {"type": INT, "minimum": 0}}, COMMON_REPLY_OPTIONAL),
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
                      "approval_id": {"type": INT, "minimum": 1}, "prompt": {"type": STR, "max_length": 200}}, {"tool": {"type": STR, "enum": ["send", "create", "history", "capture", "clipboard_read", "clipboard_write", "computer"]}}),
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
    "session.purge": _obj({"session": {"type": STR, "max_length": 64}, "generation": {"type": INT, "minimum": 1}},
                          {**_COMMON_REQ, "worktree": {"type": STR, "enum": ["keep", "remove"]}, "force_dirty": {"type": BOOL}}),
    "session.close": _obj({"session": {"type": STR, "max_length": 64}, "generation": {"type": INT, "minimum": 1}},
                          {**_COMMON_REQ, "worktree": {"type": STR, "enum": ["keep", "remove"]}, "force_dirty": {"type": BOOL}}),
    "session.create": _obj({"kind": {"type": STR, "enum": ["synthetic", "codex", "claude"]}},
                           {**_COMMON_REQ, "mode": {"type": STR, "enum": ["auto", "default"]}, "rate": {"type": INT, "minimum": 0, "maximum": 10000},
                            "history": {"type": INT, "minimum": 0, "maximum": 100000},
                            "payload": {"type": INT, "minimum": 1, "maximum": 4096},
                            "account": {"type": STR, "max_length": 64},
                            "cwd": {"type": STR, "min_length": 1, "max_length": MAX_PATH, "no_control": True, "no_hidden": True},
                            "worktree_branch": {"type": STR, "min_length": 1, "max_length": 80, "no_control": True, "no_hidden": True},
                            "name": {"type": STR, "max_length": MAX_NAME, "no_control": True, "no_hidden": True, "no_blank": "trimmed"},
                            "model": {"type": STR, "min_length": 1, "max_length": 60, "charset": "model"},
                            "effort": {"type": STR, "min_length": 1, "max_length": 16, "charset": "lower"}}),
    "session.rename": _obj({"session": {"type": STR, "max_length": 64}, "name": {"type": STR, "max_length": MAX_NAME, "no_control": True, "no_hidden": True, "no_blank": "trimmed"}}, _COMMON_REQ),
    "session.options": _obj({}, _COMMON_REQ),
    "session.rebind": _obj({"session": {"type": STR, "max_length": 64}, "generation": {"type": INT, "minimum": 1},
                            "account": {"type": STR, "max_length": 64}}, _COMMON_REQ),
    "session.mode": _obj({"session": {"type": STR, "max_length": 64}, "generation": {"type": INT, "minimum": 1},
                          "mode": {"type": STR, "enum": ["auto", "default"]}}, _COMMON_REQ),
    "account.create": _obj({"provider": {"type": STR, "enum": ["codex", "claude"]},
                            "label": {"type": STR, "min_length": 1, "max_length": 40, "no_control": True, "no_hidden": True, "no_blank": True}}, _COMMON_REQ),
    "account.rename": _obj({"account": {"type": STR, "max_length": 64},
                            "label": {"type": STR, "min_length": 1, "max_length": 40, "no_control": True, "no_hidden": True, "no_blank": True}}, _COMMON_REQ),
    "account.list": _obj({}, _COMMON_REQ),
    "history": _obj({"session": {"type": STR, "max_length": 64}, "before": {"type": INT, "minimum": 0}, "limit": {"type": INT, "minimum": 1, "maximum": 200}}, _COMMON_REQ),
    "account.affected": _obj({"account": {"type": STR, "max_length": 64}}, _COMMON_REQ),
    "account.remove": _obj({"account": {"type": STR, "max_length": 64}, "sessions": {"type": STR, "enum": ["detach", "end"]}}, _COMMON_REQ),
    "account.credential": _obj({"account": {"type": STR, "max_length": 64}, "credential_ref": {"type": STR, "max_length": 64}}, _COMMON_REQ),
    "account.default": _obj({"account": {"type": STR, "max_length": 64}}, _COMMON_REQ),
    "session.list": _obj({}, _COMMON_REQ),
    # `text` is required for text and lines and absent for a conversation, which takes `from` instead (the rule is `_transfer_shape`)
    "transfer.send": _obj({"to": {"type": STR, "max_length": 64}, "kind": {"type": STR, "enum": ["text", "lines", "conversation"]}},
                          {**_COMMON_REQ, "from": {"type": STR, "max_length": 64},
                           "text": {"type": STR, "min_length": 1, "max_bytes": MAX_INPUT_TEXT, "no_control": True, "allow": "\n\t"}}),
    "input": _obj({"session": {"type": STR, "max_length": 64}, "generation": {"type": INT, "minimum": 1},
                   "request_id": {"type": STR, "min_length": 1, "max_length": 64, "charset": "id"},
                   "text": {"type": STR, "min_length": 1, "max_bytes": MAX_INPUT_TEXT, "no_control": True, "allow": "\n\t"}}, _COMMON_REQ),
    "input.status": _obj({"session": {"type": STR, "max_length": 64},
                          "request_id": {"type": STR, "min_length": 1, "max_length": 64, "charset": "id"}},
                         _COMMON_REQ),
    "approval.answer": _obj({"session": {"type": STR, "max_length": 64}, "generation": {"type": INT, "minimum": 1},
                             "approval_id": {"type": INT, "minimum": 1}, "answer": {"type": STR, "enum": ["allow", "deny"]}},
                            {**_COMMON_REQ, "keep": {"type": BOOL}}),    # with an allow of an agent's request: no more questions for this kind of request while the session lives
    "approval.text": _obj({"session": {"type": STR, "max_length": 64}, "generation": {"type": INT, "minimum": 1}, "approval_id": {"type": INT, "minimum": 1}},
                          {**_COMMON_REQ, "offset": {"type": INT, "minimum": 0}, "limit": {"type": INT, "minimum": 4, "maximum": 16384}}),    # one page of the text that waits behind an agent's request
    "view.attach": _obj({"session": {"type": STR, "max_length": 64}, "kind": {"type": STR, "enum": ["gui", "cli"]}},
                        {**_COMMON_REQ, "generation": {"type": INT, "minimum": 1}, "after": {"type": INT, "minimum": -1}}),
    "view.detach": _obj({"view": {"type": STR, "max_length": 64}}, _COMMON_REQ),
    "resources": _obj({}, _COMMON_REQ),
    "settings.set": _obj({}, {**_COMMON_REQ, "clipboard_write": {"type": BOOL}, "notifications": {"type": BOOL}, "group_by": {"type": STR, "enum": ["account", "workspace"]}, "font_px": {"type": INT, "minimum": 12, "maximum": 20}}),
    "folder.git": _obj({"cwd": {"type": STR, "min_length": 1, "max_length": MAX_PATH, "no_control": True}}, _COMMON_REQ),
    "session.search": _obj({"query": {"type": STR, "min_length": 1, "max_length": 256, "no_control": True, "no_hidden": True}},
                           {**_COMMON_REQ, "session": {"type": STR, "max_length": 64}, "limit": {"type": INT, "minimum": 1, "maximum": 500}}),
    "display.ensure": _obj({"session": {"type": STR, "max_length": 64}}, _COMMON_REQ),
    "display.status": _obj({"session": {"type": STR, "max_length": 64}}, _COMMON_REQ),
    "display.stop": _obj({"session": {"type": STR, "max_length": 64}}, _COMMON_REQ),
    "real_screen.stop": _obj({}, {**_COMMON_REQ, "session": {"type": STR, "max_length": 64}}),
    "capabilities": _obj({}, _COMMON_REQ),
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


def _is_hidden(char: str) -> bool:
    """A character that draws nothing or reorders text; the list of `dropped` in hostd/src/codex.rs.
    U+200C and U+200D are not on it (Persian and Indic scripts, emoji sequences)."""
    code = ord(char)
    return (code in (0x061C, 0x200B, 0x200E, 0x200F, 0x2028, 0x2029, 0xFEFF) or 0x202A <= code <= 0x202E
            or 0x2060 <= code <= 0x2064 or 0x2066 <= code <= 0x2069 or 0xE0000 <= code <= 0xE007F)


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
    if isinstance(value, str) and spec.get("charset") == "model" and not (value[:1].isalnum() and all(c.isascii() and (c.isalnum() or c in "._-") for c in value)):
        errors.append(f"{path}: only letters, digits, dot, dash and underscore, starting with a letter or digit")
    if isinstance(value, str) and spec.get("charset") == "lower" and not all("a" <= c <= "z" for c in value):
        errors.append(f"{path}: only lowercase letters")
    if isinstance(value, str) and spec.get("no_control") and any((ord(c) < 32 or 127 <= ord(c) <= 159) and c not in spec.get("allow", "") for c in value):
        errors.append(f"{path}: control characters")
    if isinstance(value, str) and spec.get("no_hidden") and any(_is_hidden(c) for c in value):
        errors.append(f"{path}: control characters")
    if isinstance(value, str) and spec.get("no_blank") and shows_nothing(value, trimmed=spec["no_blank"] == "trimmed"):
        errors.append(f"{path}: shows nothing")
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


def _transfer_shape(body: dict) -> list[str]:
    """A conversation is the whole conversation of the session `from`: no text. Text and lines are text."""
    if body.get("kind") == "conversation":
        return ["$.from: missing"] * ("from" not in body) + ["$.text: not allowed for a conversation"] * ("text" in body)
    return ["$.text: missing"] * ("text" not in body)


def validate_request(op: str, message: dict) -> list[str]:
    errors: list[str] = []
    body = {k: v for k, v in message.items() if k != "op"}
    validate_object(body, REQUESTS[op], "$", errors)
    if op == "transfer.send":
        errors += _transfer_shape(body)
    if op == "settings.set" and not any(k in body for k in ("clipboard_write", "notifications", "group_by", "font_px")):
        errors.append("$.settings.set: at least one setting field required")
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
