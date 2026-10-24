# M01-P03 — First real agent path, structured input and approvals, Unicode and IME (2026-10-23)

> Status: in_progress
> Milestone: [M01](../../../milestones/202610/20261023/20261023_01_M01_measured_linux_foundation_milestone.md)
> Branch: `phase/M01-P03-first-real-agent`
> Motivation: attach one real, supported agent to the owned host with explicit target, input, approval and reconnect behavior, and prove Unicode and Korean input end to end, before accounts and a second provider are added.
> Approval: G2 approved 2026-10-23 as written, including ten tests (at most three per step). M01-P01 and M01-P02 gates G3 and G4 pending. G3 and G4 for this phase pending.

## Basis recorded from M01-P01 and M01-P02

The owner's direction: both provider paths become adapters behind one contract, but they
are **implemented one at a time**; real model turns and accounts are allowed only in a
limited form and only after the owner confirms each use. Targets are not relaxed; the GUI
idle RSS target stays 50 MiB of process-tree RSS, and the renderer is locked at the end of
this phase (the primary client A2, the fallback C2).

Evidence this plan is written from (see the P01 and P02 reports): the host owns agent
processes independently of any GUI, bounds its queues and memory, serves input with request
ids, approvals, views and a resource report; both clients keep a draft per session; a
composition in progress is committed to the session it was typed for. Open items carried in:

- real agent output is conversation content, so the durable log becomes sensitive data;
- the clients render hex records; a real agent needs UTF-8 text with wide and combining
  characters, wrapping and sanitized terminal escapes;
- the winit client can carry a trailing jamo across a view switch; this phase decides whether
  that is acceptable on a real display;
- the host log is not fsynced and drafts are lost when a GUI is killed.

## Proposed outcome

One real agent path, the Codex app-server (installed version 0.163.0 was read with a version
flag only), attached to the owned host through an adapter whose contract also fits Claude Code
(its terminal path arrives with the second adapter in M01-P04). The adapter maps the provider's
documented session, turn and approval messages onto the host's records, input states and
approvals; it never sends an ambiguous input or approval twice; it treats a vanished provider
process as disconnected and never infers idle from a quiet stream. The clients show real text
output, send Korean and other Unicode input through a real input method and keep it exact.
A real turn is run by the owner, not by this session.

Not in this phase: login, accounts and keyring storage (M01-P04), a second provider, host
crash recovery, packaging (M01-P05), Windows. **Untouched by design:** the owner's provider
credentials. The provider keeps its own login state (AUTH-08); Lico neither reads, copies nor
migrates it, and this session does not run the provider with that state.

## Research before execution

At the first step, with a private empty provider home and no sign-in: record the installed
version, the exact subcommands, whether the app-server can generate its message schema, and
whether the handshake works without credentials. Re-read the provider's current official
documentation for the protocol and its approval messages (the earlier survey read part of that
page; sign-in method detail and the rest of the approval flow are **unverified**). Pin the
version used for every claim; mark anything unavailable as unverified. If the installed
version lacks a documented method, the plan is rechecked with the owner before building.

## Proposed steps

| ID | Work after approval | Meaningful checks (proposed; not present yet) |
|---|---|---|
| S01 | Inspect the installed provider path without credentials; write the adapter contract (capabilities as versioned data, unsupported operations explicit) and a protocol fixture set recorded from documented messages | `test_adapter_capabilities_pin_the_version_and_mark_unverified`; `test_fixture_events_map_to_host_records_in_order` (an unpinned version, an unmarked unsupported operation or a reordered event makes them fail) |
| S02 | The adapter on the host against a protocol simulator built from the fixtures: input becomes one turn (steer for a running turn), provider approval requests become host approvals, reconnect resumes a thread, a vanished provider process marks the session disconnected | `test_input_becomes_exactly_one_turn_and_an_ambiguous_one_is_never_resent`; `test_approval_round_trip_and_a_stale_or_forged_approval_is_refused`; `test_provider_exit_is_disconnected_not_idle` |
| S03 | Isolation for real output: per-session private workspace, an explicit environment allowlist for the provider process, the log marked sensitive with owner-only modes and an explicit purge | `test_provider_environment_is_an_allowlist_and_the_workspace_is_private`; `test_purge_removes_the_session_log_and_nothing_else` |
| S04 | Clients render real UTF-8 output (wide and combining characters, wrapping, history on demand) with terminal escapes sanitized, and send Unicode and Korean input through a real input method | `test_untrusted_output_cannot_act_on_the_display_or_clipboard`; `test_unicode_and_korean_round_trip_through_input_method_and_agent` |
| S05 | Re-measure the targets with real-shaped output, run the hostile suite for the adapter, prepare the owner-run real-turn script, produce the phase report and inspection bundle | `test_adapter_survives_malformed_oversize_and_hostile_provider_messages` |

Tests are named here to define failure behavior; none exists until approved work runs. Each
executed step observes red, implements the minimum, observes green and commits only a green
result. Reuse existing checks; 1–3 new core tests per step. This plan proposes 10 in total;
the actual count is reported. The real turn of S05 is **not** an automated test: it is a script
the owner runs, with the owner's cap on turns and spend, and its transcript is part of the
bundle only if the owner provides it.

## Measurement contract and acceptance

Targets are **copied from the product specification** and have not been met by this phase.
Observe first, then classify each as met, missed or unverified with its scope. A missed target
stops the phase and returns a decision with at least three alternatives; nothing is relaxed.

| Criterion | Workload and required evidence | Fails or stays unverified when |
|---|---|---|
| GUI idle RSS ≤ 50 MiB (PERF-01) | both clients showing real-shaped Unicode output at 1200x800, three launches, PSS beside it | RSS above 50 MiB, a descendant omitted, or Unicode rendering moved out of the measured tree |
| Hidden growth, activation, restart (PERF-02 to PERF-04) | 100 sessions, sessions with 10000-line transcripts, at least 100 observations each, read from the display | fewer than 100 samples, API-only timing, or a target missed |
| Idle CPU (PERF-05) | 60 s with liveness checked; a noisy session separately | the process tree changed during the interval |
| Continuity (PERF-06) | a long simulated turn keeps producing while the GUI is killed for 15 s; the provider process id unchanged; no gap or duplicate | any gap, duplicate, restarted provider or stalled host |
| Input and approval integrity | at least 1000 inputs and 100 approvals with induced reconnects, stale and forged ids; zero resent, lost or misrouted | any ambiguous input or approval sent twice, or sent to a stale target |
| Unicode and Korean | the P01 composition test (four typing conditions) plus wide, combining and right-to-left samples round-tripping exactly through input method, host and agent; a view switched mid-composition | any difference, loss or crossing between sessions |
| Host resources | host RSS and PSS at 10 and 100 sessions with real-shaped history; the provider processes separately | a missing value shown as zero, or host memory growing with history |
| Provider evidence | exact version, capability data and the unverified list; simulator results kept apart from the owner-run real turn | a simulated result presented as a real-provider result |

p95 is nearest rank on a monotonic clock with the sample count published. GUI, host, provider
and display-server costs are scoped separately.

## Personal data & security

This phase persists real conversation content, so the controls apply in full: explicit
response schemas (S1); minimum data and a log marked sensitive (S2); target, generation and
approval-id authorization on every mutation (S3); opaque random ids (S4); canaries absent from
argv, URLs, standard error, errors and any sink other than the intended log (S5, S7); masked
labels (S6); a complete route inventory (S8); bounded requests, queues and provider messages
with an explicit throttle policy (S9); owner-only IPC and a protected loopback listener (S11).
Webhooks and biometric data do not exist (S10, S12). No shared private helper is imported.

Specific to a real provider: the provider process gets an explicit environment allowlist and a
private per-session workspace, never the host's secrets; approval prompts and tool output from
the provider are untrusted display data, length-capped and sanitized; an approval answer is
bound to its approval id, session and generation and is never repeated after a reconnect;
the durable log has owner-only modes, a retention limit and a purge; nothing is uploaded. The
provider keeps its own credential storage; a locked or missing credential store is the
provider's failure to report, not something Lico works around.

Hostile cases: forged and replayed approval ids, an approval for another session, a provider
that sends malformed, oversize or unending messages, escape sequences that try to move the
cursor, write the clipboard or open links, a canary in prompts and output, a provider that
exits mid-turn. A rejected mutation must not dispatch work. The plan is rechecked with the
owner if candidate research adds endpoints or stored user data beyond the log.

## Manual inspection

Proposed local bundle: `private/manual_inspection/YYYYMM/YYYYMMDD/002_M01-P03_first_real_agent/`
(date chosen when the approved phase runs). It shows the simulated session end to end (input,
turn, approval, reconnect), a GUI killed while a turn runs, Korean and mixed-script round trips
as screens and transcripts for both clients, escape-sequence attempts shown harmless, the
resource tables, the provider capability data with its unverified list, the hostile exchanges
as an attacker's view and, if the owner runs it, the real-turn transcript kept visibly apart.
A screenshot alone does not prove input or continuity.

It looks right when the correct session, provider and target stay identifiable, hidden work
continues, a restarted GUI shows current content, no input or approval lands in a stale target,
and nothing typed in one session appears in another. The bundle is generated by a public
command under `src/` from a clean commit with a receipt (command, commit, binary hashes,
workload); an independent reviewer checks the measurements; the verdict stays pending until
actually observed.

## Out of scope

Login, accounts, keyring storage and the second provider (M01-P04); host crash recovery,
packaging and a clean-clone release (M01-P05); Windows; remote access. M01-P04 and M01-P05
need their own plans and approvals.

## Progress

- 2026-10-23: Draft prepared from the P01 and P02 evidence and the owner's direction on the
  provider path and real turns. No branch, implementation, test or experiment existed.
- 2026-10-23: G2 approved as written. Branch created from the documentation commit that carries
  the P02 work; this documentation-only commit records the approval. No step has run: S01 to S05
  are not started. G3 and G4 are pending.
- 2026-10-23: S01 done. The installed Codex 0.163.0 was inspected with a private empty home and
  a minimal environment (no sign-in, no model turn): the app-server speaks JSON lines on
  standard input and output, its handshake works without credentials, `account/read` reports no
  account and that sign-in is required, and a warning says its Linux sandbox needs user
  namespaces on this host. The protocol schema the binary generates (105 client requests, 85
  notifications, 10 server requests) is pinned by hash in a capability record that also lists
  what is unsupported and what is unverified. A reference mapping turns provider messages into
  one-line host records and approvals (escape sequences and control characters removed, an
  approval answer bound to exactly one provider request, requests the client cannot serve
  answered with an error); recorded fixtures are valid for the provider's own schema. Two tests,
  each checked with mutations. The code was written before its tests (no red-before-code run);
  red was observed by mutation. Not yet verified: any signed-in behavior, the exact approval
  events of a real turn, and the Claude Code path (not run by this session).
- 2026-10-23: S02 done. The owned host runs a provider session (`session.create` with kind
  `codex`) through an adapter against a protocol simulator that replays the recorded turn: an input
  becomes one turn or steers the running one (a refused steer falls back to a turn, never both), a
  provider approval request becomes one host approval answered at most once, requests the client
  cannot serve are answered with an error, a vanished provider makes the session `disconnected`
  and `session.resume` brings it back with the execution generation raised by one (the generation
  is now a real value, not a placeholder). Three tests, each checked with mutations (two mutations
  of one guard survived because a second independent guard also refuses; a combined mutation was
  caught). The host's records equal the reference mapping for the recorded turn. Everything here is
  simulated; no real provider turn has run.
