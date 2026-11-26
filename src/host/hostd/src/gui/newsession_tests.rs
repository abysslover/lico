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
fn the_line_about_a_folders_own_settings_is_the_one_of_the_chosen_provider() {
    let mut app = opened();
    let p = app.ui.panel.as_mut().unwrap();
    (p.provider, p.account) = ("codex".into(), "a3".into());
    let (_, body) = app.panel_view().unwrap();
    assert!(body.iter().any(|l| l.contains("Codex settings (.codex: config, hooks, rules) are not used")), "{body:?}");
    assert!(!body.iter().skip(1).any(|l| l.contains("Claude") || l.contains("CLAUDE.md")), "a Codex session is not told about Claude (the Provider row names all three modes): {body:?}");
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
    app.ed.put("\u{3164}"); // N1: a name that shows nothing is refused by the host, and the form says so instead of "more than 60"
    assert!(form(&app)["problem"].as_str().unwrap().contains("shows nothing"), "{}", form(&app));
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

#[test]
fn git_worktree_rows_appear_only_for_git_folders_validate_branch_and_still_fit_600_pixels() {
    let mut app = opened();
    app.ed.put("/work/repo");
    {
        let f = app.ui.panel.as_mut().unwrap().form.as_mut().unwrap();
        f.folder = "/work/repo".into();
        f.checked_folder = "/work/repo".into();
        f.is_git = true;
        f.git_repo = "/work/repo".into();
        f.recent = (0..10).map(|n| format!("/work/project_{n}")).collect();
    }
    let rows: Vec<String> = form(&app)["rows"].as_array().unwrap().iter().map(|r| r.as_str().unwrap().to_string()).collect();
    assert!(rows.contains(&"Worktree".to_string()) && !rows.contains(&"Branch".to_string()), "{rows:?}");

    let wt_idx = rows.iter().position(|r| r == "Worktree").unwrap();
    assert!(app.form_click(wt_idx));
    assert_eq!(form(&app)["worktree"], true);
    assert!(form(&app)["problem"].as_str().unwrap().contains("branch name"), "{}", form(&app));

    app.form_focus(Field::Branch);
    app.ed.put("-bad");
    assert!(form(&app)["problem"].as_str().unwrap().contains("not valid for git"), "{}", form(&app));

    app.ed.clear();
    app.ed.put("feat/clean-branch");
    assert_eq!(form(&app)["problem"], json!(null));
    app.form_focus(Field::Start);
    let p = app.ui.panel.as_ref().unwrap();
    let req = request_of(p, p.form.as_ref().unwrap());
    assert_eq!((req["cwd"].as_str(), req["worktree_branch"].as_str()), (Some("/work/repo"), Some("feat/clean-branch")));

    let (_, body) = app.panel_view().unwrap();
    let total_rows = 1 + 1 + body.len() + 1;
    assert!(total_rows * 18 + 22 + 24 <= 600, "{total_rows} rows: {body:?}");
}

// ---- the local provider (M16-P07) ----

fn scan_reply() -> serde_json::Value {
    json!({"ok": 1, "endpoints": [
        {"address": "127.0.0.1", "port": 11434, "kind": "ollama", "auth": "none_needed", "models": ["llama3", "qwen2"], "process": "ollama"},
        {"address": "127.0.0.1", "port": 1234, "kind": "openai", "auth": "key_needed", "models": [], "process": "x"},
    ]})
}

/// The form with the Local provider chosen the way a person chooses it: Provider row, Right twice (claude, codex, local). Choosing it makes the one look (`M22-P04a`; no host answers in this window, so it
/// fails at once): the form returned is the one before any look, as the tests of the Server row need it.
fn local() -> App {
    let mut app = chosen_local();
    app.ui.panel.as_mut().unwrap().notice.clear();
    app
}

fn chosen_local() -> App {
    let mut app = opened();
    app.form_focus(Field::Provider);
    app.form_cycle(1);
    app.form_cycle(1);
    app
}

fn rows(app: &App) -> Vec<String> {
    form(app)["rows"].as_array().unwrap().iter().map(|r| r.as_str().unwrap().to_string()).collect()
}

#[test]
fn local_is_the_third_provider_and_asks_for_a_server_and_a_model_but_not_for_an_account_a_folder_or_an_effort() {
    let mut app = opened();
    app.ui.panel.as_mut().unwrap().notice = "The claude program's own help could not be read: only the default model and effort are offered.".into();
    app.form_focus(Field::Provider);
    app.form_cycle(1);
    app.form_cycle(1);
    assert!(!app.ui.panel.as_ref().unwrap().notice.contains("claude program"), "a notice about the claude program is not for another provider");
    assert_eq!(form(&app)["provider"], "local");
    assert_eq!(rows(&app), ["Provider", "Server", "Model", "Name", "Start"]);
    let (_, body) = app.panel_view().unwrap();
    assert!(body[0].contains("[Local LLM]") && body[1].starts_with("  Server"), "{body:?}");
    assert!(!body.iter().any(|l| l.contains("Account") || l.contains("Folder") || l.contains("Effort") || l.contains("Permission") || l.contains("Sign in")), "{body:?}");
    app.form_focus(Field::Provider);
    app.form_cycle(1); // wraps round to claude, with its account and its rows again
    assert_eq!((form(&app)["provider"].as_str().unwrap(), form(&app)["account"].as_str().unwrap()), ("claude", "a1"));
    assert!(rows(&app).contains(&"Account".to_string()) && rows(&app).contains(&"Folder".to_string()));
}

#[test]
fn opening_the_form_never_looks_for_servers_and_says_how_to_look() {
    let app = local();
    let f = form(&app);
    assert_eq!((f["servers"].as_array().unwrap().len(), f["scanned"].clone()), (0, json!(false)));
    let (_, body) = app.panel_view().unwrap();
    assert!(body.iter().any(|l| l.contains("press Enter on the Server row to look for model servers on this machine")), "{body:?}");
    assert!(body.iter().any(|l| l.starts_with("  Server") && l.ends_with("(none yet)")), "the row itself does not repeat the hint: {body:?}");
    assert!(form(&app)["problem"].as_str().unwrap().contains("Server row"), "{}", form(&app));
}

#[test]
fn only_enter_space_or_a_click_on_the_server_row_looks_for_servers() {
    let looked = |app: &App| app.ui.panel.as_ref().unwrap().notice.contains("Could not look for servers"); // no host answers in this window: the look was made and failed
    let mut app = local();
    app.form_focus(Field::Name);
    app.form_move(-1); // Model
    app.form_cycle(1);
    app.form_move(-1); // Server: moving onto it and changing nothing asks nothing
    assert_eq!(form(&app)["focus"], "Server");
    app.form_cycle(1);
    app.form_cycle(-1);
    assert!(!looked(&app) && form(&app)["scanned"] == json!(false));
    app.form_enter();
    assert!(looked(&app), "Enter on the Server row");
    app.ui.panel.as_mut().unwrap().notice.clear();
    app.form_act();
    assert!(looked(&app), "Space on the Server row");
    app.ui.panel.as_mut().unwrap().notice.clear();
    assert!(app.form_click(3) && !looked(&app), "a click on the Name row looks at nothing");
    assert!(app.form_click(1) && looked(&app), "a click on the Server row");
}

#[test]
fn what_the_look_finds_is_kept_in_the_form_the_first_server_and_its_first_model_are_chosen_and_a_failed_look_keeps_what_was_there() {
    let mut app = local();
    app.on_local_scan(0, scan_reply());
    let f = form(&app);
    assert_eq!((f["scanned"].clone(), f["servers"].as_array().unwrap().len(), f["server"].clone(), f["local_model"].clone()), (json!(true), 2, json!(0), json!("llama3")));
    assert_eq!(f["problem"], json!(null));
    let (_, body) = app.panel_view().unwrap();
    assert!(body.iter().any(|l| l.contains("127.0.0.1:11434  ollama, 2 models")), "{body:?}");
    assert!(app.ui.panel.as_ref().unwrap().notice.contains("2 servers"), "{}", app.ui.panel.as_ref().unwrap().notice);
    app.on_local_scan(0, json!({"ok": 0, "error": "busy"}));
    assert_eq!(form(&app)["servers"].as_array().unwrap().len(), 2);
    assert!(app.ui.panel.as_ref().unwrap().notice.contains("Could not look for servers: busy"));
    app.on_local_scan(0, json!({"ok": 1, "endpoints": []}));
    assert_eq!(form(&app)["servers"].as_array().unwrap().len(), 0);
    assert!(app.ui.panel.as_ref().unwrap().notice.contains("No model server was found"));
}

#[test]
fn left_and_right_choose_the_server_and_the_model_and_a_new_server_brings_its_own_first_model() {
    let mut app = local();
    app.on_local_scan(0, json!({"ok": 1, "endpoints": [
        {"address": "127.0.0.1", "port": 11434, "kind": "ollama", "auth": "none_needed", "models": ["llama3", "qwen2"]},
        {"address": "127.0.0.1", "port": 8080, "kind": "openai", "auth": "none_needed", "models": ["gpt-x"]},
    ]}));
    app.form_focus(Field::Model);
    app.form_cycle(1);
    assert_eq!(form(&app)["local_model"], "qwen2");
    app.form_cycle(1);
    assert_eq!(form(&app)["local_model"], "llama3", "the models wrap round");
    app.form_cycle(-1);
    app.form_focus(Field::Server);
    app.form_cycle(1);
    assert_eq!((form(&app)["server"].clone(), form(&app)["local_model"].clone()), (json!(1), json!("gpt-x")));
    app.form_cycle(1);
    assert_eq!((form(&app)["server"].clone(), form(&app)["local_model"].clone()), (json!(0), json!("llama3")));
}

#[test]
fn a_server_that_wants_a_key_says_how_to_store_it_and_cannot_start_without_models() {
    let mut app = local();
    app.on_local_scan(0, json!({"ok": 1, "endpoints": [{"address": "127.0.0.1", "port": 1234, "kind": "openai", "auth": "key_needed", "models": []}]}));
    let (_, body) = app.panel_view().unwrap();
    assert!(body.iter().any(|l| l.contains("lico local-key set 127.0.0.1:1234")), "{body:?}");
    assert!(form(&app)["problem"].as_str().unwrap().contains("wants a key"), "{}", form(&app));
}

#[test]
fn a_local_session_is_told_it_has_no_files_or_shell_and_not_about_a_folders_settings() {
    let app = local();
    let (_, body) = app.panel_view().unwrap();
    assert!(body.iter().any(|l| l.contains("no access to files or a shell")), "{body:?}");
    assert!(!body.iter().skip(1).any(|l| l.contains("Claude") || l.contains("Codex") || l.contains("CLAUDE.md")), "{body:?}");
}

#[test]
fn the_local_request_has_the_endpoint_the_format_the_model_and_a_name_but_no_account_folder_or_effort() {
    let mut app = local();
    app.on_local_scan(0, scan_reply());
    app.form_focus(Field::Name);
    app.ed.put(" notes ");
    app.form_focus(Field::Start);
    let p = app.ui.panel.as_ref().unwrap();
    let request = request_of(p, p.form.as_ref().unwrap());
    assert_eq!(request, json!({"op": "session.create", "kind": "local", "endpoint": "127.0.0.1:11434", "api": "ollama", "model": "llama3", "name": "notes"}));
    // the name's own rules still hold
    app.form_focus(Field::Name);
    app.ed.clear();
    app.ed.put(&"n".repeat(61));
    assert!(form(&app)["problem"].as_str().unwrap().contains("60"), "{}", form(&app));
}

#[test]
fn the_server_of_the_last_local_session_is_offered_when_the_form_opens_without_any_look() {
    let mut app = app(1.0, false);
    app.panel_open(Step::NewSession);
    app.form_open_from(json!({"ok": 1, "last": {"kind": "local", "mode": "auto", "model": "llama3", "endpoint": "127.0.0.1:11434", "api": "ollama"}}));
    let f = form(&app);
    assert_eq!((f["provider"].clone(), f["scanned"].clone(), f["local_model"].clone()), (json!("local"), json!(false), json!("llama3")));
    assert_eq!(f["servers"].as_array().unwrap().len(), 1);
    assert!(f["servers"][0].as_str().unwrap().contains("127.0.0.1:11434") && f["servers"][0].as_str().unwrap().contains("used last"), "{f}");
    assert_eq!(f["problem"], json!(null), "the remembered choice can be started at once");
    assert!(f["focus"] == "Server" && !app.text_mode(), "the focus is on a row the form shows, and no hidden text row has the input line: {f}");
    // a scan that finds the same server marks it and keeps it first with the remembered model
    app.on_local_scan(0, json!({"ok": 1, "endpoints": [{"address": "127.0.0.1", "port": 8080, "kind": "openai", "auth": "none_needed", "models": ["gpt-x"]}, {"address": "127.0.0.1", "port": 11434, "kind": "ollama", "auth": "none_needed", "models": ["qwen2"]}]}));
    let f = form(&app);
    assert_eq!(f["servers"].as_array().unwrap().len(), 2);
    assert!(f["servers"][0].as_str().unwrap().contains("11434") && f["local_model"] == json!("llama3"), "{f}");
    // a last session of claude does not bring a local server
    app.form_open_from(json!({"ok": 1, "last": {"kind": "claude", "mode": "auto"}}));
    assert_eq!((form(&app)["provider"].clone(), form(&app)["servers"].as_array().unwrap().len()), (json!("claude"), 0));
}

#[test]
fn the_forms_that_ask_for_a_folder_never_open_on_local() {
    let mut app = app(1.0, false);
    app.panel_open(Step::NewSession);
    app.form_open_from(json!({"ok": 1, "last": {"kind": "local", "mode": "auto", "model": "llama3", "endpoint": "127.0.0.1:11434", "api": "ollama"}}));
    app.form_leave_local();
    let f = form(&app);
    assert_eq!((f["provider"].clone(), f["focus"].clone()), (json!("claude"), json!("Folder")));
    assert!(rows(&app).contains(&"Folder".to_string()));
}

#[test]
fn the_provider_row_shows_the_three_modes_at_once_with_the_chosen_one_in_brackets() {
    let mut app = opened();
    let (_, body) = app.panel_view().unwrap();
    assert!(body[0].contains("[Claude]") && body[0].contains("Codex") && body[0].contains("Local LLM") && !body[0].contains("[Codex]") && !body[0].contains("[Local LLM]"), "{body:?}");
    app.form_focus(Field::Provider);
    app.form_cycle(1);
    let (_, body) = app.panel_view().unwrap();
    assert!(body[0].contains("[Codex]") && body[0].contains("Claude") && !body[0].contains("[Claude]"), "{body:?}");
    app.form_cycle(1);
    let (_, body) = app.panel_view().unwrap();
    assert!(body[0].contains("[Local LLM]") && body[0].contains("Codex"), "{body:?}");
}

#[test]
fn choosing_local_llm_on_the_provider_row_looks_for_its_candidates_at_once_and_a_result_that_is_there_is_not_asked_for_again() {
    let looked = |app: &App| app.ui.panel.as_ref().unwrap().notice.contains("Could not look for servers"); // no host answers in this window: the look was made and failed
    let mut app = opened();
    app.form_focus(Field::Provider);
    app.form_cycle(1);
    assert!(!looked(&app), "Codex is not Local LLM: nothing is looked at");
    app.form_cycle(1);
    assert!(looked(&app), "Local LLM chosen: the one look is made, on this machine");
    app.on_local_scan(0, scan_reply());
    app.ui.panel.as_mut().unwrap().notice.clear();
    app.form_focus(Field::Provider);
    app.form_cycle(1); // claude
    app.form_cycle(1); // codex
    app.form_cycle(1); // local again
    assert_eq!(form(&app)["provider"], "local");
    assert!(!looked(&app) && form(&app)["servers"].as_array().unwrap().len() == 2, "the result stays and is not asked for again");
}

#[test]
fn the_candidates_are_listed_under_the_rows_and_a_click_on_one_chooses_it() {
    let mut app = local();
    app.on_local_scan(0, scan_reply());
    let (_, body) = app.panel_view().unwrap();
    let head = body.iter().position(|l| l.starts_with("Model servers on this machine")).expect("the candidates have a heading");
    assert!(body[head + 1].starts_with("> 127.0.0.1:11434") && body[head + 2].starts_with("  127.0.0.1:1234"), "{body:?}");
    assert!(body[head + 1].matches("127.0.0.1:11434").count() == 1, "the address is said once on a line: {body:?}");
    assert_eq!(head, rows(&app).len() + 3, "the heading comes after the rows, a blank line, the line of keys and a blank line");
    assert!(app.form_click(head + 2), "a line of the candidates is a click that is taken");
    assert_eq!(form(&app)["server"], 1);
    let (_, body) = app.panel_view().unwrap();
    assert!(body[head + 2].starts_with("> 127.0.0.1:1234") && body[head + 1].starts_with("  127.0.0.1:11434"), "the marker follows the click: {body:?}");
    assert!(!app.form_click(head + 3), "a line past the candidates is no candidate");
}

#[test]
fn a_start_the_host_could_not_make_is_a_sentence_and_a_word_the_window_does_not_know_is_said_as_it_came() {
    assert_eq!(refusal("busy"), "The host could not start it just now.", "a program that did not start (agy) is the host's `busy`");
    assert_eq!(refusal("some_new_code"), "some_new_code");
}
