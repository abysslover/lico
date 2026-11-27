//! The sign-in screen as pictures (`M25-P13`): which steps it shows for each state the host reports and for each provider, where the parts are, and that the text lines stay the model.

use serde_json::json;

use super::super::super::super::scroll_oracle::app;
use super::*;

const CODE: &str = "CANARYCODE9";

fn view(provider: &str, state: &str, url: Option<&str>, needs_code: bool, stars: usize) -> LoginView {
    login_view(provider, state, url, needs_code, stars)
}

fn progress(v: &LoginView) -> Vec<Progress> {
    v.steps.iter().map(|s| s.progress).collect()
}

const URL: &str = "https://example.test/oauth";

#[test]
fn a_sign_in_that_has_no_address_yet_has_the_first_step_to_do_and_nothing_to_paste_into_yet() {
    for provider in ["claude", "agy"] {
        let v = view(provider, "waiting", None, false, 0);
        assert_eq!(progress(&v), [Progress::Active, Progress::Todo, Progress::Todo], "{provider}");
        assert_eq!(v.steps.iter().map(|s| s.word).collect::<Vec<_>>(), ["Open", "Sign in", "Code"]);
        assert_eq!((v.address.clone(), v.retry, v.device), (None, false, false));
        assert_eq!(v.field, Some(LoginField { active: false, stars: 0 }), "the box is there, and waits");
    }
}

#[test]
fn once_the_address_is_printed_the_page_is_open_and_the_person_signs_in() {
    let v = view("claude", "browser", Some(URL), false, 0);
    assert_eq!(progress(&v), [Progress::Done, Progress::Active, Progress::Todo]);
    assert_eq!(v.address.as_deref(), Some(URL));
    assert_eq!(v.field, Some(LoginField { active: false, stars: 0 }));
}

#[test]
fn when_the_program_asks_for_the_code_the_box_takes_input_and_shows_one_star_for_each_character() {
    let v = view("agy", "code", Some(URL), true, 7);
    assert_eq!(progress(&v), [Progress::Done, Progress::Done, Progress::Active]);
    assert_eq!(v.field, Some(LoginField { active: true, stars: 7 }));
    assert_eq!(view("agy", "code", Some(URL), true, 0).field, Some(LoginField { active: true, stars: 0 }));
}

#[test]
fn stars_are_only_for_a_box_that_takes_input() {
    let v = view("claude", "browser", Some(URL), false, 9);
    assert_eq!(v.field, Some(LoginField { active: false, stars: 0 }));
}

#[test]
fn after_the_code_was_sent_every_step_is_done_while_the_program_checks_it() {
    let v = view("claude", "waiting", Some(URL), false, 0);
    assert_eq!(progress(&v), [Progress::Done, Progress::Done, Progress::Done]);
    assert_eq!(v.field, Some(LoginField { active: false, stars: 0 }));
    assert!(!v.retry);
}

#[test]
fn a_sign_in_that_worked_has_every_step_done_and_no_address_nor_box() {
    for provider in ["claude", "agy", "codex"] {
        let v = view(provider, "succeeded", Some(URL), false, 0);
        assert!(progress(&v).iter().all(|p| *p == Progress::Done), "{provider}");
        assert_eq!((v.address, v.field, v.retry, v.device), (None, None, false, false), "{provider}");
    }
}

#[test]
fn a_sign_in_that_ended_without_success_shows_a_cross_on_the_first_picture_and_a_picture_to_try_again() {
    for state in ["failed", "timed_out", "cancelled"] {
        let v = view("agy", state, Some(URL), false, 3);
        assert_eq!(progress(&v), [Progress::Failed, Progress::Todo, Progress::Todo], "{state}");
        assert_eq!((v.address, v.field, v.retry), (None, None, true), "{state}");
        assert!(v.note.is_some_and(|n| n.contains("Again")), "a sentence says what happened and what the picture is for: {state}");
    }
}

#[test]
fn codex_has_two_steps_and_a_second_way_until_the_program_asks_for_a_code() {
    let v = view("codex", "browser", Some(URL), false, 0);
    assert_eq!(v.steps.iter().map(|s| s.word).collect::<Vec<_>>(), ["Open", "Sign in"]);
    assert_eq!(progress(&v), [Progress::Done, Progress::Active]);
    assert_eq!((v.field.clone(), v.device), (None, true));
    let asked = view("codex", "code", Some(URL), true, 2);
    assert_eq!(asked.steps.len(), 3);
    assert_eq!(asked.field, Some(LoginField { active: true, stars: 2 }));
    assert!(asked.device, "the key D still works while a code is asked");
    let done = view("codex", "succeeded", None, false, 0);
    assert!(!done.device);
}

#[test]
fn the_pictures_are_a_tick_the_ring_and_grey_by_what_is_done_active_and_to_do() {
    assert_eq!(Progress::Todo.name(), "todo");
    assert_eq!(Progress::Active.name(), "active");
    assert_eq!(Progress::Done.name(), "done");
    assert_eq!(Progress::Failed.name(), "failed");
}

const BAND: [f32; 4] = [180.0, 100.0, 980.0, 100.0 + 11.0 * 20.0];

fn inside(r: &[f32; 4], band: &[f32; 4]) -> bool {
    r[0] >= band[0] && r[1] >= band[1] && r[2] <= band[2] && r[3] <= band[3] && r[0] < r[2] && r[1] < r[3]
}

fn apart(a: &[f32; 4], b: &[f32; 4]) -> bool {
    a[2] <= b[0] || b[2] <= a[0] || a[3] <= b[1] || b[3] <= a[1]
}

#[test]
fn the_screen_has_the_pictures_then_the_address_with_two_buttons_then_the_box_with_two_buttons() {
    let v = view("agy", "code", Some(URL), true, 3);
    let l = layout(BAND, 1.0, 20.0, 8.0, &v).expect("the band holds it");
    assert_eq!(l.tiles.len(), 3);
    let (a, o, c, f, p, s) = (l.address.unwrap(), l.open.unwrap(), l.copy.unwrap(), l.field.unwrap(), l.paste.unwrap(), l.send.unwrap());
    for r in l.tiles.iter().chain([&a, &o, &c, &f, &p, &s]) {
        assert!(inside(r, &BAND), "{r:?}");
    }
    let tiles_bottom = l.tiles.iter().map(|t| t[3]).fold(0.0, f32::max);
    assert!(a[1] >= tiles_bottom && f[1] >= a[3], "pictures, then the address, then the box: {tiles_bottom} {a:?} {f:?}");
    assert_eq!((o[0], c[0]), (p[0], s[0]), "the buttons of the two rows are in the same two columns");
    assert!(a[2] <= o[0] && f[2] <= p[0] && o[2] <= c[0], "the box ends where its buttons begin");
    assert!(apart(&o, &c) && apart(&p, &s) && apart(&a, &f), "nothing overlaps");
    assert_eq!((a[0], f[0]), (BAND[0], BAND[0]));
}

#[test]
fn the_rows_are_no_wider_than_sixty_four_characters_in_a_wide_window() {
    let wide = [180.0, 100.0, 4000.0, 320.0];
    let v = view("claude", "code", Some(URL), true, 1);
    let l = layout(wide, 1.0, 20.0, 8.0, &v).unwrap();
    assert!(l.copy.unwrap()[2] <= wide[0] + 64.0 * 8.0, "{:?}", l.copy);
}

#[test]
fn a_sign_in_that_ended_has_the_pictures_and_one_more_to_try_again_and_nothing_else() {
    let v = view("claude", "failed", None, false, 0);
    let l = layout(BAND, 1.0, 20.0, 8.0, &v).unwrap();
    assert_eq!(l.tiles.len(), 4, "three steps and the picture to try again");
    assert!(l.address.is_none() && l.field.is_none() && l.open.is_none() && l.copy.is_none() && l.paste.is_none() && l.send.is_none());
    let tiles_bottom = l.tiles.iter().map(|t| t[3]).fold(0.0, f32::max);
    assert!(l.hint.is_some_and(|h| h[1] >= tiles_bottom && inside(&h, &BAND)), "the sentence is under the pictures: {:?}", l.hint);
    let ok = layout(BAND, 1.0, 20.0, 8.0, &view("claude", "succeeded", None, false, 0)).unwrap();
    assert!(ok.hint.is_none(), "nothing is said under the pictures of a sign-in that worked");
}

#[test]
fn codex_has_a_sentence_where_the_box_would_be() {
    let v = view("codex", "browser", Some(URL), false, 0);
    let l = layout(BAND, 1.0, 20.0, 8.0, &v).unwrap();
    assert!(l.field.is_none() && l.paste.is_none() && l.send.is_none());
    let boxed = layout(BAND, 1.0, 20.0, 8.0, &view("agy", "code", Some(URL), true, 1)).unwrap();
    assert_eq!(l.hint.unwrap()[1], boxed.field.unwrap()[1], "in the place of the box");
}

#[test]
fn a_window_too_small_for_the_screen_shows_the_text_and_no_pictures() {
    let v = view("agy", "code", Some(URL), true, 1);
    assert!(layout([180.0, 100.0, 980.0, 140.0], 1.0, 20.0, 8.0, &v).is_none(), "no room for the pictures");
    assert!(layout([180.0, 100.0, 980.0, 100.0 + 5.0 * 20.0 + 30.0], 1.0, 20.0, 8.0, &v).is_none(), "room for the pictures, not for the rows under them");
    assert!(layout([180.0, 100.0, 230.0, 320.0], 1.0, 20.0, 8.0, &v).is_none(), "a window too narrow");
}

fn login_app(provider: &str, login: serde_json::Value) -> App {
    let mut app = app(1.0, false);
    app.panel_open(Step::Login);
    let p = app.ui.panel.as_mut().unwrap();
    p.provider = provider.into();
    p.login = login;
    app
}

#[test]
fn the_text_lines_stay_the_model_and_the_body_has_the_room_for_the_pictures() {
    let app = login_app("agy", json!({"state": "code", "url": URL, "needs_code": true}));
    let (title, body) = app.login_body(app.ui.panel.as_ref().unwrap());
    assert!(title.starts_with("Sign in: "), "{title}");
    assert_eq!(body.len(), LOGIN_LINES);
    assert_eq!(body[0], "state: code");
    assert!(body.iter().any(|l| l == URL), "the address is a line of text too");
    assert!(body.iter().any(|l| l.starts_with("The provider asks for a code: paste it below")), "{body:?}");
    assert!(body.iter().skip(5).all(|l| l == " "), "blank lines (an empty line has no height) fill the rest: {body:?}");
}

#[test]
fn codex_text_names_the_second_way_and_the_other_providers_do_not() {
    let codex = login_app("codex", json!({"state": "browser", "url": URL}));
    assert!(codex.login_body(codex.ui.panel.as_ref().unwrap()).1.iter().any(|l| l.starts_with("D  use the device-code sign-in")));
    let agy = login_app("agy", json!({"state": "browser", "url": URL}));
    assert!(!agy.login_body(agy.ui.panel.as_ref().unwrap()).1.iter().any(|l| l.starts_with("D  use")));
}

#[test]
fn the_screen_is_only_on_the_sign_in() {
    let mut app = app(1.0, false);
    assert!(app.login_view_now().is_none() && app.login_json().is_null());
    app.panel_open(Step::Accounts);
    assert!(app.login_view_now().is_none() && app.login_json().is_null());
    assert!(!app.login_pictured());
}

#[test]
fn the_dump_says_the_steps_and_the_length_of_the_code_and_never_the_code() {
    let mut app = login_app("claude", json!({"state": "code", "url": URL, "needs_code": true}));
    assert!(app.code_entry() && app.ed.put(CODE));
    let j = app.login_json();
    assert_eq!(j["field"]["stars"], CODE.len());
    assert_eq!(j["field"]["active"], true);
    assert_eq!(j["address"], URL);
    assert_eq!(j["steps"].as_array().unwrap().iter().map(|s| s["progress"].as_str().unwrap()).collect::<Vec<_>>(), ["done", "done", "active"]);
    assert_eq!(j["steps"][0]["icon"], "browser");
    assert_eq!(j["pictured"], false, "nothing was painted in this test");
    assert!(!j.to_string().contains(CODE) && !app.panel_json().to_string().contains(CODE));
}

#[test]
fn what_is_typed_into_the_box_is_counted_in_characters_and_at_most_sixty_four() {
    let mut app = login_app("agy", json!({"state": "code", "url": URL, "needs_code": true}));
    assert!(app.ed.put(&"é".repeat(100)));
    assert_eq!(app.login_view_now().unwrap().field.unwrap().stars, 64);
    app.ed.take();
    assert!(app.ed.put("한국어"));
    assert_eq!(app.login_view_now().unwrap().field.unwrap().stars, 3, "characters, not bytes");
}

#[test]
fn the_line_at_the_bottom_says_nothing_while_the_painted_box_is_where_the_code_goes() {
    let mut app = login_app("agy", json!({"state": "code", "url": URL, "needs_code": true}));
    assert!(app.ed.put(CODE));
    let plain = app.prompt_view(80, 30);
    assert_eq!(plain.rows, vec![format!("> {}", "*".repeat(CODE.len()))], "without the pictures the stars are in the line at the bottom");
    app.ui.toolbar_geom.login = vec![("tiles", [0.0, 0.0, 10.0, 10.0])]; // the pictures were painted
    assert!(app.login_pictured());
    let pictured = app.prompt_view(80, 30);
    assert_eq!(pictured.rows, vec![" ".to_string()]);
    assert!(!format!("{:?}", pictured.rows).contains(CODE));
}

#[test]
fn a_click_on_the_picture_to_try_again_or_a_button_that_has_no_use_changes_nothing_in_the_text() {
    let mut app = login_app("agy", json!({"state": "browser", "url": URL}));
    let before = app.ui.panel.as_ref().unwrap().login.clone();
    app.dirty = false;
    app.login_act('x'); // not a key of the screen
    assert_eq!(app.ui.panel.as_ref().unwrap().login, before);
    assert!(app.dirty, "the window draws again");
}
