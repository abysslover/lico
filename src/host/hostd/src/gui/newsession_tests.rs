//! The new-session form without a host: what each row holds, how the input line follows the focus, why Start waits, and that a sign-in
//! code typed on the way is nowhere but on the input line.

use serde_json::json;

use super::super::super::scroll_oracle::app;
use super::*;

fn account(id: &str, provider: &str, label: &str, default: bool) -> (String, String, String, bool) {
    (id.into(), provider.into(), label.into(), default)
}

/// The form is open in a window whose host does not answer (the accounts and recent folders are put in by hand).
fn opened() -> App {
    let mut app = app(1.0, false);
    app.form_open();
    let p = app.ui.panel.as_mut().unwrap();
    p.accounts = vec![account("a1", "claude", "Work", true), account("a2", "claude", "Home", false), account("a3", "codex", "Lab", true)];
    p.provider = "claude".into();
    p.account = "a1".into();
    p.form.as_mut().unwrap().recent = vec!["/work/one".into(), "/work/two".into()];
    app
}

fn form(app: &App) -> serde_json::Value {
    app.form_json()
}

#[test]
fn the_form_lists_its_rows_in_order_with_the_input_line_on_the_folder() {
    let app = opened();
    let rows: Vec<String> = form(&app)["rows"].as_array().unwrap().iter().map(|r| r.as_str().unwrap().to_string()).collect();
    assert_eq!(rows, ["Provider", "Account", "Folder", "Recent(0)", "Recent(1)", "Name", "Model", "Effort", "Permission", "SignIn", "Start"]);
    assert_eq!(form(&app)["focus"], "Folder");
    assert!(app.text_mode(), "the folder takes text");
    let (_, body) = app.panel_view().unwrap();
    assert!(body[2].starts_with("> Folder") && body[3].contains("/work/one") && body[0].starts_with("  Provider"), "{body:?}");
    assert!(body.iter().any(|l| l.contains("settings (hooks, environment) are not used; its CLAUDE.md is")), "{body:?}");
}

#[test]
fn what_is_typed_stays_with_its_row_when_the_focus_moves() {
    let mut app = opened();
    app.ed.put("/tmp/project");
    app.form_move(3); // the two recent folders, then the name
    assert_eq!(form(&app)["focus"], "Name");
    assert_eq!(app.ed.text(), "", "a row of its own starts empty");
    app.ed.put("api server");
    app.form_move(-3);
    assert_eq!((form(&app)["folder"].as_str().unwrap(), form(&app)["name"].as_str().unwrap()), ("/tmp/project", "api server"));
    assert_eq!(app.ed.text(), "/tmp/project");
    app.form_move(-1); // the account row: no text row has the line
    assert!(!app.text_mode() && app.ed.text() == "");
    assert_eq!(form(&app)["folder"], "/tmp/project");
}

#[test]
fn a_recent_folder_is_taken_into_the_folder_row_and_the_focus_returns_to_it() {
    let mut app = opened();
    app.ed.put("/typed/first");
    app.form_move(2); // the second recent folder
    assert_eq!(form(&app)["focus"], "Recent(1)");
    app.form_pick();
    assert_eq!((form(&app)["focus"].as_str().unwrap(), form(&app)["folder"].as_str().unwrap()), ("Folder", "/work/two"));
    assert_eq!(app.ed.text(), "/work/two");
}

#[test]
fn start_says_why_it_waits() {
    let mut app = opened();
    assert_eq!(form(&app)["problem"], json!(null));
    app.ed.put("relative/dir");
    assert!(form(&app)["problem"].as_str().unwrap().contains("full path"));
    app.ed.clear();
    app.form_focus(Field::Name);
    app.ed.put(&"n".repeat(61));
    assert!(form(&app)["problem"].as_str().unwrap().contains("60"), "{}", form(&app));
    app.ed.clear();
    app.ed.put("a\nb");
    assert!(form(&app)["problem"].as_str().unwrap().contains("line break"), "{}", form(&app));
    app.ed.clear();
    app.form_focus(Field::Account);
    app.ui.panel.as_mut().unwrap().account = String::new(); // "add a new account"
    assert!(form(&app)["problem"].as_str().unwrap().contains("account"));
    let (_, body) = app.panel_view().unwrap();
    assert!(body.iter().any(|l| l.contains("Start session (not yet:")), "{body:?}");
}

#[test]
fn another_provider_brings_its_own_account_and_the_default_model_and_effort() {
    let mut app = opened();
    {
        let p = app.ui.panel.as_mut().unwrap();
        let f = p.form.as_mut().unwrap();
        (f.models, f.efforts, f.model, f.effort) = (vec!["opus".into(), "sonnet".into()], vec!["low".into(), "high".into()], "opus".into(), "high".into());
    }
    app.form_focus(Field::Model);
    app.form_cycle(1); // opus -> sonnet
    assert_eq!(form(&app)["model"], "sonnet");
    app.form_cycle(1); // wraps round to the default
    assert_eq!(form(&app)["model"], "default");
    app.form_focus(Field::Provider);
    app.form_cycle(1);
    assert_eq!((form(&app)["provider"].as_str().unwrap(), form(&app)["account"].as_str().unwrap()), ("codex", "a3"));
    assert_eq!((form(&app)["model"].as_str().unwrap(), form(&app)["effort"].as_str().unwrap()), ("default", "default"));
    app.form_focus(Field::Model);
    app.form_cycle(1);
    assert_eq!(form(&app)["model"], "default", "codex offers the default only");
    app.form_focus(Field::Account);
    app.form_cycle(1);
    assert_eq!(form(&app)["account"], "", "after the last account comes 'add a new account'");
}

#[test]
fn a_click_on_a_row_focuses_it_and_changes_a_choice_and_a_click_elsewhere_is_left_alone() {
    let mut app = opened();
    assert_eq!(form(&app)["mode"], "auto");
    let permission = form(&app)["rows"].as_array().unwrap().iter().position(|r| r == "Permission").unwrap();
    assert!(app.form_click(permission));
    assert_eq!((form(&app)["focus"].as_str().unwrap(), form(&app)["mode"].as_str().unwrap()), ("Permission", "default"));
    assert!(!app.form_click(40), "a line that is not a row is a selection of text");
}

const CODE: &str = "CANARYCODE9";

#[test]
fn a_sign_in_code_typed_from_the_form_is_nowhere_else_and_the_way_back_gives_the_folder_again() {
    let mut app = opened();
    app.ed.put("/tmp/project");
    app.form_focus(Field::SignIn); // the folder text goes into the form before the line is a code's
    {
        let p = app.ui.panel.as_mut().unwrap();
        p.step = Step::Login;
        p.login = json!({"needs_code": true});
    }
    assert!(app.code_entry() && app.ed.put(CODE));
    for shown in [app.editor_json().to_string(), app.panel_json().to_string(), app.shown_input()] {
        assert!(!shown.contains(CODE), "{shown}");
    }
    app.form_back();
    assert_eq!(form(&app)["focus"], "SignIn");
    app.form_focus(Field::Folder);
    assert_eq!(app.ed.text(), "/tmp/project");
    for shown in [app.editor_json().to_string(), app.panel_json().to_string()] {
        assert!(!shown.contains(CODE), "{shown}");
    }
}

#[test]
fn the_draft_of_the_session_comes_back_when_the_form_is_closed() {
    let mut app = app(1.0, false);
    app.ed.put("my draft");
    app.form_open();
    assert_eq!(app.ed.text(), "");
    app.ed.put("/typed/folder");
    app.panel_close();
    assert_eq!(app.ed.text(), "my draft");
}

#[test]
fn the_form_with_all_ten_recent_folders_and_a_notice_still_fits_a_window_of_600_pixels() {
    let mut app = opened();
    let p = app.ui.panel.as_mut().unwrap();
    p.form.as_mut().unwrap().recent = (0..10).map(|n| format!("/work/project_{n}")).collect();
    p.notice = "Could not ask the host for the choices: unknown. And a second sentence that wraps.".into();
    let (title, body) = app.panel_view().unwrap();
    // the header, the title, the body, the prompt row: one text row of 18 px each, with 22 px of menu bar and 24 px of hint bar around them
    let rows = 1 + 1 + body.len() + 1;
    assert!(title == "New session" && rows * 18 + 22 + 24 <= 600, "{rows} rows: {body:?}");
}

#[test]
fn a_name_that_reads_like_the_default_is_still_a_name_and_a_default_model_and_effort_are_left_out() {
    let mut app = opened();
    app.form_focus(Field::Name);
    app.ed.put(" default ");
    app.form_focus(Field::Model); // the text goes into the form on leaving the row
    let p = app.ui.panel.as_ref().unwrap();
    let request = request_of(p, p.form.as_ref().unwrap());
    assert_eq!(request["name"], "default");
    assert!(request.get("model").is_none() && request.get("effort").is_none() && request.get("cwd").is_none(), "{request}");
}
