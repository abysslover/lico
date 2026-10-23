"""Command line: run the M01-P01 experiments and generate the inspection bundle."""

from __future__ import annotations

import argparse
import json
import os
import platform
import subprocess
import sys
import time
from pathlib import Path

from .experiments import EXPERIMENTS
from .report import build_report
from .rig import REPO


def _capture(*cmd: str) -> str:
    result = subprocess.run(cmd, capture_output=True, text=True)
    return (result.stdout or result.stderr).strip().splitlines()[0] if (result.stdout or result.stderr) else ""


def _git(*args: str) -> str:
    return subprocess.run(["git", *args], cwd=REPO, capture_output=True, text=True).stdout.strip()


def environment() -> dict:
    return {
        "kernel": platform.release(), "machine": platform.machine(), "python": platform.python_version(),
        "gtk": _capture("pkg-config", "--modversion", "gtk+-3.0"), "rustc": _capture("rustc", "--version"),
        "display": "Xvfb, X11, no compositor, software rendering", "cpu_logical": os.cpu_count(),
        "memory_cap": "pri-heavy per-job cap (declared in receipt)",
    }


def cmd_run(args: argparse.Namespace) -> int:
    dirty = _git("status", "--porcelain")
    if dirty and not args.allow_dirty:
        print("refusing to measure a dirty tree (use --allow-dirty to record it)", file=sys.stderr)
        return 2
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    names = args.only.split(",") if args.only else list(EXPERIMENTS)
    results: dict = {"candidates": {}}
    receipt = {
        "command": "python -m lico_measure " + " ".join(sys.argv[1:]), "commit": _git("rev-parse", "HEAD"),
        "tree_dirty": bool(dirty), "started": time.strftime("%Y-%m-%dT%H:%M:%S%z"), "experiments": names,
        "candidates": args.candidates.split(","), "environment": environment(),
        "workload": {"record_bytes": 1024, "rate_per_second": 100, "stream_duration_s": 30,
                     "viewport": "1200x800", "switches": 100, "restarts": 100, "idle_interval_s": 60},
    }
    for candidate in receipt["candidates"]:
        results["candidates"][candidate] = {}
        for name in names:
            began = time.monotonic()
            print(f"[{candidate}] {name} ...", flush=True)
            results["candidates"][candidate][name] = EXPERIMENTS[name](candidate)
            print(f"[{candidate}] {name} done in {time.monotonic() - began:.0f}s", flush=True)
            (out / "raw.json").write_text(json.dumps(
                build_report([], receipt["environment"]) | results, indent=1))
    receipt["finished"] = time.strftime("%Y-%m-%dT%H:%M:%S%z")
    (out / "receipt.json").write_text(json.dumps(receipt, indent=1))
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(prog="lico_measure", description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    run = sub.add_parser("run", help="run experiments and write raw samples")
    run.add_argument("--out", required=True)
    run.add_argument("--candidates", default="a,b")
    run.add_argument("--only", default="")
    run.add_argument("--allow-dirty", action="store_true")
    run.set_defaults(func=cmd_run)
    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
