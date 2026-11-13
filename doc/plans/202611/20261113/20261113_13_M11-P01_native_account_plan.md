# M11-P01 Plan — The Native Account

- Status: in progress
- Version: 1.0.0
- Approved: 2026-11-13 (G2 approved by the owner on 2026-11-13)
- Branch: `phase/M11-P01-native-account`
- Milestone: [M11](../../../milestones/202611/20261113/20261113_04_M11_native_provider_home_chats_milestone.md)

## Decisions

- Native is a **virtual entry** with the reserved ids `native-claude` and `native-codex`; nothing is stored. The choice is `native_claude` / `native_codex` in `settings.json` (`on` | `off`; absent = not answered). The effective state is the setting, else `on` when no managed account of the provider exists (the legacy first run, now labelled), else `ask`.
- The label is fixed and shows the real default path: `Native (~/.claude)` (or the path of `CLAUDE_CONFIG_DIR`), `Native (~/.codex)` (or `CODEX_HOME`).
- A Native session is an unbound session: the existing `None =>` arm of `claude.rs` and `codex.rs` gives it `HOME` and the provider's variable of the host and nothing of a managed account (`env_clear()` comes first).
- First-run prompt: one combined dialog for the providers found (`Yes` turns on every provider listed, `No` turns them off, `Ask later` asks again at the next window start only); only counts are read before the answer (directory listings, no file content).

## Steps

- [ ] S01: Red tests: `rig.Host` default synthetic homes; Rust units (state, ids, label, counts-only probe); host tests (list, resume, create, session environment, `native_off`, the old rule gone); GUI tests (dialog list, first-run prompt).
- [ ] S02: `native.rs`, `collect` rule, `resolve_account`, `account.list` `native` array, settings and CLI, status label, MCP `account` choice.
- [ ] S03: Dialog entries, first-run prompt, Settings menu entries, header label.
- [ ] S04: Docs, report, mailbox.

## Acceptance Criteria

| # | Criterion | Checked by |
|---|---|---|
| 1 | Native listed and resumed in place; one rule for all accounts | host tests on synthetic homes |
| 2 | A Native session gets the default home and none of a managed account's variables | simulator environment log |
| 3 | First-run prompt reads counts only and its answer is kept | Rust unit and GUI test |
| 4 | No test touches the owner's real homes | synthetic homes in `rig.Host` |
