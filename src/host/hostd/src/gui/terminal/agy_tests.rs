//! The click on the card of a conversation of `agy` without a host (`M23-P04b`): what the window decides (show the terminal that runs it, open one in the conversation's folder, refuse an id
//! that is not one) and what it types into the new shell. The window here has no socket, so the answers of the host are put in as it would give them. The real window with a real host and
//! a program named `agy` is `tests/test_agy_chats.py`.

use serde_json::{json, Value};

use super::super::scroll_oracle::app;
use super::super::sidebar_chats::ChatEntry;
use super::agy::{valid_conversation, Plan};
use super::view::TermJob;

const ID: &str = "11111111-aaaa-4bbb-8ccc-000000000001";

fn opened(tab: &str, cwd: &str) -> Value {
    json!({"ok": 1, "tab": tab, "pid": 7, "cwd": cwd, "shell": "/bin/sh", "cols": 80, "rows": 24})
}

fn line_for(id: &str) -> String {
    format!("agy --conversation {id}\r")
}

fn card(folder: Option<&str>) -> ChatEntry {
    ChatEntry {
        provider: "agy".into(), provider_session: ID.into(), account: None, account_label: None, ssh_host: None, cwd: folder.map(str::to_string), global: false,
        title: "t".into(), updated_at: 1, open_session: None, open_state: None, running_elsewhere: false,
    }
}

#[test]
fn only_a_plain_token_is_a_conversation_a_shell_may_be_given() {
    for good in [ID, "c0001", "a", "A_b-c", &"x".repeat(64)] {
        assert!(valid_conversation(good), "{good}");
    }
    for bad in ["", "-rf", "--help", "a b", "a;b", "a&b", "a|b", "$(x)", "`x`", "a\nb", "a\rb", "a\"b", "a'b", "../x", "a/b", "a.b", "\u{d55c}\u{ae00}", &"x".repeat(65)] {
        assert!(!valid_conversation(bad), "{bad:?}");
    }
}

#[test]
fn a_conversation_opens_in_its_folder_when_that_is_a_folder_of_this_machine_else_at_the_home_with_the_line_agy_types() {
    let a = app(1.0, false);
    let here = std::env::temp_dir().to_string_lossy().to_string();
    assert_eq!(a.agy_plan(ID, Some(&here)), Plan::Open { cwd: Some(here.clone()), line: line_for(ID), conversation: ID.into() });
    for folder in [None, Some("/nonexistent-folder-for-agy"), Some("relative/dir"), Some("."), Some("")] {
        assert_eq!(a.agy_plan(ID, folder), Plan::Open { cwd: None, line: line_for(ID), conversation: ID.into() }, "{folder:?}: the home");
    }
    // the line is what a person types, with no flag added: never the one that skips the agent's permissions
    assert!(!line_for(ID).contains("--dangerously") && !line_for(ID).contains("skip") && line_for(ID) == format!("agy --conversation {ID}\r"));
}

#[test]
fn an_id_that_is_not_a_token_starts_nothing() {
    let a = app(1.0, false);
    for bad in ["", "-rf", "a b", "a;b", "$(x)", "a\nb"] {
        assert!(matches!(a.agy_plan(bad, None), Plan::Refuse(_)), "{bad:?}");
    }
}

#[test]
fn a_terminal_opened_for_the_conversation_that_still_shows_agy_is_shown_and_one_that_does_not_is_not() {
    let mut a = app(1.0, false);
    a.on_terminal_async(TermJob::OpenTyping { line: line_for(ID), conversation: Some(ID.into()) }, opened("t1", "/work/one"));
    a.on_terminal_async(TermJob::Open, opened("t2", "/work/two"));
    assert_eq!(a.ui.terminal.active.as_deref(), Some("t2"), "t2 is in front");
    assert!(matches!(a.agy_plan(ID, None), Plan::Open { .. }), "t1 has not shown agy yet: the picture is the host's word, and none came");
    a.ui.terminal.tabs[0].screen.agent = Some("agy");
    assert_eq!(a.agy_plan(ID, None), Plan::Show("t1".into()));
    a.ui.terminal.tabs[0].screen.agent = Some("claude");
    assert!(matches!(a.agy_plan(ID, None), Plan::Open { .. }), "another agent runs there now");
    a.ui.terminal.tabs[0].screen.agent = Some("agy");
    a.ui.terminal.tabs[0].conversation = Some("another-one".into());
    assert!(matches!(a.agy_plan(ID, None), Plan::Open { .. }), "that terminal is for another conversation");
    a.ui.terminal.tabs[1].conversation = Some(ID.into());
    a.ui.terminal.tabs[1].screen.agent = None;
    assert!(matches!(a.agy_plan(ID, None), Plan::Open { .. }), "a tab of the conversation whose program has ended is no tab that runs it");
}

#[test]
fn the_open_that_types_shows_the_new_tab_keeps_its_conversation_and_queues_the_line_for_its_shell() {
    let mut a = app(1.0, false);
    a.on_terminal_async(TermJob::OpenTyping { line: line_for(ID), conversation: Some(ID.into()) }, opened("t1", "/work/one"));
    assert_eq!(a.ui.terminal.active.as_deref(), Some("t1"));
    assert_eq!(a.ui.terminal.tabs[0].conversation.as_deref(), Some(ID));
    assert_eq!(a.ui.terminal.queue.iter().map(|(t, b)| (t.as_str(), b.clone())).collect::<Vec<_>>(), vec![("t1", line_for(ID).into_bytes())], "typed into the new tab and no other");
    // a plain open types nothing and keeps no conversation
    let mut b = app(1.0, false);
    b.on_terminal_async(TermJob::Open, opened("t1", "/work/one"));
    assert!(b.ui.terminal.queue.is_empty() && b.ui.terminal.tabs[0].conversation.is_none());
}

#[test]
fn a_refused_open_types_nothing_and_says_why() {
    let mut a = app(1.0, false);
    a.on_terminal_async(TermJob::OpenTyping { line: line_for(ID), conversation: Some(ID.into()) }, json!({"ok": 0, "error": "tab_limit"}));
    assert!(a.ui.terminal.tabs.is_empty() && a.ui.terminal.queue.is_empty());
    assert_eq!(a.ui.flash.as_deref(), Some("eight terminals are open: close one first"));
}

#[test]
fn a_click_on_the_card_shows_the_terminal_that_runs_it_and_says_so() {
    let mut a = app(1.0, false);
    a.on_terminal_async(TermJob::OpenTyping { line: line_for(ID), conversation: Some(ID.into()) }, opened("t1", "/work/one"));
    a.on_terminal_async(TermJob::Open, opened("t2", "/work/two"));
    a.ui.terminal.tabs[0].screen.agent = Some("agy");
    a.ui.sidebar.chats.items = vec![card(Some("/work/one"))];
    a.chats_activate_entry(0);
    assert_eq!(a.ui.terminal.active.as_deref(), Some("t1"), "the terminal of the conversation is in front");
    assert_eq!(a.ui.terminal.tabs.len(), 2, "no third one");
    assert_eq!(a.ui.flash.as_deref(), Some("Switched to the terminal where it runs."));
}

#[test]
fn a_click_on_a_card_with_a_bad_id_starts_nothing_and_says_so() {
    let mut a = app(1.0, false);
    let mut bad = card(None);
    bad.provider_session = "a b; rm -rf".into();
    a.ui.sidebar.chats.items = vec![bad];
    a.chats_activate_entry(0);
    assert!(a.ui.terminal.tabs.is_empty() && a.ui.terminal.queue.is_empty());
    assert_eq!(a.ui.flash.as_deref(), Some("That conversation cannot be opened here."));
}
