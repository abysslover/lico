# Owner end-to-end use of Lico (M01-P05)

This is for the person who owns the accounts. Lico never reads a provider's login files and never stores a provider
secret; each account only has a private directory that the provider itself fills when you sign in there.
A real session can be billed by the provider. Nothing here is run by the development session.

State lives in `$XDG_STATE_HOME/lico` (or `~/.local/state/lico`); the host socket is `$XDG_RUNTIME_DIR/lico/host.sock`.
Use the file you were given as `lico`; it has no other files and needs only the C library.

1. Start the window: `lico gui` (a host is started if none runs, detached from the window). `lico version` and `lico licenses` need no host.
2. Make an account per provider login you want, with a label you will recognise:
   `lico account create --provider codex --label Work` (prints the account id `aNNN...`).
3. Sign in once inside that account's private directory, with the provider's own command and your own browser:
   `CODEX_HOME=<state>/accounts/<id>/codex codex login` (Claude Code: `CLAUDE_CONFIG_DIR=<state>/accounts/<id>/claude claude`, then `/login`).
4. Start a session on it: `lico session create --kind codex --account <id>` (prints the session id). It appears in the window's list under the account.
5. Send work: `lico session send <session> "your text"`; watch with `lico session watch <session>` or in the window.
6. Approvals appear in the window (and `lico approval answer <session> <n> allow|deny`); the list shows which hidden session waits.
7. Close the window: the host and the sessions keep running; open it again with `lico gui`.
8. Stop the host (`lico status` prints its `pid`; send it SIGTERM, or SIGKILL to see a crash) and start the window again: every session is listed as
   **disconnected** with its history; nothing runs until you resume it: `lico session resume <session>`.
9. If an account stops working, the list shows it against that account and nothing switches by itself. `lico account affected <id>` names its
   sessions; `lico session rebind <session> <other-account>` moves one when you decide.
10. Remove an account: `lico account remove <id> detach` (its sessions stay, unbound) or `... end` (they end); the provider's remote login is not revoked.
11. Delete a session for good: `lico session purge <session>` (its process, log and workspace).

Not in this phase: creating sessions from the window (use step 4), Wayland-specific behavior, accessibility, a log checksum.
Write down anything that surprised you; that is the evidence the phase is judged on.
