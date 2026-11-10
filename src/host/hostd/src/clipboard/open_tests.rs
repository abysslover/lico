//! The safe open (class L0, real files in a private folder): a link at the last name is not followed, only a regular file of the user is taken, the size
//! bounds hold at both ends and a file that changes while it is read is an error. The setting `clipboard_write` is judged by the same open, so its
//! cases are here too.

use super::open::*;
use super::settings::{clipboard_write_enabled, set_clipboard_write, settings_path, status_json, MAX_SETTINGS_BYTES};
use crate::handoff::Scratch;
use std::io::{Cursor, Write};
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};

extern "C" {
    fn mkfifo(path: *const std::ffi::c_char, mode: u32) -> i32;
    fn geteuid() -> u32;
}

fn file_of(dir: &Scratch, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.0.join(name);
    std::fs::write(&path, bytes).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    path
}

fn fault(path: &Path, min: u64, max: u64) -> Fault {
    open_regular(path, min, max).expect_err("refused")
}

#[test]
fn a_symbolic_link_at_the_last_name_is_not_followed() {
    let d = Scratch::new("open_link");
    let real = file_of(&d, "real.png", b"CANARY-bytes");
    let link = d.0.join("link.png");
    symlink(&real, &link).unwrap();
    assert!(matches!(fault(&link, 1, 100), Fault::Open(_)));
    // a dangling link and a link to a directory are refused the same way
    symlink(d.0.join("nowhere"), d.0.join("dangling")).unwrap();
    assert!(matches!(fault(&d.0.join("dangling"), 1, 100), Fault::Open(_)));
    symlink(&d.0, d.0.join("dirlink")).unwrap();
    assert!(matches!(fault(&d.0.join("dirlink"), 1, 100), Fault::Open(_)));
    // the real file is taken
    assert_eq!(open_regular(&real, 1, 100).unwrap().size, 12);
}

#[test]
fn a_directory_a_fifo_and_a_missing_name_are_not_files_and_a_fifo_does_not_hang() {
    let d = Scratch::new("open_kinds");
    assert_eq!(fault(&d.0, 1, 100), Fault::NotRegular);
    let fifo = d.0.join("pipe");
    let c = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
    assert_eq!(unsafe { mkfifo(c.as_ptr(), 0o600) }, 0);
    let began = std::time::Instant::now();
    assert_eq!(fault(&fifo, 1, 100), Fault::NotRegular); // O_NONBLOCK: a pipe with no writer is not waited for
    assert!(began.elapsed() < std::time::Duration::from_secs(2));
    assert!(matches!(fault(&d.0.join("missing"), 1, 100), Fault::Open(_)));
    // a character device is not a regular file either
    assert_eq!(fault(Path::new("/dev/null"), 0, 100), Fault::NotRegular);
}

#[test]
fn a_file_of_another_user_is_refused() {
    // /etc/passwd is a regular file of root and of nobody the tests run as; as root itself the case cannot be made and the test says so
    let me = unsafe { geteuid() };
    assert_ne!(me, 0, "the tests are not run as root: a file of another user cannot be made otherwise");
    assert_eq!(fault(Path::new("/etc/passwd"), 1, 1 << 20), Fault::NotYours);
}

#[test]
fn the_size_bounds_hold_at_both_ends_for_a_picture_file() {
    let d = Scratch::new("open_size");
    let max = MAX_IMAGE_FILE;
    assert_eq!(max, 3_145_728);
    let exact = file_of(&d, "exact.png", &vec![7u8; max as usize]);
    let over = file_of(&d, "over.png", &vec![7u8; max as usize + 1]);
    let one = file_of(&d, "one.png", &[7]);
    let none = file_of(&d, "none.png", &[]);
    assert_eq!(open_regular(&exact, 1, max).unwrap().size, max);
    assert_eq!(read_checked(open_regular(&exact, 1, max).unwrap(), max).unwrap().len() as u64, max);
    assert_eq!(fault(&over, 1, max), Fault::TooBig(max + 1));
    assert_eq!(open_regular(&one, 1, max).unwrap().size, 1);
    assert_eq!(fault(&none, 1, max), Fault::TooSmall(0));
    assert!(open_regular(&none, 0, max).is_ok()); // a caller that accepts the empty file says min = 0
    assert_eq!(Fault::TooBig(1).reason(), "too_large");
    assert_eq!(Fault::TooSmall(0).reason(), "bad_image");
    for other in [Fault::NotRegular, Fault::NotYours, Fault::Open(String::new()), Fault::Read(String::new())] {
        assert_eq!(other.reason(), "write_failed");
    }
}

#[test]
fn a_file_that_grows_or_shrinks_while_it_is_read_is_an_error_and_the_limit_is_exact() {
    // a reader that holds more than fstat said (it grew), and less (it shrank)
    assert!(matches!(read_from(Cursor::new(vec![1u8; 15]), 10, 100), Err(Fault::Read(_))));
    assert!(matches!(read_from(Cursor::new(vec![1u8; 5]), 10, 100), Err(Fault::Read(_))));
    assert_eq!(read_from(Cursor::new(vec![1u8; 10]), 10, 100).unwrap().len(), 10);
    // the limit: max bytes pass, max + 1 is too large even when fstat said less
    assert_eq!(read_from(Cursor::new(vec![1u8; 100]), 100, 100).unwrap().len(), 100);
    assert_eq!(read_from(Cursor::new(vec![1u8; 101]), 100, 100), Err(Fault::TooBig(101)));
    assert_eq!(read_from(Cursor::new(vec![1u8; 5000]), 100, 100), Err(Fault::TooBig(101))); // never reads 5000 into memory: take(max + 1)
    // and a real file: appended to after the open
    let d = Scratch::new("open_grow");
    let path = file_of(&d, "grow.txt", b"0123456789");
    let opened = open_regular(&path, 1, 1000).unwrap();
    std::fs::OpenOptions::new().append(true).open(&path).unwrap().write_all(b"MORE").unwrap();
    assert!(matches!(read_checked(opened, 1000), Err(Fault::Read(_))));
    let opened = open_regular(&path, 1, 1000).unwrap();
    std::fs::write(&path, b"01").unwrap(); // truncated after the open
    assert!(matches!(read_checked(opened, 1000), Err(Fault::Read(_))));
}

// ---- the setting clipboard_write -------------------------------------------------------------------------------------------------

fn setting(dir: &Scratch, bytes: &[u8], mode: u32) -> PathBuf {
    let path = dir.0.join("state").join("settings.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let _ = std::fs::remove_file(&path);
    std::fs::write(&path, bytes).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
    path
}

#[test]
fn the_setting_is_off_unless_the_file_says_the_json_boolean_true_and_only_the_user_can_write_it() {
    let d = Scratch::new("setting");
    // the one place: beside handoff, in the state folder
    assert_eq!(settings_path(&d.0.join("state").join("handoff")), d.0.join("state").join("settings.json"));
    // missing: off
    assert!(!clipboard_write_enabled(&d.0.join("state").join("settings.json")));
    // on
    assert!(clipboard_write_enabled(&setting(&d, br#"{"clipboard_write": true}"#, 0o600)));
    assert!(clipboard_write_enabled(&setting(&d, br#"{"other": 1, "clipboard_write":true}"#, 0o644 & !0o022)));
    // every other value is off
    for bytes in [
        &br#"{"clipboard_write": false}"#[..],
        br#"{"clipboard_write": "true"}"#,
        br#"{"clipboard_write": 1}"#,
        br#"{"clipboard_write": "yes"}"#,
        br#"{"clipboard_write": null}"#,
        br#"{"clipboard_write": [true]}"#,
        br#"{}"#,
        br#"[true]"#,
        br#"true"#,
        br#"{"clipboard_write": true"#, // bad JSON
        br#"{"clipboard_write": true} trailing"#,
        b"",
        b"\xff\xfe",
    ] {
        assert!(!clipboard_write_enabled(&setting(&d, bytes, 0o600)), "{}", String::from_utf8_lossy(bytes));
    }
    // a file that the group or others can write is off, whatever it says
    for mode in [0o620, 0o602, 0o666, 0o660, 0o606] {
        assert!(!clipboard_write_enabled(&setting(&d, br#"{"clipboard_write": true}"#, mode)), "{mode:o}");
    }
    // a file over 4 KiB is off, even if it begins with the right words; 4096 exactly is read
    let mut big = br#"{"clipboard_write": true, "pad": ""#.to_vec();
    big.resize(MAX_SETTINGS_BYTES as usize - 2, b'x');
    big.extend_from_slice(b"\"}");
    assert_eq!(big.len() as u64, MAX_SETTINGS_BYTES);
    assert!(clipboard_write_enabled(&setting(&d, &big, 0o600)));
    big.insert(40, b'x');
    assert!(!clipboard_write_enabled(&setting(&d, &big, 0o600)));
}

#[test]
fn the_setting_file_must_be_a_regular_file_not_a_link_a_directory_or_a_pipe() {
    let d = Scratch::new("setting_kinds");
    let real = setting(&d, br#"{"clipboard_write": true}"#, 0o600);
    assert!(clipboard_write_enabled(&real));
    let link = d.0.join("state").join("link.json");
    symlink(&real, &link).unwrap();
    assert!(!clipboard_write_enabled(&link));
    let dir = d.0.join("state").join("dir.json");
    std::fs::create_dir_all(&dir).unwrap();
    assert!(!clipboard_write_enabled(&dir));
    let fifo = d.0.join("state").join("pipe.json");
    let c = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
    assert_eq!(unsafe { mkfifo(c.as_ptr(), 0o600) }, 0);
    assert!(!clipboard_write_enabled(&fifo));
    // a file of another user that says true is off as well
    assert!(!clipboard_write_enabled(Path::new("/etc/passwd")));
}

#[test]
fn the_setting_is_read_at_each_use_so_switching_it_needs_no_restart() {
    let d = Scratch::new("setting_fresh");
    let path = setting(&d, br#"{"clipboard_write": false}"#, 0o600);
    assert!(!clipboard_write_enabled(&path));
    setting(&d, br#"{"clipboard_write": true}"#, 0o600);
    assert!(clipboard_write_enabled(&path));
    setting(&d, br#"{"clipboard_write": false}"#, 0o600);
    assert!(!clipboard_write_enabled(&path));
}

#[test]
fn set_clipboard_write_writes_atomically_mode_0600_preserves_other_keys_and_refuses_links() {
    use std::os::unix::fs::MetadataExt;
    let d = Scratch::new("set_settings");
    let handoff = d.0.join("state").join("handoff");
    assert_eq!(status_json(&handoff), serde_json::json!({"clipboard_write": false}));
    let body = set_clipboard_write(&handoff, true).unwrap();
    assert_eq!(body.get("settings"), Some(&serde_json::json!({"clipboard_write": true})));
    let path = settings_path(&handoff);
    assert!(clipboard_write_enabled(&path));
    assert_eq!(std::fs::symlink_metadata(&path).unwrap().mode() & 0o777, 0o600);
    // another key in a valid settings file is preserved across a toggle
    setting(&d, br#"{"notifications": false, "clipboard_write": true}"#, 0o600);
    set_clipboard_write(&handoff, false).unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved, serde_json::json!({"notifications": false, "clipboard_write": false}));
    // a symbolic link or a directory at settings.json is refused and not followed
    let target = file_of(&d, "outside.json", br#"{"keep": 1}"#);
    std::fs::remove_file(&path).unwrap();
    symlink(&target, &path).unwrap();
    assert_eq!(set_clipboard_write(&handoff, true), Err("settings_failed"));
    assert_eq!(std::fs::read(&target).unwrap(), br#"{"keep": 1}"#);
}

// ------------------------------------------------------------------------------------------------------------------------ write_slot (S05b)
#[test]
fn write_slot_fills_an_empty_file_of_the_user_and_nothing_else() {
    let d = Scratch::new("write_slot");
    let empty = file_of(&d, "empty.png", b"");
    write_slot(&empty, b"all the bytes").unwrap();
    assert_eq!(std::fs::read(&empty).unwrap(), b"all the bytes");
    assert_eq!(write_slot(&empty, b"again"), Err(Fault::TooBig(13)), "a file that holds bytes is not written");
    assert_eq!(std::fs::read(&empty).unwrap(), b"all the bytes");
    // a missing name is not created
    let missing = d.0.join("missing.png");
    assert!(matches!(write_slot(&missing, b"x"), Err(Fault::Open(_))) && !missing.exists());
    // a link at the last name is not followed, a link to a directory neither
    let target = file_of(&d, "target", b"");
    symlink(&target, d.0.join("link.png")).unwrap();
    assert!(matches!(write_slot(&d.0.join("link.png"), b"x"), Err(Fault::Open(_))));
    assert_eq!(std::fs::read(&target).unwrap(), b"");
    assert!(write_slot(&d.0, b"x").is_err(), "a directory");
    // a pipe does not hang
    let fifo = d.0.join("fifo");
    let c = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
    assert_eq!(unsafe { mkfifo(c.as_ptr(), 0o600) }, 0);
    assert!(write_slot(&fifo, b"x").is_err());
    // a file of another user
    assert!(write_slot(Path::new("/dev/null"), b"x").is_err(), "not a regular file");
    assert!(unsafe { geteuid() } != u32::MAX);
    // a file the user may not write
    let ro = file_of(&d, "ro.png", b"");
    std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o400)).unwrap();
    if unsafe { geteuid() } != 0 {
        assert!(matches!(write_slot(&ro, b"x"), Err(Fault::Open(_))));
    }
}
