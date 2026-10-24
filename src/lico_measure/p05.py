"""M01-P05 collection: the one executable's inventory and a restart-recovery run at scale (simulator provider, synthetic sessions)."""
from __future__ import annotations

import hashlib
import json
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path

from .p02 import HOSTD_DIR, ProtoConn, start
from .procscope import collect_scopes
from .rig import REPO, private_dir

EXE = HOSTD_DIR / "lico"
SESSIONS = 50
INPUTS = 20


def inventory() -> dict:
    """What the file is: size, hash, linked libraries, and what each role does when it sits alone in an empty directory."""
    libraries = sorted(line.split()[0] for line in subprocess.run(["ldd", str(EXE)], capture_output=True, text=True).stdout.splitlines())
    with private_dir() as directory:
        folder = Path(directory) / "only-this"
        folder.mkdir()
        exe = folder / "lico"
        shutil.copy2(EXE, exe)
        env = {"PATH": "/usr/bin:/bin", "LANG": "C.UTF-8", "HOME": directory, "XDG_RUNTIME_DIR": directory}
        begun = time.monotonic()
        version = subprocess.run([str(exe), "version"], env=env, capture_output=True, text=True).stdout.strip()
        version_ms = (time.monotonic() - begun) * 1000
        notices = subprocess.run([str(exe), "licenses"], env=env, capture_output=True, text=True).stdout
        return {"file": "lico", "bytes": EXE.stat().st_size, "sha256": hashlib.sha256(EXE.read_bytes()).hexdigest(),
                "linked_libraries": libraries, "files_next_to_it": sorted(p.name for p in folder.iterdir()),
                "version_output": version, "version_run_ms": round(version_ms, 1), "licenses_text_bytes": len(notices),
                "licenses_has_own_and_third_party": "Permission is hereby granted" in notices and "THIRD-PARTY NOTICES" in notices}


def recovery(sessions: int = SESSIONS, inputs: int = INPUTS) -> dict:
    """Sessions with history, the host killed without warning, restarted: what comes back, how fast, and at what memory."""
    from tests.test_codex_adapter import FIXTURE, SIM

    from .p02 import read_log

    out: dict = {"sessions": sessions, "inputs_each": inputs, "provider": "protocol simulator (a test double)"}
    with private_dir() as directory:
        received = str(Path(directory) / "received.jsonl")
        extra = ["--codex", sys.executable, "--codex-arg", "-I", "--codex-arg", str(SIM), "--codex-arg", "--received",
                 "--codex-arg", received, "--codex-arg", "--fixture", "--codex-arg", str(FIXTURE)]
        host, log = start(directory, extra=extra, sessions=0, producing=0, history=0)
        try:
            conn = ProtoConn(host.socket, timeout=30)
            made = [conn.call(op="session.create", kind="codex") for _ in range(sessions)]
            for m in made:
                for k in range(inputs):
                    conn.call(op="input", session=m["session"], generation=m["generation"], request_id=f"r{k}", text=f"line {k}")
            time.sleep(4.0)                                           # the log syncs each second; a crash may lose at most that
            before = {m["session"]: read_log(log, m["session"])[-1][0] for m in made}
        finally:
            host.proc.send_signal(signal.SIGKILL)
            host.proc.wait()
        began = time.monotonic()
        host, log = start(directory, extra=extra, sessions=0, producing=0, history=0)
        try:
            conn = ProtoConn(host.socket, timeout=30)
            found = {s["session"]: s for s in conn.call(op="status")["sessions"]}
            out["restart_to_ready_and_listing_ms"] = round((time.monotonic() - began) * 1000, 1)
            out["recovered"] = len(found)
            out["disconnected"] = sum(1 for s in found.values() if s["state"] == "disconnected")
            out["sequence_equals_durable_log"] = sum(1 for sid, seq in before.items() if found.get(sid, {}).get("seq") == seq)
            out["records_lost_beyond_last_sync"] = sum(max(0, seq - found.get(sid, {}).get("seq", 0)) for sid, seq in before.items())
            gaps = 0
            for sid in before:
                seqs = [s for s, _ in read_log(log, sid)]
                gaps += 0 if seqs == list(range(len(seqs))) else 1
            out["sessions_with_a_sequence_gap"] = gaps
            scope = collect_scopes({"host": host.pid})["host"]
            out["restarted_host_rss_kib"] = scope.rss_kb
            resumed = conn.call(op="session.resume", session=made[0]["session"], generation=made[0]["generation"])
            out["one_session_resumed"] = resumed.get("ok") == 1 and resumed.get("generation") == made[0]["generation"] + 1
        finally:
            host.stop()
    return out


def login_memory() -> dict:
    """The host's own memory before and during a sign-in (a test double for the provider's login command; the login process is counted apart)."""
    from tests.test_login import FAKE

    out: dict = {"provider": "test double of a login command (no real provider)"}
    with private_dir() as directory:
        extra = ["--login-bin", sys.executable, "--login-arg", "-I", "--login-arg", str(FAKE), "--login-arg", "hang"]
        host, log = start(directory, extra=extra, sessions=0, producing=0, history=0)
        try:
            conn = ProtoConn(host.socket, timeout=30)
            made = conn.call(op="account.create", provider="codex", label="Work")["account"]

            def host_kib() -> int:
                return next(p.rss_kb for p in collect_scopes({"host": host.pid})["host"].processes if p.pid == host.pid)

            out["host_idle_rss_kib"] = host_kib()
            conn.call(op="account.login.start", account=made)
            deadline = time.monotonic() + 10
            while conn.call(op="account.login.status", account=made).get("url") is None and time.monotonic() < deadline:
                time.sleep(0.1)
            out["host_during_login_rss_kib"] = host_kib()
            scope = collect_scopes({"host": host.pid})["host"]
            out["login_process_rss_kib"] = sum(p.rss_kb for p in scope.processes if p.pid != host.pid)
            conn.call(op="account.login.cancel", account=made)
        finally:
            host.stop()
    return out


def collect_p05() -> dict:
    return {"inventory": inventory(), "recovery": recovery(), "login_memory": login_memory(), "git_repository": str(REPO.name)}
