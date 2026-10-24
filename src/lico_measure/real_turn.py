"""A real provider turn, run by the owner (never by the development session).

It starts the owned host with the real Codex app-server, opens one session, sends the owner's
prompts as input (at most `--turns`, default 1, never more than 3) and writes what the host
recorded. Lico does not read, copy or move the provider's sign-in state: the provider process
finds its own, exactly as when run by hand. Each turn can be billed to the owner's account;
nothing runs unless the owner passes `--accept-billing`.

The provider runs with approval policy "untrusted" and a read-only sandbox. Every approval it
asks for is DENIED here, and counted, because nobody is watching this script.

    python -m lico_measure real-turn --accept-billing --turns 1 --out DIR [--prompt TEXT]

`--provider-bin PATH` points at another program that speaks the same protocol (the simulator
is used this way for a dry run that costs nothing).
"""

from __future__ import annotations

import json
import os
import shutil
import sys
import time
from pathlib import Path

from .p02 import ProtoConn, read_log
from .rig import Host, private_dir

DEFAULT_PROMPT = "Reply with one short sentence in Korean and one in English saying the session works. Do not run any command."
MAX_TURNS = 3
TURN_TIMEOUT_S = 180
FINISHED = ("[turn completed]", "[turn failed]", "[turn interrupted]")
REFUSED = "[input refused]"                 # the provider refused the input (for example, it is not signed in)


def run(out: Path, turns: int, prompt: str, provider_bin: str | None) -> dict:
    if not 1 <= turns <= MAX_TURNS:
        raise SystemExit(f"--turns must be between 1 and {MAX_TURNS}")
    binary = str(Path(provider_bin).resolve()) if provider_bin else shutil.which("codex")   # the provider runs in a private directory
    if not binary:
        raise SystemExit("the codex program was not found on PATH (use --provider-bin for another one)")
    out.mkdir(parents=True, exist_ok=True)
    result: dict = {"provider_program": Path(binary).name, "turns_requested": turns, "simulated": bool(provider_bin),
                    "approvals_denied": 0, "turns": []}
    with private_dir() as directory:
        from .p02 import start

        program = ["--codex", binary]
        if binary.endswith(".py"):                  # a protocol simulator for a free dry run
            program = ["--codex", sys.executable, "--codex-arg", "-I", "--codex-arg", binary]
        host, log = start(directory, extra=program, sessions=0, producing=0, history=0)
        try:
            conn = ProtoConn(host.socket, timeout=30.0)
            made = conn.call(op="session.create", kind="codex")
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
            return _finish(out, result, log, sid)
        finally:
            host.stop()


def _finish(out: Path, result: dict, log: Path, sid: str | None) -> dict:
    records = [p.decode(errors="replace") for _, p in read_log(log, sid)] if sid else []
    (out / "transcript.txt").write_text("\n".join(records) + "\n", encoding="utf-8")
    os.chmod(out / "transcript.txt", 0o600)       # conversation content: owner only
    result["records"] = len(records)
    (out / "result.json").write_text(json.dumps(result, indent=1, ensure_ascii=False))
    return result


def main(args) -> int:
    if not args.accept_billing:
        print("A real turn can be billed to your account. Nothing was run; pass --accept-billing to run it.", file=sys.stderr)
        return 2
    result = run(Path(args.out), args.turns, args.prompt, args.provider_bin)
    print(json.dumps({k: v for k, v in result.items() if k != "turns"} | {"finished": [t.get("finished") for t in result["turns"]]}, ensure_ascii=False))
    return 0 if result["turns"] and all(t.get("finished") == "[turn completed]" for t in result["turns"]) else 1
