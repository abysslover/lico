# M23 — A Terminal In Lico: Run An Agent In It, And It Comes Into Lico

- Status: idea (2026-11-18); a plan is written after M22, from a research pass that is not done yet
- Version: 1.0.0
- Project: Lico
- Updated: 2026-11-18

## The owner's words (the source, in the chat, 2026-11-18)

"The next experiment is putting in a terminal. If an agent run in the terminal comes into Lico, things may become simpler still."

## What it means (the lead's reading, to be confirmed)

A terminal tab in the window runs a shell. When a person starts an agent there (`claude`, `codex`), the agent becomes a Lico session: it shows in the lists, can be sent to and called, and its chat is in the Chats tab, with no second step. Today a session is made by Lico (`session.create`); here it is made by the person in a terminal and Lico notices it.

## Questions the research must answer before a plan

1. How does Lico learn that an agent started in a terminal? Candidates: the terminal exports a Lico variable and the agent's own MCP configuration points at the host (the host already speaks MCP to the agents it starts); a wrapper command (`lico run claude`); watching the provider's own record files (M11 reads them already).
2. Terminal emulation: the window draws text with its own renderer. A terminal needs a pty, an escape-sequence parser and a grid. Which crate, what size and what memory (the suite's budget, the 2 GiB peak bound), and what it does to the frame cost.
3. Security: a terminal is a shell for the person; what an agent in it may do is the provider's own permission model, not the host's fence. The host must say so in the tab, as the browser tab says what its engine cannot do.
4. Does it replace the session tab for the agents Lico starts (one kind of tab for everything), or sit beside it? "Simpler" points to the first; it is a bigger change and the owner decides.

## What this milestone does not do yet

It has no phases, no plan and no branch. M22 comes first.
