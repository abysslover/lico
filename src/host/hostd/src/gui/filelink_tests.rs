//! A file under the pointer (`M24-P11e`): what a name is, whether it is there, the name under the pointer in a laid-out conversation, its underline and the click.

use super::*;
use crate::gui::scroll_oracle::app;
use crate::gui::ui::Row;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

const W: usize = 1400;
const H: usize = 900;

/// The names of a text as (the bytes of the text they cover, the path, the line, the column).
fn found(text: &str) -> Vec<(String, String, Option<u32>, Option<u32>)> {
    names(text).into_iter().map(|n| (text[n.start..n.end].to_string(), n.path, n.line, n.column)).collect()
}

fn one(text: &str) -> (String, Option<u32>, Option<u32>) {
    let all = found(text);
    assert_eq!(all.len(), 1, "one name in {text:?}: {all:?}");
    (all[0].1.clone(), all[0].2, all[0].3)
}

#[test]
fn a_name_is_a_path_that_starts_as_one_or_has_an_extension_with_the_marks_around_it_taken_off() {
    assert_eq!(found("/tmp/x.log"), [("/tmp/x.log".into(), "/tmp/x.log".into(), None, None)]);
    assert_eq!(one("see ~/notes.md now").0, "~/notes.md");
    let two = found("edit ./a.rs and ../b/c.txt");
    assert_eq!(two.iter().map(|n| n.1.as_str()).collect::<Vec<_>>(), ["./a.rs", "../b/c.txt"]);
    assert_eq!(one("src/gui/ui.rs").0, "src/gui/ui.rs", "a part with an extension after a slash");
    assert_eq!(one("Cargo.toml").0, "Cargo.toml", "a bare name with an extension");
    for rooted in ["/usr/bin", "~/Documents", "./src", "../src"] {
        assert_eq!(one(rooted).0, rooted, "a rooted path needs no extension (it must be there to be a link)");
    }
    // the position after the path is part of the name's bytes and not of its path
    assert_eq!(found("at src/a.rs:12 now"), [("src/a.rs:12".into(), "src/a.rs".into(), Some(12), None)]);
    assert_eq!(found("src/a.rs:12:3"), [("src/a.rs:12:3".into(), "src/a.rs".into(), Some(12), Some(3))]);
    assert!(found("a.rs:99999999999").is_empty(), "a number that is no line number (too big) is not a position, and the rest is no extension");
    assert!(found("src/a.rs:12:3:7").is_empty(), "two numbers at most after a path: a third makes the rest of the path no extension");
    // a sentence's marks around a name are not its own
    for text in ["main.rs.", "`main.rs`,", "(see main.rs)", "\"main.rs\"", "'main.rs'", "main.rs:", "main.rs!?", "[main.rs]", "{main.rs}", "<main.rs>", "**main.rs**", "--out=main.rs", "a|main.rs;", "(main.rs)", "see\tmain.rs\nnext", "x\u{a0}main.rs"] {
        assert_eq!(found(text).iter().map(|n| n.0.as_str()).collect::<Vec<_>>(), ["main.rs"], "{text}");
    }
    assert_eq!(found("main.rs:12.").iter().map(|n| (n.0.as_str(), n.2)).collect::<Vec<_>>(), [("main.rs:12", Some(12))], "a full stop after the line");
    assert_eq!(found("a.rs, b.rs; c.rs").len(), 3);
    assert_eq!(found("see\ta.rs\nb.rs").len(), 2, "a tab and a new line end a name as a space does");
    // digits are letters of an extension too (`clip.mp4`, `a.7z`); a hidden file with no extension is a name only with its folder rooted (`~/.bashrc`)
    assert_eq!(found("clip.mp4 a.7z h264.h264").iter().map(|n| n.1.as_str()).collect::<Vec<_>>(), ["clip.mp4", "a.7z", "h264.h264"]);
    assert!(found("a/.bashrc").is_empty() && found(".bashrc").is_empty(), "a part that starts with the full stop has no stem: no extension");
    assert_eq!(one("~/.bashrc").0, "~/.bashrc");
    // the extension is one to eight letters and digits with a letter among them
    for text in ["file.abcdefghi", "x.1", "0.8.2", "1.5", "v1.2.3", "a.b-c", "10:30", "10:30:45", "and/or", "the end.", "...", "/", "~/", "./", "a.", ".rs"] {
        assert!(found(text).is_empty(), "{text} is not a name: {:?}", found(text));
    }
    for text in ["https://example.com/a.html", "see http://x.org/p.png", "file:///tmp/a.txt"] {
        assert!(found(text).is_empty(), "a web address is not a name: {text}");
    }
    assert_eq!(one("file.abcdefgh").0, "file.abcdefgh", "eight letters are the longest extension");
    assert_eq!(one("a.b").0, "a.b", "one is the shortest");
}

#[test]
fn a_korean_particle_after_a_name_is_not_part_of_it_and_the_bytes_stay_right() {
    let text = "열기: src/main.rs를 보세요";
    let all = names(text);
    assert_eq!(all.len(), 1);
    assert_eq!((&text[all[0].start..all[0].end], all[0].path.as_str()), ("src/main.rs", "src/main.rs"));
    assert_eq!(all[0].start, "열기: ".len(), "the name starts after the Korean text, in bytes");
    assert_eq!(one("~/문서/보고서.md입니다"), ("~/문서/보고서.md".into(), None, None), "Korean letters in the path stay, the three after the extension go");
    assert_eq!(one("a.rs:12에서"), ("a.rs".into(), Some(12), None), "after the position too");
    assert_eq!(one("/tmp/보고").0, "/tmp/보고", "a folder with a Korean name keeps it: the letters after a slash are not a particle");
    assert_eq!(one("/tmp/x.log은.").0, "/tmp/x.log", "a particle and a full stop");
    assert_eq!(one("/tmp/a1을").0, "/tmp/a1", "after a digit as well as after a letter");
    assert_eq!(one("/tmp/x.log입니다만").0, "/tmp/x.log입니다만", "four letters are more than a particle: they stay");
    assert_eq!(one("보고서.md").0, "보고서.md", "a Korean stem");
    assert!(found("파일을 읽었습니다").is_empty());
}

#[test]
fn a_name_is_found_where_it_is_on_the_machine() {
    assert_eq!(resolve("~/n.md", "/w", "/h"), Some(PathBuf::from("/h/n.md")));
    assert_eq!(resolve("~/n.md", "/w", ""), None, "no home folder, no `~/`");
    assert_eq!(resolve("/a/b.rs", "/w", "/h"), Some(PathBuf::from("/a/b.rs")));
    assert_eq!(resolve("src/a.rs", "/w", "/h"), Some(PathBuf::from("/w/src/a.rs")));
    assert_eq!(resolve("./a.rs", "/w", "/h"), Some(PathBuf::from("/w/./a.rs")));
    assert_eq!(resolve("../a.rs", "/w", "/h"), Some(PathBuf::from("/w/../a.rs")));
    assert_eq!(resolve("a.rs", "", "/h"), None, "a session with no folder has no relative names");
    assert_eq!(resolve("/a/b.rs", "", ""), Some(PathBuf::from("/a/b.rs")), "a rooted one needs none");
}

fn temp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lico_filelink_{tag}_{}", crate::core::random_hex(6)));
    std::fs::create_dir_all(&dir).expect("a temporary folder");
    dir
}

#[test]
fn the_machine_is_asked_for_a_short_while_and_the_answers_are_kept_for_a_few_seconds() {
    let dir = temp("ask");
    std::fs::write(dir.join("a.txt"), "x").expect("a file");
    std::fs::create_dir(dir.join("d")).expect("a folder");
    assert_eq!((probe(&dir.join("a.txt")), probe(&dir.join("d")), probe(&dir.join("nope"))), (Some(Kind::File), Some(Kind::Folder), None));
    let socket = dir.join("s.sock");
    let _listener = std::os::unix::net::UnixListener::bind(&socket).expect("a socket");
    assert_eq!(probe(&socket), None, "a socket is neither a file nor a folder");
    assert_eq!((BUDGET, KEEP, TABLE, OUT), (Duration::from_millis(30), Duration::from_secs(5), 256, 16));
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = |kind: Option<Kind>, nap: u64| {
        let calls = Arc::clone(&calls);
        move |_: &Path| {
            calls.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(nap));
            kind
        }
    };
    let (mut st, t0, long) = (State::default(), Instant::now(), Duration::from_secs(2));
    let p = Path::new("/w/a.rs");
    assert_eq!(st.ask(p, t0, long, counted(Some(Kind::File), 0)), Some(Kind::File));
    assert_eq!(st.ask(p, t0 + Duration::from_secs(4), long, counted(None, 0)), Some(Kind::File), "kept for 5 seconds: the probe is not run again");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(st.ask(p, t0 + Duration::from_secs(5), long, counted(None, 0)), None, "after 5 seconds it is asked again");
    assert_eq!(st.ask(p, t0 + Duration::from_secs(6), long, counted(Some(Kind::File), 0)), None, "and a nothing is kept as well");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    // an answer that does not come in time is not one, and is not asked for again while it is out; its late answer is taken by the next look
    let slow = Path::new("/mnt/slow/x.rs");
    assert_eq!(st.ask(slow, t0, Duration::from_millis(10), counted(Some(Kind::Folder), 150)), None, "no answer in time: not a link now");
    assert_eq!(st.ask(slow, t0, Duration::from_millis(10), counted(Some(Kind::File), 0)), None, "still out: not asked again");
    assert_eq!(calls.load(Ordering::SeqCst), 3, "one look went out for the slow path");
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(st.ask(slow, t0, Duration::from_millis(10), counted(Some(Kind::File), 0)), Some(Kind::Folder), "the late answer is used");
    assert_eq!(st.ask(slow, t0, Duration::from_millis(10), counted(Some(Kind::File), 0)), Some(Kind::Folder), "and kept");
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    // after the late answer is used and its time is out the path is asked afresh (a late look that is still out is not left standing)
    assert_eq!(st.ask(slow, t0 + Duration::from_secs(6), long, counted(Some(Kind::File), 0)), Some(Kind::File), "asked afresh, not left waiting for the look that came");
    // a look whose thread dies without an answer does not block the path for good
    let dead = Path::new("/mnt/dead/x.rs");
    assert_eq!(st.ask(dead, t0, long, |_: &Path| -> Option<Kind> { panic!("a probe that fails (the test's own)") }), None);
    assert_eq!(st.ask(dead, t0, long, counted(Some(Kind::File), 0)), None, "the dead look is dropped: this look is not made yet");
    assert_eq!(st.ask(dead, t0, long, counted(Some(Kind::File), 0)), Some(Kind::File), "and the path is asked afresh");
    // the table keeps 256 answers; the first look past it starts it over
    let mut table = State::default();
    for i in 0..TABLE {
        table.ask(Path::new(&format!("/w/f{i}.rs")), t0, long, |_: &Path| None);
    }
    assert_eq!(table.known.len(), 256, "all of them are kept");
    table.ask(Path::new("/w/one-more.rs"), t0, long, |_: &Path| None);
    assert_eq!(table.known.len(), 1, "the 257th starts the table again");
    // at most 16 looks are out at once: the 17th is not made
    let mut out = State::default();
    for i in 0..OUT {
        out.ask(Path::new(&format!("/mnt/slow/{i}.rs")), t0, Duration::from_millis(1), counted(None, 400));
    }
    std::thread::sleep(Duration::from_millis(60)); // (every thread has counted its look)
    let before = calls.load(Ordering::SeqCst);
    assert_eq!(out.ask(Path::new("/mnt/slow/17.rs"), t0, Duration::from_millis(1), counted(None, 0)), None);
    assert_eq!(calls.load(Ordering::SeqCst), before, "no seventeenth look went out");
    let _ = std::fs::remove_dir_all(dir);
}

fn row(cwd: &Path) -> Row {
    Row { session: "session-s0".into(), provider: "claude".into(), account: "work".into(), state: "running".into(), activity: "idle".into(), cwd: cwd.to_string_lossy().into(), ..Default::default() }
}

/// A window of `w` x `H` at `scale` showing `lines` of a conversation in `cwd`, drawn once.
fn scene(scale: f32, w: usize, lines: &[&str], cwd: &Path) -> (App, Vec<u32>) {
    let mut a = app(scale, false);
    a.ui.rows = vec![row(cwd)];
    for (i, text) in lines.iter().enumerate() {
        a.push_line(i as i64 + 1, text.to_string());
    }
    let mut buf = vec![0u32; w * H];
    assert!(a.compose(&mut buf, w, H, 0).is_some(), "nothing was drawn");
    (a, buf)
}

/// The window's pixel at the middle of `needle` in the text of the conversation (its first row when it wraps).
fn point(a: &App, needle: &str) -> (f64, f64) {
    let buffer = &a.view.as_ref().expect("a view").buffer;
    let (ox, oy) = a.ui.links.origin;
    for run in buffer.layout_runs() {
        let text = buffer.lines[run.line_i].text();
        let Some(at) = text.find(needle) else { continue };
        let inside: Vec<_> = run.glyphs.iter().filter(|g| g.start >= at && g.end <= at + needle.len()).collect();
        if let (Some(first), Some(last)) = (inside.first(), inside.last()) {
            return (((ox + (first.x + last.x + last.w) / 2.0) as f64), ((oy + run.line_top + run.line_height / 2.0) as f64));
        }
    }
    panic!("{needle:?} is not on the screen");
}

/// The middle of the character just after `needle` (or the one just before it) in the text of the conversation.
fn beside(a: &App, needle: &str, after: bool) -> (f64, f64) {
    let buffer = &a.view.as_ref().expect("a view").buffer;
    let (ox, oy) = a.ui.links.origin;
    for run in buffer.layout_runs() {
        let Some(at) = buffer.lines[run.line_i].text().find(needle) else { continue };
        let target = if after { at + needle.len() } else { at - 1 };
        if let Some(g) = run.glyphs.iter().find(|g| g.start == target) {
            return ((ox + g.x + g.w / 2.0) as f64, (oy + run.line_top + run.line_height / 2.0) as f64);
        }
    }
    panic!("nothing beside {needle:?}");
}

fn hovered(a: &mut App, at: (f64, f64)) -> Option<(String, Kind)> {
    a.ui.mouse = at;
    let _ = a.file_link_moved();
    a.ui.links.hover.as_ref().map(|h| (h.name.clone(), h.kind))
}

/// The name of `needle` under the pointer when it is put in the middle of `needle`.
fn over(a: &mut App, needle: &str) -> Option<(String, Kind)> {
    let at = point(a, needle);
    hovered(a, at)
}

const LINE: &str = "see notes.md and missing.txt in ./src, then ~/nothing.md or /usr/bin ok, line ./src/inner.rs:9 end";

#[test]
fn the_name_under_the_pointer_is_found_in_the_layout_only_when_it_is_there() {
    let dir = temp("hover");
    std::fs::write(dir.join("notes.md"), "# notes\n").expect("a file");
    std::fs::create_dir(dir.join("src")).expect("a folder");
    std::fs::write(dir.join("src/inner.rs"), "fn main() {}\n").expect("a file");
    let (mut a, _) = scene(1.0, W, &[LINE], &dir);
    assert_eq!(a.ui.links.json()["armed"], true, "names are looked for in a conversation");
    a.ui.mouse = point(&a, "./src/inner.rs:9");
    assert!(a.file_link_moved(), "a name came under the pointer: the window is to be drawn again");
    assert_eq!(over(&mut a, "./src/inner.rs:9"), Some(("./src/inner.rs".into(), Kind::File)));
    assert_eq!(a.ui.links.hover.as_ref().and_then(|h| h.line), Some(9), "the line after the name");
    let ctx = a.file_link_ctx(false).expect("a conversation");
    assert_eq!((ctx.cwd.as_str(), ctx.home), (dir.to_str().expect("a path"), std::env::var("HOME").unwrap_or_default()), "names are looked for in the folder of the session and the home folder");
    assert_eq!(over(&mut a, "notes.md"), Some(("notes.md".into(), Kind::File)));
    assert!(a.ui.links.hand, "the pointer is a hand over a name");
    assert!(!a.file_link_moved(), "the same name under the pointer: nothing changed, nothing to draw again");
    let (before, after) = (beside(&a, "notes.md", false), beside(&a, "notes.md", true));
    assert_eq!(hovered(&mut a, before), None, "the space before the name is not the name");
    assert!(!a.ui.links.hand, "and the pointer is the window's own again");
    assert_eq!(hovered(&mut a, after), None, "nor the space after it");
    assert_eq!(over(&mut a, "notes.md").map(|h| h.0), Some("notes.md".into()));
    assert_eq!(a.ui.links.hover.as_ref().map(|h| h.path.clone()), Some(dir.join("notes.md")), "relative to the folder of the session");
    assert_eq!(over(&mut a, "./src"), Some(("./src".into(), Kind::Folder)), "a folder is a link as well");
    assert_eq!(over(&mut a, "/usr/bin").map(|h| h.1), Some(Kind::Folder), "a rooted path needs no session folder");
    assert_eq!(over(&mut a, "missing.txt"), None, "a name that is not a file is no link");
    assert_eq!(over(&mut a, "~/nothing.md"), None);
    assert_eq!(over(&mut a, "then"), None, "a word is not a name");
    let (x, y) = point(&a, "notes.md");
    assert_eq!(hovered(&mut a, (x, y)).map(|h| h.0), Some("notes.md".into()));
    assert_eq!(hovered(&mut a, (x, y + 400.0)), None, "under the lines");
    assert_eq!(hovered(&mut a, (W as f64 - 3.0, y)), None, "to the right of the text, where the gutter is");
    assert_eq!(hovered(&mut a, (10.0, y)), None, "in the list at the left");
    let last = a.ui.hit_lines.last().copied().expect("a line").1;
    assert_eq!(hovered(&mut a, (x, last as f64 + 20.0)), None, "in the rows under the conversation (the strip, the input box)");
    // where there are no links: a panel, a session on another machine, a session with no folder keeps its rooted names only
    assert!(a.file_link_ctx(true).is_none(), "a panel");
    a.ui.calls.active_call = Some("c1".into());
    assert!(a.file_link_ctx(false).is_none(), "the Call Room");
    a.ui.calls.active_call = None;
    let mut tabs = app(1.0, false); // a browser tab and a terminal tab are shown with no session selected
    (tabs.session, tabs.ui.browser.prompt) = (String::new(), Some(false));
    assert!(tabs.browser_active() && tabs.file_link_ctx(false).is_none(), "a browser tab");
    tabs.ui.browser.prompt = None;
    tabs.on_terminal_async(crate::gui::terminal::TermJob::Open, json!({"ok": 1, "tab": "t1", "pid": 7, "cwd": "/work/one", "shell": "/bin/sh", "cols": 80, "rows": 24}));
    assert!(tabs.terminal_active() && tabs.file_link_ctx(false).is_none(), "a terminal tab");
    a.ui.rows[0].ssh_host = "box".into();
    assert!(a.file_link_ctx(false).is_none(), "a session on another machine");
    a.ui.rows[0].ssh_host.clear();
    a.ui.rows[0].cwd.clear();
    let mut buf = vec![0u32; W * H];
    assert!(a.compose(&mut buf, W, H, 1).is_some());
    assert_eq!(over(&mut a, "notes.md"), None, "no folder, no relative names");
    assert_eq!(over(&mut a, "/usr/bin").map(|h| h.1), Some(Kind::Folder));
    a.ui.workbench = false;
    assert!(a.file_link_ctx(false).is_none(), "a window with no workbench");
    let mut plain = vec![0u32; W * H];
    assert!(a.compose(&mut plain, W, H, 2).is_some());
    assert_eq!(a.ui.links.json()["armed"], false, "a window with no workbench does not look for names");
    // a name typed in the input box is the person's own text: no link
    let (mut b, _) = scene(1.0, W, &["nothing to open here"], &dir);
    b.ed.set("notes.md and more".into());
    let mut buf = vec![0u32; W * H];
    assert!(b.compose(&mut buf, W, H, 0).is_some());
    assert_eq!(over(&mut b, "notes.md"), None, "the input box has no links");
    let _ = std::fs::remove_dir_all(dir);
}

/// The pixels of one frame that are not those of a frame with the pointer elsewhere.
fn changed(a: &[u32], b: &[u32], w: usize) -> Vec<(usize, usize)> {
    a.iter().zip(b).enumerate().filter(|(_, (p, q))| p != q).map(|(i, _)| (i % w, i / w)).collect()
}

#[test]
fn the_name_is_underlined_in_the_ink_of_the_text_one_pixel_row_under_its_glyphs_and_nothing_else_changes() {
    let dir = temp("underline");
    std::fs::write(dir.join("notes.md"), "x").expect("a file");
    for scale in [1.0f32, 1.5, 2.0] {
        let (mut a, plain) = scene(scale, W, &[LINE], &dir);
        assert_eq!(a.ui.links.json()["hover"], Value::Null, "no hover, no underline (scale {scale})");
        a.ui.mouse = point(&a, "notes.md");
        let mut buf = vec![0u32; W * H];
        assert!(a.compose(&mut buf, W, H, 0).is_some());
        let at = format!("scale {scale}");
        let top = a.ui.composed.as_ref().expect("a frame").top as i32 as f32;
        assert_eq!(a.ui.links.origin, ((a.effective_list_w(scale) + (4.0 * scale) as usize) as f32, top), "the text is drawn from here: the list's edge and 4 units, the first row of the conversation ({at})");
        let json = a.ui.links.json();
        assert_eq!(json["hover"]["name"], "notes.md", "{at}");
        let rects: Vec<[f32; 4]> = serde_json::from_value(json["hover"]["underline"].clone()).expect("the underline's rectangles");
        assert_eq!(rects.len(), 1, "one row of text, one stretch ({at})");
        let [x0, y0, x1, y1] = rects[0];
        assert_eq!(y1 - y0, scale.round(), "as thick as the scale ({at})");
        let ink = Color::rgb(0xE0, 0xE0, 0xE0);
        let argb = 0xFF00_0000 | (ink.r() as u32) << 16 | (ink.g() as u32) << 8 | ink.b() as u32;
        let (line_y, line_top, line_h) = a.view.as_ref().expect("a view").buffer.layout_runs().next().map(|r| (r.line_y, r.line_top, r.line_height)).expect("a row");
        let oy = a.ui.links.origin.1;
        assert_eq!(y0, (line_y + oy + (2.0 * scale).round()).round(), "two units under the baseline ({at})");
        assert!(y1 <= oy + line_top + line_h, "inside the row ({at})");
        // the stretch covers the glyphs of the name, whole pixels: from the left edge of the first to the right edge of the last
        let (gx0, gx1) = {
            let buffer = &a.view.as_ref().expect("a view").buffer;
            let at = buffer.lines[0].text().find("notes.md").expect("the name");
            let glyphs: Vec<_> = buffer.layout_runs().flat_map(|r| r.glyphs.iter().filter(|g| g.start >= at && g.end <= at + 8).map(|g| (g.x, g.x + g.w)).collect::<Vec<_>>()).collect();
            (glyphs.iter().map(|g| g.0).fold(f32::MAX, f32::min), glyphs.iter().map(|g| g.1).fold(0.0, f32::max))
        };
        assert_eq!((x0, x1), ((a.ui.links.origin.0 + gx0).floor(), (a.ui.links.origin.0 + gx1).ceil()), "the underline spans the name ({at})");
        let diff = changed(&buf, &plain, W);
        assert!(!diff.is_empty(), "something was drawn ({at})");
        assert!(diff.iter().all(|&(x, y)| (x as f32) >= x0 && (x as f32) < x1 && (y as f32) >= y0 && (y as f32) < y1), "only the rows of the underline differ ({at})");
        for y in y0 as usize..y1 as usize {
            for x in x0 as usize..x1 as usize {
                assert_eq!(buf[y * W + x], argb, "the underline is in the ink of the text at ({x}, {y}) ({at})");
            }
        }
        // a frame with the pointer away from it draws the plain text again
        a.ui.mouse = (3.0, 3.0);
        let mut after = vec![0u32; W * H];
        assert!(a.compose(&mut after, W, H, 0).is_some());
        assert_eq!(after, plain, "the underline goes with the pointer ({at})");
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_name_that_wraps_has_a_stretch_of_underline_on_each_row_and_a_hover_makes_the_frame_a_full_repaint() {
    let dir = temp("wrap");
    let long = format!("{}.rs", "a".repeat(100));
    std::fs::write(dir.join(&long), "x").expect("a file");
    let text = format!("open {long} please");
    let (mut a, _) = scene(1.0, 700, &[&text], &dir);
    let rows = a.view.as_ref().expect("a view").buffer.layout_runs().filter(|r| r.line_i == 0).count();
    assert!(rows >= 2, "the name does not fit on a row of this window");
    let mut pointed = point(&a, &long[..20]);
    pointed.0 += 1.0;
    a.ui.mouse = pointed;
    let mut buf = vec![0u32; 700 * H];
    assert!(a.compose(&mut buf, 700, H, 0).is_some());
    let rects: Vec<[f32; 4]> = serde_json::from_value(a.ui.links.json()["hover"]["underline"].clone()).expect("rectangles");
    assert_eq!(rects.len(), 3, "the word is on three rows of this window ('open' stands alone on the first): one stretch on each: {rects:?}");
    assert!(rects[0][1] < rects[1][1] && rects[1][1] < rects[2][1], "each stretch is one row lower than the one before: {rects:?}");
    assert!(rects[2][2] < rects[0][2] && rects[0][0] == rects[2][0], "the last row is shorter and every row starts at the left edge: {rects:?}");
    assert!(a.ui.composed.as_ref().expect("a frame").overlay.selection, "a hovered name makes the frame a full repaint, as a selection does");
    a.ui.mouse = (3.0, 3.0);
    assert!(a.compose(&mut buf, 700, H, 1).is_some());
    assert!(!a.ui.composed.as_ref().expect("a frame").overlay.selection, "and nothing under the pointer does not");
    let _ = std::fs::remove_dir_all(dir);
}

fn click(a: &mut App, at: (f64, f64)) {
    a.ui.mouse = at; // (the press knows the pointer's place by itself: no move came before it)
    a.main_mouse_press(at.0 as f32, at.1 as f32);
    a.main_mouse_release();
}

#[test]
fn a_stationary_click_on_a_name_opens_the_viewer_and_any_other_click_keeps_its_meaning() {
    use winit::keyboard::ModifiersState;
    let dir = temp("click");
    std::fs::write(dir.join("notes.md"), "x").expect("a file");
    std::fs::create_dir(dir.join("src")).expect("a folder");
    let (mut a, _) = scene(1.0, W, &[LINE, "plain line one", "plain line two"], &dir);
    let name = point(&a, "notes.md");
    // a click elsewhere selects the line as before and opens nothing
    let then = point(&a, "then");
    click(&mut a, then);
    assert!(a.ui.panel.is_none() && a.ui.sel.is_some(), "no name under the pointer: the click is the line's");
    // Shift keeps the click for the selection, a press that moved is a drag
    a.ui.mods = ModifiersState::SHIFT;
    click(&mut a, name);
    assert!(a.ui.panel.is_none(), "Shift+click extends the selection");
    a.ui.mods = ModifiersState::empty();
    a.ui.mouse = name;
    a.main_mouse_press(name.0 as f32, name.1 as f32);
    a.ui.drag_moved = true;
    a.main_mouse_release();
    assert!(a.ui.panel.is_none(), "a drag is not a click");
    // the click: the viewer is the Files panel on the file, the line that was selected is dropped and the hand goes
    click(&mut a, then);
    assert!(a.ui.sel.is_some(), "a line is selected");
    a.dirty = false;
    click(&mut a, name);
    let panel = a.ui.panel.as_ref().expect("the viewer is open");
    assert!(panel.step == crate::gui::panel::Step::Files && panel.files.is_some(), "the file viewer");
    assert_eq!(a.files_json()["cwd"], dir.to_str().expect("a path"), "the viewer is in the folder of the file, as when a file is clicked in the list");
    assert!(a.ui.sel.is_none() && a.ui.links.hover.is_none() && !a.ui.links.hand && a.dirty, "the selection and the hover are dropped, and the window is drawn again");
    // the viewer's own text is no conversation: a name in it is no link, though the file is there
    let preview = crate::gui::panel::files_ui::FilePreview { path: "/p.txt".into(), name: "p.txt".into(), size: 1, binary: false, truncated: false, lines: vec!["see notes.md here".into()], top: 0 };
    a.ui.panel.as_mut().and_then(|p| p.files.as_mut()).expect("the viewer").preview = Some(preview);
    let mut shown = vec![0u32; W * H];
    assert!(a.compose(&mut shown, W, H, 0).is_some());
    assert_eq!(over(&mut a, "notes.md"), None, "a panel has no links");
    // a folder is a click as well (the list is the host's to show: the viewer says so in words when it cannot)
    a.panel_close();
    let (mut b, _) = scene(1.0, W, &[LINE], &dir);
    let folder = point(&b, "./src");
    click(&mut b, folder);
    assert!(b.ui.sel.is_none() && b.ui.links.hover.is_none(), "the click took the folder");
    assert!(b.ui.panel.is_none(), "the list is the host's to make: the host of this test is nobody, so the viewer asked for the list and closed itself (a file would stay open, as above)");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_viewer_starts_at_the_line_after_the_name_and_never_past_the_last_page() {
    let dir = temp("line");
    let (mut a, _) = scene(1.0, W, &[LINE], &dir);
    a.files_open_preview_from(None, "/", "/nope.txt", false);
    let lines: Vec<String> = (0..500).map(|i| format!("line {i}")).collect();
    let preview = crate::gui::panel::files_ui::FilePreview { path: "/p.txt".into(), name: "p.txt".into(), size: 1, binary: false, truncated: false, lines, top: 0 };
    a.ui.panel.as_mut().and_then(|p| p.files.as_mut()).expect("the viewer").preview = Some(preview);
    a.ui.panel_rows = 40;
    let page = a.files_json()["page_size"].as_u64().expect("the page size of the viewer") as usize;
    let top = |a: &App| a.ui.panel.as_ref().and_then(|p| p.files.as_ref()).and_then(|e| e.preview.as_ref()).map(|p| p.top);
    a.files_preview_to_line(12);
    assert_eq!(top(&a), Some(11), "line 12 is the twelfth row: the viewer's top is 11");
    a.files_preview_to_line(1);
    assert_eq!(top(&a), Some(0));
    a.files_preview_to_line(0);
    assert_eq!(top(&a), Some(0), "a line 0 is the first");
    a.files_preview_to_line(9_999);
    let last = top(&a).expect("a top");
    assert_eq!(last, 500 - page, "never past the last page: the page that ends the file starts at {last} (the viewer's page is {page} rows)");
    a.files_preview_to_line(last as u32 + 1);
    assert_eq!(top(&a), Some(last), "the first row of the last page is the last top");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_edges_of_a_row_and_of_a_glyph_belong_to_the_row_below_and_to_the_glyph_after() {
    let dir = temp("edges");
    std::fs::write(dir.join("notes.md"), "x").expect("a file");
    let (a, _) = scene(1.0, W, &["plain first line here", "notes.md second"], &dir);
    let view = a.view.as_ref().expect("a view");
    let buffer = &view.buffer;
    let (ox, oy) = a.ui.links.origin;
    let at = |text: &str| buffer.lines.iter().position(|l| l.text().starts_with(text)).expect(text);
    let (plain, named) = (at("plain"), at("notes.md"));
    let run = buffer.layout_runs().find(|r| r.line_i == named).expect("the row of the name");
    let (top, bottom) = (run.line_top, run.line_top + run.line_height);
    let glyph = |start: usize| run.glyphs.iter().find(|g| g.start == start).expect("a glyph");
    let (first, last, space) = (glyph(0), glyph(7), glyph(8));
    // a row starts at its top and ends before its bottom; a glyph starts at its left edge and ends before its right edge
    assert_eq!(under(buffer, (first.x + 1.0, top)), Some((named, 0)), "the top edge is in the row");
    assert_ne!(under(buffer, (first.x + 1.0, bottom)).map(|u| u.0), Some(named), "the bottom edge is the row below's: {:?}", under(buffer, (first.x + 1.0, bottom)));
    assert_eq!(under(buffer, (first.x, top + 1.0)), Some((named, 0)), "the left edge of a glyph is in it");
    assert_eq!(under(buffer, (last.x + last.w - 0.5, top + 1.0)), Some((named, 7)), "inside the last glyph of the name");
    assert_eq!(under(buffer, (last.x + last.w, top + 1.0)), Some((named, 8)), "the right edge of the last glyph is the space's left edge: {:?}", (last.x, last.w, space.x));
    // the rows of the conversation that count (hit_lines) are the same: the top edge is in, the bottom edge is out
    let ctx = Some(Ctx { cwd: dir.to_string_lossy().into_owned(), home: String::new() });
    let pointer = |y: f32| ((ox + first.x + 1.0) as f64, y as f64);
    let (y_top, y_bottom) = (oy + top, oy + bottom);
    let mut st = State::default();
    st.frame(ctx.clone(), (ox, oy), buffer, &[(y_top, y_bottom, named, 0)], pointer(y_top));
    assert_eq!(st.hover.as_ref().map(|h| h.name.as_str()), Some("notes.md"), "the pointer on the top edge of the row is in the row");
    st.frame(ctx.clone(), (ox, oy), buffer, &[(y_top - 20.0, y_top, plain, 0)], pointer(y_top));
    assert!(st.hover.is_none(), "the pointer on the bottom edge of the row above is not in it");
    st.frame(ctx.clone(), (ox, oy), buffer, &[(y_top, y_bottom, named, 0)], pointer(y_top));
    // and the first character of a name is the name, as the last one is
    for x in [first.x + 0.5, last.x + 0.5] {
        st.frame(ctx.clone(), (ox, oy), buffer, &[(y_top, y_bottom, named, 0)], ((ox + x) as f64, (y_top + 2.0) as f64));
        assert_eq!(st.hover.as_ref().map(|h| h.name.as_str()), Some("notes.md"), "at {x}");
    }
    st.frame(ctx, (ox, oy), buffer, &[(y_top, y_bottom, named, 0)], ((ox + space.x + 0.5) as f64, (y_top + 2.0) as f64));
    assert!(st.hover.is_none(), "the space after the name is not the name");
}

#[test]
fn the_underline_and_the_names_of_the_dump_are_of_the_line_that_holds_the_name_and_of_its_first_row() {
    let dir = temp("lines");
    std::fs::write(dir.join("notes.md"), "x").expect("a file");
    std::fs::write(dir.join("b.rs"), "x").expect("a file");
    // the name is on the second line and the third has the same bytes: the stretch is on the second line's row only
    let (mut a, _) = scene(1.0, W, &["plain first line", "see notes.md", "see notes.md"], &dir);
    a.ui.mouse = point(&a, "notes.md");
    let mut buf = vec![0u32; W * H];
    assert!(a.compose(&mut buf, W, H, 0).is_some());
    let rects: Vec<[f32; 4]> = serde_json::from_value(a.ui.links.json()["hover"]["underline"].clone()).expect("rectangles");
    let buffer = &a.view.as_ref().expect("a view").buffer;
    let second = buffer.layout_runs().find(|r| buffer.lines[r.line_i].text().starts_with("see notes.md")).expect("the second line");
    assert_eq!(rects.len(), 1, "one stretch: {rects:?}");
    assert_eq!(rects[0][1], (second.line_y + a.ui.links.origin.1 + 2.0).round(), "on the row of the line that holds the name: {rects:?}");
    // a name that wraps: the dump's rectangle is its first row's, from the top of the row to its bottom
    let long = format!("{}.rs", "a".repeat(100));
    std::fs::write(dir.join(&long), "x").expect("a file");
    let text = format!("open {long} and b.rs");
    let (mut w, _) = scene(1.0, 700, &[&text], &dir);
    w.ui.mouse = point(&w, &long[..20]);
    w.ui.mouse.0 += 1.0;
    let mut buf = vec![0u32; 700 * H];
    assert!(w.compose(&mut buf, 700, H, 0).is_some());
    let rects: Vec<[f32; 4]> = serde_json::from_value(w.ui.links.json()["hover"]["underline"].clone()).expect("rectangles");
    assert_eq!(rects.len(), 3, "the long name is on three rows: {rects:?}");
    let names = w.file_link_json()["names"].clone();
    let rect: Vec<f32> = serde_json::from_value(names[0]["rect"].clone()).expect("a rectangle");
    let row = w.view.as_ref().expect("a view").buffer.layout_runs().filter(|r| r.line_i == 0).map(|r| (r.line_top, r.line_height)).collect::<Vec<_>>();
    let oy = w.ui.links.origin.1;
    assert_eq!((rect[1], rect[3]), (oy + row[1].0, oy + row[1].0 + row[1].1), "the first row of the name (the second row of the line: 'open' stands alone on the first): {rect:?} {row:?}");
    assert_eq!(rect[0].floor(), rects[0][0], "from the left edge of its first glyph");
    assert_eq!(names.as_array().map(Vec::len), Some(2), "both names are in the dump: {names}");
    // the stretch of a later hover replaces the one before it
    w.ui.mouse = point(&w, "b.rs");
    assert!(w.compose(&mut buf, 700, H, 1).is_some());
    let rects: Vec<[f32; 4]> = serde_json::from_value(w.ui.links.json()["hover"]["underline"].clone()).expect("rectangles");
    assert_eq!(rects.len(), 1, "one stretch for the short name, not the three of the one before: {rects:?}");
    // a name never has a stretch of no width, whatever the window's width leaves of it on its last row
    let mut single = 0;
    for width in (420..=700).step_by(3) {
        let (m, _) = scene(1.0, width, &[&text], &dir);
        let buffer = &m.view.as_ref().expect("a view").buffer;
        let at = buffer.lines[0].text().find(&long).expect("the name");
        for sp in spans(buffer, 0, at, at + long.len()) {
            assert!(sp.x1 - sp.x0 >= 3.0, "a stretch as wide as a glyph at least at width {width}: {} {}", sp.x0, sp.x1);
        }
        single += buffer.layout_runs().filter(|r| r.line_i == 0 && r.glyphs.iter().filter(|g| g.start >= at && g.end <= at + long.len()).count() == 1).count();
    }
    assert!(single > 0, "the widths tried leave a row of one glyph at the end of the name");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_underline_is_in_the_ink_it_is_given_in_each_channel_and_toned_for_the_light_theme() {
    use crate::gui::theme::{set_mode, tone_with, Mode};
    let dir = temp("ink");
    std::fs::write(dir.join("notes.md"), "x").expect("a file");
    let (mut a, _) = scene(1.0, W, &[LINE], &dir);
    a.ui.mouse = point(&a, "notes.md");
    let _ = a.file_link_moved();
    let view = a.view.as_ref().expect("a view");
    let ink = Color::rgb(0x10, 0x20, 0x30);
    let mut paint = |mode: Mode| {
        set_mode(mode);
        let mut buf = vec![0u32; W * H];
        a.ui.links.paint(&view.buffer, &mut buf, W, H, 1.0, ink);
        let rects: Vec<[f32; 4]> = serde_json::from_value(a.ui.links.json()["hover"]["underline"].clone()).expect("rectangles");
        buf[rects[0][1] as usize * W + rects[0][0] as usize + 1]
    };
    let (dark, light) = (paint(Mode::Dark), paint(Mode::Light));
    set_mode(Mode::Dark);
    assert_eq!(dark, 0xFF10_2030, "opaque, in the ink given: red, green and blue each in its own place");
    assert_eq!(light, 0xFF00_0000 | (tone_with(Mode::Light, 0xFF10_2030) & 0x00FF_FFFF), "the light theme tones it as it tones the text");
    assert_ne!(light, dark, "and it is another colour");
    let _ = std::fs::remove_dir_all(dir);
}
