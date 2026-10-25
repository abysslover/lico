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
