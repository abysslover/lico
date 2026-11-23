//! M24-P02: one touch. A click, Enter or Space on a picture of the new-session wizard starts the session at once when the form's own answers are valid, and shows step two only when they are
//! not; Shift is the old way. The window of these tests has no host (the socket nobody reads), so a start that is made shows as the notice `The session was not started.` on the form: what
//! the tests tell apart is a start that was tried from a step that was shown.

use winit::keyboard::ModifiersState;

use super::super::super::super::scroll_oracle::app;
use super::*;

const TRIED: &str = "The session was not started.";

fn account(id: &str, provider: &str) -> (String, String, String, bool) {
    (id.into(), provider.into(), id.into(), true)
}

/// The wizard on its first step, with these accounts, as the form's own tests build it.
fn wizard(home: bool, accounts: &[(&str, &str)]) -> App {
    let mut app = app(1.0, false);
    app.form_open();
    let p = app.ui.panel.as_mut().unwrap();
    p.accounts = accounts.iter().map(|(id, kind)| account(id, kind)).collect();
    p.provider = "claude".into();
    p.account = p.first_account();
    p.notice.clear();
    app.wizard_begin(home);
    app
}

fn both() -> App {
    wizard(false, &[("a1", "claude"), ("a3", "codex")])
}

fn page(app: &App) -> u64 {
    app.form_json()["wizard"]["page"].as_u64().unwrap()
}

fn notice(app: &App) -> String {
    app.ui.panel.as_ref().unwrap().notice.clone()
}

fn folder(app: &App) -> String {
    app.ui.panel.as_ref().unwrap().form.as_ref().unwrap().folder.clone()
}

fn tried(app: &App) -> bool {
    notice(app).starts_with(TRIED)
}

/// A folder that is there, for the left side to list.
fn real_folder(name: &str) -> String {
    let dir = std::env::temp_dir().join(format!("lico_touch_{name}_{}", crate::core::random_hex(6)));
    std::fs::create_dir_all(&dir).unwrap();
    dir.to_string_lossy().into_owned()
}

#[test]
fn a_plain_click_on_claude_or_codex_starts_the_session_at_once_and_does_not_show_step_two() {
    for (pick, kind) in [(0usize, "claude"), (2, "codex")] {
        let mut app = both();
        app.wizard_touch(pick);
        assert!(tried(&app), "{kind}: a start was made: {}", notice(&app));
        assert_eq!(app.ui.panel.as_ref().unwrap().provider, kind);
    }
}

#[test]
fn a_start_that_the_host_refuses_leaves_the_person_on_step_two_with_the_reason() {
    let mut app = both();
    app.wizard_touch(0);
    assert_eq!(page(&app), 1, "the rows are shown so that what the host refused can be changed");
    assert!(notice(&app).starts_with(TRIED), "{}", notice(&app));
}

#[test]
fn the_folder_on_the_left_is_the_folder_of_a_session_that_is_started() {
    let dir = real_folder("left");
    let mut app = both();
    app.ui.sidebar.cwd = dir.clone();
    app.ui.sidebar.ssh_host = None;
    app.ui.sidebar.unreachable = false;
    app.wizard_touch(0);
    assert_eq!(folder(&app), dir, "the left decides");
    assert!(tried(&app));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_left_side_that_lists_nothing_of_this_machine_leaves_the_forms_own_folder() {
    let mut app = both();
    app.with_form(|_, f| f.folder = "/kept/by/the/form".into());
    app.ui.sidebar.cwd = real_folder("ssh");
    app.ui.sidebar.ssh_host = Some("far".into());
    app.wizard_touch(0);
    assert_eq!(folder(&app), "/kept/by/the/form", "an ssh listing is no folder of this machine");
    let _ = std::fs::remove_dir_all(&app.ui.sidebar.cwd);
}

#[test]
fn a_home_session_has_no_folder_and_is_started_at_once_too() {
    let dir = real_folder("home");
    let mut app = wizard(true, &[("a1", "claude")]);
    app.ui.sidebar.cwd = dir.clone();
    app.wizard_touch(0);
    assert_eq!(folder(&app), "", "a Home session gets a scratch workspace of its own");
    assert!(tried(&app));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_provider_with_no_account_stays_grey_and_nothing_is_started() {
    let mut app = wizard(false, &[]);
    app.wizard_touch(0);
    assert_eq!((page(&app), tried(&app)), (0, false));
    assert!(notice(&app).contains("needs an account"), "{}", notice(&app));
}

#[test]
fn local_llm_with_no_server_known_shows_step_two_and_starts_nothing() {
    let mut app = both();
    app.wizard_touch(3);
    assert_eq!(page(&app), 1, "the servers are looked for on the step that says so");
    assert!(!tried(&app) && !notice(&app).starts_with("Not started yet"), "no start was tried and no refusal is shown for what the person has not been asked yet: {}", notice(&app));
}

#[test]
fn an_account_on_an_ssh_host_keeps_step_two() {
    let mut app = wizard(false, &[("far1", "claude")]);
    app.ui.ssh_hosts.insert("far1".into(), "far".into());
    app.wizard_touch(0);
    assert_eq!((page(&app), tried(&app)), (1, false), "a folder of this machine is no folder of that host");
}

#[test]
fn a_form_with_a_problem_shows_step_two_and_starts_nothing() {
    let mut app = both();
    app.with_form(|_, f| f.folder = "not/a/full/path".into());
    app.wizard_touch(0);
    assert_eq!((page(&app), tried(&app)), (1, false));
    assert_eq!(app.form_json()["problem"], "the folder must be a full path that starts with /");
    assert!(!notice(&app).starts_with("Not started yet"), "the step shows the problem in its own place; a start that was not tried leaves no notice: {}", notice(&app));
}

#[test]
fn shift_is_the_old_way_step_two_then_step_three_and_a_start_only_on_start() {
    let mut app = both();
    app.ui.mods = ModifiersState::SHIFT;
    app.wizard_touch(0);
    assert_eq!((page(&app), tried(&app)), (1, false), "step two, nothing started");
}

#[test]
fn enter_space_and_a_click_on_a_picture_are_the_same_touch() {
    let mut by_act = both();
    by_act.with_form(|_, f| f.focus = Field::Pick(0));
    by_act.wizard_act();
    assert!(tried(&by_act), "Enter and Space reach the wizard through its act");
    let mut by_click = both();
    assert!(by_click.wizard_click(&Action::WizardPick(0)));
    assert!(tried(&by_click), "a click reaches it through the action");
}

#[test]
fn the_terminal_and_agy_pictures_are_not_touched_by_this() {
    let mut app = both();
    app.wizard_touch(4); // the terminal opens a shell at once, as before: the wizard is closed
    assert!(app.ui.panel.is_none(), "the terminal picture closes the panel and opens a terminal tab");
}
