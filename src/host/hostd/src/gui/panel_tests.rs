//! What a panel takes from the input line it gives back, and a half-typed sign-in code is nowhere else: not in a draft, not in the dump.

use serde_json::json;

use super::super::menu::Cmd;
use super::super::scroll_oracle::app;
use super::*;

const CODE: &str = "CANARYCODE9";

/// A session is shown with `my draft` typed in its box; the sign-in panel is open and a code is half typed.
fn typing_a_code() -> App {
    let mut app = app(1.0, false);
    app.ed.put("my draft");
    app.panel_open(Step::Login);
    app.ui.panel.as_mut().unwrap().login = json!({"needs_code": true});
    assert!(app.code_entry() && app.ed.text() == "" && app.ed.put(CODE));
    app
}

fn nowhere(app: &App) {
    for shown in [app.editor_json().to_string(), app.panel_json().to_string(), app.shown_input()] {
        assert!(!shown.contains(CODE), "{shown}");
    }
}

#[test]
fn leaving_the_sign_in_for_another_panel_gives_the_draft_back_and_drops_the_code() {
    let mut app = typing_a_code();
    app.run_cmd(Cmd::NewSession); // File > New session while the code is being typed
    assert!(app.ui.panel.is_some() && app.ed.text() == "");
    nowhere(&app);
    app.panel_close();
    assert_eq!(app.ed.text(), "my draft");
    nowhere(&app);
}

#[test]
fn a_panel_opened_from_a_panel_does_not_lose_the_draft_either() {
    let mut app = app(1.0, false);
    app.ed.put("my draft");
    app.panel_open(Step::Accounts);
    app.panel_open(Step::Actions); // as when an account is chosen in the list, and again when backing out of it
    app.panel_open(Step::Accounts);
    app.panel_close();
    assert_eq!(app.ed.text(), "my draft");
}

#[test]
fn leaving_the_session_view_with_the_panel_open_parks_the_draft_and_not_the_code() {
    let mut app = typing_a_code();
    app.hide(); // the window's own `show` and `hide` commands, and a view that is shown again after the host was replaced, come here
    assert!(app.ui.panel.is_none() && app.session.is_empty() && app.ed.text() == "");
    nowhere(&app);
    app.ed.resume("session-s0");
    assert_eq!(app.ed.text(), "my draft");
    nowhere(&app);
}

#[test]
fn the_empty_account_screens_say_where_the_first_account_is_made() {
    // M22-P04b: the Claude and Codex icons of the new-session wizard are grey until an account exists, so Ctrl+N is not where the first one is made
    let mut app = app(1.0, false);
    app.panel_open(Step::Accounts);
    let (_, body) = app.panel_view().expect("the accounts screen");
    let hint = body.iter().find(|l| l.starts_with("No account yet")).expect("the empty line");
    assert!(hint.contains("press A") && hint.contains("Local LLM does not") && !hint.contains("Ctrl+N"), "{hint}");
    app.panel_open(Step::Rebind);
    app.ui.panel.as_mut().unwrap().provider = "claude".into();
    let (_, body) = app.panel_view().expect("the rebind screen");
    let hint = body.iter().find(|l| l.starts_with("No claude account")).expect("the empty line");
    assert!(hint.contains("person icon") && hint.contains("Ctrl+A") && !hint.contains("Ctrl+N"), "{hint}");
}
