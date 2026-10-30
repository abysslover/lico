//! Tests of the agent tools `capture`, `clipboard_read` and `clipboard_write` (plan M02-P04, S07a), without X: a fake job program stands for the helper, the
//! person's yes is the socket op's own function (`core::approval_answer`). The real display, the real helper and the real window are in `tests/test_agent_tools.py`.

use super::*;
use crate::mcp::Block;
use crate::transfer::tests::{world, World, ONE};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Mutex as StdMutex;

/// The tests that do a work share the host-wide lock of the work; they take this one first, so that none of them meets another's `busy`.
static SERIAL: StdMutex<()> = StdMutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

/// The first 33 bytes of a PNG of 640 x 480: all the host looks at.
const PNG_HEAD: &str = r"\211PNG\r\n\032\n\0\0\0\rIHDR\0\0\002\200\0\0\001\340\010\006\0\0\0\0\0\0\0";

/// A host with one Claude session and a fake program for `lico xjob`: `mode` (a file beside the captures folder) says what it does.
struct Room {
    w: World,
    bin: PathBuf,
    held: Arc<AtomicUsize>,
}

fn room() -> Room {
    let mut w = world(&[ONE]);
    w.with(ONE, |s| s.provider = "claude");
    let root = w.dir.0.clone();
    let bin = root.join("fake_lico");
    Arc::get_mut(&mut w.shared).expect("the world is alone").cfg.job_bin = bin.clone(); // the helper that `perform` itself would start is the fake one too: a yes that works would show
    let script = format!(
        r#"#!/bin/sh
# fake `lico xjob JOB ARGS`: the last argument is the --out path for a capture and for an image read; the mode file is beside the captures folder
root="{root}"
echo "$@" >> "$root/runs"
mode=$(cat "$root/mode")
case "$mode" in
  fail) echo '{{"ok":0,"error":"no_display","detail":"cannot open the display"}}'; exit 2 ;;
  text) echo '{{"ok":1,"kind":"text","selection":"clipboard","text":"hello from the clipboard","bytes":24}}'; exit 0 ;;
esac
for last; do :; done
printf '{head}' > "$last"
case "$2" in
  clipboard_read) echo '{{"ok":1,"kind":"image","selection":"clipboard","path":"'"$last"'","bytes":33,"width":640,"height":480}}' ;;
  *) echo '{{"ok":1,"path":"'"$last"'","width":640,"height":480,"bytes":33,"offset":{{"x":0,"y":0}},"server":{{"vendor":"fake","release":1}},"agent":{{"path":"'"$last"'","width":640,"height":480,"bytes":33,"scale":1.0}}}}' ;;
esac
"#,
        root = root.display(),
        head = PNG_HEAD
    );
    std::fs::write(&bin, script).unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::create_dir_all(w.dir.0.join("handoff")).unwrap();
    prepare(&w.shared.cfg.handoff_dir).unwrap();
    let r = Room { w, bin, held: Arc::default() };
    r.mode("ok");
    r
}

impl Room {
    fn mode(&self, what: &str) {
        std::fs::write(self.w.dir.0.join("mode"), what).unwrap();
    }
    fn runs(&self) -> usize {
        std::fs::read_to_string(self.w.dir.0.join("runs")).map_or(0, |t| t.lines().count())
    }
    fn captures(&self) -> usize {
        std::fs::read_dir(work::captures_dir(&self.w.shared.cfg.handoff_dir)).unwrap().count()
    }
    fn args(pairs: &[(&str, &str)]) -> Map<String, Value> {
        pairs.iter().map(|(k, v)| (k.to_string(), json!(v))).collect()
    }
    fn capture(&self, pairs: &[(&str, &str)]) -> Result<Answer, String> {
        two_calls(&self.w.shared, ONE, &Room::args(pairs), capture_spec, &Mutex::default(), Instant::now(), &self.held, &self.bin)
    }
    fn read(&self, pairs: &[(&str, &str)]) -> Result<Answer, String> {
        two_calls(&self.w.shared, ONE, &Room::args(pairs), read_spec, &Mutex::default(), Instant::now(), &self.held, &self.bin)
    }
    fn write(&self, text: &str) -> Result<Answer, String> {
        write_call(&self.w.shared, ONE, &Room::args(&[("text", text)]), &Mutex::default(), Instant::now(), &self.held)
    }
    fn decide(&self, id: i64, what: &str, keep: bool) -> crate::core::Reply {
        let generation = self.w.with(ONE, |s| s.generation as i64);
        crate::core::approval_answer(&self.w.shared, ONE, generation, id, what, keep)
    }
    fn notices(&self) -> Vec<String> {
        std::iter::from_fn(|| self.w.delivered(ONE)).collect()
    }
    fn waiting(&self) -> usize {
        let all = crate::core::status(&self.w.shared).unwrap();
        let row = all["sessions"].as_array().unwrap().iter().find(|s| s["session"] == ONE).unwrap().clone();
        row["approvals"].as_array().map_or(0, Vec::len)
    }
    fn grants(&self) -> usize {
        self.w.with(ONE, |s| s.pending.grants.len())
    }
}

fn text_of(answer: Answer) -> Value {
    match answer {
        Answer::Text(t) => serde_json::from_str(&t).unwrap(),
        other => panic!("a text was expected: {other:?}"),
    }
}

fn pending_id(got: Result<Answer, String>) -> i64 {
    let v = text_of(got.unwrap());
    assert_eq!(v["status"], "pending", "{v}");
    v["approval_id"].as_i64().unwrap()
}

fn blocks_of(got: Result<Answer, String>) -> Vec<Block> {
    match got.unwrap() {
        Answer::Blocks(b) => b,
        other => panic!("blocks were expected: {other:?}"),
    }
}

#[test]
fn a_first_call_asks_and_captures_nothing_and_the_same_call_again_is_already_pending() {
    let _one = serial();
    let r = room();
    let v = text_of(r.capture(&[]).unwrap());
    let keys: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!((v["status"].as_str(), keys), (Some("pending"), vec!["approval_id", "message", "status"]));
    assert!(v["message"].as_str().unwrap().contains("Nothing was captured yet"));
    assert_eq!((r.runs(), r.captures(), r.grants(), r.waiting()), (0, 0, 0, 1), "no helper, no file, no grant: only the card");
    let again = text_of(r.capture(&[]).unwrap());
    assert_eq!((again["approval_id"].clone(), again["already_pending"].clone()), (v["approval_id"].clone(), json!(true)));
    assert_eq!(r.waiting(), 1);
}

#[test]
fn the_yes_stores_one_grant_and_the_second_call_does_the_work_once_and_leaves_no_file() {
    let _one = serial();
    let r = room();
    let id = pending_id(r.capture(&[("target", "screen")]));
    r.decide(id, "allow", false).unwrap();
    assert_eq!((r.runs(), r.grants()), (0, 1), "the yes captures nothing");
    let notices = r.notices().join("\n");
    assert!(notices.contains("approved: ") && notices.contains("allowed once") && notices.contains("within 10 minutes"), "{notices}");
    let blocks = blocks_of(r.capture(&[]));
    assert_eq!(blocks.len(), 2);
    let Block::Text(json_text) = &blocks[0] else { panic!("the text comes first") };
    let shown: Value = serde_json::from_str(json_text).unwrap();
    assert_eq!((shown["kind"].clone(), shown["width"].clone(), shown["height"].clone(), shown["bytes"].clone(), shown["shrunk"].clone()), (json!("screen"), json!(640), json!(480), json!(33), json!(false)));
    assert!(!json_text.contains("path") && !json_text.contains(r.w.dir.0.to_str().unwrap()), "the agent never gets a path: {json_text}");
    let Block::Image { data, mime } = &blocks[1] else { panic!("the picture comes second") };
    assert_eq!(*mime, "image/png");
    assert_eq!(data, &crate::pasted::base64::encode(&decode_head()));
    assert_eq!((r.runs(), r.captures(), r.grants()), (1, 0, 0), "one helper, the folder is empty again, the grant is used up");
    let third = text_of(r.capture(&[]).unwrap());
    assert_eq!(third["status"], "pending", "a third call asks again");
}

fn decode_head() -> Vec<u8> {
    let mut v = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R', 0, 0, 2, 0x80, 0, 0, 1, 0xE0, 8, 6, 0, 0, 0];
    v.extend([0u8; 4]);
    v
}

#[test]
fn a_different_target_needs_its_own_yes_and_does_not_use_up_the_grant_of_another() {
    let _one = serial();
    let r = room();
    let id = pending_id(r.capture(&[]));
    r.decide(id, "allow", false).unwrap();
    let other = text_of(r.capture(&[("target", "window:5")]).unwrap());
    assert_eq!(other["status"], "pending", "a window is not the screen");
    assert_eq!(r.grants(), 1, "and the grant of the screen is still there");
    let region = text_of(r.capture(&[("target", "region:1,2,30,40")]).unwrap());
    assert_eq!(region["status"], "pending");
    assert_eq!(blocks_of(r.capture(&[])).len(), 2, "the screen is still allowed");
}

#[test]
fn a_grant_older_than_600_seconds_asks_again_by_the_injected_clock_and_a_decline_gives_none() {
    let _one = serial();
    let r = room();
    assert!(give_grant(&r.w.shared, ONE, "capture", "screen", pending::now() - 601));
    assert_eq!(text_of(r.capture(&[]).unwrap())["status"], "pending", "the yes of 601 seconds ago is dead");
    assert_eq!(r.runs(), 0);
    let id = pending_id(r.capture(&[("target", "window:7")]));
    r.decide(id, "deny", false).unwrap();
    assert!(r.notices().join("\n").contains("declined"));
    assert_eq!(text_of(r.capture(&[("target", "window:7")]).unwrap())["status"], "pending", "a no gives no grant");
}

#[test]
fn a_failed_capture_uses_up_the_grant_and_says_so_and_the_next_call_asks_again() {
    let _one = serial();
    let r = room();
    let id = pending_id(r.capture(&[]));
    r.decide(id, "allow", false).unwrap();
    r.mode("fail");
    let refused = r.capture(&[]).unwrap_err();
    assert!(refused.starts_with("no_display: ") && refused.contains("yes is used up") && refused.contains("asked again"), "{refused}");
    assert_eq!((r.grants(), r.captures()), (0, 0));
    assert_eq!(text_of(r.capture(&[]).unwrap())["status"], "pending");
}

#[test]
fn a_bad_argument_is_refused_at_once_with_the_usage_and_nothing_is_asked() {
    let _one = serial();
    let r = room();
    for pairs in [&[("target", "bogus")][..], &[("target", "region:1,2,0,5")], &[("target", "region:-1,2,5,5")], &[("target", "window:")], &[("target", "")], &[("target", "region:1,2,3")]] {
        let e = r.capture(pairs).unwrap_err();
        assert!(e.starts_with("bad_request: capture takes"), "{pairs:?}: {e}");
    }
    let e = two_calls(&r.w.shared, ONE, json!({"target": 5}).as_object().unwrap(), capture_spec, &Mutex::default(), Instant::now(), &r.held, &r.bin).unwrap_err();
    assert!(e.starts_with("bad_request"));
    assert!(r.read(&[("kind", "both")]).unwrap_err().starts_with("bad_request: clipboard_read takes"));
    assert_eq!((r.waiting(), r.runs()), (0, 0));
}

#[test]
fn only_one_work_at_a_time_and_a_busy_call_keeps_the_grant() {
    let _one = serial();
    let r = room();
    let id = pending_id(r.capture(&[]));
    r.decide(id, "allow", false).unwrap();
    let held = work::try_lock().unwrap();
    let e = r.capture(&[]).unwrap_err();
    assert!(e.starts_with("busy: "), "{e}");
    assert_eq!(r.grants(), 1, "busy used nothing up");
    drop(held);
    assert_eq!(blocks_of(r.capture(&[])).len(), 2);
}

#[test]
fn a_clipboard_text_is_one_text_block_with_the_note_and_each_read_needs_its_own_yes() {
    let _one = serial();
    let r = room();
    r.mode("text");
    let id = pending_id(r.read(&[("kind", "text")]));
    r.decide(id, "allow", false).unwrap();
    let blocks = blocks_of(r.read(&[("kind", "text")]));
    assert_eq!(blocks.len(), 1);
    let Block::Text(t) = &blocks[0] else { panic!() };
    let v: Value = serde_json::from_str(t).unwrap();
    assert_eq!(v, json!({"kind": "text", "text": "hello from the clipboard", "note": work::TEXT_NOTE}));
    assert_eq!(text_of(r.read(&[("kind", "text")]).unwrap())["status"], "pending");
    assert_eq!(text_of(r.read(&[("kind", "image")]).unwrap())["status"], "pending", "another kind, another yes");
}

#[test]
fn a_clipboard_picture_is_json_and_an_image_block_with_the_clipboards_own_bytes() {
    let _one = serial();
    let r = room();
    let id = pending_id(r.read(&[]));
    r.decide(id, "allow", false).unwrap();
    r.mode("ok");
    let blocks = blocks_of(r.read(&[]));
    assert_eq!(blocks.len(), 2);
    let Block::Text(t) = &blocks[0] else { panic!() };
    assert_eq!(serde_json::from_str::<Value>(t).unwrap(), json!({"kind": "image", "width": 640, "height": 480, "bytes": 33}));
    let Block::Image { mime, .. } = &blocks[1] else { panic!() };
    assert_eq!(*mime, "image/png");
    assert_eq!(r.captures(), 0);
}

#[test]
fn the_grants_of_a_session_are_dropped_with_it_when_it_is_forgotten() {
    let _one = serial();
    let r = room();
    let id = pending_id(r.capture(&[]));
    r.decide(id, "allow", false).unwrap();
    assert_eq!(r.grants(), 1);
    r.w.with(ONE, |s| s.drop_requests()); // what the session does when it ended for good
    assert_eq!(r.grants(), 0, "a yes does not outlive the session it was given to");
    assert_eq!(text_of(r.capture(&[]).unwrap())["status"], "pending");
}

#[test]
fn nothing_but_a_yes_is_kept_for_the_three_tools() {
    let _one = serial();
    let r = room();
    let id = pending_id(r.capture(&[]));
    assert_eq!(r.decide(id, "allow", true), Err("bad_request"));
    assert_eq!(r.waiting(), 1, "and the request is still there");
    let id = pending_id(r.read(&[]));
    assert_eq!(r.decide(id, "allow", true), Err("bad_request"));
}

fn switch(r: &Room, on: bool) {
    let path = crate::clipboard::settings::settings_path(&r.w.shared.cfg.handoff_dir);
    std::fs::write(&path, format!("{{\"clipboard_write\": {on}}}")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
}

#[test]
fn a_write_is_refused_too_large_never_cut_and_asks_with_the_text_card_and_the_text_in_pages() {
    let r = room();
    let e = r.write(&"x".repeat(TEXT_MAX + 1)).unwrap_err();
    assert!(e.starts_with("too_large: ") && e.contains("not cut"), "{e}");
    assert_eq!(r.waiting(), 0);
    assert!(r.write("").unwrap_err().starts_with("bad_request"));
    let id = pending_id(r.write("hello there"));
    let all = crate::core::status(&r.w.shared).unwrap();
    let row = all["sessions"].as_array().unwrap().iter().find(|s| s["session"] == ONE).unwrap().clone();
    assert!(row["approvals"].to_string().contains("write 11 bytes to your clipboard"), "{row}");
    let page = crate::pending::text_page(&r.w.shared, ONE, r.w.with(ONE, |s| s.generation as i64), id, 0, 100).unwrap();
    assert_eq!(page["text"], "hello there");
    let exact = r.write(&"y".repeat(TEXT_MAX));
    assert!(exact.is_ok(), "exactly the maximum is allowed");
}

#[test]
fn a_yes_after_the_setting_was_switched_off_writes_nothing_and_says_not_allowed() {
    let r = room();
    switch(&r, true);
    let id = pending_id(r.write("hello"));
    switch(&r, false);
    r.decide(id, "allow", false).unwrap();
    let notices = r.notices().join("\n");
    assert!(notices.contains("not done: not_allowed: clipboard writing was switched off"), "{notices}");
}

#[test]
fn a_yes_with_no_window_open_says_no_window_in_the_notice_at_once() {
    let r = room();
    switch(&r, true);
    let id = pending_id(r.write("hello"));
    r.decide(id, "allow", false).unwrap();
    let notices = r.notices().join("\n");
    assert!(notices.contains("not done: no_window: "), "{notices}");
}
