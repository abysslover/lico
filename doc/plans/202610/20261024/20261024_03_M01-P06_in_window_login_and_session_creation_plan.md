# M01-P06 — Session creation, account management and provider login inside the window (2026-10-24)

> Status: in_progress
> Milestone: [M01](../../../milestones/202610/20261023/20261023_01_M01_measured_linux_foundation_milestone.md)
> Branch: `phase/M01-P06-in-window-login`
> Approval: G2 approved 2026-10-24 as drafted with every recommendation. G3 and G4 are pending, as are G3 and G4 of M01-P01 to M01-P05.
> Motivation: after M01-P05 the owner ran the executable and found the window pointless: it lists sessions but cannot create one or sign in to a provider; both need the command line. A workbench whose window cannot start work does not meet its purpose. This phase makes the window sufficient for the everyday path.

## Basis

- M01-P05 shipped one executable with a first view, restart recovery and, on the command line, `account create`, `session create`,
  `session resume`, `session rebind`, `account remove`. The window has none of these (P05 report, open items).
- Providers sign in through the browser and keep their own credentials in a directory; Lico only gives each account a private directory and
  never reads the files inside. This phase keeps that rule.
- Provider facts from their official documentation (read 2026-10-24; exact versions are pinned in S01 before anything is built):
  - Claude Code: `claude` opens the browser on first launch; `claude auth login` signs in without a session; the login URL can be copied;
    when the browser cannot reach the local callback server the browser shows a code to paste at `Paste code here if prompted`;
    `CLAUDE_CONFIG_DIR` holds the credentials (`.credentials.json`, mode 0600 on Linux); `/status` shows the login row; a status command for
    scripts is **not confirmed** (to be checked in S01 against the pinned version's own help).
  - Codex: `codex login` opens the browser (ChatGPT OAuth); `codex login --device-auth` uses the device-code flow (needs a setting enabled in the
    ChatGPT security or workspace settings); `codex login status` prints the mode and exits 0 when logged in; `CODEX_HOME` holds `auth.json`
    (or the OS credential store, per `cli_auth_credentials_store`).
  - Sources: the Claude Code authentication page and the Codex authentication and CLI reference pages of the providers' documentation sites.

## Proposed outcome

1. **New session in the window.** A "new session" action: provider, account (or create one on the spot), then the session appears in the list
   and is selected. No host change: `session.create` and `account.create` exist.
2. **Account management in the window.** Rename (label), set default, remove with the explicit detach-or-end choice and the affected sessions named
   first, rebind a session. Existing host operations; no new host endpoint.
3. **Sign in from the window.** "Sign in" on an account starts the provider's own login command with the account's private directory
   (`CODEX_HOME` or `CLAUDE_CONFIG_DIR`) as a host-supervised process. The window shows the login URL (copy button; the browser opens only when the
   person clicks), a field for a pasted code where the provider asks for one, the outcome, and a cancel. Lico never reads a token, never
   writes the provider's files, and decides "signed in" only from the provider's own status command (or the login process's exit).
4. **Honest states.** An account shows `signed in`, `not signed in`, `login expired` or `unknown` (when the provider offers no status command
   for the pinned version); never a guess.

New host operations (the reason this plan needs the owner's re-check): `account.login.start`, `account.login.input` (the pasted code),
`account.login.status`, `account.login.cancel`, local socket only. One login per account at a time; the process, its output and the
pasted code live in memory only and end with the login, the cancel, a 5-minute timeout or the host; nothing about a login is written to the log,
the registry, a command line or the environment of another process. The login URL contains a one-time state and challenge, not a credential; it is
still not logged. API-key login is not offered here (keys stay keyring-only, as decided in P04).

## Proposed steps

| ID | Work after approval | Meaningful checks (proposed; not present yet) |
|---|---|---|
| S01 | Read the pinned provider versions' own help (login, status), record them in the capability records; the login supervisor in the host (process in the account's directory, URL extraction, pasted code, status, cancel, timeout) and the command-line `lico account login ID`, against a **fake login program** that prints a URL, waits for a code and writes a file | `test_login_runs_in_the_accounts_directory_and_nothing_of_it_reaches_log_registry_or_arguments`; `test_one_login_per_account_and_cancel_or_timeout_ends_the_process` |
| S02 | The window: new session, account list and actions, the sign-in panel (text fields with the existing input-method support), states from the host | `test_new_session_dialog_creates_a_session_bound_to_the_chosen_account`; `test_sign_in_panel_shows_the_url_takes_a_pasted_code_and_clears_it` |
| S03 | Re-measure every target at two scales with the new views (idle memory with the panels closed, activation, restart, idle CPU), the host's memory with a login in progress, the executable's size and checksums, a clean-clone reproducible build | existing measurement checks extended; no new test unless something is repaired |
| S04 | The owner's own sign-in and first session through the window with real accounts (evidence is the owner's record), the phase report, the inspection bundle, an independent read-only reviewer (announced first) and fixes | none (the owner's use is evidence, not a test) |

At most three new core tests per step; the actual count is reported. Each step observes red, implements the minimum, observes green, and
commits only green.

## Measurement contract and acceptance

The same targets as P05 (specification PERF-01 to PERF-07) apply to the window with the new views; a missed target stops the phase and returns a
decision with at least three alternatives (this time without an in-phase repair). Sample counts are not reduced.

| Criterion | Evidence | Fails or stays unverified when |
|---|---|---|
| Login hygiene | a canary code and URL searched for in the log, registry, standard error, command lines and environments after a fake login | any occurrence; a sink not searched is listed as not covered |
| No real credential in development | the supervisor is exercised only with the fake program; the owner performs the real sign-in | any real provider login by the development session |
| Window targets | as P05, two scales | a target missed or a scale missing |
| Owner's use | the owner signs in and starts a session of each provider through the window | claimed without the owner's record |

## Personal data & security

Login output and a pasted code are secret-adjacent: memory only, never logged, never on a command line, cleared when the login ends. The
provider's files stay unread. The window shows a URL only on the person's request path and opens a browser only on a click. A login process is
started without Lico's other environment (the allow-list used for the credential helper). Synthetic data only in tests.

## Decisions needed from the owner before approval

1. **Order.** Recommended: S01 command-line login first, then the window, so the supervisor is tested before any interface sits on it.
2. **Opening the browser.** Recommended: show the URL with a copy button and open the browser only when the person clicks. Alternative: open at
   once (convenient, but a click is the person's action).
3. **Codex device-code fallback.** Recommended: offer `--device-auth` as a second button when the browser callback cannot work, noting that it
   needs a provider-side setting. Alternative: browser only.
4. **Unknown status.** If a provider's pinned version has no usable status command, recommended: show `unknown` and let a first session's
   result decide, rather than read the provider's files.
5. **Spend.** Rough, unverified estimate: S01 4 to 6, S02 8 to 12, S03 3 to 5, S04 5 to 8 US dollars, about 20 to 30 in all with the reviewer.
   The harness meter has about 21 dollars left below its 148.67 cap; the owner asked to use what is left. Measurement counts are not traded for cost;
   if the meter runs out first, the phase stops at a green commit and says what is unmeasured.

## Out of scope

Anything beyond the owner's everyday path: plan-usage display, multiple windows, tray, Wayland-specific work, accessibility, API-key login,
revoking a login at the provider, P07 and later.

## Progress

- 2026-10-24: Draft prepared after the owner's use of the P05 executable. No branch, implementation, test, download or experiment exists. G2 pending.
- 2026-10-24: G2 approved as drafted with every recommendation. S01 to S04 are not started.
