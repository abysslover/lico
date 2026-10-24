# M01-P04 — Keyring-only credentials, same-provider accounts and a second provider adapter (2026-10-24)

> Status: in_progress
> Milestone: [M01](../../../milestones/202610/20261023/20261023_01_M01_measured_linux_foundation_milestone.md)
> Branch: `phase/M01-P04-accounts-second-provider` (not created; created from the reviewed documentation commit after approval)
> Motivation: store provider and application secrets only in the OS credential store, run several accounts of one provider side by side without sharing state, and add a second provider behind the adapter contract, before packaging and recovery.
> Approval: G2 approved 2026-10-24 as drafted. The five decisions below keep their recommendations, except that the owner asked for repeated experiments against the keyring the owner registered; that request is open (see decision 1). M01-P01 to M01-P03 gates G3 and G4 pending. G3 and G4 for this phase pending.

## Basis recorded from M01-P03

M01-P03 delivered the adapter contract on one provider (the Codex app-server), tested against a
protocol simulator; no real provider turn was run by the development session. The host already
binds an input and an approval to a session, a generation and an approval id, never resends an
ambiguous input, starts every provider process in a private workspace with an environment
allowlist (`PATH`, `HOME`, `LANG`, `CODEX_HOME`) and its own process group, and can purge a
session. The real-turn command runs the provider with an empty private `HOME` and the provider's
own `CODEX_HOME`. Open items carried in: a provider process costs about 90 to 120 MiB when idle
(runtime reuse is therefore a real question, AUTH-05); an input in flight when a provider vanishes
stays unresolved; approval numbering is per session.

## Proposed outcome

A credential store behind one interface, backed on Linux by a Secret Service implementation, that
fails closed when the keyring is locked or missing and never falls back to a file. An account
model with stable local ids, editable labels and masked labels, kept in a metadata store that holds
only opaque credential references. Sessions bound to an account and a credential generation; a
default account that only affects new sessions; an expired account, limit or login failure that
never switches to another account or to a paid mode by itself. Removal that names the affected
sessions and keeps everything else. A second adapter for Claude Code behind the same contract as
the Codex adapter, first against a simulator built from recorded messages. Each account gets its
own provider home so two accounts of the same provider never share a slot or an auth file.
Real accounts, real keyring entries and real billable turns appear only with the owner's explicit
step-by-step approval (see "Decisions needed").

Not in this phase: OAuth login flows implemented by Lico (a provider's own login stays the
provider's, AUTH-08), usage dashboards, host crash recovery, packaging (M01-P05), Windows
Credential Manager, a third provider, remote access.

## Research before execution

Read from the provider's official documentation on 2026-10-24 (pages `headless` and
`authentication` of the Claude Code documentation); everything else is **unverified** until the
first step records it for the installed version:

- Claude Code runs non-interactively with `claude -p`, reads and writes newline-delimited JSON
  (`--input-format stream-json`, `--output-format stream-json`), reports a session id, and resumes
  a session with `--resume`. A permission request in a non-interactive run is denied unless a
  `PermissionRequest` hook or `--permission-prompt-tool` answers it; `--permission-prompts none`
  denies without waiting. Exact message shapes, the way a host answers a permission request and
  how a session is resumed after the process dies are **not read yet**.
- On Linux it stores its own login in `~/.claude/.credentials.json` (mode 0600), under
  `CLAUDE_CONFIG_DIR` when that is set; it does **not** use the Secret Service. A separate
  `CLAUDE_CONFIG_DIR` per account keeps settings, history and login apart, except a keyless
  Console sign-in, which it stores outside that directory. `--bare` never reads OAuth credentials
  or the system keychain and takes a key from `ANTHROPIC_API_KEY` or an `apiKeyHelper` setting.
  Consequence for AUTH-01 and AUTH-08: a subscription login stays provider-managed in the
  provider's file and is **not** a Lico-managed secret; only an API key can live in the keyring,
  handed to the provider through a helper (never argv, never a file). Which of the two the owner
  wants is a decision below.
- The installed Codex 0.163.0 was pinned in M01-P03; whether it can keep its own credentials in
  the Secret Service (a documented setting, **not verified**) is read in S01 before any claim.
- Secret Service on this host: `gnome-keyring-daemon` and `dbus-daemon` are installed; the
  libsecret development files and every Rust or C client library are **not**. A client therefore
  means a new dependency (a crate or a library, pinned, announced and approved before download) or
  a small hand-written D-Bus client; the cost in binary size and resident memory is measured in
  S01 and decides, not assumed. If the research adds endpoints or stored user data beyond what is
  written here, the plan is rechecked with the owner before building.

## Proposed steps

| ID | Work after approval | Meaningful checks (proposed; not present yet) |
|---|---|---|
| S01 | Record the Secret Service interface and its client options with size and memory; credential store interface; fail-closed behavior; keyring entries namespaced by application, provider, account and credential kind; a metadata store with opaque references and masked labels. Tested against a **throwaway private keyring** (private session bus, `gnome-keyring-daemon` in a private home, canary values), never the owner's keyring | `test_secret_values_stay_in_the_keyring_and_every_other_sink_holds_none` (a canary in argv, environment of other processes, logs, errors, the metadata store, fixtures and crash output makes it fail); `test_locked_or_missing_keyring_fails_closed_with_an_action` (a plaintext fallback makes it fail) |
| S02 | Account model: stable ids, editable and masked labels, session binding to account and credential generation, default account for new sessions only, a private provider home per account | `test_two_accounts_of_one_provider_share_no_slot_file_or_process_state`; `test_a_running_session_keeps_its_binding_when_the_default_changes_and_an_account_change_during_a_turn_is_refused` |
| S03 | Claude Code adapter behind the P03 contract: capability record pinned to the installed version, recorded schema-valid fixtures, simulator, input and approval semantics, resume after a vanished process; credentials handed over through a helper that reads the keyring, never argv or a file | `test_claude_adapter_pins_the_version_and_marks_unverified_and_maps_fixtures_in_order`; `test_claude_input_and_approval_reach_the_provider_exactly_once_and_a_stale_target_is_refused` |
| S04 | Failure and removal behavior: expired account, limit or login failure never switches account or mode; removal names the affected sessions and offers an explicit action; local removal and remote revocation are separate outcomes; unknown usage is not zero | `test_account_failure_never_switches_account_or_paid_mode`; `test_removal_names_affected_sessions_and_leaves_other_accounts_histories_and_running_work` |
| S05 | Re-measure the targets with several accounts, runtime reuse check against account, workspace, permissions and tool state, hostile suite for both adapters, owner-run real-turn script for the second provider, report and inspection bundle | `test_runtime_reuse_never_merges_accounts_or_tool_contexts` |

Tests are named to define failure behavior; none exists until approved work runs. Each step
observes red, implements the minimum, observes green and commits only a green result. Reuse
existing checks; at most three new core tests per step. This plan proposes nine in total; the
actual count is reported. Real turns are **not** automated tests: they are scripts the owner runs
with the owner's cap on turns and spend.

## Measurement contract and acceptance

Targets are **copied from the product specification** and are not met by this phase until
measured. A missed target stops the phase and returns a decision with at least three
alternatives; nothing is relaxed.

| Criterion | Workload and required evidence | Fails or stays unverified when |
|---|---|---|
| GUI idle RSS ≤ 50 MiB (PERF-01), PERF-02 to PERF-05 | both clients as in M01-P03 with accounts and the credential store present, at least 100 observations where required | any target missed, a process of the credential path omitted from the tree, or fewer samples |
| Credential store | latency (p95 of a read, at least 100 observations), resident memory and binary size of the chosen client, with the keyring unlocked, locked and absent | a missing value shown as zero, locked or absent treated as unlocked, or a plaintext fallback |
| Secrets in sinks | a canary secret in every collected non-secret sink: argv, `/proc` environments, logs, errors, metadata store, fixtures, crash output, bundle | any occurrence |
| Account isolation | A/B concurrent requests routed to the right account, independent logout and revoke, default change, refresh race, replayed or cancelled login callback (simulated) | any crossing of accounts, a shared slot or file, a silent switch |
| Runtime reuse (AUTH-05) | provider processes per account and workspace; a shared process only when account, cwd, permissions and tool state are identical | two accounts or tool contexts in one process |
| Adapter integrity, both providers | at least 1000 inputs and 100 approvals per adapter with induced reconnects, stale and forged ids against simulators; zero resent, lost or misrouted | any ambiguous input or approval sent twice or to a stale target |
| Provider evidence | exact versions, capability data, unverified lists; simulator results kept apart from owner-run real turns | a simulated result presented as a real-provider result |

p95 is nearest rank on a monotonic clock with the sample count published.

## Personal data & security

This phase adds secrets, so the controls apply in full: explicit response schemas (S1);
minimum data, masked labels and a metadata store without secret values (S2); account, target,
generation and approval-id authorization on every mutation (S3); opaque random ids for accounts
and credential references (S4); canaries absent from argv, URLs, standard error, errors and any
sink other than the keyring (S5, S7); masked labels (S6); a complete route inventory (S8);
bounded requests, queues and provider messages (S9); owner-only IPC (S11). A locked or missing
keyring fails closed with an unlock or setup action; there is no plaintext fallback. Provider-
managed credentials stay provider-managed (AUTH-08): nothing is migrated, copied, symlinked or
decrypted. An account's removal never reads or deletes another account's entries.

Hostile cases: a canary secret everywhere; a keyring entry of another application or account; a
forged or replayed account id, credential reference or approval id; a cancelled or replayed
login callback; a locked keyring during a turn; two accounts racing a refresh; a provider that
echoes a secret into its output (the output is sanitized display data, and the canary must not
reach another sink).

## Manual inspection

Proposed local bundle: `private/manual_inspection/YYYYMM/YYYYMMDD/003_M01-P04_accounts_second_provider/`.
It shows two accounts of one provider side by side with masked labels, a session bound to one of
them while the default changes, a locked keyring failing closed with its action, a removal that
names its affected sessions, both adapters end to end against their simulators, the secret-sink
scan as an attacker's view, the resource tables, and, if the owner runs it, a real-turn transcript
kept visibly apart. The bundle is generated by a public command under `src/` from a clean commit
with a receipt; an independent reviewer checks the measurements; the verdict stays pending until
actually observed.

## Owner input after approval (2026-10-24)

The owner asked whether experiments could use the keyring registered for their own Claude and
Codex accounts, then recalled that no keyring is involved: both tools sign in through the browser
with OAuth and keep that login themselves. This matches the Claude Code documentation read above
(a login file in its configuration directory, not the Secret Service); for the installed Codex it
is **unverified**. Consequences for this plan:

- Decision 1 is settled: the development session tests the credential store only against a
  throwaway private keyring. There is no owner keyring to run experiments against.
- The real-account path is **provider-managed OAuth**: Lico creates one private provider home per
  account (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`), the owner signs in through the browser into that
  home with the provider's own command, and Lico only binds sessions to the account and isolates
  the homes. Lico never reads, copies or moves the login (AUTH-08). Repeated real experiments are
  owner-run scripts that use those homes; the development session does not log in.
- The keyring holds only what Lico itself owns: an API key the owner chooses to register for a
  helper-based mode, and other application secrets (for example the control token of the loopback
  listener, now a 0600 file, which AUTH-01 would move to the credential store; whether that
  belongs in this phase is for S01 to answer).
- If no Secret Service runs in the owner's session, Lico fails closed with a setup action; that
  is a tested behavior, not a gap.

## Decisions needed from the owner before approval

1. **Keyring testing (settled, see above).** The development session was told not to touch a keyring. The plan tests
   only against a throwaway private keyring (private session bus, `gnome-keyring-daemon` in a
   private home, canary values), which is not the owner's keyring and holds no real secret.
   Is that allowed for this session? Recommended: yes, throwaway only.
2. **Claude Code credential model (updated: (b) is the real-account path).** (a) A Lico-managed API key in the keyring, handed over through
   a helper (billed per token, fits AUTH-01); (b) the provider-managed subscription login in a
   per-account `CLAUDE_CONFIG_DIR` (stays provider-managed, AUTH-08, not in the keyring); (c) both,
   one adapter mode each. Recommended: (c), with (a) first because only it can be tested
   without a real login.
3. **Secret Service client.** A pinned Rust crate with a D-Bus library (several new dependencies),
   a pinned C library whose headers are not installed, or a small hand-written D-Bus client.
   The choice is decided by the S01 measurements of binary size and memory; the owner approves
   the download of whichever is chosen. Recommended: measure the hand-written client first,
   because the host and clients are budgeted in single-digit mebibytes.
4. **Real accounts and billable turns.** None are used by the development session. Real login,
   real keyring entries and real turns happen only as owner-run scripts with the owner's caps.
5. Whether the runtime-reuse question (one provider process per account and workspace versus per
   session) should be decided in this phase from the AUTH-05 measurements. Recommended: yes.

## Out of scope

Provider login flows written by Lico, usage dashboards, host crash recovery, packaging and a
clean-clone release (M01-P05), Windows, remote access. M01-P05 needs its own plan and approval.

## Progress

- 2026-10-24: Draft prepared from the M01-P03 evidence and the provider documentation read on
  this date. No branch, implementation, test, download or experiment existed.
- 2026-10-24: G2 approved as drafted. No step has run: S01 to S05 are not started. G3 and G4 are pending.
- 2026-10-24: S01 done. A Secret Service client written by hand (no new dependency, no download) and a
  `lico-cred` command store, read and remove a secret in a throwaway private keyring, keep opaque
  references and masked labels in a 0600 metadata file, take the secret from standard input only, and fail
  closed (exit 3 with an action, nothing stored elsewhere) when the bus, the service or the default keyring
  is missing or locked. Two tests, each checked with a mutation (a copy of the secret in a file; a plaintext
  fallback). The code was written before its tests ran against it (red was the missing binary, then a bug in
  the test itself). Indicative cost: 407 KB binary, 1.9 MiB per process, 1.6 ms median read. Tested against
  gnome-keyring 46.1 only; the loopback control token stays a 0600 file (see the design note).
- 2026-10-24: The installed Codex changed from 0.163.0 to 0.164.0 between S01 test runs; the capability test
  failed as designed. The record was regenerated for 0.164.0 (one more client request, a changed notification
  schema; the server requests, which the adapter answers, are identical); the recorded fixtures are still valid
  against the new schema and the adapter tests pass. Real-provider behavior of 0.164.0 is unverified.
