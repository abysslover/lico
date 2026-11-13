//! The clipboard reader against a real X server (class L3, one private Xvfb for the whole test: the display is the cost). The other client is an
//! owner of our own (`Owner`): it can offer any list of targets, refuse `TARGETS`, serve a property over the chunk size by the incremental
//! protocol with a pause before each chunk, or take the selection and ignore every request (the stalled owner). The real owner, `xclip`, and the
//! real command are in `tests/test_clipboard_read.py`; here the protocol is pinned down at the level of the reader.

use super::job::run;
use super::tests::head;
use super::*;
use crate::handoff::Scratch;

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use x11rb::protocol::xproto::{ChangeWindowAttributesAux, ConnectionExt as _, PropMode, SelectionNotifyEvent, SELECTION_NOTIFY_EVENT};
use x11rb::wrapper::ConnectionExt as _;

pub(crate) struct Xvfb {
    child: Child,
    pub(crate) name: String,
    _pass: crate::testgate::Pass,
}

impl Xvfb {
    pub(crate) fn start() -> Xvfb {
        let pass = crate::testgate::X_SERVERS.enter(); // at most a few private X servers at once: each is 65 to 95 MiB
        let mut child = Command::new("Xvfb")
            .args(["-displayfd", "1", "-screen", "0", "320x200x24", "-nolisten", "tcp", "-noreset"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("Xvfb must be installed for these tests (they never skip)");
        let mut line = String::new();
        BufReader::new(child.stdout.take().unwrap()).read_line(&mut line).unwrap();
        Xvfb { child, name: format!(":{}", line.trim()), _pass: pass }
    }
}

impl Drop for Xvfb {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// What an owner does.
#[derive(Clone, Default)]
pub(crate) struct Serve {
    /// The targets it converts to, with the bytes of each.
    targets: Vec<(&'static str, Vec<u8>)>,
    /// Whether it answers `TARGETS` (some owners refuse it).
    list: bool,
    /// A property over this size goes by the incremental protocol in chunks of this size.
    chunk: Option<usize>,
    /// The pause before each chunk.
    delay: Duration,
    /// The chunks are not all of one size (every second one is a third as long): a short chunk is not the end, only an empty one is.
    irregular: bool,
    /// Takes the selection and ignores every request.
    ignore: bool,
}

pub(crate) struct Owner {
    stop: Arc<AtomicBool>,
    join: Option<thread::JoinHandle<()>>,
}

impl Owner {
    pub(crate) fn start(display: &str, selection: &'static str, serve: Serve) -> Owner {
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let (d, s) = (display.to_string(), stop.clone());
        let join = thread::spawn(move || owner_loop(&d, selection, serve, &s, tx));
        rx.recv_timeout(Duration::from_secs(10)).expect("the owner took the selection");
        Owner { stop, join: Some(join) }
    }
}

impl Drop for Owner {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

struct Incr {
    requestor: u32,
    property: u32,
    target: u32,
    data: Vec<u8>,
    pos: usize,
    index: usize,
}

fn owner_loop(display: &str, selection: &str, serve: Serve, stop: &AtomicBool, ready: mpsc::Sender<()>) {
    // a server that has just printed its number can reset the first connection when several start at once: try again a few times
    let (conn, num) = (0..20)
        .find_map(|_| x11rb::connect(Some(display)).ok().or_else(|| { thread::sleep(Duration::from_millis(100)); None }))
        .expect("the test display");
    let root = conn.setup().roots[num].root;
    let win = conn.generate_id().unwrap();
    let aux = CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE);
    conn.create_window(0, win, root, 0, 0, 1, 1, 0, WindowClass::INPUT_ONLY, 0, &aux).unwrap();
    let atom = |n: &str| conn.intern_atom(false, n.as_bytes()).unwrap().reply().unwrap().atom;
    let (sel, targets_atom, incr_atom) = (atom(selection), atom("TARGETS"), atom("INCR"));
    let table: Vec<(Atom, Vec<u8>)> = serve.targets.iter().map(|(n, d)| (atom(n), d.clone())).collect();
    conn.set_selection_owner(win, sel, x11rb::CURRENT_TIME).unwrap();
    conn.flush().unwrap();
    assert_eq!(conn.get_selection_owner(sel).unwrap().reply().unwrap().owner, win, "the owner got the selection");
    ready.send(()).unwrap();
    let mut state: Option<Incr> = None;
    while !stop.load(Ordering::SeqCst) {
        match conn.poll_for_event().unwrap() {
            Some(Event::SelectionRequest(r)) => {
                if serve.ignore {
                    continue;
                }
                let mut property = r.property;
                if r.target == targets_atom {
                    if serve.list {
                        let mut list = vec![targets_atom];
                        list.extend(table.iter().map(|t| t.0));
                        conn.change_property32(PropMode::REPLACE, r.requestor, r.property, AtomEnum::ATOM, &list).unwrap();
                    } else {
                        property = x11rb::NONE;
                    }
                } else if let Some((_, data)) = table.iter().find(|t| t.0 == r.target) {
                    match serve.chunk {
                        Some(c) if data.len() > c => {
                            // the cue of every chunk is the requestor deleting the property: listen to its window
                            conn.change_window_attributes(r.requestor, &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE)).unwrap();
                            conn.change_property32(PropMode::REPLACE, r.requestor, r.property, incr_atom, &[data.len() as u32]).unwrap();
                            state = Some(Incr { requestor: r.requestor, property: r.property, target: r.target, data: data.clone(), pos: 0, index: 0 });
                        }
                        _ => {
                            conn.change_property8(PropMode::REPLACE, r.requestor, r.property, r.target, data).unwrap();
                        }
                    }
                } else {
                    property = x11rb::NONE;
                }
                let notify = SelectionNotifyEvent { response_type: SELECTION_NOTIFY_EVENT, sequence: 0, time: r.time, requestor: r.requestor, selection: r.selection, target: r.target, property };
                conn.send_event(false, r.requestor, EventMask::NO_EVENT, notify).unwrap();
                conn.flush().unwrap();
            }
            Some(Event::PropertyNotify(e)) if e.state == Property::DELETE => {
                let mut finished = false;
                if let Some(s) = state.as_mut() {
                    if e.window == s.requestor && e.atom == s.property {
                        thread::sleep(serve.delay);
                        let chunk = if serve.irregular && s.index % 2 == 1 { (serve.chunk.unwrap() / 3).max(1) } else { serve.chunk.unwrap() };
                        s.index += 1;
                        let end = (s.pos + chunk).min(s.data.len());
                        conn.change_property8(PropMode::REPLACE, s.requestor, s.property, s.target, &s.data[s.pos..end]).unwrap();
                        conn.flush().unwrap();
                        finished = s.pos == end; // an empty chunk ends the transfer
                        s.pos = end;
                    }
                }
                if finished {
                    state = None;
                }
            }
            Some(_) => {}
            None => thread::sleep(Duration::from_millis(1)),
        }
    }
}

// ---- the scenarios ---------------------------------------------------------------------------------------------------------------

fn limits(text_max: usize, image_max: usize, ms: u64) -> ReadLimits {
    ReadLimits { text_max, image_max, deadline: Duration::from_millis(ms) }
}

fn plain() -> ReadLimits {
    limits(65536, 3 * 1024 * 1024, 3000)
}

pub(crate) fn serve(targets: Vec<(&'static str, Vec<u8>)>) -> Serve {
    Serve { targets, list: true, ..Default::default() }
}

fn noise(n: usize, seed: u64) -> Vec<u8> {
    let mut s = seed | 1;
    (0..n)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            (s >> 24) as u8
        })
        .collect()
}

/// A forged PNG of `total` bytes: a real header with the given size, then noise (the reader looks at the header only).
pub(crate) fn fake_png(w: u32, h: u32, total: usize) -> Vec<u8> {
    let mut v = head(w, h);
    v.extend(noise(total - v.len(), 7));
    v
}

fn text(r: Result<Read, ClipError>) -> String {
    match r.expect("a text").clip {
        Clip::Text(t) => t,
        other => panic!("not a text: {other:?}"),
    }
}

fn why(r: Result<Read, ClipError>) -> &'static str {
    r.expect_err("a refusal").reason
}

fn read(d: &str, sel: Selection, want: Want, l: &ReadLimits) -> Result<Read, ClipError> {
    read_selection(Some(d), sel, want, l)
}

#[test]
fn the_reader_against_owners_of_our_own() {
    let x = Xvfb::start();
    let d = x.name.as_str();
    let mine = |name: &'static str, s: Serve| Owner::start(d, name, s);

    // no owner: empty; no display: no_display
    assert_eq!(why(read(d, Selection::Clipboard, Want::Text, &plain())), "empty", "no owner");
    assert_eq!(why(read_selection(Some(":99999"), Selection::Clipboard, Want::Text, &plain())), "no_display");

    // text: ASCII, Korean, an emoji; the two selections hold different texts and each read gets its own
    {
        let _c = mine("CLIPBOARD", serve(vec![("UTF8_STRING", "clipboard text, 파일을 읽습니다 \u{1F642}".as_bytes().to_vec())]));
        let _p = mine("PRIMARY", serve(vec![("UTF8_STRING", b"primary text".to_vec())]));
        assert_eq!(text(read(d, Selection::Clipboard, Want::Text, &plain())), "clipboard text, 파일을 읽습니다 \u{1F642}");
        assert_eq!(text(read(d, Selection::Primary, Want::Text, &plain())), "primary text");
        assert_eq!(text(read(d, Selection::Clipboard, Want::Auto, &plain())), "clipboard text, 파일을 읽습니다 \u{1F642}");
    }
    // the limit: exactly 64 KiB is accepted, one more is too_large, and the text is never cut
    {
        let exact = "a".repeat(65536);
        let _c = mine("CLIPBOARD", serve(vec![("UTF8_STRING", exact.clone().into_bytes())]));
        assert_eq!(text(read(d, Selection::Clipboard, Want::Text, &limits(65536, 1 << 20, 3000))), exact, "exactly the limit");
        assert_eq!(why(read(d, Selection::Clipboard, Want::Text, &limits(65535, 1 << 20, 3000))), "too_large", "one over");
    }
    {
        let _c = mine("CLIPBOARD", serve(vec![("UTF8_STRING", "a".repeat(65537).into_bytes())]));
        assert_eq!(why(read(d, Selection::Clipboard, Want::Text, &limits(65536, 1 << 20, 3000))), "too_large");
    }
    // an empty text is empty; STRING alone is read when UTF8_STRING is not offered
    {
        let _c = mine("CLIPBOARD", serve(vec![("UTF8_STRING", Vec::new())]));
        assert_eq!(why(read(d, Selection::Clipboard, Want::Text, &plain())), "empty");
    }
    {
        let _c = mine("CLIPBOARD", serve(vec![("STRING", b"latin text".to_vec())]));
        assert_eq!(text(read(d, Selection::Clipboard, Want::Text, &plain())), "latin text");
    }

    // formats: only image/jpeg or only text/html offered
    {
        let _c = mine("CLIPBOARD", serve(vec![("image/jpeg", vec![0xFF, 0xD8, 0xFF, 1, 2, 3])]));
        assert_eq!(why(read(d, Selection::Clipboard, Want::Image, &plain())), "unsupported_format", "jpeg only, an image wanted");
        assert_eq!(why(read(d, Selection::Clipboard, Want::Auto, &plain())), "unsupported_format", "jpeg only, auto");
        assert_eq!(why(read(d, Selection::Clipboard, Want::Text, &plain())), "unsupported_format", "jpeg only, a text wanted");
    }
    {
        let _c = mine("CLIPBOARD", serve(vec![("text/html", b"<b>hi</b>".to_vec())]));
        assert_eq!(why(read(d, Selection::Clipboard, Want::Text, &plain())), "unsupported_format", "html only, a text wanted");
        assert_eq!(why(read(d, Selection::Clipboard, Want::Image, &plain())), "unsupported_format");
    }
    // auto: the picture when both are offered, the text when only text is; a text owner gives no image
    {
        let png = fake_png(30, 20, 400);
        let _c = mine("CLIPBOARD", serve(vec![("UTF8_STRING", b"words".to_vec()), ("image/png", png.clone())]));
        let got = read(d, Selection::Clipboard, Want::Auto, &plain()).unwrap();
        assert_eq!(got.clip, Clip::Png { bytes: png, width: 30, height: 20 });
        assert_eq!(text(read(d, Selection::Clipboard, Want::Text, &plain())), "words");
    }
    {
        let _c = mine("CLIPBOARD", serve(vec![("UTF8_STRING", b"words".to_vec())]));
        assert_eq!(text(read(d, Selection::Clipboard, Want::Auto, &plain())), "words");
        assert_eq!(why(read(d, Selection::Clipboard, Want::Image, &plain())), "unsupported_format");
    }

    // pictures: the header decides; a blob that is not a PNG, a side over 8000, a side of exactly 8000
    {
        let _c = mine("CLIPBOARD", serve(vec![("image/png", vec![b'x'; 5000])]));
        assert_eq!(why(read(d, Selection::Clipboard, Want::Image, &plain())), "bad_image");
    }
    {
        let _c = mine("CLIPBOARD", serve(vec![("image/png", fake_png(9000, 20, 200))]));
        assert_eq!(why(read(d, Selection::Clipboard, Want::Image, &plain())), "too_large", "9000 px wide");
    }
    {
        let _c = mine("CLIPBOARD", serve(vec![("image/png", fake_png(8000, 8000, 200))]));
        assert!(matches!(read(d, Selection::Clipboard, Want::Image, &plain()).unwrap().clip, Clip::Png { width: 8000, height: 8000, .. }), "8000 x 8000 is accepted");
    }

    // an owner that refuses TARGETS: the wanted forms are asked for in turn
    {
        let png = fake_png(5, 5, 300);
        let mut s = serve(vec![("image/png", png.clone())]);
        s.list = false;
        let _c = mine("CLIPBOARD", s);
        assert_eq!(read(d, Selection::Clipboard, Want::Image, &plain()).unwrap().clip, Clip::Png { bytes: png, width: 5, height: 5 });
        assert_eq!(why(read(d, Selection::Clipboard, Want::Text, &plain())), "empty", "it refuses every text form");
    }

    // the incremental transfer: a picture over the chunk size arrives in chunks, the total is counted, the end is the empty chunk
    {
        let png = fake_png(640, 480, 300_000);
        let _c = mine("CLIPBOARD", Serve { chunk: Some(65_536), ..serve(vec![("image/png", png.clone())]) });
        let got = read(d, Selection::Clipboard, Want::Image, &limits(1 << 16, 300_000, 5000)).unwrap();
        assert_eq!(got.incr_chunks, 5, "300000 bytes in chunks of 65536 are 5 chunks (the empty one that ends it is not counted)");
        assert_eq!(got.clip, Clip::Png { bytes: png, width: 640, height: 480 }, "every byte arrived, in order");
        assert_eq!(why(read(d, Selection::Clipboard, Want::Image, &limits(1 << 16, 299_999, 5000))), "too_large", "one byte over the limit, counted over the chunks");
        assert_eq!(why(read(d, Selection::Clipboard, Want::Image, &limits(1 << 16, 100_000, 5000))), "too_large", "far over the limit");
    }
    // chunks of different sizes: a short chunk is not the end of the transfer, the empty one is
    {
        let png = fake_png(64, 64, 100_000);
        let _c = mine("CLIPBOARD", Serve { chunk: Some(30_000), irregular: true, ..serve(vec![("image/png", png.clone())]) });
        let got = read(d, Selection::Clipboard, Want::Image, &limits(1 << 16, 1 << 20, 5000)).unwrap();
        assert_eq!(got.clip, Clip::Png { bytes: png, width: 64, height: 64 }, "every byte arrived, though some chunks were short");
        assert!(got.incr_chunks == 5, "the chunks were {}", got.incr_chunks);
    }
    // a transfer that is slow as a whole is cut by the deadline of the whole, though no chunk is late: 8 chunks, 30 ms before each
    {
        let png = fake_png(100, 100, 8 * 4096);
        let slow = Serve { chunk: Some(4096), delay: Duration::from_millis(30), ..serve(vec![("image/png", png.clone())]) };
        let _c = mine("CLIPBOARD", slow);
        let started = Instant::now();
        assert_eq!(why(read(d, Selection::Clipboard, Want::Image, &limits(1 << 16, 1 << 20, 120))), "timeout", "9 pauses of 30 ms do not fit 120 ms");
        assert!(started.elapsed() < Duration::from_millis(1120), "ended near its deadline: {:?}", started.elapsed());
        let ok = read(d, Selection::Clipboard, Want::Image, &limits(1 << 16, 1 << 20, 5000)).unwrap();
        assert_eq!((ok.incr_chunks, ok.clip), (8, Clip::Png { bytes: png, width: 100, height: 100 }));
    }
    // an owner that never answers: timeout within the deadline and a second
    {
        let _c = mine("CLIPBOARD", Serve { ignore: true, ..serve(vec![("UTF8_STRING", b"never".to_vec())]) });
        let started = Instant::now();
        assert_eq!(why(read(d, Selection::Clipboard, Want::Text, &limits(65536, 1 << 20, 300))), "timeout");
        let took = started.elapsed();
        assert!(took >= Duration::from_millis(300) && took < Duration::from_millis(1300), "{took:?}");
    }

    // the job core on the same display: a text answer and a picture written to a file
    {
        let dir = Scratch::new("clipjob");
        let png = fake_png(31, 17, 900);
        let _c = mine("CLIPBOARD", serve(vec![("UTF8_STRING", "quote \" backslash \\ newline \n tab \t".as_bytes().to_vec()), ("image/png", png.clone())]));
        let line = run(&["--kind".to_string(), "text".to_string()], Some(d)).unwrap();
        assert_eq!(line["text"], "quote \" backslash \\ newline \n tab \t");
        assert_eq!((line["ok"].as_i64(), line["kind"].as_str(), line["selection"].as_str(), line["bytes"].as_u64()), (Some(1), Some("text"), Some("clipboard"), Some("quote \" backslash \\ newline \n tab \t".len() as u64)));
        let out = dir.0.join("pic.png");
        let line = run(&["--kind".into(), "auto".into(), "--out".into(), out.to_string_lossy().into_owned()], Some(d)).unwrap();
        assert_eq!(line["path"], out.to_string_lossy().as_ref());
        assert_eq!((line["kind"].as_str(), line["bytes"].as_u64(), line["width"].as_u64(), line["height"].as_u64()), (Some("image"), Some(900), Some(31), Some(17)));
        assert_eq!(std::fs::read(&out).unwrap(), png);
        // a refused picture leaves no file under the name and no temporary file
        let refused = dir.0.join("refused.png");
        let e = run(&["--kind".into(), "image".into(), "--out".into(), refused.to_string_lossy().into_owned(), "--max-bytes".into(), "899".into()], Some(d)).unwrap_err();
        assert_eq!(e.reason, "too_large");
        assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 1, "only pic.png is there");
    }
}

/// The real owner, `xclip`, serves a picture of about 2 MB by the incremental protocol; this is the evidence that INCR was really taken (the chunk
/// count of the reader), and the bytes arrive whole. (`tests/test_clipboard_read.py` does the same through the command.)
#[test]
fn xclip_serves_a_two_megabyte_picture_by_incr_and_every_byte_arrives() {
    let x = Xvfb::start();
    let dir = Scratch::new("clipxclip");
    let png = fake_png(900, 800, 2_100_000);
    let file = dir.0.join("big.png");
    std::fs::write(&file, &png).unwrap();
    let mut owner = Command::new("xclip")
        .args(["-quiet", "-selection", "clipboard", "-t", "image/png", "-i"])
        .arg(&file)
        .env("DISPLAY", &x.name)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("xclip must be installed for these tests (they never skip)");
    // ready when the owner answers a request for its targets
    let ready = (0..200).any(|_| {
        thread::sleep(Duration::from_millis(25));
        read(&x.name, Selection::Clipboard, Want::Image, &limits(1 << 16, 3 << 20, 3000)).is_ok()
    });
    assert!(ready, "xclip took the selection");
    let got = read(&x.name, Selection::Clipboard, Want::Image, &limits(1 << 16, 3 << 20, 3000)).unwrap();
    eprintln!("xclip INCR evidence: {} bytes in {} chunks", png.len(), got.incr_chunks);
    assert!(got.incr_chunks >= 2, "a picture over the maximum request size comes in chunks, got {}", got.incr_chunks);
    assert_eq!(got.clip, Clip::Png { bytes: png.clone(), width: 900, height: 800 });
    assert_eq!(why(read(&x.name, Selection::Clipboard, Want::Image, &limits(1 << 16, png.len() - 1, 3000))), "too_large", "one byte under the size");
    let _ = owner.kill();
    let _ = owner.wait();
}
