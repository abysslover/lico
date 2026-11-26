//! The status at the bottom and the border of the conversation (`M24-P11f`).

use super::*;
use crate::gui::scroll_oracle::app;
use crate::gui::ui::{display_cols, Row};

const W: usize = 1400;
const H: usize = 900;

fn running(provider: &str, account: &str, activity: &str, mode: &str) -> Row {
    Row { session: "session-s0".into(), provider: provider.into(), account: account.into(), state: "running".into(), activity: activity.into(), mode: mode.into(), ..Default::default() }
}

#[test]
fn the_state_part_says_provider_account_state_mode_and_issue_and_the_border_is_six_units() {
    assert_eq!(state_part(&running("claude", "work", "idle", "auto"), "Work", 1_000), "claude | Work | idle | mode auto");
    assert_eq!(state_part(&running("claude", "work", "idle", "default"), "Work", 1_000), "claude | Work | idle | mode asks");
    assert_eq!(state_part(&running("claude", "work", "idle", ""), "Work", 1_000), "claude | Work | idle", "a mode the host did not report is not said");
    let mut r = running("codex", "home", "working", "default");
    r.issue = "login needed".into();
    r.busy_since = 963;
    assert_eq!(state_part(&r, "Home", 1_000), "codex | Home | working 37 s | mode asks | account issue: login needed", "the clock of an open turn, the issue last");
    assert_eq!(state_part(&running("local", "", "idle", "auto"), "no account", 1_000), "Local | idle", "a local session has no account and no provider mode");
    assert_eq!(state_part(&running("agy", "", "idle", "default"), "no account", 1_000), "agy | idle | mode asks", "an agy session has no account of Lico's: the program signs in by itself");
    assert_eq!(state_part(&running("agy", "g1", "idle", "default"), "Google", 1_000), "agy | Google | idle | mode asks", "an agy session on an account of agy says it, like the others");
    assert_eq!((edge(true, 1.0), edge(true, 2.0), edge(false, 2.0)), (6.0, 12.0, 0.0), "a rule of 2 and a margin of 4, none without the bars");
    assert_eq!(rule_rows(100.0, 1.5), [100.0, 103.0]);
}

#[test]
fn a_strip_too_narrow_for_the_whole_state_takes_a_shorter_form_so_that_the_folder_keeps_its_room() {
    let mut r = running("claude", "work", "idle", "auto");
    r.issue = "login".into();
    let forms = state_forms(&r, "Work", 1_000);
    assert_eq!(forms, ["claude | Work | idle | mode auto | account issue: login", "claude | Work | idle | account issue: login", "idle | account issue: login"], "the issue is never left out");
    assert_eq!(state_forms(&running("local", "", "idle", "auto"), "", 1_000), ["Local | idle", "idle"]);
    assert_eq!(state_forms(&running("agy", "", "idle", "auto"), "no account", 1_000), ["agy | idle | mode auto", "agy | idle", "idle"]);
    assert_eq!(state_forms(&running("codex", "home", "idle", ""), "Home", 1_000), ["codex | Home | idle", "idle"], "forms that come out the same are one");
    let cols = |f: &str| f.chars().count() + SEP_COLS;
    // the whole form takes `cols(forms[0])` columns, the next one fewer: each edge in columns, with and without notices after it
    assert_eq!(pick_form(&forms, 0, cols(&forms[0])), forms[0]);
    assert_eq!(pick_form(&forms, 0, cols(&forms[0]) - 1), forms[1]);
    assert_eq!(pick_form(&forms, 0, cols(&forms[1]) - 1), forms[2]);
    assert_eq!(pick_form(&forms, 0, 0), forms[2], "the shortest when none fits");
    assert_eq!(pick_form(&forms, 30, cols(&forms[1]) + 30), forms[1], "the notices take their columns first");
    assert_eq!(pick_form(&forms, 30, cols(&forms[1]) + 29), forms[2]);
    assert_eq!(pick_form(&[], 0, 100), "", "no session, no state");
}

#[test]
fn only_a_conversation_of_a_window_with_bars_and_a_workbench_has_no_top_line() {
    let mut a = app(1.0, false);
    assert!(a.status_at_bottom(false), "a conversation");
    assert!(!a.status_at_bottom(true), "a panel (the start page is one) keeps its title");
    a.ui.menu = false;
    assert!(!a.status_at_bottom(false), "a window without the bars says its state at the top, as before");
    a.ui.menu = true;
    a.ui.workbench = false;
    assert!(!a.status_at_bottom(false));
    // a browser tab and the Call Room have the top line for their own (the terminal tab: `terminal/view_tests.rs`)
    let mut browser = app(1.0, false);
    (browser.session, browser.ui.browser.prompt) = (String::new(), Some(false));
    assert!(browser.browser_active() && !browser.status_at_bottom(false), "a browser tab");
    let mut room = app(1.0, false);
    room.ui.calls.active_call = Some("c1".into());
    assert!(!room.status_at_bottom(false), "the Call Room");
}

/// The first line of the conversation of a session, drawn once at `scale`, with the window's size.
fn conversation(scale: f32) -> (crate::gui::App, Vec<u32>) {
    let mut a = app(scale, false);
    a.ui.rows = vec![running("claude", "work", "idle", "auto")];
    for (seq, text) in [(1, "alpha line"), (2, "beta line"), (3, "gamma line")] {
        a.push_line(seq, text.to_string());
    }
    let mut buf = vec![0u32; W * H];
    assert!(a.compose(&mut buf, W, H, 0).is_some(), "nothing was drawn");
    (a, buf)
}

#[test]
fn a_conversation_starts_under_the_border_with_no_line_of_status_and_the_start_page_keeps_its_title() {
    for scale in [1.0f32, 1.5, 2.0] {
        let (a, buf) = conversation(scale);
        let at = format!("scale {scale}");
        let line_h = a.line_h();
        let bars = super::super::toolbar::bar_heights(true, scale, line_h).1;
        let tabs = super::super::tabs::height_px(a.tabs_shown(), scale, line_h);
        let chat_top = bars + tabs + edge(true, scale);
        let first = a.ui.hit_lines.first().copied().unwrap_or_else(|| panic!("a line was drawn ({at})"));
        assert!((first.0 - chat_top).abs() < 0.01, "the first line of the conversation is the first row under the border: {} against {chat_top} ({at})", first.0);
        let buffer = &a.view.as_ref().expect("a view").buffer;
        assert!(buffer.lines[0].text().contains("alpha line"), "line 0 of the text is the conversation's, not a status ({at}): {:?}", buffer.lines[0].text());
        assert!(a.ui.header.starts_with("claude | work | model not reported | idle | mode: auto"), "the dump's `header` keeps the text of the status ({at}): {}", a.ui.header);
        // the rule: two rows in its tone from the list's edge to the window's, the margin under it in the ground, the bar above it not in its tone
        let list_w = a.effective_list_w(scale);
        let (rule0, rule1) = (bars + tabs, bars + tabs + 2.0 * scale);
        for x in [list_w + 20, W - 20] {
            assert_eq!(buf[(rule0 as usize) * W + x], 0xFF3A_4B63, "the rule's first row at column {x} ({at})");
            assert_eq!(buf[(rule1 as usize - 1) * W + x], 0xFF3A_4B63, "the rule's last row ({at})");
            assert_eq!(buf[(rule1 as usize) * W + x], crate::gui::theme::tone(crate::gui::BACKGROUND), "the margin under the rule is the chat's ground ({at})");
            assert_ne!(buf[(rule0 as usize - 1) * W + x], 0xFF3A_4B63, "the tab bar above the rule is not in its tone ({at})");
        }
        assert_ne!(buf[(rule0 as usize) * W + 5], 0xFF3A_4B63, "the list on the left has no rule of the chat's ({at})");
        // a window with no session is the start page: a panel, with its title as the first line
        let mut start = app(scale, false);
        start.session = String::new();
        let mut b2 = vec![0u32; W * H];
        assert!(start.compose(&mut b2, W, H, 0).is_some());
        assert_eq!(start.view.as_ref().expect("a view").buffer.lines[0].text(), "select a session", "the start page keeps the title of its panel ({at})");
    }
    // a window without the bars has no border: the first line is at the top, the header line, and no rule is drawn over it
    let mut bare = app(1.0, false);
    bare.ui.menu = false;
    bare.ui.rows = vec![running("claude", "work", "idle", "auto")];
    let mut buf = vec![0u32; W * H];
    assert!(bare.compose(&mut buf, W, H, 0).is_some());
    let top = bare.ui.composed.as_ref().expect("a frame").top as usize;
    assert_eq!(top, 0, "nothing stands over the text of a window without the bars");
    assert!([0, 1].iter().all(|row| buf[(top + row) * W + W - 20] != 0xFF3A_4B63), "no rule without the bars");
}

#[test]
fn the_strip_says_the_state_first_then_the_notices_and_a_session_the_host_does_not_list_says_so() {
    let (mut a, _) = conversation(1.0);
    let g = &a.ui.telemetry_geom;
    assert!(g.shown, "the strip of a conversation is drawn");
    assert_eq!((g.status_text.as_str(), g.notice_text.as_str()), ("claude | work | idle | mode auto", ""));
    a.ui.host_notice = Some("The host was replaced; reconnecting...".into());
    a.ui.flash = Some("Copied 3 lines".into());
    let mut buf = vec![0u32; W * H];
    assert!(a.compose(&mut buf, W, H, 1).is_some());
    assert_eq!(a.ui.telemetry_geom.notice_text, "The host was replaced; reconnecting... | Copied 3 lines", "the host's notice and then the message, as the top line had them");
    let shown = a.telemetry_json()["status_text"].as_str().unwrap_or_default().to_string();
    assert!(shown.ends_with("idle") && shown.len() < "claude | work | idle | mode auto".len(), "the notices take their columns first, the state takes a shorter form: {shown:?}");
    a.ui.rows.clear();
    assert!(a.compose(&mut buf, W, H, 1).is_some());
    let g = &a.ui.telemetry_geom;
    assert!(g.shown && g.status_text == "session-s0 | not listed", "a session the host does not list: {:?}", g.status_text);
    assert!(g.cwd_rect.is_none() && g.cwd_text.is_empty() && g.model_text.is_empty(), "it says that and nothing else: no folder, no model");
    a.session = String::new();
    assert!(a.status_forms().is_empty(), "no session, no state");
}

/// The pixels in the strip's band that are exactly `ink`.
fn ink_pixels(a: &crate::gui::App, buf: &[u32], ink: u32) -> usize {
    let r = a.ui.telemetry_geom.rect.expect("the strip's rectangle");
    (r[1] as usize..r[3] as usize).flat_map(|y| (0..W).map(move |x| (y, x))).filter(|&(y, x)| buf[y * W + x] == ink).count()
}

#[test]
fn the_state_is_in_the_light_ink_and_the_notices_after_it_in_the_warm_one() {
    let (mut a, buf) = conversation(1.0);
    assert!(ink_pixels(&a, &buf, 0xFFD6_DEE8) > 0, "the state is in the light ink");
    assert_eq!(ink_pixels(&a, &buf, 0xFFFF_C86B), 0, "no notice, no warm ink");
    a.ui.flash = Some("Copied 3 lines".into());
    let mut buf = vec![0u32; W * H];
    assert!(a.compose(&mut buf, W, H, 1).is_some());
    assert!(ink_pixels(&a, &buf, 0xFFFF_C86B) > 0, "a notice is in the warm ink");
}

#[test]
fn the_strip_gives_each_part_its_columns_and_the_state_takes_the_longest_form_that_is_left() {
    assert_eq!(display_cols(SEP), SEP_COLS, "the separator of the strip is as wide as the number says");
    let forms = ["claude | Work | idle | mode auto".to_string(), "claude | Work | idle".to_string(), "idle".to_string()];
    assert_eq!(forms.iter().map(|f| display_cols(f)).collect::<Vec<_>>(), [32, 20, 4]);
    let listed = Some([10, 8, 6]); // the model, the tokens and the quota: 24 columns, 3 separators of 5 and 20 for the folder keep 59 of the strip
    let at = |px: f32, notices: &[&str], parts: Option<[usize; 3]>| {
        let notices: Vec<String> = notices.iter().map(|n| n.to_string()).collect();
        strip_form(&forms, &notices, px, 1.0, 10.0, parts).to_string()
    };
    // the strip keeps 12 pixels at its ends: a width of 12 + 10 * columns holds `columns`; the whole form takes 32 + a separator after the 59: 96 columns
    assert_eq!((at(972.0, &[], listed), at(971.0, &[], listed)), (forms[0].clone(), forms[1].clone()));
    assert_eq!((at(852.0, &[], listed), at(851.0, &[], listed)), (forms[1].clone(), forms[2].clone()), "the form without the mode: 20 + 5 after the 59");
    // a notice of 3 columns and its separator take 8 before the state does
    assert_eq!((at(1052.0, &["abc"], listed), at(1051.0, &["abc"], listed)), (forms[0].clone(), forms[1].clone()));
    // a session the host does not list has no parts and no folder
    assert_eq!((at(382.0, &[], None), at(381.0, &[], None)), (forms[0].clone(), forms[1].clone()));
    assert_eq!((at(262.0, &[], None), at(261.0, &[], None)), (forms[1].clone(), forms[2].clone()));
    // the margins and the columns follow the scale: at 2 the strip keeps 24 pixels and a column is 20
    let scaled = |px: f32| strip_form(&forms, &[], px, 2.0, 20.0, listed).to_string();
    assert_eq!((scaled(1944.0), scaled(1943.0)), (forms[0].clone(), forms[1].clone()));
}

#[test]
fn the_transcript_fills_the_rows_down_to_the_prompt_and_an_open_search_bar_is_the_first_line() {
    let mut a = app(1.0, false);
    a.ui.rows = vec![running("claude", "work", "idle", "auto")];
    for seq in 1..=200 {
        a.push_line(seq, format!("line number {seq}"));
    }
    let mut buf = vec![0u32; W * H];
    assert!(a.compose(&mut buf, W, H, 0).is_some());
    let lh = a.line_h();
    let (last, caret) = (a.ui.hit_lines.last().copied().expect("lines were drawn"), a.ed.caret.expect("the input box has a caret"));
    let gap = caret[1] - last.1;
    assert!((0.0..lh).contains(&gap), "no row is left empty between the last line and the prompt (the row of the top line is the conversation's now): {gap} against {lh}");
    // the rows: 44 under the border at 18 pixels, less the hint line, the path bar, the strip, the prompt and one more that is left free: 39 (the line of the header
    // held one of them before this phase: 38)
    assert_eq!(a.ui.hit_lines.len(), 39, "the rows of a conversation");
    // the search bar stands in front of the transcript as line 0, with its band, and the lines follow it
    a.ui.search.open = true;
    assert!(a.compose(&mut buf, W, H, 1).is_some());
    assert_eq!(a.ui.hit_lines.len(), 38, "the search bar takes a row of them");
    let buffer = &a.view.as_ref().expect("a view").buffer;
    assert!(buffer.lines[0].text().trim_start().starts_with("Find in conversation"), "the first line is the search bar and nothing of the status is in front of it: {:?}", buffer.lines[0].text());
    let first = a.ui.hit_lines[0];
    let chat_top = first.0 - lh;
    assert!(buffer.lines.len() > 2, "the transcript follows");
    assert_eq!(buf[(chat_top as usize + 2) * W + a.effective_list_w(1.0) + 20], 0xFF1E_2836, "the band of the search bar is on its line, the first one");
}

#[test]
fn a_view_with_a_line_of_its_own_at_the_top_keeps_its_rows() {
    // a window without the bars has the header line at the top: 50 rows at 18 pixels, less the header, the hint line, the prompt and the one that is left free: 45,
    // as before this phase
    let mut a = app(1.0, false);
    a.ui.menu = false;
    a.ui.rows = vec![running("claude", "work", "idle", "auto")];
    for seq in 1..=200 {
        a.push_line(seq, format!("line number {seq}"));
    }
    let mut buf = vec![0u32; W * H];
    assert!(a.compose(&mut buf, W, H, 0).is_some());
    assert_eq!(a.ui.hit_lines.len(), 45, "the rows of a conversation with its header line");
    assert!(a.view.as_ref().expect("a view").buffer.lines[0].text().starts_with("claude | work | "), "the header is the first line");
}

#[test]
fn the_strip_under_a_call_room_does_not_say_the_state_that_the_room_keeps_in_its_own_line() {
    let mut a = app(1.0, false);
    a.ui.rows = vec![running("claude", "work", "idle", "auto")];
    a.ui.calls.active_call = Some("c1".into());
    a.ui.host_notice = Some("The host was replaced; reconnecting...".into());
    let mut buf = vec![0u32; W * H];
    assert!(a.compose(&mut buf, W, H, 0).is_some());
    let g = &a.ui.telemetry_geom;
    assert!(g.shown, "the strip stands under a Call Room as well");
    assert_eq!((g.status_text.as_str(), g.notice_text.as_str()), ("", ""), "the line of the Call Room has the state and the notices, not the strip");
}
