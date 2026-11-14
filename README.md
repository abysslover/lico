# Lico

a lightweight workbench for agentic workflows

Lico (**Lightweight Interface for CLI Orchestration**) is a small native desktop
workbench for persistent command-line coding agents (`Claude Code` and `Codex`).
People organize local, `git worktree`, and SSH remote sessions, follow progress,
inspect tool and command activity, answer approvals, track live token and quota
usage, and coordinate multi-agent collaboration (`Call Rooms`, `@AgentName`
shared-context mentions, and cross-provider session creation) from one window.

## Purpose

Command-line agents already edit code, run tools, and carry out multi-step tasks,
but supervising several long-running sessions across folders, accounts, and
machines in raw terminals scatters context, approvals, and state. Lico provides a
fast supervisory workbench around installed provider CLIs without turning into a
full IDE, an embedded browser runtime, or an autonomous cloud scheduler.

## Design philosophy

- **Separate execution from presentation.** A background session host owns agent
  processes, output draining, and durable event logs independently of the window.
  Closing or hiding `lico gui` never stops running work; hidden tabs release UI
  resources and reattach from a host snapshot plus subsequent events.
- **Measured lightweight single executable.** Lico ships as one self-contained
  Rust binary (`lico`, `0.3.2`) with a hand-drawn native client rather than a
  bundled webview. Resource budgets are enforced by automated measurement: an
  idle window stays well under `50 MiB` RSS (`~25 MiB` window / `~36 MiB` client
  tree; `~7.6 MiB` host), scaling from `10` to `100` background sessions adds
  under `+1 MiB` (`<= +10 MiB` budget), and two clean clones build byte-for-byte
  identical release binaries.
- **Human GUI, programmatic CLI / MCP / HTTP.** `lico gui` gives the human
  administrator direct interactive control over sessions, Call Rooms, approvals,
  permission modes, accounts, quotas (`5h` and weekly `7d`), and on-demand
  local/SSH directory browsing, while scripts and agents use the exact same host
  contract through the `lico` CLI, loopback HTTP (`/v1`), and loopback MCP tools.
- **Official authentication and strict account isolation.** Lico uses official
  provider sign-in flows only and stores application-owned secrets exclusively in
  the OS keyring (failing closed when unavailable, with no plaintext fallback).
  Multiple accounts per provider keep separate credentials, private state homes,
  and session bindings. Changing a default account never retargets running work,
  and a quota limit, expiration, or login error never silently switches to
  another account or paid billing mode (`no silent account or billing fallback`).
- **Explicit platform boundaries.** Linux ships first behind explicit windowing,
  keyring, process, and display interfaces; Windows follows on the same host and
  protocol contract without claiming cross-platform completion from Linux-only
  evidence.

## Build and quick start

Build the single executable from a clean public clone (`src/host/hostd`):

```sh
cd src/host/hostd && cargo build --release --locked
```

Run `lico gui` to open the desktop workbench (starting `lico host` automatically
when needed), `lico status` or `lico capabilities` to inspect the local host from
the CLI, and `python3 -m lico_measure.suite acceptance` to verify reproducible
two-clone release builds. Provider CLIs (`claude`, `codex`) are installed
independently.

## Project map

- [Changelog and milestone history](CHANGELOG.md)
- [Product specification](doc/product/202610/20261023/00001_product_spec/product_spec.md) and [roadmap](doc/product/202610/20261025/00001_roadmap/roadmap.md)
- [Owner end-to-end guide](doc/manual_end_to_end.md) and [host build reference](src/host/README.md)
- [Current status and approval gates](PLAN.md)
- [Documentation index](doc/README.md), [experiments](exp/README.md), [source](src/README.md), and [tests](tests/README.md)
- [Contributing](CONTRIBUTING.md)

## License

[MIT](LICENSE). Lico is an independent open-source project; provider names
identify integrations and do not imply endorsement.
