use super::*;
use std::fs::OpenOptions;
use std::path::Path;
use std::time::Duration;

fn seg(first: i64, count: i64, born: Option<u64>, modified: Option<u64>) -> Segment {
    Segment { first, count, born, modified }
}

/// Three segments of 100 lines, written one after the other: the files were last written at 1000, 2000 and 3000, the oldest was made at 900.
fn three() -> Vec<Segment> {
    vec![seg(0, 100, Some(900), Some(1000)), seg(100, 100, Some(1000), Some(2000)), seg(200, 100, Some(2000), Some(3000))]
}

fn time_of(runs: &[(i64, u64)], seq: i64) -> Option<u64> {
    let mut held = Stamps::new();
    held.merge(runs);
    held.at(seq)
}

#[test]
fn a_line_is_placed_between_the_last_write_before_its_segment_and_the_last_write_of_its_own() {
    let got = estimate(&three(), None);
    assert_eq!(got.first(), Some(&(0, 900)), "the first line of the oldest segment was written when its file was made");
    assert!(got.contains(&(100, 1000)) && got.contains(&(200, 2000)), "a segment begins where the one before it ended: {got:?}");
    assert!(got.windows(2).all(|w| w[0].0 < w[1].0 && w[0].1 < w[1].1), "forward in both, and two runs never share a second: {got:?}");
    for (index, segment) in three().iter().enumerate() {
        let inside: Vec<_> = got.iter().filter(|r| r.0 >= segment.first && r.0 < segment.first + segment.count).collect();
        let (low, high) = (if index == 0 { 900 } else { 1000 * index as u64 }, segment.modified.unwrap());
        assert!(!inside.is_empty() && inside.len() <= 32, "at most 32 runs a segment: {}", inside.len());
        assert!(inside.iter().all(|r| (low..=high).contains(&r.1)), "never before the write that came before it and never after its own: {inside:?}");
    }
    let middle = time_of(&got, 150).unwrap();
    assert!((1480..=1520).contains(&middle), "the middle line of the middle segment is in the middle of its span: {middle}");
    assert!(time_of(&got, 299).unwrap() >= 2900, "the last lines are close to the last write");
}

#[test]
fn no_time_is_made_up_a_file_with_no_write_time_gives_no_run_and_what_is_known_is_kept() {
    assert_eq!(estimate(&[seg(0, 10, Some(5), None)], None), vec![], "no write time, no run");
    assert_eq!(estimate(&[seg(0, 10, Some(2000), Some(1000))], None), vec![(0, 1000)], "made after its last write (a copy): the last write is all there is");
    assert_eq!(estimate(&[seg(0, 10, None, Some(1000))], None), vec![(0, 1000)], "no start at all: one run, the last write");
    assert_eq!(estimate(&[seg(0, 1, Some(10), Some(20))], None), vec![(0, 20)], "a segment of one line was written at its last write");
    assert_eq!(estimate(&[seg(0, 0, Some(10), Some(20))], None), vec![], "an empty segment has no line");
    assert_eq!(estimate(&[], None), vec![]);
    let gap = [seg(0, 10, Some(100), Some(200)), seg(10, 10, Some(150), None), seg(20, 10, Some(250), Some(300))];
    let got = estimate(&gap, None);
    assert!(got.iter().all(|r| !(10..20).contains(&r.0)), "the segment with no write time has no run of its own: {got:?}");
    assert_eq!(got.iter().find(|r| r.0 == 20), Some(&(20, 250)), "the one after it starts at its own birth, not at the write before the gap");
}

#[test]
fn the_estimate_ends_where_the_first_recorded_run_begins() {
    let got = estimate(&three(), Some((150, 1500)));
    assert!(got.iter().all(|r| r.0 < 150 && r.1 <= 1500), "nothing at or after the first recorded line, no time after its time: {got:?}");
    assert!(got.iter().any(|r| r.0 >= 100), "the segment that holds the first recorded line has its old lines too");
    assert_eq!(time_of(&got, 149), Some(1000), "they are at the start of that segment: its last write moved when the recorded lines were written, so it says nothing of them");
    assert_eq!(estimate(&three(), Some((0, 900))), vec![], "recorded from the very first line: nothing to estimate");
    let at_the_edge = estimate(&three(), Some((200, 2000)));
    assert!(at_the_edge.iter().all(|r| r.0 < 200) && time_of(&at_the_edge, 199).unwrap() >= 1900, "recorded from the first line of a segment: the segment before it is whole and keeps its spread: {at_the_edge:?}");
    let skewed = [seg(0, 100, Some(900), Some(5000)), seg(100, 100, Some(1000), Some(6000))];
    let capped = estimate(&skewed, Some((150, 1500)));
    assert!(!capped.is_empty() && capped.iter().all(|r| r.1 <= 1500), "a file written after the first recorded second (a clock that moved, a copy) gives no time after it: {capped:?}");
    let alone = [seg(0, 100, None, Some(1000))];
    assert_eq!(estimate(&alone, Some((50, 2000))), vec![], "the segment of the first recorded line, with no start to place its old lines at: no run");
    assert_eq!(estimate(&[seg(0, 100, Some(700), Some(1000))], Some((50, 2000))), vec![(0, 700)], "its own birth is its start when nothing is before it");
}

fn files(dir: &Path, written: &[(i64, u64)]) -> impl Fn(i64) -> PathBuf {
    for &(first, at) in written {
        let path = dir.join(format!("x.{first:020}.seg"));
        let file = OpenOptions::new().create(true).write(true).truncate(true).open(&path).unwrap();
        file.set_modified(UNIX_EPOCH + Duration::from_secs(at)).unwrap();
    }
    let dir = dir.to_path_buf();
    move |first| dir.join(format!("x.{first:020}.seg"))
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lico_approx_{tag}_{}", crate::core::random_hex(6)));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_answer_reads_the_files_and_cuts_the_estimate_to_the_lines_asked_for() {
    let dir = scratch("reply");
    let list = [(0, 100), (100, 100), (200, 100)];
    let path_of = files(&dir, &[(0, 1000), (100, 2000), (200, 3000)]);
    let mut stamps = Stamps::new();
    assert!(stamps.note(250, 2500));
    let all = answer(&stamps, &list, &path_of, 0, 300);
    assert_eq!(all.exact_from, Some(250));
    assert_eq!(all.runs, vec![(250, 2500)], "the recorded runs are what they were");
    assert!(all.approx.iter().all(|r| r.0 < 250 && r.1 <= 2500), "{:?}", all.approx);
    assert!(all.approx.iter().any(|r| r.0 >= 200 && r.1 >= 2000), "the segment of the first recorded line is read too: {:?}", all.approx);
    assert!(all.approx.len() + all.runs.len() <= MAX_REPLY);
    let part = answer(&stamps, &list, &path_of, 130, 160);
    assert!(part.approx.first().is_some_and(|r| r.0 <= 130), "the run that covers the first line asked for comes first: {:?}", part.approx);
    assert!(part.approx.iter().all(|r| r.0 < 160) && part.runs.is_empty());
    let inside = time_of(&part.approx, 130).unwrap();
    assert!((1000..2000).contains(&inside), "line 130 is after the last write of the segment before its own (read though no line of it was asked for) and before its own: {inside}");
    let recorded = answer(&stamps, &list, &path_of, 260, 290);
    assert_eq!((recorded.approx.as_slice(), recorded.runs.as_slice()), (&[][..], &[(250, 2500)][..]), "lines that were recorded have no estimate");
    assert_eq!(answer(&stamps, &list, &path_of, 5, 5).approx, vec![], "an empty range");
    let mut two = Stamps::new();
    assert!(two.note(250, 2500) && two.note(280, 2600));
    assert_eq!(answer(&two, &list, &path_of, 0, 300).exact_from, Some(250), "the recorded times begin at the first run, not the last");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_log_with_no_recorded_time_is_all_estimate_and_a_file_that_is_gone_gives_nothing() {
    let dir = scratch("none");
    let list = [(0, 100), (100, 100)];
    let path_of = files(&dir, &[(0, 1000), (100, 2000)]);
    let stamps = Stamps::new();
    let got = answer(&stamps, &list, &path_of, 0, 200);
    assert_eq!((got.exact_from, got.runs.is_empty()), (None, true), "nothing was recorded: no first recorded line");
    assert_eq!(got.approx.first().map(|r| r.0), Some(0));
    assert!(time_of(&got.approx, 199).is_some_and(|t| t >= 1900));
    std::fs::remove_file(path_of(100)).unwrap();
    let missing = answer(&stamps, &list, &path_of, 100, 200);
    assert!(missing.approx.is_empty() || missing.approx.iter().all(|r| r.0 < 100), "a segment whose file is gone has no run of its own: {:?}", missing.approx);
    assert_eq!(answer(&stamps, &[], &path_of, 0, 50).approx, vec![], "no segments, no estimate");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_recorded_runs_and_the_estimate_together_stay_inside_the_bound_of_a_reply() {
    let dir = scratch("bound");
    let list = [(0, 1000)];
    let path_of = files(&dir, &[(0, 1000)]);
    let mut stamps = Stamps::new();
    for i in 0..(MAX_REPLY as i64 + 50) {
        assert!(stamps.note(1000 + i, 5000 + i as u64));
    }
    let got = answer(&stamps, &list, &path_of, 0, 1000 + MAX_REPLY as i64 + 50);
    assert_eq!(got.runs.len(), MAX_REPLY, "the recorded runs fill the reply");
    assert!(got.runs.len() + got.approx.len() <= MAX_REPLY, "the answer holds at most {MAX_REPLY} runs in all: {} + {}", got.runs.len(), got.approx.len());
    std::fs::remove_dir_all(&dir).unwrap();
}
