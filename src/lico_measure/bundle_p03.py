"""The M01-P03 inspection bundle: what an owner sees of the first real agent path.

Everything shown from a provider here comes from the protocol simulator, a test double. A real
turn appears only if the owner ran it and provides its transcript, kept visibly apart.
"""

from __future__ import annotations

import html
import json
import os
import subprocess
import time
from pathlib import Path

from .bundle import CRITERIA, LABELS, _cell, _find, _shot
from .ime import InputMethod, _focus
from .p02 import ProtoConn
from .p03 import _simulated_host
from .rig import Client, Xvfb, private_dir

MIXED = "hello é é 漢字 שלום 🙂 "


def _pre(value) -> str:
    return "<pre>" + html.escape(json.dumps(value, indent=1, ensure_ascii=False)) + "</pre>"


def _wait_text(client: Client, text: str, seconds: float = 10.0) -> None:
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        client.send("lines")
        if any(text in line for line in client.expect("lines")["lines"]):
            return
        time.sleep(0.3)
    raise TimeoutError(f"{text!r} was not shown")


def screens(out: Path) -> dict:
    """For each client: an agent turn full of escape sequences shown harmless, then mixed scripts and
    Korean typed through the real input method and shown as the agent's echo."""
    out.mkdir(parents=True, exist_ok=True)
    shots: dict[str, dict[str, str]] = {}
    for name in ("a2", "c2"):
        shots[name] = {}
        with private_dir() as directory:
            xvfb = Xvfb()
            host = method = client = None
            try:
                host, _log, _received = _simulated_host(directory)
                conn = ProtoConn(host.socket)
                made = conn.call(op="session.create", kind="codex")
                sid, generation = made["session"], made["generation"]
                method = InputMethod(xvfb.display)
                client = Client(name, host.socket, xvfb.display, extra_env=method.client_env)
                client.send(f"show {sid}")
                client.expect("state", "subscribed")
                conn.call(op="input", session=sid, generation=generation, request_id="e-1", text="escape")
                _wait_text(client, "visible after escapes")
                time.sleep(0.5)
                file = f"escape_{name}.png"
                _shot(xvfb.display, out / file)
                shots[name]["1_escape_sequences_shown_harmless"] = file
                _focus(xvfb.display)
                time.sleep(0.6)
                env = dict(os.environ, DISPLAY=xvfb.display)
                subprocess.run(["xdotool", "type", "--delay", "40", MIXED], env=env, check=True)
                time.sleep(0.4)
                subprocess.run(["xdotool", "key", "shift+space"], env=env, check=True)
                time.sleep(0.4)
                subprocess.run(["xdotool", "key", "--delay", "60", "d", "k", "s", "s", "u", "d", "g", "k", "t", "p", "d", "y", "space"], env=env, check=True)
                time.sleep(0.5)
                file = f"typed_{name}.png"
                _shot(xvfb.display, out / file)
                shots[name]["2_typed_before_sending"] = file
                subprocess.run(["xdotool", "key", "Return"], env=env, check=True)
                _wait_text(client, "echo: " + MIXED + "안녕하세요")
                time.sleep(0.5)
                file = f"echoed_{name}.png"
                _shot(xvfb.display, out / file)
                shots[name]["3_agent_echo_shown"] = file
            finally:
                if client and client.proc.poll() is None:
                    client.kill()
                if method:
                    method.stop()
                if host:
                    host.stop()
                xvfb.stop()
    return shots


def render(raw: dict, p02: dict, p03: dict, shots: dict, receipt: dict) -> str:
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

    def table(measurements: list[dict]) -> str:
        return "".join(f"<tr><td>{html.escape(m['metric'])}</td><td>{html.escape(m['scope']['workload'])}</td>"
                       f"<td>{m['value']:.1f} {html.escape(m['unit'])}</td><td>n={m['n']}</td></tr>" for m in measurements)

    figures = []
    for name, named in shots.items():
        figures.append(f"<h3>{html.escape(LABELS[name])}</h3><div class='shots'>")
        for label, file in named.items():
            figures.append(f"<figure><img src='screens/{html.escape(file)}' width='380'><figcaption>{html.escape(label)}</figcaption></figure>")
        figures.append("</div>")
    demo = p03["simulated_session"]
    return f"""<!doctype html><meta charset='utf-8'><title>M01-P03 inspection</title>
<style>body{{font:14px sans-serif;max-width:1300px;margin:2em auto}}table{{border-collapse:collapse;width:100%}}
td,th{{border:1px solid #bbb;padding:3px 6px;vertical-align:top;font-size:12px}}.met{{background:#e6f4ea}}.missed{{background:#fce8e6}}
.unverified{{background:#fff4d6}}.shots{{display:flex;gap:8px;flex-wrap:wrap}}pre{{background:#f4f4f4;padding:8px;overflow:auto;font-size:12px}}</style>
<h1>M01-P03 first real agent - inspection bundle</h1>
<p>Verdict: <b>pending</b>. Targets are copied from the product specification. Commit: <code>{html.escape(receipt['commit'])}</code>.
Every provider result below comes from the protocol <b>simulator</b> (a test double), not from the real provider.
Xvfb (X11, no compositor, software rendering); Wayland, a real compositor, GPU, other input methods and accessibility are unverified.</p>
<h2>GUI targets with agent-shaped UTF-8 output (10000-line transcripts)</h2>
<table><tr><th>ID</th><th>Measurement</th><th>Target</th>{head}</tr>{''.join(rows)}</table>
<h2>Host memory with agent-shaped transcripts</h2>
<table><tr><th>Metric</th><th>Workload</th><th>Value</th><th>Samples</th></tr>{table(p02['host_resources']['measurements'])}</table>
<h2>The provider process itself (real program, not signed in, idle; kept apart from Lico's budget)</h2>
<table><tr><th>Metric</th><th>Workload</th><th>Value</th><th>Samples</th></tr>{table(p03['provider_memory']['measurements'])}</table>{_pre(p03['provider_memory']['observations'])}
<h2>Escape sequences, mixed scripts and Korean on screen</h2>{''.join(figures)}
<h2>One simulated session end to end (input, turn, approvals, a vanished provider, resume)</h2>{_pre(demo)}
<h2>Input and approval integrity through the adapter (simulator)</h2>{_pre(p03['adapter_integrity']['observations'])}
<h2>GUI killed while a turn runs, view switched mid-composition</h2>{_pre(p02['gui_kill'])}{_pre(p02['view_switch'])}
<h2>Hostile provider messages</h2>{_pre(p03['hostile_adapter'])}
<h2>Provider evidence: pinned version, unsupported and unverified lists</h2>{_pre(p03['provider_evidence'])}
<h2>The owner's real-turn command, dry run against the simulator</h2>{_pre(p03['real_turn_dry_run'])}
<h2>Real provider turn</h2><p>Not run by the development session. If the owner runs it, the transcript is kept in
<code>owner_real_turn/</code> and nowhere else; this section stays empty until then.</p>
"""


README = """# M01-P03 first real agent - inspection bundle

Verdict: pending

Generated by `python -m lico_measure p03-bundle`; `receipt.json` lists the command, the exact
commit, the binary hashes and the workload. Open `index.html`.

## What to look at

- The GUI targets with agent-shaped UTF-8 output and 10000-line transcripts, both clients.
- Host memory with those transcripts, and the real provider program's own idle memory, apart.
- Escape-sequence attempts, mixed scripts and Korean typed through the real input method, on screen.
- One simulated session end to end: input, turn, approvals (forged, stale, repeated), a
  vanished provider, resume under a new generation.
- Input and approval integrity through the adapter, and the hostile provider messages.
- The pinned provider version with what is unsupported and what is unverified.

## Looks right when

- The correct session stays identifiable, hidden work continues, no input or approval lands in
  a stale target, and nothing typed in one session appears in another.
- Escape sequences from the agent have no effect on the screen, clipboard or window title.
- Every Korean and mixed-script character typed arrives, in order, exactly once.
- Nothing from the simulator is presented as a result of the real provider.
- No value is shown as zero when it was not measured. A screenshot alone proves nothing.
"""


def build(raw_dirs: list[Path], p02: dict, p03: dict, shots: dict, screens_dir: Path, out: Path, command: str, commit: str,
          receipts: list[dict]) -> None:
    import shutil

    raw: dict = {"candidates": {}}
    for raw_dir in raw_dirs:
        for candidate, experiments in json.loads((raw_dir / "raw.json").read_text())["candidates"].items():
            raw["candidates"].setdefault(candidate, {}).update(experiments)
    out.mkdir(parents=True, exist_ok=True)
    shutil.copytree(screens_dir, out / "screens", dirs_exist_ok=True)
    (out / "index.html").write_text(render(raw, p02, p03, shots, receipts[-1]))
    (out / "report.json").write_text(json.dumps({"p02_text": p02, "p03": p03, "gui_experiments": raw["candidates"]}, indent=1, ensure_ascii=False))
    (out / "README.md").write_text(README)
    (out / "receipt.json").write_text(json.dumps({
        "bundle_command": command, "bundle_commit": commit, "measurement_receipts": receipts,
        "generated": time.strftime("%Y-%m-%dT%H:%M:%S%z"), "verdict": "pending"}, indent=1))
