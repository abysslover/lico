# M13-P03 Plan — Chat List Polish

- Status: planned
- Version: 1.0.0
- Approved: 2026-11-14 (G2 approved by the owner on 2026-11-14)
- Branch: `phase/M13-P03-chat-list-polish` (stacked on `phase/M13-P02-discovery-dialog-follow-up`)
- Milestone: [M13](../../../milestones/202611/20261114/20261114_02_M13_discovery_follow_up_chat_list_polish_and_test_split_milestone.md)

## Objective

A chat marked running elsewhere shows its title and `written N min ago`; the account label (`Native (~/.claude)`) is shown once as a group heading where the account changes, not on every row. The running-elsewhere rule is not touched. A "before" screenshot from fixtures is taken first; before and after are shown to the owner.

## Steps

- [ ] S01: The "before" screenshot; red-first tests (window JSON: no repeated label, the live row's text; a pure function for grouping and the age words).
- [ ] S02: Implementation in `gui/sidebar_chats.rs`; the "after" screenshot.
- [ ] S03: Report `108`.

## Acceptance Criteria

| # | Criterion | Checked by |
|---|---|---|
| 1 | The account label appears once per group | window JSON test |
| 2 | A live row shows its title and how long ago it was written | window JSON test, screenshot |
| 3 | The running-elsewhere rule unchanged | existing chat tests |
