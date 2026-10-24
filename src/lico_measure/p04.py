"""M01-P04 scenarios and measurements: the credential store, accounts and the second provider.

Everything runs against a throwaway private Secret Service with canary values; no real
keyring, account, login or provider is used here.
"""

from __future__ import annotations

import json
import os
import subprocess
import time
from pathlib import Path

from .keyring_rig import PrivateKeyring
from .p02 import HOSTD_DIR
from .report import build_measurement
from .rig import private_dir

CRED = HOSTD_DIR / "lico-cred"
CANARY = "canary-key-measure-4d2e81b7"


def _run(env: dict, data: str, *args: str, stdin: str | None = None) -> tuple[subprocess.CompletedProcess, float, float]:
    """Run lico-cred; return the result and the wall time in ms (the third value is unused)."""
    began = time.monotonic()
    proc = subprocess.Popen([str(CRED), "--data-dir", data, *args], env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, text=True)
    out, err = proc.communicate(stdin)
    elapsed = (time.monotonic() - began) * 1000
    return subprocess.CompletedProcess(proc.args, proc.returncode, out, err), elapsed, 0.0


def _peak_rss_mib(env: dict, data: str, *args: str, stdin: str | None = None) -> float:
    """The peak resident size of one lico-cred process. GNU time forks from a tiny process, so the
    figure is not inflated by the size of this Python process (a child's peak includes its parent's
    image until it executes)."""
    with private_dir() as scratch:
        report = Path(scratch) / "peak"
        subprocess.run(["/usr/bin/time", "-f", "%M", "-o", str(report), str(CRED), "--data-dir", data, *args], env=env,
                       input=stdin or "", capture_output=True, text=True)
        return int(report.read_text().split()[-1]) / 1024.0


def credential_store_costs(reads: int = 100) -> dict:
    """What the hand-written Secret Service client costs, with the keyring unlocked, locked and absent."""
    measurements, observations = [], {"binary_bytes": CRED.stat().st_size}
    scope = {"candidate": "lico-cred (hand-written D-Bus client, no new dependency)", "process_scope": "one lico-cred process",
             "output": "canary secret in a throwaway keyring"}
    with PrivateKeyring() as keyring, private_dir() as data:
        added, _, _ = _run(keyring.env, data, "add", "--provider", "claude", "--account", "a-1", "--kind", "api_key", "--label", "key ending b7", stdin=CANARY + "\n")
        reference = json.loads(added.stdout)["ref"]
        reads_ms, adds_ms = [], []
        for k in range(reads):
            done, elapsed, _ = _run(keyring.env, data, "helper", "--ref", reference)
            assert done.returncode == 0 and done.stdout.strip() == CANARY
            reads_ms.append(elapsed)
        for k in range(20):
            done, elapsed, _ = _run(keyring.env, data, "add", "--provider", "claude", "--account", f"a-{k}", "--kind", "api_key", "--label", "key ending", stdin=CANARY + "\n")
            assert done.returncode == 0
            adds_ms.append(elapsed)
        peaks = [_peak_rss_mib(keyring.env, data, "helper", "--ref", reference) for _ in range(10)]
        measurements += [
            build_measurement("credential_read_ms", dict(scope, workload="helper read, keyring unlocked, one process per read"), reads_ms),
            build_measurement("credential_write_ms", dict(scope, workload="add, keyring unlocked"), adds_ms),
            build_measurement("credential_process_peak_rss_mib", dict(scope, workload="one helper read"), peaks),
        ]
        keyring.lock()
        closed_ms = []
        for _ in range(20):
            done, elapsed, _ = _run(keyring.env, data, "helper", "--ref", reference)
            assert done.returncode == 3 and done.stdout == ""
            closed_ms.append(elapsed)
        measurements.append(build_measurement("credential_fail_closed_ms", dict(scope, workload="read while the keyring is locked"), closed_ms))
    with PrivateKeyring(daemon=False) as keyring, private_dir() as data:
        env = {k: v for k, v in keyring.env.items() if k != "DBUS_SESSION_BUS_ADDRESS"}
        absent_ms = []
        for _ in range(20):
            done, elapsed, _ = _run(env, data, "status")
            assert done.returncode == 3
            absent_ms.append(elapsed)
        measurements.append(build_measurement("credential_fail_closed_ms", dict(scope, workload="status with no session bus"), absent_ms))
    return {"measurements": measurements, "observations": observations}
