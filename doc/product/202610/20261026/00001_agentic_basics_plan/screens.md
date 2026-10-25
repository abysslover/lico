# Screens: what the window shows

Status: v1.2 (v1.0 plus the measurements of POC-0 and S05, the D4 result and the input echo), v1.0 approved by the owner at G1 on 2026-10-26. The sketches below are **ASCII sketches of intent**, not final screens. When a phase ends, a capture of the real window is the reference. Labels inside the screens are in English, the language of the window. Shortcuts are proposals. Marks: `*` working · `-` idle · `!` your turn (approval wait) · `>` finished (unread).

Related documents: [planning](planning.md) · [requirements](requirements.md) · [test plan](test_plan.md) · [changes](changes.md).

**The window today (P01):** a menu bar on top (File, Session, View, Help), the session list on the left (280 px wide), the conversation and input on the right, and a shortcut help line at the bottom. The sketches show what is added on top of that frame.

## 1. Main window and memory per session (P02)

```
+---------------------------------------------------------------------------------------+
| File  Session  View  Help                                                             |
+------------------------------+--------------------------------------------------------+
| 1 * api-server        312 MiB| api-server | claude | work | opus | high               |
|     ~/work/api               | ~/work/api | working 2m 10s                            |
| 2 - docs               96 MiB| ------------------------------------------------       |
|     ~/work/docs              | you     fix the failing login test                     |
| 3 ! scratch            41 MiB| claude  I will start by reading the test file...       |
|     (scratch)  approval wait | claude  Found it: the token is compared before         |
|                              |         it is trimmed. Editing auth.py ...             |
|                              |                                                        |
|                              |                                                        |
| ---------------------------- |                                                        |
| total 449 MiB                | > _                                                    |
| sess 449 | host 38 | win 24  |                         claude | work | opus | high    |
+------------------------------+--------------------------------------------------------+
| Ctrl+N new session  Ctrl+A accounts  Ctrl+Tab next session  Alt+1-9 by number         |
+---------------------------------------------------------------------------------------+
```

- Session row: number, state mark, name, and **at the right end the memory of that session** (`312 MiB`). The second line is the work folder; a session made without a folder shows `(scratch)`.
- Memory colour: amber above 1 GiB, red above 2 GiB. A tooltip explains ("private memory of the agent and its descendant processes; the resident memory (RSS) is shown next to it; programs detached from the session are not counted").
- Two total lines at the bottom of the list: the total and its parts (sessions sum, host, window).
- Header line at the top right: name, provider, account, model, effort; below it the folder and the activity (`working 2m 10s`).
- Input box (`> _`): with several lines it grows up to 8 lines; Enter sends and Shift+Enter inserts a newline (IN-02). Over the limit nothing is sent and a reason appears under the box (IN-06).
- What you send appears in the conversation at once as lines that start with `>> ` (tool output keeps `> `): the first 12 lines or 2 KiB, and when more was sent one line says how much is not shown ("the agent received all of it") (IN-08, SC-10).

## 2. New-session screen (P02; the worktree row arrives in P06)

```
+- New session -----------------------------------------------------------------------------+
| Provider   [ Claude Code ]   Codex                                                        |
| Account    [ work ]          personal          (Ctrl+A manage accounts)                   |
|                                                                                           |
| Folder     ~/work/api_                                                                    |
|            recent:  ~/work/api   ~/work/docs   ~/src/lico   ...                           |
|            [ ] start in a new git worktree      branch: ______________   (P06)            |
|                                                                                           |
| Name       api-server                                                                     |
| Model      [ opus ]   sonnet   haiku                (from the installed Claude Code)      |
| Effort     low   medium   [ high ]   xhigh   max                                          |
| Agent asks [ skip its own prompts ]   ask in this window      (Lico approvals stay)       |
|                                                                                           |
|            [ Start ]      [ Cancel ]            Tab next field   Enter start   Esc cancel |
+-------------------------------------------------------------------------------------------+
```

- One screen: Tab moves between fields and Enter starts. The folder is typed or picked from the recent list. A folder that does not exist gets a one-line reason under the field.
- Model and effort choices come from the installed Claude Code (not hard-coded). Choosing Codex shows only the fields that provider accepts.
- "Agent asks" decides whether the agent's own permission prompts are skipped (default) or asked in this window. Approval of the sensitive actions Lico defines is always taken in the window, either way.
- The worktree row appears from P06 (it is not shown in P02).

## 3. Session menu (P02, P03)

```
+- Session menu (new entries below the line) --------------+
| Next session                                    Ctrl+Tab |
| Previous session                          Ctrl+Shift+Tab |
| Start screen                                         Esc |
| Session 1  claude / work  a1b2c3d                  Alt+1 |
| Session 2  claude / work  e4f5a6b                  Alt+2 |
| Resume session                                    Ctrl+R |
| Allow approval                                         Y |
| Deny approval                                          N |
| Copy selected lines                         Ctrl+Shift+C |
| Paste                                             Ctrl+V |
| -------------------------------------------------------- |
| Rename session...                                     F2 |
| Close session                                     Ctrl+W |
| Delete session...                                        |
| Send to session...                          Ctrl+Shift+S |
| Hand over conversation...                                |
+----------------------------------------------------------+
```

- The entries below the line are new. An entry that cannot act now stays dimmed as today and shows why (for example `needs two sessions`).
- File > New session (Ctrl+N) is unchanged.

## 4. Sending (P03)

Short:

```
+- Send to session ----------------------------------------------------------------------+
| What     [ Selected lines (24 lines, 3.1 KiB) ]   Draft text   Whole conversation      |
| To       1 api-server     [ 2 docs ]     3 scratch                                     |
|                                                                                        |
| Delivery  inline (3.1 KiB is at most 4 KiB)                                            |
|           The text goes into 'docs' as a message that says it came from 'api-server'.  |
|                                                                                        |
|           [ Send ]    [ Cancel ]                                                       |
+----------------------------------------------------------------------------------------+
```

Long:

```
+- Hand over conversation ---------------------------------------------------------------+
| What     Selected lines   Draft text   [ Whole conversation (912 lines, 38 KiB) ]      |
| To       1 api-server     [ 2 docs ]     3 scratch                                     |
|                                                                                        |
| Delivery  as a file (38 KiB is more than 4 KiB). Nothing is cut.                       |
|           Saved under Lico's handoff folder; 'docs' gets the path and the first lines. |
|           The file may hold secrets from the conversation: it stays on this machine,   |
|           owner-only, and is removed after 30 days.                                    |
|                                                                                        |
|           [ Send ]    [ Cancel ]                                                       |
+----------------------------------------------------------------------------------------+
```

- What is sent is one of: the text in the input box, the selected lines, or the whole conversation. The receiving session is chosen from the list or by number.
- **Up to 4 KiB** is delivered as it is. **Anything longer is saved as a file**, and the window says so first. Nothing is cut; because the file may contain secrets, it stays on this machine and only its owner can read it.

## 5. What the receiving session sees (P03)

```
+- What the receiving agent gets (long content) -----------------------------------------+
| [from session 'api-server' (a1b2c3d) | 14:02 | conversation | 38 KiB, 912 lines]       |
| Long content, saved as a file. Read it before you answer:                              |
|   <handoff folder>/20261026/140212_a1b2c3d_to_e4f5a6b_conversation.md                  |
| First lines:                                                                           |
|   # Conversation of 'api-server' (records 1201-3350 of 3350; older ones pruned)        |
|   you: fix the failing login test                                                      |
|   claude: I will start by reading the test file...                                     |
| The file can be removed after 30 days.                                                 |
+----------------------------------------------------------------------------------------+
```

- The header makes clear that this is "sent by another session" and not text a person wrote. The window shows it as `from api-server` too.
- For long content only the path and the first lines arrive, and the receiving agent reads the file.

The start of a handoff file:

```
+- A handoff file (the start of it) -----------------------------------------------------+
| # Handoff: conversation                                                                |
| from:     api-server (a1b2c3d)        to: docs (e4f5a6b)                               |
| when:     2026-10-26 14:02:12         kind: conversation                               |
| covers:   records 1201-3350 of 3350   (records 1-1200 were pruned from the log)        |
| size:     38 KiB, 912 lines           nothing below this header is cut or summarised   |
|                                                                                        |
| ## you  14:00:41                                                                       |
| fix the failing login test                                                             |
| ## claude  14:00:58                                                                    |
| I will start by reading the test file...                                               |
+----------------------------------------------------------------------------------------+
```

- The file header states **what it contains** (the range of record numbers) and, if older records were already pruned, says so. A pruned record never looks like the whole.

## 6. Approval requested by an agent (P03 to P05)

```
+- Approval needed ----------------------------------------------------------------------+
| Session 'api-server' asks to send a message to session 'docs'.                         |
|                                                                                        |
|    Size: 1.2 KiB (inline)      Tool: lico_send                                         |
|    First line:  Please update the README for the new login flow ...                    |
|                                                                                        |
|    [ Allow once ]   Allow for this session   [ Deny ]          Y / N                   |
+----------------------------------------------------------------------------------------+
```

- It appears in the window when an agent sends to another session or creates one. Answer with the Y/N keys or the buttons. "Allow for this session" lasts only while that session is alive.

## 7. The real screen (P05)

```
+- Real screen access -------------------------------------------------------------------+
| Session 'api-server' asks to control your REAL screen (display :0).                    |
|                                                                                        |
|    It will be able to see everything on the screen and click and type anywhere.        |
|    Lasts 15 minutes, then stops by itself. Stop at once: Ctrl+Alt+Esc or the bar.      |
|    Lico logs each action (not the screen contents).                                    |
|                                                                                        |
|    [ Allow 15 min ]   [ Deny ]          (the private virtual display needs no ask)     |
+----------------------------------------------------------------------------------------+
```

A bar that stays at the top of the window while it is on:

```
+--------------------------------------------------------------------------------------+
| !! REAL SCREEN: 'api-server' is controlling this screen    13:42 left   [ Stop now ] |
+--------------------------------------------------------------------------------------+
```

- `Stop now` on the bar and Ctrl+Alt+Esc (proposal) stop it at once. After 15 minutes it ends by itself. It is released when the host restarts.
- The private virtual display is not asked about. The memory of a session using a virtual display counts in the session sum; how it is shown is decided in the P05 plan.

## 8. Attention marks and search (P06)

```
+---------------------------------------------------------------------------------------+
| File  Session  View  Help                                                             |
+------------------------------+--------------------------------------------------------+
| 1 * api-server        312 MiB| search: login_token                    3 of 7  [Esc]   |
|     ~/work/api   working 4m  | ------------------------------------------------       |
| 2 > docs               96 MiB| claude  ... the [login_token] is compared before       |
|     ~/work/docs   finished   |         it is trimmed ...                              |
| 3 ! scratch            41 MiB| you     why does [login_token] stay valid ...          |
|     (scratch)  asks approval |                                                        |
| 4 - api-wt-fix         58 MiB|                                                        |
|     ~/work/api-wt-fix (wt)   |                                                        |
|                              |                                                        |
| ---------------------------- |                                                        |
| total 507 MiB                | Enter next   Shift+Enter previous   Esc close          |
| sess 507 | host 38 | win 25  |                                                        |
+------------------------------+--------------------------------------------------------+
| * working   > finished (unread)   ! needs you   - idle   (wt) git worktree            |
+---------------------------------------------------------------------------------------+
```

- `>` is a session that finished in the background and has not been read; `!` is a session waiting for the user (an approval). `(wt)` is a git worktree session. Desktop notifications use `notify-send` only when it exists and can be turned off.
- Search is Ctrl+F (proposal). The same line switches between searching this conversation and searching all sessions. Choosing a result goes to that line of that session.

## 9. Not drawn yet

- A window in which the user looks into the virtual display the agent sees during computer use (proposed separately in the P05 plan if needed).
- Accessibility items and Wayland-specific behaviour (out of scope for this milestone).
