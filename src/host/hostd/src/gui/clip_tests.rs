//! The owner of the CLIPBOARD selection against a real X server (class L3, one private Xvfb for the whole test; plan M02-P04, S04b). The requestor is
//! one of our own: it asks for any target, follows the incremental protocol step by step (and records the size of every chunk), or deletes the
//! announcement and then does nothing (the stalled requestor). The real reader (`xclip`, `lico clipboard read`) is in `tests/test_clipboard_write.py`.

use super::*;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _, CreateWindowAux, EventMask, GetPropertyReply, Property, WindowClass};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

struct Xvfb {
    child: Child,
    name: String,
    _pass: crate::testgate::Pass,
}

impl Xvfb {
    fn start() -> Xvfb {
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

/// An owner thread on the display, and the lines it reports.
struct Rig {
    _x: Xvfb,
    display: String,
    jobs: mpsc::Sender<Job>,
    reports: mpsc::Receiver<String>,
}

impl Rig {
    fn start() -> Rig {
        let x = Xvfb::start();
        let (jobs, rx) = mpsc::channel();
        let (tx, reports) = mpsc::channel();
        let display = x.name.clone();
        let d = display.clone();
        let tx = std::sync::Mutex::new(tx);
        thread::spawn(move || serve(Some(&d), rx, &move |line| drop(tx.lock().unwrap().send(line))));
        Rig { _x: x, display, jobs, reports }
    }

    fn give(&self, req: Option<&str>, source: Source) -> Option<serde_json::Value> {
        self.jobs.send(Job { req: req.map(str::to_string), source, clipboard: true, primary: false }).unwrap();
        req.map(|_| serde_json::from_str(&self.reports.recv_timeout(Duration::from_secs(10)).expect("the owner reported")).unwrap())
    }

    fn silent(&self) {
        assert!(self.reports.recv_timeout(Duration::from_millis(300)).is_err(), "a job with no request id was reported");
    }

    fn requestor(&self) -> Requestor {
        Requestor::for_selection(&self.display, "CLIPBOARD")
    }

    fn primary_requestor(&self) -> Requestor {
        Requestor::for_selection(&self.display, "PRIMARY")
    }
}

/// What a requestor got.
#[derive(Debug, PartialEq)]
enum Got {
    Refused,
    Atoms(Vec<String>),
    Bytes { ptype: String, data: Vec<u8>, incr: Option<IncrFacts> },
}

#[derive(Debug, PartialEq)]
struct IncrFacts {
    announced: u32,
    chunks: Vec<usize>,
}

struct Requestor {
    conn: RustConnection,
    win: u32,
    selection: u32,
    property: u32,
}

impl Requestor {
    fn new(display: &str) -> Requestor {
        Self::for_selection(display, "CLIPBOARD")
    }

    fn for_selection(display: &str, sel_name: &str) -> Requestor {
        let (conn, num) = x11rb::connect(Some(display)).unwrap();
        let root = conn.setup().roots[num].root;
        let win = conn.generate_id().unwrap();
        let aux = CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE);
        conn.create_window(0, win, root, 0, 0, 1, 1, 0, WindowClass::INPUT_ONLY, 0, &aux).unwrap();
        let atom = |n: &str| conn.intern_atom(false, n.as_bytes()).unwrap().reply().unwrap().atom;
        let (selection, property) = (atom(sel_name), atom("LICO_TEST_PROP"));
        Requestor { conn, win, selection, property }
    }

    fn atom(&self, name: &str) -> u32 {
        self.conn.intern_atom(false, name.as_bytes()).unwrap().reply().unwrap().atom
    }

    fn name(&self, atom: u32) -> String {
        String::from_utf8(self.conn.get_atom_name(atom).unwrap().reply().unwrap().name).unwrap()
    }

    fn event(&self, within: Duration) -> Option<Event> {
        let end = Instant::now() + within;
        while Instant::now() < end {
            if let Some(e) = self.conn.poll_for_event().unwrap() {
                return Some(e);
            }
            thread::sleep(Duration::from_millis(2));
        }
        None
    }

    fn convert(&self, target: &str) -> Option<u32> {
        let t = self.atom(target);
        self.conn.convert_selection(self.win, self.selection, t, self.property, x11rb::CURRENT_TIME).unwrap();
        self.conn.flush().unwrap();
        loop {
            match self.event(Duration::from_secs(5)).expect("a SelectionNotify") {
                Event::SelectionNotify(e) if e.requestor == self.win => return (e.property != x11rb::NONE).then_some(e.property),
                _ => {}
            }
        }
    }

    fn take(&self, delete: bool) -> GetPropertyReply {
        self.conn.get_property(delete, self.win, self.property, AtomEnum::ANY, 0, u32::MAX / 4).unwrap().reply().unwrap()
    }

    fn ask(&self, target: &str) -> Got {
        let Some(_) = self.convert(target) else { return Got::Refused };
        let first = self.take(true);
        if self.name(first.type_) == "ATOM" {
            return Got::Atoms(first.value32().unwrap().map(|a| self.name(a)).collect());
        }
        if self.name(first.type_) != "INCR" {
            return Got::Bytes { ptype: self.name(first.type_), data: first.value, incr: None };
        }
        let announced = first.value32().unwrap().next().unwrap();
        let (mut data, mut chunks, mut ptype) = (Vec::new(), Vec::new(), String::new());
        let _ = &ptype;
        loop {
            match self.event(Duration::from_secs(10)).expect("a chunk within 10 s") {
                Event::PropertyNotify(e) if e.window == self.win && e.atom == self.property && e.state == Property::NEW_VALUE => {
                    let chunk = self.take(true);
                    ptype = self.name(chunk.type_);
                    chunks.push(chunk.value.len());
                    if chunk.value.is_empty() {
                        break;
                    }
                    data.extend(chunk.value);
                }
                _ => {}
            }
        }
        Got::Bytes { ptype, data, incr: Some(IncrFacts { announced, chunks }) }
    }

    /// Asks, deletes the announcement of an INCR transfer (which starts it) and then does nothing at all. Returns what the first chunk was, left unread.
    fn ask_then_stall(&self, target: &str) {
        self.convert(target).expect("served");
        assert_eq!(self.name(self.take(true).type_), "INCR");
    }
}

fn noise(n: usize) -> Vec<u8> {
    let mut s = 0x9E37_79B9_7F4A_7C15u64;
    (0..n)
        .map(|_| {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            (s >> 24) as u8
        })
        .collect()
}

/// A forged PNG of `total` bytes: a real header of the size given, then noise (the owner reads the header only).
fn fake_png(w: u32, h: u32, total: usize) -> Vec<u8> {
    let mut v = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R'];
    v.extend(w.to_be_bytes());
    v.extend(h.to_be_bytes());
    v.extend([8, 2, 0, 0, 0]);
    v.extend(noise(total - v.len()));
    v
}

fn file(dir: &crate::handoff::Scratch, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.0.join(name);
    std::fs::File::create(&path).unwrap().write_all(bytes).unwrap();
    path
}

#[test]
fn a_small_text_is_served_in_one_property_targets_list_the_four_forms_and_the_job_is_confirmed_after_ownership() {
    let rig = Rig::start();
    let said = rig.give(Some("c1"), Source::Text("안녕 \u{1F600}\nline".into())).unwrap();
    assert_eq!(said["ev"], "clip.local");
    assert_eq!((said["req"].as_str(), said["ok"].as_bool(), said["kind"].as_str()), (Some("c1"), Some(true), Some("text")));
    assert_eq!(said["bytes"], "안녕 \u{1F600}\nline".len());
    let r = rig.requestor();
    for target in ["UTF8_STRING", "STRING", "TEXT"] {
        assert_eq!(r.ask(target), Got::Bytes { ptype: "UTF8_STRING".into(), data: "안녕 \u{1F600}\nline".as_bytes().to_vec(), incr: None }, "{target}");
    }
    assert_eq!(r.ask("TARGETS"), Got::Atoms(["TARGETS", "UTF8_STRING", "STRING", "TEXT"].map(String::from).to_vec()));
    // the picture form is not held while text is: refused, and so is anything else
    assert_eq!(r.ask("image/png"), Got::Refused);
    assert_eq!(r.ask("text/html"), Got::Refused);
}

#[test]
fn a_job_without_a_request_id_is_owned_but_reports_nothing() {
    let rig = Rig::start();
    assert!(rig.give(None, Source::Text("copied by a key".into())).is_none());
    rig.silent();
    assert_eq!(rig.requestor().ask("UTF8_STRING"), Got::Bytes { ptype: "UTF8_STRING".into(), data: b"copied by a key".to_vec(), incr: None });
}

#[test]
fn a_picture_is_served_with_targets_of_a_picture_and_text_is_refused_while_it_is_held() {
    let rig = Rig::start();
    let dir = crate::handoff::Scratch::new("clipx_small");
    let png = fake_png(30, 20, 2000);
    let path = file(&dir, "small.png", &png);
    let said = rig.give(Some("c2"), Source::File(path)).unwrap();
    assert_eq!((said["ok"].as_bool(), said["kind"].as_str(), said["bytes"].as_u64(), said["width"].as_u64(), said["height"].as_u64()), (Some(true), Some("image"), Some(2000), Some(30), Some(20)));
    let r = rig.requestor();
    assert_eq!(r.ask("TARGETS"), Got::Atoms(["TARGETS", "image/png"].map(String::from).to_vec()));
    assert_eq!(r.ask("image/png"), Got::Bytes { ptype: "image/png".into(), data: png, incr: None });
    for text in ["UTF8_STRING", "STRING", "TEXT"] {
        assert_eq!(r.ask(text), Got::Refused, "{text} while a picture is held");
    }
}

#[test]
fn a_large_picture_goes_by_the_incremental_protocol_in_chunks_within_the_limit_and_ends_with_an_empty_chunk() {
    let rig = Rig::start();
    let dir = crate::handoff::Scratch::new("clipx_big");
    let png = fake_png(1000, 1000, 2_000_000);
    rig.give(Some("c3"), Source::File(file(&dir, "big.png", &png))).unwrap();
    let r = rig.requestor();
    let Got::Bytes { ptype, data, incr: Some(facts) } = r.ask("image/png") else { panic!("expected an incremental transfer") };
    assert_eq!(ptype, "image/png");
    assert!(data == png, "the picture arrived whole and in order");
    assert_eq!(facts.announced as usize, png.len(), "the announcement carries the size");
    assert!(facts.chunks.iter().all(|c| *c <= 128 * 1024), "a chunk over the limit: {:?}", facts.chunks);
    assert_eq!(*facts.chunks.last().unwrap(), 0, "the last chunk is empty");
    assert_eq!(facts.chunks.iter().filter(|c| **c == 0).count(), 1);
    assert_eq!(facts.chunks.len(), 2_000_000usize.div_ceil(128 * 1024) + 1);
    // a second requestor, afterwards, gets it whole again
    let Got::Bytes { data, .. } = rig.requestor().ask("image/png") else { panic!() };
    assert!(data == png);
    // a text just under the threshold is one property, and one over it goes by INCR as well
    rig.give(Some("c4"), Source::Text("a".repeat(INCR_ABOVE))).unwrap();
    assert!(matches!(r.ask("UTF8_STRING"), Got::Bytes { incr: None, .. }));
    rig.give(Some("c5"), Source::Text("a".repeat(INCR_ABOVE + 1))).unwrap();
    assert!(matches!(r.ask("UTF8_STRING"), Got::Bytes { incr: Some(_), .. }));
}

#[test]
fn a_requestor_that_stalls_does_not_block_a_second_one_and_is_dropped_after_five_seconds() {
    let rig = Rig::start();
    let dir = crate::handoff::Scratch::new("clipx_stall");
    let png = fake_png(1000, 1000, 1_000_000);
    rig.give(Some("c6"), Source::File(file(&dir, "stall.png", &png))).unwrap();
    let stalled = rig.requestor();
    stalled.ask_then_stall("image/png");
    let began = Instant::now();
    let Got::Bytes { data, incr: Some(_), .. } = rig.requestor().ask("image/png") else { panic!("the second requestor was not served") };
    assert!(data == png);
    assert!(began.elapsed() < Duration::from_secs(4), "the second requestor waited for the stalled one: {:?}", began.elapsed());
    // the stalled one: its first chunk was written when it deleted the announcement; reading it after the stall time starts nothing more
    thread::sleep(STALL + Duration::from_millis(1500));
    while stalled.event(Duration::from_millis(50)).is_some() {} // what happened while it slept
    let first = stalled.take(true);
    assert!(!first.value.is_empty(), "the first chunk was on the property");
    let again = stalled.event(Duration::from_millis(1200));
    assert!(!matches!(again, Some(Event::PropertyNotify(e)) if e.state == Property::NEW_VALUE), "a dropped transfer wrote another chunk");
    // and the owner still serves: a small request works after the drop
    rig.give(None, Source::Text("after".into()));
    assert_eq!(rig.requestor().ask("UTF8_STRING"), Got::Bytes { ptype: "UTF8_STRING".into(), data: b"after".to_vec(), incr: None });
}

#[test]
fn a_second_write_replaces_the_first_and_another_client_taking_the_selection_makes_the_window_release_its_content() {
    let rig = Rig::start();
    rig.give(Some("c7"), Source::Text("first".into())).unwrap();
    rig.give(Some("c8"), Source::Text("second".into())).unwrap();
    let r = rig.requestor();
    assert_eq!(r.ask("UTF8_STRING"), Got::Bytes { ptype: "UTF8_STRING".into(), data: b"second".to_vec(), incr: None });
    // another client takes the selection: the server sends SelectionClear to the window, which lets its content go
    let thief = rig.requestor();
    thief.conn.set_selection_owner(thief.win, thief.selection, x11rb::CURRENT_TIME).unwrap();
    thief.conn.flush().unwrap();
    thread::sleep(Duration::from_millis(300));
    // the thief does not answer requests, so ask the window directly through a synthetic request: the conversion of a window that holds nothing is refused
    let owner_now = r.conn.get_selection_owner(r.selection).unwrap().reply().unwrap().owner;
    assert_eq!(owner_now, thief.win);
    // a request addressed to the old owner's helper window is what SelectionClear leaves behind: find it as the other window of the server's tree
    let tree = r.conn.query_tree(thief.conn.setup().roots[0].root).unwrap().reply().unwrap();
    let mut refused = 0;
    for child in tree.children.iter().filter(|c| **c != r.win && **c != thief.win) {
        let ev = x11rb::protocol::xproto::SelectionRequestEvent {
            response_type: x11rb::protocol::xproto::SELECTION_REQUEST_EVENT,
            sequence: 0,
            time: x11rb::CURRENT_TIME,
            owner: *child,
            requestor: r.win,
            selection: r.selection,
            target: r.atom("UTF8_STRING"),
            property: r.property,
        };
        r.conn.send_event(false, *child, EventMask::NO_EVENT, ev).unwrap();
        r.conn.flush().unwrap();
        let end = Instant::now() + Duration::from_millis(800);
        while Instant::now() < end {
            if let Some(Event::SelectionNotify(n)) = r.event(Duration::from_millis(50)) {
                assert_eq!(n.property, x11rb::NONE, "the window answered with content it should have released");
                refused += 1;
                break;
            }
        }
    }
    assert_eq!(refused, 1, "exactly one helper window of the owner answers, and it refuses");
}

#[test]
fn a_picture_that_cannot_be_used_is_reported_with_its_reason_and_the_held_value_is_unchanged() {
    let rig = Rig::start();
    let dir = crate::handoff::Scratch::new("clipx_bad");
    rig.give(Some("c9"), Source::Text("kept".into())).unwrap();
    let cases: Vec<(&str, PathBuf, &str)> = vec![
        ("not a png", file(&dir, "text.png", b"this is not a picture, it is plain words of some length"), "bad_image"),
        ("9000 px wide", file(&dir, "wide.png", &fake_png(9000, 10, 400)), "too_large"),
        ("9000 px high", file(&dir, "high.png", &fake_png(10, 9000, 400)), "too_large"),
        ("missing", dir.0.join("missing.png"), "write_failed"),
        ("empty", file(&dir, "empty.png", b""), "bad_image"),
        ("3 MiB + 1", file(&dir, "over.png", &fake_png(10, 10, 3 * 1024 * 1024 + 1)), "too_large"),
        ("a folder", dir.0.clone(), "write_failed"),
    ];
    for (name, path, reason) in cases {
        let said = rig.give(Some("cbad"), Source::File(path)).unwrap();
        assert_eq!((said["ok"].as_bool(), said["error"].as_str()), (Some(false), Some(reason)), "{name}");
        assert_eq!(rig.requestor().ask("UTF8_STRING"), Got::Bytes { ptype: "UTF8_STRING".into(), data: b"kept".to_vec(), incr: None }, "{name}: the held text changed");
    }
    // exactly 3 MiB is taken
    let said = rig.give(Some("cok"), Source::File(file(&dir, "exact.png", &fake_png(10, 10, 3 * 1024 * 1024)))).unwrap();
    assert_eq!(said["ok"].as_bool(), Some(true));
}

#[test]
fn the_pure_choice_of_the_answer_and_the_state_machine_of_a_transfer() {
    let atoms = Atoms { targets: 100, utf8: 101, text: 102, png: 103 };
    let text = Held::Text(Arc::new(b"t".to_vec()));
    let png = Held::Png(Arc::new(b"p".to_vec()));
    assert_eq!(answer(&text, 100, &atoms), Reply::Targets(vec![100, 101, 31, 102]));
    assert_eq!(answer(&png, 100, &atoms), Reply::Targets(vec![100, 103]));
    assert!(matches!(answer(&text, 101, &atoms), Reply::Data { ptype: 101, .. }));
    assert!(matches!(answer(&text, 31, &atoms), Reply::Data { ptype: 101, .. }));
    assert!(matches!(answer(&png, 103, &atoms), Reply::Data { ptype: 103, .. }));
    assert_eq!(answer(&text, 103, &atoms), Reply::Refuse);
    for target in [101, 102, 31, 7] {
        assert_eq!(answer(&png, target, &atoms), Reply::Refuse);
    }
    let now = Instant::now();
    let mut t = Transfer::new(1, 2, 3, Arc::new(vec![0; CHUNK * 2 + 5]), now);
    assert_eq!(t.step(now), Step::Chunk(0..CHUNK));
    assert_eq!(t.step(now), Step::Chunk(CHUNK..CHUNK * 2));
    assert_eq!(t.step(now), Step::Chunk(CHUNK * 2..CHUNK * 2 + 5));
    assert_eq!(t.step(now), Step::Chunk(CHUNK * 2 + 5..CHUNK * 2 + 5), "the empty chunk");
    assert_eq!(t.step(now), Step::Done);
    assert!(!t.stalled(now + STALL - Duration::from_millis(1)) && t.stalled(now + STALL));
}

#[test]
fn a_value_is_confirmed_only_when_the_server_says_this_window_owns_the_selection() {
    // another client takes the selection between the window's request for it and its question who owns it: the job must fail with x_error, say so,
    // and the value must not be held (a later request is refused, not served)
    let x = Xvfb::start();
    let display = x.name.clone();
    let thief = Requestor::new(&display);
    let armed = std::sync::atomic::AtomicBool::new(true);
    let (jobs, rx) = mpsc::channel();
    let (tx, reports) = mpsc::channel();
    let tx = std::sync::Mutex::new(tx);
    let d = display.clone();
    thread::spawn(move || {
        let between = || {
            if armed.swap(false, std::sync::atomic::Ordering::SeqCst) {
                // two connections are not ordered by the order of their flushes: wait until the server shows the window as the owner (under load the
                // thief's request could otherwise be handled first, and the window would then simply win), and only then take the selection
                let began = std::time::Instant::now();
                while thief.conn.get_selection_owner(thief.selection).unwrap().reply().unwrap().owner == x11rb::NONE {
                    assert!(began.elapsed() < Duration::from_secs(5), "the window never became the owner");
                    thread::sleep(Duration::from_millis(2));
                }
                thief.conn.set_selection_owner(thief.win, thief.selection, x11rb::CURRENT_TIME).unwrap();
                thief.conn.get_input_focus().unwrap().reply().unwrap(); // the server has handled it
            }
        };
        serve_with(Some(&d), rx, &move |line| drop(tx.lock().unwrap().send(line)), &between);
    });
    jobs.send(Job { req: Some("race".into()), source: Source::Text("lost the race".into()), clipboard: true, primary: false }).unwrap();
    let said: serde_json::Value = serde_json::from_str(&reports.recv_timeout(Duration::from_secs(10)).unwrap()).unwrap();
    assert_eq!((said["ok"].as_bool(), said["error"].as_str()), (Some(false), Some("x_error")), "{said}");
    // the next job is not raced: it is confirmed
    jobs.send(Job { req: Some("fine".into()), source: Source::Text("won".into()), clipboard: true, primary: false }).unwrap();
    let said: serde_json::Value = serde_json::from_str(&reports.recv_timeout(Duration::from_secs(10)).unwrap()).unwrap();
    assert_eq!(said["ok"].as_bool(), Some(true), "{said}");
}

// ---- the middle button (S06): the hit test and the choice, pure

#[test]
fn the_box_takes_its_left_and_top_edge_and_not_its_right_and_bottom_edge_at_any_scale() {
    for s in [1.0f32, 2.0] {
        let rect = Some([100.0 * s, 50.0 * s, 300.0 * s, 90.0 * s]);
        let inside = |x: f32, y: f32| in_box(rect, x * s, y * s);
        assert!(inside(100.0, 50.0) && inside(299.0, 89.0) && inside(200.0, 70.0), "inside, and the left and top edges");
        assert!(!inside(300.0, 70.0) && !inside(200.0, 90.0), "the right and bottom edges are outside");
        assert!(!inside(99.0, 70.0) && !inside(200.0, 49.0), "one pixel outside on the left and the top");
        assert!(!inside(301.0, 70.0) && !inside(200.0, 91.0), "one pixel outside on the right and the bottom");
        assert!(in_box(rect, 100.0 * s, 50.0 * s) && !in_box(rect, 100.0 * s - 0.5, 50.0 * s) && !in_box(rect, 100.0 * s, 50.0 * s - 0.5), "half a pixel decides");
    }
    assert!(!in_box(None, 150.0, 60.0), "no rectangle (a start screen): nowhere");
}

#[test]
fn only_the_press_of_the_middle_button_over_the_box_with_no_menu_asks_for_the_primary_selection() {
    use ElementState::{Pressed, Released};
    let rect = Some([100.0, 50.0, 300.0, 90.0]);
    let at = (150.0, 60.0);
    assert!(wants_primary(Pressed, MouseButton::Middle, false, rect, at));
    assert!(!wants_primary(Released, MouseButton::Middle, false, rect, at), "the release does nothing: one press, one paste");
    for other in [MouseButton::Left, MouseButton::Right, MouseButton::Back, MouseButton::Forward, MouseButton::Other(9)] {
        assert!(!wants_primary(Pressed, other, false, rect, at), "{other:?}");
    }
    assert!(!wants_primary(Pressed, MouseButton::Middle, true, rect, at), "with a menu open the press is ignored");
    assert!(!wants_primary(Pressed, MouseButton::Middle, false, rect, (150.0, 95.0)), "outside the box");
    assert!(!wants_primary(Pressed, MouseButton::Middle, false, None, at), "no box");
}

#[test]
fn explicit_copy_owns_clipboard_without_disturbing_primary_selection() {
    let rig = Rig::start();
    rig.jobs.send(Job { req: None, source: Source::Text("middle text".into()), clipboard: false, primary: true }).unwrap();
    rig.jobs.send(Job::text("결정3:SLM-EMR\t승인".into())).unwrap();
    rig.silent();
    let clip = rig.requestor();
    let prim = rig.primary_requestor();
    assert_eq!(clip.ask("UTF8_STRING"), Got::Bytes { ptype: "UTF8_STRING".into(), data: "결정3:SLM-EMR\t승인".as_bytes().to_vec(), incr: None });
    assert_eq!(prim.ask("UTF8_STRING"), Got::Bytes { ptype: "UTF8_STRING".into(), data: b"middle text".to_vec(), incr: None });
}
