# M16-P08 Report — Verification and `lico 0.4.0`

- Status: implemented; waits for the lead's close of the milestone and the owner's G3 (nothing here is accepted)
- Version: 1.0.0
- Branch: `phase/M16-P08-verification-and-0-4-0` (stacked on the P07 tip `0c6d054`; the measurements ran on the committed clean tree `de406a7`, the documents of this phase are the next commits)
- Plan: [M16-P08](../../../plans/202611/20261117/20261117_10_M16-P08_verification_and_release_0_4_0_plan.md)

## 1. Measured verification (`lico 0.4.0`)

| Check | Measured result | Budget / gate | Status |
|---|---|---|---|
| Rust unit tests (`umask 022` and `umask 077`) | `701 passed; 0 failed` (`5` ignored: the four system tests and the helper test, both umasks); the tools `17 passed`; `cargo xtask check` and `verify` pass | `0` failures | Met |
| System tests (`cargo xtask system`, the real binary, the fake server) | `0.1 s / 18.2 MiB`, `0.2 s / 18.9`, `0.4 s / 18.8`, `0.2 s / 18.6` | `512 MiB`, `60 s` each | Met |
| Full cold suite (`suite run --cold`), three counted runs, each started with load below `9` (it was `4.3..5.8`) | `557` tests in `117` files, `556` passed and `1` failed each time, the same one (`test_adapter_contract`, below): `133.9 s` / peak `1,842.2 MiB`; `129.4 s` / `1,570.4 MiB`; `128.0 s` / `1,629.7 MiB` | `<= 180.0 s`, `<= 2,048.0 MiB` | Met for time and memory; margin `46.1 s` and `205.8 MiB` at the worst |
| A fourth run, NOT counted | `134.0 s` / `1,855.5 MiB`, `2` failed: the known one and `test_first_view::test_session_list_shows_state_account_and_a_pending_approval_while_the_session_is_hidden` at `15.5 s` against its budget of `15 s` (the timing flake reported in M14 and M15) | | Not counted, said |
| The known red | `tests/test_adapter_contract.py::test_adapter_capabilities_pin_the_version_and_mark_unverified`: the installed Codex command is `0.169.0`, the record is pinned to `0.167.1`. It fails without any change of M16 and nothing of M16 touches that adapter. I did not move the pin or touch the test; report 123 holds what I found (only two problems: the version, and the hashes of `ClientRequest.json` and `ServerNotification.json`; every server request, mapped request and mapped notification still fits) and the options; the lead answers | `0` failures | **Not met in the three counted runs** (one known failure each); met after the move below |
| Idle window RSS (`idle_memory`, 10 accounts, menu on, 3 launches) | scale 1 `25.25 MiB` (`25.08..25.25`), scale 2 `36.29 MiB` (`36.05..36.29`), host `8.19` and `8.14 MiB` | `< 50 MiB` | Met (`0.3.3`: `25.65`, `36.62`, host `8.27`) |
| Hidden growth, 10 to 100 sessions (3 repetitions) | `+0.19 MiB` (`0.07..0.19`) | `<= +10 MiB` | Met (`0.3.3`: `+0.40`) |
| Tool list | `4,849` bytes (writing off) and `5,103` (on): unchanged | `<= 5,120` | Met |
| Binary free of the test support | no match of `png/src/testing`, `slow_crc`, `paeth_spec`, `read_png` in `strings` of the release binary | none | Met |
| Two-clone build | `cargo xtask accept`: two clean clones build the same `lico`, sha256 `97a22eec71764ae3234164c94f9467990a1591763589f2350a81c7c24c922e71`; Python `suite acceptance`: `1 passed` | identical | Met |

Time and memory of the suite are within the budget, but the worst peak is `208.7 MiB` (`+12.8 %`) over the worst of `0.3.3` (`1,633.2`) and the margin to `2,048` is `205.8 MiB`. The peak moves from run to run (`1,570..1,842`) with which tests overlap during the first seconds, when `cargo test --bin lico`, `cargo test -p lico-xtask` and the Python files that start a host run together; the host unit-test binary now holds `701` tests against `649`. I did not trace it further; the lead may want to look at the margin before the next milestone adds tests.

### The pin move (commit `b69277e`, after the three counted runs)

Option A of question 123, approved by the lead under the owner's delegation (the owner away): the pin of the Codex adapter moved from `0.167.1` to `0.169.0` in a commit of its own (the test first, red against the old record; then the hand-edited record: version, `checked`, two hashes, `106` to `109` client requests; the notes, the `unverified` list and the owner-run note at `0.164.0` stay). What was read: the server requests are the same set, every mapped request and notification is still in the schema, the fixtures still validate against the installed schema; no real turn was run. **This move is outside the Local LLM work** (the local session never touches that adapter); it was needed because the whole suite must be green at a release; the owner may ask for it to be moved back. The three counted runs had this test red (known); after the move both tests of the file are green in a single run (`5 passed`), `cargo xtask check` and `verify` pass, and no fourth whole-suite run was made. The record is not built into the binary and not part of the product source hash (`6f45df7e...3b17` before and after), so the `0.4.0` binary is unchanged. **Known weakness, for the owner to decide on later:** `record_codex()` does not give the committed file back (it writes a fixed `checked` date and drops the hand-added notes), so a pin move is a hand edit.

## 2. What M16 delivers (the release notes in short)

A model server on this machine (OpenAI-compatible or Ollama, loopback only) is a session of its own kind `local`, made in the window (`Ctrl+N`, `+ New chat`), with `lico session create --kind local` or over HTTP; the host asks the server itself, runs the host's tools with the approvals every session has, keeps an owner-only record, and has no files or shell. Six components (`lico-llm-wire 0.1.0`, `lico-llm-http 0.2.0`, `lico-llm-fake 0.1.0`, `lico-convo 0.1.0`, `lico-toolloop 0.1.0`, `lico-png 1.0.0`), a driver port that keeps the new kind out of the 48-module cycle (still `48`), four system tests, and the retired prototypes deleted (P01). The key is stored only with `lico local-key set ADDRESS:PORT`.

## 3. What is left over, for the owner

1. **A masked key entry in the window** (the key goes in with the command line only).
2. The notes "the session answers in text only" and "N oldest turns are no longer sent" have no picture (the screenshot fixture cannot make them); the Rust tests prove them.
3. The pin of `test_adapter_contract` (report 123).
4. The discovery-onto-`llm-http` proposal stays with the lead.
5. The wording of who approved what in the P06 notes and the commit `f39d174` (the lead settles it at the close).
6. P06 left three source files over the size ratchet; P07 fixed it without an allowlist change.

## 4. Screenshots (for looking, not a test)

`private/scratch/202611/20261117/m16_p07_screens/`: `1_new_session_form_local_row.png`, `2_server_and_model_choice.png`, `3_local_session_row_and_answer.png`, `4_waiting_card.png` (details in the P07 note).

## 5. Release

Built by `build-release.sh` from the clean commit `824502c`: `lico 0.4.0 (commit 824502c, build 6f73bae535ec8aa1)`, sha256 `988be12b36d5f5b4c5d20a9aa2fa3939420aa791d31feddba3ea1fec996ab889`, `9,942,008` bytes: `+160,776` (`+1.64 %`) over `0.3.3` (`9,781,232`): the adapter, the port, the five new crates and the window code. (The acceptance hash above is of the tree at `de406a7`; the commit id is inside the binary, so the two differ.)

The `0.3.3` binary (commit `ef44556`, sha256 `c40b5d9e...2ead`) is kept at `private/release_0_3_3_backup/lico`; the new one was copied beside and renamed to `~/bin/lico` (to go back: copy the backup over it); `lico --version` prints the line above. A capture with the installed binary on a private virtual display gave a valid 800x600 PNG. The artifact is at `private/release_0_4_0/lico`. Link scan of the changed documents: 0 dead links. This install is the one the plan names; nothing else outside the repository was touched.
