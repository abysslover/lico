# M01-P01 S01 — candidates, constraints and provider matrix

Checked: 2026-10-23. Scope: documentation survey and local version reads only.
No candidate was built, no resource was measured and no provider session was
started. Nothing here selects a renderer; it only fixes what S02 and S03 compare.

## Candidates to compare (at most two)

| ID | Candidate | Why it is feasible | Public build inputs | License |
|---|---|---|---|---|
| A | Owned minimal native client: Rust, `winit` window and input, `softbuffer` CPU pixel surface, `fontdue` glyph rasterizer | Smallest dependency surface that can show a text view on X11 and Wayland without a GPU stack | `winit` 0.30.13, `softbuffer` 0.4.8, `fontdue` 0.9.5 plus transitive crates, all resolved from the public crate registry and pinned by lockfile | winit Apache-2.0; softbuffer MIT OR Apache-2.0; fontdue MIT OR Apache-2.0 OR Zlib; transitive licenses are unverified until the lockfile exists |
| B | Existing toolkit: GTK 3 (`gtk+-3.0` 3.24.41, installed from the distribution), drawn with its own text widget | Mature toolkit with built-in IME and accessibility; no code download is needed | Distribution development package that provides `gtk+-3.0` 3.24.41, plain C | LGPL-2.1-or-later (dynamic linking) |

Both clients talk to one shared synthetic host written for this phase, so the
host is held constant and only the presentation side differs. The host is a
small Python program that uses only the standard library and an owner-only local
socket, so no further download is required. Its memory is reported separately.

Not selected, and not measured in this phase: Qt 5 Widgets (5.15.13 is
installed) and WebKitGTK-based shells (2.52.3 is installed). They are recorded as
alternatives only because two candidates is the plan's limit. No performance
claim is made about either. GTK 4, Qt 6 and libadwaita are not installed.

Differences that limit comparability, to be repeated in the report: A is Rust and
B is C; A draws its own pixels on the CPU while B draws through its toolkit; the
Xvfb server provides software rendering and no compositor.

## Facts read from official sources

| Item | Observation | Source |
|---|---|---|
| winit | 0.30.13, Apache-2.0, minimum Rust 1.70.0, released 2026-03-02 | crate registry entry |
| softbuffer | 0.4.8, MIT OR Apache-2.0, minimum Rust 1.71.0, released 2025-12-13; X11 (Xlib, XCB) and Wayland supported; CPU only, no GPU | crate registry entry and its API documentation |
| fontdue | 0.9.5, MIT OR Apache-2.0 OR Zlib, released 2026-10-19; a simple `no_std` font parser and rasterizer | crate registry entry |
| winit IME | The event model has Enabled, Preedit, Commit and Disabled; platform support for X11 and Wayland was **not** confirmed in this survey | winit API documentation (partial read) |
| GTK 3 | LGPL-2.1-or-later; the documentation lists X11 and Wayland backends | GTK 3 documentation |
| GTK 3 status | Maintenance status and successor were **not** confirmed | not read |
| Toolchain | Rust and Cargo 1.95.0, Python 3.12.3, GTK 3.24.41, WebKitGTK 2.52.3, Qt 5.15.13 read locally with the package-config tool | local read |

## Provider capability matrix (documentation only, no sign-in)

| Provider path | Transport and sessions | Authentication | Limits recorded |
|---|---|---|---|
| Codex app-server | Default stdio JSONL; JSON-RPC style `thread/start`, `thread/resume`, `turn/start`, `turn/steer`, `turn/interrupt`; events such as `item/agentMessage/delta`; approvals are server-initiated requests | Credential store `file`, `keyring` (fails if unavailable), `auto` (falls back to a file) or `ephemeral`; browser, device code (beta), API key and access token logins | WebSocket transport is "experimental and unsupported"; the command is not supported for production workloads; several thread and process methods need an experimental opt-in; multiple-account isolation guidance was not found on the auth page |
| Codex CLI installed | Version `codex-cli 0.163.0` read with a version flag only | Not exercised | App-server behavior of this exact version is unverified |
| Claude Code CLI installed | Version **unverified**: this session is not permitted to run it | Delegate sign-in to the official CLI | Do not extract subscription credentials or misrepresent client identity; third-party integrations should use API-key or supported cloud authentication |
| Anthropic API | Documented API key or supported cloud authentication | API key from the OS keyring in later phases | Subscription use by an independent client is a separate documented capability, not assumed |

The app-server documentation page was read only in part (about 100000 of 143632
characters); sign-in method detail and the rest of the approval flow there are
**unverified**. Everything above that is not stated by a source is unverified,
not zero.

## Scope check

The synthetic host and the clients add no network listener beyond an owner-only
local socket, no endpoint inventory beyond that socket and no stored user data.
The plan's personal-data controls therefore apply as written; this survey does
not trigger the plan re-check. If the socket grows beyond status, stream and
snapshot messages, the plan is rechecked with the owner first.

## Update after the first measurements

The owner directed that candidate B is not dropped on a single minimal-view
measurement, which widened the set beyond two. Measured in the end: A (as
above), A2 (A with cosmic-text 0.19.0, MIT OR Apache-2.0), B (GTK 3.24.41), C (Qt
5.15.13 Widgets as installed), D (WebKitGTK 2.52.3 as installed) and
environment-only variants of B and C. Qt and WebKitGTK licenses and
redistribution terms were not re-read and stay unverified. The results and the
decision are in the [phase report](../../../../reports/202610/20261023/20261023_01_M01-P01_architecture_measurement_report.md).
