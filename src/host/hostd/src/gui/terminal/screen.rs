//! The pure parts of the window's terminal tab (`M23-P03`): the colours, the screen as the host tells it (`terminal.watch`), where each run of text goes in the grid, the grid of an area,
//! the selection, and the words of the header and the bottom row. Nothing here draws, asks the host or reads the window; every function takes what it needs and gives a value, so the
//! tests in `screen_tests.rs` read them as a specification. What the host says is not trusted: a reply is read with bounds (sizes, lengths, colour numbers) and a field that is not
//! what it should be is dropped, never a panic.

use lico_vt::{char_width, Modes, MouseMode, Term};
use serde_json::Value;

/// The colours of a cell with no colour of its own.
pub(crate) const DEFAULT_FG: u32 = 0xFFE0_E0E0;
pub(crate) const DEFAULT_BG: u32 = 0xFF0F_131A;
/// The ground of a selected cell.
pub(crate) const SELECTION_BG: u32 = 0xFF2C_4A78;

/// The attributes of a run, as the host numbers them (`terminal/encode.rs`: the low byte of the emulator's flags).
pub(crate) const BOLD: i64 = 1;
pub(crate) const DIM: i64 = 1 << 1;
pub(crate) const UNDERLINE: i64 = 1 << 3;
pub(crate) const INVERSE: i64 = 1 << 5;
pub(crate) const HIDDEN: i64 = 1 << 6;
pub(crate) const STRIKE: i64 = 1 << 7;

/// The host's bounds (`lico_vt::MAX_COLS`, `MAX_ROWS`), and the smallest grid the window asks for.
pub(crate) const MAX_COLS: u16 = lico_vt::MAX_COLS;
pub(crate) const MAX_ROWS: u16 = lico_vt::MAX_ROWS;
pub(crate) const MIN_GRID: (u16, u16) = (2, 1);
/// The most characters one run keeps (a reply's run is never longer than the line; this bounds what a damaged one costs).
const MAX_RUN_CHARS: usize = 1_000;
const MAX_TITLE_CHARS: usize = 256;
/// The 16 colours of the terminal (the table of the usual editors' terminals).
const ANSI: [u32; 16] = [
    0x000000, 0xCD3131, 0x0DBC79, 0xE5E510, 0x2472C8, 0xBC3FBC, 0x11A8CD, 0xE5E5E5, 0x666666, 0xF14C4C, 0x23D18B, 0xF5F543, 0x3B8EEA, 0xD670D6, 0x29B8DB, 0xFFFFFF,
];

/// An indexed colour: the 16, then the 6 x 6 x 6 cube, then the 24 greys.
pub(crate) fn palette(index: u8) -> u32 {
    let i = u32::from(index);
    let rgb = match i {
        0..=15 => ANSI[i as usize],
        16..=231 => {
            let n = i - 16;
            let level = |v: u32| if v == 0 { 0 } else { 55 + 40 * v };
            (level(n / 36) << 16) | (level(n / 6 % 6) << 8) | level(n % 6)
        }
        _ => {
            let g = 8 + 10 * (i - 232);
            (g << 16) | (g << 8) | g
        }
    };
    0xFF00_0000 | rgb
}

/// The colour a number of the wire is: `-1` has none of its own, `0..=255` is indexed, `256 + (r << 16 | g << 8 | b)` is true colour; anything else has none.
pub(crate) fn colour(code: i64) -> Option<u32> {
    match code {
        0..=255 => Some(palette(code as u8)),
        256..=16_777_471 => Some(0xFF00_0000 | (code - 256) as u32),
        _ => None,
    }
}

fn mix(a: u32, b: u32) -> u32 {
    let ch = |shift: u32| (((a >> shift) & 0xFF) + ((b >> shift) & 0xFF)) / 2;
    0xFF00_0000 | (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

/// How one run is drawn: its two colours, whether its ground is painted at all (a run on the default ground is not painted over the window's own), and its lines.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Look {
    pub fg: u32,
    pub bg: u32,
    pub paint_bg: bool,
    pub underline: bool,
    pub strike: bool,
}

/// The look of a run. Bold takes the bright colour of an indexed 0 to 7; dim is half way to the ground; inverse swaps the two (and paints the ground, which is the foreground now); hidden
/// is drawn in the colour of its ground.
pub(crate) fn look(fg: i64, bg: i64, attrs: i64) -> Look {
    let (own_bg, own_fg) = (colour(bg), colour(fg));
    let mut f = if attrs & BOLD != 0 && (0..=7).contains(&fg) { palette(fg as u8 + 8) } else { own_fg.unwrap_or(DEFAULT_FG) };
    let mut b = own_bg.unwrap_or(DEFAULT_BG);
    if attrs & DIM != 0 {
        f = mix(f, b);
    }
    let inverse = attrs & INVERSE != 0;
    if inverse {
        std::mem::swap(&mut f, &mut b);
    }
    if attrs & HIDDEN != 0 {
        f = b;
    }
    Look { fg: f, bg: b, paint_bg: own_bg.is_some() || inverse, underline: attrs & UNDERLINE != 0, strike: attrs & STRIKE != 0 }
}

/// One run of a line: the cells that share a style.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Run {
    pub text: String,
    pub fg: i64,
    pub bg: i64,
    pub attrs: i64,
}

/// What the program switched on, as far as the window needs it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Wire {
    pub app_cursor: bool,
    pub app_keypad: bool,
    pub bracketed_paste: bool,
    pub alt_screen: bool,
    pub cursor_visible: bool,
    pub focus_events: bool,
    pub mouse: MouseMode,
    pub mouse_sgr: bool,
}

impl Default for Wire {
    /// What a terminal is before a program says anything.
    fn default() -> Wire {
        Wire { app_cursor: false, app_keypad: false, bracketed_paste: false, alt_screen: false, cursor_visible: true, focus_events: false, mouse: MouseMode::Off, mouse_sgr: false }
    }
}

impl Wire {
    fn read(v: &Value) -> Wire {
        let flag = |k: &str, d: bool| v.get(k).and_then(Value::as_bool).unwrap_or(d);
        let d = Wire::default();
        Wire {
            app_cursor: flag("app_cursor", d.app_cursor),
            app_keypad: flag("app_keypad", d.app_keypad),
            bracketed_paste: flag("bracketed_paste", d.bracketed_paste),
            alt_screen: flag("alt_screen", d.alt_screen),
            cursor_visible: flag("cursor_visible", d.cursor_visible),
            focus_events: flag("focus_events", d.focus_events),
            mouse: match v.get("mouse").and_then(Value::as_str) {
                Some("press") => MouseMode::Press,
                Some("drag") => MouseMode::Drag,
                Some("any") => MouseMode::Any,
                _ => MouseMode::Off,
            },
            mouse_sgr: flag("mouse_sgr", d.mouse_sgr),
        }
    }

    /// The emulator's modes for `lico_vt::encode_key` and the like (the three it does not tell over the wire are the emulator's own start values).
    pub(crate) fn modes(&self) -> Modes {
        let mut m = Term::new(MIN_GRID.0, MIN_GRID.1).modes();
        (m.app_cursor, m.app_keypad, m.bracketed_paste, m.alt_screen, m.cursor_visible, m.focus_events, m.mouse, m.mouse_sgr) =
            (self.app_cursor, self.app_keypad, self.bracketed_paste, self.alt_screen, self.cursor_visible, self.focus_events, self.mouse, self.mouse_sgr);
        m
    }
}

/// The screen of a terminal as one `terminal.watch` answer tells it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Screen {
    pub seq: u64,
    pub cols: u16,
    pub rows: u16,
    pub title: String,
    /// The cursor's row and column on the screen; none when the program hid it or the view is scrolled back.
    pub cursor: Option<(u16, u16)>,
    pub modes: Wire,
    /// How many rows the view is above the bottom, the absolute number of its first line, and the lines there are in all (scrollback and screen).
    pub scroll: usize,
    pub top: usize,
    pub total: usize,
    pub lines: Vec<Vec<Run>>,
    pub running: bool,
    pub exit: Option<i64>,
}

impl Default for Screen {
    fn default() -> Screen {
        Screen { seq: 0, cols: 0, rows: 0, title: String::new(), cursor: None, modes: Wire::default(), scroll: 0, top: 0, total: 0, lines: Vec::new(), running: true, exit: None }
    }
}

fn clean(text: &str, most: usize) -> String {
    text.chars().filter(|c| !c.is_control()).take(most).collect()
}

fn run_of(v: &Value) -> Option<Run> {
    let a = v.as_array().filter(|a| a.len() >= 4)?;
    Some(Run { text: clean(a[0].as_str()?, MAX_RUN_CHARS), fg: a[1].as_i64()?, bg: a[2].as_i64()?, attrs: a[3].as_i64()? })
}

impl Screen {
    /// Reads a `terminal.watch` answer; none when it has no change number or a size the host never gives. Extra lines are cut, a line or a run of the wrong shape is dropped (a line
    /// that is not an array is an empty one, so the rows below it stay where they are).
    pub(crate) fn parse(v: &Value) -> Option<Screen> {
        let seq = v.get("seq")?.as_u64()?;
        let size = |k: &str, max: u16| v.get(k).and_then(Value::as_u64).filter(|n| (1..=u64::from(max)).contains(n)).map(|n| n as u16);
        let (cols, rows) = (size("cols", MAX_COLS)?, size("rows", MAX_ROWS)?);
        let cursor = v
            .get("cursor")
            .and_then(Value::as_array)
            .and_then(|c| Some((c.first()?.as_u64()?, c.get(1)?.as_u64()?)))
            .filter(|(r, c)| *r < u64::from(rows) && *c < u64::from(cols))
            .map(|(r, c)| (r as u16, c as u16));
        let count = |k: &str| v.get(k).and_then(Value::as_u64).map_or(0, |n| n.min(1_000_000_000) as usize);
        let lines = v
            .get("lines")
            .and_then(Value::as_array)
            .map(|ls| ls.iter().take(usize::from(rows)).map(|l| l.as_array().map(|runs| runs.iter().filter_map(run_of).collect()).unwrap_or_default()).collect())
            .unwrap_or_default();
        Some(Screen {
            seq,
            cols,
            rows,
            title: clean(v.get("title").and_then(Value::as_str).unwrap_or(""), MAX_TITLE_CHARS),
            cursor,
            modes: v.get("modes").map(Wire::read).unwrap_or_default(),
            scroll: count("scroll"),
            top: count("top"),
            total: count("total"),
            lines,
            running: v.get("running").and_then(Value::as_bool).unwrap_or(true),
            exit: v.get("exit").and_then(Value::as_i64),
        })
    }

    /// The text of the screen row by row (a line's runs joined, the right half of a wide character is not there): what a test reads, and what the dump says.
    pub(crate) fn plain_lines(&self) -> Vec<String> {
        self.lines.iter().map(|l| l.iter().map(|r| r.text.as_str()).collect::<String>().trim_end().to_string()).collect()
    }
}

/// The cells of a line: a run's columns, from `col`, as many as its characters take.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Span {
    pub run: usize,
    pub col: usize,
    pub cols: usize,
}

/// Where each run of a line is, in columns, cut at `max_cols`.
pub(crate) fn spans(runs: &[Run], max_cols: usize) -> Vec<Span> {
    let mut out = Vec::new();
    let mut col = 0;
    for (i, r) in runs.iter().enumerate() {
        let width: usize = r.text.chars().map(|c| usize::from(char_width(c))).sum();
        if col >= max_cols {
            break;
        }
        out.push(Span { run: i, col, cols: width.min(max_cols - col) });
        col += width;
    }
    out
}

/// A piece of text to draw at a column: a run of narrow characters is one piece, a wide character is a piece of its own (drawn at its own cell, so that the font's advance for it does not
/// move what follows).
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct Piece {
    pub run: usize,
    pub col: usize,
    pub cols: usize,
    pub text: String,
}

/// The pieces of a line, cut at `max_cols` (a wide character that does not fit is not drawn); pieces of spaces alone are left out (nothing to draw, and the ground is the run's).
pub(crate) fn pieces(runs: &[Run], max_cols: usize) -> Vec<Piece> {
    let mut out = Vec::new();
    let mut col = 0;
    'runs: for (i, r) in runs.iter().enumerate() {
        let mut narrow = Piece { run: i, col, cols: 0, text: String::new() };
        for c in r.text.chars() {
            let w = usize::from(char_width(c));
            if w == 0 {
                continue;
            }
            if col + w > max_cols {
                flush(&mut out, &mut narrow);
                break 'runs;
            }
            if w == 2 {
                flush(&mut out, &mut narrow);
                out.push(Piece { run: i, col, cols: 2, text: c.to_string() });
                narrow = Piece { run: i, col: col + 2, cols: 0, text: String::new() };
            } else {
                narrow.text.push(c);
                narrow.cols += 1;
            }
            col += w;
        }
        flush(&mut out, &mut narrow);
    }
    out
}

fn flush(out: &mut Vec<Piece>, narrow: &mut Piece) {
    if !narrow.text.is_empty() && narrow.text.chars().any(|c| c != ' ') {
        out.push(narrow.clone());
    }
    narrow.col += narrow.cols;
    narrow.cols = 0;
    narrow.text.clear();
}

/// The character that is at a column of a line, and how many columns it takes (the cursor's block is drawn over it in the ground's colour); none when the column is past the text.
pub(crate) fn char_at(runs: &[Run], col: usize) -> Option<(char, usize)> {
    let mut at = 0;
    for c in runs.iter().flat_map(|r| r.text.chars()) {
        let w = usize::from(char_width(c));
        if w == 0 {
            continue;
        }
        if col < at + w {
            return Some((c, w));
        }
        at += w;
    }
    None
}

/// The grid of an area: whole cells only, at most what the host takes, at least `MIN_GRID`; a cell that is not a positive number gives the host's default size.
pub(crate) fn grid_of(width: f32, height: f32, cell_w: f32, cell_h: f32) -> (u16, u16) {
    if !(cell_w.is_finite() && cell_h.is_finite() && cell_w > 0.0 && cell_h > 0.0 && width.is_finite() && height.is_finite()) {
        return (80, 24);
    }
    let n = |len: f32, cell: f32, lo: u16, hi: u16| ((len.max(0.0) / cell).floor() as u32).clamp(u32::from(lo), u32::from(hi)) as u16;
    (n(width, cell_w, MIN_GRID.0, MAX_COLS), n(height, cell_h, MIN_GRID.1, MAX_ROWS))
}

/// The cell under a point of the window, none outside the grid.
pub(crate) fn cell_at(point: (f32, f32), origin: (f32, f32), cell: (f32, f32), grid: (u16, u16)) -> Option<(u16, u16)> {
    let (x, y) = (point.0 - origin.0, point.1 - origin.1);
    if x < 0.0 || y < 0.0 || cell.0 <= 0.0 || cell.1 <= 0.0 {
        return None;
    }
    let (col, row) = ((x / cell.0) as u32, (y / cell.1) as u32);
    (col < u32::from(grid.0) && row < u32::from(grid.1)).then_some((col as u16, row as u16))
}

/// The cell nearest a point, for a drag that goes outside the grid: the point is brought inside first.
pub(crate) fn cell_near(point: (f32, f32), origin: (f32, f32), cell: (f32, f32), grid: (u16, u16)) -> (u16, u16) {
    let (cols, rows) = (f32::from(grid.0.max(1)), f32::from(grid.1.max(1)));
    let (x, y) = ((point.0 - origin.0).clamp(0.0, cols * cell.0 - 1.0), (point.1 - origin.1).clamp(0.0, rows * cell.1 - 1.0));
    cell_at((x + origin.0, y + origin.1), origin, cell, grid).unwrap_or((0, 0))
}

/// A selection: from the cell where the press was to the cell where the pointer is, as an absolute line (0 is the oldest line of the scrollback) and a column, so that it stays on the
/// same text when the screen scrolls.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Sel {
    pub anchor: (usize, u16),
    pub focus: (usize, u16),
}

impl Sel {
    /// From the earlier end to the later one.
    pub(crate) fn ordered(&self) -> ((usize, u16), (usize, u16)) {
        if self.anchor <= self.focus { (self.anchor, self.focus) } else { (self.focus, self.anchor) }
    }

    /// A click that did not move selects nothing.
    pub(crate) fn is_point(&self) -> bool {
        self.anchor == self.focus
    }

    /// The columns of an absolute line that are selected, both ends included; none when the line is outside it (or it is a point).
    pub(crate) fn span_on(&self, line: usize, cols: u16) -> Option<(u16, u16)> {
        if self.is_point() || cols == 0 {
            return None;
        }
        let ((l0, c0), (l1, c1)) = self.ordered();
        if line < l0 || line > l1 {
            return None;
        }
        let from = if line == l0 { c0 } else { 0 };
        let to = if line == l1 { c1 } else { cols - 1 };
        Some((from.min(cols - 1), to.min(cols - 1)))
    }
}

/// The last component of a folder.
fn leaf(path: &str) -> &str {
    path.trim_end_matches('/').rsplit('/').next().unwrap_or("")
}

/// The caption of a terminal tab: its title, else the folder's last name, else its id; short.
pub(crate) fn caption(title: &str, cwd: &str, id: &str) -> String {
    let name = [title.trim(), leaf(cwd)].into_iter().find(|s| !s.is_empty()).unwrap_or(id);
    let mut cut: String = name.chars().take(20).collect();
    if name.chars().count() > 20 {
        cut.push('\u{2026}');
    }
    format!(">_ {cut}")
}

/// The header of the window while a terminal is shown.
pub(crate) fn header(title: &str, cwd: &str, running: bool) -> String {
    let state = if running { "" } else { " | ended" };
    format!("terminal | {}{} | {cwd} | not fenced: it runs as you", if title.trim().is_empty() { "shell" } else { title.trim() }, state)
}

/// The one row under the grid: what is Lico's among the keys, what the person is composing, how far back the view is, or that the shell has ended.
pub(crate) fn status_row(running: bool, exit: Option<i64>, scroll: usize, composing: &str) -> String {
    if !running {
        return match exit {
            Some(n) => format!("the shell ended (exit {n}): Enter closes this tab"),
            None => "the shell ended: Enter closes this tab".into(),
        };
    }
    if !composing.is_empty() {
        return format!("composing: {composing}");
    }
    if scroll > 0 {
        return format!("history: {scroll} {} up   typing or Shift+End returns   Shift+PgUp/PgDn move", if scroll == 1 { "line" } else { "lines" });
    }
    "Ctrl+Shift+C copy   Ctrl+Shift+V paste   Shift+PgUp/PgDn history   Ctrl+Shift+W close".into()
}
