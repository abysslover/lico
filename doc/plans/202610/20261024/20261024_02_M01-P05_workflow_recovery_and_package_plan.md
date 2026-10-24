# M01-P05 — User-facing workflow, resource acceptance, failure recovery and a reproducible Linux package (2026-10-24)

> Status: in_progress
> Milestone: [M01](../../../milestones/202610/20261023/20261023_01_M01_measured_linux_foundation_milestone.md)
> Branch: `phase/M01-P05-workflow-recovery-package`
> Motivation: turn the measured foundation into something the owner can actually use end to end, prove it survives the failures that matter, re-measure it as shipped, and produce a package a clean public clone can rebuild.
> Approval: G2 approved 2026-10-24: the plan as drafted with every recommendation, except decision 6 (the package format), which the owner replaced by a single self-contained executable (see "Owner decision on the package"). M01-P01 to M01-P04 gates G3 and G4 pending. G3 and G4 for this phase pending.

## Basis recorded from M01-P01 to M01-P04

Measured and tested against synthetic work, simulators and, since 2026-10-24, one owner-run real turn per provider: a persistent
Rust host with a durable log, a bounded protocol, two provider adapters (Codex verified for input, streaming and a real
approval denial; Claude Code verified for input and streaming, its permission flow unverified), accounts with private homes,
a fail-closed credential store, and two minimal clients (A2 primary, C2 fallback). No target has been missed.

What the clients cannot do yet (they were measurement vehicles driven by commands on standard input): list sessions, show state,
approvals or the account beside the input, read older history, select or copy, or be used without a test harness. What the host
cannot do yet: survive its own restart (sessions and their bindings are in memory; a host that stops loses the session table, the
provider processes die with it by design, and the log is not fsynced). Open items carried in from the reports: A2's noisy-case CPU
(99%), C2's thin PERF-01 margin (4.0 MiB), a provider process costs about 90 to 120 MiB, the keyring bus reaching every process of
a provider that has a registered key, no fake-bus test, no test of the purge and resume race, the Claude permission flow and the
Codex allow path unobserved, accessibility and bidirectional layout unchecked.

## Proposed outcome

1. **A usable first view in the primary client** (SES-02, SES-03, SES-05, section 8 of the specification): sessions grouped by workspace
   with a stable order and their real state (working, idle, waiting for approval, disconnected, exited, with the time of the last known
   state), a clear selected session with provider, account and model beside the input, approval requests discoverable while their
   session is hidden, older history read on demand, selection and copy as an explicit user action, resize, and no renderer for a
   hidden session. The wording of every control is plain; nothing moves under the pointer.
2. **Recovery from the failures that matter**: the host restarts and finds its sessions again from a persisted registry (id, provider,
   account binding, provider session id, generation) and its durable log, showing each as `disconnected` with an explicit resume
   action, never as running; a torn log tail or a damaged registry is detected and reported, never replayed as data; the log's
   durability policy is explicit (fsync on a stated interval); a GUI killed with a draft keeps the draft or says it did not.
   Live provider processes are **not** preserved across a host restart (SES-07: a saved conversation resume is not the preservation
   of a live process); the plan states this to the user instead of hiding it.
3. **Resource acceptance on the shipped binaries**: every PERF target re-measured with the real workflow UI, with the package size
   (PERF-07), idle and peak RSS and PSS per process category, fonts and display configuration, at more than one display scale.
4. **One self-contained executable**: a clean public clone, with only public inputs and a pinned toolchain, builds the same single
   file twice (same checksums). That one file is the host, the command line, the credential command and the primary client (one
   program with subcommands), carries the complete MIT license and the third-party notices of every bundled dependency inside
   itself, needs no file beside it and holds no host path, local instruction file or private data. It depends on the system's C library
   and loads the display and input libraries of the machine when it starts (see "Owner decision on the package").
5. **The owner's own use**: a checklist and a script the owner follows with the real accounts (sign in per account, two sessions, an
   approval, a GUI restart, a host restart, a removal), its transcript kept private; the bundle carries the sanitized evidence only.

Not in this phase: Windows (specification step 6), remote access, a third provider, tabs or panes beyond the first view, themes,
notifications beyond a small status mark, an auto-updater, Wayland-specific work beyond what the measurements can state.

## Research before execution

At the first step, with no download: list the tracked inputs of a clean clone and the build requirements of the primary client and
the host (toolchain versions, system libraries each binary links, their licenses and whether each is provided by the system or
bundled); record the display-server and input-method assumptions the primary client makes and what is untested (Wayland, other
input methods, accessibility); read the current documentation for the archive format and the license-notice obligations of every
dependency (`Cargo.lock`) and write what is **unverified**. Any download of code, toolchain or crate is announced with its exact
version and approved first. If research adds endpoints or stored user data (the registry is stored data), this plan is rechecked
with the owner before building.

## Proposed steps

| ID | Work after approval | Meaningful checks (proposed; not present yet) |
|---|---|---|
| S01 | Persisted session registry, restart recovery (every session returns `disconnected` with its bindings and history; a resume action per provider), log durability policy, detection of a torn tail or a damaged registry; a test of the purge and resume race | `test_host_restart_recovers_sessions_as_disconnected_with_bindings_and_history`; `test_a_torn_log_tail_or_a_damaged_registry_is_reported_and_never_replayed_as_data` |
| S02 | The first view in the primary client: session list by workspace with state and time, selected session with provider, account and model beside the input, approvals visible for hidden sessions, scrollback on demand, selection and copy as a user action, resize | `test_session_list_shows_state_account_and_a_pending_approval_while_the_session_is_hidden`; `test_older_history_is_read_on_demand_without_replaying_the_transcript`; `test_only_selected_text_reaches_the_clipboard_and_only_on_a_user_action` |
| S03 | Re-measure every target on the real UI at two display scales, with package size and every process category; repair of the noisy-case CPU if the owner wants it (decision below); accessibility and bidirectional layout stated as checked or unverified | the existing measurement reports must carry the new workload, scale and scope or fail (`test_measurement_report_requires_scope_and_samples` extended); no new test unless a repair is built |
| S04 | The single executable: one program with subcommands replacing the separate binaries, build script, embedded license notices, checksums, a dependency license check, a guard that the file holds no private content or host path | `test_the_single_executable_runs_every_role_alone_in_an_empty_directory`; `test_a_clean_clone_builds_the_same_executable_twice_without_private_or_host_content`; `test_every_bundled_dependency_has_its_license_notice` |
| S05 | The owner's end-to-end script and checklist, the phase report and the inspection bundle, an independent read-only reviewer (announced first), fixes | none (the owner's use is evidence, not a test) |

Tests are named to define failure behavior; none exists until approved work runs. Each step observes red, implements the minimum,
observes green and commits only a green result. Reuse existing checks; at most three new core tests per step. This plan proposes
nine; the actual count is reported.

## Measurement contract and acceptance

Targets are copied from the product specification and are not met by this phase until measured. A missed target stops the phase and
returns a decision with at least three alternatives; nothing is relaxed.

| Criterion | Workload and required evidence | Fails or stays unverified when |
|---|---|---|
| PERF-01 to PERF-06 on the real UI | both clients that remain shipped, the first view with several sessions, hidden sessions, 10000-line transcripts, 100 observations where required, two display scales | a target missed, a process of the tree omitted, fewer samples, only one scale |
| PERF-07 inventory | executable size on disk, what it links and loads, idle and peak RSS/PSS per process category (a process of the executable's host role and of its client role are separate processes), fonts, display configuration | a missing value shown as zero, or package size and process memory merged |
| Recovery | host killed with sessions running, with a draft in the GUI, with a log write in flight: every session `disconnected`, no record lost that was durable, no duplicate, none replayed from a torn tail | any session shown running that has no process, any lost durable record, any ambiguous input resent |
| Reproducibility | two builds from a clean clone, checksums compared, the executable searched for private content, host paths and canaries | differing checksums, a private string or a canary in the file |
| Owner's use | the owner's checklist completed with real accounts, with the failures and surprises recorded | claimed without the owner's own record |

p95 is nearest rank on a monotonic clock with the sample count published.

## Personal data & security

The registry adds stored user data (session ids, provider, account binding, provider session ids, workspace paths): owner-only modes,
no secret values, no conversation content, purge removes its entry, and the file is covered by the secret-sink scan. The log
remains sensitive data (owner-only, retention limit, purge). The copy action is the only way text reaches the clipboard; terminal
content stays untrusted display data. The executable contains no `CLAUDE.md`, `AGENTS.md`, local instructions, receipts, host paths,
runtime bindings, dashboard or card references or names of outside systems; a guard checks the file as it checks the tree. A real
turn or login happens only in the owner's own script with the owner's caps. Hostile cases: a damaged or forged registry (an id of
another session, a path outside the workspace root, an account that does not exist), a log with a torn tail or a forged length, a
resume of a session whose provider session id is hostile, a copy request from agent text.

## Manual inspection

Proposed local bundle: `private/manual_inspection/YYYYMM/YYYYMMDD/004_M01-P05_workflow_recovery_package/`. It shows the first view
with several sessions in each state, an approval raised in a hidden session, scrollback, the host killed and restarted with every
session returned as `disconnected` and resumed, the resource tables at two scales, the executable's size, links and checksums of two builds,
and, if the owner provides it, the sanitized account of the owner's own use. The bundle is generated by a public command under
`src/` from a clean commit with a receipt; an independent reviewer checks the measurements; the verdict stays pending until
actually observed.

## Decisions needed from the owner before approval

1. **How much interface is enough for M01.** The plan above is the smallest view that is usable and satisfies the specification's
   everyday-interaction section; accessibility and a full text-editing input line (beyond Korean composition) are not included.
   Recommended: as written.
2. **Which client ships.** A2 (the primary: smallest memory; its dependencies are MIT or Apache) or C2 (the fallback: Qt 5, LGPL or GPL).
   Recommended: ship A2 only and keep C2 as a measured fallback in the source, not in the package.
3. **The noisy-case CPU of A2** (99% of a core at 100 lines per second). Recommended: repair it in S03 with a frame cap or
   incremental shaping and re-measure, because the primary client would otherwise burn a core on an ordinary busy session.
4. **Host restart semantics.** Recommended: live provider processes end with the host and come back as `disconnected` with a resume
   action (consistent with SES-07). Preserving live processes across a host restart would need a detached supervisor and a
   reattach protocol that no provider documents; it is left out.
5. **The keyring reaching every process of a provider.** A helper that talks to the host over a private socket removes it.
   Recommended: do it in S01, before a real key is registered.
6. **Package format** — replaced by the owner: a single executable (see below).
7. **The public repository.** A clean-clone rebuild needs the source readable by a clean checkout; publication and the first public
   push (with the history inspection the specification requires) are the owner's and Pri's to arrange.
8. **Spend.** Rough, unverified estimate for the phase: S01 5 to 8, S02 10 to 15, S03 5 to 8, S04 6 to 10, S05 8 to 12 US dollars, 35 to 55 in all
   including the reviewer; the owner's stated remaining limit is about 73 dollars. Measurement quality and sample counts are not traded for cost.

## Owner decision on the package (2026-10-24)

The owner's final goal is one executable that runs by itself, like curl: a completely built single file, which is the product's reason
to exist and fits its design philosophy; some hard work to get there is accepted. This replaces decision 6 and changes the plan:

- **One program, several roles.** The host, the command line, the credential command and the primary client become subcommands of
  one executable. A role is a separate process (the client and the host are still different processes, so the client's measured
  memory is not the host's); the client starts the host if none runs. The test agent, the simulators and the measurement tools are
  not part of it.
- **What "single file" can honestly mean.** It needs no file beside it and has the license notices and any data inside it. It still
  needs the machine's C library, and a graphical program cannot avoid the machine's display system: the display and input libraries are
  loaded by name when the client starts (the primary client already does this), and fonts come from the system. A fully static file
  would have to give up the display system on this platform; this plan does not claim it. The step records exactly what the file links
  and loads, what happens without a display library or a font (a clear message, never a crash), and the cost of embedding a font
  (a Hangul and CJK font is large) as a measured alternative, not an assumption.
- **No download without approval.** Merging the host and the client into one program reuses crates already in the offline cache; a
  new target or toolchain component (for example a static C library target) is announced with its exact version first.
- **Risks to measure, not assume:** the client's idle memory with the host's code in the same file (file-backed pages count in RSS only
  when touched), the file size, startup time, and that a build is byte-identical twice.

## Out of scope

Windows, remote access, a third provider, tabs or panes beyond the first view, an updater, anything that needs a download that was not
approved. Windows is specification step 6 and needs its own milestone.

## Progress

- 2026-10-24: Draft prepared from the M01-P01 to M01-P04 evidence and the specification's delivery section. No branch, implementation,
  test, download or experiment existed.
- 2026-10-24: G2 approved as drafted with every recommendation; the package became a single self-contained executable (owner decision
  above). No step has run: S01 to S05 are not started. G3 and G4 are pending.
- 2026-10-24: S01 done. A persisted session registry and restart recovery: sessions return as `disconnected` with their account
  binding, provider session id, generation, history and the time of their last state; a purged session does not return; each log
  frame is checked on start and a torn, out-of-order or zero-filled tail is set aside in `.damaged` files and reported; logs sync
  about once a second; a damaged or forged registry stops the host. The purge-during-resume race is tested (a deleted session gets no
  process, log or workspace). The keyring exposure decision is done: a provider gets a private per-session key broker instead of the
  keyring's bus. Three tests; mutations caught: sessions recovered as running, frames not checked beyond their length (found only
  after a zero-filled-tail case was added) and registry ids not validated. Limits: no checksum (damage inside a frame is not
  detected), a log can lose its last second on a power cut, live provider processes do not survive a host restart (by design).
- 2026-10-24: S02 done. The primary client (A2, started with `--ui`) has a first view: a list of the host's provider sessions grouped by
  provider and account (the host's order within a group), each with its real state (working, idle, waiting for approval, disconnected,
  exited, as the host and the provider report them, with the age of the state), the selected session with provider, account label and
  model beside the input, an approval bar with Allow and Deny for the selected session (the list shows pending approvals of hidden
  sessions; clicking the row brings the bar up), older history read on demand from the host's log (`history`, a page of 100, never the
  whole transcript, at most 2000 lines held), and line-wise selection copied only by Ctrl+Shift+C into the X11 clipboard. The host gained
  `provider`, `model` and `activity` in its status and the `history` operation. Three tests; mutations caught: a pending approval hidden
  behind "idle", older history never requested, and copying on selecting. Limits: selection is by line (no character selection, no
  paste into the input, no Wayland clipboard), the list is grouped by provider and account because every session's workspace is its own
  private directory, status is polled once a second (idle CPU is re-measured in S03), the list shows no scroll bar and is not
  scrollable beyond the window, long lines wrap by an estimate that is verified against the layout, nothing here is accessible to a
  screen reader, and C2 keeps its older view and is not extended.
- 2026-10-24: S03 (code part) done. The first view follows the display scale (sizes are logical sizes times the factor; the list is
  clipped to its column), the list's ages are coarse (under a minute, then minutes) so an idle client does not redraw, and a flood of
  frames (more than 8 draws in a second) is drawn at 10 frames a second, because one frame of the first view costs about 35 ms of
  software drawing (a burst such as an activation is still drawn at once). The measurement rig takes `--ui` and `--scale 1|2` (the X
  screen is scaled and the toolkit told the factor). One test: 100 agent-shaped lines a second keep the client under 55% of a core
  (99% without the cap) and the text stays within about a second of the host's newest. The measurement runs follow.
