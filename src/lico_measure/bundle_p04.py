"""The M01-P04 inspection bundle: credentials in the keyring only, accounts, and the second provider.

Every provider result shown here comes from a protocol simulator (a test double) and every keyring from a
throwaway private one with canary values; no real account, login, keyring or provider turn was used.
"""

from __future__ import annotations

import html
import json
import time
from pathlib import Path

from .bundle import CRITERIA, LABELS, _cell, _find


def _pre(value) -> str:
    return "<pre>" + html.escape(json.dumps(value, indent=1, ensure_ascii=False)) + "</pre>"


def _table(measurements: list[dict]) -> str:
    return "".join(f"<tr><td>{html.escape(m['metric'])}</td><td>{html.escape(m['scope']['workload'])}</td><td>{html.escape(m['scope']['process_scope'])}</td>"
                   f"<td>{m['value']:.2f} {html.escape(m['unit'])}</td><td>{html.escape(m['statistic'])}, n={m['n']}</td></tr>" for m in measurements)


def render(raw: dict, p02: dict, p03: dict, p04: dict, receipts: dict, bundle_commit: str) -> str:
    names = [c for c in ("a2", "c2") if c in raw["candidates"]]
    head = "".join(f"<th>{html.escape(LABELS[c])}</th>" for c in names)
    rows = []
    for spec, metric, label in CRITERIA:
        records = [_find(raw, c, metric) for c in names]
        if not any(records):
            continue
        target = next((r["target"] for r in records if r and "target" in r), None)
        limit = f"{target['comparison']} {target['limit']}" if target else "no target"
        rows.append(f"<tr><td>{spec}</td><td>{html.escape(label)}</td><td>{limit}</td>{''.join(_cell(r) for r in records)}</tr>")
    commits = ", ".join(f"{name} <code>{html.escape(r['commit'])}</code>" for name, r in receipts.items())
    transcripts = p04["transcripts"]
    store = "".join(f"<h4>{html.escape(state)}</h4>{_pre(steps)}" for state, steps in transcripts["credential_store"].items())
    return f"""<!doctype html><meta charset='utf-8'><title>M01-P04 inspection</title>
<style>body{{font:14px sans-serif;max-width:1300px;margin:2em auto}}table{{border-collapse:collapse;width:100%}}
td,th{{border:1px solid #bbb;padding:3px 6px;vertical-align:top;font-size:12px}}.met{{background:#e6f4ea}}.missed{{background:#fce8e6}}
.unverified{{background:#fff4d6}}pre{{background:#f4f4f4;padding:8px;overflow:auto;font-size:12px}}</style>
<h1>M01-P04 accounts, credential store and second provider - inspection bundle</h1>
<p>Verdict: <b>pending</b>. Targets are copied from the product specification. Runs: {commits}; bundle <code>{html.escape(bundle_commit)}</code>.
Every provider result is from a protocol <b>simulator</b> (a test double); every keyring is a <b>throwaway private</b> one with canary values.
No real account, login, keyring or provider turn was used. Xvfb (X11, no compositor, software rendering); Wayland, GPU, other input methods,
other Secret Service implementations than gnome-keyring 46.1, and accessibility are unverified.</p>
<h2>GUI targets with agent-shaped UTF-8 output (10000-line transcripts); the host holds 10 accounts, no credential operation runs while a client is measured</h2>
<table><tr><th>ID</th><th>Measurement</th><th>Target</th>{head}</tr>{''.join(rows)}</table>
<h2>Host memory with agent-shaped transcripts</h2>
<table><tr><th>Metric</th><th>Workload</th><th>Scope</th><th>Value</th><th>Samples</th></tr>{_table(p02['host_resources']['measurements'])}</table>
<h2>Host memory with accounts and simulated provider sessions</h2>
<table><tr><th>Metric</th><th>Workload</th><th>Scope</th><th>Value</th><th>Samples</th></tr>{_table(p04['accounts_host_memory']['measurements'])}</table>
<h2>Credential store cost (hand-written Secret Service client)</h2>
<table><tr><th>Metric</th><th>Workload</th><th>Scope</th><th>Value</th><th>Samples</th></tr>{_table(p04['credential_store_costs']['measurements'])}</table>
{_pre(p04['credential_store_costs']['observations'])}
<h2>A canary key in every non-secret sink</h2>{_pre(p04['secret_sink_scan']['observations'])}
<h2>Input and approval integrity through the adapters (simulators)</h2>
<h3>Claude Code (this phase)</h3>{_pre(p04['claude_integrity']['observations'])}
<h3>Codex (M01-P03 run, same scenario)</h3>{_pre(p03['adapter_integrity']['observations'])}
<h2>Hostile provider messages, runtime reuse (test outcomes)</h2>{_pre(p04['hostile_adapters'])}
<h2>The credential store when the keyring is not ready</h2>{store}
<h2>An account failure: shown against the account, nothing switches</h2>{_pre(transcripts['account_failure'])}
<h2>Removing an account: sessions named first, an explicit choice, separate outcomes</h2>{_pre(transcripts['removal'])}
<h2>Provider evidence: pinned versions, unsupported and unverified lists</h2>{_pre(p04['capability_records'])}
<h2>The owner's real-turn command, dry runs against the simulators</h2>{_pre(p04['real_turn_dry_runs'])}
<h2>Real provider turns</h2><p>None was run by the development session. If the owner runs one, its transcript stays in the owner's own output
directory; nothing is added here.</p>
"""


README = """# M01-P04 accounts, credential store and second provider - inspection bundle

Verdict: pending

Generated by `python -m lico_measure p04-bundle`; `receipt.json` lists the command, the exact commits, the
binary hashes and the workload. Open `index.html`.

## What to look at

- The GUI targets and host memory with agent-shaped output; the host holds 10 accounts (no credential operation runs during a client measurement: the credential command is started on demand and ends).
- The credential store: its cost, and what it prints when the keyring is ready, locked, without a default keyring,
  without a Secret Service and without a session bus (it never stores a secret anywhere else).
- A canary key searched for in every non-secret sink, after the scanner is shown to find a planted one.
- Two accounts and provider sessions bound to them: one failure shown against its account, a removal that names
  its sessions first and reports the local removal, the key and the (not performed) remote revocation apart.
- Input and approval integrity for both providers, hostile provider messages and runtime reuse.
- The pinned provider versions with what is unsupported and what is unverified.

## Looks right when

- The correct account and session stay identifiable; nothing switches account or authentication mode on its own.
- No secret appears anywhere but the keyring; a locked or missing keyring stops the operation with an action.
- Removing an account leaves other accounts, histories and running work alone.
- Nothing from a simulator is presented as a result of a real provider; unverified items are marked.
- No value is shown as zero when it was not measured.
"""


def build(raw_dirs: list[Path], p02: dict, p03: dict, p04: dict, out: Path, command: str, commit: str, receipts: dict) -> None:
    raw: dict = {"candidates": {}}
    for raw_dir in raw_dirs:
        for candidate, experiments in json.loads((raw_dir / "raw.json").read_text())["candidates"].items():
            raw["candidates"].setdefault(candidate, {}).update(experiments)
    out.mkdir(parents=True, exist_ok=True)
    (out / "index.html").write_text(render(raw, p02, p03, p04, receipts, commit))
    (out / "report.json").write_text(json.dumps({"p02_text": p02, "p03": p03, "p04": p04, "gui_experiments": raw["candidates"]}, indent=1, ensure_ascii=False))
    (out / "README.md").write_text(README)
    (out / "receipt.json").write_text(json.dumps({"bundle_command": command, "bundle_commit": commit, "measurement_receipts": receipts,
                                                  "generated": time.strftime("%Y-%m-%dT%H:%M:%S%z"), "verdict": "pending"}, indent=1))
