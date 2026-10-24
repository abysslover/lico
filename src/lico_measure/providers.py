"""Provider capability data: what an installed provider path offers, pinned to its version.

Everything here runs the installed provider with a private empty home and a minimal
environment: no sign-in, no credential and no model turn. A provider's login state
belongs to the provider and is never read.
"""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
from pathlib import Path

from lico_protocol.jsonschema_lite import validate  # noqa: F401  (re-exported for tests)

from .rig import REPO

CAPABILITIES = REPO / "src" / "host" / "adapters" / "codex" / "capabilities.json"
SCHEMA_FILES = ("ClientRequest.json", "ServerRequest.json", "ServerNotification.json", "ClientNotification.json")
SERVER_DECISIONS = ("mapped_to_host_approval", "answered_with_error")


def codex_binary() -> str:
    found = shutil.which("codex")
    if not found:
        raise RuntimeError("the codex command is not installed")
    return found


def _private_env(home: Path) -> dict:
    (home / "home").mkdir(parents=True, exist_ok=True)
    (home / "codex").mkdir(parents=True, exist_ok=True)
    return {"PATH": "/usr/bin:/bin", "HOME": str(home / "home"), "CODEX_HOME": str(home / "codex"), "LANG": "C.UTF-8"}


def installed_version(home: Path) -> str:
    out = subprocess.run([codex_binary(), "--version"], env=_private_env(home), capture_output=True, text=True, timeout=30)
    return out.stdout.strip().split()[-1] if out.stdout.strip() else ""


def generate_schema(home: Path, out: Path) -> None:
    done = subprocess.run([codex_binary(), "app-server", "generate-json-schema", "--out", str(out)],
                          env=_private_env(home), capture_output=True, text=True, timeout=60)
    if done.returncode != 0:
        raise RuntimeError("schema generation failed")


def _methods(path: Path) -> list[str]:
    names = []
    for option in json.loads(path.read_text())["oneOf"]:
        method = option["properties"]["method"]
        names += method.get("enum") or [method.get("const")]
    return sorted(n for n in names if n)


def schema_inventory(schema_dir: Path) -> dict:
    return {
        "client_requests": _methods(schema_dir / "ClientRequest.json"),
        "server_requests": _methods(schema_dir / "ServerRequest.json"),
        "server_notifications": _methods(schema_dir / "ServerNotification.json"),
        "sha256": {name: hashlib.sha256((schema_dir / name).read_bytes()).hexdigest() for name in SCHEMA_FILES},
    }


def problems(cap: dict, installed: str, inventory: dict) -> list[str]:
    """Why this capability record cannot be trusted for the installed provider (empty: it can)."""
    found: list[str] = []
    version = cap.get("version", {})
    if not version.get("pinned") or not version.get("cli"):
        found.append("the version is not pinned")
    elif version["cli"] != installed:
        found.append(f"installed version {installed} is not the pinned {version['cli']}")
    if cap.get("protocol_schema", {}).get("sha256") != inventory["sha256"]:
        found.append("the provider's protocol schema differs from the recorded one")
    for method in inventory["server_requests"]:
        decision = cap.get("server_requests", {}).get(method, {}).get("decision")
        if decision not in SERVER_DECISIONS:
            found.append(f"server request {method} has no handling decision")
    for method in cap.get("client_requests", {}).get("mapped", []):
        if method not in inventory["client_requests"]:
            found.append(f"mapped client request {method} is not in the schema")
    for method in cap.get("notifications", {}).get("mapped", []):
        if method not in inventory["server_notifications"]:
            found.append(f"mapped notification {method} is not in the schema")
    if not cap.get("unverified"):
        found.append("nothing is marked unverified")
    for item in cap.get("unsupported", []):
        if not item.get("reason"):
            found.append(f"unsupported entry {item.get('what')} has no reason")
    return found


def record_codex(out: Path = CAPABILITIES) -> dict:
    """Write the capability record for the installed provider (the decisions are authored here)."""
    import tempfile

    with tempfile.TemporaryDirectory(dir=REPO / "private" / "scratch") as scratch:
        home = Path(scratch)
        version = installed_version(home)
        generate_schema(home, home / "schema")
        inventory = schema_inventory(home / "schema")
    approvals = ("item/commandExecution/requestApproval", "item/fileChange/requestApproval", "item/permissions/requestApproval")
    cap = {
        "schema": "lico.provider_capabilities.v1",
        "provider": "codex",
        "path": "codex app-server over standard input and output (JSON lines)",
        "version": {"cli": version, "pinned": True, "checked": "2026-10-23",
                    "how": "the installed binary run with a private empty home and a minimal environment: no sign-in, no model turn"},
        "protocol_schema": {"generated_with": "codex app-server generate-json-schema", "sha256": inventory["sha256"],
                            "client_requests": len(inventory["client_requests"]),
                            "server_requests": inventory["server_requests"],
                            "server_notifications": len(inventory["server_notifications"])},
        "handshake": ["initialize", "initialized"],
        "observed_without_credentials": {
            "initialize": "answers with userAgent, codexHome, platformFamily and platformOs",
            "account/read": {"account": None, "requiresOpenaiAuth": True},
            "thread/list": "an empty list",
            "configWarning": "the Linux sandbox uses bubblewrap and needs access to create user namespaces (reported on the development host)",
        },
        "client_requests": {"mapped": ["initialize", "thread/start", "thread/resume", "thread/turns/list",
                                       "thread/items/list", "turn/start", "turn/steer", "turn/interrupt", "thread/unsubscribe"],
                            "not_forwarded": "every other client request of the schema is refused by the adapter"},
        "server_requests": {m: {"decision": "mapped_to_host_approval" if m in approvals else "answered_with_error",
                                "note": "becomes a host approval bound to its id" if m in approvals else
                                        "answered with a JSON-RPC error; the adapter supplies no tokens, tools or attestations"}
                            for m in inventory["server_requests"]},
        "notifications": {"mapped": ["item/agentMessage/delta", "item/commandExecution/outputDelta", "item/started",
                                     "item/completed", "turn/started", "turn/completed", "error", "serverRequest/resolved",
                                     "thread/started", "thread/status/changed"],
                          "others": "ignored and counted, never an error"},
        "unsupported": [
            {"what": "websocket and unix transports", "reason": "the provider documents websocket as experimental and unsupported"},
            {"what": "account login and token refresh by Lico", "reason": "sign-in stays with the provider's own storage until M01-P04"},
            {"what": "dynamic tools, attestation and MCP elicitation", "reason": "no consumer exists in this phase; answered with an error"},
        ],
        "unverified": [
            "thread/start and turn/start with a signed-in account (no sign-in was used; no model turn ran)",
            "the order and exact content of approval events during a real turn",
            "whether the sandbox works on the development host (a warning says user namespaces are needed)",
            "the Claude Code terminal path (its version could not be read by the development session)",
        ],
    }
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(cap, indent=1) + "\n")
    return cap


def message_errors(schema_dir: Path, file: str, method: str, params: dict) -> list[str]:
    """Check a message's params against the provider's own generated schema."""
    root = json.loads((schema_dir / file).read_text())
    for option in root["oneOf"]:
        names = option["properties"]["method"].get("enum") or [option["properties"]["method"].get("const")]
        if method in names:
            return validate(params, option["properties"].get("params", {}), root)
    return [f"{method}: not in {file}"]
