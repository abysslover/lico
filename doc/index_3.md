# Documentation index, page 3

| Document | Current meaning |
|---|---|
| [M01-P03 plan](plans/202610/20261023/20261023_03_M01-P03_first_real_agent_plan.md) | First real agent path (Codex app-server adapter, Unicode and Korean input); G2 approved |
| [M01-P03 report](reports/202610/20261024/20261024_01_M01-P03_first_real_agent_report.md) | Adapter, text sessions and hostile-provider results; no real provider turn run; inspection verdict pending |
| [M01-P04 plan](plans/202610/20261024/20261024_01_M01-P04_accounts_and_second_provider_plan.md) | Keyring-only credentials, same-provider accounts, second provider adapter; G2 approved |
| [Future idea: chat store full-text index](design/202610/20261024/00001_future_ideas/chat_store_full_text_index.md) | The owner's idea of an updatable compressed full-text index for conversations; not planned |
| [M01-P04 report](reports/202610/20261024/20261024_02_M01-P04_accounts_and_second_provider_report.md) | Credential store, accounts and the second provider against simulators and a throwaway keyring; inspection verdict pending |
| [M01-P05 plan](plans/202610/20261024/20261024_02_M01-P05_workflow_recovery_and_package_plan.md) | Workflow, recovery, resource acceptance and the single executable; G2 approved |
| [M01-P05 report](reports/202610/20261024/20261024_03_M01-P05_workflow_recovery_and_single_executable_report.md) | Recovery, first view, single executable, measurements; the owner's own use and inspection pending |
| [Owner end-to-end guide](manual_end_to_end.md) | The steps the owner runs with real accounts |
| [M01-P06 plan](plans/202610/20261024/20261024_03_M01-P06_in_window_login_and_session_creation_plan.md) | Session creation, account management and provider login inside the window; G2 approved |
| [M01-P06 report](reports/202610/20261025/20261025_01_M01-P06_in_window_login_and_session_creation_report.md) | Session creation, accounts and sign-in in the window; the owner's real sign-in and inspection pending |
| [M01-P07 plan](plans/202610/20261025/20261025_01_M01-P07_window_first_view_and_keys_plan.md) | The window's first view and key input: cause, scope and tests |
| [M01-P07 report](reports/202610/20261025/20261025_02_M01-P07_window_first_view_and_keys_report.md) | Panel text drawn again, no marker square, survives a closed input; menu bar judgement; the owner's look pending |
| [M02-P01 plan](plans/202610/20261025/20261025_02_M02-P01_menu_bar_and_shared_controls_plan.md) | A menu bar and the small shared control set for the hand-drawn window |
| [M02-P01 report](reports/202610/20261025/20261025_03_M02-P01_menu_bar_and_shared_controls_report.md) | The menu bar and the shared controls; measurements at both scales; the owner's try pending |
| [M02-P02 plan](plans/202610/20261026/20261026_01_M02-P02_folder_sessions_plan.md) | Sessions in a folder the person picks, a real input box, a host echo of each input, close/rename/delete, memory per session |
| [M02-P02 report](reports/202610/20261026/20261026_01_M02-P02_folder_sessions_report.md) | What was built, tests and mutations, memory and CPU numbers, folder trust, MI-07, try-it list; the owner's try and inspection pending |
| [Daily run cost note](plans/202610/20261026/20261026_02_daily_run_cost_note.md) | What one changed line in the big Rust files costs the daily run, three options and a pick; for the owner at the P02 gate |
| [M02-P03 plan](plans/202610/20261026/20261026_03_M02-P03_sending_between_sessions_plan.md) | Sending text between sessions (inline up to 4 KiB, longer as a file in the receiver's own folder) and a tool connection for Claude agents with approvals |
| [M02-P03 report](reports/202610/20261027/20261027_01_M02-P03_sending_between_sessions_report.md) | What was built, tests and mutations, memory and traffic numbers, real-program checks, security limits, try-it list, carry lists; checked in four rounds by an independent verifier; the owner's try and inspection pending |
| [M02-P04 plan](plans/202610/20261029/20261029_01_M02-P04_screen_and_copy_paste_plan.md) | Screen capture, clipboard read and write, image paste, middle-button paste, short-lived X helper and agent screen/clipboard tools |
| [M02-P04 report](reports/202611/20261111/20261111_01_M02-P04_screen_and_copy_paste_report.md) | What was built, tests and 175 mutations, S08 memory/latency/X11 measurements, S07b live canary rows, combined P03+P04 G3 try-it list; inspection verdict pending |
| [M02-P05 plan](plans/202611/20261111/20261111_01_M02-P05_computer_use_plan.md) | Private per-session virtual display, XTEST computer helper, coordinate scaling and 15-minute gated real-screen control |
| [M02-P05 report](reports/202611/20261111/20261111_02_M02-P05_computer_use_report.md) | What was built, live Chrome CU-07 (4/4) and WM-less focus CU-08 (5/5), memory/latency measurements and captures; inspection verdict pending (with M02-P06) |
| [M02-P06 plan](plans/202611/20261111/20261111_02_M02-P06_everyday_comfort_plan.md) | Git worktree sessions, two-step dirty removal, attention marks, notify-send notifications and on-demand conversation search |
| [M02-P06 report](reports/202611/20261111/20261111_03_M02-P06_everyday_comfort_report.md) | What was built, zero-resident-index search measurements (+0 KiB host RSS delta, 0.28 ms median), 5 captures and combined P05+P06 G3 try-it guide; inspection verdict pending |

Back to the [first index page](README.md) or the [second](index_2.md). More documents: [page 4](index_4.md).
