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
