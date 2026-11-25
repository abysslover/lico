use super::*;
use std::os::unix::fs::PermissionsExt;

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lico_stamps_{tag}_{}", crate::core::random_hex(6)));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn runs_of(secs: &[(i64, u64)]) -> Stamps {
    let mut stamps = Stamps::new();
    for &(seq, at) in secs {
        assert!(stamps.note(seq, at), "{seq} {at}");
    }
    stamps
}

fn file_bytes(log: &Path, id: &str) -> Vec<u8> {
    std::fs::read(dir_for(log).join(format!("{id}.times"))).unwrap_or_default()
}

fn record(seq: u64, secs: u64) -> Vec<u8> {
    let mut out = seq.to_le_bytes().to_vec();
    out.extend(secs.to_le_bytes());
    out
}

#[test]
fn a_run_begins_only_when_the_second_changes() {
    let mut stamps = Stamps::new();
    assert!(stamps.note(0, 100), "the first record begins a run");
    assert!(!stamps.note(1, 100), "the same second is the same run");
    assert!(!stamps.note(2, 100));
    assert!(stamps.note(3, 101), "a new second begins one");
    assert_eq!(stamps.len(), 2);
    assert_eq!((stamps.at(0), stamps.at(2), stamps.at(3), stamps.at(99)), (Some(100), Some(100), Some(101), Some(101)));
    assert_eq!(stamps.last(), Some((3, 101)));
}

#[test]
fn nothing_is_known_before_the_first_run_and_the_last_run_reaches_on() {
    let mut stamps = Stamps::new();
    assert_eq!((stamps.at(0), stamps.last()), (None, None), "an empty list says nothing");
    stamps.note(5, 100);
    assert_eq!((stamps.at(4), stamps.at(5), stamps.at(i64::MAX)), (None, Some(100), Some(100)));
    assert_eq!(stamps.at(-1), None);
}

#[test]
fn a_record_that_does_not_go_forward_is_ignored_and_a_clock_that_goes_back_is_a_new_run() {
    let mut stamps = Stamps::new();
    stamps.note(5, 100);
    assert!(!stamps.note(5, 200), "the same sequence again");
    assert!(!stamps.note(3, 300), "an older sequence");
    assert_eq!((stamps.len(), stamps.at(5)), (1, Some(100)));
    assert!(stamps.note(6, 90), "the clock was set back: the second differs, so a run begins");
    assert_eq!((stamps.at(5), stamps.at(6)), (Some(100), Some(90)));
}

#[test]
fn between_gives_the_run_that_covers_the_start_and_every_run_that_begins_before_the_end() {
    let stamps = runs_of(&[(0, 10), (5, 11), (9, 12), (20, 13)]);
    assert_eq!(stamps.between(6, 10, 99), [(5, 11), (9, 12)], "the run that covers 6 and the one that begins at 9");
    assert_eq!(stamps.between(0, 5, 99), [(0, 10)], "the end is not included");
    assert_eq!(stamps.between(5, 6, 99), [(5, 11)], "a run that begins at the start");
    assert_eq!(stamps.between(21, 30, 99), [(20, 13)], "the last run reaches on");
    assert_eq!(stamps.between(0, 100, 2), [(0, 10), (5, 11)], "at most the limit, the oldest first");
    assert_eq!(stamps.between(10, 10, 99), Vec::<(i64, u64)>::new(), "an empty range");
    assert_eq!(stamps.between(10, 3, 99), Vec::<(i64, u64)>::new(), "a range that runs backwards");
    assert_eq!(stamps.between(0, 100, 0), Vec::<(i64, u64)>::new(), "a limit of nothing");
    let late = runs_of(&[(5, 11)]);
    assert_eq!(late.between(0, 3, 99), Vec::<(i64, u64)>::new(), "nothing begins before 3 and nothing covers it");
    assert_eq!(late.between(0, 6, 99), [(5, 11)]);
    assert_eq!(Stamps::new().between(0, 9, 9), Vec::<(i64, u64)>::new());
}

#[test]
fn drop_before_keeps_the_run_that_covers_the_point() {
    let mut stamps = runs_of(&[(0, 10), (5, 11), (9, 12)]);
    stamps.drop_before(0);
    assert_eq!(stamps.len(), 3, "nothing is older than 0");
    stamps.drop_before(5);
    assert_eq!(stamps.between(0, 99, 99), [(5, 11), (9, 12)], "the run that begins at 5 stays and the one before it goes");
    stamps.drop_before(6);
    assert_eq!(stamps.between(0, 99, 99), [(5, 11), (9, 12)], "5 still covers 6");
    stamps.drop_before(100);
    assert_eq!(stamps.between(0, 999, 99), [(9, 12)], "the last run covers what comes after it");
    Stamps::new().drop_before(5);
}

#[test]
fn merge_inserts_replaces_and_keeps_the_order_and_a_second_merge_changes_nothing() {
    let mut stamps = runs_of(&[(5, 11)]);
    stamps.merge(&[(9, 12), (0, 10), (5, 11)]);
    assert_eq!(stamps.between(0, 99, 99), [(0, 10), (5, 11), (9, 12)]);
    let before = stamps.clone();
    stamps.merge(&[(9, 12), (0, 10)]);
    assert_eq!(stamps, before, "the same runs again");
    stamps.merge(&[(5, 99)]);
    assert_eq!(stamps.at(5), Some(99), "a run that is told again with another second is replaced");
    stamps.merge(&[]);
    assert_eq!(stamps.len(), 3);
    stamps.merge(&[(-3, 1)]);
    assert_eq!(stamps.len(), 3, "a sequence below zero is no record");
}

#[test]
fn the_runs_are_bounded_and_the_oldest_go_first() {
    let mut stamps = Stamps::new();
    for n in 0..(MAX_RUNS as i64 + 10) {
        stamps.note(n, 1000 + n as u64);
    }
    assert_eq!(stamps.len(), MAX_RUNS);
    assert_eq!(stamps.at(0), None, "the oldest runs were dropped");
    assert_eq!(stamps.at(MAX_RUNS as i64 + 9), Some(1000 + MAX_RUNS as u64 + 9));
    let mut merged = Stamps::new();
    let many: Vec<(i64, u64)> = (0..MAX_RUNS as i64 + 10).map(|n| (n, n as u64)).collect();
    merged.merge(&many);
    assert_eq!(merged.len(), MAX_RUNS, "a merge is bounded too");
    assert_eq!(merged.at(MAX_RUNS as i64 + 9), Some(MAX_RUNS as u64 + 9));
}

#[test]
fn the_file_is_beside_the_log_folder_owner_only_and_read_back_by_the_next_book() {
    let root = scratch("book");
    let log = root.join("log");
    assert_eq!(dir_for(&log), root.join("log.times"));
    let mut book = Book::new(&log, "s1");
    book.note(0, 100);
    book.note(1, 100);
    book.note(2, 101);
    assert!(!log.exists(), "the log folder holds nothing new: a build that does not know the file still starts");
    let bytes = file_bytes(&log, "s1");
    assert_eq!(bytes, [record(0, 100), record(2, 101)].concat(), "one record per run, little-endian");
    let file = dir_for(&log).join("s1.times");
    assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
    assert_eq!(std::fs::metadata(dir_for(&log)).unwrap().permissions().mode() & 0o777, 0o700);
    let again = Book::new(&log, "s1");
    assert_eq!(again.stamps(), book.stamps());
    assert_eq!(again.stamps().at(1), Some(100));
    let other = Book::new(&log, "s2");
    assert_eq!(other.stamps().len(), 0, "another session has its own file");
}

#[test]
fn a_torn_tail_and_a_record_that_goes_back_are_ignored_and_the_file_is_repaired_before_the_next_record() {
    let root = scratch("torn");
    let log = root.join("log");
    std::fs::create_dir_all(dir_for(&log)).unwrap();
    let mut bytes = [record(0, 100), record(4, 101), record(2, 500), record(4, 600), record(7, 102)].concat();
    bytes.extend([1, 2, 3, 4, 5]);
    std::fs::write(dir_for(&log).join("s1.times"), &bytes).unwrap();
    let mut book = Book::new(&log, "s1");
    assert_eq!(book.stamps().between(0, 99, 99), [(0, 100), (4, 101), (7, 102)], "the two records that did not go forward are skipped");
    book.note(10, 103);
    let after = file_bytes(&log, "s1");
    assert_eq!(after.len() % 16, 0, "the stray bytes would have shifted every later record");
    assert_eq!(Book::new(&log, "s1").stamps().between(0, 99, 99), [(0, 100), (4, 101), (7, 102), (10, 103)]);
    let huge = [record(u64::MAX, 5), record(1, 9)].concat();
    std::fs::write(dir_for(&log).join("s3.times"), &huge).unwrap();
    assert_eq!(Book::new(&log, "s3").stamps().between(0, 99, 99), [(1, 9)], "a sequence that is no i64 is no record");
    assert_eq!(file_bytes(&log, "s3"), record(1, 9), "and the file is repaired without it");
}

#[test]
fn a_missing_or_unreadable_file_or_folder_means_no_times_and_never_stops_the_runs() {
    let root = scratch("broken");
    let log = root.join("log");
    let mut book = Book::new(&log, "s1");
    assert_eq!(book.stamps().at(0), None, "no file: no time");
    std::fs::write(dir_for(&log), "not a folder").unwrap();
    let mut blocked = Book::new(&log, "s2");
    blocked.note(0, 100);
    blocked.note(1, 101);
    assert_eq!(blocked.stamps().at(1), Some(101), "the folder cannot be made: the runs stay in memory");
    std::fs::remove_file(dir_for(&log)).unwrap();
    std::fs::create_dir_all(dir_for(&log).join("s4.times")).unwrap();
    let mut dir_in_the_way = Book::new(&log, "s4");
    dir_in_the_way.note(0, 100);
    assert_eq!(dir_in_the_way.stamps().at(0), Some(100), "the path is a folder: the runs stay in memory");
    book.note(0, 5);
    assert_eq!(file_bytes(&log, "s1"), record(0, 5));
}

#[test]
fn an_id_that_could_leave_the_folder_is_kept_in_memory_only() {
    let root = scratch("ids");
    let log = root.join("log");
    for id in ["../x", "a/b", "", ".hidden", "a\0b"] {
        let mut book = Book::new(&log, id);
        book.note(0, 100);
        assert_eq!(book.stamps().at(0), Some(100), "{id:?}");
    }
    assert!(!dir_for(&log).exists(), "no file and no folder were made for them");
    assert!(!root.join("x").exists() && !root.join("x.times").exists());
}

#[test]
fn drop_before_trims_and_the_file_is_rewritten_when_it_is_twice_what_is_kept() {
    let root = scratch("trim");
    let log = root.join("log");
    let mut book = Book::new(&log, "s1");
    for n in 0..1000 {
        book.note(n, 1000 + n as u64);
    }
    assert_eq!(file_bytes(&log, "s1").len(), 1000 * 16);
    book.drop_before(980);
    assert_eq!(book.stamps().len(), 20, "980 stays (it covers itself) and 981 to 999 follow");
    assert_eq!(file_bytes(&log, "s1").len(), 20 * 16, "the file holds what is kept and no more");
    assert_eq!(Book::new(&log, "s1").stamps(), book.stamps());
    book.drop_before(985);
    assert_eq!((book.stamps().len(), file_bytes(&log, "s1").len()), (15, 20 * 16), "after a rewrite the file holds 20 and 15 are kept: not twice, so it stays (the count of what the file holds is the new one)");
    book.note(1000, 5000);
    assert_eq!(Book::new(&log, "s1").stamps().at(1000), Some(5000), "the file still takes new records after a rewrite");
    let small_log = scratch("small").join("log");
    let mut small = Book::new(&small_log, "s1");
    for n in 0..10 {
        small.note(n, 1000 + n as u64);
    }
    small.drop_before(8);
    assert_eq!(small.stamps().len(), 2, "8 stays and 9 follows");
    assert_eq!(file_bytes(&small_log, "s1").len(), 10 * 16, "a small file is not rewritten for a few dropped runs");
    assert_eq!(small.stamps().at(7), None, "what the log dropped is no longer known");
    assert_eq!(small.stamps().at(8), Some(1008));
}

#[test]
fn purge_removes_the_file_and_nothing_is_written_again() {
    let root = scratch("purge");
    let log = root.join("log");
    let mut book = Book::new(&log, "s1");
    book.note(0, 100);
    let mut other = Book::new(&log, "s2");
    other.note(0, 100);
    std::fs::write(dir_for(&log).join("s1.times.tmp"), b"half").unwrap();
    book.purge();
    assert!(!dir_for(&log).join("s1.times.tmp").exists(), "a work file left by a crash goes too");
    assert!(!dir_for(&log).join("s1.times").exists());
    assert!(dir_for(&log).join("s2.times").exists(), "only its own file");
    assert_eq!(book.stamps().len(), 0);
    book.note(1, 101);
    assert!(!dir_for(&log).join("s1.times").exists(), "a purged session writes nothing again");
}

#[test]
fn sweep_removes_the_files_of_sessions_that_are_not_registered_and_nothing_else() {
    let root = scratch("sweep");
    let log = root.join("log");
    sweep(&log, &HashSet::new());
    let mut a = Book::new(&log, "sa");
    a.note(0, 1);
    let mut b = Book::new(&log, "sb");
    b.note(0, 1);
    std::fs::write(dir_for(&log).join("notes.txt"), "mine").unwrap();
    std::fs::write(dir_for(&log).join("sb.times.tmp"), "half").unwrap();
    std::fs::write(dir_for(&log).join("sa.times.tmp"), "half").unwrap();
    let keep: HashSet<String> = ["sa".to_string()].into();
    sweep(&log, &keep);
    assert!(!dir_for(&log).join("sb.times.tmp").exists(), "the work file of a session nobody knows goes with it");
    assert!(dir_for(&log).join("sa.times.tmp").exists(), "and one of a registered session is left to its own book");
    assert!(dir_for(&log).join("sa.times").exists());
    assert!(!dir_for(&log).join("sb.times").exists());
    assert!(dir_for(&log).join("notes.txt").exists(), "a file that is not a `.times` file is not touched");
}

#[test]
fn between_with_a_limit_of_zero_gives_nothing() {
    let stamps = runs_of(&[(0, 100), (5, 101)]);
    assert!(stamps.between(0, 99, 0).is_empty());
    assert_eq!(stamps.between(0, 99, 1), [(0, 100)]);
}

#[test]
fn drop_from_forgets_the_runs_that_begin_at_or_after_the_point() {
    let mut stamps = runs_of(&[(0, 100), (5, 101), (9, 102)]);
    assert!(!stamps.drop_from(10), "nothing begins at or after 10");
    assert_eq!(stamps.len(), 3);
    assert!(stamps.drop_from(9), "the run that begins at 9 goes");
    assert_eq!((stamps.len(), stamps.last()), (2, Some((5, 101))));
    assert!(stamps.drop_from(3));
    assert_eq!((stamps.len(), stamps.last()), (1, Some((0, 100))));
    assert!(stamps.drop_from(0), "every run begins at or after 0");
    assert_eq!(stamps.len(), 0);
    assert!(!stamps.drop_from(0), "an empty list has nothing to drop");
}

#[test]
fn a_book_drops_the_runs_of_records_the_log_lost_and_takes_the_same_numbers_again() {
    let root = scratch("lost");
    let log = root.join("log");
    let mut book = Book::new(&log, "s1");
    for n in 0..5 {
        book.note(n * 10, 100 + n as u64);
    }
    assert_eq!(file_bytes(&log, "s1").len(), 5 * 16);
    book.drop_from(25);
    assert_eq!(book.stamps().len(), 3, "the runs at 30 and 40 are for records the log does not hold");
    assert_eq!(file_bytes(&log, "s1").len(), 3 * 16, "the file does not keep them either");
    book.note(25, 500);
    assert_eq!(book.stamps().at(25), Some(500), "the next record takes its own time");
    assert_eq!(Book::new(&log, "s1").stamps(), book.stamps());
    book.drop_from(1000);
    assert_eq!(file_bytes(&log, "s1").len(), 4 * 16, "nothing to drop leaves the file as it is");
}

#[test]
fn the_file_is_rewritten_only_when_it_holds_more_than_twice_what_is_kept_and_a_little_more() {
    let root = scratch("edge");
    let log = root.join("log");
    let mut book = Book::new(&log, "s1");
    for n in 0..656 {
        book.note(n, 1000 + n as u64);
    }
    book.drop_before(456);
    assert_eq!(book.stamps().len(), 200);
    assert_eq!(file_bytes(&log, "s1").len(), 656 * 16, "656 is exactly twice 200 and 256 over: not more, so the file stays");
    book.note(656, 5000);
    book.drop_before(457);
    assert_eq!(book.stamps().len(), 200);
    assert_eq!(file_bytes(&log, "s1").len(), 200 * 16, "657 is more: the file is rewritten");
    assert_eq!(Book::new(&log, "s1").stamps(), book.stamps());
}

#[test]
fn a_link_in_the_place_of_the_file_is_never_followed_and_a_stale_work_file_does_not_stay_in_the_new_one() {
    let root = scratch("link");
    let log = root.join("log");
    let target = root.join("target");
    std::fs::write(&target, record(0, 7)).unwrap();
    std::fs::create_dir_all(dir_for(&log)).unwrap();
    std::os::unix::fs::symlink(&target, dir_for(&log).join("s1.times")).unwrap();
    let mut book = Book::new(&log, "s1");
    assert_eq!(book.stamps().len(), 0, "a link is not read");
    book.note(5, 100);
    assert_eq!(book.stamps().at(5), Some(100), "the time stays in memory");
    assert_eq!(std::fs::read(&target).unwrap(), record(0, 7), "and nothing is written through the link");

    let other = scratch("stale").join("log");
    std::fs::create_dir_all(dir_for(&other)).unwrap();
    std::fs::write(dir_for(&other).join("s1.times"), [record(0, 1), record(3, 2), vec![9, 9, 9]].concat()).unwrap();
    std::fs::write(dir_for(&other).join("s1.times.tmp"), vec![0xAB; 4000]).unwrap();
    let book = Book::new(&other, "s1");
    assert_eq!(book.stamps().len(), 2);
    assert_eq!(file_bytes(&other, "s1"), [record(0, 1), record(3, 2)].concat(), "the repaired file holds the runs and nothing of the stale work file");
    assert!(!dir_for(&other).join("s1.times.tmp").exists());
}

#[test]
fn a_record_that_is_not_a_number_of_a_record_is_dropped_when_read() {
    let root = scratch("huge");
    let log = root.join("log");
    std::fs::create_dir_all(dir_for(&log)).unwrap();
    std::fs::write(dir_for(&log).join("s1.times"), [record(0, 1), record(u64::MAX, 2), record(4, 3)].concat()).unwrap();
    let book = Book::new(&log, "s1");
    assert_eq!(book.stamps(), &runs_of(&[(0, 1), (4, 3)]));
}

#[test]
fn a_failed_write_says_so_once_and_the_runs_go_on_in_memory() {
    let root = scratch("fail");
    let log = root.join("log");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(dir_for(&log), b"in the way").unwrap();
    let mut book = Book::new(&log, "s1");
    book.note(0, 100);
    book.note(1, 101);
    book.note(2, 102);
    assert_eq!((book.stamps().len(), book.stamps().at(2)), (3, Some(102)));
    assert!(std::fs::read(dir_for(&log)).unwrap() == b"in the way", "the file in the way is left alone");
    std::fs::remove_file(dir_for(&log)).unwrap();
    book.note(3, 103);
    assert!(!dir_for(&log).exists(), "a file that could not be opened is let go for good: the next run does not try again");
    assert_eq!(book.stamps().at(3), Some(103));
}

#[test]
fn a_file_with_more_runs_than_are_held_gives_the_newest_and_is_rewritten() {
    let root = scratch("big");
    let log = root.join("log");
    std::fs::create_dir_all(dir_for(&log)).unwrap();
    let extra = 10usize;
    let bytes: Vec<u8> = (0..MAX_RUNS + extra).flat_map(|n| record(n as u64, 1000 + n as u64)).collect();
    std::fs::write(dir_for(&log).join("s1.times"), &bytes).unwrap();
    let book = Book::new(&log, "s1");
    assert_eq!(book.stamps().len(), MAX_RUNS);
    assert_eq!(book.stamps().at(extra as i64 - 1), None, "the oldest ten are not held");
    assert_eq!(book.stamps().at(extra as i64), Some(1000 + extra as u64));
    assert_eq!(book.stamps().last(), Some(((MAX_RUNS + extra - 1) as i64, (1000 + MAX_RUNS + extra - 1) as u64)));
    assert_eq!(file_bytes(&log, "s1").len(), MAX_RUNS * 16, "the file was rewritten without them");
    let mut torn = bytes;
    torn.extend([1, 2, 3]);
    std::fs::write(dir_for(&log).join("s1.times"), &torn).unwrap();
    assert_eq!(Book::new(&log, "s1").stamps().len(), MAX_RUNS, "a torn tail on a big file as well");
}

#[test]
fn an_id_is_safe_when_it_is_letters_digits_dash_and_underscore_and_not_long() {
    let root = scratch("safe");
    let log = root.join("log");
    for id in ["a-b_c", "S6B12", "7"] {
        let mut book = Book::new(&log, id);
        book.note(0, 100);
        assert!(dir_for(&log).join(format!("{id}.times")).exists(), "{id}");
    }
    let long = "x".repeat(129);
    let mut book = Book::new(&log, &long);
    book.note(0, 100);
    assert!(!dir_for(&log).join(format!("{long}.times")).exists(), "an id of 129 characters stays in memory");
    assert_eq!(book.stamps().at(0), Some(100));
    let fine = "y".repeat(128);
    Book::new(&log, &fine).note(0, 100);
    assert!(dir_for(&log).join(format!("{fine}.times")).exists(), "128 is the longest");
}

#[test]
fn a_negative_sequence_is_no_record() {
    let mut stamps = Stamps::new();
    assert!(!stamps.note(-1, 100));
    assert_eq!(stamps.len(), 0);
    assert!(stamps.note(0, 100));
}

#[test]
fn a_link_or_a_stale_file_at_the_work_file_is_taken_away_and_never_written_through() {
    let root = scratch("worklink");
    let log = root.join("log");
    let target = root.join("target");
    std::fs::write(&target, b"mine").unwrap();
    std::fs::create_dir_all(dir_for(&log)).unwrap();
    std::fs::write(dir_for(&log).join("s1.times"), [record(0, 1), record(3, 2), vec![9, 9]].concat()).unwrap();
    std::os::unix::fs::symlink(&target, dir_for(&log).join("s1.times.tmp")).unwrap();
    let book = Book::new(&log, "s1");
    assert_eq!(book.stamps().len(), 2);
    assert_eq!(std::fs::read(&target).unwrap(), b"mine", "nothing is written through a link at the work file");
    assert_eq!(file_bytes(&log, "s1"), [record(0, 1), record(3, 2)].concat(), "the repair went through all the same");
    assert!(!dir_for(&log).join("s1.times.tmp").exists());
    assert_eq!(std::fs::metadata(dir_for(&log).join("s1.times")).unwrap().permissions().mode() & 0o777, 0o600, "the new file is the owner's alone");
}

#[test]
fn a_rewrite_that_cannot_finish_leaves_no_work_file_and_the_file_is_let_go_for_good() {
    let root = scratch("rewritefail");
    let log = root.join("log");
    let mut book = Book::new(&log, "s1");
    for n in 0..5 {
        book.note(n * 10, 100 + n as u64);
    }
    let file = dir_for(&log).join("s1.times");
    std::fs::remove_file(&file).unwrap();
    std::fs::create_dir_all(file.join("in the way")).unwrap(); // a folder that is not empty where the file belongs: the work file cannot be put in its place
    book.drop_from(25);
    assert_eq!(book.stamps().len(), 3, "the runs are cut in memory all the same");
    assert!(!dir_for(&log).join("s1.times.tmp").exists(), "the work file of the failed rewrite is not left behind");
    std::fs::remove_dir_all(&file).unwrap();
    book.note(25, 500);
    book.note(26, 501);
    assert!(!file.exists(), "a file that failed once is let go: nothing is written to it again, the runs go on in memory");
    assert_eq!(book.stamps().at(26), Some(501));
}
