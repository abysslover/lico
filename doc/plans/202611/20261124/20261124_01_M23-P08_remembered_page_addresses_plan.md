# M23-P08 Plan — The Remembered Pages Keep No Secret In An Address

- Status: done (2026-11-24)
- Version: 1.0.0
- Branch: `phase/M23-P08-remembered-page-addresses` from `main` (`49fef2f`, everything through M23-P04d is published); the prefix `phase/M23-P08-` was approved by the owner at commit time (the question on the prefix, 2026-11-24) and is in the local guard; the code is `daaa416`
- Milestone: [M23](../../../milestones/202611/20261118/20261118_02_M23_terminal_agents_milestone.md); the proposal is gap 1 of [the security note](../../202611/20261119/20261119_12_person_facing_surfaces_security_note.md) (1.0.0)
- The owner's words (2026-11-24, in the chat): "남아 있는 P08, P09, UI 단순화 -> 레벨 C로 가면 됨" (P08, P09, the UI simplification, then level C); the note's proposed order puts P08 first.

## What this phase changes

The host remembers every page a normal browser tab has shown (M22-P05c): the address goes to `browser_pages.json` and is listed as a card in Chats. Today the address is kept whole, so a sign-in link that carries a one-time code, or an address with a token in its query or after the `#`, is written to disk and shown on a card. The file is owner-only (`0600`), but a person who does not know this cannot know it. This phase keeps such an address out.

## Decisions (the lead's defaults; the owner may change any)

| # | Decision | Why |
|---|---|---|
| D1 | **The part after `#` is never kept.** The page is remembered without its fragment; two addresses that differ only there are one entry | an implicit sign-in flow puts the token there; the server never sees a fragment, so what the page is does not depend on it (a page that routes by the fragment reopens at its start, said in the guide) |
| D2 | **A page whose query carries a secret by NAME is not remembered at all** (not kept with the value cut). A name is a secret's when one of its words is `token`, `secret`, `password`, `passwd`, `pwd`, `pass`, `passcode`, `auth`, `authorization`, `session`, `sessionid`, `sessid`, `sid`, `sig`, `signature`, `key`, `apikey`, `code`, `otp`, `jwt`, `ticket`, `credential`, `credentials`, `bearer`, `magic`, `invite` or `csrf`, or when the name, with its separators taken out, ends with `token`, `secret`, `password`, `passwd`, `apikey`, `sessionid`, `sessid` or `signature` | the proposal named both ways; a link with its value cut is a broken link that looks like a page, and a sign-in or reset link is not one to come back to anyway |
| D3 | **Words, not letters**: the name is split at anything that is not a letter or digit and at a small letter followed by a capital (`accessToken`, `x-amz-signature`, `api_key`), after percent-decoding (`to%6Ben`); a word must equal a listed word, so `keyword`, `author`, `encoding`, `design`, `passenger`, `zipcode` and `q` are kept | letters inside a word would refuse ordinary search and form pages |
| D4 | **A path parameter counts too**: `;jsessionid=...` after a path segment is read as a name and a value | a classic way to put a session in an address |
| D5 | **The rule is applied when the file is READ as well as when it is written**: a file written before this phase has its secret pages hidden and its fragments cut at the next load, and the next write drops them for good; nothing is migrated by a separate run | the pages already on the owner's disk are the ones that matter |
| D6 | **`Reopen closed tab` compares without the fragment**: a tab showing `https://a/x#top` is the page `https://a/x` of the list | otherwise the page already open would be opened again |
| D7 | **The refusal is `secret_address`** (the store's answer; the supervisor still says only `not_saved`), distinct from `bad_address` | a test can tell the two apart |

## Not covered, said plainly

- A secret in the PATH (`/reset/3f9a...`) or in a name that is not on the list: no rule can tell it from a page's own name. The person's way out is `Clear remembered pages`.
- Personal data in an address that is not a secret (an e-mail address in a query).
- A page's TITLE (a title is the page's own text).
- What the engine or the guard keeps in memory while a page is open (nothing here changes).

## Steps

1. [x] **Tests first** (`pages_tests.rs`, `browser/pages_ops_tests.rs`): the fragment is cut and two addresses that differ there are one entry; a table of secret names refused with `secret_address` and nothing written; a table of ordinary names kept; a file of an older run with a secret page and a fragment reads clean; `note` refuses a secret page and `to_reopen` ignores the fragment of a shown tab. Red first.
2. [x] **The store** (`pages.rs`): `clean` returns the reason, cuts the fragment, tests the query and the path parameters; `strip_fragment` for the reopen.
3. [x] **The host**: `to_reopen` compares without the fragment.
4. [x] **Mutation checks**, before the final green gate.
5. [ ] **Gates and documents**: `cargo test --bin lico`, clippy on the new code, the changed-files suite, `cargo xtask check`, the owner's build; the guide (`doc/manual_end_to_end.md`: what is remembered and what is not), the protocol row, the milestone row (2.13.0), `PLAN.md` (also: P04d is published, `main` = `49fef2f`), the index rows.

## Results (2026-11-24)

- **What the owner sees**: nothing changes for an ordinary page. A page whose address has a secret by name (`?token=`, `?code=`, `?access_token=`, `;jsessionid=` and the like) no longer appears in Chats and is not in `browser_pages.json`; a page with a `#` part appears without it. A file written by an earlier run is read with the same rule: its secret pages are hidden and its fragments cut at once, and the next page remembered rewrites the file without them (the file itself is not rewritten by a reading).
- **Host**: `pages.rs` 195 lines (`strip_fragment`, `percent_decoded`, `words`, `is_secret_name`, `carries_secret`; `clean` now says why: `bad_address` or `secret_address`); `browser/pages_ops.rs`: `to_reopen` compares a shown tab without its fragment. No protocol, window or `Cargo.lock` change.
- **Tests (8 new, red first against a stub of `strip_fragment`)**: the fragment is cut and the addresses that differ only there are one entry; 34 refused names (query, percent-encoded, camelCase, path parameters, a `;` separator) leave nothing in the file; 20 ordinary names are kept (`keyword`, `author`, `encoding`, `zipcode`, `tokenizer`, a `%` with no digits after it, a path segment named `token`); every word of the list and every ending is spelt out in the test (the list in the program is not read by it), each as the last word, as the first word and in capitals; an older file reads clean and the next write drops its secrets; a secret that stands after the `#` goes with the fragment and does not refuse the page; `note` and `to_reopen`. Whole `cargo test --bin lico`: 1038 passed, 12 ignored.
- **Mutation checks (45 runs in three rounds, every mutant of the final code caught)**: round one 27 mutants of the first version: 24 caught, one compile error (a shorter array: redone with the length changed too), two survivors: `token` as a word of the list was also covered by the endings (the test now puts every word first in a name, where only the list can see it) and a lower-case mark in the word splitter that a second guard made equivalent (the state variable is gone: the splitter reads the last character of the word it is building); round two 9 mutants of the new splitter and the two redone ones: one survivor (a capital word before a separator was not lower-cased: a test with `WORD_X` now) and then 9 of 9 caught. Reading the mutants first found that `+` as a space in the percent decoder changes nothing (a `+` is a separator either way), so the branch was removed rather than tested.
- **Gates**: clippy as at the parent (155 warnings in `bin lico test`, none in the changed files), `cargo xtask check` passes.
- **Decisions that stand until the owner says otherwise**: a page with a secret name is not remembered at all (the alternative, cutting the value, makes a broken link that looks like a page); the fragment is always cut (a page that routes by its `#` part reopens at its start); `csrf` is on the list of words.
- **Spec**: BROW-11 gained one sentence on secrets; the wording is the lead's, for the owner to confirm.
- **Not done, said again**: a secret in a path, a name not on the list, an e-mail address in a query, and what the engine keeps while the page is open (see the first section).
