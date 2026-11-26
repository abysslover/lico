//! A time for the lines that have no recorded one (`M25-P10`).
//!
//! `M24-P11a` keeps the time of each record in a file beside the log, but a session that was written before that build, or whose file is gone, has none for its old lines, and the
//! window drew nothing at their right. The log keeps its records in segment files (`<id>.<first sequence>.seg`) and the file system knows when each was last written (its
//! modification time) and, where it keeps one, when it was made (its birth time): the lines of a segment were written between the last write of the segment before it (or, for the
//! oldest, the birth of the file) and its own last write. A line is placed in that span in proportion to its place in the segment, as at most `STEPS` runs per segment (a run is
//! `(first sequence, unix seconds)`, as in the file of the times).
//!
//! The result is an estimate and the reply says so: `exact_from` names the first line whose time was recorded, and the window draws `~` before the time of the lines below it. No
//! time is made up from nothing: a segment whose file the file system cannot tell about has no run of its own, and a line at or after `exact_from` keeps its recorded time.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{Stamps, MAX_REPLY};

/// The most runs one segment gives (a segment holds at most 4,096 records).
const STEPS: i64 = 32;

/// What the file system says about one segment file: the sequence of its first record, how many records it holds, and when the file was made and last written (unix seconds).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    pub first: i64,
    pub count: i64,
    pub born: Option<u64>,
    pub modified: Option<u64>,
}

/// The answer of the host to a `stamps` request: the recorded runs, the estimated runs for the lines below the first recorded one, and where the recorded ones begin
/// (`None`: nothing was recorded, every line is an estimate).
#[derive(Debug, PartialEq, Eq)]
pub struct Answer {
    pub runs: Vec<(i64, u64)>,
    pub approx: Vec<(i64, u64)>,
    pub exact_from: Option<i64>,
}

fn secs(time: std::io::Result<SystemTime>) -> Option<u64> {
    time.ok()?.duration_since(UNIX_EPOCH).ok().map(|d| d.as_secs())
}

/// The answer for the lines `from..until` of a log whose segments are `list` (`(first sequence, records)`, oldest first) and whose files `path_of` names.
pub fn answer(stamps: &Stamps, list: &[(i64, usize)], path_of: &dyn Fn(i64) -> PathBuf, from: i64, until: i64) -> Answer {
    let runs = stamps.between(from, until, MAX_REPLY);
    let exact = stamps.first();
    let below = exact.map_or(until, |run| run.0.min(until)); // the lines of the request that no run was recorded for
    let mut approx = Vec::new();
    if below > from {
        let overlaps = |s: &(i64, usize)| s.0 < below && s.0.saturating_add(s.1 as i64) > from;
        if let Some(begin) = list.iter().position(overlaps) {
            let end = list.iter().rposition(overlaps).unwrap_or(begin) + 1;
            let segments: Vec<Segment> = list[begin.saturating_sub(1)..end] // (the one before the first: its last write is where that one began)
                .iter()
                .map(|&(first, count)| {
                    let meta = std::fs::metadata(path_of(first)).ok();
                    Segment { first, count: count as i64, born: meta.as_ref().and_then(|m| secs(m.created())), modified: meta.as_ref().and_then(|m| secs(m.modified())) }
                })
                .collect();
            let mut held = Stamps::new();
            held.merge(&estimate(&segments, exact));
            approx = held.between(from, below, MAX_REPLY.saturating_sub(runs.len()));
        }
    }
    Answer { runs, approx, exact_from: exact.map(|run| run.0) }
}

/// The estimated runs of `segments` (oldest first, one after the other) for the lines below the first recorded run `exact`. A segment is spread between the last write of the one
/// before it (its own birth when there is none before it) and its own last write; a segment with no last write has no run, one that was made after its last write (a copy) has only that
/// last write, and with no start at all the segment has one run, its last write. The segment that holds the first recorded line was written to after the upgrade, so its last write
/// says nothing of its old lines: they are placed at its start (the last write before it, or its birth) and have no run when it has none.
pub fn estimate(segments: &[Segment], exact: Option<(i64, u64)>) -> Vec<(i64, u64)> {
    let (cap_seq, cap_secs) = exact.unwrap_or((i64::MAX, u64::MAX));
    let mut out: Vec<(i64, u64)> = Vec::new();
    let mut before: Option<u64> = None; // the last write of the segment before this one
    for segment in segments {
        let end = segment.modified.map(|m| m.min(cap_secs));
        let lines = segment.first.saturating_add(segment.count).min(cap_seq) - segment.first;
        let carried = std::mem::replace(&mut before, end);
        if lines <= 0 {
            continue;
        }
        let start = carried.or(segment.born);
        let held = lines < segment.count; // the recorded lines begin inside this segment: its last write is theirs
        let Some((start, end)) = (if held { start.map(|s| (s, s)) } else { end.map(|e| (start.map_or(e, |s| s.min(e)), e)) }) else { continue };
        let steps = lines.min(STEPS);
        for step in 0..steps {
            let line = lines * step / steps; // the first line of this step
            let secs = if lines == 1 { end } else { start + (u128::from(end - start) * line as u128 / (lines as u128 - 1)) as u64 };
            if out.last().is_none_or(|last| last.1 != secs) {
                out.push((segment.first + line, secs));
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "approx_tests.rs"]
mod tests;
