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

#[test]
fn a_name_is_changed_in_place_and_the_content_goes_with_it() {
    let root = scratch("rename");
    std::fs::create_dir(root.join("old folder")).unwrap();
    std::fs::write(root.join("old folder").join("inside.txt"), "inside").unwrap();
    std::fs::write(root.join("a.txt"), "text").unwrap();
    symlink("a.txt", root.join("l")).unwrap();
    let to = rename(&root, "old folder", "  새 폴더 ").unwrap();
    assert_eq!(to, root.join("새 폴더"), "the spaces around the new name are dropped");
    assert_eq!(std::fs::read_to_string(to.join("inside.txt")).unwrap(), "inside");
    assert!(!root.join("old folder").exists());
    rename(&root, "a.txt", "b.txt").unwrap();
    assert_eq!(std::fs::read_to_string(root.join("b.txt")).unwrap(), "text");
    rename(&root, "l", "m").unwrap();
    assert!(std::fs::symlink_metadata(root.join("m")).unwrap().file_type().is_symlink(), "a link stays a link, only its name changes");
    assert_eq!(std::fs::read_link(root.join("m")).unwrap(), PathBuf::from("a.txt"));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_rename_never_replaces_whatever_holds_the_new_name() {
    let root = scratch("rename_taken");
    std::fs::create_dir(root.join("folder")).unwrap();
    std::fs::write(root.join("folder").join("keep.txt"), "kept").unwrap();
    std::fs::write(root.join("file"), "a file").unwrap();
    std::fs::write(root.join("mine"), "mine").unwrap();
    symlink(root.join("nowhere"), root.join("dangling")).unwrap();
    for name in ["folder", "file", "dangling"] {
        let why = rename(&root, "mine", name).unwrap_err();
        assert!(why.contains("taken"), "{name}: {why}");
    }
    assert_eq!(std::fs::read_to_string(root.join("mine")).unwrap(), "mine", "what was asked to move stays where it was");
    assert_eq!(std::fs::read_to_string(root.join("file")).unwrap(), "a file");
    assert_eq!(std::fs::read_to_string(root.join("folder").join("keep.txt")).unwrap(), "kept");
    assert!(std::fs::symlink_metadata(root.join("dangling")).unwrap().file_type().is_symlink());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_rename_says_when_the_name_is_the_same_bad_or_the_thing_is_gone() {
    let root = scratch("rename_bad");
    std::fs::write(root.join("a"), "x").unwrap();
    assert!(rename(&root, "a", "a").unwrap_err().contains("its name already"));
    assert!(rename(&root, "a", " a ").unwrap_err().contains("its name already"), "the spaces do not make it another name");
    for name in ["", "..", "x/y", "two\nlines"] {
        assert!(rename(&root, "a", name).is_err(), "{name:?}");
    }
    assert!(root.join("a").is_file(), "a refused name leaves it where it was");
    assert!(rename(&root, "missing", "b").unwrap_err().contains("gone"));
    assert!(!root.join("b").exists());
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_file_a_folder_with_a_tree_in_it_and_a_link_are_removed_and_a_link_is_never_followed() {
    let root = scratch("remove");
    let outside = scratch("remove_outside");
    std::fs::write(outside.join("precious.txt"), "precious").unwrap();
    let tree = root.join("tree");
    std::fs::create_dir_all(tree.join("a").join("b")).unwrap();
    std::fs::write(tree.join("a").join("b").join("deep.txt"), "deep").unwrap();
    symlink(&outside, tree.join("a").join("to-outside")).unwrap();
    symlink(outside.join("precious.txt"), tree.join("to-file")).unwrap();
    remove(&tree).unwrap();
    assert!(std::fs::symlink_metadata(&tree).is_err(), "the tree is gone");
    assert_eq!(std::fs::read_to_string(outside.join("precious.txt")).unwrap(), "precious", "a link inside the tree was removed as a link");
    std::fs::write(root.join("f"), "x").unwrap();
    remove(&root.join("f")).unwrap();
    assert!(!root.join("f").exists());
    symlink(&outside, root.join("link")).unwrap();
    remove(&root.join("link")).unwrap();
    assert!(std::fs::symlink_metadata(root.join("link")).is_err(), "the link is gone");
    assert!(outside.join("precious.txt").is_file(), "what the link pointed to is not touched");
    symlink(root.join("nowhere"), root.join("dangling")).unwrap();
    remove(&root.join("dangling")).unwrap();
    assert!(std::fs::symlink_metadata(root.join("dangling")).is_err(), "a dangling link goes too");
    assert!(remove(&root.join("dangling")).unwrap_err().contains("gone"), "what is gone is said");
    std::fs::remove_dir_all(&root).ok();
    std::fs::remove_dir_all(&outside).ok();
}

#[test]
fn what_the_question_says_about_a_folder_counts_its_items_up_to_a_bound_and_never_follows_a_link() {
    let root = scratch("describe");
    let big = scratch("describe_big");
    for i in 0..20 {
        std::fs::write(big.join(format!("f{i}")), "x").unwrap();
    }
    std::fs::create_dir_all(root.join("d").join("e")).unwrap();
    std::fs::write(root.join("d").join("one"), "1").unwrap();
    std::fs::write(root.join("d").join("e").join("two"), "2").unwrap();
    assert_eq!(describe(&root.join("d")).unwrap(), "a folder with 3 items", "e, one and two");
    std::fs::create_dir(root.join("empty")).unwrap();
    assert_eq!(describe(&root.join("empty")).unwrap(), "an empty folder");
    std::fs::create_dir(root.join("one-item")).unwrap();
    std::fs::write(root.join("one-item").join("x"), "x").unwrap();
    assert_eq!(describe(&root.join("one-item")).unwrap(), "a folder with 1 item");
    symlink(&big, root.join("d").join("link-to-big")).unwrap();
    assert_eq!(describe(&root.join("d")).unwrap(), "a folder with 4 items", "the link counts as one and what it points to is not entered");
    assert_eq!(describe_limited(&big, 5).unwrap(), "a folder with more than 5 items", "the count stops at its bound");
    assert_eq!(describe_limited(&big, 20).unwrap(), "a folder with 20 items", "the bound itself is still counted exactly");
    std::fs::remove_dir_all(&root).ok();
    std::fs::remove_dir_all(&big).ok();
}

#[test]
fn what_the_question_says_about_a_file_a_link_and_what_is_gone() {
    let root = scratch("describe_other");
    std::fs::write(root.join("small"), "12345").unwrap();
    std::fs::write(root.join("kib"), vec![b'x'; 1536]).unwrap();
    assert_eq!(describe(&root.join("small")).unwrap(), "a file of 5 bytes");
    assert_eq!(describe(&root.join("kib")).unwrap(), "a file of 1.5 KiB");
    assert_eq!(human_bytes(0), "0 bytes");
    assert_eq!(human_bytes(1023), "1023 bytes");
    assert_eq!(human_bytes(1024), "1.0 KiB");
    assert_eq!(human_bytes(5 * 1024 * 1024 + 512 * 1024), "5.5 MiB");
    assert_eq!(human_bytes(3 * 1024 * 1024 * 1024), "3.0 GiB");
    assert_eq!(human_bytes(1024 * 1024), "1.0 MiB", "a MiB is a MiB, not 1024 KiB");
    assert_eq!(human_bytes(1024 * 1024 * 1024), "1.0 GiB");
    assert_eq!(human_bytes(2 * 1024 * 1024 * 1024 * 1024), "2048.0 GiB", "there is no unit above GiB: a very large file is said in them");
    symlink("small", root.join("l")).unwrap();
    assert_eq!(describe(&root.join("l")).unwrap(), "a link to small");
    symlink(root.join("nowhere"), root.join("dangling")).unwrap();
    assert!(describe(&root.join("dangling")).unwrap().starts_with("a link to "), "a dangling link is described as a link");
    assert!(describe(&root.join("gone")).unwrap_err().contains("gone"));
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn the_root_the_home_folder_and_every_folder_above_it_are_protected_and_a_link_to_them_is_not() {
    let root = scratch("protected");
    let home = root.join("people").join("me");
    std::fs::create_dir_all(home.join("work")).unwrap();
    std::fs::create_dir(root.join("other")).unwrap();
    let canon_root = root.canonicalize().unwrap();
    assert!(protected(Path::new("/"), Some(&home)), "the root has no parent: nothing to rename or delete");
    assert!(protected(Path::new("/"), None));
    assert!(protected(&home, Some(&home)), "the home folder");
    assert!(protected(&root.join("people"), Some(&home)), "the folder above the home folder");
    assert!(protected(&canon_root, Some(&home)), "and every one above that");
    assert!(!protected(&home.join("work"), Some(&home)), "inside the home folder is free");
    assert!(!protected(&root.join("other"), Some(&home)), "beside it too");
    assert!(!protected(&home, None), "with no home known only the root is protected");
    assert!(!protected(&root.join("other"), None));
    symlink(root.join("people"), root.join("ppl-home")).unwrap();
    assert!(protected(&home, Some(&root.join("ppl-home").join("me"))), "the home folder named through a link is the same folder");
    symlink(&home, root.join("to-home")).unwrap();
    assert!(!protected(&root.join("to-home"), Some(&home)), "a link that points at the home folder is only a link");
    // a path that reaches the home folder through a link in the folder above is the home folder
    symlink(root.join("people"), root.join("ppl")).unwrap();
    assert!(protected(&root.join("ppl").join("me"), Some(&home)), "the same folder by another way in");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_folder_that_may_not_be_written_refuses_a_rename_and_says_so() {
    let root = scratch("rename_denied");
    let locked = root.join("locked");
    std::fs::create_dir(&locked).unwrap();
    std::fs::write(locked.join("a"), "x").unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o500)).unwrap();
    let probe = locked.join("probe");
    if std::fs::write(&probe, "x").is_ok() {
        std::fs::remove_file(&probe).ok(); // the account may write anywhere (a superuser): nothing to see here
    } else {
        let why = rename(&locked, "a", "b").unwrap_err();
        assert!(why.contains("no permission to rename it here"), "{why}");
        assert!(locked.join("a").is_file(), "the file stays where it was");
    }
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::remove_dir_all(&root).ok();
}
