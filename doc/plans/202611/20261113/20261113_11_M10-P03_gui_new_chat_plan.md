# M10-P03 Plan — GUI "+ New chat" Row and Modal

- Status: implemented (see report)
- Version: 1.0.0
- Approved: 2026-11-13 (G2 approved by the owner on 2026-11-13)
- Branch: `phase/M10-P03-gui-new-chat`
- Milestone: [M10](../../../milestones/202611/20261113/20261113_03_M10_chats_tab_rename_and_resume_milestone.md)

## Objective

A "+ New chat" button on its own row above `[..]  parent directory` (also when there is no parent) acting on the viewed directory (local or ssh), disabled with the reason for an unreachable directory; the directory page size loses one row. A small modal in the existing centered-modal style: step 1 the account list (label, provider, default marked, from `account.list`); step 2 `Resume` (default) or `New`. `Resume` asks `session.chats {limit: 1}` through the P02 rule and shows `Last Session : <provider> / <YYYY-MM-DD HH:MM> / <title> (<N> turns)` (`?` when `turns` is `null`, local time), and falls back to `New` with a one-line notice when there is none. `New` uses the quick-start defaults (`mode: "auto"`). An "Options..." link opens the full form prefilled. Keys: Up/Down account, Left/Right Resume or New, Enter, Esc; every part also works by mouse. The UI json exposes the modal state.

## Steps

- [x] S01: Red tests for the row, the modal states, keys, mouse, the Last Session line, the fallback, unreachable and ssh directories.
- [x] S02: Row, page-size change, modal, async preview, create paths.
- [x] S03: Report.

## Acceptance Criteria

| # | Criterion | Checked by |
|---|---|---|
| 1 | Button row present, disabled when unreachable | GUI test on Xvfb |
| 2 | Resume shows the line and opens the latest chat; New and fallback work | GUI test with mocked providers |
| 3 | Keyboard and mouse complete the whole flow; Options reaches the full form | GUI test |
