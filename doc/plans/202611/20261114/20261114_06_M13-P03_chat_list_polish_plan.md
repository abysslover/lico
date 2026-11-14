# M13-P03 Plan — Chat List Polish

- Status: implemented (see report)
- Version: 1.1.0
- Approved: 2026-11-14 (G2 approved by the owner on 2026-11-14)
- Branch: `phase/M13-P03-chat-list-polish` (stacked on `phase/M13-P02-discovery-dialog-follow-up`)
- Milestone: [M13](../../../milestones/202611/20261114/20261114_02_M13_discovery_follow_up_chat_list_polish_and_test_split_milestone.md)

## Objective

A chat marked running elsewhere shows its title and `written 4m ago` (its `[elsewhere]` mark moves to the second line); every card says first when its chat was last written; the account label (`Native (~/.claude)`) is not repeated on every row: the default home says nothing and another account or an ssh host is named. (Changed from the first plan, a group heading: the "before" fixture showed that headings would alternate in the newest-first list, four of them for six rows, and would break the page-size calculation; see the report.) The running-elsewhere rule is not touched. A "before" screenshot from fixtures is taken first; before and after are shown to the owner.

## Steps

- [x] S01: The "before" screenshot; red-first test (window JSON: no repeated label, the live row's text, the age words) and pure functions for the card lines.
- [x] S02: Implementation in `gui/sidebar_chats.rs`; the "after" screenshot.
- [x] S03: Report `108`.

## Acceptance Criteria

| # | Criterion | Checked by |
|---|---|---|
| 1 | `Native` is not repeated on any row; another account or ssh host is named | window JSON test |
| 2 | A live row shows its title and how long ago it was written | window JSON test, screenshot |
| 3 | The running-elsewhere rule unchanged | existing chat tests |
