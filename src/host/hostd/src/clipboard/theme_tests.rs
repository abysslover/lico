//! The setting `theme` (`M24-P11d`): the window's look, `dark` (the look it has always had, and the default), `light`, or `system` (the desktop's). The host only keeps the word; the window draws.

use super::settings::{settings_path, status_json, theme_value, update_settings, Patch};
use serde_json::{json, Map, Value};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

/// A state folder with a `handoff` folder in it, and the settings file's path there.
fn state() -> (PathBuf, PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("lico-theme-{}", crate::core::random_hex(6)));
    let handoff = root.join("handoff");
    std::fs::create_dir_all(&handoff).unwrap();
    let file = settings_path(&handoff);
    (root, handoff, file)
}

fn write(file: &std::path::Path, text: &str, mode: u32) {
    std::fs::write(file, text).unwrap();
    std::fs::set_permissions(file, std::fs::Permissions::from_mode(mode)).unwrap();
}

fn map(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

#[test]
fn the_theme_is_dark_without_a_file_without_the_key_and_with_any_word_that_is_not_one_of_three() {
    let (root, _, file) = state();
    assert_eq!(theme_value(&file), "dark", "no file at all");
    assert_eq!(theme_value(&root.join("no_such_folder").join("settings.json")), "dark", "not even the folder");
    for text in ["{}", r#"{"clipboard_write": true}"#, r#"{"theme": "dark"}"#, r#"{"theme": "Light"}"#, r#"{"theme": "blue"}"#, r#"{"theme": ""}"#, r#"{"theme": 3}"#, r#"{"theme": null}"#, "[1]", "not json"] {
        write(&file, text, 0o600);
        assert_eq!(theme_value(&file), "dark", "{text}");
    }
    for word in ["light", "system"] {
        write(&file, &format!(r#"{{"theme": "{word}"}}"#), 0o600);
        assert_eq!(theme_value(&file), word);
    }
    write(&file, r#"{"theme": "light"}"#, 0o666);
    assert_eq!(theme_value(&file), "dark", "a file that others can write says nothing (as for every setting)");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn status_says_the_theme_only_while_it_is_not_dark_so_that_an_older_window_reads_what_it_always_read() {
    let (root, handoff, file) = state();
    let plain = status_json(&handoff);
    assert!(plain.get("theme").is_none(), "dark is the default and has no field: {plain}");
    assert_eq!(plain, json!({"clipboard_write": false, "agent_panes": true, "notifications": true, "group_by": "account", "font_px": 14}));
    write(&file, r#"{"theme": "light"}"#, 0o600);
    assert_eq!(status_json(&handoff)["theme"], json!("light"));
    write(&file, r#"{"theme": "system"}"#, 0o600);
    assert_eq!(status_json(&handoff)["theme"], json!("system"));
    write(&file, r#"{"theme": "dark"}"#, 0o600);
    assert!(status_json(&handoff).get("theme").is_none(), "a stored dark is no field either");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_patch_takes_one_of_three_words_and_answers_with_the_settings() {
    let (root, handoff, file) = state();
    for word in ["light", "system"] {
        let answer = update_settings(&handoff, Patch { theme: Some(word), ..Default::default() }).unwrap();
        assert_eq!(answer["settings"]["theme"], json!(word), "the answer carries it");
        assert_eq!(theme_value(&file), word, "and the file keeps it");
        assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
    }
    for word in ["Light", "LIGHT", "", "auto", "night", "dark ", "0"] {
        assert_eq!(update_settings(&handoff, Patch { theme: Some(word), ..Default::default() }), Err("bad_request"), "{word:?}");
    }
    assert_eq!(theme_value(&file), "system", "a refused word changes nothing");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn dark_takes_the_key_out_and_every_other_key_stays() {
    let (root, handoff, file) = state();
    write(&file, r#"{"notifications": false, "font_px": 17}"#, 0o600);
    update_settings(&handoff, Patch { theme: Some("light"), ..Default::default() }).unwrap();
    let saved: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(saved, json!({"notifications": false, "font_px": 17, "theme": "light"}), "the other keys are kept");
    update_settings(&handoff, Patch { theme: Some("dark"), ..Default::default() }).unwrap();
    let saved: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(saved, json!({"notifications": false, "font_px": 17}), "dark is the default: the key goes, the file is what it was");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_patch_of_the_map_takes_the_theme_and_only_a_word() {
    assert_eq!(Patch::of_map(&map(json!({"theme": "light"}))).theme, Some("light"));
    assert_eq!(Patch::of_map(&map(json!({"theme": 3}))).theme, None, "a number is no theme");
    assert_eq!(Patch::of_map(&map(json!({"group_by": "workspace"}))).theme, None);
}

#[test]
fn the_request_takes_a_theme_of_three_words_alone_or_with_another_setting() {
    let ok = |value: Value| crate::ipc::valid_request("settings.set", &map(value));
    for word in ["dark", "light", "system"] {
        assert!(ok(json!({"op": "settings.set", "theme": word})), "{word}");
    }
    assert!(ok(json!({"op": "settings.set", "theme": "light", "font_px": 16})), "with another setting");
    for bad in [json!("Dark"), json!(""), json!("auto"), json!(1), json!(true), json!(null), json!(["light"])] {
        assert!(!ok(json!({"op": "settings.set", "theme": bad})), "{bad}");
    }
    assert!(!ok(json!({"op": "settings.set"})), "a request that sets nothing is still refused");
}
