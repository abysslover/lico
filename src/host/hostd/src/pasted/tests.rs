//! Tests of the pasted-picture folder, the slot, the file checks and the transcript word (plan M02-P04, S05a). The whole path through a real host and a
//! provider is `tests/test_picture_input.py`.

use super::*;
use crate::handoff::Scratch;
use crate::transfer::tests::world;
use std::os::unix::fs::symlink;

/// The 33 bytes `png_size` reads: signature, IHDR length and type, width, height, the rest of IHDR and its CRC.
fn header(w: u32, h: u32) -> Vec<u8> {
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R'];
    out.extend(w.to_be_bytes());
    out.extend(h.to_be_bytes());
    out.extend([8, 2, 0, 0, 0, 0, 0, 0, 0]); // depth, colour, three methods, and a CRC that is not checked
    out
}

fn mode(path: &Path) -> u32 {
    std::fs::symlink_metadata(path).unwrap().mode() & 0o7777
}

fn put(shared: &Shared, name: &str, bytes: &[u8]) -> PathBuf {
    let dir = pasted_dir(&shared.cfg.handoff_dir);
    ensure_dir(&dir).unwrap();
    std::fs::write(dir.join(name), bytes).unwrap();
    dir.join(name)
}

const A: &str = "0123456789abcdef0123456789abcdef.png";
const B: &str = "fedcba9876543210fedcba9876543210.png";

#[test]
fn the_folder_is_beside_handoff_and_a_name_is_32_lowercase_hex_and_png() {
    assert_eq!(pasted_dir(Path::new("/state/handoff")), Path::new("/state/pasted"));
    assert!(valid_name(A));
    let bad = ["", "a.png", &A[..35], &format!("{A}x"), &A.replace(".png", ".PNG"), &A.to_uppercase().replace(".PNG", ".png"), "0123456789abcdef0123456789abcdeg.png", &format!("../{}", &A[3..]), &format!("0123456789abcdef0123456789abcde/.png"), "0123456789abcdef0123456789abcdef.pn_", "0123456789abcdef0123456789abcdef.jpg"];
    for name in bad {
        assert!(!valid_name(name), "{name:?}");
    }
    assert!(!valid_name(&format!("{}\u{e9}.png", &A[..30]))); // 36 bytes, but not hex
}

#[test]
fn prepare_makes_the_folder_0700_and_removes_everything_in_it() {
    let dir = Scratch::new("pasted_prepare");
    let handoff = dir.0.join("handoff");
    let pasted = dir.0.join("pasted");
    std::fs::create_dir_all(&pasted).unwrap();
    std::fs::set_permissions(&pasted, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(pasted.join("left.png"), b"x").unwrap();
    symlink("/CANARY-nowhere", pasted.join("link.png")).unwrap();
    std::fs::create_dir(pasted.join("sub")).unwrap();
    prepare(&handoff).unwrap();
    assert_eq!(mode(&pasted), 0o700);
    assert_eq!(std::fs::read_dir(&pasted).unwrap().count(), 0);
    // a link or a file in the place of the folder is replaced, never followed
    std::fs::remove_dir(&pasted).unwrap();
    let target = dir.0.join("elsewhere");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("keep"), b"k").unwrap();
    symlink(&target, &pasted).unwrap();
    prepare(&handoff).unwrap();
    assert!(std::fs::symlink_metadata(&pasted).unwrap().is_dir() && target.join("keep").exists());
}

#[test]
fn the_sweep_removes_what_is_older_than_the_limit_and_judges_a_link_by_itself() {
    let dir = Scratch::new("pasted_sweep");
    let (old, young, link) = (dir.0.join("old"), dir.0.join("young"), dir.0.join("link"));
    std::fs::write(&old, b"o").unwrap();
    std::fs::write(&young, b"y").unwrap();
    let target = dir.0.join("target");
    std::fs::write(&target, b"t").unwrap();
    symlink(&target, &link).unwrap();
    let now = now_secs();
    std::fs::File::options().write(true).open(&old).unwrap().set_modified(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(now - MAX_AGE_SECS - 5)).unwrap();
    std::fs::File::options().write(true).open(&young).unwrap().set_modified(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(now - MAX_AGE_SECS + 30)).unwrap();
    sweep(&dir.0, now, MAX_AGE_SECS);
    assert!(!old.exists() && young.exists() && link.exists() && target.exists(), "only the old file goes");
    sweep(&dir.0, now + 100, MAX_AGE_SECS); // the young file is now old too; the link was made just now by the clock and is young
    assert!(!young.exists() && link.symlink_metadata().is_ok() && target.exists());
}

#[test]
fn a_slot_is_an_empty_0600_file_in_a_0700_folder_and_the_ninth_is_busy() {
    let w = world(&[]);
    let dir = pasted_dir(&w.shared.cfg.handoff_dir);
    let mut names = Vec::new();
    for _ in 0..MAX_SLOTS {
        let reply = slot(&w.shared).unwrap();
        let (name, path) = (reply["name"].as_str().unwrap().to_string(), PathBuf::from(reply["path"].as_str().unwrap()));
        assert!(valid_name(&name) && path.is_absolute() && path.file_name().unwrap() == name.as_str());
        assert_eq!(path.parent().unwrap(), std::fs::canonicalize(&dir).unwrap());
        assert_eq!((mode(&path), std::fs::metadata(&path).unwrap().len()), (0o600, 0));
        names.push(name);
    }
    assert_eq!(mode(&dir), 0o700);
    names.sort();
    names.dedup();
    assert_eq!(names.len(), MAX_SLOTS, "every name is new");
    assert_eq!(slot(&w.shared).unwrap_err(), "busy");
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), MAX_SLOTS, "a refused call makes nothing");
}

#[test]
fn a_slot_call_sweeps_first_so_old_files_do_not_keep_it_busy() {
    let w = world(&[]);
    let first = slot(&w.shared).unwrap()["path"].as_str().unwrap().to_string();
    for _ in 1..MAX_SLOTS {
        slot(&w.shared).unwrap();
    }
    assert_eq!(slot(&w.shared).unwrap_err(), "busy");
    let old = SystemTime::now() - std::time::Duration::from_secs(MAX_AGE_SECS + 60);
    std::fs::File::options().write(true).open(&first).unwrap().set_modified(old).unwrap();
    assert!(slot(&w.shared).is_ok());
    assert!(!Path::new(&first).exists());
}

#[test]
fn the_checks_of_the_file_have_a_word_each_and_the_picture_comes_out_in_base64() {
    let w = world(&[]);
    let s = &w.shared;
    assert_eq!(take(s, None, true).unwrap().map(|p| p.size), None);
    assert_eq!(take(s, Some(A), false).err(), Some("attachment_unsupported"), "the kind comes first, even for a file that is not there");
    assert_eq!(take(s, Some(A), true).err(), Some("attachment_gone"));
    let dir = pasted_dir(&s.cfg.handoff_dir);
    let good = header(40, 30);
    put(s, A, &good);
    let picture = take(s, Some(A), true).unwrap().unwrap();
    assert_eq!((picture.size, picture.base64.len()), (good.len() as u64, good.len().div_ceil(3) * 4));
    // the words
    put(s, B, &[]);
    assert_eq!(take(s, Some(B), true).err(), Some("bad_image"), "empty");
    put(s, B, &good[..7]);
    assert_eq!(take(s, Some(B), true).err(), Some("bad_image"), "shorter than a signature");
    put(s, B, b"this is a text file, long enough to be read, but not a picture");
    assert_eq!(take(s, Some(B), true).err(), Some("bad_image"), "not a PNG");
    for (w8, h8) in [(0, 30), (30, 0), (8001, 30), (30, 8001)] {
        put(s, B, &header(w8, h8));
        assert_eq!(take(s, Some(B), true).err(), Some("bad_image"), "{w8} x {h8}");
    }
    put(s, B, &header(8000, 8000));
    assert!(take(s, Some(B), true).is_ok(), "8000 is allowed");
    let mut big = header(40, 30);
    big.resize(MAX_IMAGE_FILE as usize, 0);
    put(s, B, &big);
    assert!(take(s, Some(B), true).is_ok(), "exactly 3 MiB passes");
    big.push(0);
    put(s, B, &big);
    assert_eq!(take(s, Some(B), true).err(), Some("attachment_too_large"), "3 MiB + 1");
    std::fs::remove_file(dir.join(B)).unwrap();
    symlink(dir.join(A), dir.join(B)).unwrap();
    assert_eq!(take(s, Some(B), true).err(), Some("attachment_gone"), "a link is not followed, even to a good file");
    std::fs::remove_file(dir.join(B)).unwrap();
    std::fs::create_dir(dir.join(B)).unwrap();
    assert_eq!(take(s, Some(B), true).err(), Some("attachment_gone"), "a folder");
}

#[test]
fn the_file_is_removed_after_every_answer_and_a_link_only_as_a_link() {
    let w = world(&[]);
    let s = &w.shared;
    let path = put(s, A, &header(4, 4));
    let answer = with_cleanup(s, Some(A), |name| {
        assert_eq!(name, Some(A));
        Err("session_exited")
    });
    assert_eq!(answer.unwrap_err(), "session_exited");
    assert!(!path.exists(), "removed after a refusal");
    let path = put(s, A, &header(4, 4));
    assert!(with_cleanup(s, Some(A), |_| Ok(Default::default())).is_ok());
    assert!(!path.exists(), "removed after a success");
    let target = path.with_file_name("target-not-a-name");
    std::fs::write(&target, b"keep me").unwrap();
    symlink(&target, &path).unwrap();
    let _ = with_cleanup(s, Some(A), |_| Err("x"));
    assert!(!path.exists() && target.exists(), "the link goes, its target stays");
    let folder = path.clone();
    std::fs::create_dir(&folder).unwrap();
    let _ = with_cleanup(s, Some(A), |_| Err("x"));
    assert!(!folder.exists(), "an empty folder goes");
    let _ = with_cleanup(s, None, |name| {
        assert_eq!(name, None);
        Err("x")
    });
}

#[test]
fn the_transcript_word_rounds_to_the_nearest_kib_and_never_shows_zero() {
    let of = |size: u64| echo("hello", Some(&Picture { size, base64: String::new() }));
    assert_eq!(echo("hello", None), "hello");
    assert_eq!(of(8), "[picture 1 KiB] hello");
    assert_eq!(of(1068), "[picture 1 KiB] hello");
    assert_eq!(of(1536), "[picture 2 KiB] hello");
    assert_eq!(of(250_880), "[picture 245 KiB] hello");
}

#[test]
fn the_member_attachment_is_valid_on_input_and_on_no_other_op() {
    use serde_json::{json, Map, Value};
    let request = |op: &str, extra: Value| -> Map<String, Value> {
        let mut m = Map::new();
        m.insert("op".into(), json!(op));
        for (k, v) in extra.as_object().unwrap() {
            m.insert(k.clone(), v.clone());
        }
        m
    };
    let input = json!({"session": "s", "generation": 1, "request_id": "r", "text": "t"});
    let with = |name: Value| {
        let mut m = input.as_object().unwrap().clone();
        m.insert("attachment".into(), name);
        request("input", Value::Object(m))
    };
    assert!(crate::ipc::valid_request("input", &with(json!(A))));
    for bad in [json!(""), json!(&A[..35]), json!(format!("{A}x")), json!(1), json!(null), json!(["x"]), json!(A.to_uppercase()), json!(format!("../{}", &A[3..]))] {
        assert!(!crate::ipc::valid_request("input", &with(bad.clone())), "{bad}");
    }
    let mut no_text = input.as_object().unwrap().clone();
    no_text.remove("text");
    no_text.insert("attachment".into(), json!(A));
    assert!(!crate::ipc::valid_request("input", &request("input", Value::Object(no_text))), "the text stays required");
    let mut empty = input.as_object().unwrap().clone();
    empty.insert("text".into(), json!(""));
    empty.insert("attachment".into(), json!(A));
    assert!(!crate::ipc::valid_request("input", &request("input", Value::Object(empty))), "and non-empty");
    // every other op: a valid request, which the member then spoils (the ones that take a text first)
    let others = [
        ("transfer.send", json!({"to": "s", "kind": "text", "text": "t"})),
        ("transfer.send", json!({"to": "s", "kind": "conversation", "from": "s"})),
        ("clipboard.write", json!({"kind": "text", "text": "t"})),
        ("clipboard.write", json!({"kind": "image", "path": "/x.png"})),
        ("input.status", json!({"session": "s", "request_id": "r"})),
        ("snapshot", json!({"session": "s"})),
        ("session.create", json!({"kind": "claude"})),
        ("session.rename", json!({"session": "s", "name": "n"})),
        ("account.login.input", json!({"account": "a", "code": "c"})),
        ("approval.answer", json!({"session": "s", "generation": 1, "approval_id": 1, "answer": "allow"})),
        ("hello", json!({})),
    ];
    for (op, body) in others {
        assert!(crate::ipc::valid_request(op, &request(op, body.clone())), "{op}: the request is valid without the member");
        let mut m = request(op, body);
        m.insert("attachment".into(), json!(A));
        assert!(!crate::ipc::valid_request(op, &m), "{op} takes no attachment");
    }
    assert!(crate::ipc::valid_request("picture.slot", &request("picture.slot", json!({}))));
    assert!(crate::ipc::valid_request("picture.slot", &request("picture.slot", json!({"id": "x", "v": 1}))));
    for extra in [json!({"name": "x"}), json!({"path": "/x"}), json!({"attachment": A}), json!({"session": "s"})] {
        assert!(!crate::ipc::valid_request("picture.slot", &request("picture.slot", extra.clone())), "{extra}");
    }
}
