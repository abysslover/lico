//! The terminal tab without a host (`M23-P03`): what the window does with the host's answers (`terminal.list`, `terminal.open`, `terminal.watch`), what it keeps of them, and what it
//! queues for the shell. The window here has no socket, so nothing is sent: the answers are put in as the host would give them. The keys themselves are `keys_tests.rs`, the real
//! window with a real host is `tests/test_terminal_tab.py`.

use serde_json::{json, Value};
use winit::event::MouseScrollDelta;
use winit::keyboard::NamedKey;

use super::keys::{Held, Lico, Logical};
use super::screen::Sel;
use super::super::scroll_oracle::app;
use super::super::ui::Row;
use super::super::App;
use super::view::{text_request, TermJob};

fn listed(rows: &[(&str, &str)]) -> Value {
    json!({"ok": 1, "terminals": rows.iter().map(|(tab, cwd)| json!({"tab": tab, "cwd": cwd, "shell": "/bin/sh", "pid": 7, "title": "", "cols": 80, "rows": 24, "running": true, "exit": null, "started": 1})).collect::<Vec<_>>(), "max_terminals": 8})
}

fn screen(tab: &str, seq: u64, lines: &[&str]) -> Value {
    json!({"ok": 1, "tab": tab, "seq": seq, "cols": 80, "rows": 24, "title": "", "cursor": [0, 0], "modes": {}, "scroll": 0, "top": 0, "total": 24,
           "lines": lines.iter().map(|t| json!([[t, -1, -1, 0]])).collect::<Vec<_>>(), "wrapped": [], "bells": 0, "running": true, "exit": null})
}

fn opened(tab: &str, cwd: &str) -> Value {
    json!({"ok": 1, "tab": tab, "pid": 7, "cwd": cwd, "shell": "/bin/sh", "cols": 80, "rows": 24})
}

/// A window with one terminal shown (`t1`, generation 1: the one a watch has to carry to be taken).
fn shown() -> App {
    let mut a = app(1.0, false);
    a.on_terminal_async(TermJob::Open, opened("t1", "/work/one"));
    a
}

fn watch(a: &mut App, gen: u64, reply: Value) {
    a.on_terminal_async(TermJob::Watch { tab: "t1".into(), gen }, reply);
}

fn tab(a: &App) -> &super::view::TTab {
    &a.ui.terminal.tabs[0]
}

#[test]
fn the_hosts_list_is_adopted_tab_by_tab_without_showing_any_of_them() {
    let mut a = app(1.0, false);
    a.on_terminal_async(TermJob::List, listed(&[("t1", "/work/one"), ("t2", "/work/two")]));
    assert_eq!(a.ui.terminal.tabs.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["t1", "t2"]);
    assert!(a.ui.terminal.listed && a.ui.terminal.active.is_none() && !a.terminal_active() && !a.page_active(), "adopting is not showing");
    a.on_terminal_async(TermJob::List, listed(&[("t1", "/elsewhere"), ("t3", "/work/three")]));
    assert_eq!(a.ui.terminal.tabs.iter().map(|t| (t.id.as_str(), t.cwd.as_str())).collect::<Vec<_>>(), [("t1", "/work/one"), ("t2", "/work/two"), ("t3", "/work/three")], "a tab already known is not replaced, a new one is added");
    a.on_terminal_async(TermJob::List, json!({"ok": 0, "error": "host_unreachable"}));
    assert_eq!(a.ui.terminal.tabs.len(), 3, "a list that fails changes nothing");
}

#[test]
fn an_opened_terminal_is_shown_with_its_header_and_the_row_of_keys() {
    let a = shown();
    assert_eq!(a.ui.terminal.active.as_deref(), Some("t1"));
    assert!(a.terminal_active() && a.page_active() && !a.browser_active());
    let header = a.terminal_header().unwrap();
    assert!(header.contains("terminal") && header.contains("/work/one") && header.contains("not fenced"), "{header}");
    assert!(a.terminal_status_row().contains("Ctrl+Shift+C copy") && a.terminal_status_row().contains("Ctrl+Shift+W close") && a.terminal_status_row().chars().count() < 100, "{}", a.terminal_status_row());
    assert!(tab(&a).gen >= 1 && tab(&a).rearm && !tab(&a).watching, "the shown tab is to ask for its screen after the next paint");
    assert_eq!(a.terminal_tab_entries(), [("t1".to_string(), ">_ one".to_string(), true)]);
}

#[test]
fn a_refused_open_says_why_in_the_words_of_the_window() {
    let mut a = app(1.0, false);
    a.on_terminal_async(TermJob::Open, json!({"ok": 0, "error": "tab_limit"}));
    assert_eq!(a.ui.flash.as_deref(), Some("eight terminals are open: close one first"));
    assert!(a.ui.terminal.tabs.is_empty() && !a.terminal_active());
    a.on_terminal_async(TermJob::Open, json!({"ok": 0, "error": "folder_missing"}));
    assert!(a.ui.flash.as_deref().unwrap().contains("folder cannot be used"), "{:?}", a.ui.flash);
}

#[test]
fn the_screen_of_a_watch_is_kept_when_it_is_of_the_current_generation_and_dropped_when_it_is_not() {
    let mut a = shown();
    let gen = tab(&a).gen;
    watch(&mut a, gen, screen("t1", 5, &["hello", "world"]));
    assert_eq!((tab(&a).seq, tab(&a).ask), (5, 5), "the next watch asks for what comes after 5");
    assert_eq!(&tab(&a).screen.plain_lines()[..2], ["hello", "world"]);
    assert!(tab(&a).rearm, "a live screen is watched again after its paint");
    watch(&mut a, gen + 7, screen("t1", 9, &["stale"]));
    assert_eq!(tab(&a).seq, 5, "the answer of a watch that was replaced is not taken");
    watch(&mut a, gen, json!({"ok": 0, "error": "host_unreachable"}));
    assert_eq!(tab(&a).seq, 5, "a failed watch keeps the last screen");
    watch(&mut a, gen, json!({"ok": 1, "tab": "t1"}));
    assert_eq!(tab(&a).seq, 5, "an answer with no change number is refused");
}

#[test]
fn a_view_scrolled_back_asks_for_no_live_screen_and_typing_brings_it_back() {
    let mut a = shown();
    let gen = tab(&a).gen;
    let mut reply = screen("t1", 6, &["old"]);
    (reply["scroll"], reply["total"], reply["top"]) = (json!(10), json!(100), json!(66));
    watch(&mut a, gen, reply);
    assert_eq!((tab(&a).scroll, tab(&a).screen.top), (10, 66));
    assert!(!tab(&a).rearm, "no live watch while the view is back: it is a still picture");
    assert!(a.terminal_status_row().contains("history: 10 lines up"), "{}", a.terminal_status_row());
    let before = tab(&a).gen;
    assert!(a.terminal_commit("x"), "typing is the shell's");
    assert_eq!(tab(&a).scroll, 0, "typing returns to the live screen");
    assert!(tab(&a).gen > before && tab(&a).rearm && tab(&a).ask == 0, "and asks at once for it, with a generation of its own");
}

#[test]
fn the_wheel_and_the_history_keys_move_by_rows_and_stay_inside_what_the_host_has() {
    let mut a = shown();
    let gen = tab(&a).gen;
    let mut reply = screen("t1", 6, &["x"]);
    (reply["total"], reply["top"]) = (json!(60), json!(36));
    watch(&mut a, gen, reply);
    a.terminal_scroll_by(10);
    assert_eq!(tab(&a).scroll, 10);
    assert!(tab(&a).rearm && !tab(&a).watching && tab(&a).ask == 0, "a scroll asks at its own offset, at once");
    a.terminal_scroll_by(500);
    assert_eq!(tab(&a).scroll, 36, "no further than the first line the host has (60 lines, 24 on the screen)");
    a.terminal_scroll_by(-1000);
    assert_eq!(tab(&a).scroll, 0, "and no further than the live screen");
}

#[test]
fn a_tab_the_host_no_longer_has_is_dropped_and_the_view_goes_to_the_next_or_to_the_first_screen() {
    let mut a = shown();
    a.on_terminal_async(TermJob::Open, opened("t2", "/work/two"));
    assert_eq!(a.ui.terminal.active.as_deref(), Some("t2"));
    a.on_terminal_async(TermJob::Watch { tab: "t2".into(), gen: a.ui.terminal.tabs[1].gen }, json!({"ok": 0, "error": "unknown_tab"}));
    assert_eq!(a.ui.terminal.tabs.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["t1"]);
    assert_eq!(a.ui.terminal.active.as_deref(), Some("t1"), "the other terminal is shown");
    let gen = tab(&a).gen;
    watch(&mut a, gen, json!({"ok": 0, "error": "unknown_tab"}));
    assert!(a.ui.terminal.tabs.is_empty() && a.ui.terminal.active.is_none() && !a.terminal_active(), "the last one gone: the first screen");
}

#[test]
fn what_is_typed_waits_for_the_shell_in_order_and_is_joined_while_one_request_is_out() {
    let mut a = shown();
    assert!(a.terminal_commit("\u{d55c}\u{ae00}"), "a committed Korean text is the shell's");
    assert!(a.terminal_commit("ab\ncd\u{7}"), "control characters are not typed");
    let queued: Vec<(String, Vec<u8>)> = a.ui.terminal.queue.iter().cloned().collect();
    assert_eq!(queued, [("t1".to_string(), "\u{d55c}\u{ae00}abcd".as_bytes().to_vec())], "one tab's text is joined, in order");
    a.ui.terminal.queue.clear();
    a.on_terminal_async(TermJob::Input, json!({"ok": 0, "error": "terminal_ended"}));
    assert!(!a.ui.terminal.input_inflight && a.ui.terminal.queue.is_empty());
}

#[test]
fn nothing_is_sent_to_an_ended_shell_and_the_row_says_how_to_leave() {
    let mut a = shown();
    let gen = tab(&a).gen;
    let mut reply = screen("t1", 4, &["bye"]);
    (reply["running"], reply["exit"]) = (json!(false), json!(3));
    watch(&mut a, gen, reply);
    assert!(a.terminal_commit("x"), "the text is taken (the tab is shown) and goes nowhere");
    assert!(a.ui.terminal.queue.is_empty());
    assert!(a.terminal_status_row().contains("the shell ended (exit 3)") && a.terminal_status_row().contains("Enter closes this tab"), "{}", a.terminal_status_row());
    assert!(a.terminal_header().unwrap().contains("ended"));
}

#[test]
fn a_paste_is_wrapped_as_the_program_asked_and_cut_into_pieces_the_host_takes() {
    let mut a = shown();
    let gen = tab(&a).gen;
    let mut reply = screen("t1", 3, &["$"]);
    reply["modes"] = json!({"bracketed_paste": true});
    watch(&mut a, gen, reply);
    assert!(a.terminal_paste(&Ok("a\nb".into())));
    assert_eq!(a.ui.terminal.queue.front().map(|(_, b)| b.clone()), Some(b"\x1b[200~a\nb\x1b[201~".to_vec()), "bracketed: the line break is kept inside the brackets");
    a.ui.terminal.queue.clear();
    let mut plain = shown();
    assert!(plain.terminal_paste(&Ok("a\nb\x1b[31m".into())));
    assert_eq!(plain.ui.terminal.queue.front().map(|(_, b)| b.clone()), Some(b"a\rb[31m".to_vec()), "plain: line breaks are returns and no escape reaches the shell");
    let mut big = shown();
    assert!(big.terminal_paste(&Ok("x".repeat(100_000))));
    let sizes: Vec<usize> = big.ui.terminal.queue.iter().map(|(_, b)| b.len()).collect();
    assert!(sizes.len() >= 3 && sizes.iter().all(|n| *n <= 64 * 1024) && sizes.iter().sum::<usize>() == 100_000, "{sizes:?}");
    assert!(plain.terminal_paste(&Err("timeout")), "a paste that failed is still the terminal's");
    assert_eq!(plain.ui.flash.as_deref(), Some("Nothing to paste."));
}

#[test]
fn a_session_a_call_room_or_a_panel_in_front_hides_the_terminal_and_the_keys_are_not_its() {
    let mut a = shown();
    assert!(a.terminal_active());
    a.session = "s1".into();
    assert!(!a.terminal_active() && !a.page_active() && !a.terminal_commit("x"), "a session is shown: the terminal is not");
    a.session.clear();
    assert!(a.terminal_active(), "(the tab is still there when the session goes)");
    a.run_cmd(super::super::menu::Cmd::Accounts);
    assert!(!a.terminal_active() && !a.terminal_paste(&Ok("x".into())), "a panel is in front");
}

#[test]
fn the_state_a_test_reads_says_what_the_window_holds() {
    let mut a = shown();
    let gen = tab(&a).gen;
    watch(&mut a, gen, screen("t1", 2, &["one", "two"]));
    let j = a.terminal_json();
    assert_eq!((j["active"].clone(), j["shown"].clone(), j["listed"].clone()), (json!("t1"), json!(true), json!(false)));
    assert_eq!(j["tabs"][0]["id"], "t1");
    assert_eq!((j["tabs"][0]["seq"].clone(), j["tabs"][0]["running"].clone(), j["tabs"][0]["cols"].clone()), (json!(2), json!(true), json!(80)));
    assert_eq!(j["lines"][0], "one");
    assert!(j["header"].as_str().unwrap().contains("terminal"));
}

#[test]
fn another_view_taking_the_window_takes_the_terminal_out_of_it() {
    let mut a = shown();
    a.browser_prompt(false);
    assert!(a.browser_active() && !a.terminal_active(), "a browser tab shown: the terminal is not (one page at a time)");
    a.on_terminal_async(TermJob::List, listed(&[("t1", "/work/one")]));
    let id = a.ui.terminal.tabs[0].id.clone();
    a.terminal_activate(&id);
    assert!(a.terminal_active() && !a.browser_active(), "and the other way round");
}

fn held(ctrl: bool, shift: bool) -> Held {
    Held { ctrl, shift, alt: false, logo: false }
}

fn plain() -> Held {
    held(false, false)
}

fn queued(a: &App) -> Vec<(String, Vec<u8>)> {
    a.ui.terminal.queue.iter().cloned().collect()
}

/// The shown tab told that its screen holds `total` lines, the one on view starting at `top`, with the program's `modes`.
fn holding(a: &mut App, total: u64, top: u64, modes: Value) {
    let gen = tab(a).gen;
    let mut reply = screen("t1", 5, &["$"]);
    (reply["total"], reply["top"], reply["modes"]) = (json!(total), json!(top), modes);
    watch(a, gen, reply);
}

/// Where the grid is painted: 80 x 24 cells of 10 x 20 pixels from the corner.
fn grid_at(a: &mut App) {
    a.ui.terminal.rect = [0, 0, 800, 480];
    a.ui.terminal.cell = (10.0, 20.0);
    a.ui.terminal.grid = (80, 24);
}

#[test]
fn a_key_is_the_shells_unless_a_menu_or_a_composition_has_it() {
    let mut a = shown();
    assert!(a.terminal_key(Logical::Char("l"), Some("l"), plain(), false, false));
    assert_eq!(queued(&a), [("t1".to_string(), b"l".to_vec())]);
    a.ui.terminal.queue.clear();
    a.preedit = "\u{d55c}".into();
    assert!(a.terminal_key(Logical::Char("x"), Some("x"), plain(), false, false), "a composition owns the keys: they are taken");
    assert!(a.ui.terminal.queue.is_empty(), "and none of them reaches the shell");
    a.preedit.clear();
    a.ui.menu_open = Some(1);
    assert!(!a.terminal_key(Logical::Char("x"), Some("x"), plain(), false, false), "an open dropdown has the keys first");
    assert!(a.ui.terminal.queue.is_empty());
    a.ui.menu_open = None;
    assert!(!a.terminal_key(Logical::Char("x"), Some("x"), Held { logo: true, ..plain() }, false, false), "the logo key goes on to the window");
    assert!(a.ui.terminal.queue.is_empty());
}

#[test]
fn on_an_ended_shell_every_key_is_taken_and_only_enter_closes_the_tab() {
    let mut a = shown();
    let gen = tab(&a).gen;
    let mut reply = screen("t1", 4, &["bye"]);
    (reply["running"], reply["exit"]) = (json!(false), json!(0));
    watch(&mut a, gen, reply);
    assert!(a.terminal_key(Logical::Char("x"), Some("x"), plain(), false, false));
    assert!(a.terminal_key(Logical::Named(NamedKey::ArrowUp), None, plain(), false, false));
    assert!(a.ui.terminal.queue.is_empty() && a.ui.terminal.tabs.len() == 1, "nothing is sent and the tab stays");
    assert!(a.terminal_key(Logical::Named(NamedKey::Enter), None, plain(), false, true));
    assert!(a.ui.terminal.tabs.is_empty() && !a.terminal_active(), "Enter closes it");
}

#[test]
fn the_keys_that_are_licos_act_once_the_history_keys_repeat_and_a_page_is_a_screen_less_a_row() {
    let mut a = shown();
    holding(&mut a, 200, 176, json!({}));
    let pgup = Logical::Named(NamedKey::PageUp);
    assert!(a.terminal_key(pgup, None, held(false, true), false, false));
    assert_eq!(tab(&a).scroll, 23, "a page back is the 24 rows of the screen less one, so that a line of what was read stays");
    assert!(a.terminal_key(pgup, None, held(false, true), true, false), "held down it goes on");
    assert_eq!(tab(&a).scroll, 46);
    assert!(a.terminal_key(Logical::Char("w"), Some("W"), held(true, true), true, false), "the key is Lico's even when it repeats");
    assert_eq!(a.ui.terminal.tabs.len(), 1, "but a held Ctrl+Shift+W closes nothing");
    assert!(a.terminal_key(Logical::Char("w"), Some("W"), held(true, true), false, false));
    assert!(a.ui.terminal.tabs.is_empty(), "pressed once it closes the tab");
}

#[test]
fn what_waits_for_another_tab_is_not_joined_and_an_error_of_a_dead_tab_empties_the_wait() {
    let mut a = shown();
    a.ui.terminal.queue.push_back(("t0".into(), b"a".to_vec()));
    a.ui.terminal.input_inflight = true;
    assert!(a.terminal_commit("b"));
    assert_eq!(queued(&a), [("t0".to_string(), b"a".to_vec()), ("t1".to_string(), b"b".to_vec())], "text for one tab is never put on the end of another's");
    a.on_terminal_async(TermJob::Input, json!({"ok": 0, "error": "terminal_ended"}));
    assert!(a.ui.terminal.queue.is_empty() && !a.ui.terminal.input_inflight, "a shell that ended takes nothing more: the wait is emptied");
    a.ui.terminal.queue.push_back(("t1".into(), b"c".to_vec()));
    a.ui.terminal.input_inflight = true;
    a.on_terminal_async(TermJob::Input, json!({"ok": 0, "error": "host_unreachable"}));
    assert_eq!(queued(&a), [("t1".to_string(), b"c".to_vec())], "another error keeps what the person typed");
}

#[test]
fn copy_with_nothing_selected_says_so_and_a_point_is_nothing() {
    let mut a = shown();
    a.terminal_lico(Lico::Copy, "t1");
    assert_eq!(a.ui.flash.as_deref(), Some("Nothing selected: drag over the text first."));
    a.ui.flash = None;
    a.ui.terminal.sel = Some(Sel { anchor: (3, 2), focus: (3, 2) });
    a.terminal_lico(Lico::Copy, "t1");
    assert_eq!(a.ui.flash.as_deref(), Some("Nothing selected: drag over the text first."), "a click is a point, not a selection");
}

#[test]
fn a_selection_is_kept_by_absolute_lines_follows_the_drag_and_a_click_that_did_not_move_selects_nothing() {
    let mut a = shown();
    holding(&mut a, 74, 50, json!({}));
    grid_at(&mut a);
    a.ui.mouse = (25.0, 45.0); // column 2, row 2
    assert!(a.terminal_mouse_button(true));
    let sel = a.ui.terminal.sel.unwrap();
    assert_eq!((sel.anchor, sel.focus), ((52, 2), (52, 2)), "the line is the screen's first line (50) and the row: it stays on its text when the screen scrolls");
    a.ui.mouse = (95.0, 85.0); // column 9, row 4
    a.terminal_drag();
    assert_eq!(a.ui.terminal.sel.unwrap().focus, (54, 9), "the far end follows the pointer");
    assert!(a.terminal_mouse_button(false) && a.ui.terminal.sel.is_some() && !a.ui.terminal.dragging, "a drag stays selected when the button is let go");
    a.ui.mouse = (25.0, 45.0);
    assert!(a.terminal_mouse_button(true) && a.terminal_mouse_button(false));
    assert!(a.ui.terminal.sel.is_none(), "a click that did not move selects nothing");
    a.ui.mouse = (900.0, 45.0);
    assert!(!a.terminal_mouse_button(true), "a press outside the grid is not the terminal's");
}

#[test]
fn the_wheel_moves_the_history_three_lines_a_step_and_on_the_alternate_screen_it_is_the_arrow_keys() {
    let mut a = shown();
    holding(&mut a, 100, 76, json!({}));
    grid_at(&mut a);
    a.ui.mouse = (25.0, 45.0);
    assert!(a.terminal_wheel(&MouseScrollDelta::LineDelta(0.0, 1.0)));
    assert_eq!(tab(&a).scroll, 3, "a step of the wheel is three lines back");
    a.ui.mouse = (900.0, 45.0);
    assert!(!a.terminal_wheel(&MouseScrollDelta::LineDelta(0.0, 1.0)), "outside the grid the wheel is not the terminal's");

    let mut b = shown();
    holding(&mut b, 24, 0, json!({"alt_screen": true}));
    grid_at(&mut b);
    b.ui.mouse = (25.0, 45.0);
    assert!(b.terminal_wheel(&MouseScrollDelta::LineDelta(0.0, 1.0)));
    assert_eq!(queued(&b), [("t1".to_string(), b"\x1b[A\x1b[A\x1b[A".to_vec())], "a pager or an editor gets the arrow keys: up for a step up");
    b.ui.terminal.queue.clear();
    assert!(b.terminal_wheel(&MouseScrollDelta::LineDelta(0.0, -1.0)));
    assert_eq!(queued(&b), [("t1".to_string(), b"\x1b[B\x1b[B\x1b[B".to_vec())], "and down for a step down");
    assert_eq!(b.ui.terminal.tabs[0].scroll, 0, "the history did not move");
}

#[test]
fn a_terminal_from_the_menu_starts_in_the_folder_of_the_selected_session_else_at_home() {
    let mut a = app(1.0, false);
    assert_eq!(a.terminal_start_dir(), None, "no session: the host's home");
    a.ui.rows = vec![Row { session: "s1".into(), cwd: "/work/api".into(), ..Default::default() }];
    a.session = "s1".into();
    assert_eq!(a.terminal_start_dir().as_deref(), Some("/work/api"));
    a.ui.rows[0].cwd.clear();
    assert_eq!(a.terminal_start_dir(), None, "a session with no folder: the home");
}

#[test]
fn the_text_of_a_selection_is_asked_for_from_its_first_end_to_its_last_whichever_way_it_was_dragged() {
    let left_to_right = Sel { anchor: (52, 2), focus: (54, 9) };
    let right_to_left = Sel { anchor: (54, 9), focus: (52, 2) };
    let want = json!({"op": "terminal.text", "tab": "t1", "from": [52, 2], "to": [54, 9]});
    assert_eq!(text_request("t1", left_to_right), want);
    assert_eq!(text_request("t1", right_to_left), want, "a drag from the right to the left asks for the same text");
    let same_line = Sel { anchor: (7, 12), focus: (7, 3) };
    assert_eq!(text_request("t2", same_line), json!({"op": "terminal.text", "tab": "t2", "from": [7, 3], "to": [7, 12]}), "on one line the columns are ordered too");
}
