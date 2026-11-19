# M23-P04d Plan — The Folder The Left Lists Starts The Terminal And agy, Also In The Home Scope

- Status: in_progress (2026-11-20)
- Version: 1.0.0
- Branch: `phase/M23-P04d-left-folder-in-home-scope` from `main` (`fffdc08`)
- Milestone: [M23](../../../milestones/202611/20261118/20261118_02_M23_terminal_agents_milestone.md), a third fix of the start folder (after P03c and P03d)
- The owner's words (2026-11-20, after P04a, P04b and P04c): "agy 실행 디렉토리가 work directory와 관련이 없음" (the folder agy runs in has nothing to do with the work directory), and, asked which folder it should have been: **the folder the left lists** (the first of four choices; the others were the session in front's folder, a folder step in the wizard, a Chats card's folder).
- This phase changes no operation, no setting and no file format: it is one condition of the rule that picks the folder a new terminal (the Terminal picture, `Session > New terminal tab`) and `agy` (the second picture of New session) start in.

## What was found (2026-11-20)

- `agy` writes the folder it was started in into its own log (`workspaceDirs=[...]`): the three plain starts and the one resumed conversation the owner made from the new window were all in the home folder.
- The session in front of that window is a session with no folder of its own (a Home session). While one is in front the left is in the **Home scope** (`global_scope`), and the rule of P03c and P03d skipped the folder Directories lists in that scope ("the Global scope has no folder of its own"): the start fell to the last folder kept, which is the home.
- The scope and the list are not the same thing: in the Home scope Directories still **lists a folder** (`sidebar.cwd`, the one the person sees and can walk in), and a click on the Home row or on the card of a Home chat sets the scope without changing the list. A probe with the window test rig showed it: a Home session restored in front, the window started: scope Home, the list on `aaa`; the person clicks a folder: scope off, list on `aaa/bbb`, the terminal starts there. The ways in which the scope is on and the list is a folder that is not the kept one (the click on the Home row after a walk, the card of a Home chat) were never tested.
- Not the cause (checked): `agy` itself (an ELF program, no wrapper, no alias; it takes the folder it is started in as its workspace), the shell's rc files (no `cd`), the host (it sets the folder as the shell's working directory).

## Decisions (the lead's defaults; the owner may change any)

| # | Decision | Why |
|---|---|---|
| D1 | The folder the left **lists** counts in every scope of this machine: **Home too**. The order is unchanged: the folder the left lists, then the folder of the session in front, then the last folder kept, else the home; each only when it is a folder of this machine | the owner's word: what the left lists is the work directory |
| D2 | The other skip rules stay: an ssh host (a folder of another machine), a failed listing (`unreachable`), a folder that is not there (the next is tried) | unchanged and measured |
| D3 | No new setting, no new operation, no version number; the guide's sentence is corrected | a one-condition fix |

Limit to say in the guide: a person who clicks the Home row on purpose, wanting the home, still gets the folder the list shows (the list does not change with that click); the way to the home is the Home folder in the list or a terminal from a Home session with an empty list (then the last folder kept, then the home).

## Steps

1. [ ] **Tests first**: the unit test of `terminal/view_tests.rs` changes where it pins the old rule (two lines: the Global scope, with and without a session in front) and is red on the old condition; one window test (`tests/test_terminal_tab.py`: the left walks to a folder, the Home row is clicked, the Terminal picture and the `agy` picture) red on the old build.
2. [ ] **The rule** in `terminal/view.rs` (one condition removed, the comment says why).
3. [ ] **Gates**: the changed-files suite, `cargo xtask check`, clippy compared with the parent, mutation checks of the condition (the old condition back, the ssh skip, the unreachable skip).
4. [ ] **The guide** says it (1.14.1) and the plan is closed with its results.
