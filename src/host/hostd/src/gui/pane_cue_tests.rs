//! The person's side of the agents' control of the panes (`M24-P05b`): the cue in a pane's header, the switch in the menu `More`, and the words after the stop key. The window here has no host: the
//! host's `status` is put in as the host would give it. The real window with a real host is `tests/test_pane_switch_window.py`.

use serde_json::{json, Value};
use std::sync::Arc;

use super::browser_view::BTab;
use super::menu::{group_of, place_of, Cmd, Place};
use super::pane_cue::{stop_note, PaneCue, PanesNow};
use super::scroll_oracle::app;
use super::terminal::TermJob;
use super::App;

fn status(settings: Value, cues: Value) -> String {
    json!({"ev": "status", "sessions": [], "settings": settings, "pane_cues": cues}).to_string()
}

fn cue(tab: &str, by: &str, did: &str) -> Value {
    json!({"tab": tab, "by": by, "did": did, "ms": 100})
}

/// A window with the terminal `t1` shown.
fn with_terminal() -> App {
    let mut a = app(1.0, false);
    a.on_terminal_async(TermJob::Open, json!({"ok": 1, "tab": "t1", "pid": 7, "cwd": "/work/one", "shell": "/bin/sh", "cols": 80, "rows": 24}));
    a
}

/// A window with the browser tab `b1` shown.
fn with_browser() -> App {
    let mut a = app(1.0, false);
    a.session.clear();
    let row = json!({"tab": "b1", "title": "A page", "phase": "ready", "url": "https://a.test/"});
    a.ui.browser.tabs.push(BTab { id: "b1".into(), local: false, row, seq: 0, frame_seq: 0, png: Vec::new(), image: None, decoded_seq: 0, watching: false, rearm: false, sent: None, inflight: false, inbox: Arc::default(), painted: 0, put_url: String::new(), waiting: false });
    a.ui.browser.active = Some("b1".into());
    a
}

#[test]
fn a_cue_is_who_did_what_and_the_one_key_that_stops_it_and_an_entry_without_a_tab_or_words_is_none() {
    let c = PaneCue::of_json(&cue("t1", "Claude 1", "typed")).unwrap();
    assert_eq!(c.text(), "Claude 1 typed (Ctrl+Alt+Esc stops agents)");
    for bad in [json!({"by": "x", "did": "typed"}), json!({"tab": "t1", "did": "typed"}), json!({"tab": "t1", "by": "x"}), json!({"tab": "", "by": "x", "did": "typed"}), json!({"tab": "t1", "by": "  ", "did": "typed"}), json!({"tab": 1, "by": "x", "did": "typed"}), json!("t1")] {
        assert_eq!(PaneCue::of_json(&bad), None, "{bad}");
    }
    let long = PaneCue::of_json(&cue("t1", &"n".repeat(80), "typed")).unwrap();
    assert_eq!(long.by.chars().count(), 24, "what the host sends is cut again here: the header is the person's");
    let dirty = PaneCue::of_json(&cue("t1", "a\u{1b}[2Jb\n", "ty\tped")).unwrap();
    assert_eq!((dirty.by.as_str(), dirty.did.as_str()), ("a[2Jb", "typed"), "no control character reaches the header");
}

#[test]
fn the_cues_of_the_last_status_are_kept_and_a_status_without_them_clears_them() {
    let mut a = app(1.0, false);
    assert!(a.ui.pane_cues.is_empty());
    a.ui_status(&status(json!({}), json!([cue("t1", "Claude 1", "typed"), cue("b2", "Codex", "clicked"), {"by": "no tab"}])));
    assert_eq!(a.ui.pane_cues.iter().map(|c| (c.tab.as_str(), c.by.as_str(), c.did.as_str())).collect::<Vec<_>>(), [("t1", "Claude 1", "typed"), ("b2", "Codex", "clicked")], "an entry that is none is left out");
    a.ui_status(&status(json!({}), json!([])));
    assert!(a.ui.pane_cues.is_empty(), "the host tells what is current: an empty list is no cue");
    a.ui_status(&status(json!({}), json!([cue("t1", "Claude 1", "typed")])));
    a.ui_status(&json!({"ev": "status", "sessions": [], "settings": {}}).to_string());
    assert!(a.ui.pane_cues.is_empty(), "a status with no such field (an older host) is no cue either");
}

#[test]
fn a_new_cue_is_a_reason_to_paint_and_the_same_one_again_is_not() {
    let mut a = app(1.0, false);
    a.dirty = false;
    a.ui_status(&status(json!({}), json!([cue("t1", "Claude 1", "typed")])));
    assert!(a.dirty, "a new cue is to be seen");
    a.dirty = false;
    a.ui_status(&status(json!({}), json!([{"tab": "t1", "by": "Claude 1", "did": "typed", "ms": 900}])));
    assert!(!a.dirty, "the age moved and nothing that is shown did");
}

#[test]
fn the_header_of_the_shown_terminal_carries_its_own_cue_and_not_another_tabs() {
    let mut a = with_terminal();
    let plain = a.terminal_header().unwrap();
    assert!(!plain.contains("stops agents"), "{plain}");
    a.ui_status(&status(json!({}), json!([cue("t1", "Claude 1", "typed")])));
    let header = a.terminal_header().unwrap();
    assert_eq!(header, format!("{plain} | Claude 1 typed (Ctrl+Alt+Esc stops agents)"), "the header as it was, then the cue");
    assert!(a.header_text().contains("Claude 1 typed"), "the line the window shows is made of it");
    a.ui_status(&status(json!({}), json!([cue("t2", "Claude 1", "typed"), cue("b1", "Codex", "clicked")])));
    assert_eq!(a.terminal_header().unwrap(), plain, "an agent in another tab is not an agent in this one");
    a.ui_status(&status(json!({}), json!([cue("t1", "Codex", "pressed a key")])));
    assert!(a.terminal_header().unwrap().ends_with("Codex pressed a key (Ctrl+Alt+Esc stops agents)"));
    a.ui_status(&status(json!({}), json!([])));
    assert_eq!(a.terminal_header().unwrap(), plain, "the cue goes with the next status that does not carry it");
}

#[test]
fn the_header_of_the_shown_browser_tab_carries_its_own_cue_too() {
    let mut a = with_browser();
    let plain = a.browser_header().unwrap();
    assert!(plain.starts_with("browser | A page | ready") && !plain.contains("stops agents"), "{plain}");
    a.ui_status(&status(json!({}), json!([cue("b1", "Claude 1", "clicked")])));
    assert_eq!(a.browser_header().unwrap(), format!("{plain} | Claude 1 clicked (Ctrl+Alt+Esc stops agents)"));
    a.ui_status(&status(json!({}), json!([cue("t1", "Claude 1", "clicked")])));
    assert_eq!(a.browser_header().unwrap(), plain);
}

#[test]
fn the_window_knows_the_switch_from_the_hosts_settings_and_it_is_on_unless_the_host_says_false() {
    let mut a = app(1.0, false);
    assert!(!a.ui.agent_panes_off, "on before the host has said anything: the owner's default");
    a.ui_status(&status(json!({"agent_panes": false}), json!([])));
    assert!(a.ui.agent_panes_off);
    a.ui_status(&status(json!({"agent_panes": true}), json!([])));
    assert!(!a.ui.agent_panes_off);
    a.ui_status(&status(json!({"agent_panes": false}), json!([])));
    a.ui_status(&status(json!({}), json!([])));
    assert!(!a.ui.agent_panes_off, "a host that does not say is an older one: the tool it has is on");
}

#[test]
fn the_entry_in_the_menu_more_says_the_state_and_sits_with_the_other_settings() {
    let mut a = app(1.0, false);
    assert_eq!((place_of(Cmd::TogglePanes), group_of(Cmd::TogglePanes)), (Place::Hide, Some("Settings")), "More > Settings, not on a menu that is always in sight");
    let label = |a: &App| a.menu_entries(4).into_iter().find(|e| e.cmd == Cmd::TogglePanes).map(|e| e.label);
    assert_eq!(label(&a).as_deref(), Some("Agent pane control: on"));
    a.ui.agent_panes_off = true;
    assert_eq!(label(&a).as_deref(), Some("Agent pane control: off"));
    let settings: Vec<String> = a.menu_entries(4).into_iter().filter(|e| group_of(e.cmd) == Some("Settings")).map(|e| e.label.split(':').next().unwrap().to_string()).collect();
    assert_eq!(settings, ["Agent clipboard write", "Agent pane control", "Desktop notifications", "Use existing Claude Code login", "Use existing Codex login"].map(String::from).to_vec(), "{settings:?}");
}

#[test]
fn a_switch_the_host_does_not_take_says_so_and_changes_nothing() {
    let mut a = app(1.0, false);
    a.toggle_agent_panes();
    assert!(!a.ui.agent_panes_off, "no host answered: what the window shows is what it was");
    assert_eq!(a.ui.flash.as_deref(), Some("Could not change the agents' control of your panes."));
    assert_eq!(a.stop_agent_panes(), PanesNow::Failed, "the stop that no host took says so");
    assert!(a.ui.pane_cues.is_empty(), "the cues are dropped in any case: the person asked for a stop");
}

#[test]
fn the_stop_key_clears_the_cues_before_the_host_has_answered() {
    let mut a = with_terminal();
    a.ui_status(&status(json!({}), json!([cue("t1", "Claude 1", "typed")])));
    assert!(a.terminal_header().unwrap().contains("typed"));
    a.stop_real_screen();
    assert!(!a.terminal_header().unwrap().contains("typed"), "no cue is left after a stop");
    assert!(a.ui.flash.as_deref().is_some_and(|f| f.contains("Could not")), "with no host the notice says that: {:?}", a.ui.flash);
}

#[test]
fn the_notice_after_the_stop_key_says_what_was_stopped_and_how_to_turn_the_panes_on_again() {
    let on_again = "More > Settings turns it on again";
    let both = stop_note(Some(2), PanesNow::Stopped);
    assert!(both.starts_with("Real-screen control stopped.") && both.contains("Agents can no longer type or click in your panes") && both.contains(on_again), "{both}");
    assert_eq!(stop_note(Some(1), PanesNow::WasOff), "Real-screen control stopped.");
    let panes_only = stop_note(Some(0), PanesNow::Stopped);
    assert!(panes_only.starts_with("Agents can no longer") && !panes_only.contains("Real-screen"), "{panes_only}");
    assert_eq!(stop_note(Some(0), PanesNow::WasOff), "Nothing to stop: real-screen control and the agents' control of your panes are off.");
    assert_eq!(stop_note(None, PanesNow::WasOff), "Could not stop real-screen control.");
    let failed = stop_note(None, PanesNow::Failed);
    assert!(failed.contains("Could not stop real-screen control.") && failed.contains("Could not switch the agents' control of your panes off."), "{failed}");
    assert!(stop_note(Some(0), PanesNow::Failed).starts_with("Could not switch"), "a stop that failed is not said to have worked");
}
