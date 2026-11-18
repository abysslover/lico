//! How a terminal's screen is told over the socket (M23-P02): a line is a list of runs `[text, fg, bg, attrs]`, a colour is one number, and the trailing blank cells of a line are
//! not sent. Pure functions of the emulator's types; the window turns the numbers into its palette.

use lico_vt::{Attrs, Cell, Color, Modes, MouseMode};
use serde_json::{json, Value};

/// `-1` the default colour, `0..=255` an indexed colour, `256 + (r << 16 | g << 8 | b)` a true colour.
pub fn color(c: Color) -> i64 {
    match c {
        Color::Default => -1,
        Color::Indexed(i) => i64::from(i),
        Color::Rgb(r, g, b) => 256 + ((i64::from(r) << 16) | (i64::from(g) << 8) | i64::from(b)),
    }
}

/// The attributes that are drawn (bold, dim, italic, underline, blink, inverse, hidden, strike): the low byte; the wide and tail flags are the layout's, not the style's.
pub fn attrs(a: Attrs) -> i64 {
    i64::from(a.bits() & 0xFF)
}

fn plain_blank(c: &Cell) -> bool {
    c.ch == ' ' && c.fg == Color::Default && c.bg == Color::Default && attrs(c.attrs) == 0 && !c.attrs.contains(Attrs::TAIL)
}

/// One line as runs. The tail of a wide character is skipped (the wide cell already holds the character); trailing default blanks are dropped.
pub fn runs(cells: &[Cell]) -> Value {
    let end = cells.iter().rposition(|c| !plain_blank(c)).map_or(0, |i| i + 1);
    let mut out: Vec<Value> = Vec::new();
    let mut text = String::new();
    let mut style: Option<(i64, i64, i64)> = None;
    for c in &cells[..end] {
        if c.attrs.contains(Attrs::TAIL) {
            continue;
        }
        let s = (color(c.fg), color(c.bg), attrs(c.attrs));
        if style != Some(s) {
            if let Some((fg, bg, at)) = style {
                out.push(json!([std::mem::take(&mut text), fg, bg, at]));
            }
            style = Some(s);
        }
        text.push(c.ch);
    }
    if let Some((fg, bg, at)) = style {
        out.push(json!([text, fg, bg, at]));
    }
    Value::Array(out)
}

pub fn modes(m: Modes) -> Value {
    let mouse = match m.mouse {
        MouseMode::Off => "off",
        MouseMode::Press => "press",
        MouseMode::Drag => "drag",
        MouseMode::Any => "any",
    };
    json!({"app_cursor": m.app_cursor, "app_keypad": m.app_keypad, "bracketed_paste": m.bracketed_paste, "alt_screen": m.alt_screen, "mouse": mouse, "mouse_sgr": m.mouse_sgr, "focus_events": m.focus_events, "cursor_visible": m.cursor_visible})
}
