# M24 — One Touch To Start, Few Words, And Every Pane Can Be Driven By An Agent

- Status: in_progress (2026-11-24); G1 approved by the owner in the chat on 2026-11-24 ("승인, P01부터"); P01 (the Phase 0 report, no code) is done and the owner's picks are in (the menu as the lead classified it, the pane tools to every session, the account screen as cards with five small pictures); nothing is implemented yet. The research below is read from the code and from the names (never the values) of the agents' own settings.
- Version: 0.4.0
- Project: Lico
- Updated: 2026-11-24
- Follows: [M23](../20261118/20261118_02_M23_terminal_agents_milestone.md) (P08 and P09 are the last security phases of it); level C of M23-P04 (`agy` as a Lico session) comes after this milestone, in the owner's order of 2026-11-24.

## The owner's words (the source, in the chat, 2026-11-24)

The order: "P08, P09, UI 단순화 -> 레벨 C로 가면 됨". Then, asked what the UI simplification is:

> UI 단순화는 가령 현재 네 세션이 있는 사용하는 orca의 design을 참고해보면, 거의 단순한 화면이 보임. 하지만, 각 기능들은 사실 좀 누르기 어렵고 복잡함. agent cli가 해야하는 것은 다른 에이전트 별 세션 만들어 주기, 다른 에이전트 세션 간 연결해주기, 영속성 관리해주기, 너무 커지는 세션 관리(필요시 강제 종료) 등을 해줘서 사용자가 쓸 때는 마치 그냥 계속 연속적으로 작업할 수 있듯이 하는 것임. 현재는 가령 New Home session 등 세션 만드는 것까지는 아이콘화 단순화가 되었음. 하지만 계정으로 오면 다시 텍스트가 보이고, 메뉴바도 꽤 많은 내용들이 있는데 다 필요한 것은 아님.. 그리고 근본적으로 마우스 클릭이라거나 키보드 치기, 다른 세션에 대화 보내기 등은 사용자가 주로 요청을 직접 안하더라도 기본적으로 lico ui에 보이는 모든 창에 대해서 실현이 가능해야함(bash, agent 창, browser 등). 그래서 이게 UI 문제 해결이고, 가령 새 세션 해서 codex를 치면 그 전까지 아이콘이다가 다시 텍스트 위주 창이 보임, 게다가 두번이나 창을 넘겨야 세션이 시작됨. 기본 세션 등은 공통 관리소에서 이미 각 에이전트 별로 관리되므로 새로운 세션 정보는 codex 전용 세션 config 파일 경로 등을 lico가 조사해서 알려주면 됨. 이것은 codex에 대한 설명일 뿐 claude나 agy나 다 마찬가지임.

(Three typing slips are read by the lead: `너므` as `너무`, `치키` as `치기` (with a comma put after it) and `마찬 가지` as `마찬가지`; the rest is the owner's text.)

## What it means (the lead's reading; the owner corrects it)

| # | The owner's point | The lead's reading |
|---|---|---|
| R1 | The screen is near-simple; the functions are hard to press | The target is not fewer features but fewer things to read and press: pictures, a short list of what is on show, the rest one step away |
| R2 | What the agent CLI does: make sessions per agent, connect sessions of different agents, keep them persistent, manage sessions that grow too big (force a close when needed), so the person works as if it never broke | These already exist as functions (`create`, `send`, resume, the memory guard); this milestone does not add them but makes the person meet them as one continuous thing |
| R3 | Starting a session stops being icons at the account and again at the options; two screens must be crossed | **One touch**: a click on the Claude, `agy`, Codex or Local LLM picture starts the session at once; account, folder, name, model, effort and permission are filled in by Lico, not asked |
| R4 | The sessions of an agent are already kept in the agent's own common store: Lico should find the agent's own settings (the paths) and use what they say | For each agent Lico reads what the agent itself says it uses (model, permission mode, folder rules) and starts with that; the person changes it later in the session, not before it |
| R5 | The menu bar holds a lot that is not needed | A menu inventory, then a short list on show; this changes M22's rule "no menu function is removed" (the owner says not all are needed): which go is the owner's list, not the lead's |
| R6 | Clicking, typing and sending text to another session must be possible, by default, for every window the Lico window shows (bash, an agent, the browser...), even when the person does not ask for it | Every pane has an agent-side way to be read and driven, with the same operations a person has (the product rule of 2026-11-11: the window is for the person, the command line and the tool channel are for agents and scripts, and they do the same things) |

## What was found (read from the code and the settings' names, 2026-11-24)

- **Starting a session takes three screens** (`gui/newsession_wizard.rs`, `STEPS = 3`): the five pictures, then the account and folder (or the server), then the options and the start. Steps two and three are rows of words and cards (M22-P07). The Terminal picture is the only one that starts at once (M23-P03a).
- **The menu bar** has four menus (`File`, `Session`, `View`, `Help`) and **55 commands** (`gui/menu.rs`, the enum `Cmd`); M22-P04a made them a tree with no function removed.
- **What an agent can drive today** (`mcp.rs`, the table `TOOLS`, 11 tools): sessions (`sessions`, `send`, `create`, `history`, `read_handoff`, `rename`), the screen (`capture`, `clipboard_read`, `computer`: the private virtual display by default, the real screen only with a grant of 15 minutes) and `llm_endpoints`. **No tool reaches a terminal tab or the browser tab**: `terminal.*` are socket-only operations kept away from agents on purpose (the security note of 2026-11-19), and `browser.*` has a command line (`lico browser ...`) but no tool. So an agent can send text to another session but cannot type in the person's shell or click in the browser tab, which is what R6 asks for.
- **Where the agents keep their own defaults** (names only, no value was read): Codex has `config.toml` in its home folder; Claude has `settings.json` (top-level names: `model`, `availableModels`, `permissions`, `env`, `hooks`, `theme` among others); `agy` has a `settings.json` in its own folder. The sessions themselves are already read from each agent's own records (M11, M23-P04b). So the defaults R4 asks for exist as files; which names carry the model and the permission mode, for each agent and each version, is what Phase 0 has to measure.
- **The security tension R6 meets**: M23-P08 and P09 just narrowed what a shell and a remembered page hand out, and the note's gap 3 says an agent in a terminal runs as the person. Giving agents a tool that types into the person's shell is a command line as the person for whatever the agent reads (a page, a file, another agent's answer). The way the product already handles the screen tools is the model: they exist for every pane, and a use is asked (a card in the window) or granted for a time. This milestone must decide the same for the terminal and the browser.

## Proposed phases (the lead's draft; each needs its own G2 and its own prefix yes)

| Phase | What | The person can inspect |
|---|---|---|
| M24-P01 | **(done 2026-11-24, [report](../../../reports/202611/20261124/20261124_01_M24-P01_phase0_one_touch_menu_and_panes_report.md); the owner's picks came the same day)** **Phase 0 report, no code**: the menu inventory (every one of the 55 commands: what it does, where else it is reachable, whether the toolbar already has it); every word a person meets between "New session" and a running session, and on the account screen; per agent, which names of its own settings carry the model, the permission mode and the folder, and what Lico must still ask because the agent does not say; what "driven by an agent" means for each pane (session, terminal, browser, files, calls) and what each costs; then STOP for the owner's picks | a report with the inventory as a table the owner marks keep, hide or drop |
| M24-P02 | **(done 2026-11-24, [plan](../../../plans/202611/20261124/20261124_03_M24-P02_one_touch_start_plan.md); code `dee3dd9`; the owner's look is next)** **One touch**: the four session pictures start a session at once (the default account, the folder on the left, a name Lico makes, the agent's own model and permission); what a person may want to change is changed in the session afterwards, with a picture each, not before | click Claude: a running session in one step |
| M24-P03 | **The account screen in pictures**: the account list as cards with the provider's picture and a state disc, the sign-in as the shortest path the provider allows, almost no words | the account screen with no sentence on it |
| M24-P04 | **The menu on show is the owner's short list**; what is not on it is reached from one place (a search box or a "more" entry: to decide in P01) | the menu bar with the owner's list |
| M24-P05 | **Every pane can be driven by an agent**: tools for the terminal tab (read the screen, type, keys) and the browser tab (read, click, type) with the same grant or ask as the screen tools; the CLI and the tool channel say the same | an agent types `ls` into a terminal tab after the person granted it, and cannot without |
| M24-P06 | The guide, the README, the CHANGELOG, the report and the release | |

## Questions for the owner (the lead's recommendations first)

| # | Question | Recommendation |
|---|---|---|
| Q1 | **Gate on an agent that types or clicks in a pane** (R6) | **Decided by the owner, 2026-11-24: always allowed by default** (the lead had recommended a grant for a time per pane by one click on a card, as the real screen has; the owner chose otherwise). The lead's additions that are not gates (a cue in the pane while an agent types, a record in the audit log, one owner-only switch and a key to stop it) are in the P01 report, section F, for the owner to keep or drop; which sessions get the tools is open |
| Q2 | **Defaults of a one-touch session** (R3, R4) | Account: the default account of the provider (else the one used last). Folder: the one on the left. Name: made by Lico. Model, effort, permission: what the agent's own settings say, else the agent's own default; **never** the "skip all prompts" mode unless the person turned it on in the agent's own settings |
| Q3 | **Where does the person change what they did not choose?** | In the session tab, by pictures in its header (model, permission, memory), not before the start |
| Q4 | **The menu** (R5) | **Decided by the owner, 2026-11-24: the lead's classification** (show 9, key only 16, hide 21, drop 4: `New session options...`, `New terminal tab`, the nine session jumps, `Hidden files`); hide = reachable from one search entry, drop = removed with its test and its guide line |
| Q5 | **A session and a terminal as one kind of tab** (M23 Q4) | Not in this milestone: the panes stay different kinds, and what is common (the toolbar, the cues, the grant) is made common |

## Out of scope

- Level C of M23-P04 (`agy` as a Lico session), which comes next; the remote host (M18); a second window; Windows and macOS.
- Any change of what an agent is allowed to do **by itself** in its own session (the approvals stay each agent's own).

## When done

(Filled in at the milestone's close.)
