# M01-P05 — User-facing workflow, resource acceptance, failure recovery and a reproducible Linux package (2026-10-24)

> Status: draft
> Milestone: [M01](../../../milestones/202610/20261023/20261023_01_M01_measured_linux_foundation_milestone.md)
> Branch: `phase/M01-P05-workflow-recovery-package` (not created; created from the reviewed documentation commit after approval)
> Motivation: turn the measured foundation into something the owner can actually use end to end, prove it survives the failures that matter, re-measure it as shipped, and produce a package a clean public clone can rebuild.
> Approval: G2 **pending**. Nothing below is approved, built or measured. M01-P01 to M01-P04 gates G3 and G4 pending.

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
4. **A reproducible Linux package**: a clean public clone, with only public inputs and a pinned toolchain, builds the same package
   twice (same checksums), containing the host, the command line, the credential command, the primary client, the complete MIT
   license and the third-party notices of every bundled dependency, with no host path, no local instruction file and no private
   data. The first format is a plain archive with an install script; other formats are for later.
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
| S04 | The reproducible package: build script, archive, license notices, checksums, a dependency license check, a guard that the archive holds no private file or host path | `test_a_clean_clone_builds_the_same_package_twice_without_private_or_host_content`; `test_every_bundled_dependency_has_its_license_notice` |
| S05 | The owner's end-to-end script and checklist, the phase report and the inspection bundle, an independent read-only reviewer (announced first), fixes | none (the owner's use is evidence, not a test) |

Tests are named to define failure behavior; none exists until approved work runs. Each step observes red, implements the minimum,
observes green and commits only a green result. Reuse existing checks; at most three new core tests per step. This plan proposes
eight; the actual count is reported.

## Measurement contract and acceptance

Targets are copied from the product specification and are not met by this phase until measured. A missed target stops the phase and
returns a decision with at least three alternatives; nothing is relaxed.

| Criterion | Workload and required evidence | Fails or stays unverified when |
|---|---|---|
| PERF-01 to PERF-06 on the real UI | both clients that remain shipped, the first view with several sessions, hidden sessions, 10000-line transcripts, 100 observations where required, two display scales | a target missed, a process of the tree omitted, fewer samples, only one scale |
| PERF-07 inventory | archive size, installed size, binaries and what each links, idle and peak RSS/PSS per category, fonts, display configuration | a missing value shown as zero, or package size and process memory merged |
| Recovery | host killed with sessions running, with a draft in the GUI, with a log write in flight: every session `disconnected`, no record lost that was durable, no duplicate, none replayed from a torn tail | any session shown running that has no process, any lost durable record, any ambiguous input resent |
| Reproducibility | two builds from a clean clone, checksums compared, the archive searched for private files, host paths and canaries | differing checksums, a private file or a canary in the archive |
| Owner's use | the owner's checklist completed with real accounts, with the failures and surprises recorded | claimed without the owner's own record |

p95 is nearest rank on a monotonic clock with the sample count published.

## Personal data & security

The registry adds stored user data (session ids, provider, account binding, provider session ids, workspace paths): owner-only modes,
no secret values, no conversation content, purge removes its entry, and the file is covered by the secret-sink scan. The log
remains sensitive data (owner-only, retention limit, purge). The copy action is the only way text reaches the clipboard; terminal
content stays untrusted display data. The package contains no `CLAUDE.md`, `AGENTS.md`, local instructions, receipts, host paths,
runtime bindings, dashboard or card references or names of outside systems; a guard checks the archive as it checks the tree. A real
turn or login happens only in the owner's own script with the owner's caps. Hostile cases: a damaged or forged registry (an id of
another session, a path outside the workspace root, an account that does not exist), a log with a torn tail or a forged length, a
resume of a session whose provider session id is hostile, a copy request from agent text.

## Manual inspection

Proposed local bundle: `private/manual_inspection/YYYYMM/YYYYMMDD/004_M01-P05_workflow_recovery_package/`. It shows the first view
with several sessions in each state, an approval raised in a hidden session, scrollback, the host killed and restarted with every
session returned as `disconnected` and resumed, the resource tables at two scales, the package listing and checksums of two builds,
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
6. **Package format.** Recommended: a plain archive with an install script first; AppImage or a distribution package later.
7. **The public repository.** A clean-clone rebuild needs the source readable by a clean checkout; publication and the first public
   push (with the history inspection the specification requires) are the owner's and Pri's to arrange.
8. **Spend.** Rough, unverified estimate for the phase: S01 5 to 8, S02 10 to 15, S03 5 to 8, S04 6 to 10, S05 8 to 12 US dollars, 35 to 55 in all
   including the reviewer; the owner's stated remaining limit is about 73 dollars. Measurement quality and sample counts are not traded for cost.

## Out of scope

Windows, remote access, a third provider, tabs or panes beyond the first view, an updater, anything that needs a download that was not
approved. Windows is specification step 6 and needs its own milestone.

## Progress

- 2026-10-24: Draft prepared from the M01-P01 to M01-P04 evidence and the specification's delivery section. No branch, implementation,
  test, download or experiment exists. G2 pending.
