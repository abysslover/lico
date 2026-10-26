//! Renaming, closing and deleting without a host: what each asks, that nothing is done before the answer, and that the box and the
//! draft of the session come out the way they went in. (The host's side is in `tests/test_session_lifecycle.py`; the host here does not
//! answer, which is also how a refusal and a lost host are shown.)

use super::super::super::menu::Cmd;
use super::super::super::scroll_oracle::app;
use super::super::super::ui::Row;
use super::*;

const HALF: &str = "HALFTYPEDNAME";

/// The session `session-s0` is open with `my draft` in its box; the host says what `state` and `activity` are.
fn open(state: &str, activity: &str, cwd: &str) -> App {
    let mut app = app(1.0, false);
    app.ui.rows = vec![Row { session: "session-s0".into(), provider: "claude".into(), state: state.into(), activity: activity.into(), generation: 3, cwd: cwd.into(), name: "api".into(), ..Default::default() }];
    app.ui.numbered = vec!["session-s0".into()];
    app.ed.put("my draft");
    app
}

fn step(app: &App) -> Option<Step> {
    app.ui.panel.as_ref().map(|p| p.step)
}

fn notice(app: &App) -> String {
    app.ui.panel.as_ref().map(|p| p.notice.clone()).unwrap_or_default()
}

#[test]
fn the_rename_line_takes_the_box_and_always_gives_the_draft_back_and_a_half_typed_name_is_never_a_draft() {
    let mut app = open("running", "idle", "/work/api");
    app.run_cmd(Cmd::Rename);
    assert!(step(&app) == Some(Step::Rename) && app.text_mode());
    assert_eq!(app.ed.text(), "api", "the line starts with the present name");
    app.ed.put(HALF);
    app.panel_close();
    assert_eq!(app.ed.text(), "my draft");
    // leaving the view with the line open, and opening another panel from it
    app.run_cmd(Cmd::Rename);
    app.ed.put(HALF);
    app.hide();
    assert!(app.ui.panel.is_none() && app.session.is_empty() && app.ed.text() == "");
    app.ed.resume("session-s0");
    assert_eq!(app.ed.text(), "my draft", "{HALF} is not a draft");
    app.session = "session-s0".into();
    app.run_cmd(Cmd::Rename);
    app.run_cmd(Cmd::NewSession);
    app.panel_close();
    assert_eq!(app.ed.text(), "my draft");
    for shown in [app.editor_json().to_string(), app.shown_input()] {
        assert!(!shown.contains(HALF), "{shown}");
    }
}

#[test]
fn a_name_the_host_would_refuse_stays_on_the_line_with_the_reason_and_nothing_is_sent() {
    let mut app = open("running", "idle", "");
    app.run_cmd(Cmd::Rename);
    app.ed.clear();
    app.rename_submit(&"n".repeat(61));
    assert!(notice(&app).contains("at most 60 characters") && app.ed.text().len() == 61, "{}", notice(&app));
    app.ed.clear();
    app.rename_submit("api2"); // a good name, but the host does not answer: it stays to be tried again
    assert!(notice(&app).contains("the host did not answer") && app.ed.text() == "api2" && step(&app) == Some(Step::Rename), "{}", notice(&app));
}

#[test]
fn a_delete_and_the_stopping_of_a_working_turn_ask_first_and_n_changes_nothing() {
    let mut app = open("running", "working", "/work/api");
    app.run_cmd(Cmd::Delete);
    let (title, body) = app.panel_view().unwrap();
    assert!(title == "Delete session" && body.iter().any(|l| l.contains("The folder /work/api is not touched")) && body.iter().any(|l| l.contains("A turn is running")), "{body:?}");
    app.confirm_key('x');
    assert_eq!(step(&app), Some(Step::Confirm), "any other key leaves the question open");
    app.confirm_key('n');
    assert!(app.ui.panel.is_none() && app.ed.text() == "my draft" && app.ui.rows.len() == 1 && app.session == "session-s0");
    app.run_cmd(Cmd::Close); // working: asked about, not done
    assert_eq!(app.panel_view().unwrap().0, "Close session");
    app.panel_close();
    (app.ui.rows[0].activity, app.ui.rows[0].approvals) = ("idle".into(), vec![(7, "run: ls".into())]); // idle but a request waits for a person: closing asks too
    app.run_cmd(Cmd::Close);
    assert_eq!(app.panel_view().unwrap().0, "Close session");
    app.panel_close();
    app.ui.rows[0].cwd.clear(); // a scratch session says that its workspace goes with it
    app.run_cmd(Cmd::Delete);
    assert!(app.panel_view().unwrap().1.iter().any(|l| l.contains("scratch workspace")));
}

#[test]
fn a_yes_the_host_does_not_carry_out_leaves_the_session_the_list_and_the_draft_as_they_were_and_says_so() {
    let mut app = open("running", "working", "/work/api");
    app.run_cmd(Cmd::Delete);
    app.confirm_key('y');
    assert!(step(&app) == Some(Step::Confirm) && notice(&app).contains("Not done: the host did not answer"), "{}", notice(&app));
    assert!(app.ui.rows.len() == 1 && app.ui.numbered.len() == 1 && app.session == "session-s0");
    app.panel_close();
    assert_eq!(app.ed.text(), "my draft");
    app.ui.rows[0].activity = "idle".into(); // an idle session is closed at once, without a question
    app.run_cmd(Cmd::Close);
    assert!(app.ui.panel.is_none() && app.ui.flash.as_deref().is_some_and(|f| f.starts_with("Close session: the host did not answer")), "{:?}", app.ui.flash);
}

#[test]
fn the_menu_says_why_an_action_cannot_be_asked_for_now() {
    let reason = |app: &App, label: &str| app.menu_entries(1).into_iter().find(|e| e.label == label).map(|e| e.reason);
    let mut app = open("disconnected", "", "");
    assert_eq!((reason(&app, "Close session"), reason(&app, "Rename session"), reason(&app, "Delete session")), (Some(Some("the session is already closed")), Some(None), Some(None)));
    app.ui.rows[0].state = "running".into();
    assert_eq!(reason(&app, "Close session"), Some(None));
    app.ui.rows.clear();
    assert_eq!(reason(&app, "Delete session"), Some(Some("no session is open")));
    app.ui.rows.push(Row { session: "session-s0".into(), state: "running".into(), ..Default::default() });
    app.ui.panel = None;
    app.run_cmd(Cmd::Rename);
    assert_eq!(reason(&app, "Close session"), Some(Some("close the open panel first")));
}

#[test]
fn a_second_click_on_the_open_row_soon_after_the_first_asks_for_a_name_and_a_click_on_another_row_does_not() {
    let mut app = open("running", "idle", "");
    app.row_click("session-s0".into());
    assert!(app.ui.panel.is_none());
    app.row_click("session-s0".into());
    assert_eq!(step(&app), Some(Step::Rename));
    app.panel_close();
    app.ui.clicked = Some((std::time::Instant::now() - DOUBLE_CLICK * 2, "session-s0".into()));
    app.row_click("session-s0".into()); // too late: a first click again
    assert!(app.ui.panel.is_none());
}

#[test]
fn a_deleted_session_leaves_the_screen_the_list_the_draft_and_the_bookkeeping_and_another_session_keeps_its_own() {
    let mut app = open("running", "idle", "");
    app.ui.rows.push(Row { session: "other".into(), provider: "claude".into(), state: "running".into(), ..Default::default() });
    app.ui.numbered.push("other".into());
    app.ed.clear();
    app.ed.put("draft of the other");
    app.ed.park("other", "");
    app.ed.put("draft of the deleted"); // the box belongs to the open session again
    app.pending_inputs.insert("gui-1".into(), ("session-s0".into(), "late".into()));
    app.pending_inputs.insert("gui-2".into(), ("other".into(), "kept".into()));
    app.run_cmd(Cmd::Delete);
    app.session_gone("session-s0");
    assert!(app.ui.panel.is_none() && app.session.is_empty() && app.ed.text() == "", "the screen is the start screen");
    assert_eq!((app.ui.rows.len(), app.ui.numbered.len(), app.pending_inputs.len()), (1, 1, 1));
    app.ed.resume("session-s0");
    assert_eq!(app.ed.text(), "", "no draft is left for it");
    app.ed.resume("other");
    assert_eq!(app.ed.text(), "draft of the other");
}
