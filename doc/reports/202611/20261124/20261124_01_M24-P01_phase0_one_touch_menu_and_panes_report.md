# M24-P01 Report — Phase 0: What The Window Asks Of A Person, And What An Agent Can Reach

- Status: done (research, no code); waits for the owner's picks (section G)
- Version: 1.0.0
- Milestone: [M24](../../../milestones/202611/20261124/20261124_01_M24_one_touch_window_and_every_pane_driven_milestone.md) (draft 0.1.0)
- Read: the code at `701f73c` and the **names** (never the values) of the agents' own settings files on this machine. Nothing was run in the window, nothing measured; what is not measured is said in section H.

## The owner's decisions so far (2026-11-24, by the questions in the chat)

- **G1 of M24: approved**, "P01부터" (this report, no code).
- **Q1, the gate on an agent that types or clicks in a pane: "always allowed by default"** (the lead had recommended a grant for a time per pane; the owner chose otherwise). Section F says what that asks of the design.

## A. Starting a session: three screens, and where each row could come from

The wizard is three steps (`STEPS = 3`, `gui/newsession_wizard.rs`). Only the Terminal picture starts at once. For Claude, `agy`, Codex and Local LLM the person crosses two screens of rows (`page_of` puts every row of the one-page form on a step).

| Step | Row (its word today) | One touch could fill it from | When nothing is there |
|---|---|---|---|
| 1 | The picture (Claude, agy, Codex, Local LLM, Terminal) | the click itself | - |
| 2 | `Account` | the provider's default account (`F  make it the default for its provider` exists), else the one used last | no account yet: the sign-in path is the one screen that stays |
| 2 | `Server` (Local LLM) | the server used last, else the one found by the look for servers | none found: the look for servers is the one screen that stays |
| 2 | `Folder`, `recent` | the folder the left lists (the terminal and the `agy` picture already start there since M23-P04d); a Home session has none | - |
| 2 | `Worktree`, `Branch` | off (today's default was not traced) | - |
| 2 | `SignIn` | only when the account is not signed in | the sign-in path |
| 3 | `Name` | a name Lico makes (today the form asks) | - |
| 3 | `Model` | the agent's own `model` setting (section E) | the agent's own default |
| 3 | `Effort` | Codex: `model_reasoning_effort`; Claude and `agy`: no such name was seen on this machine | the agent's own default |
| 3 | `Permission` | Claude: `permissions.defaultMode`; `agy`: `toolPermission`; Codex: `sandbox_mode` (its approval policy is not set in the file here, so the agent's default) | the agent's own default; Lico's own "approval or auto" mode has to be mapped to these (not traced) |

So the three steps become one touch, and two cases keep a screen: **no account yet** and **Local LLM with no server found**. What a person may want to change afterwards (model, effort, permission, name) belongs in the session's own header, one picture each.

## B. The account screen: the words a person meets

`Ctrl+A` or the account icon opens a text list: one line per account (`1  claude / <label>  (default)  [<quota>]`), the line `A  add a new account (local or SSH remote)`, and, when there is none, `No account yet: press A to add one. Claude and Codex need an account; Local LLM does not.` A number opens the account's own page with five lettered actions: `S  sign in`, `R  rename this account`, `F  make it the default for its provider`, `D  remove it (its sessions stay, unbound)`, `E  remove it and end its sessions`, and the provider's sign-in state as a sentence (`provider says: ...`).

The picture version has the same functions with no sentence: a card per account (the provider's picture, a state disc for signed in or not, a mark for the default, the quota as a colour bar), a plus card for a new account (local or ssh), and, on a card, five small pictures for the five actions (the two removals asking once). The sign-in itself is the provider's own flow and stays as it is.

## C. The menu bar: 50 lines in four menus

The enum `Cmd` has 55 commands; the menus list 50 of them (49 fixed lines and the jump to one of up to nine sessions). The toolbar already has the plus (New session), Home, browser, Accounts and Files icons. M22-P04a made the menu a tree and promised that no function would be removed; the owner now says that not all are needed, which changes that rule: the owner's marks below decide what goes. **The lead's suggestion** is only a start: `show` (on the short menu), `key only` (no menu line, the key and the pane's own control stay), `hide` (reached from one search entry, to be decided), `drop?` (a duplicate of something that exists, or replaced by one touch).

50 lines: **show** 9, **key only** 16, **hide** 21, **drop?** 4.

| # | Menu | Entry | Key | Also reachable from | The lead's suggestion | The owner's mark |
|---|---|---|---|---|---|---|
| 1 | File | New session | Ctrl+N | plus icon of the toolbar | show | |
| 2 | File | New session options... | - | the one-page form: the same rows as the wizard | drop? | |
| 3 | File | Accounts | Ctrl+A | account icon of the toolbar | show | |
| 4 | File | Quit | Ctrl+Q | closing the window | key only | |
| 5 | Session | New Home session | Ctrl+Shift+N | Home icon of the toolbar | show | |
| 6 | Session | New browser tab | - | browser icon of the toolbar | show | |
| 7 | Session | New local browser tab | - | - | hide | |
| 8 | Session | New terminal tab | - | the Terminal picture of New session | drop? | |
| 9 | Session | Reopen closed tab | - | - | hide | |
| 10 | Session | Clear remembered pages | - | - | hide | |
| 11 | Session | Next session | Ctrl+Tab | the tabs | key only | |
| 12 | Session | Previous session | Ctrl+Shift+Tab | the tabs | key only | |
| 13 | Session | Start screen | Esc | - | hide | |
| 14 | Session | Session 1 to 9 (up to nine rows) | Alt+1..9 | the tabs and the left side | drop? | |
| 15 | Session | Resume session | Ctrl+R | the memory guard resumes by itself | hide | |
| 16 | Session | Rebind account... | Ctrl+Shift+B | - | hide | |
| 17 | Session | Permission mode: approval or auto | Ctrl+Shift+M | - | show | |
| 18 | Session | Rename session | F2 | - | hide | |
| 19 | Session | Close session | Ctrl+Shift+W | - | key only | |
| 20 | Session | Delete session | Ctrl+Delete | - | hide | |
| 21 | Session | Send to session... | Ctrl+Shift+S | - | show | |
| 22 | Session | Hand over conversation... | - | - | show | |
| 23 | Session | Find Local LLM servers... | - | - | hide | |
| 24 | Session | Allow approval | Y | the approval card | key only | |
| 25 | Session | Deny approval | N | the approval card | key only | |
| 26 | Session | Copy selected lines or text | Ctrl+C | - | key only | |
| 27 | Session | Cut selection | Ctrl+X | - | key only | |
| 28 | Session | Paste | Ctrl+V | - | key only | |
| 29 | Session | Virtual display: start or stop | - | - | hide | |
| 30 | Session | Stop real-screen control | Ctrl+Alt+Esc | (only while it is on) | key only | |
| 31 | View | Working directory files | Ctrl+E | Files icon of the toolbar | show | |
| 32 | View | Left sidebar: shown or hidden | Ctrl+B | - | key only | |
| 33 | View | Find in conversation | Ctrl+F | - | key only | |
| 34 | View | Search all sessions | Ctrl+Shift+F | - | key only | |
| 35 | View | Older output | Page Up | - | key only | |
| 36 | View | Newer output | Page Down | - | key only | |
| 37 | View | Newest output | End | - | key only | |
| 38 | View | Hidden files: shown or hidden | Ctrl+H | a button of the Files panel | drop? | |
| 39 | View | Group sessions: workspace or account | Ctrl+Shift+G | - | hide | |
| 40 | View | Internal events: shown or hidden | Ctrl+Shift+E | - | hide | |
| 41 | View | Text size: larger | Ctrl+= | - | hide | |
| 42 | View | Text size: smaller | Ctrl+- | - | hide | |
| 43 | View | Text size: 14 px (reset) | Ctrl+0 | - | hide | |
| 44 | View | Capture screen to input | Ctrl+Shift+P | - | hide | |
| 45 | View | Agent clipboard write: on or off | - | - | hide | |
| 46 | View | Desktop notifications: on or off | - | - | hide | |
| 47 | View | Claude's own sessions: shown or hidden | - | - | hide | |
| 48 | View | Codex's own sessions: shown or hidden | - | - | hide | |
| 49 | Help | Keys | - | - | show | |
| 50 | Help | About Lico | - | - | hide | |

## D. What an agent can reach in each pane today

The tool channel has 11 tools (`mcp.rs`, `TOOLS`). None reaches a terminal tab or the browser tab. The host already has the operations behind both, for the person's window and the command line.

| Pane | What the person does | An agent's tool today | The host's operations (socket only) and the command line | To add for "every pane" |
|---|---|---|---|---|
| Session (an agent's conversation) | types, answers approvals | `send`, `history`, `read_handoff`, `rename`, `capture` | `session.*` | nothing for sending and reading |
| Terminal tab | types, keys, selects, scrolls | none | `terminal.open/list/close/input/resize/watch/text`; `lico terminal open, list, close, send, show, resize` | one tool over the existing operations |
| Browser tab | clicks, types, goes to an address, scrolls | none | `browser.open/list/close/shot/show/resize/hide/go/nav/watch/input/reopen/forget`; `lico browser open, list, close, shot` (no verb for the input) | one tool over the existing operations; the command line needs the input and navigation verbs |
| Files panel, call rooms, the window's own menus and toolbar | the person | `computer` can click anything on the private display, or on the real screen with a grant | not traced here | to be traced in M24-P05's plan |

**The size of the tool list is the obstacle, not the operations.** `tools/list` is held to 5,120 bytes (`LIST_BUDGET`; kept by every tool phase since at least M09-P02) and stood at 5,103 with the clipboard-write switch on (4,849 off): 17 bytes of room. Three ways: raise the budget (every session pays for it at its start), make **one** tool `pane` with an action and a target and trim the descriptions to make room, or offer the pane tool only to the sessions that need it. The lead recommends the second; its size is not measured yet (an estimate would be a guess).

## E. The agents' own settings: what Lico can read (names only)

| Agent | File (in the agent's own folder) | Names seen on this machine that carry |
|---|---|---|
| Codex | `config.toml` | model: `model`; effort: `model_reasoning_effort`; sandbox: `sandbox_mode`; also `service_tier`, `web_search`, tables `projects`, `mcp_servers`, `features`; no approval-policy name was set |
| Claude | `settings.json` | model: `model`, `availableModels`, `modelSettings`; permission: `permissions.defaultMode`; also `env`, `hooks`, `theme` |
| `agy` | `settings.json` | model: `model`; permission: `toolPermission`, `allowNonWorkspaceAccess`; folder trust: `trustedWorkspaces`; also `verbosity`, `statusLine` |

Limits, said plainly: this is one file per agent, on one machine, at one version of each; the names of another version or another machine were not seen; no value was read. Two of these files can hold things that are not settings (Claude's `env`, Codex's `mcp_servers`): Lico must read only the named keys, never the whole file, and never write what it read into a log or a card.

## F. What "always allowed" asks of the design

The owner chose that a tool for typing and clicking in every pane is on by default. Said once and plainly: an agent that reads a page, a file or another agent's reply with instructions in it can be led to type into the person's shell, which runs as the person. P09 limits what that shell holds (its environment), nothing else. The owner chose this knowingly; the design below does not turn it into a gate.

1. **Which sessions get the tools**: every session of Lico (as for the other tools), or only the main agent (the one the memory guard names; as of M19 it is a name in `memory.json`, set by the person). *To the owner (G2).*
2. **The lead's additions, not gates** (the owner may drop any): (a) the pane's header shows for a while that an agent typed or clicked there, and which one; (b) every use is written to the host's audit log, as the screen tools' uses already are; (c) one owner-only switch (default on) and a key that stop it at once, as `Ctrl+Alt+Esc` stops real-screen control.
3. **The tool list** as in section D.
4. **The command line says the same** (the product rule of 2026-11-11: the window is for the person, the command line and the tool channel are for agents and scripts, with the same operations).

## G. For the owner's picks

1. **The menu** (section C): mark each line `show`, `key only`, `hide` or `drop`. With no mark the lead's suggestion stands.
2. **Which sessions get the pane tools** (section F, 1).
3. **The account screen** (section B): the cards and the five small pictures as written, or another shape.

After the picks: the plans of M24-P02 (one touch) and the rest, each with its own prefix yes.

## H. Not measured or not traced here

The defaults the form has today for each row; how Lico's "approval or auto" mode maps to each agent's own permission setting; the settings names of other versions and machines; the byte size of a `pane` tool; the Files panel, the call rooms and the window's own controls as targets of an agent; what a window test would need to prove "one touch".
