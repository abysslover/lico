//! The unit tests of the window's browser module (`browser_view.rs`): its pure parts and the one call that reads a watch answer.

use super::browser_view::{call_watch, caption, page_rect, BTab};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::sync::Arc;

fn tab(row: Value, local: bool) -> BTab {
    BTab { id: "b1".into(), local, row, seq: 0, frame_seq: 0, png: Vec::new(), image: None, decoded_seq: 0, watching: false, rearm: false, sent: None, inflight: false, inbox: Arc::default(), painted: 0, put_url: String::new(), waiting: false }
}

#[test]
fn a_caption_is_the_title_or_the_host_marks_the_local_tab_loading_and_notices_and_is_short() {
    assert_eq!(caption(&tab(json!({"title": "A page", "url": "https://a.test/x"}), false)), "\u{1f310} A page");
    assert_eq!(caption(&tab(json!({"title": "", "url": "https://a.test/x"}), false)), "\u{1f310} a.test/x");
    assert!(caption(&tab(json!({"title": "T", "loading": true}), false)).ends_with(" \u{25cf}"));
    assert!(caption(&tab(json!({"title": "T", "notice": "n"}), false)).ends_with(" !"));
    assert!(caption(&tab(json!({"title": "T"}), true)).contains("\u{b7}L "), "a local tab is marked");
    let long = caption(&tab(json!({"title": "x".repeat(80)}), false));
    assert!(long.chars().count() <= 26 && long.contains('\u{2026}'), "{long}");
    assert!(caption(&tab(json!({}), false)).contains("b1"), "no title and no address: the tab's id");
}

#[test]
fn the_page_area_runs_from_the_header_to_the_prompt_and_is_never_negative() {
    assert_eq!(page_rect(10.0, 88.0, 700.0, 1200), [10.0, 88.0, 1200.0, 700.0]);
    assert_eq!(page_rect(10.0, 88.0, 50.0, 1200), [10.0, 88.0, 1200.0, 88.0], "a prompt above the header leaves no area");
}

fn serve_one(reply: String) -> (String, std::thread::JoinHandle<String>) {
    let dir = std::env::temp_dir().join(format!("lico_bv_{}", crate::core::random_hex(6)));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("s.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let handle = std::thread::spawn(move || {
        let (mut c, _) = listener.accept().unwrap();
        let mut buf = [0u8; 1024];
        let n = c.read(&mut buf).unwrap();
        let _ = writeln!(c, "{reply}");
        String::from_utf8_lossy(&buf[..n]).to_string()
    });
    (path.to_string_lossy().into_owned(), handle)
}

#[test]
fn a_watch_answer_gives_the_frame_as_bytes_and_the_reply_without_the_text_of_it() {
    let png = vec![137u8, 80, 78, 71, 1, 2, 3, 250];
    let (socket, server) = serve_one(json!({"ok": 1, "seq": 7, "frame_seq": 4, "png": crate::base64::base64_encode(&png), "tab": {"url": "u"}}).to_string());
    let (reply, frame) = call_watch(&socket, json!({"op": "browser.watch", "tab": "b1", "seq": 0}));
    assert!(server.join().unwrap().contains("browser.watch"));
    assert_eq!((reply["seq"].clone(), reply.get("png").is_none()), (json!(7), true), "the text of the frame is not kept in the reply: {reply}");
    assert_eq!(frame, Some((4, png)));
    let (socket, server) = serve_one(json!({"ok": 1, "hidden": true}).to_string());
    let (reply, frame) = call_watch(&socket, json!({"op": "browser.watch"}));
    server.join().unwrap();
    assert_eq!((reply["hidden"].clone(), frame), (json!(true), None));
    let (reply, frame) = call_watch("/nonexistent/socket", json!({}));
    assert_eq!((reply["ok"].clone(), reply["error"].clone(), frame), (json!(0), json!("host_unreachable"), None));
    let (socket, server) = serve_one("not json".into());
    let (reply, _) = call_watch(&socket, json!({}));
    server.join().unwrap();
    assert_eq!(reply["error"], json!("host_unreachable"), "a garbled line is a failure, not a frame");
}
