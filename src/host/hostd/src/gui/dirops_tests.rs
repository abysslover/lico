use super::*;
use std::os::unix::fs::{symlink, PermissionsExt};

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lico_dirops_{tag}_{}", crate::core::random_hex(6)));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_name_is_refused_when_it_is_empty_a_dot_has_a_slash_a_break_or_is_too_long() {
    for (name, word) in [
        ("", "empty"),
        ("   ", "empty"),
        (".", "called"),
        ("..", "called"),
        (" .. ", "called"),
        ("a/b", "slash"),
        ("/", "slash"),
        ("a\0b", "slash"),
        ("two\nlines", "control"),
        ("tab\there", "control"),
        ("\u{200b}\u{200b}", "shows nothing"),
    ] {
        let why = name_problem(name).unwrap_or_else(|| panic!("{name:?} must be refused"));
        assert!(why.contains(word), "{name:?}: {why}");
    }
    let long = "x".repeat(MAX_NAME_BYTES + 1);
    assert!(name_problem(&long).is_some_and(|w| w.contains("long")), "one byte over the limit");
    assert_eq!(name_problem(&"x".repeat(MAX_NAME_BYTES)), None, "the limit itself is a name");
    let korean = "가".repeat(MAX_NAME_BYTES / 3);
    assert_eq!(name_problem(&korean), None, "the limit counts bytes: 85 syllables of three bytes are 255");
    assert!(name_problem(&format!("{korean}가")).is_some(), "86 syllables are 258 bytes");
}

#[test]
fn a_usual_name_is_a_name() {
    for name in ["new", "My Project", ".hidden", "프로젝트", "a.b.c", "x - y", " padded "] {
        assert_eq!(name_problem(name), None, "{name:?}");
    }
}

#[test]
fn a_folder_is_made_inside_the_parent_and_the_spaces_around_its_name_are_dropped() {
    let root = scratch("make");
    let made = make_dir(&root, "  fresh ").unwrap();
    assert_eq!(made, root.join("fresh"));
    assert!(made.is_dir());
    let again = make_dir(&root, "프로젝트").unwrap();
    assert!(again.is_dir() && again.ends_with("프로젝트"));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_name_that_is_taken_is_refused_whatever_holds_it_and_nothing_is_touched() {
    let root = scratch("taken");
    std::fs::create_dir(root.join("folder")).unwrap();
    std::fs::write(root.join("folder").join("keep.txt"), "kept").unwrap();
    std::fs::write(root.join("file"), "a file").unwrap();
    symlink(root.join("nowhere"), root.join("dangling")).unwrap();
    symlink(root.join("folder"), root.join("link")).unwrap();
    for name in ["folder", "file", "dangling", "link"] {
        let why = make_dir(&root, name).unwrap_err();
        assert!(why.contains("taken"), "{name}: {why}");
    }
    assert_eq!(std::fs::read_to_string(root.join("file")).unwrap(), "a file");
    assert_eq!(std::fs::read_to_string(root.join("folder").join("keep.txt")).unwrap(), "kept");
    assert!(std::fs::symlink_metadata(root.join("dangling")).unwrap().file_type().is_symlink(), "the dangling link stays a link");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_bad_name_makes_nothing_and_a_parent_that_is_gone_is_said() {
    let root = scratch("bad");
    for name in ["", "..", "a/b"] {
        assert!(make_dir(&root, name).is_err(), "{name:?}");
    }
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0, "a refused name leaves the parent as it was");
    let gone = root.join("not-here");
    let why = make_dir(&gone, "x").unwrap_err();
    assert!(why.contains("gone"), "{why}");
    assert!(!gone.exists(), "the missing parent is not made on the way");
    let file = root.join("a-file");
    std::fs::write(&file, "x").unwrap();
    assert!(make_dir(&file, "x").unwrap_err().contains("gone"), "a file is no parent");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_parent_that_may_not_be_written_is_said() {
    let root = scratch("denied");
    let locked = root.join("locked");
    std::fs::create_dir(&locked).unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o500)).unwrap();
    let probe = locked.join("probe");
    if std::fs::create_dir(&probe).is_ok() {
        std::fs::remove_dir(&probe).ok(); // the account may write anywhere (a superuser): nothing to see here
    } else {
        let why = make_dir(&locked, "x").unwrap_err();
        assert!(why.contains("no permission to make a folder here"), "{why}");
    }
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::remove_dir_all(&root).ok();
}
