# M11 — The Provider's Own Default Home as a Visible "Native" Choice, Running-Elsewhere Safety, and the Empty-Resume Hint (`lico 0.2.7`)

- Status: approved, in progress
- Version: 1.0.0
- Approved: 2026-11-13 (G1 for M11 and G2 for M11-P01 to M11-P03 approved by the owner on 2026-11-13)
- Project: Lico
- Updated: 2026-11-13
- Base: the tip of `phase/M10-P04-verification-and-0-2-6` (M10 is not accepted or merged); later phases are stacked on the previous phase tip
- Milestone branch: `milestone/M11-native-provider-home-chats` (created at the closing commit after acceptance)
- Implementation: `phase/M11-P01-native-account` .. `phase/M11-P03-verification-and-0-2-7` (`lico 0.2.7`)
- Next gate: G3 and G4 stay with the owner

## Goal

A person who used Claude Code or Codex before installing Lico must find those chats again. Today the provider's own default home (`~/.claude`, `$CODEX_HOME` or `~/.codex`) is read only while no managed account of that provider exists, so after the first managed account the earlier chats of a folder are never shown (the data is not deleted, only never shown). This milestone:
(1) makes the default home a visible, explicitly chosen entry named `Native (~/.claude)` / `Native (~/.codex)`, a virtual entry with the reserved ids `native-claude` and `native-codex` (nothing is stored, nothing is copied, symlinked or decrypted; the provider runs in place with its own login), selectable in `+ New chat`, `lico session create --account`, `lico chat --account` and `mcp__lico__create`, with a one-time first-run prompt (Yes / No / Ask later) that reads counts only;
(2) marks a chat that another process is writing right now as `running elsewhere` and refuses its Resume with a plain reason (`New` stays allowed), without adopting or touching the other process;
(3) when Resume finds no chat for the chosen account, shows a read-only hint with the number of chats of the folder under other accounts and homes.

## Spec rules kept

AUTH-08 (no migration, copy, symlink or decryption of another application's credential store; Lico opens no credential file), AUTH-06 (the account is shown beside the input target and Native has a fixed visible name; never a silent fallback or switch), AUTH-04 (a session keeps its account), SES-07 (a saved conversation may be resumed; a running external process is never adopted, no stdin injection). Owner rules: idle RSS `< 50 MiB`, one executable, GUI with CLI and tool parity, no real sign-in, no billable turn, never the owner's own socket, `~/.claude` or `~/.codex` in a test.

## Done when

1. The Native entries are listed (when enabled) with their fixed labels in the `+ New chat` dialog and accepted by `lico session create --account`, `lico chat list|latest --account` and `mcp__lico__create`; the chats of the default home are listed and resumed in place; a Native session is started with the default home and none of a managed account's variables; the old "default home only when no managed account exists" rule is gone.
2. The first-run prompt appears once per window start when the default home has chats and the choice is open, and reads only counts before the answer; the choice can be changed in the Settings menu and with `lico settings`.
3. A chat written by another process (recent modification, or a same-user provider process that names it or runs in its folder since before its last write) is listed `running elsewhere`; its Resume (dialog, CLI, tool, `resume_latest`) is refused with the reason and `New` works; a closed chat of the same folder resumes.
4. The empty-Resume hint and the reports of `--resume-latest` and `resume` say which case happened.
5. Cold full suite `<= 180.0 s` and `<= 2,048.0 MiB`, idle RSS `< 50.0 MiB`, hidden growth `<= +10.0 MiB`, two-clone `suite acceptance`, `lico 0.2.7` installed to `~/bin/lico` (the `0.2.6` binary kept as a backup). Numbers only as measured.

## Out of scope

- Any copy, symlink, migration or decryption of a provider's credential store; real sign-in; billable turns; adoption of a running external process; changes to the M10 branches.
- Push, pull request, merge or publication (owner G4).

## Phases

| ID | Phase | Plan | Report | Status |
|---|---|---|---|---|
| M11-P01 | The Native account: virtual entry, one listing rule, settings, label, dialog and first-run prompt | [Plan](../../plans/202611/20261113/20261113_13_M11-P01_native_account_plan.md) | [Report](../../reports/202611/20261113/20261113_13_M11-P01_native_account_report.md) | implemented |
| M11-P02 | Running-elsewhere detection and refusal, the empty-Resume hint | [Plan](../../plans/202611/20261113/20261113_14_M11-P02_running_elsewhere_and_hint_plan.md) | [Report](../../reports/202611/20261113/20261113_14_M11-P02_running_elsewhere_and_hint_report.md) | implemented |
| M11-P03 | End-to-end verification, measurements, and `lico 0.2.7` | [Plan](../../plans/202611/20261113/20261113_15_M11-P03_verification_and_0_2_7_plan.md) | pending | planned |
