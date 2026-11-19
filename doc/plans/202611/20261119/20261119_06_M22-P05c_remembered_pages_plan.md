# M22-P05c Plan — Pages Are Remembered, Listed In Chats, Reopened By A Click

- Status: in progress (2026-11-19)
- Version: 0.1.0
- Branch: `phase/M22-P05c-remembered-pages` from `phase/M22-P05b-search` (`a24736c`; `main` is `a063ea8`, P03b, P05a and P05b are committed and not pushed); the prefix `phase/M22-P05c-` needs the owner's yes at commit time
- Milestone: [M22](../../../milestones/202611/20261118/20261118_01_M22_simple_window_chats_of_all_sessions_milestone.md), the browser paragraph of "The owner's answers"
- The owner's words (in the milestone): "Browser pages are remembered (address, title, time; at most 100, in the host's state directory, mode 0600, never for a tab opened `--local`, cleared by a menu entry), listed in Chats and reopened by a click; `Reopen closed tab` is in the menu."
- P05 is three phases: P05a (done: the bar), P05b (done: search), **P05c** (this one).

## What this phase changes

1. **The host remembers pages**: when a page of a normal tab has loaded and its title is known, its address, title and time go into a list of at most 100 in the host's state directory (D41, D42).
2. **Chats lists them**: the Chats tab shows the remembered pages among the chats, newest first, as cards with the browser's picture; a click opens the page in a new browser tab (D44, D45).
3. **The menu**: `Reopen closed tab` opens the newest remembered page that no tab shows now; `Clear remembered pages` empties the list (D43, D45).

## What was found (2026-11-19)

- The tab model (`lico-browser-view`) already has the page's address and its cleaned, cut title; the host's supervisor gets `LoadFinished` and then asks the page's title (`ask_title` in `browser/supervisor.rs`): that is where a page is known whole. `Setup` carries the state directory; the entry carries `local`.
- The host's state directory is the parent of the registry directory (`Browser::state_dir_of`); `chats::collect` has the same `shared`, so the list the chats make can read the file too.
- `session.chats` makes the Chats list (items sorted by time, paged by `offset` and `limit`, filtered by `query`); an item is a `ChatItem` (provider, provider session, title, time, ...); the window's card reads `provider` for the picture (`chatcard::icon_of`), the title and the time; `Icon::Browser` exists.
- `browser/ops.rs` has 772 of 800 lines: the new operations live in a new file.

## Decisions (the lead's defaults; the owner may change any)

| # | Decision | Why |
|---|---|---|
| D41 | **The store** (`pages.rs` at the crate level, used by the browser adapter and by the chats): `Page {address, title, time}`; a list newest first; **at most 100**; the same address is one entry (it moves to the front, its title and time are renewed); an address must be an explicit `http://` or `https://` address (the scheme written in lower case) that `lico_browser_view::parse_address` takes (no user name, no white space, no local form); the title is the model's cleaned text cut at 200 characters (an empty title is kept empty, the card shows the address); the file is `browser_pages.json` in the state directory, **mode 0600**, at most 64 KiB, written whole through a temporary file and a rename; a missing, damaged, group-readable or link file reads as an empty list and says nothing (it is rewritten by the next page); a write that fails is one log line with no address | the owner's words; the same reading rules as `browser.json` |
| D42 | **When a page is remembered**: after `LoadFinished` and the title the supervisor asks, for a tab that is **not local**, whose phase is Ready, when the model's address is an `http(s)` address (`pages_ops::note` says which are kept); the time is the host's clock in seconds. Never a tab opened `--local`, never an unloaded page, never `about:blank` or the engine's error page | the owner's words; a refused page was never seen |
| D43 | **Two operations of the host** (protocol rows, `browser/pages_ops.rs`): `browser.reopen` (no fields) opens the newest remembered page that no tab of the host that is not local shows now (the same address, compared as it is written), as `browser.open` does, with the same reply and the address as `url`; `no_page` when there is none; `browser.forget` (no fields) empties the list and says how many were there. Neither takes an address from the caller: an address can come only from the list | a window or a script cannot make the host open anything it did not see |
| D44 | **Chats**: `session.chats` takes an optional boolean `pages`; when true, with scope `all` and neither `account` nor `ssh_host`, the remembered pages join the items: provider `browser`, provider session = the address, title = the page's title (else the address), `updated_at` = its time, no account, folder or file; they sort and page with the rest and `query` matches their title and address; the total counts them | the owner's words ("listed in Chats"); one list, one paging; other callers (the command line) do not say `pages` and see what they saw |
| D45 | **The window**: the Chats fetch of scope `all` says `pages: true`; a page card has the browser's picture, the page's title (the address when there is none), the time with its colour, no state picture, the second line is the address (`https://` dropped, cut in the middle as the bar cuts it), and the hint row says the title, the time and the whole address; a click opens the page in a new browser tab through `browser.open` (exact address, no search) and shows it, and does not move Directories; opening a page remembers it again, so it is the newest; `Reopen closed tab` and `Clear remembered pages` (no key) are in a group `Browser pages` of the Session menu after `New terminal tab`, greyed only with the open panel; `no_page` is told in the flash (`no closed tab to reopen`), so is the clearing (`Remembered pages cleared (n).`, `No remembered pages.`), and the Chats list loses the pages at once; Chats orders by time, pages that share a second by their address | the owner's words; no folder belongs to a page |
| D46 | **No `lico-browser-view` change**: the model already has what is stored; its 0.9.0 stays | a smaller phase; the receipt stays current |

## Steps

1. [ ] **Tests first**: `pages.rs` (remember: a new page at the front, the same address renewed and moved, the 101st drops the oldest, a bad address or scheme refused, a title cleaned and cut; the file: the round trip, mode 0600, a group-readable file, a link, a damaged file, a missing file, a file over the size, an atomic rewrite), the supervisor's hook (a normal tab remembers, a local tab does not, a page with no http address does not), `browser.reopen` and `browser.forget` (the newest not shown, none, the count), `session.chats` with `pages` (merged and sorted, the query, the total, absent `pages` unchanged, an account or ssh host excludes them); the window tests with the fake engine (a page loaded in a tab appears in Chats, a click opens it, `Reopen closed tab` after closing, `Clear remembered pages` empties, a local tab leaves nothing).
2. [ ] **The store** `pages.rs` and its module line.
3. [ ] **The host**: the supervisor's hook, `browser/pages_ops.rs` (`reopen`, `forget`), the operation names and the dispatch in `ops.rs`, `chats/mod.rs` (the `pages` field and the merge).
4. [ ] **The window**: the request field, the card's picture and hint, the click, the two menu entries and their key.
5. [ ] **Mutation checks**, before the final green gate.
6. [ ] **Gates**: `cargo test --bin lico`, clippy on the new code, the changed-files suite, the owner's build; the protocol, the product spec and the index rows.

## Not decided here

- A menu entry to remove one page (the list is cleared whole for now).
- The guide, the manual's browser paragraph, the README and the CHANGELOG (P06).
