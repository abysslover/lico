# Security Of What The Person Does By Hand: The Terminal, The Browser, Logged-In Sessions, Keys

- Status: research note (2026-11-19), changes no code; the gaps at the end wait for the owner's choice
- Version: 1.0.0
- The owner's question (2026-11-19): "컴퓨터 사용 측면에서 bash를 열어서 타이핑을 한다거나 브라우저에 타이핑한다거나 브라우저에서 마우스를 움직인다거나 로그인이 유지된 세션에서 작업한다거나 사용자 키 등이 노출되지 않는 보안 문제라거나 이런 것들 다 고려되어서 구현되어 있나?" and the philosophy: usable by a person who hardly knows computers.
- Method: read from the code and the guide at `lico 0.8.0` (`40481b9`); what was **not** run or measured is marked (not verified).

## What is considered (read from the code)

| Concern | What holds today | Where |
|---|---|---|
| Typing in a terminal | The shell is the person's own: it runs as them, in a pseudo-terminal the host owns. It is **not fenced** and says so in the tab's header. No agent tool types into it: the eleven tools given to a session are `sessions`, `send`, `create`, `history`, `read_handoff`, `rename`, `capture`, `clipboard_read`, `clipboard_write`, `computer`, `llm_endpoints`; none reaches a terminal tab or a browser tab. Keys the window has are the person's (Ctrl+Shift+C/V/W). | `terminal/`, `mcp.rs` `TOOLS` |
| Who may open or drive a terminal | The `terminal.*` operations are socket-only: the owner-only Unix socket serves them, the loopback HTTP `POST /v1` refuses them. At most 8 terminals; a close sends the process group a hangup then a kill; no shell outlives the host. | `admin.rs` `SOCKET_ONLY_OPS`, `terminal/ops.rs` |
| What a shell is given | `LICO_*` variables are removed except `LICO_SOCKET`. | `terminal/ops.rs` `child_env` |
| Typing and the mouse in the browser | The keys and the mouse go to the page through a local link to the engine; the engine is an unprivileged, fenced process: it may read only its own folder and the system folders, write only its scratch folder and `/dev/null`, connect only to the host's guard port, and its whole environment is five names (its scratch folder as `HOME` and `TMPDIR`, a font file, the proxy); none of the person's variables, files or keys are visible to it. Its memory is capped. | `browser/profile.rs` `confinement` |
| What the page can reach on the network | Only through the guard (a local proxy with credentials made at each start); a tab for this machine's own services is a separate kind (`local`) and is the only one that may reach it. | `browser/profile.rs`, `lico-proxy` |
| Logged-in sessions of the browser | The browser keeps **no** login between runs: the engine's `HOME` is a scratch folder that is deleted when the engine stops (when the last tab closes); cookies live in the engine's memory only. | `browser/supervisor.rs` (`scratch_dir`, `remove_dir_all`) |
| Logged-in sessions of Claude and Codex | Each account has its own private folder (`0700`) that the provider fills when the person signs in with the provider's own flow; Lico never reads a provider's login files. | guide, first paragraph |
| Keys | Application credentials live only in the OS keyring (`lico cred`, failing closed when it is locked or missing); a local model server's key is set by `lico local-key set` from standard input, never as an argument. | guide, `lico cred`, `local-key` |
| Computer use by an agent | The tool `computer` acts on a private virtual display (Xvfb) by default with no prompt; the real screen needs a grant of 15 minutes given by the person; `capture` and `clipboard_read` ask each time and allow one call (valid 10 minutes), per target. | `mcp.rs`, `screentools/grants.rs` |
| The memory of pages | At most 100 pages (address, title, time) in an owner-only file (`0600`); never for a local tab; no user name in an address. | `pages.rs` `clean` |

## What is NOT covered, or only by the person's own care (the gaps)

1. **Remembered pages keep the whole address, the part after `?` and `#` too.** A sign-in link that carries a one-time code, or a page whose address holds a token, is then written to `browser_pages.json` and shown as a card in Chats. The file is owner-only, but a person who does not know this cannot know it. *Proposal:* keep the address without the part after `#`, and without the values of query names that look like secrets (token, code, key, secret, password, auth, session, sig, magic) or not remember such a page at all.
2. **A terminal's shell gets the host's whole environment** (minus `LICO_*`). Whatever the window was started with, and whatever the person's own shell start-up files export (on the owner's machine they export the keys of a private env file to every shell), is in every shell, and so in every agent started in it. A fence cannot be put around a person's own shell without taking away what a terminal is for, but the environment can be an allow-list (home, path, language, shell, display, the ssh agent's socket) and the person's own start-up files then add what they want. *Proposal:* an allow-list in `child_env`, with a test that a variable of the host that is not on the list does not arrive. (not verified: which tools of the owner's break without a variable)
3. **An agent started in a terminal is outside Lico's approvals.** `claude`, `codex` or `agy` run there under their own permission settings, as the person; a session started with Lico's wizard has Lico's `Skip agent prompts` / `Ask for approval` choice and its transcript. Level A and B of M23-P04 only show and reopen what runs; they add no approval. The `agy` picture must start plain `agy` (its own default), never with its "skip all permission" flag.
4. **`LICO_SOCKET` is given to the shell**, so a program in a terminal (an agent too) can use every socket-only operation: open and drive other terminals, the browser, sessions. It is the person's own socket and the same user could do it anyway, but a prompt-injected agent in a terminal inherits that reach. *Proposal:* decide whether the shell needs the socket at all (`lico terminal ...` from inside a terminal is the only known use).
5. **The browser does not keep a login**, which is safe and which a person who hardly knows computers will find tiresome: every time the last tab is closed they sign in again. Keeping cookies means writing a secret store; it needs a decision and a design (a file in the state folder encrypted with a key from the OS keyring, per site, with a menu entry that forgets all).
6. **The notice of the terminal is developer language** ("not fenced: it runs as you"). For the owner's philosophy (usable by someone who hardly knows computers) the tile and the first screen of a terminal should say in plain words what it can do, in a line.
7. **Not verified at all:** that the host writes no typed text to a log (the pages file and the session transcripts were read, the key path of the browser tab and of the terminal tab was not traced for a log line); the guard's rules for private addresses were not re-read in this note (they have their own tests in `lico-proxy`).

## Proposed order (if the owner agrees)

- **P08 (small, first):** gap 1, the remembered pages' addresses.
- **P09:** gap 2 and 4, the environment allow-list and the question of the socket.
- **Later, with the owner:** gap 5 (a kept login) and gap 6 (the plain-words notice, with the UI rule of the owner's philosophy).
