"""A real provider turn, run by the owner (never by the development session).

It starts the owned host with the real Codex app-server, opens one session, sends the owner's
prompts as input (at most `--turns`, default 1, never more than 3) and writes what the host
recorded. Lico does not read, copy or move the provider's sign-in state: the provider process
finds its own, exactly as when run by hand. Each turn can be billed to the owner's account;
nothing runs unless the owner passes `--accept-billing`.

Codex runs with approval policy "untrusted" and a read-only sandbox; Claude Code runs in its default
permission mode and, unbound, loads the owner's own settings (whose allow rules and hooks can widen what it
may do without asking). Every approval a provider asks for is DENIED here, and counted, because nobody
is watching this script.

    python -m lico_measure real-turn --accept-billing --turns 1 --out DIR [--prompt TEXT]

`--provider-bin PATH` points at another program that speaks the same protocol (the simulator
is used this way for a dry run that costs nothing).
"""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

from .p02 import ProtoConn, read_log
from .rig import REPO, Host, private_dir

DEFAULT_PROMPT = "Reply with one short sentence in Korean and one in English saying the session works. Do not run any command."
MAX_TURNS = 3
TURN_TIMEOUT_S = 180
FINISHED = ("[turn completed]", "[turn failed]", "[turn interrupted]")
REFUSED = "[input refused]"                 # the provider refused the input (for example, it is not signed in)


PROVIDERS = {"codex": {"home": "CODEX_HOME", "default_home": ".codex", "record": "codex"},
             "claude": {"home": "CLAUDE_CONFIG_DIR", "default_home": ".claude", "record": "claude"}}


def _pinned(provider: str) -> str:
    record = REPO / "src" / "host" / "adapters" / PROVIDERS[provider]["record"] / "capabilities.json"
    return json.loads(record.read_text())["version"]["cli"]


def run(out: Path, turns: int, prompt: str, provider_bin: str | None, provider: str = "codex", allow_unpinned: bool = False) -> dict:
    if not 1 <= turns <= MAX_TURNS:
        raise SystemExit(f"--turns must be between 1 and {MAX_TURNS}")
    binary = str(Path(provider_bin).resolve()) if provider_bin else shutil.which(provider)   # the provider runs in a private directory
    if not binary:
        raise SystemExit(f"the {provider} program was not found on PATH (use --provider-bin for another one)")
    simulated = binary.endswith(".py")                 # a protocol simulator; a real program given with --provider-bin is still pinned
    receipt = _receipt(binary)
    if not simulated and _pinned(provider) not in receipt["provider_version"].replace("(", " ").replace(")", " ").split() and not allow_unpinned:
        raise SystemExit(f"installed {provider} reports {receipt['provider_version']!r}; the adapter is pinned to {_pinned(provider)}. "
                         "Nothing was run. Pass --allow-unpinned-version to run anyway (behavior is then unverified).")
    os.umask(0o077)                                  # everything written here is the owner's alone
    out.mkdir(mode=0o700, parents=True, exist_ok=True)
    os.chmod(out, 0o700)
    result: dict = {"provider_program": Path(binary).name, "turns_requested": turns, "simulated": simulated,
                    "approvals_denied": 0, "turns": [], "receipt": receipt, "provider": provider}
    # The provider keeps its own sign-in in its own home (CODEX_HOME), which is passed on as a path and never
    # read here. Its HOME is an empty private directory, so a command it runs does not find the owner's files
    # (ssh keys, shell history, other tools' settings) under the real home. Other files the owner's account can
    # read are still reachable by a command; the prompt asks for none and the sandbox is read-only (unverified).
    with private_dir() as directory:
        provider_home = Path(directory) / "provider_home"
        provider_home.mkdir(mode=0o700)
        variable = PROVIDERS[provider]["home"]
        if simulated:                                 # a dry run never points anything at the owner's provider home
            keep_home = str(Path(directory) / "provider_own_home")
        else:
            keep_home = os.environ.get(variable) or str(Path.home() / PROVIDERS[provider]["default_home"])
        overrides = {"HOME": str(provider_home), variable: keep_home}
        from .p02 import start

        flag = "--" + provider
        program = [flag, binary]
        if simulated:                               # a protocol simulator for a free dry run
            fixture = (REPO / "tests" / "fixtures" / "codex_app_server" / "turn_with_approval.json" if provider == "codex"
                       else REPO / "tests" / "fixtures" / "claude_stream" / "turn_with_permission.json")
            program = [flag, sys.executable, flag + "-arg", "-I", flag + "-arg", binary, flag + "-arg", "--fixture", flag + "-arg", str(fixture)]
        host, log = start(directory, extra=program, sessions=0, producing=0, history=0, env_overrides=overrides)
        try:
            conn = ProtoConn(host.socket, timeout=30.0)
            made = conn.call(op="session.create", kind=provider)
            if made.get("ok") != 1:
                result["error"] = made
                return _finish(out, result, log, None)
            sid, generation = made["session"], made["generation"]
            for k in range(turns):
                before = len(read_log(log, sid))
                reply = conn.call(op="input", session=sid, generation=generation, request_id=f"real-{k + 1}", text=prompt)
                turn = {"accepted": reply.get("ok") == 1, "reply": reply}
                deadline = time.monotonic() + TURN_TIMEOUT_S
                while reply.get("ok") == 1 and time.monotonic() < deadline:
                    status = [s for s in conn.call(op="status")["sessions"] if s["session"] == sid][0]
                    for approval in status.get("approvals", []):
                        conn.call(op="approval.answer", session=sid, generation=generation, approval_id=approval["approval_id"], answer="deny")
                        result["approvals_denied"] += 1
                    records = [p.decode(errors="replace") for _, p in read_log(log, sid)[before:]]
                    if any(r in FINISHED for r in records):
                        turn["finished"] = [r for r in records if r in FINISHED][-1]
                        break
                    refused = [r for r in records if r.startswith(REFUSED)]
                    if refused:
                        turn["finished"] = refused[0]
                        break
                    if status["state"] != "running":
                        turn["finished"] = f"session {status['state']}"
                        break
                    time.sleep(0.5)
                else:
                    turn["finished"] = "timeout" if reply.get("ok") == 1 else "refused"
                result["turns"].append(turn)
                if turn.get("finished") not in FINISHED:
                    break
            finished = _finish(out, result, log, sid)
            conn.call(op="session.purge", session=sid, generation=generation)    # ends the provider and every helper it started
            return finished
        finally:
            host.stop()


def _receipt(binary: str) -> dict:
    # the owner runs this, so the program may be asked its version (the development session never does)
    version = subprocess.run([binary, "--version"], capture_output=True, text=True, timeout=30).stdout.strip() if not binary.endswith(".py") else "simulator"
    commit = subprocess.run(["git", "rev-parse", "HEAD"], cwd=REPO, capture_output=True, text=True).stdout.strip()
    return {"commit": commit, "provider_version": version, "provider_binary_sha256": hashlib.sha256(Path(binary).read_bytes()).hexdigest(),
            "started": time.strftime("%Y-%m-%dT%H:%M:%S%z")}


def _finish(out: Path, result: dict, log: Path, sid: str | None) -> dict:
    records = [p.decode(errors="replace") for _, p in read_log(log, sid)] if sid else []
    (out / "transcript.txt").write_text("\n".join(records) + "\n", encoding="utf-8")   # conversation content: owner only (umask 077)
    result["records"] = len(records)
    (out / "result.json").write_text(json.dumps(result, indent=1, ensure_ascii=False))
    return result


def main(args) -> int:
    if not args.accept_billing:
        print("A real turn can be billed to your account. Nothing was run; pass --accept-billing to run it.", file=sys.stderr)
        return 2
    result = run(Path(args.out), args.turns, args.prompt, args.provider_bin, args.provider, args.allow_unpinned_version)
    print(json.dumps({k: v for k, v in result.items() if k != "turns"} | {"finished": [t.get("finished") for t in result["turns"]]}, ensure_ascii=False))
    return 0 if result["turns"] and all(t.get("finished") == "[turn completed]" for t in result["turns"]) else 1
