# M25-P11 Plan: An agy Account, And The Provider Of A New Account Is Chosen By Icon

- Status: done (2026-11-27)
- Version: 1.0.0
- Branch: `phase/M25-P11-agy-accounts-and-provider-icons` from `phase/M25-P10-bot-session-and-times` (`b0f5976`)
- Milestone: [M25](../../../milestones/202611/20261126/20261126_01_M25_agy_as_a_lico_session_milestone.md), row P11 (the `0.9.1` patch release)
- The owner's words (2026-11-26, after trying `0.9.0`): "현재 agy가 살짝 다르게 구현된 것 같은데.. 의미적으로 Account에서 agy를 Provider로 고를 수 있어야 하고, 애초에 Account에서 지금 텍스트로 새 account를 고르게 되어 있는데 claude, agy, codex 중에 골라서 OAuth 형태로 로그인 하는 것만 지원하고 다른 방식은 지원 안하는 형태로 설계를 했어. 다만, 지금은 텍스트로 되어 있으니 그 부분을 아이콘으로 바꾸고 만약 claude 아이콘을 누르면 그 계정에 이름을 주는 형태로(현재 claude_initial)로 된 것처럼 추가 되도록 agy에서도 지원 방식을 찾아서 동일하게 구현."
- Standing authorization of 2026-11-25 and the owner's answer of 2026-11-26 ("0.9.1로 끝까지 진행"): the prefix, the commits, the merge and the push are approved in advance.

## What the owner asked, read

1. In the Accounts screen (the person icon of the toolbar, then *New account*) the provider is today a line of text, `Provider: < claude >   (Tab or click to switch)`, that toggles between Claude and Codex. It becomes **three pictures** (Claude, agy, Codex, the same pictures and the same size as the tiles of the New session wizard); a click on one chooses the provider and the screen goes on to the **name** of the account (as it does now for Claude).
2. **agy is a provider of accounts** and is added the same way: a name, then a sign-in that is OAuth only (the program's own sign-in; no other way is supported, as for the other two).

## What was found out about agy's sign-in (2026-11-27, the installed `agy` 1.5.18, an empty home, no credentials)

`agy acp --login` ("Log in with Google interactively, then exit (ACP terminal auth)") prints `Open this link in a browser to log in with Google:`, an `https://accounts.google.com/o/oauth2/auth...` address, then `Then paste the authorization code, or the full URL of the page you land on, and press Enter:` and waits on **standard input**; with no input it ends (`login cancelled: no authorization code was entered`, exit 1). It keeps its files under `$HOME/.gemini/antigravity-cli/` (a log and a small database appeared there and nothing else before the code), so **a private `HOME` isolates an account**. This is the same shape as the sign-in of Claude and Codex that the host already supervises (`login.rs`: the address is handed to the window, the pasted code goes straight to the process, nothing is logged), so the same flow serves. What cannot be checked here: a real Google sign-in round trip, and where the program keeps the token afterwards (a file under the private home or the system's key store): the owner's hands-on check (G3).

## What this phase changes

### Host

1. **`agy` is a provider of accounts.** `accounts::provider_variable("agy")` is `HOME` and an agy account's provider home **is** its home (`<accounts>/<id>/home`, mode `0700`): the sign-in's environment already sets `HOME` to the account's home, so the program's `.gemini` lands in it. An account of agy has no `ssh_host` (`bad_request`: the program is local, as an agy session is). `account.create` accepts `agy`.
2. **The sign-in.** `login::start` runs `agy acp --login --disable-telemetry` for an agy account (method `browser`; no device method), with the private environment of a login (`HOME` of the account, `PATH`, `LANG`, nothing else); the program is the one the host was told (`--agy`, kept by `agy::program()`; the test double `--login-bin` stands in for it as for the others). The prompt is recognised in agy's words: `paste the authorization code` (and the existing two).
3. **An agy session takes an account.** `session.create kind=agy` accepts `account` (an agy account); with none given it takes the **default agy account** when agy has any, and when agy has accounts but no default it is refused (`unknown_account`), as for the other two (never an unbound fall-back once the provider is managed); with no agy account at all it runs as before, in the person's own home (the sign-in the person made in the program itself). The session records the account (and the credential generation); the program is started with `HOME` of the account's home and **no** `XDG_*` names (it looks under `HOME`), else with the host's own as before. `session.rebind` stays for Claude and Codex (an agy session's account is the one it started with).
4. Nothing else changes: the program's own flag that skips its permissions is never passed, the telemetry flag is still passed, no token is read or written by Lico.

### Window

5. **The provider of a new account is three pictures.** The first step of *New account* (from the Accounts screen) shows Claude, agy and Codex as pictures over blank lines (`picker.rs`'s tiles, the same size as the wizard's); the one chosen has the ring; a click, or Tab / Shift+Tab, chooses (Left and Right are not used: the step is a text entry, the cursor keys are the text's); the line of text under them still says the provider (the model of the keys and the dump), the target line (the SSH host) is shown for Claude and Codex only. The step after is the name, as before. An agy picture of a machine with no `agy` is grey and says so (as in the wizard).
6. The list of accounts shows an agy account with agy's picture (`icon_of`).
7. The wizard's agy picture still starts an agy session at once (no account step: the account is the default agy one when there is any, the host decides).

## Decisions (the lead's defaults; the owner may change any)

| # | Decision | Why |
|---|---|---|
| D1 | The sign-in of an agy account is `agy acp --login` and only that | the program's own words: "ACP terminal auth"; OAuth only, as the owner designed it |
| D2 | A private `HOME` is the account | agy keeps its files under `$HOME/.gemini`; no flag or variable of its own is needed (`--gemini-dir` exists but a relative default under `HOME` is what it uses) |
| D3 | An agy session takes the default agy account, else the host's own home | the rule of the other two, and a person who never makes an agy account sees no change |
| D4 | No SSH target for agy | an agy session is local (the milestone's D5); the program on another machine is not supervised |
| D5 | The wizard's agy picture is unchanged (no account step) | the owner's "one touch" start; the account is the default one |
| D6 | No account switch for a running agy session (`session.rebind`) | not asked for; one more thing the real program would have to be tried for |

## Steps

1. [x] **Tests first (host)**: `accounts.rs` (agy account, homes, no SSH), `login.rs` (agy's prompt, the words), `new_session.rs` (the account rules for agy), `agy/mod.rs` (the environment with an account), `tests/test_agy_accounts.py` (create, sign-in through the double with agy's wording, a session on the account has the account's `HOME`, none given: the default, none at all: as before).
2. [x] **The change (host)**.
3. [x] **Tests first (window)**: the provider step is three pictures (`tiles_json`, the dump), a click on agy chooses it and the name step creates an agy account, Tab cycles three, no target line for agy, the list shows agy's picture; a real window (Python).
4. [x] **The change (window)**: `panel.rs` (the step's lines), `newsession_wizard.rs` (`wizard_tiles` for the step), `picker.rs`, `admin_ui.rs` (`toggle_label_provider`, the click lines), `account_cards.rs` (`icon_of`).
5. [x] **Mutation checks**; the guide (the account screen, the agy sign-in, what is not checked), the security note (an agy account: the private home, no token read), the protocol note (the `provider` of `account.create`), the plan's results, the milestone row, the index.

## Acceptance

| # | Criterion | Checked by |
|---|---|---|
| C1 | `account.create` with provider `agy` makes an account whose home is private (`0700`) and which has no SSH host; an SSH host is refused | unit test, host test |
| C2 | the sign-in of an agy account runs `agy acp --login` (or the double), hands the address to the window, takes the code on standard input, and ends as `succeeded` or `failed` | unit tests (`login.rs`), host test |
| C3 | an agy session takes the account it was asked for, else the default one, else (no agy account) none; an account of another provider is `unknown_account` | unit tests, host test |
| C4 | the program of an agy session on an account is started with `HOME` of the account and no `XDG_*` name; with no account the environment is as before | unit test, host test |
| C5 | the provider step of *New account* is three pictures; a click chooses; the name step follows; an agy account is made | unit tests, window test |
| C6 | the target (SSH) line is not shown for agy and the host is never sent one | unit test |
| C7 | no middle dot, no local path, no secret in any new text; no file crosses the size ratchet | the guards of the commit |

## Personal data and security

- Lico reads and writes **no token**: the sign-in is the program's own, in an environment cleared to `HOME`, `PATH` and `LANG`; the pasted code goes to the process and is logged nowhere (the existing rules of `login.rs`).
- The account's home is `0700` under the host's accounts folder; two agy accounts never share a login file. Where the program stores the credential (a file under that home or the system's key store) is **not verified**: if it is the system's key store, two accounts would share it. This is the first thing the owner's hands-on check must look at and is written in the security note.
- The address the program prints is shown to the person and opened in the browser by the window, as for the others; nothing is sent by Lico.

## Not decided here

An account switch for a running agy session; an SSH agy account; a quota for agy accounts (the host knows none, the card shows no disc); signing in through the toolbar's *Terminal* tab instead (the program's own TUI also signs in there).

## Results

**Done 2026-11-27.** The provider of a new account is three pictures (Claude, agy, Codex) and agy is a provider of accounts, signed in by the program's own `agy acp --login` and by nothing else.

### What was built

- **Host.** `account.create` accepts `agy` (`ipc.rs`); an agy account is one private home (`0700`) that is the program's `HOME` (`provider_variable` is `HOME`, `provider_home == home`) and never an SSH account (`bad_request`) (`accounts.rs`); the sign-in runs `agy acp --login --disable-telemetry` in the cleared environment of every sign-in, `device` is `bad_request`, and the prompt `Then paste the authorization code ...` is a request for a code (`login.rs`, the program is the one `--agy` named, kept by `set_agy_program`, and `--login-bin` stands in for it in the tests as for the others). An agy session takes the account asked for, else the default agy account, else (no agy account at all, or a conversation resumed by its id) the person's own home as before; accounts with no default are `unknown_account`; the session records the account and its credential generation (`new_session.rs`); the program is started with `PATH`, `LANG` and the account's `HOME` and no other name of the person's (`agy/mod.rs`); an account that was removed under a session is `unknown_account`, never another home; `session.rebind` is refused for agy (`core.rs`); a Claude or Codex session made by an agy caller (`session.create` from a session) does not carry the agy account (`create.rs`, two places). The CLI help names agy.
- **Window.** The first screen of *New account* keeps six blank lines (`PROVIDER_LINES`) under which three pictures are painted (the wizard's tiles, `tile_rects` / `draw_tiles`, a ring on the one chosen, the word under each, grey for agy when the program is not found, each a target `Action::ProviderPick(i)`); the text line under them (`Provider: < claude >   (click a picture, or Tab)`) stays the model of the keys and the dump; Tab and Shift+Tab go round the ones that can be chosen; a grey picture is not taken and says `agy is not installed on this machine.`; agy has no *Target* row and a host chosen for another provider is dropped with it; a screen that the form asked for (the provider is the form's) has no pictures. The dump has `acct.providers` (`provider`, `icon`, `word`, `chosen`, `enabled`, `rect`). The list of accounts shows an agy account with agy's picture; a session on an account of agy says `agy | <account>` in the strip and the header and is headed `agy / <account>` in the list (a session of the person's own home still says `agy` alone); `Rebind account...` says `an agy session keeps the account it started with`. The protocol schemas know `agy` for the account operations (`v1.py`) and the protocol note is `0.1.18`.
- **Docs.** The guide `1.40.0` (the account screen, the agy sign-in, the agy sessions section, what is not checked), the security note `1.2.0` (two table rows, gaps 11 to 13), the protocol note.

### Tests

12 new unit tests (accounts 1, login 2, agy 1, create 1, account cards 7) and the assertions of three existing ones that said an agy session has no account (strip, list heading, header), 2 host tests and 2 window tests in Python (`tests/test_agy_accounts.py`, `tests/test_agy_account_window.py`, with the double `tests/fixtures/login/fake_agy_login.py` and the simulator's `--mark-home`): 16 new tests, under the owner's 100 for a turn. `gui::` 679 passed; the account-related Python files 34 passed.

**Mutants: 38, all killed.** Round one killed 33 and left 5 that were not mutants of the product: the text of one mutant matched twice (the test says the same words), one did not compile (a type the mutant lost), and three were real gaps in the tests, each answered with a test and killed: the unit test cannot see the environment names of the person's home (the Python test does; the mutant was run again in its kind), Shift+Tab with a grey agy picture came out like Tab (the window test now uses the three pictures: a step back is not a step forward), and the header of an agy session on an account was not tested (`row.rs`).

### Not decided or not done (said to the owner)

- The real Google sign-in through an agy account, and **where the real program keeps its token** (a file under the account's home, or the system's key store: then two accounts would share one login): not verifiable here, the first thing of the owner's hands-on check (security note, gap 11).
- A closed conversation of an account's private home is not in `[ Chats ]` (the list reads the person's own home only); no quota disc; no account switch for a running agy session; no SSH agy account (D4, D6).
- The wizard's agy picture still has no account step (D5): the account is the default agy one when there is any.
