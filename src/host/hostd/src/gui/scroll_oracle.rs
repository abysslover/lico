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

pub(super) fn app(scale: f32, no_scroll_step: bool) -> App {
    let (sock, peer) = UnixStream::pair().expect("a socket pair");
    std::mem::forget(peer); // the window may write to the host; nobody reads
    let mut app = App {
        stale_pending: false, stale_empty: false, pending_inputs: Default::default(), view_id: String::new(), generation: 1, sent_count: 0,
        ed: Default::default(), preedit: String::new(), dirty: false, quit: false, meta: Vec::new(), pending_show: None, print_status: false,
        window: None, context: None, view: None, font_system: None, sock, socket: String::new(), serial: 0, follow: false, updating: false, ended_meanwhile: false,
        session: "session-s0".into(), cursor: -1, dups: 0, gaps: 0, resyncs: 0, subscribed: false,
        ui: UiState { enabled: true, markers: false, workbench: true, menu: true, faces: fonts::Faces::new(false), no_scroll_step, scale, fonts_gen: 1, ..Default::default() }, recent_draws: VecDeque::new(),
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

thread_local! {
    /// The windows this thread makes are in the light look (`M24-P11d`: `theme_oracle.rs` runs the oracle again in it).
    static LIGHT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Runs `f` with every window of this thread made in the light look; the thread is dark again after it, whether `f` ends or panics.
fn in_light(f: impl FnOnce()) {
    struct Dark;
    impl Drop for Dark {
        fn drop(&mut self) {
            LIGHT.with(|l| l.set(false));
        }
    }
    LIGHT.with(|l| l.set(true));
    let _dark = Dark;
    f();
}

impl Side {
    fn new(size: (usize, usize), scale: f32, no_scroll_step: bool) -> Side {
        let mut app = app(scale, no_scroll_step);
        if LIGHT.with(std::cell::Cell::get) {
            app.ui.theme.choice = super::theme::Choice::Light;
        }
        Side { app, buf: vec![GARBAGE; size.0 * size.1], age: 0, size }
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
    // the window has about 40 rows at every scale, so that a burst of twelve lines leaves old lines on the screen (M24-P09: the toolbar is 36 px taller at scale 2, so the window is 40 px taller)
    let size = if scale == 1.0 { (1000, 620) } else if scale == 2.0 { (1500, 1440) } else { (1100, 800) };
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
        p.both(|app| {
            app.ed.put(word);
        });
        p.shift(&format!("typing '{word}' with no new line"));
    }
    p.push(&lines(next, 2));
    next += 2;
    p.shift("output while a draft is typed");

    // marks stacked above the first letters of the draft reach into the rows of the last line; marks below the header reach into the first lines
    p.both(|app| app.ed.set("e\u{301}\u{302}\u{303}\u{304}\u{305}\u{306}\u{307}\u{308}\u{309} o\u{323}\u{324}\u{325}".into()));
    p.push(&lines(next, 2));
    next += 2;
    p.shift("stacked marks in the draft");
    p.push(&lines(next, 1));
    next += 1;
    p.shift("stacked marks in the draft, one more line");
    p.both(|app| {
        app.ed.clear();
    });
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

    // the clock of a working session: a status line appears under the last record, counts on without a new line, and goes with the turn
    p.both(|app| {
        let row = app.ui.rows.iter_mut().find(|r| r.session == "session-s2").unwrap();
        row.state = "running".into();
        row.activity = "working".into();
        row.busy_since = 1000;
        row.last_event_at = 1000;
        app.ui.fixed_now = Some(1003);
    });
    p.push(&lines(next, 1));
    next += 1;
    p.shift("a turn opens: the status line appears");
    assert_eq!(p.on.app.status_line(false).as_deref(), Some("… working (3 s)"), "the segment is about a status line that is there");
    for (second, burst) in [(1004, 0), (1005, 0), (1006, 1), (1007, 3), (1008, 0), (1066, 1), (1067, 0)] {
        p.both(|app| app.ui.fixed_now = Some(second));
        p.push(&lines(next, burst));
        next += burst as i64;
        p.shift(&format!("the clock reads {second} with {burst} new lines"));
        assert_eq!(p.on.app.status_line(false), Some(format!("… working ({})", super::ui::clock_text(second - 1000))));
    }
    p.both(|app| {
        let row = app.ui.rows.iter_mut().find(|r| r.session == "session-s2").unwrap();
        row.stalled = true;
        app.ui.fixed_now = Some(1100);
    });
    p.shift("the host says the session is silent: the status line is longer");
    assert_eq!(p.on.app.status_line(false).as_deref(), Some("… no output for 1 m 40 s (working 1 m 40 s)"));
    p.push(&lines(next, 2));
    next += 2;
    p.shift("lines arrive under a silent status");
    p.both(|app| {
        let row = app.ui.rows.iter_mut().find(|r| r.session == "session-s2").unwrap();
        row.stalled = false;
        row.last_event_at = 1100;
        app.ui.fixed_now = Some(1101);
    });
    p.shift("the silence ends");
    p.both(|app| app.ui.scroll = 4);
    p.full("scrolled back while a turn works", Cause::Scrollback);
    p.both(|app| app.ui.scroll = 0);
    p.full("back at the bottom while a turn works", Cause::Scrollback);
    p.both(|app| {
        let row = app.ui.rows.iter_mut().find(|r| r.session == "session-s2").unwrap();
        row.activity = "idle".into();
        row.busy_since = 0;
        row.stalled = false;
    });
    p.push(&lines(next, 1));
    next += 1;
    p.shift("the turn ends: the status line goes");
    assert_eq!(p.on.app.status_line(false), None);
    p.both(|app| app.ui.fixed_now = None);

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

#[test]
fn user_prompt_backgrounds_role_colors_and_hidden_control_events_match_full_repaint_across_scales() {
    for scale in [1.0, 1.25, 2.0] {
        let mut p = Pair::new((1000, 620), scale);
        p.full("first", Cause::First);
        for turn in 0..16 {
            p.push(&[
                format!(">> user prompt {turn} 한글 🙂"),
                "[turn started]".into(),
                "[session started] model=claude-sonnet-4-5".into(),
                format!("Agent answer {turn} with details and wrapped prose."),
                "$ cargo test --quiet".into(),
                "> test result: ok.".into(),
                "[exit 0]".into(),
                if turn % 5 == 4 { "[error] transient warning".into() } else { "[turn completed]".into() },
            ]);
            p.step(&format!("turn {turn}"));
        }
        p.both(|app| app.toggle_show_events());
        p.step("toggle events shown");
        p.push(&[">> follow-up prompt".into(), "[turn started]".into(), "Reply while events shown".into(), "[turn completed]".into()]);
        p.step("turn while events shown");
        p.both(|app| app.toggle_show_events());
        p.step("toggle events hidden");
    }
}

#[test]
fn command_execution_boxes_exit_errors_mcp_cards_and_peer_banners_match_full_repaint_across_scales() {
    for scale in [1.0, 1.25, 2.0] {
        let mut p = Pair::new((1040, 660), scale);
        p.full("first", Cause::First);
        for turn in 0..14 {
            p.push(&[
                format!(">> [from session 'peer' (s123456) | 14:{turn:02} | message | 24 B]"),
                format!(">> cross-session check {turn}"),
                "$ /bin/bash -lc 'lico session list'".into(),
                "stdout row 1: checking state".into(),
                if turn % 2 == 0 { "[exit 1]".into() } else { "[exit 0]".into() },
                "[tool] mcp__lico__sessions".into(),
                "> {\"sessions\":[]}".into(),
                "[tool] mcp__lico__send".into(),
                "[tool error]".into(),
                "> receiver busy".into(),
                "[patch] 1 change(s)".into(),
                format!("Agent prose after command card {turn} 한글 요약."),
            ]);
            p.step(&format!("boxed turn {turn}"));
        }
        p.push(&["$ echo incremental".into(), "line inside box".into(), "[exit 1]".into(), "plain prose after box".into()]);
        p.shift("incremental shift with boxed command card and exit 1");
        let last_cmd_raw = p.on.app.view.as_ref().unwrap().lines.len() - 4;
        p.both(|app| { assert!(app.try_toggle_tool_card(last_cmd_raw)); });
        p.step("expand latest command card via click toggle");
        p.both(|app| { assert!(app.try_toggle_tool_card(last_cmd_raw)); });
        p.step("collapse latest command card via click toggle");
        p.both(|app| app.toggle_show_events());
        p.step("events shown with exit 0 inside command cards");
        p.both(|app| app.toggle_show_events());
        p.step("events hidden again");
    }
}

/// The seconds of a run for the lines `from` on: a fixed day of 2026 and a second per three lines.
fn runs_for(from: i64, to: i64) -> Vec<(i64, u64)> {
    (from..to).step_by(3).map(|seq| (seq, 1_795_000_000 + seq as u64 * 7)).collect()
}

fn know(p: &mut Pair, to: i64) {
    p.both(|app| app.ui.times = super::times::Times::seeded("session-s0", 1, to, &runs_for(1, to)));
}

/// What the last frame drew in the gutter, as the dump says it.
fn drawn(side: &Side) -> (u64, usize) {
    let dump = side.app.ui.times.json();
    (dump["gutter"].as_u64().unwrap_or(0), dump["drawn"].as_array().map_or(0, Vec::len))
}

/// A colour of the dark look as this window paints it (the helpers below compare pixels with the constants of the window, in whichever look it is in).
fn look(side: &Side, color: u32) -> u32 {
    super::theme::tone_with(side.app.ui.theme.mode(), color)
}

/// Whether the gutter of every line on the screen is the plain background.
fn gutter_plain(side: &Side) -> bool {
    let gutter = drawn(side).0 as usize;
    let (width, height) = side.size;
    side.app.ui.hit_lines.iter().all(|&(y0, y1, _, _)| {
        (y0.floor().max(0.0) as usize..(y1.ceil() as usize).min(height)).all(|y| side.buf[y * width + width - gutter..(y + 1) * width].iter().all(|&px| px == look(side, BACKGROUND)))
    })
}

/// The pixel at the right end of the band of every line of the person and of every reply on the screen (just before the gutter): how many of each were looked at; none may be another colour.
fn band_ends(side: &Side) -> (usize, usize) {
    use super::blocks::{kind_of, Kind};
    use super::transcript::{RESPONSE_BG, ROLE_USER_BG, USER_BG};
    let (width, gutter) = (side.size.0, drawn(side).0 as usize);
    let (mut people, mut replies) = (0, 0);
    for &(y0, y1, _, role) in &side.app.ui.hit_lines {
        let px = side.buf[((y0 + y1) / 2.0) as usize * width + width - gutter - 1];
        if role & super::transcript::ROLE_BOX != 0 {
            assert_eq!(px, look(side, BACKGROUND), "a command box ends before the gutter, a little short of it");
        } else if role & ROLE_USER_BG != 0 {
            assert_eq!(px, look(side, USER_BG), "the band of the person's line ends at the gutter");
            people += 1;
        } else if kind_of(role) == Kind::Response {
            assert_eq!(px, look(side, RESPONSE_BG), "the reply has its band, to the gutter");
            replies += 1;
        }
    }
    (people, replies)
}

/// Every time of the last frame stands in a row whose whole label is above the hint line (and the path bar under it): none is drawn for a line that is under them.
fn times_stay_in_the_text_area(side: &Side) {
    let lh = side.app.line_h() * side.app.ui.scale;
    for row in side.app.ui.times.json()["drawn"].as_array().unwrap() {
        assert!(row[0].as_f64().unwrap() as f32 + lh <= side.size.1 as f32 - 2.0 * lh, "a time at {} is under the hint line", row[0]);
    }
}

/// The text of every time the last frame drew is the time of the first line of its block (the seeded runs begin at 1, 4, 7, ... and a run is 7 seconds on from the one before).
fn times_are_right(side: &Side) {
    for row in side.app.ui.times.json()["drawn"].as_array().unwrap() {
        let y0 = row[0].as_f64().unwrap() as f32;
        let line = side.app.ui.hit_lines.iter().find(|l| l.0 == y0).expect("a time stands at a line of the screen");
        let seq = side.app.view.as_ref().unwrap().lines[line.2].0;
        let secs = 1_795_000_000 + ((seq - 1) / 3 * 3 + 1) as u64 * 7;
        assert_eq!(row[2].as_str().unwrap(), super::blocks::stamp_text(secs, crate::transfer::utc_offset(secs as i64)), "the time of line {seq}");
    }
}

#[test]
fn the_gutter_with_the_times_draws_the_same_pixels_with_the_scroll_step_as_with_a_full_repaint_across_scales() {
    for scale in [1.0, 1.25, 2.0] {
        let mut p = Pair::new((2400, 700), scale);
        p.full("first", Cause::First);
        assert!(drawn(&p.on).0 > 0 && drawn(&p.on).1 == 0, "a session keeps its gutter before its first line, so that the text never moves when the lines come");
        for turn in 0..12 {
            p.push(&[
                format!(">> user prompt {turn} 한글 🙂"),
                format!("Agent answer {turn} with details and wrapped prose."),
                "$ cargo test --quiet".into(),
                "> test result: ok.".into(),
                "[exit 0]".into(),
                format!("Agent prose after the command {turn}."),
                format!(">> {}", "a long question that fills the width ".repeat(4)),
                "word ".repeat(30),
            ]);
            p.step(&format!("turn {turn}, no time known yet"));
        }
        let (gutter, rows) = drawn(&p.on);
        assert!(gutter_plain(&p.on), "no band, box or text reaches the gutter, however long the lines");
        let (people, replies) = band_ends(&p.on);
        assert!(people > 0 && replies > 0, "the frame has a person's line and a reply to look at ({people}, {replies})");
        assert!(gutter > 0, "the window is wide enough for a gutter at scale {scale}");
        assert_eq!(rows, 0, "no time is known, so none is drawn, and the gutter stays");
        let to = p.seq + 1;
        know(&mut p, to);
        p.step("a late answer brings the times");
        let (gutter_now, rows_now) = drawn(&p.on);
        assert_eq!(gutter_now, gutter, "a time arriving late does not move the text");
        assert!(rows_now >= 5, "the first line on the screen and the blocks of the turns show a time, drew {rows_now}");
        assert_eq!(drawn(&p.off), (gutter_now, rows_now), "a full repaint draws the same texts");
        times_are_right(&p.on);
        times_are_right(&p.off);
        times_stay_in_the_text_area(&p.on);
        for n in 0..10 {
            p.push(&[format!("one more line {n}")]);
            let to = p.seq + 1;
            know(&mut p, to);
            p.shift(&format!("a line {n} with times"));
            times_stay_in_the_text_area(&p.on);
        }
        p.push(&["$ echo more".into(), "inside the box".into(), "[exit 1]".into(), "prose".into()]);
        let to = p.seq + 1;
        know(&mut p, to);
        p.shift("a command card with times");
        p.resize((2200, 650), 1);
        p.full("resize with times", Cause::Size);
    }
}

#[test]
fn a_page_in_the_main_area_has_no_gutter_no_time_and_no_band_for_its_blank_rows() {
    let mut side = Side::new((2400, 700), 1.0, false);
    side.app.session.clear();
    side.app.ui.browser.prompt = Some(true);
    assert!(side.app.page_active(), "the address prompt of a new browser tab is a page");
    side.frame();
    assert_eq!(drawn(&side), (0, 0), "no room is kept beside a page and no time is drawn");
    assert_eq!(side.buf.iter().filter(|&&px| px == super::transcript::RESPONSE_BG).count(), 0, "the blank rows under a page are not the agent's replies");
}

fn fold(p: &mut Pair, first_seq: i64) {
    p.both(|app| {
        app.ui.block_fold.folded.insert((app.session.clone(), first_seq));
        app.ui.composed = None; // what a click that folds does
        app.dirty = true;
    });
}

#[test]
fn a_block_that_is_folded_draws_the_same_pixels_with_the_scroll_step_as_with_a_full_repaint_across_scales() {
    for scale in [1.0, 1.25, 2.0] {
        let mut p = Pair::new((2400, 700), scale);
        p.full("first", Cause::First);
        for turn in 0..8 {
            p.push(&[format!(">> question {turn}"), format!("Reply {turn} line a"), format!("Reply {turn} line b with more words"), format!("Reply {turn} line c")]);
            p.step(&format!("turn {turn}"));
        }
        let before = p.on.app.shown_lines_dump().len();
        fold(&mut p, 4 * 5 + 2); // the reply of the sixth turn
        fold(&mut p, 4 * 7 + 2); // and the last one
        p.step("two blocks folded");
        let after = p.on.app.shown_lines_dump();
        assert_eq!(after.len(), before - 4, "each of the two replies is one line, from three");
        assert_eq!(after.iter().filter(|l| l.starts_with("▸ Reply")).count(), 2);
        assert_eq!(p.on.app.shown_lines_dump(), p.off.app.shown_lines_dump());
        let line = p.on.app.ui.hit_lines.iter().rev().find(|l| l.3 & super::transcript::ROLE_BLOCK_FOLDED != 0).expect("the folded reply is on the screen");
        assert_eq!(super::blocks::kind_of(line.3), super::blocks::Kind::Response, "it keeps the colour of the agent's reply");
        // the selection of a folded block, and of an open one
        p.both(|app| app.ui.sel = Some((4 * 7 + 1, 4 * 7 + 1)));
        p.step("the folded block is selected");
        p.both(|app| app.ui.sel = Some((4 * 6 + 1, 4 * 6 + 3)));
        p.step("an open block is selected");
        p.both(|app| app.ui.sel = None);
        // the folded reply grows: its count changes and nothing else
        p.push(&["Reply 7 line d".into()]);
        p.step("the folded reply grows");
        assert!(p.on.app.shown_lines_dump().last().is_some_and(|l| l.ends_with("· 4 lines")), "{:?}", p.on.app.shown_lines_dump().last());
        for n in 0..6 {
            p.push(&[format!(">> more {n}"), format!("An answer {n}")]);
            p.step(&format!("lines {n} after a folded block"));
        }
        // reading older lines with folded blocks on the screen
        p.both(|app| app.ui.scroll = 9);
        p.step("scrolled up");
        p.both(|app| app.ui.scroll = 0);
        p.step("back down");
        // unfolded again: the block is as it was
        p.both(|app| {
            app.ui.block_fold.folded.clear();
            app.ui.composed = None;
            app.dirty = true;
        });
        p.step("everything unfolded");
        assert_eq!(p.on.app.shown_lines_dump().iter().filter(|l| l.starts_with('▸')).count(), 0);
    }
}

#[cfg(test)]
#[path = "theme_oracle.rs"]
mod light;
