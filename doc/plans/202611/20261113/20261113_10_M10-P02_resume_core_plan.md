# M10-P02 Plan — Resume Core: `turns`, One `latest_chat` Rule, `--resume-latest`, MCP `resume`

- Status: in progress
- Version: 1.0.0
- Approved: 2026-11-13 (G2 approved by the owner on 2026-11-13)
- Branch: `phase/M10-P02-resume-core`
- Milestone: [M10](../../../milestones/202611/20261113/20261113_03_M10_chats_tab_rename_and_resume_milestone.md)

## Objective

1. `turns` (user plus assistant visible-text messages, same extractors as the title scan) in `session.chats` items, computed only for the returned page, streaming scan bounded to 8 MiB and 100,000 lines per file, `null` beyond the bound or when unknown, small bounded cache keyed by (file, mtime, size). ssh: counted by the same bounded rule in the existing remote script if the fake-ssh fixture allows, else `null` (decided red first and reported). `session.chats` latency is measured before and after on the 2,500-file fixture.
2. One host rule `latest_chat(cwd, account, ssh_host)`: the first item of the same list function (same sort, scope and open-tab merge). `session.chats`, `lico chat latest`, the GUI preview and creation with `resume_latest` all use it; creation re-resolves on the host at that moment.
3. `session.create {resume_latest: true}` (mutually exclusive with `resume_id`): running tab -> reuse it (`already_open: true`); disconnected tab -> resume it; otherwise create with the chat's provider session. No chat: create a new session and reply `resumed: false` with a plain sentence; a hit replies `resumed: true` with `{provider_session, title, turns, updated_at}`.
4. CLI `lico session create --resume-latest` and `lico chat latest --cwd DIR --account ID`; MCP `create` optional `resume` (bool). Default off for both, because an agent silently inheriting an old context is surprising; the approval card says that the latest chat is continued if one exists.

## Steps

- [ ] S01: Red tests: `turns` (counts, null cases, bound, cache), latest rule, three provider resume paths for Claude and Codex, ssh fixtures, refusals.
- [ ] S02: `turns` and the cache; latency measurement.
- [ ] S03: `latest_chat`, `resume_latest` creation, CLI and MCP parity (`capabilities`, protocol schema, docs).
- [ ] S04: Report with the measured latency.

## Acceptance Criteria

| # | Criterion | Checked by |
|---|---|---|
| 1 | `turns` correct, bounded, `null` when unknown, no latency regression | tests and measurement |
| 2 | One rule serves list, CLI, GUI preview and creation | tests that compare them on one fixture |
| 3 | Three provider resume paths and ssh pass; no billable turn | tests on mock providers and fake-ssh |
