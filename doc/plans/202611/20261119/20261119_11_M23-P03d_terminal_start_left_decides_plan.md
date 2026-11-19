# M23-P03d Plan — The Folder On The Left Decides Where A Terminal Starts

- Status: in progress (2026-11-19)
- Version: 1.0.0
- Branch: `phase/M23-P03d-terminal-start-left` from `phase/M22-P07-wizard-cards` (`e438701`); the prefix `phase/M23-P03d-` needs the owner's yes at commit time
- Milestone: [M23](../../../milestones/202611/20261118/20261118_02_M23_terminal_agents_milestone.md), a second fix after P03c
- The owner's words (2026-11-19, after P03c and P07): "아무리 시작 디렉토리를 왼쪽에서 결정해도 지금은 계속 ~에서 배시가 시작하고 있어." (however I decide the start folder on the left, bash keeps starting at `~`)
- This phase changes no operation, no setting and no file format: it is the order of the rule that picks the folder a new terminal starts in.

## What was found (2026-11-19)

- The rule of P03c (decision D1 there) put **the folder of the session in front first** and the folder Directories shows second. A person who has a session open (the ordinary case) and walks to another folder on the left therefore got the session's folder, not the one on the left; when that session's folder is the home (a session started at `~`) the terminal "always" starts at `~`.
- A probe with the window test rig reproduced it: a session started in `project`, Directories moved to `project/inner`, the Terminal picture: the shell started in `project`. The P03c tests never had a session in front together with a different folder on the left, so they could not see it.
- Not the cause (checked): the shell itself. The host starts the shell with the folder as its working directory (`current_dir`), and the person's own `.bashrc` was run in a pseudo-terminal the same way: the shell stayed in the folder it was given.
- The sidebar does not follow a session that was just started in this rig (it stays on the folder the window was started in or the one kept), so the two folders can differ for real.

## Decisions (the lead's defaults; the owner may change any)

| # | Decision | Why |
|---|---|---|
| D1 | The order is: **the folder Directories shows now**, then the folder of the session in front, then the last folder kept, else the home; each only when it is a folder of this machine | the owner's word: the left decides; a person who has not moved on the left still gets the session's folder when the left shows none of this machine's (an ssh host, Home, a failed listing) |
| D2 | The skip rules of P03c stay (an ssh host, the Home scope and a failed listing show no folder of this machine; a folder that is not there is skipped, the next is tried) | unchanged and measured |
| D3 | No new setting, no new operation, no version number; the guide's sentence is corrected | a one-rule fix |

## Steps

1. [ ] **Tests first**: the two unit tests of `terminal/view_tests.rs` change to the new order (red on P03c's rule: `left` the session's folder, `right` the folder on the left); one window test (`tests/test_terminal_tab.py`: a session in front, the left moved to another folder, the Terminal picture) red on the old build.
2. [ ] **The rule** in `terminal/view.rs`.
3. [ ] **Gates**: the changed-files suite, `cargo xtask check`, mutation checks of the order.
4. [ ] **The guide** says it (1.11.1).
