//! The window side of a pasted picture, below the level of a real window (plan M02-P04, S05b): the store of chips, the paste thread's function against owners of our
//! own on a private X server (a text AND a picture: the text wins), and the whole flow of Enter, the slot, the write and the answers with the window's end of a
//! socket pair as the host. The real window, a real host and `xclip` are in `tests/test_picture_paste.py`.

use std::io::Read as _;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::Path;

use serde_json::{json, Value};

use super::super::scroll_oracle::app;
use super::super::ui::Row;
use super::*;
use crate::clipboard::tests_x::{fake_png, serve, Owner, Xvfb};
use crate::handoff::Scratch;

const MIB: usize = 1024 * 1024;
const NAME: &str = "0123456789abcdef0123456789abcdef.png";

fn pic(bytes: usize) -> Picture {
    Picture { bytes: vec![7; bytes], width: 64, height: 48 }
}

fn row(id: &str, provider: &str, state: &str, generation: i64) -> Row {
    Row { session: id.into(), name: id.into(), provider: provider.into(), state: state.into(), generation, ..Default::default() }
}

/// `s-claude` (running, generation 5) is open; `s-two` is a second Claude session, `s-codex` a Codex one, `s-old` is disconnected and `s-gone` has ended.
fn window() -> (App, UnixStream) {
    let mut app = app(1.0, false);
    app.ui.rows = vec![row("s-claude", "claude", "running", 5), row("s-two", "claude", "running", 1), row("s-codex", "codex", "running", 1), row("s-old", "claude", "disconnected", 2), row("s-gone", "claude", "exited", 2)];
    app.session = "s-claude".into();
    app.generation = 5;
    let (ours, host) = UnixStream::pair().unwrap();
    host.set_nonblocking(true).unwrap();
    app.sock = ours;
    (app, host)
}

fn asked(host: &mut UnixStream) -> Vec<Value> {
    let (mut text, mut buffer) = (String::new(), [0u8; 65536]);
    while let Ok(n @ 1..) = host.read(&mut buffer) {
        text.push_str(std::str::from_utf8(&buffer[..n]).unwrap());
    }
    text.lines().map(|l| serde_json::from_str(l).unwrap()).collect()
}

fn note(app: &App) -> Option<String> {
    app.ed.dump(&app.session)["note"].as_str().map(String::from)
}

fn png_read(bytes: usize, w: u32, h: u32) -> Result<Read, ClipError> {
    Ok(Read { clip: Clip::Png { bytes: vec![9; bytes], width: w, height: h }, incr_chunks: 0 })
}

fn paste(app: &mut App, bytes: usize) {
    app.ui.paste_pending = true;
    app.on_paste_answer(Pasted::Picture(png_read(bytes, 64, 48)));
}

fn type_and_enter(app: &mut App, text: &str) {
    app.ed.set(text.to_string());
    app.send_draft();
}

/// The slot id of the one `picture.slot` the window asked for.
fn slot_id(lines: &[Value]) -> String {
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0]["op"], "picture.slot", "{lines:?}");
    lines[0]["id"].as_str().unwrap().to_string()
}

/// The place the host would hand out: an empty file of mode 0600 in a folder of our own.
fn place(dir: &Scratch) -> std::path::PathBuf {
    let path = dir.0.join(NAME);
    std::fs::write(&path, b"").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    path
}

fn slot_ok(id: &str, path: &Path) -> String {
    json!({"v": 1, "ok": 1, "id": id, "name": NAME, "path": path.to_str().unwrap()}).to_string()
}

// ------------------------------------------------------------------------------------------------------------------------ the store
#[test]
fn the_chip_line_names_the_size_the_way_the_host_rounds_it_and_the_dump_has_the_size_and_never_the_pixels() {
    let mut store = Pictures::default();
    store.put("s", Picture { bytes: vec![1; 245 * 1024], width: 64, height: 48 });
    assert_eq!(store.chip_line("s").as_deref(), Some("[picture 245 KiB, 64x48] - Esc removes it"));
    let (picture, chip) = store.dump("s");
    assert_eq!(picture, json!({"bytes": 245 * 1024, "width": 64, "height": 48}));
    assert_eq!(chip, json!("[picture 245 KiB, 64x48] - Esc removes it"));
    assert_eq!(store.dump("other"), (Value::Null, Value::Null));
    for (bytes, kib) in [(1, 1), (511, 1), (512, 1), (1535, 1), (1536, 2), (1024 * 1024, 1024), (3 * MIB - 513, 3071), (3 * MIB - 512, 3072)] {
        assert_eq!(Pictures::kib(bytes), kib, "{bytes} bytes");
    }
}

#[test]
fn a_second_picture_replaces_the_first_and_each_session_has_its_own_chip() {
    let mut store = Pictures::default();
    assert_eq!(store.put("a", pic(100)), Put::Added);
    assert_eq!(store.put("b", pic(200)), Put::Added);
    assert_eq!(store.put("a", pic(300)), Put::Replaced);
    assert_eq!(store.held(), 500, "the replaced picture is not kept");
    assert!(store.remove("a") && !store.remove("a"));
    assert!(!store.has("a") && store.has("b") && store.held() == 200);
}

#[test]
fn the_window_holds_six_mib_of_pictures_in_all_chips_and_pictures_on_their_way_together() {
    let mut store = Pictures::default();
    assert_eq!(store.put("a", pic(2 * MIB)), Put::Added);
    assert_eq!(store.put("b", pic(4 * MIB)), Put::Added, "exactly 6 MiB fits");
    assert_eq!(store.put("c", pic(1)), Put::Full);
    assert!(!store.has("c") && store.held() == 6 * MIB, "a refused picture is refused whole");
    assert_eq!(store.put("a", pic(2 * MIB)), Put::Replaced, "a replacement counts the new picture, not both");
    assert_eq!(store.put("a", pic(2 * MIB + 1)), Put::Full, "one byte over");
    assert!(store.has("a") && store.held() == 6 * MIB, "the old chip is kept when the new one is refused");
    assert!(store.launch("b", "slot-1") && store.held() == 6 * MIB, "a picture on its way still counts");
    assert_eq!(store.put("c", pic(1)), Put::Full);
    store.take_flying("slot-1");
    assert_eq!(store.put("c", pic(1)), Put::Added);
}

#[test]
fn a_picture_on_its_way_changes_its_key_comes_back_to_its_session_unless_a_newer_chip_is_there_and_is_let_go_with_the_host() {
    let mut store = Pictures::default();
    store.put("a", pic(10));
    assert!(store.launch("a", "slot-1") && !store.has("a") && !store.launch("a", "slot-2"));
    store.rekey("slot-1", "gui-1");
    assert_eq!(store.bytes_of("slot-1"), None);
    assert_eq!(store.bytes_of("gui-1").map(<[u8]>::len), Some(10));
    let (session, picture) = store.take_flying("gui-1").unwrap();
    assert_eq!(session, "a");
    assert!(store.restore(&session, picture) && store.has("a"));
    store.launch("a", "gui-2");
    store.put("a", pic(20)); // a newer picture while the first is on its way
    let (_, old) = store.take_flying("gui-2").unwrap();
    assert!(!store.restore("a", old), "the older one is dropped");
    assert_eq!(store.held(), 20);
    store.launch("a", "gui-3");
    store.put("b", pic(5));
    store.launch("b", "gui-4");
    store.forget("a");
    assert_eq!(store.held(), 5, "a deleted session takes its pictures on their way with it");
    store.let_go();
    assert_eq!(store.held(), 0);
}

// ------------------------------------------------------------------------------------------------------------------------ the paste thread
#[test]
fn the_paste_function_asks_the_text_first_and_a_picture_only_when_there_is_no_text_and_only_from_the_clipboard() {
    let x = Xvfb::start();
    let d = x.name.as_str();
    let png = fake_png(64, 48, 3000);
    // a private server that has just been used can reset a new connection under load: the reader then says no_display, and the test asks again
    let ask = |d: &str, image: bool| {
        let mut answer = fetch_pasted(Some(d), b"CLIPBOARD", 8192, image);
        for _ in 0..10 {
            if !matches!(&answer, Pasted::Picture(Err(e)) if e.reason == "no_display") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
            answer = fetch_pasted(Some(d), b"CLIPBOARD", 8192, image);
        }
        answer
    };
    let text = |p: Pasted| match p {
        Pasted::Text(t) => t,
        Pasted::Picture(_) => panic!("a picture was asked for"),
    };
    let picture = |p: Pasted| match p {
        Pasted::Picture(r) => r,
        Pasted::Text(t) => panic!("a text answered: {t:?}"),
    };
    {
        let _owner = Owner::start(d, "CLIPBOARD", serve(vec![("UTF8_STRING", b"words".to_vec()), ("image/png", png.clone())]));
        assert_eq!(text(ask(d, true)), Ok("words".to_string()), "a copy that carries both pastes as TEXT");
    }
    {
        let _owner = Owner::start(d, "CLIPBOARD", serve(vec![("image/png", png.clone())]));
        let read = picture(ask(d, true)).expect("the picture");
        assert_eq!(read.clip, Clip::Png { bytes: png.clone(), width: 64, height: 48 });
        assert_eq!(text(ask(d, false)), Err("empty"), "no picture is asked for where the caller did not allow it (a panel, PRIMARY)");
    }
    {
        let _owner = Owner::start(d, "PRIMARY", serve(vec![("image/png", png.clone())]));
        assert_eq!(text(fetch_pasted(Some(d), b"PRIMARY", 8192, false)), Err("empty"));
        let error = picture(ask(d, true)).expect_err("the picture is read from the CLIPBOARD, never from PRIMARY");
        assert_eq!(error.reason, "empty");
    }
    assert_eq!(picture(ask(d, true)).expect_err("nothing").reason, "empty", "neither a text nor a picture");
    let refused = |bytes: Vec<u8>| {
        let _owner = Owner::start(d, "CLIPBOARD", serve(vec![("image/png", bytes)]));
        picture(ask(d, true)).map(|_| ()).map_err(|e| e.reason)
    };
    assert_eq!(refused(fake_png(64, 48, 3 * MIB)), Ok(()), "exactly 3 MiB is taken");
    assert_eq!(refused(fake_png(64, 48, 3 * MIB + 1)), Err("too_large"));
    assert_eq!(refused(fake_png(9000, 48, 3000)), Err("too_large"), "9000 pixels wide");
    assert_eq!(refused(b"not a picture at all, just words and words and words".to_vec()), Err("bad_image"));
    let jpeg = Owner::start(d, "CLIPBOARD", serve(vec![("image/jpeg", vec![0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3, 4])]));
    assert_eq!(picture(ask(d, true)).expect_err("jpeg").reason, "unsupported_format");
    drop(jpeg);
}

#[test]
fn every_refusal_of_the_reader_has_its_sentence() {
    for (reason, sentence) in [
        ("empty", NOTHING),
        ("no_display", NOTHING),
        ("x_error", NOTHING),
        ("timeout", "Paste: the owner of the selection did not answer."),
        ("too_large", "Paste refused: the picture is over 3 MiB or 8000 pixels on a side."),
        ("bad_image", "Paste refused: that is not a usable PNG picture."),
        ("unsupported_format", "Nothing to paste: only text and PNG pictures can be pasted."),
    ] {
        let (mut app, _host) = window();
        app.ui.paste_pending = true;
        app.on_paste_answer(Pasted::Picture(Err(ClipError::new(reason, "detail"))));
        assert_eq!(note(&app).as_deref(), Some(sentence), "{reason}");
        assert!(!app.ed.pictures.has("s-claude") && !app.ui.paste_pending);
    }
}

// ------------------------------------------------------------------------------------------------------------------------ the chip
#[test]
fn a_picture_becomes_the_chip_of_a_claude_session_with_its_line_under_the_box_and_in_the_dump() {
    let (mut app, _host) = window();
    paste(&mut app, 245 * 1024);
    let dump = app.editor_json();
    assert_eq!(dump["picture"], json!({"bytes": 245 * 1024, "width": 64, "height": 48}));
    assert_eq!(dump["chip"], "[picture 245 KiB, 64x48] - Esc removes it");
    assert_eq!(note(&app), None, "the line is not a note of the editor");
    let prompt = app.prompt_view(80, 40);
    assert_eq!(prompt.note.as_deref(), Some("[picture 245 KiB, 64x48] - Esc removes it"), "the line under the box");
    app.ed.say("another line");
    assert_eq!(app.prompt_view(80, 40).note.as_deref(), Some("another line"), "a note of the box comes first; the chip is still held");
    assert!(app.ed.pictures.has("s-claude"));
}

#[test]
fn a_session_that_is_not_claude_takes_no_picture_and_says_so() {
    let (mut app, _host) = window();
    app.session = "s-codex".into();
    paste(&mut app, 1000);
    assert!(!app.ed.pictures.has("s-codex") && app.ed.pictures.held() == 0);
    assert_eq!(note(&app).as_deref(), Some("Pictures can be pasted into Claude sessions only."));
    app.session = "no-such-row".into(); // a synthetic or unknown row has no provider that is Claude
    paste(&mut app, 1000);
    assert_eq!(app.ed.pictures.held(), 0);
}

#[test]
fn a_second_picture_replaces_the_first_and_says_so_and_a_picture_past_six_mib_is_refused_whole() {
    let (mut app, _host) = window();
    paste(&mut app, 1000);
    paste(&mut app, 2000);
    assert_eq!(app.editor_json()["picture"]["bytes"], 2000);
    assert_eq!(note(&app).as_deref(), Some("The picture was replaced."));
    app.ed.pictures.remove("s-claude");
    paste(&mut app, 3 * MIB);
    app.session = "s-two".into();
    paste(&mut app, 3 * MIB);
    assert!(app.ed.pictures.has("s-two") && app.ed.pictures.held() == 6 * MIB);
    app.session = "s-old".into();
    paste(&mut app, 1);
    assert!(!app.ed.pictures.has("s-old"));
    assert_eq!(note(&app).as_deref(), Some("Send or remove the waiting picture of another session first."));
}

#[test]
fn a_chip_stays_with_its_session_when_another_is_opened_and_the_person_comes_back() {
    let (mut app, _host) = window();
    paste(&mut app, 1000);
    app.ed.park("s-claude", "");
    app.session = "s-two".into();
    app.ed.resume("s-two");
    assert_eq!(app.editor_json()["picture"], Value::Null, "the other session has no chip");
    assert_eq!(app.prompt_view(80, 40).note, None);
    app.ed.park("s-two", "");
    app.session = "s-claude".into();
    app.ed.resume("s-claude");
    assert_eq!(app.editor_json()["picture"]["bytes"], 1000);
}

#[test]
fn escape_removes_the_chip_and_leaves_the_text_and_a_deleted_session_takes_its_chip() {
    let (mut app, _host) = window();
    paste(&mut app, 1000);
    app.ed.set("keep this".to_string());
    assert!(app.drop_chip() && !app.ed.pictures.has("s-claude") && app.ed.pictures.held() == 0);
    assert_eq!(app.ed.text(), "keep this");
    assert!(!app.drop_chip(), "a second Escape finds no chip: the caller goes on to the start screen");
    paste(&mut app, 500);
    app.ed.forget("s-claude");
    assert_eq!(app.ed.pictures.held(), 0);
}

// ------------------------------------------------------------------------------------------------------------------------ Enter
#[test]
fn enter_with_a_chip_and_text_asks_for_a_place_and_holds_both_and_sends_no_input_yet() {
    let (mut app, mut host) = window();
    paste(&mut app, 2000);
    type_and_enter(&mut app, "what is this?");
    let lines = asked(&mut host);
    let id = slot_id(&lines);
    assert!(id.starts_with("slot-") && id == format!("slot-{}-1", std::process::id()), "{id}");
    assert_eq!(app.pending_inputs[&id], ("s-claude".to_string(), "what is this?".to_string()), "the text waits under the id of the slot");
    assert_eq!((app.ed.text(), app.editor_json()["picture"].clone()), ("", Value::Null), "the box is empty and the chip is on its way");
    assert_eq!(app.ed.pictures.held(), 2000, "and its memory still counts");
}

#[test]
fn the_slot_answer_writes_the_picture_and_sends_the_input_with_the_name_not_the_path_and_the_input_answer_drops_the_picture() {
    let (mut app, mut host) = window();
    let dir = Scratch::new("picture_flow");
    let path = place(&dir);
    app.ed.pictures.put("s-claude", Picture { bytes: (0..=255u8).cycle().take(5000).collect(), width: 64, height: 48 });
    type_and_enter(&mut app, "look \"here\"");
    let id = slot_id(&asked(&mut host));
    assert!(app.on_picture_reply(&slot_ok(&id, &path)));
    assert_eq!(std::fs::read(&path).unwrap(), (0..=255u8).cycle().take(5000).collect::<Vec<u8>>(), "every byte was written");
    let lines = asked(&mut host);
    assert_eq!(lines.len(), 1, "{lines:?}");
    let input = &lines[0];
    assert_eq!(input["op"], "input");
    assert_eq!((input["session"].as_str(), input["generation"].as_i64(), input["text"].as_str()), (Some("s-claude"), Some(5), Some("look \"here\"")));
    assert_eq!(input["attachment"], NAME, "the NAME and nothing else");
    assert!(!input.to_string().contains(dir.0.to_str().unwrap()), "no path goes to the host in the input");
    let request = input["request_id"].as_str().unwrap().to_string();
    assert!(request.starts_with("gui-") && !app.pending_inputs.contains_key(&id) && app.pending_inputs.contains_key(&request));
    assert_eq!(app.ed.pictures.bytes_of(&request).map(<[u8]>::len), Some(5000), "the picture waits under the request id");
    app.on_sock_line(&format!(r#"{{"v":1,"ok":1,"id":"{request}","session":"s-claude","request_id":"{request}","state":"accepted","duplicate":false}}"#));
    assert!(app.pending_inputs.is_empty() && app.ed.pictures.held() == 0, "taken: the picture is dropped from memory");
    assert_eq!(app.editor_json()["picture"], Value::Null);
}

#[test]
fn enter_with_a_chip_and_an_empty_box_sends_nothing_and_the_chip_stays() {
    let (mut app, mut host) = window();
    paste(&mut app, 1000);
    for text in ["", "  \n "] {
        type_and_enter(&mut app, text);
        assert!(asked(&mut host).is_empty() && app.pending_inputs.is_empty());
        assert_eq!(note(&app).as_deref(), Some("Type a line to go with the picture."));
        assert!(app.ed.pictures.has("s-claude"));
    }
}

#[test]
fn enter_with_a_chip_in_a_session_that_is_not_running_sends_nothing_and_text_and_chip_stay() {
    for session in ["s-old", "s-gone"] {
        let (mut app, mut host) = window();
        app.session = session.into();
        paste(&mut app, 1000);
        type_and_enter(&mut app, "hello?");
        assert!(asked(&mut host).is_empty() && app.pending_inputs.is_empty(), "{session}");
        assert_eq!((app.ed.text(), app.ed.pictures.has(session)), ("hello?", true));
        assert_eq!(note(&app).as_deref(), Some("Reconnect the session first: send a line without the picture, then paste it again."), "{session}");
    }
}

#[test]
fn a_text_over_the_limit_with_a_chip_is_kept_with_its_reason_and_nothing_is_asked() {
    let (mut app, mut host) = window();
    paste(&mut app, 1000);
    type_and_enter(&mut app, &"x".repeat(super::super::editor::MAX_TEXT + 1));
    assert!(asked(&mut host).is_empty() && app.ed.pictures.has("s-claude"));
    assert!(note(&app).unwrap().starts_with("Not sent: "), "{:?}", note(&app));
}

#[test]
fn a_session_without_a_chip_sends_as_it_always_did() {
    let (mut app, mut host) = window();
    type_and_enter(&mut app, "plain");
    let lines = asked(&mut host);
    assert_eq!(lines.len(), 1);
    assert!(lines[0]["op"] == "input" && lines[0].get("attachment").is_none(), "{lines:?}");
}

#[test]
fn a_slot_the_host_refused_or_a_write_that_failed_gives_text_and_chip_back_with_a_sentence() {
    let dir = Scratch::new("picture_slot_refused");
    let good = place(&dir);
    std::fs::write(dir.0.join("full.png"), b"already").unwrap();
    let other = json!({"v": 1, "ok": 1, "name": "ffffffffffffffffffffffffffffffff.png", "path": dir.0.join("ffffffffffffffffffffffffffffffff.png").to_str().unwrap()});
    for (reply, sentence) in [
        (json!({"v": 1, "ok": 0, "error": "busy"}), "Too many pictures are waiting, try again in a moment."),
        (json!({"v": 1, "ok": 0, "error": "slot_failed"}), "The host could not make a place for the picture."),
        (json!({"v": 1, "ok": 1, "name": NAME, "path": dir.0.join("nowhere").join(NAME).to_str().unwrap()}), "The picture could not be written to the host's folder."),
        (json!({"v": 1, "ok": 1, "name": NAME, "path": dir.0.join("full.png").to_str().unwrap()}), "The host could not make a place for the picture."),
        (other, "The picture could not be written to the host's folder."), // the file does not exist: it is not created
        (json!({"v": 1, "ok": 1, "name": "UPPER.png", "path": good.to_str().unwrap()}), "The host could not make a place for the picture."),
        (json!({"v": 1, "ok": 1, "name": NAME}), "The host could not make a place for the picture."),
    ] {
        let (mut app, mut host) = window();
        paste(&mut app, 1000);
        type_and_enter(&mut app, "the text");
        let id = slot_id(&asked(&mut host));
        let mut reply = reply;
        reply["id"] = json!(id);
        assert!(app.on_picture_reply(&reply.to_string()));
        assert!(asked(&mut host).is_empty(), "no input after a refused slot: {reply}");
        assert_eq!((app.ed.text(), app.ed.pictures.has("s-claude"), app.ed.pictures.held()), ("the text", true, 1000), "{reply}");
        assert_eq!(note(&app).as_deref(), Some(sentence), "{reply}");
        assert!(app.pending_inputs.is_empty());
    }
    assert_eq!(std::fs::read(&good).unwrap(), b"", "the good place was never touched");
    assert!(!dir.0.join("ffffffffffffffffffffffffffffffff.png").exists(), "the window creates no file");
    assert_eq!(std::fs::read(dir.0.join("full.png")).unwrap(), b"already", "a file that is not empty is not written");
}

#[test]
fn a_slot_for_a_session_that_is_gone_or_a_late_answer_writes_nothing() {
    let (mut app, mut host) = window();
    let dir = Scratch::new("picture_late");
    let path = place(&dir);
    paste(&mut app, 1000);
    type_and_enter(&mut app, "text");
    let id = slot_id(&asked(&mut host));
    app.ed.forget("s-claude");
    app.pending_inputs.retain(|_, (s, _)| s != "s-claude");
    assert!(app.on_picture_reply(&slot_ok(&id, &path)));
    assert_eq!((std::fs::read(&path).unwrap(), app.ed.pictures.held()), (Vec::new(), 0));
    assert!(asked(&mut host).is_empty());
}

#[test]
fn a_symbolic_link_for_a_place_is_not_followed() {
    let (mut app, mut host) = window();
    let dir = Scratch::new("picture_link");
    let target = dir.0.join("target.txt");
    std::fs::write(&target, b"").unwrap();
    let link = dir.0.join(NAME);
    std::os::unix::fs::symlink(&target, &link).unwrap();
    paste(&mut app, 1000);
    type_and_enter(&mut app, "text");
    let id = slot_id(&asked(&mut host));
    app.on_picture_reply(&slot_ok(&id, &link));
    assert_eq!(std::fs::read(&target).unwrap(), b"", "the target of the link was not written");
    assert_eq!(note(&app).as_deref(), Some("The picture could not be written to the host's folder."));
    assert!(app.ed.pictures.has("s-claude") && app.ed.text() == "text");
}

#[test]
fn a_refused_input_gives_text_and_chip_back_with_the_sentence_for_its_word() {
    for (word, sentence) in [
        ("attachment_gone", "The picture file vanished; paste it again."),
        ("attachment_too_large", "The picture is over 3 MiB."),
        ("bad_image", "That is not a usable PNG picture."),
        ("attachment_unsupported", "This session cannot take pictures."),
        ("session_disconnected", "Not sent: that session was closed meanwhile; press Enter again to reconnect it and send. The text is back in the box."),
    ] {
        let (mut app, mut host) = window();
        let dir = Scratch::new("picture_refused");
        let path = place(&dir);
        paste(&mut app, 1000);
        type_and_enter(&mut app, "the text");
        let id = slot_id(&asked(&mut host));
        app.on_picture_reply(&slot_ok(&id, &path));
        let request = asked(&mut host)[0]["request_id"].as_str().unwrap().to_string();
        app.on_sock_line(&format!(r#"{{"v":1,"ok":0,"id":"{request}","error":"{word}"}}"#));
        let said = note(&app).unwrap();
        assert_eq!(said, sentence, "{word}");
        assert_eq!((app.ed.text(), app.ed.pictures.has("s-claude"), app.ed.pictures.held()), ("the text", true, 1000), "{word}");
        assert!(app.pending_inputs.is_empty());
    }
}

#[test]
fn a_refused_input_whose_session_has_a_newer_chip_drops_the_old_picture_and_says_so() {
    let (mut app, mut host) = window();
    let dir = Scratch::new("picture_newer");
    let path = place(&dir);
    paste(&mut app, 1000);
    type_and_enter(&mut app, "the text");
    let id = slot_id(&asked(&mut host));
    app.on_picture_reply(&slot_ok(&id, &path));
    let request = asked(&mut host)[0]["request_id"].as_str().unwrap().to_string();
    paste(&mut app, 2000); // a newer picture while the first is on its way
    app.on_sock_line(&format!(r#"{{"v":1,"ok":0,"id":"{request}","error":"attachment_gone"}}"#));
    assert!(note(&app).unwrap().ends_with("A newer picture was pasted meanwhile, so this one was dropped."), "{:?}", note(&app));
    assert_eq!((app.editor_json()["picture"]["bytes"].as_i64(), app.ed.pictures.held(), app.ed.text()), (Some(2000), 2000, "the text"));
}

#[test]
fn when_the_host_is_replaced_a_text_that_waited_for_a_place_comes_back_with_its_chip_and_what_was_in_flight_is_let_go() {
    let (mut app, mut host) = window();
    paste(&mut app, 1000);
    type_and_enter(&mut app, "waiting for a place");
    asked(&mut host);
    app.session = "s-two".into();
    paste(&mut app, 3000);
    app.ed.set("in flight".into());
    app.ed.pictures.launch("s-two", "gui-in-flight");
    app.pending_inputs.insert("gui-in-flight".into(), ("s-two".into(), "in flight".into()));
    app.release_held();
    assert!(app.pending_inputs.is_empty());
    assert!(app.ed.pictures.has("s-claude"), "the chip is back with its session");
    assert_eq!(app.ed.pictures.held(), 1000, "and the picture that was in flight is let go");
    app.session = "s-claude".into();
    app.ed.resume("s-claude");
    assert_eq!(app.ed.text(), "waiting for a place");
}

#[test]
fn the_key_help_names_the_picture_keys() {
    let help = include_str!("panel.rs");
    assert!(help.contains("Ctrl+V pastes text, or a PNG picture into a Claude session (Esc removes it)"));
}
