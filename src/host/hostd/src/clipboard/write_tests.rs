//! The command `lico clipboard write` without a host (class L0): the words, the limits, the request that is built and the way a host's reply is read.

use super::write::*;
use crate::handoff::Scratch;
use serde_json::json;
use std::os::unix::fs::PermissionsExt;

fn args(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| w.to_string()).collect()
}

fn usage_of(words: &[&str]) -> String {
    let e = parse_args(&args(words)).expect_err("a usage error");
    assert_eq!(e.reason, "usage", "{words:?}: {}", e.detail);
    e.detail
}

#[test]
fn exactly_one_source_and_a_timeout_from_one_to_the_ceiling() {
    let p = parse_args(&args(&["--text", "hi"])).unwrap();
    assert_eq!(p, Params { source: Source::Text("hi".into()), timeout_ms: 3000 });
    assert_eq!(parse_args(&args(&["--timeout-ms", "1", "--text", "x"])).unwrap().timeout_ms, 1);
    assert_eq!(parse_args(&args(&["--text", "x", "--timeout-ms", "600000"])).unwrap().timeout_ms, 600_000);
    assert_eq!(parse_args(&args(&["--image", "/tmp/a.png"])).unwrap().source, Source::Image("/tmp/a.png".into()));
    assert_eq!(parse_args(&args(&["--text-file", "/tmp/a.txt"])).unwrap().source, Source::TextFile("/tmp/a.txt".into()));
    // a relative name is made absolute against the working folder of the command
    match parse_args(&args(&["--image", "rel.png"])).unwrap().source {
        Source::Image(p) => assert!(p.is_absolute() && p.ends_with("rel.png")),
        other => panic!("{other:?}"),
    }
    for bad in [
        &[][..],
        &["--text"],
        &["--text", ""],
        &["--text", "a", "--text", "b"],
        &["--text", "a", "--image", "/x.png"],
        &["--text-file", "/a", "--image", "/b"],
        &["--image", ""],
        &["--text", "a", "--timeout-ms", "0"],
        &["--text", "a", "--timeout-ms", "600001"],
        &["--text", "a", "--timeout-ms", "-5"],
        &["--text", "a", "--timeout-ms", "1.5"],
        &["--text", "a", "--timeout-ms", "abc"],
        &["--text", "a", "--timeout-ms", ""],
        &["--text", "a", "--timeout-ms", "5", "--timeout-ms", "6"],
        &["--text", "a", "--bogus", "1"],
        &["--kind", "text"],
        &["hello"],
        &["--timeout-ms", "100"],
    ] {
        usage_of(bad);
    }
}

#[test]
fn a_text_is_limited_to_64_kib_and_one_byte_more_is_too_large() {
    let exact = "a".repeat(TEXT_MAX);
    let (request, kind) = build_request(&parse_args(&args(&["--text", &exact])).unwrap()).unwrap();
    assert_eq!(kind, "text");
    assert_eq!(request["text"].as_str().unwrap().len(), 65_536);
    assert_eq!(request["op"], "clipboard.write");
    assert_eq!(request["timeout_ms"], 3000);
    let over = "a".repeat(TEXT_MAX + 1);
    let e = build_request(&parse_args(&args(&["--text", &over])).unwrap()).expect_err("too large");
    assert_eq!(e.reason, "too_large");
    // the limit counts bytes, not characters: 21846 Korean letters are 65538 bytes
    let korean = "한".repeat(21_846);
    assert_eq!(korean.len(), 65_538);
    assert_eq!(build_request(&parse_args(&args(&["--text", &korean])).unwrap()).expect_err("bytes").reason, "too_large");
    assert!(build_request(&parse_args(&args(&["--text", &"한".repeat(21_845)])).unwrap()).is_ok());
}

#[test]
fn a_text_of_control_characters_that_the_json_escape_makes_too_long_for_the_host_is_too_large() {
    // 64 KiB of U+0001 is 6 bytes each in JSON: 393216 bytes, over the 262144 the host reads in one line
    let nasty = "\u{1}".repeat(TEXT_MAX);
    let e = build_request(&parse_args(&args(&["--text", &nasty])).unwrap()).expect_err("too long a line");
    assert_eq!(e.reason, "too_large");
    assert!(e.detail.contains("JSON escape"), "{}", e.detail);
    // 40000 of them is 240000 bytes plus the frame: fits
    assert!(build_request(&parse_args(&args(&["--text", &"\u{1}".repeat(40_000)])).unwrap()).is_ok());
}

#[test]
fn a_text_file_is_read_by_the_safe_open_with_the_same_limit() {
    let d = Scratch::new("write_textfile");
    let put = |name: &str, bytes: &[u8]| {
        let p = d.0.join(name);
        std::fs::write(&p, bytes).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).unwrap();
        p.to_str().unwrap().to_string()
    };
    let build = |path: &str| build_request(&parse_args(&args(&["--text-file", path])).unwrap());
    let (request, _) = build(&put("a.txt", "안녕 \u{1F600}\nline".as_bytes())).unwrap();
    assert_eq!(request["text"], "안녕 \u{1F600}\nline");
    assert_eq!(build(&put("exact.txt", &vec![b'x'; TEXT_MAX])).unwrap().0["text"].as_str().unwrap().len(), TEXT_MAX);
    assert_eq!(build(&put("over.txt", &vec![b'x'; TEXT_MAX + 1])).expect_err("too large").reason, "too_large");
    assert_eq!(build(&put("empty.txt", b"")).expect_err("empty").reason, "usage");
    assert_eq!(build(&put("bin.txt", &[0xff, 0xfe, 0x80])).expect_err("not UTF-8").reason, "write_failed");
    assert_eq!(build(d.0.join("missing").to_str().unwrap()).expect_err("missing").reason, "write_failed");
    assert_eq!(build(d.0.to_str().unwrap()).expect_err("a directory").reason, "write_failed");
    let link = d.0.join("link.txt");
    std::os::unix::fs::symlink(d.0.join("a.txt"), &link).unwrap();
    assert_eq!(build(link.to_str().unwrap()).expect_err("a link").reason, "write_failed");
}

#[test]
fn a_picture_travels_as_a_path_and_is_not_read_by_the_command() {
    let p = parse_args(&args(&["--image", "/does/not/exist.png", "--timeout-ms", "250"])).unwrap();
    let (request, kind) = build_request(&p).unwrap();
    assert_eq!(kind, "image");
    assert_eq!(request, json!({"op": "clipboard.write", "kind": "image", "path": "/does/not/exist.png", "timeout_ms": 250}));
}

#[test]
fn the_host_reply_is_read_into_one_answer_or_one_reason() {
    let ok = answer_of(&json!({"ok": 1, "bytes": 12, "kind": "text", "v": 1}), "text").unwrap();
    assert_eq!(ok, json!({"ok": 1, "kind": "text", "bytes": 12}));
    let ok = answer_of(&json!({"ok": 1, "bytes": 99, "width": 3, "height": 2}), "image").unwrap();
    assert_eq!(ok, json!({"ok": 1, "kind": "image", "bytes": 99, "width": 3, "height": 2}));
    for reason in ["no_window", "timeout", "too_large", "bad_image", "write_failed", "x_error", "unknown_op"] {
        let e = answer_of(&json!({"ok": 0, "error": reason}), "text").expect_err(reason);
        assert_eq!(e.reason, reason);
    }
    assert_eq!(answer_of(&json!({"ok": 0, "error": "something_new"}), "text").expect_err("other").reason, "bad_request");
    assert_eq!(answer_of(&json!({"ok": 0}), "text").expect_err("none").reason, "bad_request");
}
