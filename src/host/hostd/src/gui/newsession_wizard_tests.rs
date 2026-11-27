//! The new-session wizard without a host (`M22-P04b`, `M23-P03`): five icons first (the second is `agy`, the fifth the terminal), then the account and folder (or the server), then the options and the start; the rows are the
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
fn the_wizard_opens_on_step_one_with_five_icons_and_no_text_row() {
    let app = both();
    assert_eq!(page(&app), 0);
    assert_eq!(rows(&app), ["Pick(0)", "Pick(1)", "Pick(2)", "Pick(3)", "Pick(4)"]);
    assert_eq!(title(&app), "New session");
    assert_eq!(form(&app)["wizard"]["title"], "New session - step 1 of 3 - Choose");
    let tiles = app.wizard_tiles().unwrap();
    assert_eq!(tiles.iter().map(|t| (t.provider, t.enabled)).collect::<Vec<_>>(), [("claude", true), ("agy", false), ("codex", true), ("local", true), ("terminal", true)]);
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
    assert_eq!(enabled(&app), [false, false, false, true, true], "Local LLM and the terminal work with no account; agy is found or not on the machine, not by an account");
    app.wizard_pick(0);
    assert_eq!(page(&app), 0, "a grey tile is not taken");
    assert!(notice(&app).contains("Claude") && notice(&app).contains("account"), "{}", notice(&app));
    assert_eq!(form(&app)["focus"], "Pick(0)", "the focus is on the grey tile, so that its word is read");
    let (_, body) = app.panel_view().unwrap();
    assert!(body.iter().any(|l| l.contains("Claude") && l.contains("account")), "the word line says why: {body:?}");
    app.ui.panel.as_mut().unwrap().accounts = vec![account("a3", "codex", "Lab", true)];
    assert_eq!(enabled(&app), [false, false, true, true, true], "an account of one provider lights that provider only");
    app.wizard_pick(3);
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
    assert_eq!(title(&app), "New session");
    assert_eq!(form(&app)["wizard"]["title"], "New session - step 2 of 3 - Account and folder");
    let (_, body) = app.panel_view().unwrap();
    assert!(body[0].starts_with("  Account") && body[1].starts_with("> Folder") && body.iter().any(|l| l.contains("Next")), "{body:?}");
    assert!(!body.iter().any(|l| l.starts_with("  Name") || l.starts_with("  Model") || l.starts_with("  Effort")), "the options are the next step: {body:?}");
    assert!(body.iter().any(|l| l.contains("Claude settings") && l.contains("CLAUDE.md")), "the line under the rows says what a folder's own settings do: {body:?}");
}

#[test]
fn choosing_codex_brings_its_own_account() {
    let mut app = both();
    app.wizard_pick(2);
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
    assert_eq!(title(&app), "New session");
    assert_eq!(form(&app)["wizard"]["title"], "New session - step 3 of 3 - Options and start");
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
    assert_eq!(title(&app), "New Bot session");
    assert_eq!(form(&app)["wizard"]["title"], "New Bot session - step 1 of 3 - Choose");
    assert_eq!(form(&app)["wizard"]["home"], true);
    app.wizard_pick(0);
    assert_eq!(rows(&app), ["Account", "SignIn", "Back", "Next"]);
    assert_eq!(title(&app), "New Bot session");
    assert_eq!(form(&app)["wizard"]["title"], "New Bot session - step 2 of 3 - Account");
    assert_eq!(form(&app)["focus"], "Account");
    let (_, body) = app.panel_view().unwrap();
    assert!(body.iter().any(|l| l.contains("Bot session") && l.contains("scratch workspace")), "the step says what a Bot session is: {body:?}");
    app.wizard_next();
    assert_eq!(page(&app), 2);
    let p = app.ui.panel.as_ref().unwrap();
    let request = request_of(p, p.form.as_ref().unwrap());
    assert!(request.get("cwd").is_none(), "a Bot session has no folder: {request}");
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
        app.wizard_pick(if local { 3 } else { 0 });
        {
            let f = app.ui.panel.as_mut().unwrap().form.as_mut().unwrap();
            (f.is_git, f.worktree) = (git, worktree);
        }
        let (one_page, first) = {
            let f = app.ui.panel.as_ref().unwrap().form.as_ref().unwrap();
            let all = f.all_rows();
            (all, f.fields())
        };
        app.with_form(|_, f| f.wiz = Some(Wiz { page: 2, home: false }));
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
    app.wizard_pick(3);
    assert_eq!(page(&app), 1);
    assert_eq!(rows(&app), ["Server", "Back", "Next"]);
    assert_eq!(title(&app), "New session");
    assert_eq!(form(&app)["wizard"]["title"], "New session - step 2 of 3 - Local LLM server");
    assert_eq!(form(&app)["focus"], "Server", "the server row has the focus: it is the only choice of the step");
    assert!(notice(&app).contains("Could not look for servers"), "the one look was made at once (no host answers here): {}", notice(&app));
    app.on_local_scan(0, json!({"ok": 1, "endpoints": [
        {"address": "127.0.0.1", "port": 11434, "kind": "ollama", "auth": "none_needed", "models": ["llama3", "qwen2"], "process": "ollama"},
    ]}));
    assert!(notice(&app).contains("Found 1 server"), "{}", notice(&app));
    assert!(app.wizard_escape());
    app.wizard_pick(3);
    assert!(!notice(&app).contains("Could not look") && !notice(&app).contains("Looking"), "a result that is there is kept and no second look is made: {}", notice(&app));
    assert_eq!(form(&app)["servers"].as_array().unwrap().len(), 1);
    let (_, body) = app.panel_view().unwrap();
    assert!(body.iter().any(|l| l.contains("11434")), "the servers found are listed: {body:?}");
}

#[test]
fn local_llm_goes_on_to_the_model_and_the_name_and_starts_with_the_servers_request() {
    let mut app = both();
    app.wizard_pick(3);
    app.on_local_scan(0, json!({"ok": 1, "endpoints": [
        {"address": "127.0.0.1", "port": 11434, "kind": "ollama", "auth": "none_needed", "models": ["llama3", "qwen2"], "process": "ollama"},
    ]}));
    app.wizard_next();
    assert_eq!(page(&app), 2);
    assert_eq!(rows(&app), ["Model", "Name", "Back", "Start"]);
    assert_eq!(title(&app), "New session");
    assert_eq!(form(&app)["wizard"]["title"], "New session - step 3 of 3 - Options and start");
    let p = app.ui.panel.as_ref().unwrap();
    let request = request_of(p, p.form.as_ref().unwrap());
    assert_eq!(request["kind"], "local");
    assert!(request["endpoint"].as_str().unwrap().contains("11434"), "{request}");
}

#[test]
fn local_llm_cannot_go_on_with_no_server_and_says_so() {
    let mut app = both();
    app.wizard_pick(3);
    app.wizard_next();
    assert_eq!(page(&app), 1);
    assert!(notice(&app).contains("Not yet"), "{}", notice(&app));
}

#[test]
fn enter_on_the_server_row_with_no_server_looks_again_it_does_not_go_on() {
    let mut app = both();
    app.wizard_pick(3);
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
    app.form_focus(Field::Pick(2)); // the Codex tile (the second is agy)
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
    for _ in 0..4 {
        app.form_cycle(1);
    }
    assert_eq!(form(&app)["focus"], "Pick(0)", "round");
    app.form_cycle(-1);
    assert_eq!(form(&app)["focus"], "Pick(4)");
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
    app.wizard_pick(2);
    assert_eq!((form(&app)["provider"].as_str().unwrap(), form(&app)["account"].as_str().unwrap(), form(&app)["model"].as_str().unwrap()), ("codex", "a3", "default"));
}

#[test]
fn the_first_focus_is_the_icon_of_the_provider_the_form_opened_on_or_else_the_first_one_that_can_be_chosen() {
    assert_eq!(form(&both())["focus"], "Pick(0)");
    assert_eq!(form(&opened_on(false, &[("a3", "codex")], "codex"))["focus"], "Pick(2)", "the provider the form opened on");
    assert_eq!(form(&opened_on(false, &[("a3", "codex")], "claude"))["focus"], "Pick(2)", "the last provider has no account any more: the first one that has");
    assert_eq!(form(&opened_on(false, &[], "claude"))["focus"], "Pick(3)", "with no account at all only Local LLM can be chosen, and it has the focus: not agy, which is no provider of a session");
    let mut app = both();
    app.wizard_pick(2);
    assert!(app.wizard_escape());
    assert_eq!(form(&app)["focus"], "Pick(2)", "back from a step the icon that was chosen has the focus");
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
    assert_eq!(tiles.iter().map(|t| t["provider"].as_str().unwrap()).collect::<Vec<_>>(), ["claude", "agy", "codex", "local", "terminal"]);
    assert_eq!(tiles.iter().map(|t| t["word"].as_str().unwrap()).collect::<Vec<_>>(), ["Claude", "agy", "Codex", "Local LLM", "Terminal"]);
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

// ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- the terminal icon (`M23-P03`, `M23-P03a`)

#[test]
fn the_terminal_icon_needs_no_account_and_has_the_fifth_place() {
    let app = opened(false, &[]);
    assert_eq!(rows(&app), ["Pick(0)", "Pick(1)", "Pick(2)", "Pick(3)", "Pick(4)"]);
    let tiles = app.wizard_tiles().unwrap();
    assert_eq!(tiles.iter().map(|t| (t.provider, t.word, t.enabled)).collect::<Vec<_>>(), [("claude", "Claude", false), ("agy", "agy", false), ("codex", "Codex", false), ("local", "Local LLM", true), ("terminal", "Terminal", true)]);
    let said = app.panel_view().unwrap().1.join("\n");
    assert!(!said.contains("Terminal needs") && !said.contains("and Terminal"), "a terminal is never said to need an account: {said}");
}

#[test]
fn choosing_the_terminal_opens_a_shell_at_once_with_no_step_and_no_account() {
    let mut app = opened(false, &[]);
    app.wizard_pick(4);
    assert!(app.ui.panel.is_none(), "the panel is gone: the Terminal icon has no step after it");
    assert!(app.ui.flash.as_deref().is_some_and(|f| f.contains("terminal refused")), "(no host answers here, and the window says so): {:?}", app.ui.flash);
    let mut with_accounts = both();
    with_accounts.wizard_pick(4);
    assert!(with_accounts.ui.panel.is_none(), "the same with accounts: nothing is asked");
}

// ---- M23-P04c: the agy picture ----

#[test]
fn the_agy_picture_is_second_and_grey_with_its_own_reason_until_agy_is_found_and_asks_for_no_account() {
    let mut app = opened(false, &[]);
    let agy = |app: &App| app.wizard_tiles().unwrap().into_iter().find(|t| t.provider == "agy").map(|t| (t.word, t.enabled)).unwrap();
    assert_eq!(agy(&app), ("agy", false), "the unit tests start with none found");
    let said = app.panel_view().unwrap().1.join("\n");
    assert!(said.contains("agy is not installed on this machine"), "{said}");
    assert!(said.contains("Claude and Codex need an account first") && !said.contains("agy need") && !said.contains("and agy") && !said.contains("agy and"), "agy is no matter of an account: {said}");
    app.wizard_pick(1);
    assert_eq!(page(&app), 0, "a grey picture is not taken");
    assert_eq!(form(&app)["focus"], "Pick(1)");
    assert!(notice(&app).contains("agy is not installed on this machine") && !notice(&app).contains("account"), "{}", notice(&app));
    assert!(app.panel_view().unwrap().1.join("\n").contains("agy: not installed"), "the word line of the picture that has the focus");
    app.ui.panel.as_mut().unwrap().agy = true;
    app.ui.panel.as_mut().unwrap().notice.clear(); // (the sentence of the click above would still be there)
    assert_eq!(agy(&app), ("agy", true), "found: it needs no account of Lico's");
    let said = app.panel_view().unwrap().1.join("\n");
    assert!(!said.contains("not installed") && said.contains("Claude and Codex need an account first"), "{said}");
}

#[test]
fn choosing_a_found_agy_asks_the_host_for_a_session_at_once_and_a_refusal_leaves_the_person_on_the_icons_with_the_reason() {
    let mut app = opened(false, &[]);
    app.ui.panel.as_mut().unwrap().agy = true;
    app.wizard_pick(1);
    assert_eq!(page(&app), 0, "no step after it: the start was asked at once, and with no host to answer it stays on the icons");
    assert!(notice(&app).starts_with("The session was not started."), "(no host answers here, and the window says so): {}", notice(&app));
    assert!(app.ui.flash.is_none() && app.ui.terminal.tabs.is_empty(), "it is no terminal: nothing was opened");
}

#[test]
fn another_icon_is_still_a_step_and_the_focus_returns_to_it_on_back() {
    let mut app = both();
    app.wizard_pick(0);
    assert_eq!(page(&app), 1);
    assert_eq!(rows(&app).first().map(String::as_str), Some("Account"), "Claude: its own rows");
    assert_eq!(title(&app), "New session");
    assert_eq!(form(&app)["wizard"]["title"], "New session - step 2 of 3 - Account and folder");
    assert!(app.wizard_escape());
    assert_eq!((page(&app), form(&app)["focus"].as_str().unwrap()), (0, "Pick(0)"), "the focus is on the icon that was chosen");
}

#[test]
fn left_and_right_go_round_the_five_icons() {
    let mut app = both();
    app.form_focus(Field::Pick(4));
    app.form_cycle(1);
    assert_eq!(form(&app)["focus"], "Pick(0)", "after the fifth, the first");
    app.form_cycle(-1);
    assert_eq!(form(&app)["focus"], "Pick(4)", "and before the first, the fifth");
}

// ---- M22-P07: the choices of steps two and three as chips and cards painted over the rows ----

fn chosen_of(row: &CardRow) -> Vec<&str> {
    row.items.iter().filter(|i| i.chosen).map(|i| i.text.as_str()).collect()
}

fn texts_of(row: &CardRow) -> Vec<&str> {
    row.items.iter().map(|i| i.text.as_str()).collect()
}

fn one_page() -> App {
    let mut one = app(1.0, false);
    one.form_open();
    one
}

fn on_step_three(provider: &str) -> App {
    let mut app = opened_on(false, &[("a1", "claude"), ("a3", "codex")], provider);
    app.with_form(|_, f| (f.models, f.efforts) = (vec!["opus".into(), "sonnet".into()], vec!["low".into(), "high".into()]));
    app.wizard_pick(PICKS.iter().position(|k| *k == provider).unwrap());
    app.ed.put("/work/one");
    app.wizard_next();
    assert_eq!(page(&app), 2);
    app
}

#[test]
fn step_one_and_the_one_page_form_have_no_chips_and_no_cards() {
    let app = both();
    assert!(app.wizard_cards().is_empty(), "step one has its own icons");
    assert!(one_page().wizard_cards().is_empty(), "the one-page form stays text");
}

#[test]
fn the_account_is_a_row_of_chips_the_recent_folders_are_cards_and_the_signed_in_row_is_a_card() {
    let mut app = both();
    app.wizard_pick(0);
    let cards = app.wizard_cards();
    let account = cards.iter().find(|r| r.label == "Account").expect("the account row has chips");
    assert!(account.chips && account.line == 0 && account.focused == (form(&app)["focus"] == "Account"));
    assert_eq!(texts_of(account), ["a1 (default)", "+ New account"], "Claude's accounts only, then the way to add one: {account:?}");
    assert_eq!(chosen_of(account), ["a1 (default)"]);
    assert_eq!(account.items[0].icon, Some("claude"), "the picture of the provider");
    let recents: Vec<&CardRow> = cards.iter().filter(|r| !r.chips && r.items.len() == 1 && r.items[0].text.starts_with("/work/")).collect();
    assert_eq!(recents.iter().map(|r| r.items[0].text.as_str()).collect::<Vec<_>>(), ["/work/one", "/work/two"], "one card per recent folder");
    assert_eq!(recents.iter().map(|r| r.line).collect::<Vec<_>>(), [2, 3], "each over the line of its own row");
    assert!(recents.iter().all(|r| !r.items[0].chosen), "no folder is taken yet");
    app.wizard_choose(recents[1].line, 0);
    assert_eq!(app.ui.panel.as_ref().unwrap().form.as_ref().unwrap().folder, "/work/two", "a click on a card does what a click on its line did: the folder is taken");
    let after = app.wizard_cards();
    assert!(after.iter().any(|r| !r.chips && r.items[0].text == "/work/two" && r.items[0].chosen), "and the card of the folder shows it");
}

#[test]
fn a_click_on_the_account_chips_chooses_an_account_or_asks_to_add_one() {
    let mut app = opened(false, &[("a1", "claude"), ("a2", "claude"), ("a3", "codex")]);
    app.wizard_pick(0);
    let line = app.wizard_cards().iter().find(|r| r.label == "Account").unwrap().line;
    assert!(app.wizard_choose(line, 1));
    assert_eq!(app.ui.panel.as_ref().unwrap().account, "a2");
    assert_eq!(chosen_of(app.wizard_cards().iter().find(|r| r.label == "Account").unwrap()), ["a2 (default)"]);
    assert!(app.wizard_choose(line, 2));
    assert_eq!(app.ui.panel.as_ref().unwrap().account, "", "the last chip is `+ New account`: no account is chosen");
    assert_eq!(chosen_of(app.wizard_cards().iter().find(|r| r.label == "Account").unwrap()), ["+ New account"]);
    assert!(!app.wizard_choose(line, 9), "a chip that is not there is nothing");
    assert!(app.ui.panel.as_ref().unwrap().step == Step::NewSession, "choosing `+ New account` the first time only chooses it");
    assert!(app.wizard_choose(line, 2));
    assert!(app.ui.panel.as_ref().unwrap().step == Step::Label, "a second click on the chosen `+ New account` goes to adding an account");
}

#[test]
fn step_three_has_model_effort_and_permission_as_chips_and_start_and_back_as_cards() {
    let mut app = on_step_three("claude");
    let cards = app.wizard_cards();
    let by = |label: &str| cards.iter().find(|r| r.label == label).unwrap_or_else(|| panic!("no chips for {label}: {cards:?}")).clone();
    assert_eq!(texts_of(&by("Model")), ["default", "opus", "sonnet"]);
    assert_eq!(texts_of(&by("Effort")), ["default", "low", "high"]);
    assert_eq!(texts_of(&by("Permission")), ["Skip agent prompts", "Ask for approval"]);
    assert_eq!((chosen_of(&by("Model")), chosen_of(&by("Permission"))), (vec!["default"], vec!["Skip agent prompts"]));
    app.wizard_choose(by("Model").line, 2);
    app.wizard_choose(by("Effort").line, 1);
    app.wizard_choose(by("Permission").line, 1);
    let f = app.ui.panel.as_ref().unwrap();
    assert_eq!((f.form.as_ref().unwrap().model.as_str(), f.form.as_ref().unwrap().effort.as_str(), f.mode), ("sonnet", "low", "default"));
    assert_eq!(form(&app)["focus"], "Permission", "a click on a chip puts the keys on its row");
    let again = app.wizard_cards();
    let chosen = |label: &str| chosen_of(again.iter().find(|r| r.label == label).unwrap()).join(",");
    assert_eq!((chosen("Model"), chosen("Effort"), chosen("Permission")), ("sonnet".into(), "low".into(), "Ask for approval".into()));
    let singles: Vec<&str> = cards.iter().filter(|r| !r.chips).map(|r| r.items[0].text.as_str()).collect();
    assert!(singles.iter().any(|t| t.starts_with("Back")) && singles.iter().any(|t| t.starts_with("Start session")), "{singles:?}");
    assert!(!cards.iter().any(|r| r.label == "Name"), "the name is typed: it stays a text row");
}

#[test]
fn codex_has_only_the_default_model_and_effort_so_its_chips_are_one_each() {
    let app = on_step_three("codex");
    let cards = app.wizard_cards();
    assert_eq!(texts_of(cards.iter().find(|r| r.label == "Model").unwrap()), ["default"], "the program's own choices are Claude's");
    assert_eq!(texts_of(cards.iter().find(|r| r.label == "Effort").unwrap()), ["default"]);
}

#[test]
fn local_llm_has_a_card_per_server_the_chosen_one_filled_and_its_models_as_chips() {
    let mut app = opened(false, &[]);
    app.with_form(|_, f| {
        let server = |addr: &str, models: &[&str]| Server { addr: addr.into(), api: "openai".into(), models: models.iter().map(|m| m.to_string()).collect(), wants_key: false, remembered: false };
        (f.local, f.scanned) = (true, true);
        f.servers = vec![server("127.0.0.1:1111", &["tiny"]), server("127.0.0.1:2222", &["qwen-small", "tiny-llama"])];
    });
    app.wizard_pick(3);
    let first = app.wizard_cards();
    let servers: Vec<&CardRow> = first.iter().filter(|r| r.items.len() == 1 && r.items[0].text.contains("127.0.0.1:")).collect();
    assert_eq!(servers.len(), 2, "one card per server: {first:?}");
    assert_eq!(servers[0].items[0].icon, Some("local"));
    assert_eq!((servers[0].items[0].chosen, servers[1].items[0].chosen), (true, false));
    app.wizard_choose(servers[1].line, 0);
    assert_eq!(app.ui.panel.as_ref().unwrap().form.as_ref().unwrap().server, 1, "a click on a server's card chooses it");
    app.wizard_next();
    assert_eq!(page(&app), 2);
    let cards = app.wizard_cards();
    let model = cards.iter().find(|r| r.label == "Model").expect("the models of the chosen server are chips");
    assert_eq!(texts_of(model), ["qwen-small", "tiny-llama"]);
    assert_eq!(chosen_of(model), ["qwen-small"]);
    app.wizard_choose(model.line, 1);
    assert_eq!(app.ui.panel.as_ref().unwrap().form.as_ref().unwrap().local_model, "tiny-llama");
    assert!(!cards.iter().any(|r| r.label == "Effort" || r.label == "Permission"), "a local session has no effort and no permission");
}

#[test]
fn the_line_of_keys_is_blank_in_the_wizard_and_the_one_page_form_keeps_it() {
    let mut app = both();
    app.wizard_pick(0);
    let (_, body) = app.panel_view().unwrap();
    assert!(!body.iter().any(|l| l.contains("Tab, Up, Down")), "{body:?}");
    assert!(one_page().panel_view().unwrap().1.iter().any(|l| l.contains("Tab, Up, Down")));
}

#[test]
fn next_is_a_card_that_says_why_it_cannot_be_taken_yet_and_a_click_on_it_then_says_so_and_stays() {
    let mut app = both();
    app.wizard_pick(0);
    app.ed.put("relative/folder"); // (the Folder row has the focus: its text is the input line's)
    let next = |app: &App| app.wizard_cards().into_iter().find(|r| r.items[0].text == "Next").unwrap();
    let row = next(&app);
    assert!(!row.items[0].enabled && row.items[0].hint.starts_with("not yet: the folder must be a full path"), "{row:?}");
    assert!(app.wizard_choose(row.line, 0));
    assert_eq!(page(&app), 1, "a click on a card that cannot be taken is what a click on its line was: it says why and stays");
    assert!(notice(&app).starts_with("Not yet:"), "{}", notice(&app));
    app.with_form(|_, f| f.folder = "/work/one".into()); // (the click moved the focus to Next, so the folder is the form's again)
    let ready = next(&app);
    assert!(ready.items[0].enabled && ready.items[0].hint == "Enter", "{ready:?}");
    assert!(app.wizard_choose(ready.line, 0));
    assert_eq!(page(&app), 2);
}

#[test]
fn the_worktree_of_a_git_folder_is_two_chips_and_choosing_the_second_asks_for_a_branch() {
    let mut app = both();
    app.with_form(|_, f| (f.folder, f.checked_folder, f.is_git) = ("/work/one".into(), "/work/one".into(), true));
    app.wizard_pick(0);
    let chips = |app: &App| app.wizard_cards().into_iter().find(|r| r.label == "Worktree").expect("a git folder has the worktree row");
    let row = chips(&app);
    assert_eq!((texts_of(&row), chosen_of(&row)), (vec!["Use the folder", "New worktree"], vec!["Use the folder"]));
    assert!(!rows(&app).contains(&"Branch".to_string()));
    assert!(app.wizard_choose(row.line, 1));
    assert_eq!(chosen_of(&chips(&app)), ["New worktree"]);
    assert!(rows(&app).contains(&"Branch".to_string()), "the branch is typed: a text row appears under the chips");
    assert!(app.wizard_choose(row.line, 0));
    assert!(!app.ui.panel.as_ref().unwrap().form.as_ref().unwrap().worktree);
}

#[test]
fn a_click_beside_the_chips_of_a_row_is_no_choice_and_not_a_click_on_the_hidden_text_either() {
    let mut app = on_step_three("claude");
    let model = app.wizard_cards().into_iter().find(|r| r.label == "Model").unwrap();
    assert!(app.wizard_click(&Action::WizardChoose(model.line, usize::MAX)), "the click is taken by the veil of the row");
    assert_eq!(app.ui.panel.as_ref().unwrap().form.as_ref().unwrap().model, "default", "and it changed nothing: the row's text, which cycled on a click, is hidden");
    assert!(app.wizard_click(&Action::WizardChoose(model.line, 2)));
    assert_eq!(app.ui.panel.as_ref().unwrap().form.as_ref().unwrap().model, "sonnet");
}

#[test]
fn the_wizards_title_stands_alone_and_another_panels_title_has_the_notice_and_the_message_after_a_bar() {
    let mut app = both();
    app.ui.host_notice = Some("Restarted the host (new build abc, was def); resumed 0 sessions".into());
    app.ui.flash = Some("Account added.".into());
    assert_eq!(app.panel_header(title(&app)), "New session", "the wizard says what is made and nothing else");
    app.panel_close();
    app.panel_open(Step::Accounts);
    let shown = app.panel_header(title(&app));
    assert_eq!(shown, "Accounts | Restarted the host (new build abc, was def); resumed 0 sessions | Account added.", "the other panels keep their notices");
    app.ui.host_notice = None;
    assert_eq!(app.panel_header("Accounts".into()), "Accounts | Account added.");
    app.ui.flash = None;
    assert_eq!(app.panel_header("Accounts".into()), "Accounts");
}
