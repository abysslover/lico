# Owner end-to-end use of Lico (M01-P06)

Version: 0.1.1

This is for the person who owns the accounts. Lico never reads a provider's login files and never stores a provider secret; each
account only has a private directory that the provider itself fills when you sign in. A real session can be billed by the provider.
Nothing here is run by the development session, and the sign-in of Claude Code and Codex has only been tried against a test double:
what you see first is the evidence.

State lives in `$XDG_STATE_HOME/lico` (or `~/.local/state/lico`). Use the file you were given as `lico`; it has no other files.

## In the window (`lico gui`)

1. `lico gui` starts the window (and a host if none runs). On the first screen: **Ctrl+N** new session, **Ctrl+A** accounts.
2. **Ctrl+N**: press `1` (codex) or `2` (claude), then the number of an account, or `n` to make one (type a label, Enter).
   Then **Enter** starts the session, or **L** signs the account in first.
3. **Sign in**: the window shows the address the provider printed. **Ctrl+Shift+C** copies it, **Ctrl+O** opens it in the browser (only when
   you press it). If the provider asks for a code, paste it into the input line and press Enter; it is cleared at once and never echoed.
   For Codex, **D** switches to the device-code sign-in. **Esc** leaves and ends an unfinished sign-in.
4. **Ctrl+A**: pick an account for sign-in (S), default (F), or removal: **D** (its sessions stay, unbound) or **E** (ends them); the window names the
   sessions first and asks for the same key again. The provider's own login is not revoked.
5. Pick a session in the list; approvals, history, selection and copy work as before.
6. **A folder you choose** (Ctrl+N, the Folder row): Claude is started in it and reads its `CLAUDE.md`, but not the folder's own Claude settings
   (`.claude/settings.json`: hooks, environment, helper programs; nor its `.mcp.json`), because those would run without your decision. Your own
   Claude settings are used. A session without a folder works in a scratch folder Lico made, which has none of these.
   A Codex session in a chosen folder does not load the folder's `.codex/` configuration (MCP servers, hooks, notify, model providers) either: Codex
   trusts a folder when a thread may write in it, so Lico starts every Codex thread read-only and carries the mode in each turn. That is Codex's
   behaviour at 0.164.0; run the check again when Codex updates.
7. **Memory** (right end of each row, and one line at the bottom of the list): what the session's program and everything it started hold of their
   own (`RssAnon` + `RssShmem`, in M or G; amber from 1 GiB, red from 2 GiB). Put the mouse on a row and the bottom line gives the figure, the RSS
   beside it and the number of processes. A program a tool detached from the session (`setsid`) is not counted. The host looks only while a window is
   open and asks the system every 3 seconds at most.

## On the command line (same host)

- `lico account create --provider codex --label Work`, `lico account login ID [--device]`, `lico account auth ID` (Codex says if it is signed in;
  Claude Code: null), `lico session create --kind codex --account ID`, `lico session send ID TEXT`, `lico session watch ID`,
  `lico session resume ID`, `lico session rebind ID ACCOUNT` (not in the window yet), `lico session close ID` (stops the agent, keeps the session to resume), `lico session delete ID` (the same as `purge`), `lico account remove ID detach|end`.
- Restart check: `lico status` shows the host pid; stop it (SIGTERM, or SIGKILL for a crash) and start `lico gui` again: every session is
  **disconnected** with its history until you resume it.

Not in this phase: renaming an account, rebinding from the window, Wayland-specific behavior, accessibility. Write down anything that surprised you
(especially the first real sign-in of each provider: the address, a code prompt, a failure); that is the evidence the phase is judged on.
