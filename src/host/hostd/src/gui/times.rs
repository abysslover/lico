//! When each line of the shown conversation was written (`M24-P11a`): the window's side of the host's `stamps` request. Nothing is drawn here (the gutter is `M24-P11b`'s); this keeps
//! the runs the host answered with, for the session that is shown, and the range of line numbers they answer for, and says what to ask next. The host keeps the times, not the
//! lines (the protocol's lines are `[sequence, text]`), so the window asks for them on their own: about once a second, one request at a time, the newest lines first, and only for
//! what the view holds and does not know yet.

use serde_json::{json, Value};

use crate::stamps::Stamps;

/// The most lines one request spans. The host answers at most 2,048 runs, and a span of this many lines holds at most this many runs and the one that covers its start, so an answer is never cut.
pub(super) const SPAN: i64 = 2000;
/// Ticks (seconds) an answer is awaited before the same request goes out again (an older host never answers).
const RETRY_TICKS: u64 = 10;

#[derive(Default)]
pub(crate) struct Times {
    session: String,
    stamps: Stamps,
    lo: i64,
    hi: i64,                          // the lines `lo` up to `hi` (not included) have their runs here; one piece, never two
    pending: Option<(i64, i64, u64)>, // the request that is awaited: from, until, the tick it went out at
    asked: u64,
    answered: u64,
    gutter: usize,                    // the pixels the last frame kept for the gutter (0: none), for the dump
    rows: Vec<(f32, f32, String)>,    // what the last frame drew in it: the top and bottom of the line and the text of its time
}

impl Times {
    /// The second the line `seq` was written at, when the answers cover it and a run begins at or before it.
    pub(super) fn at(&self, seq: i64) -> Option<u64> {
        if seq < self.lo || seq >= self.hi {
            return None;
        }
        self.stamps.at(seq)
    }

    /// A `Times` that knows `[lo, hi)` of the session `session` with `runs`, for the tests of what is drawn from it.
    #[cfg(test)]
    pub(super) fn seeded(session: &str, lo: i64, hi: i64, runs: &[(i64, u64)]) -> Times {
        let mut stamps = Stamps::new();
        stamps.merge(runs);
        Times { session: session.to_string(), stamps, lo, hi, ..Times::default() }
    }

    /// The next request for the session `session` whose view holds the lines `have` (the first and one past the last), at tick `now`, or None: nothing is missing, or an answer is awaited.
    /// A view that the known range neither overlaps nor adjoins (another session, or a part of the same one with a gap between) starts again: what was known is let go, so that the range stays one piece.
    pub(super) fn next(&mut self, session: &str, have: Option<(i64, i64)>, now: u64) -> Option<String> {
        let (first, end) = have.filter(|h| h.0 < h.1)?;
        if self.pending.is_some_and(|p| now.saturating_sub(p.2) < RETRY_TICKS && self.session == session) {
            return None;
        }
        let range = if self.session != session || self.lo >= self.hi || end < self.lo || first > self.hi {
            *self = Times { session: session.to_string(), asked: self.asked, answered: self.answered, gutter: self.gutter, rows: std::mem::take(&mut self.rows), ..Times::default() };
            ((end - SPAN).max(first), end)
        } else if end > self.hi {
            (self.hi, end.min(self.hi + SPAN))
        } else if first < self.lo {
            (first.max(self.lo - SPAN), self.lo)
        } else {
            return None;
        };
        self.pending = Some((range.0, range.1, now));
        self.asked += 1;
        Some(json!({"op": "stamps", "session": session, "from": range.0, "until": range.1}).to_string())
    }

    /// Takes in an answer, when it is the one that is awaited (the same session and range); true then. An entry that is not `[sequence, seconds]` is skipped.
    pub(super) fn on_reply(&mut self, value: &Value) -> bool {
        let Some((from, until, _)) = self.pending else { return false };
        if value["session"].as_str() != Some(self.session.as_str()) || value["from"].as_i64() != Some(from) || value["until"].as_i64() != Some(until) {
            return false;
        }
        let runs: Vec<(i64, u64)> = value["stamps"].as_array().into_iter().flatten().filter_map(|run| Some((run[0].as_i64()?, run[1].as_u64()?))).collect();
        self.stamps.merge(&runs);
        if self.lo >= self.hi {
            (self.lo, self.hi) = (from, until);
        } else if from == self.hi {
            self.hi = until;
        } else if until == self.lo {
            self.lo = from;
        }
        self.pending = None;
        self.answered += 1;
        true
    }

    /// What the state dump says, for the tests: the session, the range, the number of runs, the time of the first and of the newest line known, the last run, and the requests.
    pub(super) fn json(&self) -> Value {
        json!({
            "session": self.session, "lo": self.lo, "hi": self.hi, "runs": self.stamps.len(), "first": self.at(self.lo), "newest": self.at(self.hi - 1),
            "last_run": self.stamps.last().map(|(seq, secs)| json!([seq, secs])), "asked": self.asked, "answered": self.answered, "pending": self.pending.is_some(),
            "gutter": self.gutter, "drawn": self.rows.iter().map(|r| json!([r.0, r.1, r.2])).collect::<Vec<_>>(),
        })
    }
}

/// The lines of the screen that carry a time: the top and bottom of the first line of every block and of the first line on the screen, with the text of its time, for the
/// lines whose time is known. `hit` is the lines on the screen as `(top, bottom, raw index, role)`; `seq_of` says the sequence number of a raw index and `text_of` writes a time.
pub(super) fn stamp_rows(hit: &[(f32, f32, usize, u16)], seq_of: &dyn Fn(usize) -> Option<i64>, at: &dyn Fn(i64) -> Option<u64>, text_of: &dyn Fn(u64) -> String) -> Vec<(f32, f32, String)> {
    let mut previous = None;
    let mut rows = Vec::new();
    for &(top, bottom, raw, role) in hit {
        if super::blocks::starts_block(previous, role) {
            if let Some(secs) = seq_of(raw).and_then(at) {
                rows.push((top, bottom, text_of(secs)));
            }
        }
        previous = Some(role);
    }
    rows
}

/// Clears the gutter of every line on the screen (the part of the window that is `gutter` pixels from the right edge) to the window's background: the rows that a scroll step kept
/// carry the last frame's times, and the ones that are drawn now go over a clean ground.
pub(super) fn gutter_clear(buf: &mut [u32], width: usize, height: usize, gutter: usize, top_y: i32, hint_y: i32, hit: &[(f32, f32, usize, u16)]) {
    let left = width.saturating_sub(gutter);
    for &(y0, y1, _, _) in hit {
        let (from, to) = ((y0.floor() as i32).max(top_y).max(0), (y1.ceil() as i32).min(hint_y).min(height as i32)); // whole rows that the line touches, inside the text area
        for y in from..to {
            buf[y as usize * width + left..(y as usize + 1) * width].fill(super::theme::tone(super::BACKGROUND));
        }
    }
}

/// Draws the times of `rows` at the left of the gutter, one column in.
#[allow(clippy::too_many_arguments)]
pub(super) fn gutter_draw(controls: &mut super::controls::Controls, fs: &mut cosmic_text::FontSystem, swash: &mut cosmic_text::SwashCache, buf: &mut [u32], width: usize, height: usize, gutter: usize, col_px: f32, top_y: i32, hint_y: i32, rows: &[(f32, f32, String)]) {
    let (x, label_h, ink) = ((width.saturating_sub(gutter)) as f32 + col_px, (controls.font_px + 4.0) * controls.scale, cosmic_text::Color::rgb(0x7A, 0x84, 0x96));
    for (y0, _, text) in rows.iter().filter(|r| r.0 >= top_y as f32 && r.0 + label_h <= hint_y as f32) {
        controls.text(fs, swash, buf, width, height, x, *y0, width as i32, text, ink);
    }
}

impl Times {
    /// Draws the times of the lines on the screen (the gutter of each was cleared before the text) and remembers what it drew, for the dump. `gutter` 0 draws nothing.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn paint(&mut self, controls: &mut super::controls::Controls, fs: &mut cosmic_text::FontSystem, swash: &mut cosmic_text::SwashCache, buf: &mut [u32], size: (usize, usize), gutter: usize, col_px: f32, area: (i32, i32), hit: &[(f32, f32, usize, u16)], seq_of: &dyn Fn(usize) -> Option<i64>) {
        self.gutter = gutter;
        self.rows = if gutter == 0 { Vec::new() } else { stamp_rows(hit, seq_of, &|seq| self.at(seq), &|secs| super::blocks::stamp_text(secs, crate::transfer::utc_offset(secs as i64))) };
        gutter_draw(controls, fs, swash, buf, size.0, size.1, gutter, col_px, area.0, area.1, &self.rows);
    }
}

impl super::App {
    /// The pixels kept at the right of the transcript of a session for the time of each block (0: none) and the width of a column: the area has the columns for it, and it is the
    /// transcript of a session (not a panel or the start lines; a browser or terminal tab is shown only with no session selected, so it is no session's either).
    pub(crate) fn times_gutter(&self, main_w: usize, s: f32) -> (usize, f32) {
        let col_px = self.char_w() * s;
        if self.ui.panel.is_some() || self.session.is_empty() {
            return (0, col_px);
        }
        (super::blocks::gutter_px(main_w.saturating_sub((4 + 4) * s as usize), col_px), col_px)
    }

    /// Once a tick: asks for the times of the lines the view holds and does not know yet.
    pub(crate) fn times_tick(&mut self) {
        let have = self.view.as_ref().and_then(|view| Some((view.lines.front()?.0, view.lines.back()?.0 + 1)));
        if let Some(request) = self.ui.times.next(&self.session, have, self.ui.ticks) {
            self.send(&request);
        }
    }

    /// The host's answer to a `stamps` request; when it was the one awaited the window is drawn again, so that the times it brought show.
    pub(crate) fn ui_stamps(&mut self, line: &str) {
        if let Ok(value) = serde_json::from_str::<Value>(line) {
            self.dirty |= self.ui.times.on_reply(&value);
        }
    }
}

#[cfg(test)]
#[path = "times_tests.rs"]
mod tests;
