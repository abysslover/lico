//! The setting `agent_panes` (`M24-P05b`): the person's switch for the `pane` tool. On by default (the owner's answer of 2026-11-24); a file that cannot be trusted turns it **off**, never on.

use super::settings::{agent_panes_enabled, settings_path, status_json, update_settings, Patch};
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

/// A state folder with a `handoff` folder in it, and the settings file's path there.
fn state() -> (PathBuf, PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("lico-agent-panes-{}", crate::core::random_hex(6)));
    let handoff = root.join("handoff");
    std::fs::create_dir_all(&handoff).unwrap();
    let file = settings_path(&handoff);
    (root, handoff, file)
}

fn write(file: &std::path::Path, text: &str, mode: u32) {
    std::fs::write(file, text).unwrap();
    std::fs::set_permissions(file, std::fs::Permissions::from_mode(mode)).unwrap();
}

#[test]
fn the_panes_are_on_without_a_file_without_the_key_and_with_true() {
    let (root, _, file) = state();
    assert!(agent_panes_enabled(&file), "no file at all: on, as the owner chose");
    assert!(agent_panes_enabled(&root.join("no_such_folder").join("settings.json")), "not even the folder: on");
    for text in ["{}", r#"{"clipboard_write": true}"#, r#"{"agent_panes": true}"#] {
        write(&file, text, 0o600);
        assert!(agent_panes_enabled(&file), "{text}");
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_panes_are_off_with_false() {
    let (root, _, file) = state();
    write(&file, r#"{"agent_panes": false, "clipboard_write": true}"#, 0o600);
    assert!(!agent_panes_enabled(&file));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn any_doubt_about_a_file_that_is_there_is_off_because_this_is_the_way_to_stop_it() {
    let (root, _, file) = state();
    for text in [r#"{"agent_panes": "off"}"#, r#"{"agent_panes": 0}"#, r#"{"agent_panes": null}"#, r#"{"agent_panes": "true"}"#, "{not json", "[]", "", r#""on""#] {
        write(&file, text, 0o600);
        assert!(!agent_panes_enabled(&file), "{text:?} must not turn the tool on");
    }
    write(&file, r#"{"agent_panes": true}"#, 0o666);
    assert!(!agent_panes_enabled(&file), "a file the group or others can write is not trusted: off, even with true in it");
    write(&file, r#"{"agent_panes": true}"#, 0o600);
    assert!(agent_panes_enabled(&file), "the same text in an owner-only file is on");
    std::fs::remove_file(&file).unwrap();
    std::fs::create_dir(&file).unwrap();
    assert!(!agent_panes_enabled(&file), "a folder where the file should be is a doubt: off");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_switch_is_set_and_read_back_and_nothing_else_in_the_file_changes() {
    let (root, handoff, file) = state();
    assert_eq!(status_json(&handoff)["agent_panes"], json!(true), "status says on at the start");
    update_settings(&handoff, Patch { clipboard_write: Some(true), notifications: Some(false), ..Default::default() }).unwrap();
    let off = update_settings(&handoff, Patch { agent_panes: Some(false), ..Default::default() }).unwrap();
    assert_eq!(off["settings"]["agent_panes"], json!(false), "the answer of the change says what is now");
    let status = status_json(&handoff);
    assert_eq!((status["agent_panes"].clone(), status["clipboard_write"].clone(), status["notifications"].clone()), (json!(false), json!(true), json!(false)), "the other settings stay");
    assert!(!agent_panes_enabled(&file));
    let on = update_settings(&handoff, Patch { agent_panes: Some(true), ..Default::default() }).unwrap();
    assert_eq!((on["settings"]["agent_panes"].clone(), on["settings"]["clipboard_write"].clone()), (json!(true), json!(true)));
    assert!(agent_panes_enabled(&file));
    assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600, "the file stays owner-only");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_patch_of_the_map_takes_the_switch_and_only_a_boolean() {
    let on = json!({"agent_panes": true});
    let word = json!({"agent_panes": "off"});
    assert_eq!(Patch::of_map(on.as_object().unwrap()).agent_panes, Some(true));
    assert_eq!(Patch::of_map(word.as_object().unwrap()).agent_panes, None, "a word is no switch");
    assert_eq!(Patch::of_map(json!({}).as_object().unwrap()).agent_panes, None);
}

// ---------------------------------------------------------------- the pause (M24-P07)

use super::settings::{agent_panes_state_at, update_settings_at, PanesState, PAUSE_SECS};

const NOW: u64 = 1_800_000_000;

fn state_at(file: &std::path::Path, now: u64) -> PanesState {
    agent_panes_state_at(file, now)
}

fn pause(handoff: &std::path::Path, now: u64) {
    update_settings_at(handoff, Patch { agent_panes_pause: Some(true), ..Default::default() }, now).unwrap();
}

fn real_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()
}

#[test]
fn a_pause_is_five_minutes() {
    assert_eq!(PAUSE_SECS, 300, "the owner said a pause; the lead's default is five minutes, in one place");
}

#[test]
fn a_pause_that_has_not_ended_says_the_seconds_left_and_one_that_has_ended_is_on() {
    let (root, _, file) = state();
    for (until, want) in [
        (NOW + 120, PanesState::Paused { secs_left: 120 }),
        (NOW + PAUSE_SECS, PanesState::Paused { secs_left: PAUSE_SECS }),
        (NOW + 1, PanesState::Paused { secs_left: 1 }),
        (NOW, PanesState::On),     // it ends at its second
        (NOW - 1, PanesState::On),
        (0, PanesState::On),
    ] {
        write(&file, &format!(r#"{{"agent_panes_paused_until": {until}}}"#), 0o600);
        assert_eq!(state_at(&file, NOW), want, "an end at {until}, now {NOW}");
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_switch_the_tool_reads_is_off_while_paused_and_on_again_when_the_pause_has_ended() {
    let (root, _, file) = state();
    write(&file, &format!(r#"{{"agent_panes_paused_until": {}}}"#, real_now() + 100), 0o600);
    assert!(!agent_panes_enabled(&file), "paused: the tool is not offered");
    write(&file, &format!(r#"{{"agent_panes_paused_until": {}}}"#, real_now() - 10), 0o600);
    assert!(agent_panes_enabled(&file), "ended: on again with no one doing anything");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn any_doubt_about_a_pause_is_off_and_never_on() {
    let (root, _, file) = state();
    for until in [r#""soon""#, "-5", "1.5", "null", "true", r#""300""#, "[]", "{}", &format!("{}", NOW + PAUSE_SECS + 1), &format!("{}", NOW + 86_400)] {
        write(&file, &format!(r#"{{"agent_panes_paused_until": {until}}}"#), 0o600);
        assert_eq!(state_at(&file, NOW), PanesState::Off, "an end of {until} is no pause the host wrote: off, so that it never turns the tool on");
    }
    write(&file, &format!(r#"{{"agent_panes_paused_until": {}}}"#, NOW + 60), 0o666);
    assert_eq!(state_at(&file, NOW), PanesState::Off, "a file others can write is not trusted, a pause in it or not");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_permanent_switch_wins_over_a_pause_and_a_pause_does_not_hide_an_on() {
    let (root, _, file) = state();
    write(&file, &format!(r#"{{"agent_panes": false, "agent_panes_paused_until": {}}}"#, NOW + 60), 0o600);
    assert_eq!(state_at(&file, NOW), PanesState::Off, "off for good is stronger than a pause");
    write(&file, &format!(r#"{{"agent_panes": true, "agent_panes_paused_until": {}}}"#, NOW + 60), 0o600);
    assert_eq!(state_at(&file, NOW), PanesState::Paused { secs_left: 60 });
    write(&file, r#"{"agent_panes": true}"#, 0o600);
    assert_eq!(state_at(&file, NOW), PanesState::On);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_pause_is_written_with_its_end_and_nothing_else_in_the_file_changes() {
    let (root, handoff, file) = state();
    update_settings(&handoff, Patch { clipboard_write: Some(true), notifications: Some(false), ..Default::default() }).unwrap();
    pause(&handoff, NOW);
    assert_eq!(state_at(&file, NOW), PanesState::Paused { secs_left: PAUSE_SECS });
    assert_eq!(state_at(&file, NOW + 100), PanesState::Paused { secs_left: PAUSE_SECS - 100 });
    assert_eq!(state_at(&file, NOW + PAUSE_SECS), PanesState::On, "it ends by itself");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains(&format!("\"agent_panes_paused_until\": {}", NOW + PAUSE_SECS)) || text.contains(&format!("\"agent_panes_paused_until\":{}", NOW + PAUSE_SECS)), "{text}");
    assert!(text.contains("\"clipboard_write\"") && text.contains("\"notifications\""), "the other settings stay: {text}");
    assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600, "the file stays owner-only");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_stop_key_pressed_again_starts_the_five_minutes_anew_and_never_ends_the_pause() {
    let (root, handoff, file) = state();
    pause(&handoff, NOW);
    pause(&handoff, NOW + 200);
    assert_eq!(state_at(&file, NOW + 200), PanesState::Paused { secs_left: PAUSE_SECS }, "a second press is a stop again, not a resume");
    assert_eq!(state_at(&file, NOW + 400), PanesState::Paused { secs_left: PAUSE_SECS - 200 }, "and the first end no longer counts");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_write_of_the_switch_ends_a_pause_and_so_does_a_resume_and_a_resume_never_turns_off_for_good_into_on() {
    let (root, handoff, file) = state();
    pause(&handoff, NOW);
    update_settings_at(&handoff, Patch { agent_panes: Some(true), ..Default::default() }, NOW).unwrap();
    assert_eq!(state_at(&file, NOW), PanesState::On, "on means on, at once");
    assert!(!std::fs::read_to_string(&file).unwrap().contains("paused_until"), "the end is gone from the file, not only in the past");
    pause(&handoff, NOW);
    update_settings_at(&handoff, Patch { agent_panes: Some(false), ..Default::default() }, NOW).unwrap();
    assert_eq!(state_at(&file, NOW), PanesState::Off, "off for good is off, and the pause does not come back when it is switched on later");
    update_settings_at(&handoff, Patch { agent_panes: Some(true), ..Default::default() }, NOW).unwrap();
    assert_eq!(state_at(&file, NOW + 1), PanesState::On);
    pause(&handoff, NOW);
    update_settings_at(&handoff, Patch { agent_panes_pause: Some(false), ..Default::default() }, NOW).unwrap();
    assert_eq!(state_at(&file, NOW), PanesState::On, "a resume ends the pause");
    update_settings_at(&handoff, Patch { agent_panes: Some(false), ..Default::default() }, NOW).unwrap();
    update_settings_at(&handoff, Patch { agent_panes_pause: Some(false), ..Default::default() }, NOW).unwrap();
    assert_eq!(state_at(&file, NOW), PanesState::Off, "a resume of a pause does not switch on what the person switched off for good");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_pause_over_the_permanent_off_changes_nothing_that_is_shown() {
    let (root, handoff, file) = state();
    update_settings_at(&handoff, Patch { agent_panes: Some(false), ..Default::default() }, NOW).unwrap();
    pause(&handoff, NOW);
    assert_eq!(state_at(&file, NOW), PanesState::Off);
    assert!(!std::fs::read_to_string(&file).unwrap().contains("paused_until"), "a pause asked while it is off for good is not written: it would come back as a pause when the person switches it on");
    update_settings_at(&handoff, Patch { agent_panes: Some(true), ..Default::default() }, NOW).unwrap();
    assert_eq!(state_at(&file, NOW), PanesState::On, "the pause that was asked while off did not wait in the file");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn status_says_the_seconds_left_only_while_paused_and_the_switch_is_false_then() {
    let (root, handoff, _) = state();
    let plain = status_json(&handoff);
    assert!(plain.get("agent_panes_paused").is_none(), "no pause, no field: {plain}");
    assert_eq!(plain["agent_panes"], json!(true));
    let answer = update_settings(&handoff, Patch { agent_panes_pause: Some(true), ..Default::default() }).unwrap();
    for status in [status_json(&handoff), answer["settings"].clone()] {
        let left = status["agent_panes_paused"].as_u64().expect("the seconds left");
        assert!((PAUSE_SECS - 5..=PAUSE_SECS).contains(&left), "{status}");
        assert_eq!(status["agent_panes"], json!(false), "an older window reads: not allowed");
    }
    update_settings(&handoff, Patch { agent_panes: Some(true), ..Default::default() }).unwrap();
    let after = status_json(&handoff);
    assert!(after.get("agent_panes_paused").is_none() && after["agent_panes"] == json!(true), "{after}");
    update_settings(&handoff, Patch { agent_panes: Some(false), ..Default::default() }).unwrap();
    assert!(status_json(&handoff).get("agent_panes_paused").is_none(), "off for good is not a pause");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_patch_of_the_map_takes_the_pause_and_only_a_boolean() {
    let yes = json!({"agent_panes_pause": true});
    let no = json!({"agent_panes_pause": false});
    let word = json!({"agent_panes_pause": "now"});
    assert_eq!(Patch::of_map(yes.as_object().unwrap()).agent_panes_pause, Some(true));
    assert_eq!(Patch::of_map(no.as_object().unwrap()).agent_panes_pause, Some(false));
    assert_eq!(Patch::of_map(word.as_object().unwrap()).agent_panes_pause, None, "a word is no pause");
}
