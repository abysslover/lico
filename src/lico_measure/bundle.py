"""Generate the local inspection bundle from raw measurement output.

The bundle shows both candidates at the same viewport, the visible, hidden and
reattached states, the observed values next to their targets, continuity
counters and the rejected hostile requests. Its verdict stays pending until it
has actually been observed.
"""

from __future__ import annotations

import html
import json
import subprocess
import time
from pathlib import Path

from .experiments import Rig
from .hostile import run_hostile_cases
from .report import SCHEMA
from .rig import REPO, Host, private_dir

CRITERIA = [
    ("PERF-01", "gui_idle_rss_mib", "GUI idle RSS (process tree), one visible session"),
    ("PERF-01", "gui_idle_pss_mib", "GUI idle PSS, reported separately"),
    ("PERF-02", "hidden_growth_rss_mib", "GUI RSS growth, 10 to 100 sessions, views released"),
    ("PERF-03", "activation_p95_ms", "Warm activation, first correct frame on the display"),
    ("PERF-04", "restart_p95_ms", "GUI restart, first correct frame"),
    ("PERF-05", "idle_cpu_percent", "Idle GUI CPU, 60 s"),
    ("PERF-05", "noisy_cpu_percent", "Noisy case: visible session producing 100 records/s"),
    ("PERF-07", "gui_peak_rss_mib", "Peak RSS (sum of per-process high-water marks)"),
    ("PERF-07", "host_idle_rss_mib", "Host process RSS, reported separately"),
    ("PERF-07", "display_server_idle_rss_mib", "Virtual display server RSS, reported separately"),
    ("PERF-07", "cold_start_ms", "Best-effort cold start (page-cache eviction)"),
    ("PERF-07", "activation_first_visit_ms", "First visit to a session (supplementary)"),
]


def _shot(display: str, path: Path) -> None:
    subprocess.run(["import", "-display", display, "-window", "root", str(path)], check=True)


def take_screenshots(out: Path) -> dict[str, dict[str, str]]:
    out.mkdir(parents=True, exist_ok=True)
    shots: dict[str, dict[str, str]] = {}
    for candidate in ("a", "b"):
        shots[candidate] = {}
        with Rig(candidate, sessions=3, producing=1, history=5000) as rig:
            first, second = rig.host.sessions[0], rig.host.sessions[1]

            def snap(label: str) -> None:
                name = f"candidate_{candidate}_{label}.png"
                _shot(rig.xvfb.display, out / name)
                shots[candidate][label] = name

            rig.show(first)
            time.sleep(1.5)
            snap("1_visible_active_output")
            rig.client.send("hide")
            rig.client.expect("state", "hidden")
            time.sleep(0.5)
            snap("2_hidden")
            rig.show(first)
            time.sleep(1.5)
            snap("3_reattached")
            rig.show(second)
            time.sleep(1.0)
            snap("4_switched_to_quiet_session")
    return shots


def _cell(record: dict | None) -> str:
    if record is None:
        return "<td>not run</td>"
    unit = html.escape(record["unit"])
    return (f"<td class='{record['classification']}'>{record['value']:.2f} {unit}"
            f"<br><small>{record['statistic']}, n={record['n']}: <b>{record['classification']}</b></small></td>")


def _find(raw: dict, candidate: str, metric: str) -> dict | None:
    for experiment in raw["candidates"].get(candidate, {}).values():
        for record in experiment["measurements"]:
            if record["metric"] == metric:
                return record
    return None


def _render(raw: dict, receipt: dict, shots: dict, hostile: list[dict]) -> str:
    rows = []
    for spec, metric, label in CRITERIA:
        a, b = _find(raw, "a", metric), _find(raw, "b", metric)
        target = next((r["target"] for r in (a, b) if r and "target" in r), None)
        limit = f"{target['comparison']} {target['limit']}" if target else "no target"
        rows.append(f"<tr><td>{spec}</td><td>{html.escape(label)}</td><td>{limit}</td>{_cell(a)}{_cell(b)}</tr>")
    shot_html = []
    for candidate, named in shots.items():
        shot_html.append(f"<h3>Candidate {candidate.upper()}</h3><div class='shots'>")
        for label, name in named.items():
            shot_html.append(f"<figure><img src='screens/{html.escape(name)}' width='400'>"
                             f"<figcaption>{html.escape(label)}</figcaption></figure>")
        shot_html.append("</div>")
    observations = {c: {n: e["observations"] for n, e in exps.items() if e["observations"]}
                    for c, exps in raw["candidates"].items()}
    hostile_rows = "".join(
        f"<tr><td>{html.escape(h['case'])}</td><td>{h['rejected']}</td><td>{h['no_effect']}</td>"
        f"<td>{html.escape(h['detail'])}</td></tr>" for h in hostile)
    return f"""<!doctype html><meta charset='utf-8'><title>M01-P01 inspection</title>
<style>body{{font:14px sans-serif;max-width:1100px;margin:2em auto}}table{{border-collapse:collapse;width:100%}}
td,th{{border:1px solid #bbb;padding:4px 8px;vertical-align:top}}.met{{background:#e6f4ea}}.missed{{background:#fce8e6}}
.unverified{{background:#fff4d6}}.shots{{display:flex;gap:8px;flex-wrap:wrap}}pre{{background:#f4f4f4;padding:8px;overflow:auto}}</style>
<h1>M01-P01 architecture measurement - inspection bundle</h1>
<p>Verdict: <b>pending</b>. Targets are copied from the product specification; they are not results.
Commit measured: <code>{html.escape(receipt['commit'])}</code>. Environment: Xvfb (X11, no compositor,
software rendering). Wayland, GPU allocation, package size and real IME composition are unverified.</p>
<h2>Observed values against targets</h2>
<table><tr><th>ID</th><th>Measurement</th><th>Target</th><th>Candidate A (winit + softbuffer)</th>
<th>Candidate B (GTK 3)</th></tr>{''.join(rows)}</table>
<h2>Screens at the same 1200x800 viewport</h2>{''.join(shot_html)}
<h2>Rejected hostile requests (attacker's view)</h2>
<table><tr><th>Case</th><th>Rejected</th><th>No effect</th><th>Detail</th></tr>{hostile_rows}</table>
<h2>Observations (continuity, inventory, load)</h2><pre>{html.escape(json.dumps(observations, indent=1))}</pre>
"""


README = """# M01-P01 architecture measurement - inspection bundle

Verdict: pending

This bundle was generated by `python -m lico_measure bundle`; `receipt.json` lists
the command, the exact commits and the workload. `report.json` holds every raw
sample. Open `index.html`.

## What to look at

- Both candidates at the same viewport: visible with active output, hidden,
  reattached, and switched to a quiet session (`screens/`).
- Observed values next to the proposed targets, with scope, sample count and
  classification (met, missed or unverified).
- Continuity counters and the rejected hostile requests.

## Looks right when

- The session colour block and the Korean title identify the session shown.
- The hidden state shows no session content, and the reattached state shows
  current content.
- No value is shown as zero when it was not measured.
- A screenshot alone does not prove IME input or latency. Real IME composition
  was not exercised (no input-method server in the virtual display) and is
  unverified.
"""


def build_bundle(raw_dirs: list[Path], out: Path, command: str, commit: str) -> None:
    """Later raw directories replace earlier ones per candidate (a changed candidate is re-measured)."""
    raw: dict = {"candidates": {}}
    receipts = []
    for raw_dir in raw_dirs:
        raw["candidates"].update(json.loads((raw_dir / "raw.json").read_text())["candidates"])
        receipts.append(json.loads((raw_dir / "receipt.json").read_text()))
    run_receipt = receipts[-1]
    out.mkdir(parents=True, exist_ok=True)
    shots = take_screenshots(out / "screens")
    with private_dir() as directory:
        host = Host(directory, sessions=2, producing=1)
        try:
            hostile = run_hostile_cases(host)
        finally:
            host.stop()
    records = [dict(r, candidate_key=c) for c, exps in raw["candidates"].items()
               for e in exps.values() for r in e["measurements"]]
    (out / "report.json").write_text(json.dumps(
        {"schema": SCHEMA, "environment": run_receipt["environment"], "measurements": records,
         "observations": {c: {n: e["observations"] for n, e in exps.items()}
                          for c, exps in raw["candidates"].items()}, "hostile": hostile}, indent=1))
    (out / "index.html").write_text(_render(raw, run_receipt, shots, hostile))
    (out / "README.md").write_text(README)
    (out / "receipt.json").write_text(json.dumps({
        "bundle_command": command, "bundle_commit": commit, "measurement_receipts": receipts,
        "generated": time.strftime("%Y-%m-%dT%H:%M:%S%z"), "verdict": "pending",
    }, indent=1))
