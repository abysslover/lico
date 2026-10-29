//! The clipboard reader without a display (class L0): the arguments, the choice of a target, the PNG header, the count of an incremental transfer,
//! and the safe writer of the picture. The reader against a real X server is `tests_x`; the real command is `tests/test_clipboard_read.py`.

use super::job::{parse_args, save, Params, IMAGE_DEFAULT, MAX_BYTES_CEILING, TEXT_DEFAULT, TIMEOUT_CEILING_MS, TIMEOUT_DEFAULT_MS};
use super::*;
use crate::handoff::{mode_of, Scratch};
use std::path::Path;

fn args(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| w.to_string()).collect()
}

fn reason(words: &[&str]) -> &'static str {
    parse_args(&args(words)).expect_err(&format!("{words:?}")).reason
}

// ---- the arguments ---------------------------------------------------------------------------------------------------------

#[test]
fn the_defaults_are_the_clipboard_as_text_in_64_kib_within_two_seconds() {
    let p = parse_args(&[]).unwrap();
    assert_eq!(p, Params { selection: Selection::Clipboard, want: Want::Text, out: None, max_bytes: None, timeout_ms: 2000 });
    let l = p.limits();
    assert_eq!((l.text_max, l.image_max, l.deadline), (65536, 3_145_728, Duration::from_millis(2000)));
    assert_eq!((TEXT_DEFAULT, IMAGE_DEFAULT, TIMEOUT_DEFAULT_MS), (65536, 3_145_728, 2000));
}

#[test]
fn every_flag_is_read_and_an_explicit_limit_is_the_limit_of_both_kinds() {
    let p = parse_args(&args(&["--selection", "primary", "--kind", "auto", "--out", "/CANARY-x/p.png", "--max-bytes", "1234", "--timeout-ms", "77"])).unwrap();
    assert_eq!((p.selection, p.want, p.max_bytes, p.timeout_ms), (Selection::Primary, Want::Auto, Some(1234), 77));
    assert_eq!(p.out.as_deref(), Some(Path::new("/CANARY-x/p.png")));
    let l = p.limits();
    assert_eq!((l.text_max, l.image_max, l.deadline), (1234, 1234, Duration::from_millis(77)));
    assert_eq!(parse_args(&args(&["--kind", "image", "--out", "p.png"])).unwrap().want, Want::Image);
    // a relative --out is made absolute: the answer carries an absolute path
    assert!(parse_args(&args(&["--kind", "image", "--out", "p.png"])).unwrap().out.unwrap().is_absolute());
}

#[test]
fn bad_arguments_are_usage_errors() {
    for words in [
        &["--selection"][..],                                      // a missing value, each flag
        &["--kind"],
        &["--out"],
        &["--max-bytes"],
        &["--timeout-ms"],
        &["--selection", "secondary"],
        &["--selection", "CLIPBOARD"],
        &["--kind", "png"],
        &["--kind", "image"],                                      // an image needs --out
        &["--kind", "auto"],
        &["--out", "x.png"],                                       // a text has no file
        &["--kind", "text", "--out", "x.png"],
        &["--kind", "image", "--out", ""],
        &["--selection", "primary", "--selection", "primary"],     // each flag doubled
        &["--kind", "text", "--kind", "text"],
        &["--kind", "image", "--out", "a.png", "--out", "b.png"],
        &["--max-bytes", "5", "--max-bytes", "5"],
        &["--timeout-ms", "5", "--timeout-ms", "5"],
        &["--max-bytes", "0"],
        &["--max-bytes", "-1"],
        &["--max-bytes", "1e3"],
        &["--max-bytes", "12 "],
        &["--max-bytes", "+5"],
        &["--max-bytes", "67108865"],                               // over the ceiling
        &["--max-bytes", "99999999999999999999999"],
        &["--timeout-ms", "0"],
        &["--timeout-ms", "600001"],
        &["--timeout-ms", "abc"],
        &["--bogus"],
        &["clipboard"],
        &["--selection", "primary", "extra"],
    ] {
        assert_eq!(reason(words), "usage", "{words:?}");
    }
    // the ceilings themselves are accepted
    assert!(parse_args(&args(&["--max-bytes", &MAX_BYTES_CEILING.to_string(), "--timeout-ms", &TIMEOUT_CEILING_MS.to_string()])).is_ok());
    assert!(parse_args(&args(&["--max-bytes", "1", "--timeout-ms", "1"])).is_ok());
}

// ---- the choice of a target -------------------------------------------------------------------------------------------------

const ATOMS: Atoms = Atoms { utf8: 300, png: 301 };
const JPEG: Atom = 400;
const HTML: Atom = 401;
const TARGETS: Atom = 402;

#[test]
fn the_target_is_picked_from_what_the_owner_offers() {
    let p = |want, offered: &[Atom]| pick(want, offered, &ATOMS).map_err(|e| e.reason);
    // text: UTF8_STRING first, STRING when that is all there is
    assert_eq!(p(Want::Text, &[TARGETS, 300, 31]), Ok(Pick::Text(300)));
    assert_eq!(p(Want::Text, &[TARGETS, 31]), Ok(Pick::Text(31)));
    assert_eq!(p(Want::Text, &[31, 300]), Ok(Pick::Text(300)));
    // an image is image/png only
    assert_eq!(p(Want::Image, &[TARGETS, 300, 301]), Ok(Pick::Png));
    assert_eq!(p(Want::Image, &[TARGETS, JPEG]), Err("unsupported_format"));
    assert_eq!(p(Want::Image, &[TARGETS, 300, 31]), Err("unsupported_format"));    // text only, an image wanted
    assert_eq!(p(Want::Text, &[TARGETS, HTML]), Err("unsupported_format"));
    assert_eq!(p(Want::Text, &[TARGETS, 301]), Err("unsupported_format"));          // a picture only, a text wanted
    // auto: the picture when it is offered, else the text, else nothing
    assert_eq!(p(Want::Auto, &[TARGETS, 300, 301]), Ok(Pick::Png));
    assert_eq!(p(Want::Auto, &[301, 300]), Ok(Pick::Png));
    assert_eq!(p(Want::Auto, &[TARGETS, 300]), Ok(Pick::Text(300)));
    assert_eq!(p(Want::Auto, &[TARGETS, 31]), Ok(Pick::Text(31)));
    assert_eq!(p(Want::Auto, &[TARGETS, JPEG, HTML]), Err("unsupported_format"));
    assert_eq!(p(Want::Text, &[]), Err("unsupported_format"));
}

// ---- the PNG header ----------------------------------------------------------------------------------------------------------

/// The 33 bytes a reader looks at, with any width and height (a forged header: nothing after it).
pub(super) fn head(w: u32, h: u32) -> Vec<u8> {
    let mut v = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
    v.extend_from_slice(b"IHDR");
    v.extend_from_slice(&w.to_be_bytes());
    v.extend_from_slice(&h.to_be_bytes());
    v.extend_from_slice(&[8, 2, 0, 0, 0, 0, 0, 0, 0]);
    v
}

fn size(data: &[u8]) -> Result<(u32, u32), &'static str> {
    png_size(data).map_err(|e| e.reason)
}

#[test]
fn the_size_comes_from_the_header_and_nothing_else_is_a_png() {
    assert_eq!(size(&head(1, 1)), Ok((1, 1)));
    assert_eq!(size(&head(7, 9)), Ok((7, 9)));
    assert_eq!(size(&head(8000, 8000)), Ok((8000, 8000)));
    assert_eq!(size(&head(8000, 1)), Ok((8000, 1)));
    // over 8000 pixels on a side: refused as too large (never cut), whichever side
    assert_eq!(size(&head(8001, 5)), Err("too_large"));
    assert_eq!(size(&head(5, 8001)), Err("too_large"));
    assert_eq!(size(&head(9000, 20)), Err("too_large"));
    assert_eq!(size(&head(u32::MAX, u32::MAX)), Err("too_large"));
    // a side of 0, a wrong signature, a wrong chunk, a short head: not a PNG
    assert_eq!(size(&head(0, 5)), Err("bad_image"));
    assert_eq!(size(&head(5, 0)), Err("bad_image"));
    assert_eq!(size(&head(0, 0)), Err("bad_image"));
    for bad in 0..16 {
        let mut v = head(5, 5);
        v[bad] ^= 0x01; // one flipped bit in the signature, the length or the type of the first chunk
        assert_eq!(size(&v), Err("bad_image"), "byte {bad}");
    }
    assert_eq!(size(&head(5, 5)[..32]), Err("bad_image"));
    assert_eq!(size(&[]), Err("bad_image"));
    assert_eq!(size(b"\xFF\xD8\xFF\xE0 this is a JPEG, not a PNG, whatever the owner says it is"), Err("bad_image"));
    assert_eq!(size(&vec![b'x'; 5000]), Err("bad_image"));
    // a real picture written by our own encoder
    let mut enc = crate::png::PngEncoder::new(Vec::new(), 13, 4).unwrap();
    for _ in 0..4 {
        enc.push_row(&[7u8; 39]).unwrap();
    }
    assert_eq!(size(&enc.finish().unwrap()), Ok((13, 4)));
}

// ---- the count of an incremental transfer -----------------------------------------------------------------------------------

#[test]
fn the_total_of_the_chunks_is_counted_against_the_limit() {
    let mut c = Collector::new(10);
    c.push(&[1; 4]).unwrap();
    c.push(&[]).unwrap();
    c.push(&[2; 6]).unwrap(); // 4 + 6 = 10: exactly the limit is accepted
    assert_eq!(c.into_bytes().len(), 10);
    let mut c = Collector::new(10);
    c.push(&[1; 4]).unwrap();
    assert_eq!(c.push(&[2; 7]).unwrap_err().reason, "too_large"); // 4 + 7 = 11: one over, though each chunk is under the limit
    let mut c = Collector::new(10);
    assert_eq!(c.push(&[0; 11]).unwrap_err().reason, "too_large");
    let mut c = Collector::new(10);
    for _ in 0..10 {
        c.push(&[9]).unwrap();
    }
    assert_eq!(c.push(&[9]).unwrap_err().reason, "too_large"); // many small chunks add up
}

// ---- the picture file ------------------------------------------------------------------------------------------------------------

#[test]
fn the_picture_is_written_whole_with_mode_0600_and_nothing_is_left_after_a_failure() {
    let dir = Scratch::new("clipsave");
    let dest = dir.0.join("p.png");
    save(&dest, &head(3, 3)).unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), head(3, 3));
    assert_eq!(mode_of(&dest), 0o600);
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 1, "no temporary file stays");
    // a symbolic link at the name is replaced, never written through
    let target = dir.0.join("target.txt");
    std::fs::write(&target, b"CANARY-untouched").unwrap();
    let link = dir.0.join("link.png");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    save(&link, &head(4, 4)).unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"CANARY-untouched");
    assert_eq!(std::fs::read(&link).unwrap(), head(4, 4));
    assert!(!std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
    // the folder must exist: no folder is made, the reason is write_failed
    let missing = dir.0.join("no-such-folder").join("p.png");
    assert_eq!(save(&missing, &head(3, 3)).unwrap_err().reason, "write_failed");
    assert!(!dir.0.join("no-such-folder").exists());
    // a folder at the name cannot be replaced by a file: write_failed, and no temporary file is left
    let folder = dir.0.join("folder.png");
    std::fs::create_dir(&folder).unwrap();
    assert_eq!(save(&folder, &head(3, 3)).unwrap_err().reason, "write_failed");
    let mut names: Vec<String> = std::fs::read_dir(&dir.0).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    assert_eq!(names, ["folder.png", "link.png", "p.png", "target.txt"]);
}

#[test]
fn the_line_has_no_character_a_line_splitter_would_cut_at() {
    let text = format!("a{}b{}c{}d\ne\x0bf\x0cg\x1ch", char::from_u32(0x85).unwrap(), char::from_u32(0x2028).unwrap(), char::from_u32(0x2029).unwrap());
    let line = super::job::one_line(&serde_json::json!({"ok": 1, "text": text}));
    assert!(!line.contains(|c: char| matches!(c as u32, 0x85 | 0x2028 | 0x2029 | 0x0A | 0x0B | 0x0C | 0x1C)), "{line}");
    assert_eq!(serde_json::from_str::<serde_json::Value>(&line).unwrap()["text"], text.as_str(), "the escapes mean the same characters");
}
