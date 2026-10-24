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
from .real_turn import DEFAULT_PROMPT as DEFAULT_REAL_PROMPT
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


TEXT_WORKLOAD = {"payload": 110, "history": 10000,
                 "shape": "UTF-8 lines of mixed scripts (Latin, Korean, ideographs, Hebrew, emoji, a combining accent, code), about 110 bytes"}


def _text_workload(args: argparse.Namespace) -> dict | None:
    """With --text, synthetic sessions produce agent-shaped UTF-8 lines and long transcripts."""
    from . import rig

    rig.TEXT_WORKLOAD = TEXT_WORKLOAD if args.text else None
    return rig.TEXT_WORKLOAD


def cmd_run(args: argparse.Namespace) -> int:
    dirty = _git("status", "--porcelain")
    if dirty and not args.allow_dirty:
        print("refusing to measure a dirty tree (use --allow-dirty to record it)", file=sys.stderr)
        return 2
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    text = _text_workload(args)
    names = args.only.split(",") if args.only else list(EXPERIMENTS)
    results: dict = {"candidates": {}}
    receipt = {
        "command": "python -m lico_measure " + " ".join(sys.argv[1:]), "commit": _git("rev-parse", "HEAD"),
        "tree_dirty": bool(dirty), "started": time.strftime("%Y-%m-%dT%H:%M:%S%z"), "experiments": names,
        "candidates": args.candidates.split(","), "environment": environment(),
        "binary_sha256": _binary_hashes(args.candidates.split(",")),
        "workload": {"record_bytes": (text or {}).get("payload", 1024), "rate_per_second": 100, "stream_duration_s": 30,
                     "viewport": "1200x800", "switches": 100, "restarts": 100, "idle_interval_s": 60,
                     "text_workload": text},
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


def cmd_p02(args: argparse.Namespace) -> int:
    """Run every M01-P02 scenario once; the receipt is written first."""
    from .bundle_p02 import collect_p02

    dirty = _git("status", "--porcelain")
    if dirty and not args.allow_dirty:
        print("refusing to measure a dirty tree (use --allow-dirty to record it)", file=sys.stderr)
        return 2
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    receipt = {"command": "python -m lico_measure " + " ".join(sys.argv[1:]), "commit": _git("rev-parse", "HEAD"),
               "tree_dirty": bool(dirty), "started": time.strftime("%Y-%m-%dT%H:%M:%S%z"), "environment": environment(),
               "binary_sha256": _p02_hashes(), "text_workload": _text_workload(args)}
    (out / "receipt.json").write_text(json.dumps(receipt, indent=1))
    results = collect_p02(out)
    (out / "p02.json").write_text(json.dumps(results, indent=1, ensure_ascii=False))
    receipt["finished"] = time.strftime("%Y-%m-%dT%H:%M:%S%z")
    (out / "receipt.json").write_text(json.dumps(receipt, indent=1))
    return 0


def cmd_p03(args: argparse.Namespace) -> int:
    """The adapter against the simulator and the provider evidence; the receipt is written first."""
    from .p03 import collect_p03

    dirty = _git("status", "--porcelain")
    if dirty and not args.allow_dirty:
        print("refusing to measure a dirty tree (use --allow-dirty to record it)", file=sys.stderr)
        return 2
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    receipt = {"command": "python -m lico_measure " + " ".join(sys.argv[1:]), "commit": _git("rev-parse", "HEAD"),
               "tree_dirty": bool(dirty), "started": time.strftime("%Y-%m-%dT%H:%M:%S%z"), "environment": environment(),
               "binary_sha256": _p02_hashes(), "provider": "scenarios: protocol simulator (a test double); provider_memory: the installed codex program, version and hash recorded in p03.json"}
    (out / "receipt.json").write_text(json.dumps(receipt, indent=1))
    (out / "p03.json").write_text(json.dumps(collect_p03(), indent=1, ensure_ascii=False))
    receipt["finished"] = time.strftime("%Y-%m-%dT%H:%M:%S%z")
    (out / "receipt.json").write_text(json.dumps(receipt, indent=1))
    return 0


def cmd_real_turn(args: argparse.Namespace) -> int:
    from . import real_turn

    return real_turn.main(args)


def _p02_hashes() -> dict:
    import hashlib

    from . import p02

    hashes = {}
    for name, path in (("lico-host", p02.HOSTD), ("lico", p02.LICO), ("lico-synth-agent", p02.AGENT)):
        hashes[name] = hashlib.sha256(path.read_bytes()).hexdigest()
    hashes.update(_binary_hashes(["a2", "c2"]))
    return hashes


def cmd_p02_bundle(args: argparse.Namespace) -> int:
    from .bundle_p02 import build

    dirty = _git("status", "--porcelain")
    if dirty and not args.allow_dirty:
        print("refusing to build a bundle from a dirty tree (use --allow-dirty to record it)", file=sys.stderr)
        return 2
    run = Path(args.p02)
    results = json.loads((run / "p02.json").read_text())
    raw_dirs = [Path(p) for p in args.raw]
    receipts = [json.loads((d / "receipt.json").read_text()) for d in [run, *raw_dirs]]
    build(raw_dirs, results, Path(args.out), "python -m lico_measure " + " ".join(sys.argv[1:]), _git("rev-parse", "HEAD"), receipts)
    import shutil

    if (run / "screens").is_dir():
        shutil.copytree(run / "screens", Path(args.out) / "screens", dirs_exist_ok=True)
    return 0


def cmd_p03_bundle(args: argparse.Namespace) -> int:
    from .bundle_p03 import build, screens

    dirty = _git("status", "--porcelain")
    if dirty and not args.allow_dirty:
        print("refusing to build a bundle from a dirty tree (use --allow-dirty to record it)", file=sys.stderr)
        return 2
    runs = [Path(args.p03), Path(args.p02), *[Path(p) for p in args.raw]]
    receipts = [json.loads((d / "receipt.json").read_text()) for d in runs]
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    shots = screens(out / "screens")
    build([Path(p) for p in args.raw], json.loads((Path(args.p02) / "p02.json").read_text()),
          json.loads((Path(args.p03) / "p03.json").read_text()), shots, out,
          "python -m lico_measure " + " ".join(sys.argv[1:]), _git("rev-parse", "HEAD"), receipts)
    return 0


def cmd_providers(args: argparse.Namespace) -> int:
    from .providers import record_codex

    cap = record_codex()
    print(json.dumps({"version": cap["version"]["cli"], "server_requests": len(cap["server_requests"])}))
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(prog="lico_measure", description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    run = sub.add_parser("run", help="run experiments and write raw samples")
    run.add_argument("--out", required=True)
    run.add_argument("--candidates", default="a,b")
    run.add_argument("--only", default="")
    run.add_argument("--allow-dirty", action="store_true")
    run.add_argument("--text", action="store_true", help="agent-shaped UTF-8 output and 10000-line transcripts")
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
    p02 = sub.add_parser("p02", help="run the M01-P02 scenarios and experiments")
    p02.add_argument("--out", required=True)
    p02.add_argument("--allow-dirty", action="store_true")
    p02.add_argument("--text", action="store_true", help="agent-shaped UTF-8 output and 10000-line transcripts")
    p02.set_defaults(func=cmd_p02)
    p03 = sub.add_parser("p03", help="run the M01-P03 adapter scenarios against the simulator")
    p03.add_argument("--out", required=True)
    p03.add_argument("--allow-dirty", action="store_true")
    p03.set_defaults(func=cmd_p03)
    real = sub.add_parser("real-turn", help="a real provider turn, run by the owner (can be billed)")
    real.add_argument("--out", required=True)
    real.add_argument("--turns", type=int, default=1)
    real.add_argument("--prompt", default=DEFAULT_REAL_PROMPT)
    real.add_argument("--provider-bin", default=None, help="another program speaking the same protocol (a free dry run)")
    real.add_argument("--accept-billing", action="store_true")
    real.add_argument("--provider", choices=["codex", "claude"], default="codex")
    real.add_argument("--allow-unpinned-version", action="store_true", help="run although the installed version is not the one the adapter is pinned to")
    real.set_defaults(func=cmd_real_turn)
    p02b = sub.add_parser("p02-bundle", help="generate the M01-P02 inspection bundle")
    p02b.add_argument("--p02", required=True, help="output directory of the p02 command")
    p02b.add_argument("--raw", required=True, nargs="+", help="raw output directories of the run command")
    p02b.add_argument("--out", required=True)
    p02b.add_argument("--allow-dirty", action="store_true")
    p02b.set_defaults(func=cmd_p02_bundle)
    p03b = sub.add_parser("p03-bundle", help="generate the M01-P03 inspection bundle")
    p03b.add_argument("--p03", required=True, help="output directory of the p03 command")
    p03b.add_argument("--p02", required=True, help="output directory of the p02 --text command")
    p03b.add_argument("--raw", required=True, nargs="+", help="raw output directories of the run --text command")
    p03b.add_argument("--out", required=True)
    p03b.add_argument("--allow-dirty", action="store_true")
    p03b.set_defaults(func=cmd_p03_bundle)
    prov = sub.add_parser("providers", help="record the capability data of an installed provider")
    prov.add_argument("provider", choices=["codex"])
    prov.set_defaults(func=cmd_providers)
    args = parser.parse_args()
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
