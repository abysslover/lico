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
}

impl Times {
    /// The second the line `seq` was written at, when the answers cover it and a run begins at or before it.
    pub(super) fn at(&self, seq: i64) -> Option<u64> {
        if seq < self.lo || seq >= self.hi {
            return None;
        }
        self.stamps.at(seq)
    }

    /// The next request for the session `session` whose view holds the lines `have` (the first and one past the last), at tick `now`, or None: nothing is missing, or an answer is awaited.
    /// A view that the known range neither overlaps nor adjoins (another session, or a part of the same one with a gap between) starts again: what was known is let go, so that the range stays one piece.
    pub(super) fn next(&mut self, session: &str, have: Option<(i64, i64)>, now: u64) -> Option<String> {
        let (first, end) = have.filter(|h| h.0 < h.1)?;
        if self.pending.is_some_and(|p| now.saturating_sub(p.2) < RETRY_TICKS && self.session == session) {
            return None;
        }
        let range = if self.session != session || self.lo >= self.hi || end < self.lo || first > self.hi {
            *self = Times { session: session.to_string(), asked: self.asked, answered: self.answered, ..Times::default() };
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
        })
    }
}

impl super::App {
    /// Once a tick: asks for the times of the lines the view holds and does not know yet.
    pub(crate) fn times_tick(&mut self) {
        let have = self.view.as_ref().and_then(|view| Some((view.lines.front()?.0, view.lines.back()?.0 + 1)));
        if let Some(request) = self.ui.times.next(&self.session, have, self.ui.ticks) {
            self.send(&request);
        }
    }

    /// The host's answer to a `stamps` request. Nothing is redrawn for it (the gutter that shows a time is `M24-P11b`'s).
    pub(crate) fn ui_stamps(&mut self, line: &str) {
        if let Ok(value) = serde_json::from_str::<Value>(line) {
            self.ui.times.on_reply(&value);
        }
    }
}

#[cfg(test)]
#[path = "times_tests.rs"]
mod tests;
