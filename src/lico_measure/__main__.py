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

from .experiments import EXPERIMENTS, Rig
from .report import build_report
from .rig import REPO


ONLY_FOR = {"glyph_stress": {"a", "a2"}, "korean_ime": {"a", "a2", "b", "c2"}}


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
    }


def _binary_hashes(candidates: list[str]) -> dict:
    import hashlib

    from .rig import candidate_command

    out = {}
    for name in candidates:
        with open(candidate_command(name, "x")[0], "rb") as handle:
            out[name] = hashlib.sha256(handle.read()).hexdigest()
    return out


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
        "binary_sha256": _binary_hashes(args.candidates.split(",")),
        "workload": {"record_bytes": 1024, "rate_per_second": 100, "stream_duration_s": 30,
                     "viewport": "1200x800", "switches": 100, "restarts": 100, "idle_interval_s": 60},
    }
    (out / "receipt.json").write_text(json.dumps(receipt, indent=1))  # kept even if a run aborts
    for candidate in receipt["candidates"]:
        results["candidates"][candidate] = {}
        print(f"[{candidate}] warm-up (fills the private font caches, not measured)", flush=True)
        with Rig(candidate, sessions=1, producing=0, history=100) as warm:
            warm.show(warm.host.sessions[0])
        for name in names:
            if candidate not in ONLY_FOR.get(name, {candidate}):
                continue  # the stress command and the input line exist only in some clients
            began = time.monotonic()
            print(f"[{candidate}] {name} ...", flush=True)
            results["candidates"][candidate][name] = EXPERIMENTS[name](candidate)
            print(f"[{candidate}] {name} done in {time.monotonic() - began:.0f}s", flush=True)
            (out / "raw.json").write_text(json.dumps(
                build_report([], receipt["environment"]) | results, indent=1))
    receipt["finished"] = time.strftime("%Y-%m-%dT%H:%M:%S%z")
    (out / "receipt.json").write_text(json.dumps(receipt, indent=1))
    return 0


def cmd_bundle(args: argparse.Namespace) -> int:
    from .bundle import build_bundle

    dirty = _git("status", "--porcelain")
    if dirty and not args.allow_dirty:
        print("refusing to build a bundle from a dirty tree (use --allow-dirty to record it)", file=sys.stderr)
        return 2
    build_bundle([Path(p) for p in args.raw], Path(args.out), "python -m lico_measure " + " ".join(sys.argv[1:]),
                 _git("rev-parse", "HEAD"))
    return 0


def cmd_hostcmp(args: argparse.Namespace) -> int:
    from .hostcmp import compare

    dirty = _git("status", "--porcelain")
    if dirty and not args.allow_dirty:
        print("refusing to measure a dirty tree (use --allow-dirty to record it)", file=sys.stderr)
        return 2
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    results = compare()
    (out / "raw.json").write_text(json.dumps({"hosts": results, "commit": _git("rev-parse", "HEAD"),
                                               "tree_dirty": bool(dirty), "environment": environment()}, indent=1))
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
    bundle = sub.add_parser("bundle", help="generate the inspection bundle from raw output")
    bundle.add_argument("--raw", required=True, nargs="+", help="raw output directories, later ones win per candidate")
    bundle.add_argument("--out", required=True)
    bundle.add_argument("--allow-dirty", action="store_true")
    bundle.set_defaults(func=cmd_bundle)
    hostcmp = sub.add_parser("hostcmp", help="compare host implementations by size and memory")
    hostcmp.add_argument("--out", required=True)
    hostcmp.add_argument("--allow-dirty", action="store_true")
    hostcmp.set_defaults(func=cmd_hostcmp)
    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
