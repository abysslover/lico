# M15-P01 Appendix — The Long Tables of the Phase 0 Report

- Version: 1.0.0
- Report: [M15-P01 Phase 0 report](20261114_11_M15-P01_phase0_component_analysis_report.md)
- Marks as in the report: MEASURED, COMPUTED, ASSUMED. The scratch scripts that produced these tables are in `private/phase0/` (not committed).

## A1. All 170 Rust modules (COMPUTED; lines of the module's own file)

| module | file | lines | test file |
|---|---|---:|---|
| `(root)` | `unified.rs` | 92 |  |
| `accounts` | `accounts.rs` | 266 |  |
| `admin` | `admin.rs` | 200 |  |
| `broker` | `broker.rs` | 53 |  |
| `call_room` | `call_room.rs` | 724 |  |
| `call_room::ops` | `call_room/ops.rs` | 396 |  |
| `calls` | `calls.rs` | 640 |  |
| `calls::tests` | `calls/tests.rs` | 436 | yes |
| `capture` | `capture/mod.rs` | 386 |  |
| `capture::layout` | `capture/layout.rs` | 85 |  |
| `capture::out` | `capture/out.rs` | 65 |  |
| `capture::shrink` | `capture/shrink.rs` | 109 |  |
| `capture::tests` | `capture/tests.rs` | 358 | yes |
| `capture::tests_x` | `capture/tests_x.rs` | 540 | yes |
| `chats` | `chats/mod.rs` | 726 |  |
| `chats::latest` | `chats/latest.rs` | 92 |  |
| `chats::live` | `chats/live.rs` | 213 |  |
| `chats::others` | `chats/others.rs` | 53 |  |
| `chats::seed` | `chats/seed.rs` | 383 |  |
| `chats::turns` | `chats/turns.rs` | 107 |  |
| `claude` | `claude.rs` | 599 |  |
| `cli` | `cli.rs` | 740 |  |
| `clipboard` | `clipboard/mod.rs` | 372 |  |
| `clipboard::job` | `clipboard/job.rs` | 181 |  |
| `clipboard::open` | `clipboard/open.rs` | 135 |  |
| `clipboard::open_tests` | `clipboard/open_tests.rs` | 253 | yes |
| `clipboard::settings` | `clipboard/settings.rs` | 551 |  |
| `clipboard::tests` | `clipboard/tests.rs` | 219 | yes |
| `clipboard::tests_x` | `clipboard/tests_x.rs` | 400 | yes |
| `clipboard::write` | `clipboard/write.rs` | 198 |  |
| `clipboard::write_tests` | `clipboard/write_tests.rs` | 128 | yes |
| `clipwrite` | `clipwrite/mod.rs` | 161 |  |
| `clipwrite::tests` | `clipwrite/tests.rs` | 244 | yes |
| `codex` | `codex.rs` | 759 |  |
| `computer` | `computer/mod.rs` | 275 |  |
| `computer::real` | `computer/real.rs` | 178 |  |
| `computer::xact` | `computer/xact.rs` | 549 |  |
| `core` | `core.rs` | 938 |  |
| `core::tests` | `core/tests.rs` | 113 | yes |
| `create` | `create.rs` | 491 |  |
| `create::tests` | `create/tests.rs` | 626 | yes |
| `credcmd` | `credcmd.rs` | 190 |  |
| `creds` | `creds.rs` | 106 |  |
| `dbus` | `dbus.rs` | 453 |  |
| `echo` | `echo.rs` | 139 |  |
| `folders` | `folders.rs` | 754 |  |
| `fsview` | `fsview.rs` | 512 |  |
| `gui` | `gui/mod.rs` | 792 |  |
| `gui::approval_modal` | `gui/approval_modal.rs` | 200 |  |
| `gui::call_view` | `gui/call_view.rs` | 704 |  |
| `gui::controls` | `gui/controls.rs` | 216 |  |
| `gui::editor` | `gui/editor/mod.rs` | 722 |  |
| `gui::editor::app` | `gui/editor/app.rs` | 405 |  |
| `gui::editor::selection` | `gui/editor/selection.rs` | 354 |  |
| `gui::editor::tests` | `gui/editor/tests.rs` | 411 | yes |
| `gui::events` | `gui/events.rs` | 58 |  |
| `gui::fonts` | `gui/fonts.rs` | 607 |  |
| `gui::jobs` | `gui/jobs.rs` | 24 |  |
| `gui::llm_find` | `gui/llm_find.rs` | 522 |  |
| `gui::llm_rows` | `gui/llm_rows.rs` | 264 |  |
| `gui::mention_popup` | `gui/mention_popup.rs` | 382 |  |
| `gui::menu` | `gui/menu.rs` | 409 |  |
| `gui::native_prompt` | `gui/native_prompt.rs` | 263 |  |
| `gui::newchat` | `gui/newchat.rs` | 518 |  |
| `gui::newchat_draw` | `gui/newchat_draw.rs` | 140 |  |
| `gui::panel` | `gui/panel.rs` | 785 |  |
| `gui::panel::admin_ui` | `gui/admin_ui.rs` | 302 |  |
| `gui::panel::cardview` | `gui/cardview.rs` | 538 |  |
| `gui::panel::cardview::tests` | `gui/cardview_tests.rs` | 720 | yes |
| `gui::panel::confirm` | `gui/confirm.rs` | 405 |  |
| `gui::panel::confirm::tests` | `gui/confirm_tests.rs` | 209 | yes |
| `gui::panel::files_host` | `gui/files_host.rs` | 432 |  |
| `gui::panel::files_render` | `gui/files_render.rs` | 385 |  |
| `gui::panel::files_ui` | `gui/files_ui.rs` | 715 |  |
| `gui::panel::newsession` | `gui/newsession.rs` | 675 |  |
| `gui::panel::newsession::tests` | `gui/newsession_tests.rs` | 233 | yes |
| `gui::panel::sendto` | `gui/sendto.rs` | 505 |  |
| `gui::panel::sendto::tests` | `gui/sendto_tests.rs` | 590 | yes |
| `gui::panel::tests` | `gui/panel_tests.rs` | 59 | yes |
| `gui::pathbar` | `gui/pathbar.rs` | 202 |  |
| `gui::picture` | `gui/picture.rs` | 436 |  |
| `gui::picture::tests` | `gui/picture_tests.rs` | 543 | yes |
| `gui::realbar` | `gui/realbar.rs` | 181 |  |
| `gui::scroll` | `gui/scroll.rs` | 417 |  |
| `gui::scroll_oracle` | `gui/scroll_oracle.rs` | 501 | yes |
| `gui::sidebar` | `gui/sidebar.rs` | 780 |  |
| `gui::sidebar_calls` | `gui/sidebar_calls.rs` | 516 |  |
| `gui::sidebar_chats` | `gui/sidebar_chats.rs` | 768 |  |
| `gui::sidebar_nav` | `gui/sidebar_nav.rs` | 716 |  |
| `gui::sidebar_render` | `gui/sidebar_render.rs` | 665 |  |
| `gui::sidebar_tabs` | `gui/sidebar_tabs.rs` | 104 |  |
| `gui::tool_fold` | `gui/tool_fold.rs` | 210 |  |
| `gui::transcript` | `gui/transcript.rs` | 696 |  |
| `gui::ui` | `gui/ui.rs` | 1095 |  |
| `gui::ui::attn` | `gui/attn.rs` | 510 |  |
| `gui::ui::clip` | `gui/clip.rs` | 601 |  |
| `gui::ui::clip::tests` | `gui/clip_tests.rs` | 475 | yes |
| `gui::ui::reconnect` | `gui/reconnect.rs` | 188 |  |
| `gui::ui::reconnect::tests` | `gui/reconnect_tests.rs` | 249 | yes |
| `gui::ui::row` | `gui/row.rs` | 422 |  |
| `gui::ui::row::memrow` | `gui/memrow.rs` | 225 |  |
| `gui::ui::search` | `gui/search.rs` | 354 |  |
| `gui::ui::tabs` | `gui/tabs.rs` | 444 |  |
| `gui::ui::telemetry` | `gui/telemetry.rs` | 439 |  |
| `gui::ui::tests` | `gui/ui_tests.rs` | 113 | yes |
| `gui::ui::toolbar` | `gui/toolbar.rs` | 317 |  |
| `gui::update` | `gui/update.rs` | 487 |  |
| `gui::update::tests` | `gui/update_tests.rs` | 148 | yes |
| `gui::windowstate` | `gui/windowstate.rs` | 122 |  |
| `gui::wire` | `gui/wire.rs` | 29 |  |
| `handoff` | `handoff.rs` | 294 |  |
| `helper` | `helper/mod.rs` | 246 |  |
| `helper::tests` | `helper/tests.rs` | 448 | yes |
| `history` | `history/mod.rs` | 195 |  |
| `history::tests` | `history/tests.rs` | 400 | yes |
| `host` | `host.rs` | 231 |  |
| `http` | `http.rs` | 707 |  |
| `ident` | `ident.rs` | 126 |  |
| `inbox_read` | `inbox_read/mod.rs` | 186 |  |
| `inbox_read::tests` | `inbox_read/tests.rs` | 277 | yes |
| `ipc` | `ipc.rs` | 504 |  |
| `keyring` | `keyring.rs` | 139 |  |
| `leads` | `leads.rs` | 345 |  |
| `lifecycle` | `lifecycle.rs` | 62 |  |
| `lines` | `lines.rs` | 556 |  |
| `llm` | `llm/mod.rs` | 205 |  |
| `llm::classify` | `llm/classify.rs` | 179 |  |
| `llm::http` | `llm/http.rs` | 212 |  |
| `llm::net` | `llm/net.rs` | 217 |  |
| `llm::remote` | `llm/remote.rs` | 108 |  |
| `llm::remote_script` | `llm/remote_script.rs` | 294 |  |
| `llm::tool` | `llm/tool.rs` | 137 |  |
| `login` | `login.rs` | 316 |  |
| `marks` | `marks/mod.rs` | 124 |  |
| `marks::openers` | `marks/openers.rs` | 25 |  |
| `marks::table` | `marks/table.rs` | 80 |  |
| `mcp` | `mcp.rs` | 438 |  |
| `memory` | `memory.rs` | 335 |  |
| `mention` | `mention.rs` | 797 |  |
| `native` | `native.rs` | 240 |  |
| `new_session` | `new_session.rs` | 158 |  |
| `notify` | `notify.rs` | 156 |  |
| `pasted` | `pasted/mod.rs` | 171 |  |
| `pasted::base64` | `pasted/base64.rs` | 42 |  |
| `pasted::tests` | `pasted/tests.rs` | 253 | yes |
| `payload` | `payload.rs` | 108 |  |
| `pending` | `pending.rs` | 791 |  |
| `png` | `png/mod.rs` | 513 |  |
| `png::readers` | `png/readers.rs` | 334 | yes |
| `png::tests` | `png/tests.rs` | 514 | yes |
| `provider_record` | `provider_record/mod.rs` | 471 |  |
| `provider_record::tests` | `provider_record/tests.rs` | 696 | yes |
| `quota` | `quota.rs` | 429 |  |
| `registry` | `registry.rs` | 202 |  |
| `screentools` | `screentools/mod.rs` | 264 |  |
| `screentools::grants` | `screentools/grants.rs` | 104 |  |
| `screentools::tests` | `screentools/tests.rs` | 373 | yes |
| `screentools::work` | `screentools/work.rs` | 187 |  |
| `search` | `search.rs` | 192 |  |
| `session` | `session.rs` | 801 |  |
| `ssh` | `ssh.rs` | 677 |  |
| `ssh_remote` | `ssh_remote.rs` | 692 |  |
| `testgate` | `testgate.rs` | 97 | yes |
| `tokens` | `tokens.rs` | 281 |  |
| `tools` | `tools.rs` | 703 |  |
| `transfer` | `transfer.rs` | 394 |  |
| `transfer::tests` | `transfer/tests.rs` | 408 | yes |
| `vdisplay` | `vdisplay/mod.rs` | 485 |  |
| `worktree` | `worktree.rs` | 291 |  |
| `xjob` | `xjob.rs` | 70 |  |

## A2. Rust tests by top-level module (MEASURED; each test alone in its own process, tree RSS; 671 tests)

| module | tests | sum of single-test times (s) | slowest (s) | highest tree peak (MiB) |
|---|---:|---:|---:|---:|
| `gui` | 217 | 53.0 | 6.72 | 272 |
| `capture` | 17 | 21.6 | 11.85 | 151 |
| `create` | 30 | 16.0 | 1.10 | 15 |
| `pending` | 22 | 11.1 | 1.05 | 17 |
| `png` | 21 | 11.0 | 3.90 | 61 |
| `calls` | 19 | 9.0 | 4.52 | 15 |
| `history` | 20 | 7.9 | 1.08 | 15 |
| `screentools` | 20 | 6.0 | 1.09 | 15 |
| `clipwrite` | 11 | 4.9 | 3.04 | 14 |
| `provider_record` | 40 | 4.0 | 1.12 | 56 |
| `core` | 3 | 3.9 | 3.03 | 19 |
| `llm` | 18 | 3.5 | 2.52 | 9 |
| `helper` | 16 | 3.1 | 1.79 | 105 |
| `clipboard` | 27 | 2.4 | 0.96 | 105 |
| `folders` | 12 | 1.6 | 1.02 | 10 |
| `transfer` | 25 | 1.3 | 0.06 | 13 |
| `chats` | 9 | 1.3 | 0.87 | 20 |
| `lines` | 13 | 1.1 | 0.30 | 113 |
| `tools` | 14 | 0.8 | 0.06 | 0 |
| `accounts` | 1 | 0.8 | 0.75 | 12 |
| `leads` | 13 | 0.7 | 0.05 | 0 |
| `pasted` | 11 | 0.6 | 0.10 | 20 |
| `mcp` | 11 | 0.6 | 0.06 | 0 |
| `inbox_read` | 11 | 0.6 | 0.05 | 13 |
| `marks` | 6 | 0.5 | 0.16 | 8 |
| `ident` | 5 | 0.3 | 0.15 | 8 |
| `ssh` | 3 | 0.3 | 0.24 | 26 |
| `echo` | 6 | 0.3 | 0.05 | 0 |
| `handoff` | 6 | 0.3 | 0.05 | 0 |
| `vdisplay` | 2 | 0.3 | 0.24 | 171 |
| `computer` | 5 | 0.2 | 0.05 | 0 |
| `call_room` | 2 | 0.2 | 0.19 | 13 |
| `memory` | 4 | 0.2 | 0.09 | 16 |
| `fsview` | 4 | 0.2 | 0.05 | 9 |
| `native` | 4 | 0.2 | 0.05 | 0 |
| `worktree` | 2 | 0.2 | 0.11 | 11 |
| `mention` | 3 | 0.2 | 0.05 | 0 |
| `admin` | 2 | 0.1 | 0.05 | 12 |
| `notify` | 1 | 0.1 | 0.10 | 12 |
| `payload` | 2 | 0.1 | 0.05 | 0 |
| `quota` | 2 | 0.1 | 0.05 | 0 |
| `testgate` | 1 | 0.1 | 0.10 | 9 |
| `tokens` | 2 | 0.1 | 0.05 | 0 |
| `xjob` | 2 | 0.1 | 0.05 | 0 |
| `cli` | 1 | 0.1 | 0.05 | 0 |
| `codex` | 1 | 0.1 | 0.05 | 0 |
| `creds` | 1 | 0.1 | 0.05 | 9 |
| `registry` | 1 | 0.1 | 0.05 | 0 |
| `search` | 1 | 0.1 | 0.05 | 0 |
| `ssh_remote` | 1 | 0.1 | 0.05 | 0 |

## A3. Every Python test file (MEASURED time and peak from one cold run; "exercises" READ from the file)

| test file | tests | time (s) | peak (MiB) | areas (testmap) | exercises (read from the file) |
|---|---:|---:|---:|---|---|
| `host_replacement` | 18 | 27.7 | 204 | gui,host | host, lifecycle, gui (update) |
| `card_window` | 3 | 22.2 | 236 | gui,host | gui (cardview), pending, calls |
| `activity` | 5 | 18.7 | 161 | gui,host | session, gui (telemetry) |
| `client_integration` | 2 | 18.3 | 242 | gui,host,synth | gui, host, session (synth agent) |
| `korean_ime` | 1 | 11.5 | 248 | gui,host,synth | gui (editor, IME) |
| `peer_reply_copy_and_dir_nav` | 3 | 10.7 | 200 | cli,gui,host | gui, calls |
| `host_core` | 4 | 10.0 | 40 | host,synth | host, session, core |
| `window_keys` | 6 | 9.5 | 241 | gui,host | gui |
| `conversation` | 10 | 8.9 | 423 | host | transfer, lines |
| `selection` | 28 | 8.9 | 68 | - | lico_measure (suite selection, receipts) |
| `first_view` | 4 | 8.9 | 197 | gui,host,synth | gui, host |
| `menu_bar` | 5 | 8.8 | 198 | gui,host | gui (menu) |
| `tool_connection` | 16 | 7.9 | 390 | host | tools, mcp, http |
| `paste_open` | 3 | 7.9 | 141 | gui,host | gui, pasted |
| `llm_dialog_follow_up` | 3 | 7.6 | 146 | gui,host | gui (llm_find), llm |
| `session_memory` | 1 | 7.4 | 291 | gui,host | memory, gui |
| `computer_use` | 5 | 7.3 | 289 | cli,gui,host | computer, vdisplay, helper, xjob |
| `frame_cost` | 8 | 7.3 | 141 | gui,host,synth | gui (fonts, scroll) |
| `agent_tools` | 16 | 7.0 | 532 | gui,host | tools, mcp, capture, clipboard |
| `llm_discover_ssh` | 4 | 6.5 | 61 | cli,host | llm, ssh_remote |
| `unicode_clients` | 2 | 6.4 | 263 | gui,host | gui |
| `view_lifecycle` | 2 | 5.8 | 132 | gui,host,synth | host, ipc |
| `llm_discover` | 6 | 5.6 | 46 | cli,host | llm |
| `call_rooms_and_mentions` | 3 | 5.5 | 180 | cli,gui,host | call_room, mention, calls |
| `ssh_sessions_and_files` | 3 | 5.3 | 152 | cli,gui,host | ssh, ssh_remote, folders |
| `middle_paste` | 1 | 5.0 | 202 | gui,host | gui, clipboard |
| `host_control` | 8 | 4.9 | 45 | host,synth | host, ipc, core |
| `toolbar_and_nav` | 1 | 4.9 | 196 | cli,gui,host | gui |
| `path_status_bar` | 3 | 4.9 | 154 | gui,host | gui (pathbar) |
| `picture_paste` | 3 | 4.7 | 235 | gui,host | gui (picture), pasted, clipboard |
| `native_first_run` | 2 | 4.7 | 155 | gui,host | native, gui |
| `codex_adapter` | 12 | 4.7 | 71 | host | codex |
| `clipboard_write` | 14 | 4.4 | 151 | cli,gui,host | clipwrite, clipboard, gui |
| `llm_end_to_end_m13` | 1 | 4.4 | 148 | gui,host,cli | llm, gui |
| `running_elsewhere` | 4 | 4.3 | 59 | host,cli | chats, native |
| `login` | 3 | 4.3 | 59 | cli,host | login, creds |
| `maximized_start` | 6 | 4.2 | 149 | gui | gui (windowstate) |
| `session_comms_and_ux_fixes` | 4 | 4.2 | 193 | cli,gui,host | tools, mcp, transfer, gui |
| `mask_code_line` | 1 | 4.1 | 143 | gui,host | gui, login |
| `stale_host` | 2 | 4.1 | 139 | cli,gui,host,synth | host, lifecycle |
| `editor_selection` | 2 | 4.0 | 176 | gui,host | gui (editor) |
| `left_directory_sidebar` | 1 | 3.9 | 197 | cli,gui,host | gui (sidebar), folders |
| `recovery` | 5 | 3.8 | 73 | host | host, session (log recovery) |
| `login_real_shape` | 2 | 3.8 | 144 | gui,host | login, gui |
| `ssh_accounts` | 2 | 3.7 | 157 | cli,gui,host | ssh, accounts |
| `clipboard_read` | 36 | 3.7 | 111 | cli | clipboard, helper, cli |
| `permission_mode` | 4 | 3.5 | 187 | gui,host | session, claude, codex, gui |
| `m14_end_to_end` | 1 | 3.5 | 168 | gui,host | gui |
| `collapsible_tools_and_create_tab` | 1 | 3.3 | 224 | cli,gui,host | gui (tool_fold), create |
| `workspace_files` | 2 | 3.3 | 166 | cli,gui,host | fsview, folders, gui |
| `reconnect_window` | 4 | 3.2 | 142 | gui,host | gui (reconnect) |
| `send_to_session_window` | 2 | 3.1 | 196 | gui,host | gui (sendto), transfer |
| `chats_pagination_and_resume` | 2 | 3.1 | 216 | cli,gui,host | chats, gui |
| `single_executable` | 2 | 2.8 | 114 | cli,gui,host | unified (cli, host, gui) |
| `provider_isolation` | 2 | 2.8 | 62 | host,synth | host, claude, codex |
| `login_window` | 2 | 2.8 | 212 | gui,host | login, gui |
| `llm_window` | 2 | 2.7 | 147 | gui,host | llm, gui |
| `llm_end_to_end` | 1 | 2.7 | 152 | gui,host,cli | llm, gui |
| `workspace_navigator` | 2 | 2.6 | 197 | cli,gui,host | gui, folders |
| `llm_discover_speed` | 2 | 2.5 | 58 | cli,host | llm |
| `claude_adapter` | 11 | 2.5 | 97 | host | claude |
| `folders` | 4 | 2.4 | 64 | cli,host | folders, session |
| `tool_send` | 25 | 2.4 | 1094 | host | tools, transfer, pending |
| `input_box` | 2 | 2.3 | 186 | gui,host | gui (editor), session |
| `window_stdout` | 2 | 2.3 | 154 | gui,host | gui |
| `session_lifecycle` | 3 | 2.2 | 84 | host | session |
| `protocol_conformance` | 4 | 2.2 | 51 | host,synth | lico_protocol, host |
| `admin_controls` | 2 | 2.1 | 153 | cli,gui,host | admin, ipc, gui, cli |
| `session_actions_window` | 1 | 2.1 | 174 | gui,host | gui, session |
| `picture_input` | 12 | 2.0 | 94 | host | pasted, png, session |
| `workspace_restore` | 3 | 1.9 | 153 | cli,gui,host | gui, clipboard (settings) |
| `everyday_comfort` | 2 | 1.9 | 174 | cli,gui,host | worktree, session, gui |
| `new_session_screen` | 1 | 1.9 | 154 | gui,host | gui (newsession), folders |
| `native_end_to_end` | 1 | 1.9 | 179 | gui,host | native, chats, gui |
| `session_tabs_and_telemetry` | 1 | 1.8 | 196 | cli,gui,host | gui (tabs, telemetry) |
| `new_chat_modal` | 1 | 1.7 | 197 | gui,host | gui (newchat), chats |
| `rig_teardown` | 5 | 1.6 | 158 | gui,host | lico_measure (rig) |
| `mark_table` | 10 | 1.6 | 29 | - | marks |
| `account_lifecycle` | 3 | 1.6 | 138 | cred,host | accounts, credcmd, creds, keyring, dbus |
| `navigation` | 2 | 1.5 | 193 | gui,host | gui |
| `tool_create` | 19 | 1.4 | 544 | host | tools, create |
| `running_elsewhere_window` | 1 | 1.2 | 172 | gui,host | chats, gui |
| `directory_pagination` | 1 | 1.2 | 135 | gui,host | gui, fsview, folders |
| `chats_end_to_end` | 1 | 1.2 | 172 | cli,gui,host | chats, gui |
| `host_resources` | 1 | 1.1 | 41 | host,synth | host, memory |
| `transfer` | 13 | 1.1 | 518 | cli,host | transfer |
| `hidden_and_ssh` | 3 | 1.1 | 133 | cli,gui,host | fsview, folders, ssh |
| `capture_cli` | 15 | 1.0 | 124 | cli | capture, png, cli |
| `credential_store` | 2 | 1.0 | 46 | cred | creds, keyring, dbus |
| `forged_leads` | 9 | 0.9 | 61 | host | leads, lines, marks |
| `conversation_view` | 1 | 0.9 | 161 | cli,gui,host | gui, lines |
| `quota_display` | 1 | 0.9 | 175 | cli,gui,host | quota, gui (telemetry) |
| `chats_resume_latest` | 6 | 0.9 | 83 | cli,host | chats, folders, cli |
| `m02_milestone_review` | 1 | 0.8 | 193 | cli,gui,host | memory, gui (regression sweep) |
| `accounts` | 2 | 0.8 | 96 | cred,host | accounts, creds, keyring, dbus |
| `resource_scope` | 2 | 0.8 | 61 | - | lico_measure (Python) |
| `tool_history` | 8 | 0.7 | 360 | host | tools, history |
| `chat_list_polish` | 1 | 0.7 | 154 | gui,host | gui (sidebar_chats), chats |
| `llm_tool` | 3 | 0.6 | 80 | host | llm, tools |
| `identity` | 1 | 0.6 | 128 | cli,gui,host | ident, cli, gui |
| `input_echo` | 5 | 0.5 | 62 | cli,host | echo, session, core |
| `chats_rename` | 4 | 0.5 | 29 | cli,host | chats, cli, ipc |
| `build_commit` | 6 | 0.4 | 138 | - | ident (build.rs) |
| `xjob` | 10 | 0.4 | 124 | cli | xjob, helper, computer |
| `text_rules` | 5 | 0.3 | 46 | host | folders, lines, lico_protocol |
| `native_account` | 5 | 0.3 | 81 | cli,host | native, accounts |
| `adapter_contract` | 5 | 0.3 | 74 | - | Python adapters/simulators (not a Rust module) |
| `no_hidden_characters` | 2 | 0.2 | 29 | - | repository text guard (marks table) |
| `hostile` | 1 | 0.2 | 37 | host,synth | host, lico_measure (hostile) |
| `control_parity` | 4 | 0.2 | 29 | cli,host,synth | cli, ipc, http, tools (parity of surfaces) |
| `reference_mapping` | 5 | 0.1 | 28 | - | lico_protocol mappings (Python) |
| `size_ratchet` | 2 | 0.1 | 28 | - | repository size guard |
| `model_names` | 3 | 0.1 | 28 | - | folders (valid_model), lico_protocol |
| `python_requirement` | 1 | 0.0 | 28 | - | Python tooling guard |
| `report` | 1 | 0.0 | 28 | - | lico_measure (Python) |
| `decision` | 1 | 0.0 | 28 | - | lico_measure (Python) |
| `latency` | 1 | 0.0 | 28 | - | lico_measure (Python) |

## A4. `src/lico_measure`, file by file (verdict K keep as Rust, X retire; the functions are the top-level ones of the file)

| file | lines | verdict | functions and classes (top level) |
|---|---:|---|---|
| `__init__.py` | 1 | K | - |
| `__main__.py` | 422 | X (October experiment commands; the `notices` command moves to the release tool) | `_capture`, `_git`, `environment`, `_binary_hashes`, `_text_workload`, `cmd_run`, `cmd_bundle`, `cmd_hostcmp`, `cmd_p02`, `cmd_p03`, `cmd_real_turn`, `cmd_p04`, `cmd_p05`, `cmd_p05_bundle`, `_p02_hashes_path`, `cmd_p04_bundle`, `cmd_notices`, `_p02_hashes`, `cmd_p02_bundle`, `cmd_p03_bundle`, `cmd_providers`, `main` |
| `bundle.py` | 194 | X | `_shot`, `take_screenshots`, `_cell`, `_find`, `_render`, `build_bundle` |
| `bundle_p02.py` | 119 | X | `_pre`, `collect_p02`, `render`, `build` |
| `bundle_p03.py` | 184 | X | `_pre`, `_wait_text`, `screens`, `render`, `build` |
| `bundle_p04.py` | 111 | X | `_pre`, `_table`, `render`, `build` |
| `bundle_p05.py` | 72 | X | `_targets`, `render`, `build` |
| `captures.py` | 314 | K (the window rig) | `class ProductWindow`, `capture_three`, `capture_backspace`, `capture_paste` |
| `decision.py` | 52 | X (one test; replace or retire with it) | `class DecisionError`, `_problems`, `validate_claim`, `recommend` |
| `experiments.py` | 516 | X (the October experiments; `Rig` ideas go to the Rust test kit) | `display_geometry`, `_mib`, `class Rig`, `host_only_kb`, `_load_info`, `_idle_once`, `idle_memory`, `_cycle_and_measure`, `hidden_growth`, `_switch_samples`, `activation`, `_library_paths`, `_evict`, `_launch_and_time`, `restart`, `_cpu_percent`, `idle_cpu`, `continuity`, `inventory`, `glyph_stress`, `korean_ime` |
| `fakewm.py` | 165 | K (the window-manager model) | `class MapRequest`, `class ClientMessage`, `class ConfigureRequest`, `class Changes`, `class Event`, `load`, `main` |
| `hostcmp.py` | 97 | X (compares the retired host variants) | `_mib`, `_rss_pss`, `_one`, `compare` |
| `hostile.py` | 295 | X after `test_hostile` is replaced by a host contract test | `_subscribers`, `_case`, `run_hostile_cases`, `http_request`, `run_hostd_suite` |
| `ime.py` | 214 | K (the Korean input method rig) | `class InputMethod`, `_focus`, `_analyse`, `class CompositionRig`, `_focus_when_mapped`, `_collect`, `composition_run` |
| `keyring_rig.py` | 103 | K (private bus and keyring for credential tests) | `class PrivateKeyring` |
| `latency.py` | 34 | X (one test) | `measure_switch` |
| `ledger.py` | 211 | K -> xtask (per-test time and peak) | `scan`, `kind`, `class Tree`, `class Ledger`, `end_like_ctrl_c`, `pytest_addoption`, `pytest_configure`, `pytest_unconfigure`, `pytest_collection_finish`, `pytest_runtest_logstart`, `pytest_runtest_logreport`, `pytest_runtest_logfinish`, `pytest_sessionfinish` |
| `ledger_report.py` | 72 | K -> xtask | `classify`, `areas`, `table` |
| `legacy.py` | 25 | X (the retired prototypes) | `_marked`, `clients`, `hosts` |
| `notices.py` | 100 | K -> release tooling (third-party notices) | `registry_src`, `locked_packages`, `acceptable`, `package_info`, `build`, `problems` |
| `p02.py` | 499 | split: K `ProtoConn`, `start`, `read_log`, `cli`, `link_roles`; X the rest (M01-P02 evidence) | `link_roles`, `class ProtoConn`, `start`, `read_log`, `echoes`, `cli`, `_mib`, `gui_kill`, `view_switch`, `_host_rss`, `drain`, `host_resources`, `backpressure`, `input_integrity`, `cli_transcript`, `input_states_demo`, `gui_kill_shots` |
| `p03.py` | 319 | X (M01-P03 evidence) | `_simulated_host`, `_provider_messages`, `_until`, `adapter_integrity`, `_completed`, `_state`, `_settled`, `_approvals`, `_pending`, `simulated_session_demo`, `provider_evidence`, `provider_idle_memory`, `real_turn_dry_run`, `hostile_adapter_check`, `collect_p03` |
| `p04.py` | 451 | X (M01-P04 evidence); `secret_sink_scan` idea K as a credential test | `_run`, `_peak_rss_mib`, `credential_store_costs`, `real_turn_dry_run`, `_claude_host`, `_received_lines`, `_until`, `_status`, `claude_integrity`, `_scan`, `secret_sink_scan`, `accounts_host_memory`, `transcripts`, `hostile_adapter_checks`, `capability_records`, `collect_p04` |
| `p05.py` | 118 | X (M01-P05 evidence) | `inventory`, `recovery`, `login_memory`, `collect_p05` |
| `procscope.py` | 160 | K -> xtask (process-tree scopes) | `class ScopeError`, `class ProcessSample`, `class Scope`, `_parent_map`, `tree_pids`, `_read_rollup`, `_read_status`, `cpu_seconds`, `collect_scopes`, `mapping_top` |
| `providers.py` | 175 | X (provider capability evidence; replaced by the provider components) | `codex_binary`, `_private_env`, `_start`, `_failed`, `installed_version`, `generate_schema`, `_methods`, `schema_inventory`, `problems`, `record_codex`, `message_errors` |
| `real_turn.py` | 153 | X (an owner-run real turn script; may stay as a private tool) | `_pinned`, `run`, `_receipt`, `_finish`, `main` |
| `receipts.py` | 132 | K -> xtask (the receipt, file level) | `build_root`, `digest`, `binaries`, `reach`, `key`, `path_of`, `valid`, `failed`, `record`, `forget`, `mark_failed`, `durations`, `save_durations` |
| `report.py` | 84 | X (one test) | `class ReportError`, `class Target`, `build_measurement`, `build_report` |
| `rig.py` | 286 | K (Xvfb, the host, the client, `private_dir`) | `candidate_command`, `short_base`, `_client_home`, `session_rgb`, `class Xvfb`, `_host_command`, `host_impls`, `effective_history`, `mixed_script_text`, `output_kind`, `class Host`, `class HostConn`, `class Client`, `private_dir`, `finished_lines`, `files_under` |
| `stats.py` | 19 | K (two small functions) | `percentile_nearest_rank`, `median` |
| `suite.py` | 371 | K -> xtask (the runner, budgets, acceptance) | `budget`, `allowed_seconds`, `over_budget`, `source_hash`, `ensure_build`, `ensure_cargo_test_build`, `class Job`, `pytest_job`, `cargo_job`, `execute`, `passed`, `use_receipts`, `run`, `marked`, `acceptance`, `legacy`, `select`, `main` |
| `testmap.py` | 317 | K -> xtask (selection by changed files; superseded by component receipts) | `class Selection`, `glob_match`, `load_map`, `git`, `tracked`, `changed`, `_candidates`, `python_graph`, `importers`, `rust_module`, `_group_heads`, `crate_names`, `rust_uses`, `rust_dependents`, `select`, `render` |
| `workload.py` | 37 | X | `class Record`, `record`, `records` |
| `xpixel.py` | 111 | K (pixel and selection reader over libX11) | `class PixelReader` |

> Note (M16-P01, 2026-11-17): the retired prototypes this document names (`src/candidates`, `src/host/rust`, `src/host/c`, `legacy.py`, `hostcmp.py`, the `legacy_candidates` marker) were deleted in M16-P01 on the owner's answer A; git history keeps them at `6712fe5`.
