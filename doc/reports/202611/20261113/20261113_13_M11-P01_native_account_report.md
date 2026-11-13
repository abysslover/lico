# M11-P01 Report — The Native Account

- Status: accepted at G3 and merged to `main` (2026-11-14)
- Version: 1.0.1
- Branch: `phase/M11-P01-native-account` (from the tip of `phase/M10-P04-verification-and-0-2-6`; implementation commit follows the document commit `9d461e1`)
- Plan: [M11-P01](../../plans/202611/20261113/20261113_13_M11-P01_native_account_plan.md)

## What changed

- **`native.rs`** (new): the provider's own default home as a virtual entry with the reserved ids `native-claude` and `native-codex` (nothing stored). The state is the person's choice in `settings.json` (`native_claude`, `native_codex`: `on` or `off`), else open when no managed account of the provider exists (the first run, now named), else `ask`. `probe` counts transcript files from a directory listing (`projects/*/*.jsonl`, `sessions/**/*.jsonl`, bounded at 2,500, links not followed, no file opened, no credential file looked at); `account.list` reports counts only while the state is `ask`, at most every 30 seconds. The label shows the real default path: `Native (~/.claude)`, `Native (~/.codex)`.
- **One listing rule** (`chats::collect`): the old rule "default home only when no managed account exists" is gone. A managed account lists its own home; a Native id lists the default home (`native_off` while it is not open); no account named lists every account that is open, Native included. Items of Native carry `account: "native-claude"` and the label; open sessions without an account are Native items too.
- **Creation**: `session.create {account: "native-claude"}` is an unbound session (the existing `None =>` arm of `claude.rs` and `codex.rs`: `env_clear()`, then `HOME` and the provider's variable of the host and nothing of a managed account). `native_off` while closed, `bad_request` for the other provider's Native. `resume_latest` works on Native chats. Status entries of such sessions carry `native` (the label).
- **Settings and CLI**: `settings.set native_claude|native_codex on|off|ask` (`ask` takes the choice back), `lico settings --native-claude|--native-codex`, protocol schemas (`account.list` has a separate `native` array; `accounts` is unchanged).
- **Tool** `create`: a Native caller's new session of the same kind stays Native (it was moved to the managed default by the old choice rule); optional `account: "native"` (only that value is an agent's choice, any other value is ignored as every unknown argument is) asks for Native when the person turned it on, shown on the card as `account: Native (~/.claude)`.
- **Window**: the `+ New chat` dialog lists open Native entries after the managed accounts; the Chats tab shows them with their label (the entries dump now carries `account` and `account_label`); the header beside the input target, the menu line, telemetry and the list headings show `Native (~/.claude)` instead of `no account`; the first-run question (`gui/native_prompt.rs`: Yes, No, Ask later; keys Y, N, L, Esc, Enter is Yes) appears once per window start when a default home has chats and the choice is `ask`; View menu entries `Use existing Claude Code login (...) : on|off|not chosen` and the same for Codex.
- **Test hygiene**: `rig.Host` now starts every test host with a private synthetic `HOME` and without `CLAUDE_CONFIG_DIR`/`CODEX_HOME`, so no host test can read the owner's real homes (before, a host without accounts scanned the real default home). A full cold run with this change had no failure caused by it.

## Evidence (as measured)

| Check | Result |
|---|---|
| Rust unit tests | `native` 4 (ids, `~` display, counts-only listing with an unreadable credential file and a link, the Codex dated folders and the bound), `gui::native_prompt` 2, `gui::newchat` account filter; all `gui::` tests `203 passed` |
| `tests/test_native_account.py` | 5 passed: closed until chosen (`native_off`, nothing listed), choice per provider and `ask` takes it back, one rule for every account, resume in place for Claude and Codex with the process environment read from `/proc` (`HOME` and the default-home variable are the synthetic ones, no path of a managed account), the status label, a tab that already holds the chat is shown again, turning Native off keeps a running session, the first run without managed accounts still works and is labelled, the CLI choice with an unreadable credential file, the tool cases |
| `tests/test_native_first_run.py` (private Xvfb, mocked providers, synthetic homes) | 2 passed: Ask later stores nothing and does not return in that start, the dialog lists only managed accounts while closed, the View menu turns Native on and off; Yes opens Native in the dialog, the Chats tab and the header, and Resume starts the program with `--resume cl-nat-new` |
| Full changed suite on the staged tree | 505 tests in 97 files, 505 passed, 115.2 s, tree peak 1,851.6 MiB |

## Honest notes

- The new tests were written before the code but were not run red on their own; three of them needed a correction of my own expectations (the listing includes both providers; Codex stays open while only Claude has a managed account; the tool ignores a non-Native `account` value like any unknown argument).
- A first full cold run after the hygiene change had 10 failures: eight were the new `native` field of the status entries that the protocol schema did not know yet (fixed), one was the new module missing from `tests/testmap.toml` (fixed), one was the same schema. Nothing failed because of the synthetic `HOME`.
- Not exercised: a real Claude Code or Codex default home (by rule), the first-run question with a very large home (the count is bounded at 2,500 files, so very large homes show at most `2500`), and ssh hosts (Native is local only; a remote host's own default home is not offered).
- Memory margin: the cold peaks of M10 were `1,732` to `1,976 MiB`; this phase's suite run peaked at `1,851.6 MiB`.

## Next

P02: running-elsewhere detection and the empty-Resume hint, on a branch stacked on this tip.
