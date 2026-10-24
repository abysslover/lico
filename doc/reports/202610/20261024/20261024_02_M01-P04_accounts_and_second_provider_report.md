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
workload (agent-shaped UTF-8 lines, 10000-line transcripts) with this phase's host. **The host holds 10 accounts during
the GUI runs; no credential operation runs while a client is measured** (the credential command is started on demand and
ends, so no credential-path process exists in the measured tree); the clients do not show accounts yet.

| Target | A2 (primary) | C2 (fallback) | A2 / C2 in M01-P03 |
|---|---|---|---|
| PERF-01 idle process-tree RSS ≤ 50 MiB (3 launches, max) | 22.5 met | 46.3 met | 22.4 / 46.3 |
| same, PSS (reported separately) | 17.1 | 32.5 | 16.6 / 29.3 |
| PERF-02 growth 10→100 sessions ≤ 10 MiB (3 repetitions, max) | 0.27 met | 0.21 met | 0.12 / 0.27 |
| PERF-03 activation p95 ≤ 100 ms (n=100) | 62.4 met | 17.2 met | 65.5 / 15.7 |
| PERF-04 restart p95 ≤ 500 ms (n=100) | 113.8 met | 240.9 met | 109.6 / 226.5 |
| PERF-05 idle CPU ≤ 0.5% of a core (one 60 s window) | 0.00 met | 0.00 met | 0.00 / 0.00 |
| noisy case CPU: 100 lines/s, one window (reported separately) | **99.2** | 12.9 | 99.2 / 10.4 |
| cold start, best effort (median of 10, ms) | 133 | 261 | 121 / 253 |
| PERF-06 continuity, 30 s at 100 lines/s, GUI absent 15 s | no gap or duplicate | same | same |
| Korean composition (4 typing conditions) | exact | exact | exact |

No target was missed. The A2 noisy-case saturation (99%) and C2's thin PERF-01 margin (4.0 MiB) from M01-P03 are unchanged and
still open. Run-to-run differences are noticeable (A2 cold start 121, 192 and 133 ms and C2 restart p95 207 and 241 ms in complete runs of the same code).

| Host memory (window 64, 3 repetitions) | RSS | PSS | CPU (10 s) |
|---|---|---|---|
| 10 sessions, 10000-line transcripts | 3.5 MiB | 1.8 MiB | 0.0% |
| 100 sessions, 50-line histories | 11.4 MiB | 9.6 MiB | 0.0% |
| 100 sessions, 10000-line transcripts | 11.9 MiB | 10.1 MiB | 0.0% |
| 100 sessions, 5 producing, 10 views streaming | 12.5 MiB | 10.7 MiB | 4.0% |

| Host with accounts (3 repetitions) | RSS | PSS | provider processes |
|---|---|---|---|
| 0 accounts | 2.4 MiB | 0.7 MiB | none |
| 10 accounts | 2.4 MiB | 0.8 MiB | none |
| 60 accounts (the cap is 64) | 2.6 MiB | 1.0 MiB | none |
| 10 accounts, 20 simulated Claude sessions | 4.3 MiB | 2.6 MiB | 332 MiB (Python simulators, **not** the real provider) |

An account costs the host about 0.03 MiB. The real provider is far heavier: the idle Codex program measured 89 to 123 MiB in
M01-P03 (Claude Code was not measured: it may not be run here), so the memory of provider processes, not of Lico, decides how
many live sessions fit.

**Credential store (hand-written client).** The binary is 409 KB; one process peaks at 1.9 MiB resident in each of the three
states (10 runs each); a `helper` read takes 2.0 ms median and 3.9 ms p95 over 100 runs including process start; an `add` takes
86 ms median and 139 ms p95 over 20 (the keyring does the work); failing closed takes 1.6 ms median (p95 2.0) with a locked
keyring and 0.8 ms (p95 1.1) with no session bus, 100 runs each. Only gnome-keyring 46.1 was tried; KeePassXC and other
services are unverified. The `plain` transfer algorithm is used (the encrypted one is not implemented), so a secret crosses the
owner-only session bus unencrypted.

**Secrets in sinks.** A canary key was stored, attached to an account and used by a provider session, then searched for in the
files of the run and of the keyring (9 files), the host's standard error, the command lines and environments of the 4 processes
of the run, the protocol replies (including provoked error replies) and the output of a failed helper call: 0 occurrences. The
same scan function first found a canary planted in a file and confirmed that a process's environment is readable. The bundle and
the raw measurement files hold no canary value either (searched by hand). Not covered: crash output (none was produced), the
repository's fixtures, the argv and environment of `lico-cred` itself (it ends before they can be read; its argv is built by the
test and holds no secret), a keyring entry of another application, a locked keyring during a turn, and any sink a real provider
adds (its own logs and history under its home). A replayed or cancelled login callback and a refresh race do not exist here:
Lico implements no login flow and no refresh.

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

One read-only reviewer read the code, the raw data and the bundle (nothing was run by it). Its verdict: the credential path is
competently built (no plaintext fallback, no secret in argv or an environment, a correct hand-written D-Bus marshaller) and the
adapters hold up under the simulators; the weakness was the evidence, and it would not accept the phase until the GUI rows were
re-measured or relabelled and the other should-fix items were addressed. Findings and what was done:

| Finding | Action |
|---|---|
| The GUI rows were headed "accounts and credential store present" but the runs had neither (blocker) | the GUI runs now hold 10 accounts and say so; no credential operation runs during them |
| The secret-sink scan overclaimed: its control never used the scan pipeline; host stderr and error texts were not scanned; uncovered sinks were not listed | one function is used for the control and the run; the control also checks a process environment; stderr and provoked errors are scanned; uncovered sinks are listed above |
| Vacuous or indistinguishable assertions (the argv check, the locked versus no-default-keyring words) | the argv assertion was removed, the two states now have distinct wording and expectations |
| `credential_store_unavailable` and several plan cases untested | the refusal is tested (nothing is removed while the keyring is locked); the cases that cannot exist here are stated above |
| `account.remove` with `end` ignored purge failures; a purged session could get a provider back from a racing resume | the store is checked first, sessions are purged and verified before the key and the record go, and a purged session never gets a new process. **No test of the race itself** |
| Unmarked assumptions: permission requests may need a flag, the `api_error_status` mapping, `apiKeyHelper` in `-p`, 1024 unacknowledged inputs, oversize lines dropped | all listed in the capability record |
| The provider and every tool it runs can reach the whole Secret Service when its account has a credential | not changed; written into the capability record and the design note; an owner decision (see open items) |
| An unreadable accounts file meant "no accounts" (sessions would run in the host's own provider home); removing the last account had the same effect; a replaced key stayed in the keyring; a rebind kept the old issue | the host refuses to start on an unreadable file, a provider that has had accounts never runs unbound again, a replaced key is deleted, a rebind clears the issue; each tested |
| D-Bus client: unbounded nesting, no call deadline, a panic in `%`-unescaping; `lico-cred`: no metadata lock, silent truncation of a long secret, dumpable | bounded and fixed; **no fake-bus test exists** for a hostile peer |
| The provider session id became a command argument unchecked | validated (letters, digits, dash, underscore, no leading dash) |
| Credential latency shown as a median without p95, locked and absent states with 20 runs, a 0.00 for rows without provider processes | p95 reported, 100 runs, peak memory for all three states, the rows without processes removed |
| The real-turn pin was a substring match and a real program passed with `--provider-bin` was labelled simulated | exact token match; only a simulator script counts as simulated |
| Not changed: PERF-02 from 3 repetitions, continuity with the synthetic agent, the A2 noisy case without a lag check, the C2 emoji box | stated in the reports |

## Steps and commits

| Step | Commits | New tests |
|---|---|---|
| Approval record | `d0406ec`, `54477b2` (the owner's correction) | none |
| S01 credential store | `f8ed780` | 2 |
| S02 accounts and bound sessions | `ae0682f` | 2 |
| S03 Claude Code adapter | `a6f9bf3` | 2 |
| S04 failures and removal | `0cdda88` | 2 (+1 added after the review) |
| S05 hostile and reuse tests, measurement and bundle commands, review fixes | `39b947b`, `b49508e`, `2f6491e`, `8616069` and the commits between | 2 |

Eleven test functions (the plan proposed nine: S05 gained a hostile-provider test for the second adapter and S04 a test that a
corrupt accounts file stops the host) and 64 runs in the whole suite including the earlier phases; 64 passed on the final tree
(about 195 s).

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
- The measurement receipts carry two commits (see "Measurement runs").

## Measurement runs

| Run | Commit | Content |
|---|---|---|
| GUI (`run --text --accounts 10`) | `2f6491e` | all experiments for A2 and C2 with 10 accounts on the host |
| P02 (`p02 --text`) | `2f6491e` | host memory, backpressure, input integrity, hostile suite, GUI kill, view switch |
| P03 (`p03`) | `2f6491e` | the Codex adapter scenario, provider evidence, provider idle memory, dry run, simulated session |
| P04 (`p04`) | `8616069` | credential store cost, host memory with accounts, the Claude scenario, the sink scan, transcripts, hostile and reuse tests, capability records, dry runs |

The P04 stage ran at a later commit that changed only `p04.py` (the first P04 attempt at `2f6491e` stopped on a function the
review rewrite had dropped; the earlier complete sets of runs, at `39b947b` and `b49508e`, before the review fixes, are
superseded and gave the same picture).

Each started from a clean tree; the receipts carry the command, commit, binary SHA-256 values and workload.

## Decision

Nothing was missed, so no budget decision is required. The earlier recommendation stands (A2 primary, C2 fallback) with its two
open risks. M01-P05 (measured resources, failure recovery, packaging, owner end-to-end use) needs its own plan and approval.

## Open items for the owner

1. **First real Claude turn: done by you, 2026-10-24 15:34.** One turn through the adapter completed (3 records: session
   started with the model name, one text line, turn completed; 0 approvals). It shows that the provider's own login found through
   its config directory works while its `HOME` is an empty private directory, that a message written at process start is
   accepted and that the init, assistant and result messages map as documented. It does not touch the permission flow, the
   acknowledgment by replay, resume, the key helper or any failure; those stay unverified (the capability record has an
   `observed_in_an_owner_run` section). The result and transcript are kept in the bundle's `owner_real_turn/`.
   **First real Codex turn: also done by you, 15:35.** One turn through the adapter completed (4 records: turn started, the answer
   as two lines, turn completed; 0 approvals): the real login in its own config directory with an empty private `HOME`, the strict
   thread settings and the streamed answer all worked. Approvals, a command under the read-only sandbox, steering, resume and
   failures are unverified. Both runs wrote to the same output directory, so the second overwrote the first's files; the Claude
   files in the bundle's `owner_real_turn/` were reconstructed from what was read before (marked in the file), the Codex ones are
   the originals. Give each run its own `--out` directory.
   **Real turns (owner-run).** `cd src && PYTHONPATH=. python3 -m lico_measure real-turn --provider claude --accept-billing --turns 1 --out DIR`
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
4. **Decision for you: the keyring reaches the provider.** An account with a registered key passes the keyring's bus address to
   its provider so the key helper can answer; every process the provider starts (tools the model drives included) can then reach
   your whole Secret Service. A helper that talks to the host over a private socket would avoid it. Recommended before a real
   key is registered.
5. A2's noisy-case CPU, C2's thin PERF-01 margin, provider memory policy, selection, scrollback, accessibility, the encrypted
   Secret Service transfer, a fake-bus test, a test of the purge and resume race and the Claude permission flag question remain
   open.
6. Publication: the branches `phase/M01-P04-accounts-second-provider` and `doc/M01-P04-plan` need Pri. G3 and G4 are yours.
