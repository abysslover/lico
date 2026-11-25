use super::*;
use serde_json::json;

fn reply(session: &str, from: i64, until: i64, runs: &[(i64, u64)]) -> Value {
    json!({"ok": 1, "session": session, "stamps": runs.iter().map(|r| json!([r.0, r.1])).collect::<Vec<_>>(), "from": from, "until": until})
}

fn asked(request: Option<String>) -> Option<(String, i64, i64)> {
    let value: Value = serde_json::from_str(&request?).unwrap();
    assert_eq!(value["op"], "stamps");
    Some((value["session"].as_str()?.to_string(), value["from"].as_i64()?, value["until"].as_i64()?))
}

/// A `Times` that has been told about `[from, until)` of session `s` with `runs`.
fn known(from: i64, until: i64, runs: &[(i64, u64)]) -> Times {
    let mut times = Times::default();
    assert!(asked(times.next("s", Some((from, until)), 0)).is_some());
    assert!(times.on_reply(&reply("s", from, until, runs)));
    times
}

#[test]
fn nothing_is_known_at_first_and_the_newest_lines_are_asked_for_first() {
    let mut times = Times::default();
    assert_eq!(times.at(0), None);
    assert_eq!(asked(times.next("s", Some((0, 10)), 1)), Some(("s".into(), 0, 10)));
    let mut long = Times::default();
    assert_eq!(asked(long.next("s", Some((0, 5000)), 1)), Some(("s".into(), 3000, 5000)), "at most SPAN lines at once: the host answers at most 2,048 runs");
}

#[test]
fn nothing_is_asked_for_an_empty_view() {
    let mut times = Times::default();
    assert_eq!(times.next("s", None, 0), None);
    assert_eq!(times.next("s", Some((5, 5)), 0), None);
    assert_eq!(times.next("s", Some((7, 5)), 0), None);
}

#[test]
fn one_request_at_a_time_and_a_retry_only_after_ten_ticks() {
    let mut times = Times::default();
    assert!(times.next("s", Some((0, 10)), 100).is_some());
    for tick in 101..110 {
        assert_eq!(times.next("s", Some((0, 10)), tick), None, "the answer is awaited at tick {tick}");
    }
    assert_eq!(asked(times.next("s", Some((0, 10)), 110)), Some(("s".into(), 0, 10)), "an older host never answers: ask again, the same");
    assert_eq!(times.next("s", Some((0, 10)), 111), None);
}

#[test]
fn an_answer_makes_the_lines_it_covers_known_and_nothing_else() {
    let times = known(0, 10, &[(0, 1000), (4, 1001)]);
    assert_eq!((times.at(0), times.at(3), times.at(4), times.at(9)), (Some(1000), Some(1000), Some(1001), Some(1001)));
    assert_eq!((times.at(10), times.at(-1), times.at(500)), (None, None, None), "outside what was asked for the window says nothing");
}

#[test]
fn lines_before_the_first_run_have_no_time_and_the_range_is_still_known() {
    let mut times = known(0, 10, &[(6, 1000)]);
    assert_eq!((times.at(5), times.at(6)), (None, Some(1000)));
    assert_eq!(times.next("s", Some((0, 10)), 50), None, "an answer without a time is an answer: it is not asked for again");
    let mut none = known(0, 10, &[]);
    assert_eq!(none.at(3), None);
    assert_eq!(none.next("s", Some((0, 10)), 50), None);
}

#[test]
fn new_lines_after_the_known_range_are_asked_for_and_join_it() {
    let mut times = known(0, 10, &[(0, 1000)]);
    assert_eq!(asked(times.next("s", Some((0, 15)), 1)), Some(("s".into(), 10, 15)));
    assert!(times.on_reply(&reply("s", 10, 15, &[(0, 1000), (12, 1002)])));
    assert_eq!((times.at(11), times.at(12), times.at(14), times.at(15)), (Some(1000), Some(1002), Some(1002), None));
    assert_eq!(times.next("s", Some((0, 15)), 2), None, "everything the view holds is known");
}

#[test]
fn the_span_of_one_request_is_capped_forward_and_backward() {
    let mut times = known(0, 10, &[(0, 1000)]);
    assert_eq!(asked(times.next("s", Some((0, 10_000)), 1)), Some(("s".into(), 10, 2010)));
    let mut older = known(8000, 9000, &[(8000, 1000)]);
    assert_eq!(asked(older.next("s", Some((0, 9000)), 1)), Some(("s".into(), 6000, 8000)), "older lines in pieces, the newest of them first");
    assert!(older.on_reply(&reply("s", 6000, 8000, &[(6000, 900)])));
    assert_eq!((older.at(6000), older.at(7999), older.at(8000)), (Some(900), Some(900), Some(1000)));
    assert_eq!(asked(older.next("s", Some((5500, 9000)), 2)), Some(("s".into(), 5500, 6000)), "only as far back as the view reaches");
}

#[test]
fn the_new_lines_come_before_the_old_ones() {
    let mut times = known(100, 200, &[(100, 1000)]);
    assert_eq!(asked(times.next("s", Some((50, 250)), 1)), Some(("s".into(), 200, 250)));
    assert!(times.on_reply(&reply("s", 200, 250, &[(100, 1000)])));
    assert_eq!(asked(times.next("s", Some((50, 250)), 2)), Some(("s".into(), 50, 100)));
}

#[test]
fn an_answer_that_is_not_the_awaited_one_is_ignored() {
    let mut times = Times::default();
    assert!(!times.on_reply(&reply("s", 0, 10, &[(0, 1)])), "nothing was asked");
    assert!(times.next("s", Some((0, 10)), 0).is_some());
    for wrong in [reply("t", 0, 10, &[(0, 1)]), reply("s", 1, 10, &[(0, 1)]), reply("s", 0, 11, &[(0, 1)]), json!({"ok": 1})] {
        assert!(!times.on_reply(&wrong), "{wrong}");
    }
    assert_eq!(times.at(0), None);
    assert!(times.on_reply(&reply("s", 0, 10, &[(0, 1)])), "the awaited one still comes");
    assert_eq!(times.at(0), Some(1));
    assert!(!times.on_reply(&reply("s", 0, 10, &[(0, 2)])), "an answer twice is taken once");
    assert_eq!(times.at(0), Some(1));
}

#[test]
fn another_session_starts_again_and_nothing_of_the_old_one_is_kept() {
    let mut times = known(0, 10, &[(0, 1000)]);
    assert_eq!(asked(times.next("t", Some((0, 10)), 1)), Some(("t".into(), 0, 10)));
    assert_eq!(times.at(0), None, "the runs of the other session are gone at once");
    assert!(!times.on_reply(&reply("s", 0, 10, &[(0, 1000)])), "a late answer for the old one is ignored");
    assert!(times.on_reply(&reply("t", 0, 10, &[(0, 2000)])));
    assert_eq!(times.at(0), Some(2000));
}

#[test]
fn a_view_that_does_not_touch_the_known_range_starts_again() {
    let mut times = known(0, 10, &[(0, 1000)]);
    assert_eq!(asked(times.next("s", Some((100, 110)), 1)), Some(("s".into(), 100, 110)));
    assert_eq!(times.at(5), None, "the old range is let go: the range is one piece");
    assert!(times.on_reply(&reply("s", 100, 110, &[(98, 1500)])));
    assert_eq!((times.at(100), times.at(5)), (Some(1500), None));
}

#[test]
fn garbage_in_an_answer_is_skipped() {
    let mut times = Times::default();
    assert!(times.next("s", Some((0, 10)), 0).is_some());
    let odd = json!({"ok": 1, "session": "s", "from": 0, "until": 10, "stamps": [[0, 1000], "x", [1], [-3, 5], [2, -1], null, [4, 1004, 9], [6, 1006]]});
    assert!(times.on_reply(&odd));
    assert_eq!((times.at(0), times.at(5), times.at(7)), (Some(1000), Some(1004), Some(1006)), "the good ones are taken: 0, 4 and 6");
}

#[test]
fn the_dump_says_what_is_known_for_the_tests() {
    let mut times = known(5, 20, &[(5, 1000), (9, 1004)]);
    let dump = times.json();
    assert_eq!((dump["session"].as_str(), dump["lo"].as_i64(), dump["hi"].as_i64(), dump["runs"].as_u64()), (Some("s"), Some(5), Some(20), Some(2)));
    assert_eq!((dump["first"].as_u64(), dump["newest"].as_u64(), dump["last_run"].clone()), (Some(1000), Some(1004), json!([9, 1004])));
    assert_eq!((dump["asked"].as_u64(), dump["answered"].as_u64(), dump["pending"].as_bool()), (Some(1), Some(1), Some(false)));
    assert!(times.next("s", Some((5, 30)), 3).is_some());
    assert_eq!((times.json()["asked"].as_u64(), times.json()["pending"].as_bool()), (Some(2), Some(true)));
    assert_eq!(Times::default().json()["runs"].as_u64(), Some(0));
    assert!(Times::default().json()["first"].is_null());
}

#[test]
fn a_request_awaited_for_one_session_does_not_hold_back_another() {
    let mut times = Times::default();
    assert!(times.next("s", Some((0, 10)), 100).is_some());
    assert_eq!(asked(times.next("t", Some((0, 10)), 101)), Some(("t".into(), 0, 10)), "the view moved to another session: ask for it at once, not after ten ticks");
    assert_eq!(times.next("t", Some((0, 10)), 102), None, "and that one is awaited in its turn");
}

#[test]
fn a_view_next_to_the_known_range_joins_it_and_one_with_a_gap_starts_again() {
    let mut below = known(10, 20, &[(10, 1000)]);
    assert_eq!(asked(below.next("s", Some((0, 10)), 1)), Some(("s".into(), 0, 10)), "the lines just before the range");
    assert_eq!(below.at(15), Some(1000), "what was known stays: the two pieces are one");
    assert!(below.on_reply(&reply("s", 0, 10, &[(3, 990)])));
    assert_eq!((below.at(2), below.at(3), below.at(9), below.at(15), below.json()["lo"].as_i64(), below.json()["hi"].as_i64()), (None, Some(990), Some(990), Some(1000), Some(0), Some(20)));
    let mut above = known(10, 20, &[(10, 1000)]);
    assert_eq!(asked(above.next("s", Some((20, 30)), 1)), Some(("s".into(), 20, 30)), "the lines just after the range");
    assert_eq!(above.at(15), Some(1000));
    assert!(above.on_reply(&reply("s", 20, 30, &[(10, 1000), (25, 1005)])));
    assert_eq!((above.at(15), above.at(25), above.json()["lo"].as_i64(), above.json()["hi"].as_i64()), (Some(1000), Some(1005), Some(10), Some(30)));
    let mut gap = known(10, 20, &[(10, 1000)]);
    assert_eq!(asked(gap.next("s", Some((0, 9)), 1)), Some(("s".into(), 0, 9)));
    assert_eq!(gap.at(15), None, "a gap: the old range is let go");
}

#[test]
fn a_new_session_asks_for_the_newest_lines_first_even_when_the_view_starts_at_the_first_line() {
    let mut times = Times::default();
    assert_eq!(asked(times.next("s", Some((0, 5000)), 0)), Some(("s".into(), 3000, 5000)));
    let mut again = Times::default();
    assert!(again.next("s", Some((0, 5000)), 0).is_some());
    assert_eq!(asked(again.next("s", Some((0, 5000)), 10)), Some(("s".into(), 3000, 5000)), "an unanswered first request goes out again the same, not from the oldest line");
}

#[test]
fn the_counters_go_on_when_the_view_starts_again() {
    let mut times = known(0, 10, &[(0, 1000)]);
    assert!(times.next("t", Some((0, 10)), 1).is_some());
    let dump = times.json();
    assert_eq!((dump["asked"].as_u64(), dump["answered"].as_u64()), (Some(2), Some(1)), "a restart forgets the runs and the range, not how many requests there were");
    assert!(times.on_reply(&reply("t", 0, 10, &[])));
    assert_eq!((times.json()["asked"].as_u64(), times.json()["answered"].as_u64()), (Some(2), Some(2)));
}
