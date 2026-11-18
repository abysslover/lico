//! The new-session wizard without a host (`M22-P04b`, `M23-P03`): four icons first (the fourth is the terminal), then the account and folder (or the server), then the options and the start; the rows are the
//! one-page form's own, so each of its functions is on one page of the wizard, and what the wizard starts is what the form would start for the same answers.

use serde_json::json;

use super::super::super::super::scroll_oracle::app;
use super::*;

fn account(id: &str, provider: &str, label: &str, default: bool) -> (String, String, String, bool) {
    (id.into(), provider.into(), label.into(), default)
}

/// The form is open in a window whose host does not answer (so the accounts are put in by hand, as the form's own tests do) on `provider`, as `form_open_from` leaves it, and the wizard begins
/// on top of it.
fn opened_on(home: bool, accounts: &[(&str, &str)], provider: &str) -> App {
    let mut app = app(1.0, false);
    app.form_open();
    let p = app.ui.panel.as_mut().unwrap();
    p.accounts = accounts.iter().map(|(id, kind)| account(id, kind, id, true)).collect();
    p.provider = provider.into();
    p.account = p.first_account();
    p.notice.clear();
    p.form.as_mut().unwrap().recent = vec!["/work/one".into(), "/work/two".into()];
    app.wizard_begin(home);
    app
}

fn opened(home: bool, accounts: &[(&str, &str)]) -> App {
    opened_on(home, accounts, "claude")
}

fn both() -> App {
    opened(false, &[("a1", "claude"), ("a3", "codex")])
}

fn form(app: &App) -> serde_json::Value {
    app.form_json()
}

fn rows(app: &App) -> Vec<String> {
    form(app)["rows"].as_array().unwrap().iter().map(|r| r.as_str().unwrap().to_string()).collect()
}

fn page(app: &App) -> u64 {
    form(app)["wizard"]["page"].as_u64().unwrap()
}

fn title(app: &App) -> String {
    app.panel_view().unwrap().0
}

fn notice(app: &App) -> String {
    app.ui.panel.as_ref().unwrap().notice.clone()
}

#[test]
fn the_wizard_opens_on_step_one_with_four_icons_and_no_text_row() {
    let app = both();
    assert_eq!(page(&app), 0);
    assert_eq!(rows(&app), ["Pick(0)", "Pick(1)", "Pick(2)", "Pick(3)"]);
    assert_eq!(title(&app), "New session - step 1 of 3 - Choose");
    let tiles = app.wizard_tiles().unwrap();
    assert_eq!(tiles.iter().map(|t| (t.provider, t.enabled)).collect::<Vec<_>>(), [("claude", true), ("codex", true), ("local", true), ("terminal", true)]);
    assert!(!app.text_mode(), "no row takes text on the first step");
    let (_, body) = app.panel_view().unwrap();
    let said = body.join("\n");
    for word in ["Provider", "Account", "Folder", "Name", "Model", "Effort", "Permission", "Start session"] {
        assert!(!said.contains(word), "step one shows icons, not the form's rows: {body:?}");
    }
    assert_eq!(form(&app)["wizard"]["name"], "Choose");
}

#[test]
fn a_tile_of_a_provider_with_no_account_is_grey_and_says_why_and_local_never_is() {
    let mut app = opened(false, &[]);
    let enabled = |app: &App| app.wizard_tiles().unwrap().iter().map(|t| t.enabled).collect::<Vec<_>>();
    assert_eq!(enabled(&app), [false, false, true, true], "Local LLM and the terminal work with no account");
    app.wizard_pick(0);
    assert_eq!(page(&app), 0, "a grey tile is not taken");
    assert!(notice(&app).contains("Claude") && notice(&app).contains("account"), "{}", notice(&app));
    assert_eq!(form(&app)["focus"], "Pick(0)", "the focus is on the grey tile, so that its word is read");
    let (_, body) = app.panel_view().unwrap();
    assert!(body.iter().any(|l| l.contains("Claude") && l.contains("account")), "the word line says why: {body:?}");
    app.ui.panel.as_mut().unwrap().accounts = vec![account("a3", "codex", "Lab", true)];
    assert_eq!(enabled(&app), [false, true, true, true], "an account of one provider lights that provider only");
    app.wizard_pick(2);
    assert_eq!(page(&app), 1, "Local LLM is chosen with no Claude account");
}

#[test]
fn choosing_claude_goes_to_step_two_with_the_account_and_folder_rows() {
    let mut app = both();
    app.wizard_pick(0);
    assert_eq!(page(&app), 1);
    assert_eq!(rows(&app), ["Account", "Folder", "Recent(0)", "Recent(1)", "SignIn", "Back", "Next"]);
    assert_eq!(form(&app)["focus"], "Folder");
    assert!(app.text_mode(), "the folder takes text");
    assert_eq!((form(&app)["provider"].as_str().unwrap(), form(&app)["account"].as_str().unwrap()), ("claude", "a1"));
    assert_eq!(title(&app), "New session - step 2 of 3 - Account and folder");
    let (_, body) = app.panel_view().unwrap();
    assert!(body[0].starts_with("  Account") && body[1].starts_with("> Folder") && body.iter().any(|l| l.contains("Next")), "{body:?}");
    assert!(!body.iter().any(|l| l.starts_with("  Name") || l.starts_with("  Model") || l.starts_with("  Effort")), "the options are the next step: {body:?}");
    assert!(body.iter().any(|l| l.contains("Claude settings") && l.contains("CLAUDE.md")), "the line under the rows says what a folder's own settings do: {body:?}");
}

#[test]
fn choosing_codex_brings_its_own_account() {
    let mut app = both();
    app.wizard_pick(1);
    assert_eq!((form(&app)["provider"].as_str().unwrap(), form(&app)["account"].as_str().unwrap()), ("codex", "a3"));
    assert_eq!(page(&app), 1);
    let (_, body) = app.panel_view().unwrap();
    assert!(body.iter().any(|l| l.contains("Codex settings") && l.contains(".codex")), "Codex has its own line under the rows: {body:?}");
    assert!(!body.iter().any(|l| l.contains("CLAUDE.md")), "{body:?}");
}

#[test]
fn the_account_is_taken_when_the_icon_is_chosen_though_the_form_opened_with_none() {
    let mut app = opened(false, &[]); // the form opened on Claude, which had no account: only Local LLM could be chosen
    assert_eq!(form(&app)["account"], "");
    app.ui.panel.as_mut().unwrap().accounts = vec![account("a1", "claude", "a1", true)]; // an account was added since
    app.wizard_pick(0);
    assert_eq!((page(&app), form(&app)["provider"].as_str().unwrap(), form(&app)["account"].as_str().unwrap()), (1, "claude", "a1"), "the provider did not change, the account must still be chosen");
}

#[test]
fn next_is_refused_while_the_step_has_a_problem_and_goes_on_when_it_has_none() {
    let mut app = both();
    app.wizard_pick(0);
    app.ed.put("relative/dir");
    app.wizard_next();
    assert_eq!(page(&app), 1);
    assert!(notice(&app).contains("full path"), "{}", notice(&app));
    assert_eq!(app.ed.text(), "relative/dir", "what was typed is still on the input line");
    app.ed.clear();
    app.ed.put("/work/one");
    app.wizard_next();
    assert_eq!(page(&app), 2);
    assert_eq!(rows(&app), ["Name", "Model", "Effort", "Permission", "Back", "Start"]);
    assert_eq!(title(&app), "New session - step 3 of 3 - Options and start");
    assert_eq!(form(&app)["folder"], "/work/one", "the folder went into the form when the step was left");
    assert_eq!(form(&app)["focus"], "Name");
    assert!(notice(&app).is_empty(), "{}", notice(&app));
}

#[test]
fn a_problem_of_the_name_is_the_last_steps_and_does_not_hold_the_first_one_back() {
    let mut app = both();
    app.wizard_pick(0);
    app.ed.put("/work/one");
    app.wizard_next();
    app.ed.put(&"n".repeat(61)); // the name is judged where it is typed, on the last step
    assert!(app.wizard_escape());
    app.wizard_next();
    assert_eq!(page(&app), 2, "step two does not wait for a name");
    assert!(form(&app)["problem"].as_str().unwrap().contains("60"), "{}", form(&app));
}

#[test]
fn back_goes_one_step_back_with_what_was_typed_kept_and_the_first_step_leaves_to_the_panel() {
    let mut app = both();
    app.wizard_pick(0);
    app.ed.put("/work/two");
    app.wizard_next();
    app.ed.put("my session");
    assert!(app.wizard_escape());
    assert_eq!(page(&app), 1);
    assert_eq!(app.ed.text(), "/work/two", "the folder is on the input line again");
    assert!(app.wizard_escape());
    assert_eq!(page(&app), 0);
    assert_eq!(form(&app)["focus"], "Pick(0)", "the focus is on the icon that was chosen");
    assert!(!app.wizard_escape(), "on the first step Esc is the panel's: it closes");
    app.wizard_pick(0);
    app.wizard_next();
    assert_eq!(form(&app)["name"], "my session", "the name typed on the last step is kept");
}

#[test]
fn the_last_step_makes_the_request_the_one_page_form_makes_for_the_same_answers() {
    let mut app = both();
    app.wizard_pick(0);
    app.ed.put("/work/one");
    app.wizard_next();
    app.ed.put("api");
    app.form_focus(Field::Permission);
    app.form_cycle(1);
    let (p, f) = (app.ui.panel.as_ref().unwrap(), app.ui.panel.as_ref().unwrap().form.as_ref().unwrap());
    let wizard = request_of(p, f);
    let mut plain = app_with_form();
    plain.ed.put("/work/one");
    plain.form_focus(Field::Name);
    plain.ed.put("api");
    plain.form_focus(Field::Permission);
    plain.form_cycle(1);
    let (p, f) = (plain.ui.panel.as_ref().unwrap(), plain.ui.panel.as_ref().unwrap().form.as_ref().unwrap());
    assert_eq!(wizard, request_of(p, f));
    assert_eq!((wizard["kind"].as_str(), wizard["account"].as_str(), wizard["cwd"].as_str(), wizard["name"].as_str(), wizard["mode"].as_str()), (Some("claude"), Some("a1"), Some("/work/one"), Some("api"), Some("default")));
}

/// The one-page form with the same two accounts and the same recent folders, on Claude.
fn app_with_form() -> App {
    let mut app = app(1.0, false);
    app.form_open();
    let p = app.ui.panel.as_mut().unwrap();
    p.accounts = vec![account("a1", "claude", "a1", true), account("a3", "codex", "a3", true)];
    (p.provider, p.account) = ("claude".into(), "a1".into());
    p.form.as_mut().unwrap().recent = vec!["/work/one".into(), "/work/two".into()];
    app
}

#[test]
fn the_home_session_leaves_the_folder_out_of_the_steps_and_of_the_request() {
    let mut app = opened(true, &[("a1", "claude")]);
    assert_eq!(title(&app), "New Home session - step 1 of 3 - Choose");
    assert_eq!(form(&app)["wizard"]["home"], true);
    app.wizard_pick(0);
    assert_eq!(rows(&app), ["Account", "SignIn", "Back", "Next"]);
    assert_eq!(title(&app), "New Home session - step 2 of 3 - Account");
    assert_eq!(form(&app)["focus"], "Account");
    let (_, body) = app.panel_view().unwrap();
    assert!(body.iter().any(|l| l.contains("Home session") && l.contains("scratch workspace")), "the step says what a Home session is: {body:?}");
    app.wizard_next();
    assert_eq!(page(&app), 2);
    let p = app.ui.panel.as_ref().unwrap();
    let request = request_of(p, p.form.as_ref().unwrap());
    assert!(request.get("cwd").is_none(), "a Home session has no folder: {request}");
    assert_eq!(request["account"], "a1");
}

#[test]
fn a_folder_that_is_a_git_repository_brings_the_worktree_rows_to_the_first_step() {
    let mut app = both();
    app.wizard_pick(0);
    app.ed.put("/work/repo"); // (the folder is on the input line while its row has the focus)
    {
        let f = app.ui.panel.as_mut().unwrap().form.as_mut().unwrap();
        (f.folder, f.checked_folder, f.is_git, f.git_repo) = ("/work/repo".into(), "/work/repo".into(), true, "/work/repo".into());
    }
    assert_eq!(rows(&app), ["Account", "Folder", "Recent(0)", "Recent(1)", "Worktree", "SignIn", "Back", "Next"]);
    app.form_focus(Field::Worktree);
    app.form_cycle(1);
    assert!(rows(&app).contains(&"Branch".to_string()));
    app.wizard_next();
    assert_eq!(page(&app), 1, "a worktree needs a branch name first");
    assert!(notice(&app).contains("branch"), "{}", notice(&app));
}

#[test]
fn no_row_of_the_one_page_form_is_lost_in_the_wizard() {
    for (local, git, worktree) in [(false, false, false), (false, true, false), (false, true, true), (true, false, false)] {
        let mut app = both();
        app.wizard_pick(if local { 2 } else { 0 });
        {
            let f = app.ui.panel.as_mut().unwrap().form.as_mut().unwrap();
            (f.is_git, f.worktree) = (git, worktree);
        }
        let (one_page, first) = {
            let f = app.ui.panel.as_ref().unwrap().form.as_ref().unwrap();
            let all = f.all_rows();
            (all, f.fields())
        };
        app.with_form(|_, f| f.wiz = Some(Wiz { page: 2, home: false, terminal: false }));
        let second = app.ui.panel.as_ref().unwrap().form.as_ref().unwrap().fields();
        let mut seen: Vec<String> = first.iter().chain(&second).filter(|r| !matches!(r, Field::Back | Field::Next)).map(|r| format!("{r:?}")).collect();
        let mut wanted: Vec<String> = one_page.iter().filter(|r| **r != Field::Provider).map(|r| format!("{r:?}")).collect();
        seen.sort();
        wanted.sort();
        assert_eq!(seen, wanted, "local {local} git {git} worktree {worktree}: every row but the provider is on exactly one page");
    }
}

#[test]
fn local_llm_looks_at_this_machine_when_chosen_and_not_again_when_the_step_is_left_and_entered() {
    let mut app = both();
    app.wizard_pick(2);
    assert_eq!(page(&app), 1);
    assert_eq!(rows(&app), ["Server", "Back", "Next"]);
    assert_eq!(title(&app), "New session - step 2 of 3 - Local LLM server");
    assert_eq!(form(&app)["focus"], "Server", "the server row has the focus: it is the only choice of the step");
    assert!(notice(&app).contains("Could not look for servers"), "the one look was made at once (no host answers here): {}", notice(&app));
    app.on_local_scan(0, json!({"ok": 1, "endpoints": [
        {"address": "127.0.0.1", "port": 11434, "kind": "ollama", "auth": "none_needed", "models": ["llama3", "qwen2"], "process": "ollama"},
    ]}));
    assert!(notice(&app).contains("Found 1 server"), "{}", notice(&app));
    assert!(app.wizard_escape());
    app.wizard_pick(2);
    assert!(!notice(&app).contains("Could not look") && !notice(&app).contains("Looking"), "a result that is there is kept and no second look is made: {}", notice(&app));
    assert_eq!(form(&app)["servers"].as_array().unwrap().len(), 1);
    let (_, body) = app.panel_view().unwrap();
    assert!(body.iter().any(|l| l.contains("11434")), "the servers found are listed: {body:?}");
}

#[test]
fn local_llm_goes_on_to_the_model_and_the_name_and_starts_with_the_servers_request() {
    let mut app = both();
    app.wizard_pick(2);
    app.on_local_scan(0, json!({"ok": 1, "endpoints": [
        {"address": "127.0.0.1", "port": 11434, "kind": "ollama", "auth": "none_needed", "models": ["llama3", "qwen2"], "process": "ollama"},
    ]}));
    app.wizard_next();
    assert_eq!(page(&app), 2);
    assert_eq!(rows(&app), ["Model", "Name", "Back", "Start"]);
    assert_eq!(title(&app), "New session - step 3 of 3 - Options and start");
    let p = app.ui.panel.as_ref().unwrap();
    let request = request_of(p, p.form.as_ref().unwrap());
    assert_eq!(request["kind"], "local");
    assert!(request["endpoint"].as_str().unwrap().contains("11434"), "{request}");
}

#[test]
fn local_llm_cannot_go_on_with_no_server_and_says_so() {
    let mut app = both();
    app.wizard_pick(2);
    app.wizard_next();
    assert_eq!(page(&app), 1);
    assert!(notice(&app).contains("Not yet"), "{}", notice(&app));
}

#[test]
fn enter_on_the_server_row_with_no_server_looks_again_it_does_not_go_on() {
    let mut app = both();
    app.wizard_pick(2);
    app.ui.panel.as_mut().unwrap().notice.clear();
    app.form_enter();
    assert_eq!(page(&app), 1, "there is nothing to go on with");
    assert!(notice(&app).contains("Could not look for servers"), "Enter on the server row, with none chosen, is the look for servers: {}", notice(&app));
}

#[test]
fn enter_is_next_on_step_two_and_the_start_on_step_three_and_a_row_with_a_meaning_of_its_own_keeps_it() {
    let mut app = both();
    app.wizard_pick(0);
    app.form_move(1); // the first recent folder
    assert_eq!(form(&app)["focus"], "Recent(0)");
    app.form_enter();
    assert_eq!((page(&app), form(&app)["folder"].as_str().unwrap()), (1, "/work/one"), "Enter on a recent folder takes it and goes no further");
    app.form_enter();
    assert_eq!(page(&app), 2, "Enter on the folder is Next");
    app.form_enter();
    assert!(notice(&app).starts_with("The session was not started"), "Enter on step three starts the session (no host answers here): {}", notice(&app));
}

#[test]
fn enter_on_a_tile_chooses_it_and_on_back_and_next_does_what_they_say() {
    let mut app = both();
    app.form_move(1); // the Codex tile
    app.form_enter();
    assert_eq!((page(&app), form(&app)["provider"].as_str().unwrap()), (1, "codex"));
    app.form_focus(Field::Back);
    app.form_enter();
    assert_eq!(page(&app), 0);
    app.wizard_pick(0);
    app.form_focus(Field::Next);
    app.ed.clear();
    app.form_enter();
    assert_eq!(page(&app), 2, "Enter on Next goes on");
}

#[test]
fn left_and_right_move_over_the_icons_and_a_click_on_a_back_or_next_row_acts() {
    let mut app = both();
    assert_eq!(form(&app)["focus"], "Pick(0)");
    app.form_cycle(1);
    assert_eq!(form(&app)["focus"], "Pick(1)");
    app.form_cycle(1);
    app.form_cycle(1);
    app.form_cycle(1);
    assert_eq!(form(&app)["focus"], "Pick(0)", "round");
    app.form_cycle(-1);
    assert_eq!(form(&app)["focus"], "Pick(3)");
    app.wizard_pick(0);
    let next = rows(&app).iter().position(|r| r == "Next").unwrap();
    assert!(app.form_click(next));
    assert_eq!(page(&app), 2);
    let back = rows(&app).iter().position(|r| r == "Back").unwrap();
    assert!(app.form_click(back));
    assert_eq!(page(&app), 1);
}

#[test]
fn on_the_first_step_a_click_on_a_line_is_left_alone_the_icons_have_their_own_targets() {
    let mut app = both();
    for line in 0..4 {
        assert!(!app.form_click(line), "line {line}");
    }
    assert_eq!(page(&app), 0);
}

#[test]
fn another_icon_after_going_back_resets_the_account_and_the_options_of_the_first_choice() {
    let mut app = both();
    {
        let f = app.ui.panel.as_mut().unwrap().form.as_mut().unwrap();
        (f.models, f.model) = (vec!["opus".into()], "opus".into());
    }
    app.wizard_pick(0);
    assert!(app.wizard_escape());
    app.wizard_pick(1);
    assert_eq!((form(&app)["provider"].as_str().unwrap(), form(&app)["account"].as_str().unwrap(), form(&app)["model"].as_str().unwrap()), ("codex", "a3", "default"));
}

#[test]
fn the_first_focus_is_the_icon_of_the_provider_the_form_opened_on_or_else_the_first_one_that_can_be_chosen() {
    assert_eq!(form(&both())["focus"], "Pick(0)");
    assert_eq!(form(&opened_on(false, &[("a3", "codex")], "codex"))["focus"], "Pick(1)", "the provider the form opened on");
    assert_eq!(form(&opened_on(false, &[("a3", "codex")], "claude"))["focus"], "Pick(1)", "the last provider has no account any more: the first one that has");
    assert_eq!(form(&opened_on(false, &[], "claude"))["focus"], "Pick(2)", "with no account at all only Local LLM can be chosen, and it has the focus");
    let mut app = both();
    app.wizard_pick(1);
    assert!(app.wizard_escape());
    assert_eq!(form(&app)["focus"], "Pick(1)", "back from a step the icon that was chosen has the focus");
}

#[test]
fn with_a_grey_icon_the_first_step_says_where_an_account_comes_from() {
    let app = opened(false, &[]);
    let (_, body) = app.panel_view().unwrap();
    assert!(body.iter().any(|l| l.contains("Claude and Codex need an account") && l.contains("person icon")), "{body:?}");
    let (_, body) = both().panel_view().unwrap();
    assert!(!body.iter().any(|l| l.contains("need an account")), "with both accounts nothing is grey and nothing is said: {body:?}");
}

#[test]
fn the_state_says_the_step_the_tiles_and_nothing_when_the_one_page_form_is_open() {
    let app = both();
    let f = form(&app);
    assert_eq!((f["wizard"]["page"].clone(), f["wizard"]["pages"].clone(), f["wizard"]["home"].clone()), (json!(0), json!(3), json!(false)));
    let tiles = f["tiles"].as_array().unwrap();
    assert_eq!(tiles.iter().map(|t| t["provider"].as_str().unwrap()).collect::<Vec<_>>(), ["claude", "codex", "local", "terminal"]);
    assert_eq!(tiles.iter().map(|t| t["word"].as_str().unwrap()).collect::<Vec<_>>(), ["Claude", "Codex", "Local LLM", "Terminal"]);
    assert_eq!(tiles.iter().filter(|t| t["focus"] == json!(true)).count(), 1);
    let one_page = app_with_form();
    assert_eq!((form(&one_page)["wizard"].clone(), form(&one_page)["tiles"].clone()), (json!(null), json!(null)));
    assert_eq!(title(&one_page), "New session");
    let mut p = app_with_form();
    p.wizard_pick(0);
    assert_eq!(rows(&p).first().map(String::as_str), Some("Provider"), "the one-page form ignores the wizard's keys");
}

#[test]
fn the_wizard_steps_and_their_text_fit_a_window_of_600_pixels() {
    let mut app = both();
    app.ui.panel.as_mut().unwrap().form.as_mut().unwrap().recent = (0..10).map(|n| format!("/work/project_{n}")).collect();
    app.ui.panel.as_mut().unwrap().notice = "Could not ask the host for the choices: unknown. And a second sentence that wraps.".into();
    for step in 0..3 {
        if step == 1 {
            app.wizard_pick(0);
        }
        if step == 2 {
            app.ed.put("/work/one");
            app.wizard_next();
        }
        let (_, body) = app.panel_view().unwrap();
        let rows = 1 + 1 + body.len() + 1;
        assert!(rows * 18 + 22 + 24 <= 600, "step {step}: {rows} rows: {body:?}");
    }
}

// ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- the terminal icon (`M23-P03`)

#[test]
fn the_terminal_icon_needs_no_account_and_has_the_fourth_place() {
    let app = opened(false, &[]);
    assert_eq!(rows(&app), ["Pick(0)", "Pick(1)", "Pick(2)", "Pick(3)"]);
    let tiles = app.wizard_tiles().unwrap();
    assert_eq!(tiles.iter().map(|t| (t.provider, t.word, t.enabled)).collect::<Vec<_>>(), [("claude", "Claude", false), ("codex", "Codex", false), ("local", "Local LLM", true), ("terminal", "Terminal", true)]);
    let said = app.panel_view().unwrap().1.join("\n");
    assert!(!said.contains("Terminal needs") && !said.contains("and Terminal"), "a terminal is never said to need an account: {said}");
}

#[test]
fn choosing_the_terminal_goes_to_one_more_step_that_asks_for_the_folder_and_nothing_else() {
    let mut app = both();
    app.wizard_pick(3);
    assert_eq!(page(&app), 1);
    assert_eq!(rows(&app), ["Folder", "Recent(0)", "Recent(1)", "Back", "Next"], "no account, worktree, name, model or permission row");
    assert_eq!(title(&app), "New session - step 2 of 2 - Terminal folder");
    assert_eq!(form(&app)["focus"], "Folder");
    assert_eq!(form(&app)["wizard"]["pages"], 2);
    assert_eq!(form(&app)["wizard"]["terminal"], true);
    assert!(app.text_mode(), "the folder row takes text");
    let (_, body) = app.panel_view().unwrap();
    let said = body.join("\n");
    assert!(said.contains("(empty: your home folder)"), "{said}");
    assert!(said.contains("Start terminal") && said.contains("not fenced"), "{said}");
    assert!(!said.contains("Home session") && !said.contains("scratch workspace"), "{said}");
}

#[test]
fn the_terminal_is_started_by_next_and_by_enter_on_a_folder_that_is_a_full_path_or_empty() {
    let mut app = both();
    app.wizard_pick(3);
    app.ed.put("relative/dir");
    app.wizard_next();
    assert_eq!(page(&app), 1);
    assert!(app.ui.panel.is_some() && notice(&app).contains("full path"), "a folder that is not a full path is refused on the step: {}", notice(&app));
    assert!(rows(&app).len() == 5 && app.wizard_value_of_next().contains("not yet"), "{}", app.wizard_value_of_next());
    app.ed.clear();
    app.ed.put("/work/one");
    assert_eq!(app.wizard_value_of_next(), "Start terminal  (Enter)");
    app.form_enter();
    assert!(app.ui.panel.is_none(), "Enter on the folder starts the terminal and the panel is gone");
    assert!(app.ui.flash.as_deref().is_some_and(|f| f.contains("terminal refused")), "(no host answers here, and the window says so): {:?}", app.ui.flash);
    let mut again = both();
    again.wizard_pick(3);
    again.wizard_next(); // an empty folder is the person's home
    assert!(again.ui.panel.is_none(), "an empty folder is allowed: the host opens the shell at home");
}

#[test]
fn back_from_the_terminal_step_puts_the_focus_on_its_icon_and_another_icon_is_not_a_terminal() {
    let mut app = both();
    app.wizard_pick(3);
    assert!(app.wizard_escape());
    assert_eq!((page(&app), form(&app)["focus"].as_str().unwrap()), (0, "Pick(3)"), "the focus is on the terminal icon");
    app.wizard_pick(0);
    assert_eq!(page(&app), 1);
    assert_eq!(rows(&app).first().map(String::as_str), Some("Account"), "Claude again: its own rows");
    assert_eq!(title(&app), "New session - step 2 of 3 - Account and folder");
    assert_eq!(form(&app)["wizard"]["terminal"], false);
}

#[test]
fn left_and_right_go_round_the_four_icons() {
    let mut app = both();
    app.form_focus(Field::Pick(3));
    app.form_cycle(1);
    assert_eq!(form(&app)["focus"], "Pick(0)", "after the fourth, the first");
    app.form_cycle(-1);
    assert_eq!(form(&app)["focus"], "Pick(3)", "and before the first, the fourth");
}

impl App {
    /// The text of the Next row on the terminal's step.
    fn wizard_value_of_next(&self) -> String {
        let p = self.ui.panel.as_ref().unwrap();
        self.wizard_value(p, p.form.as_ref().unwrap(), Field::Next)
    }
}
