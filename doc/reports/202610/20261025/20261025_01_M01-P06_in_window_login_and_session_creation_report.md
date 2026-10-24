# M01-P06 report — session creation, accounts and provider sign-in inside the window

- Status: in_review (inspection verdict pending; the owner's own real sign-in is still to come)
- Date: 2026-10-25
- Milestone: [M01](../../../milestones/202610/20261023/20261023_01_M01_measured_linux_foundation_milestone.md)
- Plan: [M01-P06](../../../plans/202610/20261024/20261024_03_M01-P06_in_window_login_and_session_creation_plan.md)
- Branch: `phase/M01-P06-in-window-login`
- PR: not opened
- Inspection bundle: `private/manual_inspection/202610/20261025/005_M01-P06_in_window_login/` (local, verdict `pending`)
- Gates: G2 approved 2026-10-24 as drafted; G3 pending; G4 pending (also for M01-P01 to M01-P05).

## What was built

- **Sign-in supervised by the host** (`account.login.start`, `.input`, `.status`, `.cancel`; `lico account login ID [--device]`): the provider's own
  login command runs in the account's private directory with only that directory, a path and a locale in its environment (no display, no host
  variables), one login per account, at most 5 minutes. What it prints is cleaned to one address and "a code is expected"; a pasted code goes to its
  standard input only. Nothing of a login is logged, stored or put on a command line; the address and the code-request mark are dropped when
  the login ends; cancel, timeout and account removal end the process group.
- **The window** (primary client): Ctrl+N new session (provider, account or a new account with a label, Enter to start or L to sign in first),
  Ctrl+A accounts (sign in, make default, remove with the sessions named first and the same key pressed twice). The address can be copied
  (Ctrl+Shift+C) or opened (Ctrl+O) only on the person's key; a pasted code is cleared at once and not echoed to standard output.
  The input line is the one text field. No new host operation besides the four.
- **Guide**: [manual_end_to_end.md](../../../manual_end_to_end.md). `~/bin/lico` was replaced by this build (sha256 `c53e982f...`, reproducible build script).

## Results (Xvfb, X11, software drawing; 10 accounts; 10000-line transcripts; commit 93bbb6a)

| Target | Scale 1 | Scale 2 | Limit |
|---|---|---|---|
| Idle RSS of the client (own process tree) | 23.0 MiB | 34.0 MiB | 50 MiB |
| Hidden-session growth | 0.36 MiB | 0.25 MiB | 10 MiB |
| Activation p95 (n=100) | 70.6 ms | 71.5 ms | 100 ms |
| Restart p95 (n=100) | 63.7 ms | 71.9 ms | 500 ms |
| Idle CPU (n=1) | 0.02 % | 0.03 % | 0.5 % |
| Noisy case CPU | 1.0 % | 1.7 % | none set |

Host program alone: 6.0 MiB idle, 16.1 MiB with 100 sessions. During a sign-in the host grows from 5.4 to 5.6 MiB; the login process (a test double) is
10.4 MiB, counted apart. Executable 6.5 MB, links only libc, libm and libgcc_s, notices inside; clean-clone reproducibility test passes on this
commit. Restart recovery of 50 sessions: all `disconnected`, no gap, no loss. Tests: 81 pass (this phase added 7: login 3, window 2 plus
the tightened ones; red shown by mutation: a code written to standard error, a cancel that leaves the process, a typed code left in the input line, an address
copied without a key, an echo of the input line).

## Deviations, plainly

- S01 and S02 are one commit (7d28f4f): the guard refused the branch name until the owner allowed it and the work accumulated staged.
- The tests were written after the code for S01 and S02 and proved red by mutation rather than before it.
- The window cannot rename an account or rebind a session (no host operation for a rename; rebind stays on the command line).

## Independent review (read-only, one reviewer, 10 findings)

Fixed in 93bbb6a: an echo of the typed code to standard output; a held key counting as the removal confirmation, and a confirmation surviving other keys or
a failed lookup; a login's helper processes outliving a cancel; the address and code request staying after a login ended; a non-UTF-8 byte stopping
the reading of the login's output; account removal not ending a login; a click on another session carrying the input line along; a zombie after opening the
browser; tests that passed without the behavior (a second account must now be chosen, the client's output is searched, the leftover address is checked).

Not fixed, stated as limits: the window asks the host on its own thread and stands still while it waits (at most 2 s per poll, 8 s otherwise, 5 s for a
Codex status check); the browser receives the address as an argument (that is its purpose); the login output formats of the real Claude Code and Codex
were never seen by the development session, the supervisor looks for an `https://` address and the phrases "paste code" or "enter the code", so a different wording
shows no address or no code field until the owner reports it; `claude auth login` on a pipe instead of a terminal is unverified; Claude Code has no status
command known here, so its account shows no signed-in state; same-user processes can still ask the host for a login status.

## Open items for the owner

The first real sign-in of each provider through the window (the one thing that cannot be tested here); G3 for P01 to P06 and G4 for every branch; branch
publication; Wayland, accessibility and other input methods unverified.
