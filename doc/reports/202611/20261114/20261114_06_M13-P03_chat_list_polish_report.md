# M13-P03 Report — Chat List Polish

- Status: accepted at G3 and merged to `main` (2026-11-14)
- Version: 1.0.1
- Branch: `phase/M13-P03-chat-list-polish` (stacked on `phase/M13-P02-discovery-dialog-follow-up` at `834504e`; commit `d9baeab`)
- Plan: [M13-P03](../../plans/202611/20261114/20261114_06_M13-P03_chat_list_polish_plan.md)

## What the "before" showed

I cannot see the owner's screen, so the "before" was rendered from a fixture (private Xvfb, synthetic homes: three chats of the default home, two of an account, one that another program writes, one open tab). At the sidebar's width (about 38 columns) the old card was `claude · Native (~/.claude) · #cl-… ` on the second line: the id and the account word pushed the age off the card, so on every `Native` row the "how long ago" was cut away (it showed only where the account name was short), and `[elsewhere]` on the first line left a live chat only `Refactor the billing …` (22 columns) of its title.

## What changed (`gui/sidebar_chats.rs`)

- **The second line says when the chat was written first**, in the short words of the list: `10m ago`, `3h ago`, `2d ago`, `just now`; then the account or ssh host, then the short id. The provider word is dropped (the `C·` / `X·` badge says it).
- **`Native` is not repeated**: the provider's own default home says nothing on a row; another account (`ClaudeWork`) or an ssh host is named. Nothing of this is stored or changes in the host; the JSON `account_label` still carries `Native (~/.claude)`.
- **A chat that another program writes** keeps the first line for its title (`Refactor the billing exporter so …`, 34 columns instead of 22) and its mark moves to the second line with the age: `[elsewhere] written 1m ago · Claud…` (the account is clipped first when the line is long). `[open]`, `[closed]`, `[ended]` stay on the first line. The running-elsewhere rule itself, the order of the marks and what a click does are untouched.
- The window JSON of a chat entry has `pill`, `pill_line`, `sub` (the second line, whole) and `title_shown` (as drawn).

## Why not the group heading of the plan

The plan said `Native` becomes a heading once where the account changes. The fixture showed the list is newest first across accounts, so the account changes after almost every row (four headings for six rows, more than the old repeats), and a heading row takes a line of the page whose size is computed from the card height alone (`calc_chats_ps`), so the last cards of a page would be cut off. Regrouping the page by account would change its order. The tag rule above gives the owner's two asks (the age, no repeated word) with no layout change. Before and after screenshots: `private/shots/m13_p03/chats_before.png` and `chats_after.png` (private, not in the repository).

## Evidence (as measured)

| Check | Result |
|---|---|
| `tests/test_chat_list_polish.py` (window JSON on synthetic homes) | red first: `KeyError: 'pill'` (`private/runs/m13_p03_red.log`); green after: the live row has `pill_line` 2, `sub` `[elsewhere] written 1m ago...`, `title_shown` over 30 columns; Native rows say `10m ago`, `3h ago`, `2d ago` with no `Native`; the account row names `ClaudeWork`; the open row keeps `[open]` on line 1 |
| Rust units in `sidebar_chats.rs` | `4 passed` (card lines, marks, the short words of the age); written with the code, not run red |
| `test_running_elsewhere_window`, `test_chats_end_to_end`, `test_chats_pagination_and_resume`, `test_native_end_to_end` | green |
| Cold full suite on the tip | `539` tests in `113` files, `539 passed`, `116.7 s`, tree peak `1,354.1 MiB`; Rust `666 passed, 1 ignored` |

## Honest notes

- One of my own first expectations was wrong (a title of 37 columns where the card has 35); the test now asks for what the card can show.
- The age of a live row comes from the chat's last write as the host reports it (`updated_at`); the rule that decides `running_elsewhere` (a write in the last 180 s, or a provider process) is not looked at again here.
- A `Native` row is now recognisable only by having no account word; the first-run question and the `+ New chat` dialog still name it.

## Next

P04: end-to-end verification, three cold runs, idle and hidden-growth measurements, acceptance, `lico 0.3.1`, install.
