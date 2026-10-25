//! The oracle of the scroll step: two windows get the same events, one with the step and one with `--no-scroll-step` (every frame a full repaint),
//! and after EVERY step of a script their pixels must be equal. The buffer of the first keeps the last frame like softbuffer's does (age 1),
//! and starts as garbage, so a row that is neither kept nor drawn shows.

use std::collections::VecDeque;
use std::os::unix::net::UnixStream;

use cosmic_text::{Buffer, Metrics, SwashCache};

use super::scroll::Cause;
use super::ui::{Row, UiState};
use super::{fonts, App, View, BACKGROUND, FONT_PX, LINE_HEIGHT};

const GARBAGE: u32 = 0xFF12_3456;

fn app(scale: f32, no_scroll_step: bool) -> App {
    let (sock, peer) = UnixStream::pair().expect("a socket pair");
    std::mem::forget(peer); // the window may write to the host; nobody reads
    let mut app = App {
        stale_pending: false, stale_empty: false, pending_inputs: Default::default(), view_id: String::new(), generation: 1, drafts: Default::default(), sent_count: 0,
        input: String::new(), preedit: String::new(), dirty: false, quit: false, meta: Vec::new(), pending_show: None, print_status: false,
        window: None, context: None, view: None, font_system: None, sock, socket: String::new(), serial: 0, follow: false, updating: false, ended_meanwhile: false,
        session: "session-s0".into(), cursor: -1, dups: 0, gaps: 0, resyncs: 0, subscribed: false,
        ui: UiState { enabled: true, markers: false, menu: true, faces: fonts::Faces::new(false), no_scroll_step, scale, fonts_gen: 1, ..Default::default() }, recent_draws: VecDeque::new(),
    };
    let mut fs = fonts::system(false);
    let buffer = Buffer::new(&mut fs, Metrics::new(FONT_PX * scale, LINE_HEIGHT as f32 * scale));
    app.view = Some(View { lines: VecDeque::new(), surface: None, buffer, swash: SwashCache::new(), session_rgb: 0xFF00_0000, seq_rgb: 0xFF00_0000 });
    app.font_system = Some(fs);
    app
}

/// One window with its own buffer.
struct Side {
    app: App,
    buf: Vec<u32>,
    age: u8,
    size: (usize, usize),
}

impl Side {
    fn new(size: (usize, usize), scale: f32, no_scroll_step: bool) -> Side {
        Side { app: app(scale, no_scroll_step), buf: vec![GARBAGE; size.0 * size.1], age: 0, size }
    }

    fn causes(&self) -> [u64; Cause::ALL.len()] {
        self.app.ui.stats.causes
    }

    /// Draws a frame; the cause is the one counter that went up.
    fn frame(&mut self) -> Cause {
        let before = self.causes();
        let (width, height) = self.size;
        assert!(self.app.compose(&mut self.buf, width, height, self.age).is_some(), "nothing was drawn");
        self.age = 1; // a buffer that was drawn into holds the last frame
        let after = self.causes();
        let up: Vec<usize> = (0..before.len()).filter(|&i| after[i] != before[i]).collect();
        assert_eq!(up.len(), 1, "one frame, one cause");
        Cause::ALL[up[0]]
    }
}

/// The two windows and the script.
struct Pair {
    on: Side,
    off: Side,
    seq: i64,
    shifts: usize,
    steps: usize,
}

impl Pair {
    fn new(size: (usize, usize), scale: f32) -> Pair {
        Pair { on: Side::new(size, scale, false), off: Side::new(size, scale, true), seq: 0, shifts: 0, steps: 0 }
    }

    fn both(&mut self, f: impl Fn(&mut App)) {
        f(&mut self.on.app);
        f(&mut self.off.app);
    }

    fn push(&mut self, texts: &[String]) {
        for text in texts {
            self.seq += 1;
            let seq = self.seq;
            self.both(|app| app.push_line(seq, text.clone()));
        }
    }

    /// The backend hands out a buffer that does not hold the last frame: new (age 0) or the one before (age 2).
    fn other_buffer(&mut self, age: u8) {
        for side in [&mut self.on, &mut self.off] {
            side.buf = vec![GARBAGE; side.size.0 * side.size.1];
            side.age = age;
        }
    }

    fn resize(&mut self, size: (usize, usize), age_after: u8) {
        for side in [&mut self.on, &mut self.off] {
            side.size = size;
            side.buf = vec![GARBAGE; size.0 * size.1];
            side.age = age_after;
        }
    }

    /// Draws both and compares every pixel; returns the cause of the first.
    fn step(&mut self, what: &str) -> Cause {
        let cause = self.on.frame();
        let full = self.off.frame();
        self.steps += 1;
        assert_eq!(full, Cause::Disabled);
        if cause == Cause::Shift {
            self.shifts += 1;
        }
        if self.on.buf != self.off.buf {
            let (width, _) = self.on.size;
            let at = self.on.buf.iter().zip(&self.off.buf).position(|(a, b)| a != b).unwrap();
            let rows = self.on.buf.chunks(width).zip(self.off.buf.chunks(width)).filter(|(a, b)| a != b).count();
            panic!("step '{what}' (cause {cause:?}): the pixels differ, first at x={} y={}, {rows} rows differ", at % width, at / width);
        }
        cause
    }

    /// A step that must be the cheap one (or, at a scale whose row height is not whole pixels, a full repaint that says so).
    fn shift(&mut self, what: &str) {
        let cause = self.step(what);
        let scale = self.on.app.ui.scale;
        if scale == 1.0 || scale == 2.0 {
            assert_eq!(cause, Cause::Shift, "step '{what}'");
        } else {
            // rows of 22.5 pixels: a shift by an odd number of rows, or lines whose tops do not all move by the same whole pixels, is declined
            assert!(matches!(cause, Cause::Shift | Cause::RowHeight | Cause::LineChanged), "step '{what}': {cause:?}");
        }
    }

    fn full(&mut self, what: &str, expected: Cause) {
        assert_eq!(self.step(what), expected, "step '{what}'");
    }
}

/// Lines of every kind: plain, wrapped, Korean, emoji, stacked accents that reach rows above their own, Hebrew, brackets and descenders.
fn text(n: i64) -> String {
    match n % 12 {
        0 => format!("{n}: plain ASCII line with a few words and a number {}", n * 7),
        1 => format!("{n}: fn main() {{ let g = [1, 2, 3]; println!(\"{{:?}}\", g); }} // gjpqy"),
        2 => format!("{n}: 파일을 읽었습니다. 안녕하세요, 테스트 줄입니다 {}", n * 3),
        3 => format!("{n}: 🙂 done ✓ → next ⚠ 漢字かな"),
        4 => format!("{n}: a\u{301}\u{302}\u{303}\u{304} stacked e\u{308}\u{301} accents o\u{323}\u{302}\u{301}"),
        5 => format!("{n}: x\u{300}\u{301}\u{302}\u{303}\u{304}\u{305}\u{306}\u{307}\u{308}\u{309}\u{30A}\u{30B}\u{30C} zalgo Ṽ̃̂ Ǻ y\u{316}\u{317}\u{323}\u{324}\u{325}\u{329}\u{32D}\u{330}\u{331}\u{332}\u{333} below"),
        6 => format!("{n}: wrapped line {}", "that goes on for a long while and wraps around the right edge of the window ".repeat(3)),
        7 => format!("{n}: שלום עולם mixed with English and 한국어"),
        8 => format!("{n}: {{ [ ( | ) ] }} ƒ ǰ Ý ý ɡ ʝ ç ȷ {}", n),
        9 => format!("{n}: ───── box ┌──┐ │ │ └──┘ ▓▒░ ♥ ★"),
        10 => format!("{n}: a very long hangul line {}", "가나다라마바사아자차카타파하 ".repeat(8)),
        _ => format!("{n}: short"),
    }
}

fn lines(from: i64, count: usize) -> Vec<String> {
    (from..from + count as i64).map(text).collect()
}

fn script(scale: f32) {
    // the window has about 40 rows at every scale, so that a burst of twelve lines leaves old lines on the screen
    let size = if scale == 1.0 { (1000, 620) } else if scale == 2.0 { (1500, 1400) } else { (1100, 800) };
    let mut p = Pair::new(size, scale);
    p.full("the first frame", Cause::First);
    p.push(&lines(0, 60));
    p.full("sixty lines at once", Cause::NothingInCommon);

    // tail-follow in bursts of every size, with every kind of line coming in at the bottom and going out at the top
    let mut next = 60;
    for burst in [1, 1, 3, 1, 12, 2, 1, 5, 1, 1] {
        p.push(&lines(next, burst));
        next += burst as i64;
        p.shift(&format!("burst of {burst} (lines up to {next})"));
    }
    for _ in 0..30 {
        p.push(&lines(next, 1));
        next += 1;
        p.shift(&format!("one line ({next})"));
    }

    // a menu opens in the middle of a burst, output goes on behind it, it closes
    p.push(&lines(next, 2));
    next += 2;
    p.both(|app| app.ui.menu_open = Some(1));
    p.full("menu opens", Cause::Menu);
    p.push(&lines(next, 3));
    next += 3;
    p.full("output behind the open menu", Cause::Menu);
    p.both(|app| app.ui.menu_sel = 1);
    p.full("menu moves", Cause::Menu);
    p.both(|app| app.ui.menu_open = None);
    p.full("menu closes", Cause::Menu);
    p.push(&lines(next, 1));
    next += 1;
    p.shift("first tail-follow after the menu");

    // a selection while output flows
    let len = p.on.app.view.as_ref().unwrap().lines.len();
    p.both(|app| app.ui.sel = Some((len - 9, len - 4)));
    p.full("selection made", Cause::Selection);
    p.push(&lines(next, 2));
    next += 2;
    p.full("output while selected", Cause::Selection);
    p.both(|app| app.ui.sel = None);
    p.full("selection cleared", Cause::Selection);
    p.push(&lines(next, 2));
    next += 2;
    p.shift("tail-follow after the selection");

    // scroll back and return
    p.both(|app| app.ui.scroll = 7);
    p.full("scrolled back", Cause::Scrollback);
    p.push(&lines(next, 3));
    next += 3;
    p.full("output while reading older lines", Cause::Scrollback);
    p.both(|app| app.ui.scroll = 0);
    p.full("back at the bottom", Cause::Scrollback);
    p.push(&lines(next, 1));
    next += 1;
    p.shift("tail-follow after the return");

    // a composition in progress, and typing
    p.both(|app| app.preedit = "ㅎ".into());
    p.full("pre-edit", Cause::Preedit);
    p.push(&lines(next, 1));
    next += 1;
    p.full("output during pre-edit", Cause::Preedit);
    p.both(|app| app.preedit.clear());
    p.full("pre-edit ends", Cause::Preedit);
    for word in ["h", "e", "llo wor", "ld \u{d55c}\u{ae00}"] {
        p.both(|app| app.input.push_str(word));
        p.shift(&format!("typing '{word}' with no new line"));
    }
    p.push(&lines(next, 2));
    next += 2;
    p.shift("output while a draft is typed");

    // marks stacked above the first letters of the draft reach into the rows of the last line; marks below the header reach into the first lines
    p.both(|app| app.input = "e\u{301}\u{302}\u{303}\u{304}\u{305}\u{306}\u{307}\u{308}\u{309} o\u{323}\u{324}\u{325}".into());
    p.push(&lines(next, 2));
    next += 2;
    p.shift("stacked marks in the draft");
    p.push(&lines(next, 1));
    next += 1;
    p.shift("stacked marks in the draft, one more line");
    p.both(|app| app.input.clear());
    p.push(&lines(next, 1));
    next += 1;
    p.shift("the draft is gone");
    p.both(|app| app.ui.flash = Some("notice x\u{316}\u{317}\u{323}\u{324}\u{325}\u{329}\u{32D}\u{330}\u{331}\u{332}\u{333}\u{316}\u{317}\u{323}\u{324}\u{325}".into()));
    p.push(&lines(next, 2));
    next += 2;
    let cause = p.step("a header with marks below it");
    println!("scale {scale}: a header with marks below it: {cause:?}");
    assert!(matches!(cause, Cause::Shift | Cause::Overhang | Cause::RowHeight), "{cause:?}");
    p.push(&lines(next, 1));
    next += 1;
    let cause = p.step("one more line under such a header");
    assert!(matches!(cause, Cause::Shift | Cause::Overhang | Cause::RowHeight), "{cause:?}");
    p.both(|app| app.ui.flash = None);
    p.push(&lines(next, 1));
    next += 1;
    p.step("the header is plain again");

    // the header changes, and wraps to two rows and back
    p.both(|app| app.ui.flash = Some("a notice".into()));
    p.push(&lines(next, 1));
    next += 1;
    p.shift("header changes");
    p.both(|app| app.ui.flash = Some("a very long notice that cannot fit in one row of the window and so wraps around ".repeat(4)));
    p.push(&lines(next, 1));
    next += 1;
    p.shift("header wraps to more rows");
    p.both(|app| app.ui.flash = None);
    p.push(&lines(next, 2));
    next += 2;
    p.shift("header back to one row");

    // the session list changes beside the transcript
    p.both(|app| {
        for (n, state) in [("s1", "running"), ("s2", "idle"), ("s3", "waiting")] {
            app.ui.rows.push(Row { session: format!("session-{n}"), provider: "codex".into(), account: "a1".into(), state: state.into(), model: "m".into(), ..Default::default() });
        }
        app.session = "session-s2".into();
    });
    p.push(&lines(next, 2));
    next += 2;
    p.shift("the list gets rows");
    p.both(|app| app.ui.rows[0].state = "exited".into());
    p.push(&lines(next, 1));
    next += 1;
    p.shift("a row of the list changes");

    // a line that is not new changes
    p.both(|app| {
        let view = app.view.as_mut().unwrap();
        let at = view.lines.len() - 6;
        view.lines[at].1.push_str(" (edited)");
    });
    p.full("an old line changes", Cause::LineChanged);
    p.push(&lines(next, 1));
    next += 1;
    p.shift("tail-follow after the edit");

    // a window larger than the screenful of new text
    p.push(&lines(next, 200));
    next += 200;
    p.full("two hundred lines at once", Cause::NothingInCommon);
    p.push(&lines(next, 2));
    next += 2;
    p.shift("tail-follow after the flood");

    // a resize: the backend says the buffer is new (age 0); then one that wrongly says the old frame is still there
    p.resize((size.0 - 120, size.1 - 80), 0);
    p.full("resized", Cause::BufferAge);
    p.push(&lines(next, 2));
    next += 2;
    p.shift("tail-follow after the resize");
    p.resize((size.0 - 60, size.1 - 40), 1);
    p.full("resized, with a wrong age", Cause::Size);
    p.push(&lines(next, 3));
    next += 3;
    p.shift("tail-follow after the second resize");

    // a buffer that is not the last frame's
    p.other_buffer(0);
    p.full("a new buffer", Cause::BufferAge);
    p.push(&lines(next, 2));
    next += 2;
    p.shift("tail-follow after the new buffer");
    p.other_buffer(2);
    p.full("the buffer of two frames ago", Cause::BufferAge);
    p.push(&lines(next, 1));
    next += 1;
    p.shift("tail-follow after the old buffer");

    // the scale changes: the lines are laid out again at another height
    let target = if scale == 2.0 { 1.0 } else { 2.0 };
    p.both(|app| {
        app.ui.scale = target;
        let mut fs = fonts::system(false);
        let buffer = Buffer::new(&mut fs, Metrics::new(FONT_PX * target, LINE_HEIGHT as f32 * target));
        let view = app.view.as_mut().unwrap();
        view.buffer = buffer;
        app.font_system = Some(fs);
        app.ui.fonts_gen += 1;
    });
    p.full("scale and fonts change", Cause::Scale);
    p.push(&lines(next, 2));
    // the window is small for the larger scale: two lines may be all that there is room for
    let cause = p.step("tail-follow after the scale change");
    assert!(matches!(cause, Cause::Shift | Cause::RowHeight | Cause::NothingInCommon), "{cause:?}");

    // at 1.25 a row is 22.5 pixels: only a shift by an even number of rows is whole pixels, the others are repainted and say so
    let least = if scale == 1.0 || scale == 2.0 { 50 } else { 15 };
    println!("scale {scale}: {} steps, {} shifts", p.steps, p.shifts);
    assert!(p.steps >= 85, "the script has {} steps", p.steps);
    assert!(p.shifts >= least, "the shift must have been taken often: {}", p.shifts);
    assert!(p.on.app.ui.stats.kept_rows > 0);
    assert_eq!(p.off.app.ui.stats.kept_rows, 0);
    let _ = BACKGROUND;
}

#[test]
fn a_scripted_session_draws_the_same_pixels_with_the_scroll_step_as_with_a_full_repaint_at_scale_1() {
    script(1.0);
}

#[test]
fn a_scripted_session_draws_the_same_pixels_with_the_scroll_step_as_with_a_full_repaint_at_scale_2() {
    script(2.0);
}

#[test]
fn a_scripted_session_draws_the_same_pixels_with_the_scroll_step_as_with_a_full_repaint_at_scale_one_and_a_quarter() {
    script(1.25);
}
