# M01-P04 report — keyring-only credentials, same-provider accounts and a second provider

- Status: in_review (inspection verdict pending; the status stays below `success` until the verdict is `accept`)
- Date: 2026-10-24
- Milestone: [M01](../../../milestones/202610/20261023/20261023_01_M01_measured_linux_foundation_milestone.md)
- Plan: [M01-P04](../../../plans/202610/20261024/20261024_01_M01-P04_accounts_and_second_provider_plan.md)
- Branch: `phase/M01-P04-accounts-second-provider`
- PR: not opened
- Inspection bundle: `private/manual_inspection/202610/20261024/003_M01-P04_accounts_second_provider/` (local, verdict `pending`)
- Gates: G2 approved 2026-10-24 as drafted; G3 pending; G4 pending. M01-P01 to M01-P03 gates G3 and G4 are also pending.

## What was built and what that does and does not show

- **A credential store that fails closed.** A Secret Service client written by hand (no new dependency, nothing downloaded)
  behind the `lico-cred` command stores, reads and removes a secret in the default keyring. The secret comes in on standard
  input only (a `--secret` argument is refused), leaves only through `helper` to a caller that asked for one reference, and
  never reaches a file, a log or an error. When the session bus, the Secret Service or the default keyring is missing, or the
  keyring is locked, every command exits 3 with the action a person can take and nothing is stored anywhere else. Metadata
  (opaque reference, provider, account, kind, masked label) is a 0600 file with no secret in it.
- **Accounts.** Stable opaque ids, editable labels, a default per provider, and one private home per account (mode 0700):
  a session bound to an account runs its provider with that account's own `HOME` and provider home and nothing of the host's.
  Changing the default affects new sessions only; a running session is never rebound (only a session whose provider is gone
  can move, as a new thread under a new generation). A provider that has accounts but no default refuses a new session
  instead of running unbound. A provider failure about the account (authentication, organization, hold, billing, limit) is
  shown against the account and its session and cleared by the next good turn; nothing switches account or authentication
  mode. `account.remove` names the affected sessions first, requires a choice (`detach` keeps running work, histories and the
  account's home; `end` purges its sessions), removes the key and the record, leaves other accounts alone, and reports the
  local removal, the key and the revocation at the provider (never performed) as separate outcomes.
- **A second provider.** Claude Code behind the same contract as the Codex adapter: one `claude -p` process per session
  speaking stream-json; an input is a user message acknowledged when the provider replays it with the uuid the host gave it
  (there is no steer); a permission request is one host approval answered at most once; a vanished process is
  `disconnected` and `session.resume` passes `--resume`. A key reaches the provider only through an `apiKeyHelper` that runs
  `lico-cred helper` (its command names the reference, never the key).

**Nothing in this phase ran against a real keyring, a real login or a real provider.** The keyring is a throwaway private one
(a private session bus and `gnome-keyring-daemon` 46.1 in a private home, canary values); every provider result is from a
protocol simulator (a test double). The installed Claude Code is 2.1.310, reported by the owner (the development session may
not run it). **The Claude message shapes are built from the documentation, not recorded:** the subtype of a permission
request (`can_use_tool`), the shape of the allow and deny response, the replayed user message that acknowledges an input and
the behavior of `--resume` with stream-json are unverified, as are the real failure shapes. If the provider never replays the
user message, inputs stay `submitted` (never resent); if it never answers a response, nothing is held by Lico beyond its
bounds. The real-provider behavior of Codex 0.164.0 (re-pinned during this phase after an update) is also unverified. Owner-run
scripts are the way to learn any of these; none has been run.

## Results

Targets are copied from the product specification. "met" describes this scope only (Xvfb: X11, no compositor, software
rendering; Wayland, GPU, other input methods and accessibility are not covered). The GUI and host runs are the M01-P03
workload (agent-shaped UTF-8 lines, 10000-line transcripts) with this phase's host (accounts and the credential store
present); the GUI clients do not show accounts yet.

| Target | A2 (primary) | C2 (fallback) | A2 / C2 in M01-P03 |
|---|---|---|---|
| PERF-01 idle process-tree RSS ≤ 50 MiB (3 launches, max) | 22.2 met | 46.0 met | 22.4 / 46.3 |
| same, PSS (reported separately) | 16.5 | 32.9 | 16.6 / 29.3 |
| PERF-02 growth 10→100 sessions ≤ 10 MiB (3 repetitions, max) | 0.15 met | 0.03 met | 0.12 / 0.27 |
| PERF-03 activation p95 ≤ 100 ms (n=100) | 66.9 met | 20.6 met | 65.5 / 15.7 |
| PERF-04 restart p95 ≤ 500 ms (n=100) | 117.3 met | 206.8 met | 109.6 / 226.5 |
| PERF-05 idle CPU ≤ 0.5% of a core (one 60 s window) | 0.00 met | 0.00 met | 0.00 / 0.00 |
| noisy case CPU: 100 lines/s, one window (reported separately) | **99.2** | 10.5 | 99.2 / 10.4 |
| cold start, best effort (median of 10, ms) | 192 | 242 | 121 / 253 |
| PERF-06 continuity, 30 s at 100 lines/s, GUI absent 15 s | no gap or duplicate | same | same |
| Korean composition (4 typing conditions) | exact | exact | exact |

No target was missed. The A2 noisy-case saturation (99%) and C2's thin PERF-01 margin (4.0 MiB) from M01-P03 are unchanged and
still open. Run-to-run differences are a few percent (A2 cold start 121 and 192 ms in two complete runs).

| Host memory (window 64, 3 repetitions) | RSS | PSS | CPU (10 s) |
|---|---|---|---|
| 10 sessions, 10000-line transcripts | 3.6 MiB | 1.8 MiB | 0.0% |
| 100 sessions, 50-line histories | 11.3 MiB | 9.6 MiB | 0.0% |
| 100 sessions, 10000-line transcripts | 11.9 MiB | 10.1 MiB | 0.0% |
| 100 sessions, 5 producing, 10 views streaming | 12.4 MiB | 10.6 MiB | 3.2% |

| Host with accounts (3 repetitions) | RSS | PSS | provider processes |
|---|---|---|---|
| 0 accounts | 2.4 MiB | 0.8 MiB | none (true zero) |
| 10 accounts | 2.5 MiB | 0.9 MiB | none |
| 60 accounts (the cap is 64) | 2.6 MiB | 1.0 MiB | none |
| 10 accounts, 20 simulated Claude sessions | 4.3 MiB | 2.5 MiB | 333.6 MiB (Python simulators, **not** the real provider) |

An account costs the host about 0.03 MiB. The real provider is far heavier: the idle Codex program measured 89 to 123 MiB in
M01-P03 (Claude Code was not measured: it may not be run here), so the memory of provider processes, not of Lico, decides how
many live sessions fit.

**Credential store (hand-written client).** The binary is 407 KB; one `helper` process peaks at 1.9 MiB resident; a read takes
1.7 ms median over 100 runs including process start; an `add` takes 72 ms median over 20 (the keyring does the work); failing
closed takes 1.5 ms with a locked keyring and 0.8 ms with no session bus. Only gnome-keyring 46.1 was tried; KeePassXC and other
services are unverified. The `plain` transfer algorithm is used (the encrypted one is not implemented), so a secret crosses the
owner-only session bus unencrypted.

**Secrets in sinks.** A canary key was stored, attached to an account and used by a provider session, then searched for in the
files of the run and of the keyring (8 files), the command lines and environments of the 4 processes of the run, the protocol
replies and the credential list: 0 occurrences, after the scanner was shown to find a canary planted in a file. The bundle and
the raw measurement files hold no canary value either. Not covered: crash output (the host produced none), and any sink a real
provider adds (a provider's own logs and history under its home).

**Adapters against the simulators** (counted from the provider's received log and the durable session log, not from replies):

| Count (2 sessions, 1000 inputs, 9 provider restarts, 150 stale and foreign attempts, 50 turns with 100 approvals) | Claude | Codex |
|---|---|---|
| inputs applied zero times / more than once | 0 / 0 | 0 / 0 |
| inputs sent to the provider zero times / more than once | 0 / 0 | 0 / 0 |
| inputs misrouted to the other session; refused inputs that reached the provider | 0; 0 | 0; 0 |
| final state of every input | 1000 observed | 1000 observed |
| approvals raised / answered / provider responses (50 allow, 50 deny) | 100 / 100 / 100 | 100 / 100 / 100 |
| forged, stale and repeated approval answers refused | 100 / 100 / 100 | 100 / 100 / 100 |

Not checked: that an approval was answered for the session that raised it (both sessions share one received log). An input in
flight when a provider vanishes stays `accepted` or `submitted`; the host never resends it.

**Hostile provider and runtime reuse.** The Claude hostile test sends non-JSON, a field of the wrong type, a forged status line
inside a tool command, a 3 MB line, 300 permission requests with 5 KB commands full of escapes (one id repeated, an unknown
subtype, a 100 KB id, an oversize input) and a canary prompt: the session keeps running, every record is at most 4096 bytes
without control characters, 64 approvals are pending with short prompts that end with an ellipsis, the rest are refused to the
provider, no oversize id is echoed, host memory grows by less than 24 MiB, and the canary is only in the session log and the
provider's received log. The runtime reuse test shows that no provider process serves two sessions, every session has its own
private workspace, an account's home is shared only by that account's sessions, and the permission context is the strict one in
all of them. **Decision on runtime reuse:** this phase keeps one provider process per session, which is what guarantees
isolation; sharing one Codex process between threads of the same account, workspace and permissions would save about 100 MiB per
session but cannot be verified without a real provider, so it is left to a later phase.

## Independent review

Pending: one read-only reviewer is checking the code, the raw data and the bundle; this section is updated with its findings
and their fixes.

## Steps and commits

| Step | Commits | New tests |
|---|---|---|
| Approval record | `d0406ec`, `54477b2` (the owner's correction) | none |
| S01 credential store | `f8ed780` | 2 |
| S02 accounts and bound sessions | `ae0682f` | 2 |
| S03 Claude Code adapter | `a6f9bf3` | 2 |
| S04 failures and removal | `0cdda88` | 2 |
| S05 hostile and reuse tests, measurement and bundle commands | `39b947b`, `b49508e` | 2 |

Ten test functions (the plan proposed nine: S05 gained a hostile-provider test for the second adapter) and 63 runs in the whole
suite including the earlier phases; 63 passed on the final tree (about 190 s).

Red evidence: S01's tests were red because the program did not exist, then because of a bug in a test; S02 to S04 were red
(unknown operation) before the code; S05's two tests passed on their first run after the code they check was written, so red
was observed by mutation. Mutations caught: a secret copied to a file, a plaintext fallback, a shared `HOME`, every session in
the default account's home, a rebind of a running session (it had survived until a test checked that a refused rebind touches
nothing), no acknowledgment of an input, a resume without `--resume`, a wrong request id in an answer, a silent fallback to an
unbound session, `detach` ending sessions, the home always removed, an issue never cleared, no approval bound, an unbounded
request id. The mutation outputs are local scratch files.

## Deviations and corrections

- The owner first asked for experiments against the keyring registered for their accounts, then recalled that Claude and
  Codex sign in through the browser and keep their own login; the plan was corrected accordingly (provider-managed login in a
  private home per account; the keyring only for what Lico owns).
- The public-source guard refused test canaries that looked like API keys; they were changed to a different shape.
- The installed Codex changed from 0.163.0 to 0.164.0 during the phase; the capability record was regenerated (one more client
  request, a changed notification schema, identical server requests).
- The first accounts memory row used 100 accounts, above the cap of 64; it now uses 60.
- The loopback control token stays a 0600 file: moving it into the keyring would make the optional listener depend on a
  Secret Service that a headless host may not have. This departs from the letter of AUTH-01.
- The measurement receipts carry two commits: the GUI, host and Codex-scenario stages ran at `39b947b` and the P04 stage at
  `b49508e` (a one-line fix to the accounts row in between).

## Measurement runs

| Run | Commit | Content |
|---|---|---|
| GUI (`run --text`) | `39b947b` | all experiments for A2 and C2 |
| P02 (`p02 --text`) | `39b947b` | host memory, backpressure, input integrity, hostile suite, GUI kill, view switch |
| P03 (`p03`) | `39b947b` | the Codex adapter scenario, provider evidence, provider idle memory, dry run, simulated session |
| P04 (`p04`) | `b49508e` | credential store cost, host memory with accounts, the Claude scenario, the sink scan, transcripts, hostile and reuse tests, capability records, dry runs |

Each started from a clean tree; the receipts carry the command, commit, binary SHA-256 values and workload.

## Decision

Nothing was missed, so no budget decision is required. The earlier recommendation stands (A2 primary, C2 fallback) with its two
open risks. M01-P05 (measured resources, failure recovery, packaging, owner end-to-end use) needs its own plan and approval.

## Open items for the owner

1. **Real turns (owner-run).** `cd src && PYTHONPATH=. python3 -m lico_measure real-turn --provider claude --accept-billing --turns 1 --out DIR`
   (and `--provider codex`). It refuses an installed version other than the pinned one (Claude Code 2.1.310, Codex 0.164.0), denies
   every approval, writes an owner-only transcript and receipt, runs the provider with an empty private `HOME` and the provider's
   own config directory passed as a path, and purges its session. Residual risk as in M01-P03: other files the account can read
   are reachable by a command the provider may run without asking. The first real run is also what confirms or corrects the
   unverified Claude message shapes listed in `src/host/adapters/claude/capabilities.json`.
2. **Real login per account.** To use two accounts of one provider, create them with `account.create`, then sign in through the
   browser with the provider's own command inside each account's home (`CLAUDE_CONFIG_DIR` or `CODEX_HOME` set to that home).
   Lico does not do this for you and never reads the result.
3. **Keyring.** Whether a Secret Service runs in your session is unknown here. Without one, `lico-cred status` says what to
   start; only a Lico-registered API key needs it.
4. A2's noisy-case CPU, C2's thin PERF-01 margin, provider memory policy, selection, scrollback, accessibility and the
   encrypted Secret Service transfer remain open from earlier phases.
5. Publication: the branches `phase/M01-P04-accounts-second-provider` and `doc/M01-P04-plan` need Pri. G3 and G4 are yours.
