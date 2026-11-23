# M23-P09 Plan — A Terminal's Shell Is Given A Short List Of Names, Not The Host's Whole Environment

- Status: done (2026-11-24)
- Version: 1.0.0
- Branch: `phase/M23-P09-shell-environment-allow-list` from `phase/M23-P08-remembered-page-addresses` (`2fb8062`; `main` is `49fef2f`, P08 is committed and not pushed); the prefix `phase/M23-P09-` was approved by the owner at commit time (the question on the prefix, 2026-11-24) and is in the local guard; the code is `0e236f9`
- Milestone: [M23](../../../milestones/202611/20261118/20261118_02_M23_terminal_agents_milestone.md); the proposal is gaps 2 and 4 of [the security note](../../202611/20261119/20261119_12_person_facing_surfaces_security_note.md) (1.0.0)
- The owner's words (2026-11-24, in the chat): "남아 있는 P08, P09, UI 단순화 -> 레벨 C로 가면 됨" (P08, P09, the UI simplification, then level C); the note's proposed order puts P09 after P08.

## What this phase changes

A terminal's shell is started with the host's whole environment (minus the `LICO_*` names except `LICO_SOCKET`). Whatever the window was started with is in every shell and so in every agent started in it: the keys a person exports for one program, the tokens a launcher hands to its children, a proxy address with a password in it. Level A to C of M23-P04 put agents (`claude`, `codex`, `agy`) into that shell on purpose, so the shell is now where an agent that was prompt-injected is one step from the whole environment. This phase gives the shell a short list of names and the person's own start-up files add the rest.

## Decisions (the lead's defaults; the owner may change any)

| # | Decision | Why |
|---|---|---|
| D1 | **An allow-list in `child_env`** (`terminal/ops.rs`, the one place the shell's environment is made): the names `PATH`, `HOME`, `USER`, `LOGNAME`, `SHELL`, `LANG`, `LANGUAGE`, `TZ`, `TMPDIR`, `DISPLAY`, `WAYLAND_DISPLAY`, `XAUTHORITY`, `DBUS_SESSION_BUS_ADDRESS`, `SSH_AUTH_SOCK`, `EDITOR`, `VISUAL`, `PAGER`, `DESKTOP_SESSION`, `XMODIFIERS`, `GTK_IM_MODULE`, `QT_IM_MODULE`, the three names that point an interactive shell at the person's own start-up files (`ENV` of `sh`, `ZDOTDIR` of `zsh`, `INPUTRC` of the line editor; found by the first run of the window tests: they give `/bin/sh` its prompt by `ENV`, and the list without it broke seven of them), and every name that starts with `LC_` or `XDG_`; `TERM` and `COLORTERM` are Lico's own as before; names are matched exactly, in their case | the note's list (home, path, language, shell, display, the ssh agent's socket) plus what a graphical program started from the shell and the person's input method need; the two prefixes hold locale categories and the base-directory variables, which name places, not secrets |
| D2 | **Everything else of the host's environment does not arrive**, `LICO_*` included (`LICO_SOCKET` apart, D3) and `BASH_ENV` too, which names a file that every non-interactive `bash` a program starts would run, and the person's own start-up files (an interactive shell reads them as it always did) add what they want | a variable the person wants in every shell belongs in their own file, where they can see it; a variable that a launcher set for the window is not theirs to hand to an agent |
| D3 | **`LICO_SOCKET` stays** | `lico` finds the host at `$LICO_SOCKET`, else at `$XDG_RUNTIME_DIR/lico/host.sock` (`cli.rs`), and `XDG_RUNTIME_DIR` is on the list; the socket is the person's own (mode owner-only) and an agent that runs as the person can open it by that path or by reading the process table, so taking the name away closes nothing and breaks a host that runs on another path (a private home, a test). Said in the guide: an agent in a terminal runs as the person; what keeps it away from the person's other doors is its own permission setting, not Lico (gap 3 of the note) |
| D4 | **No setting for extra names in this phase** | one more thing to understand for a person who hardly knows computers; if the owner finds a tool that breaks, a setting follows (`terminal_env`, owner-only) |
| D5 | **The browser engine's environment is not touched**: it is five names already (the note) | nothing to change |

## Not covered, said plainly

- What the person's own start-up files export. On the owner's machine they export the keys of a private env file to every shell: an agent in a terminal sees them again, by the person's choice and not by Lico's.
- An agent in a terminal runs as the person and can read the person's files; this phase does not fence it.
- The host's own memory still holds its environment (only the shells are given the short list).

## Steps

1. [x] **Tests first**: `terminal/tests.rs` (every listed name and both prefixes arrive; a table of names that do not: keys, tokens, proxies, `LICO_*`, names that only start like a listed one, a listed name in another case; `TERM` and `COLORTERM` once), `tests/test_terminal_cli.py` (a real host started with canary variables: the shell sees none of them, and sees `HOME` and `PATH`). Red first.
2. [x] **The code**: `child_env`.
3. [x] **Mutation checks**, before the final green gate.
4. [ ] **Gates and documents**: `cargo test --bin lico`, clippy on the new code, the changed-files suite, `cargo xtask check`, the owner's build; the guide (the terminal section: what a shell is given, and that an agent in it runs as the person), the milestone row (2.14.0), `PLAN.md`, the index rows.

## Results (2026-11-24)

- **What the owner sees**: nothing in everyday use. A shell of a terminal tab no longer has the keys, tokens and proxy addresses that the window was started with; `PATH`, `HOME`, the language, the display and input method, the ssh agent, the editor and the shell's own start-up files are there, and so is everything the person's own start-up files export. An agent started in the shell (`claude`, `codex`, `agy`) sees only that.
- **Host**: `terminal/ops.rs` (247 lines): the list of 24 names and two families (`LC_`, `XDG_`), `given_to_shell`, `child_env`; no protocol operation, window or `Cargo.lock` change. The protocol note (0.1.11) and the guide (1.14.3) say what the shell is given.
- **A real finding on the way, said plainly**: the first commit attempt failed in the hook: seven window tests of `tests/test_terminal_tab.py` give `/bin/sh` its prompt through `ENV` (the file an interactive `sh` reads), and the first list did not carry `ENV`. That is the class of break the note called "not verified": a name that points a shell at the person's own start-up files. `ENV`, `ZDOTDIR` and `INPUTRC` are on the list now; `BASH_ENV` is left out on purpose (it names a file that every non-interactive `bash` a program starts would run). The unit test spells the dropped names too, `BASH_ENV` among them.
- **Tests (red first)**: the real-host test (`tests/test_terminal_cli.py`, a host started with canary variables named like keys and tokens, the shell asked about each by name and never made to print its environment) was red against the old code: the canaries reached the shell; the unit test `the_shell_gets_a_short_list_of_names_and_nothing_else_of_the_hosts_environment` spells the list out (32 kept, 32 dropped: the names that only start like a listed one, a listed name in another case, a name that only contains a family prefix, `LICO_SOCKETX`, `BASH_ENV`); the older shell test now expects a host variable that is not listed to be gone. Whole `cargo test --bin lico` 1038 passed, 12 ignored; the window tests of the terminal tab (12) and the real-host ones (7) pass.
- **Mutation checks (39 runs, all caught)**: every listed name changed, `LICO_SOCKET` not given, every `LICO_` name given, a prefix match or a case-blind match or a containing match in place of the exact one, no family, a family by containing, the two prefixes changed, no `TERM`, no `COLORTERM`, the host's own `TERM` arriving too. The first round (35 runs against the first list) was all caught too; it was run again against the final list.
- **Gates**: clippy as at the parent (155 warnings in `bin lico test`, none in the changed files), `cargo xtask check` passes, the hook's suite is green (one unrelated window-load flake, `test_account_lifecycle`, passed alone twice and on the retry).
- **Decisions that stand until the owner says otherwise**: `LICO_SOCKET` stays (D3: taking it away closes nothing); no setting for extra names (D4: the person's own start-up files are the way, and a setting follows if a tool of the owner's breaks); `tests/test_terminal_cli.py` pins `LANG` in the canary host so that the test does not depend on the runner's locale.
- **Not covered, said again**: what the person's start-up files export; an agent runs as the person and can read the person's files; the host's own memory still holds its environment.
