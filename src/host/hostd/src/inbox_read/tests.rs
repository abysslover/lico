use super::*;
use crate::handoff::Scratch;
use std::os::unix::fs::symlink;

const ME: &str = "s0123456789abcdef";
const OTHER: &str = "s1111111111111111";

/// A handoff folder with one file in the inbox of `ME` (as the host stores it): the folder, the path of the file.
fn inbox_with(content: &str) -> (Scratch, PathBuf, PathBuf) {
    let dir = Scratch::new("inbox-read");
    let root = dir.0.join("handoff");
    let path = crate::handoff::store(&root, ME, "20261027", "101500", "s111111", "message", content.as_bytes()).unwrap();
    (dir, root, path)
}

fn arguments(path: &Path, rest: Value) -> Map<String, Value> {
    let mut all = rest.as_object().unwrap().clone();
    all.insert("path".into(), json!(path.to_str().unwrap()));
    all
}

fn answer(root: &Path, who: &str, path: &Path, rest: Value) -> Result<Value, String> {
    read_in(root, who, &arguments(path, rest)).map(|got| serde_json::from_str(&got).unwrap()).map_err(|fault| fault.words())
}

/// The file read in parts of `limit` bytes (none: the default) by following `next`: the parts, and the text they make.
fn in_parts(root: &Path, path: &Path, limit: Option<u64>) -> (Vec<Value>, String) {
    let (mut parts, mut text, mut offset) = (Vec::new(), String::new(), 0u64);
    loop {
        let rest = match limit {
            Some(limit) => json!({"offset": offset, "limit": limit}),
            None => json!({"offset": offset}),
        };
        let got = answer(root, ME, path, rest).unwrap();
        text += got["text"].as_str().unwrap();
        let next = got["next"].as_u64();
        parts.push(got);
        match next {
            Some(next) => offset = next,
            None => return (parts, text),
        }
        assert!(parts.len() < 1_000_000, "a read that does not end");
    }
}

/// What `run` gives back, or a failure when it has not come back in five seconds: a read that waits for a pipe must not hang the test run.
fn soon<T: Send + 'static>(run: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || tx.send(run()).ok());
    rx.recv_timeout(std::time::Duration::from_secs(5)).expect("a read that waits for a pipe")
}

/// The stored file: the header the host writes, then the content.
fn stored(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn a_file_of_a_mebibyte_read_in_parts_of_the_default_size_is_the_file() {
    let line = "한글 text, 😀 emoji, plain ascii and a long enough line to cross a part\n";
    let (_dir, root, path) = inbox_with(&line.repeat((1 << 20) / line.len() + 1));
    let whole = stored(&path);
    assert!(whole.len() > 1 << 20);
    let (parts, text) = in_parts(&root, &path, None);
    assert_eq!(text, whole, "the parts, put together, are the file");
    let least = whole.len().div_ceil(PART);
    assert!((least..=least + 1).contains(&parts.len()), "{} parts of a file of {} bytes", parts.len(), whole.len());
    for (k, part) in parts.iter().enumerate() {
        let (offset, end) = (part["offset"].as_u64().unwrap(), part["end"].as_u64().unwrap());
        assert_eq!(part["size"].as_u64(), Some(whole.len() as u64));
        assert!(end > offset && end - offset <= PART as u64, "part {k}: {offset}..{end}");
        assert_eq!(part["text"].as_str().unwrap().len() as u64, end - offset);
        assert_eq!(part["next"].as_u64(), (end < whole.len() as u64).then_some(end));
    }
    assert_eq!(parts[0]["offset"], 0);
    assert_eq!(parts.last().unwrap()["next"], Value::Null);
    assert_eq!(parts[0]["end"], parts[1]["offset"], "each part starts where the one before ended");
}

#[test]
fn a_small_limit_cuts_at_a_character_never_inside_one_and_still_ends_with_the_file() {
    let (_dir, root, path) = inbox_with("한글😀é plain 한😀한😀\n".repeat(40).as_str());
    let whole = stored(&path);
    for limit in [4u64, 5, 6, 7, 8, 100, 1000, 4097] {
        let (parts, text) = in_parts(&root, &path, Some(limit));
        assert_eq!(text, whole, "limit {limit}");
        for part in &parts {
            let length = part["text"].as_str().unwrap().len() as u64;
            assert!(length >= 1 && length <= limit, "limit {limit}: a part of {length} bytes");
            assert!(!part["text"].as_str().unwrap().contains('\u{fffd}'), "no cut character: limit {limit}");
        }
    }
}

#[test]
fn the_limit_is_at_most_32_kib_at_least_a_character_and_the_numbers_may_be_digits() {
    let (_dir, root, path) = inbox_with(&"x".repeat(100_000));
    let size = stored(&path).len() as u64;
    let end = |rest: Value| answer(&root, ME, &path, rest).map(|got| got["end"].as_u64().unwrap());
    assert_eq!(end(json!({})), Ok(PART as u64), "no limit: a whole part");
    assert_eq!(end(json!({"limit": 1_000_000})), Ok(PART as u64), "a larger limit is the part");
    assert_eq!(end(json!({"limit": "40000"})), Ok(PART as u64), "digits are a number");
    assert_eq!(end(json!({"limit": "100", "offset": "50"})), Ok(150));
    assert_eq!(end(json!({"limit": 1})), Ok(4), "a limit under four is four: one character always fits");
    assert_eq!(end(json!({"offset": size - 3, "limit": 100})), Ok(size));
    for bad in [json!({"limit": 0}), json!({"limit": -1}), json!({"limit": 1.5}), json!({"limit": "abc"}), json!({"limit": ""}), json!({"limit": true}), json!({"limit": [1]}), json!({"offset": -1}),
                json!({"offset": "1e3"}), json!({"offset": "99999999999999999999"}), json!({"offset": {}})] {
        let said = end(bad.clone()).unwrap_err();
        assert!(said.starts_with("bad_request: read_handoff needs `path`"), "{bad}: {said}");
    }
    assert_eq!(end(json!({"limit": null, "offset": null})), Ok(PART as u64), "null is left out");
}

#[test]
fn an_offset_at_the_end_is_an_empty_part_and_past_the_end_or_inside_a_character_is_refused() {
    let (_dir, root, path) = inbox_with("한글");
    let size = stored(&path).len() as u64;
    let at_end = answer(&root, ME, &path, json!({"offset": size})).unwrap();
    assert_eq!((at_end["text"].as_str(), at_end["next"].clone(), at_end["end"].as_u64()), (Some(""), Value::Null, Some(size)));
    let past = answer(&root, ME, &path, json!({"offset": size + 1})).unwrap_err();
    assert_eq!(past, format!("bad_request: offset is past the end of the file, which is {size} bytes"));
    let text = stored(&path);
    let inside = text.find('한').unwrap() as u64 + 1;
    assert_eq!(answer(&root, ME, &path, json!({"offset": inside})).unwrap_err(), "bad_request: offset is inside a character; start at `next` of the last answer");
    assert!(answer(&root, ME, &path, json!({"offset": inside - 1})).is_ok());
}

#[test]
fn a_path_that_is_not_below_the_callers_inbox_is_refused_the_same_whether_or_not_a_file_is_there() {
    let (dir, root, path) = inbox_with("mine");
    let other = crate::handoff::store(&root, OTHER, "20261027", "101500", "s222222", "message", b"theirs").unwrap();
    let outside = dir.0.join("outside.txt");
    std::fs::write(&outside, "outside").unwrap();
    let said = "not_allowed: read_handoff reads files in your own handoff folder only";
    let inbox = root.join(ME);
    let refused = [
        other.clone(),                                        // a file in another session's inbox
        root.join(OTHER).join("20261027").join("999999_nobody_message.md"), // and one that is not there
        outside.clone(),
        PathBuf::from("/etc/passwd"),
        PathBuf::from("/"),
        root.clone(),
        root.join(format!("{ME}x")).join("20261027"),         // a folder whose name begins like the inbox's
        inbox.join("..").join(OTHER).join("20261027").join(other.file_name().unwrap()),
        inbox.join("20261027").join("..").join("..").join(OTHER),
        PathBuf::from(format!("../{OTHER}/20261027/{}", other.file_name().unwrap().to_str().unwrap())),
        PathBuf::from("20261027/../../outside.txt"),
        PathBuf::from(".."),
        PathBuf::from("/20261027"),
    ];
    for path in &refused {
        assert_eq!(answer(&root, ME, path, json!({})).unwrap_err(), said, "{}", path.display());
    }
    let mine = answer(&root, ME, &path, json!({})).unwrap();
    assert!(mine["text"].as_str().unwrap().ends_with("mine"));
    assert_eq!(answer(&root, OTHER, &other, json!({})).unwrap()["text"].as_str().unwrap().rsplit("\n\n").next(), Some("theirs"), "and the owner of that inbox reads it");
    assert_eq!(answer(&root, OTHER, &path, json!({})).unwrap_err(), said, "the same file, asked for by the other session");
}

#[test]
fn the_path_the_notice_gives_and_a_path_below_the_inbox_both_read_the_file() {
    let (_dir, root, path) = inbox_with("hello");
    let relative = path.strip_prefix(root.join(ME)).unwrap().to_path_buf();
    let whole = stored(&path);
    let text = |p: &Path| answer(&root, ME, p, json!({})).unwrap()["text"].as_str().unwrap().to_string();
    assert_eq!(text(&path), whole);
    assert_eq!(text(&relative), whole);
    assert_eq!(text(&Path::new("./").join(&relative)), whole);
    assert_eq!(text(Path::new(&format!("{}//{}", root.join(ME).display(), relative.display()))), whole, "a doubled slash is one");
    assert_eq!(answer(&root, ME, &root.join(ME), json!({})).unwrap_err(), "bad_request: that is a folder, not a file", "the inbox is not a file");
    assert_eq!(answer(&root, ME, Path::new("."), json!({})).unwrap_err(), "bad_request: that is a folder, not a file");
    assert_eq!(answer(&root, ME, relative.parent().unwrap(), json!({})).unwrap_err(), "bad_request: that is a folder, not a file");
}

#[test]
fn a_link_is_refused_at_every_hop_of_the_path() {
    let (dir, root, path) = inbox_with("mine");
    let said = "not_allowed: a link on that path is not followed";
    let secret = dir.0.join("secret.txt");
    std::fs::write(&secret, "secret").unwrap();
    let folder = path.parent().unwrap().to_path_buf();
    // the file itself is a link to a file outside
    symlink(&secret, folder.join("link_to_file.md")).unwrap();
    assert_eq!(answer(&root, ME, &folder.join("link_to_file.md"), json!({})).unwrap_err(), said);
    // a link to a file inside the inbox is a link as well
    symlink(&path, folder.join("link_inside.md")).unwrap();
    assert_eq!(answer(&root, ME, &folder.join("link_inside.md"), json!({})).unwrap_err(), said);
    // the date folder is a link to a folder outside that holds a file
    let elsewhere = dir.0.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::fs::write(elsewhere.join("note.md"), "elsewhere").unwrap();
    symlink(&elsewhere, root.join(ME).join("20261028")).unwrap();
    assert_eq!(answer(&root, ME, &root.join(ME).join("20261028").join("note.md"), json!({})).unwrap_err(), said);
    // a folder below the date folder is a link
    symlink(&elsewhere, folder.join("deeper")).unwrap();
    assert_eq!(answer(&root, ME, &folder.join("deeper").join("note.md"), json!({})).unwrap_err(), said);
    // the inbox itself is a link to another session's inbox
    let other = crate::handoff::store(&root, OTHER, "20261027", "101500", "s222222", "message", b"theirs").unwrap();
    let third = "s3333333333333333";
    symlink(root.join(OTHER), root.join(third)).unwrap();
    let through = root.join(third).join("20261027").join(other.file_name().unwrap());
    assert_eq!(answer(&root, third, &through, json!({})).unwrap_err(), said);
    // a link that points nowhere is a link too, not a missing file
    symlink(dir.0.join("nowhere"), folder.join("dangling.md")).unwrap();
    assert_eq!(answer(&root, ME, &folder.join("dangling.md"), json!({})).unwrap_err(), said);
    assert!(answer(&root, ME, &path, json!({})).is_ok(), "the file itself still reads");
}

#[test]
fn a_file_that_is_not_there_says_what_the_retention_rules_are_and_a_folder_or_a_pipe_is_not_read() {
    let (_dir, root, path) = inbox_with("mine");
    let folder = path.parent().unwrap().to_path_buf();
    let gone = answer(&root, ME, &folder.join("000000_nobody_message.md"), json!({})).unwrap_err();
    assert_eq!(gone, "not_found: there is no such file in your handoff folder; it may have been removed by the retention rules: files are kept for at most 30 days and 256 MiB in all");
    assert_eq!(answer(&root, ME, &folder.join("20260101").join("x.md"), json!({})).unwrap_err(), gone, "nothing below a folder that is not there");
    assert_eq!(answer(&root, ME, &path.join("below.md"), json!({})).unwrap_err(), gone, "nothing below a file");
    assert_eq!(answer(&root, "s4444444444444444", &folder.join("x.md"), json!({})).unwrap_err(), "not_allowed: read_handoff reads files in your own handoff folder only");
    assert_eq!(answer(&root, "s4444444444444444", Path::new("20261027/x.md"), json!({})).unwrap_err(), gone, "an inbox that was never made has no files");
    assert_eq!(answer(&root, ME, &folder, json!({})).unwrap_err(), "bad_request: that is a folder, not a file");
    let pipe = folder.join("pipe.md");
    assert!(std::process::Command::new("mkfifo").arg(&pipe).status().unwrap().success());
    let (root_for, pipe_for) = (root.clone(), pipe.clone());
    assert_eq!(soon(move || answer(&root_for, ME, &pipe_for, json!({}))).unwrap_err(), "not_allowed: that is not a plain file");
}

#[test]
fn the_file_that_is_opened_is_the_one_the_walk_saw_a_replacement_in_between_is_not_read() {
    let (dir, root, path) = inbox_with("mine");
    let names: Vec<OsString> = path.strip_prefix(root.join(ME)).unwrap().components().map(|c| c.as_os_str().to_os_string()).collect();
    let (at, seen) = walk(&root, ME, &names).unwrap();
    assert!(open_seen(&at, &seen).is_ok());
    // each replacement is made beside the file and renamed over it, so that its inode is not one the file gave back
    let swap = |make: &dyn Fn(&Path)| {
        let beside = at.with_file_name("beside");
        make(&beside);
        std::fs::rename(&beside, &at).unwrap();
    };
    swap(&|p| std::fs::write(p, "another file").unwrap());
    assert_eq!(open_seen(&at, &seen).err(), Some(Fault::Busy));
    // replaced by a link to a file outside
    let secret = dir.0.join("secret.txt");
    std::fs::write(&secret, "secret").unwrap();
    swap(&|p| symlink(&secret, p).unwrap());
    assert_eq!(open_seen(&at, &seen).err(), Some(Fault::Link));
    // replaced by a pipe: opened without waiting, and not a file
    swap(&|p| assert!(std::process::Command::new("mkfifo").arg(p).status().unwrap().success()));
    let (at_for, seen_for) = (at.clone(), seen.clone());
    assert_eq!(soon(move || open_seen(&at_for, &seen_for).err()), Some(Fault::Special));
    // removed
    std::fs::remove_file(&at).unwrap();
    assert_eq!(open_seen(&at, &seen).err(), Some(Fault::Gone));
}

#[test]
fn the_arguments_are_checked_before_any_file_is_looked_at() {
    let (_dir, root, path) = inbox_with("mine");
    let needs = |a: Value| read_in(&root, ME, a.as_object().unwrap()).unwrap_err().words();
    let said = "bad_request: read_handoff needs `path` (a file in your handoff folder, as a message from the host gave it) and may have `offset` and `limit`, whole numbers of bytes";
    for bad in [json!({}), json!({"path": ""}), json!({"path": 5}), json!({"path": null}), json!({"path": ["a"]}), json!({"path": "a\u{0}b"}), json!({"path": "x".repeat(PATH_MAX + 1)}), json!({"file": path.to_str().unwrap()})] {
        assert_eq!(needs(bad.clone()), said, "{bad}");
    }
    for caller in ["", "..", "a/b", "s1\n", "../x"] {
        assert_eq!(read_in(&root, caller, &arguments(&path, json!({}))).unwrap_err().words(), said, "{caller:?}");
    }
}

#[test]
fn the_answer_has_five_fields_and_the_log_line_names_the_session_and_the_result_and_never_the_path() {
    let (_dir, root, path) = inbox_with("mine");
    let got = answer(&root, ME, &path, json!({})).unwrap();
    let mut keys: Vec<&str> = got.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, ["end", "next", "offset", "size", "text"]);
    assert_eq!(audit_line(ME, "read"), format!("lico-host: call session={ME} tool=read_handoff result=read"));
    assert_eq!(audit_line(ME, "not_allowed"), format!("lico-host: call session={ME} tool=read_handoff result=not_allowed"));
}
