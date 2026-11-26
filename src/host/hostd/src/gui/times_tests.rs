use super::*;
use serde_json::json;

fn reply(session: &str, from: i64, until: i64, runs: &[(i64, u64)]) -> Value {
    json!({"ok": 1, "session": session, "stamps": runs.iter().map(|r| json!([r.0, r.1])).collect::<Vec<_>>(), "from": from, "until": until})
}

fn reply_with(session: &str, from: i64, until: i64, runs: &[(i64, u64)], approx: &[(i64, u64)], exact_from: Value) -> Value {
    let mut value = reply(session, from, until, runs);
    value["approx"] = json!(approx.iter().map(|r| json!([r.0, r.1])).collect::<Vec<_>>());
    value["exact_from"] = exact_from;
    value
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

#[test]
fn a_new_session_keeps_the_form_and_the_gutter_the_last_frame_drew_until_the_next_frame() {
    let mut times = known(0, 10, &[(0, 5)]);
    times.gutter = 100;
    times.form = Some(crate::gui::blocks::Form::Clock);
    assert!(times.next("other", Some((0, 10)), 50).is_some(), "another session starts the range again");
    let json = times.json();
    assert_eq!((json["gutter"].as_u64(), json["form"].as_str()), (Some(100), Some("clock")), "the dump says what the last frame drew, not a blank, until the next frame draws again");
}

#[test]
fn the_lines_below_the_first_recorded_one_carry_an_estimated_time_and_say_so() {
    let mut times = Times::default();
    assert!(asked(times.next("s", Some((0, 20)), 0)).is_some());
    assert!(times.on_reply(&reply_with("s", 0, 20, &[(10, 1500)], &[(0, 1000), (5, 1200)], json!(10))));
    assert_eq!((times.marked(2), times.marked(7), times.marked(9), times.marked(10), times.marked(19)), (Some((1000, true)), Some((1200, true)), Some((1200, true)), Some((1500, false)), Some((1500, false))));
    assert_eq!(times.json()["approx_below"], 10, "the dump says where the recorded times begin");
    assert_eq!((times.marked(20), times.marked(-1)), (None, None), "outside what was asked for the window says nothing");
}

#[test]
fn a_recorded_run_replaces_an_estimate_of_the_same_line() {
    let mut times = Times::default();
    assert!(asked(times.next("s", Some((0, 20)), 0)).is_some());
    assert!(times.on_reply(&reply_with("s", 0, 20, &[(10, 2000)], &[(10, 1000)], json!(10))));
    assert_eq!(times.marked(10), Some((2000, false)), "a line named in both lists has the time that was recorded");
}

#[test]
fn a_host_that_says_nothing_of_estimates_marks_none_and_one_with_nothing_recorded_marks_every_line() {
    let old = known(0, 10, &[(0, 1000)]);
    assert_eq!((old.marked(3), old.json()["approx_below"].clone()), (Some((1000, false)), Value::Null), "an older host: the reply has no new field, every time is as it was");
    let mut none = Times::default();
    assert!(asked(none.next("s", Some((0, 10)), 0)).is_some());
    assert!(none.on_reply(&reply_with("s", 0, 10, &[], &[(0, 1000)], Value::Null)));
    assert_eq!((none.marked(4), none.json()["approx_below"].as_i64()), (Some((1000, true)), Some(i64::MAX)), "no run was recorded: every time is an estimate");
    let mut odd = Times::default();
    assert!(asked(odd.next("s", Some((0, 10)), 0)).is_some());
    assert!(odd.on_reply(&reply_with("s", 0, 10, &[(0, 1000)], &[(0, 900)], json!("soon"))));
    assert_eq!(odd.json()["approx_below"], Value::Null, "a number that is not one is not believed");
}

#[test]
fn what_the_window_knows_of_the_estimate_is_let_go_with_the_session_and_follows_the_newest_answer() {
    let mut times = Times::default();
    assert!(asked(times.next("s", Some((0, 10)), 0)).is_some());
    assert!(times.on_reply(&reply_with("s", 0, 10, &[], &[(0, 1000)], Value::Null)));
    assert!(asked(times.next("s", Some((0, 20)), 20)).is_some());
    assert!(times.on_reply(&reply_with("s", 10, 20, &[(12, 2000)], &[], json!(12))));
    assert_eq!((times.marked(5), times.marked(15)), (Some((1000, true)), Some((2000, false))), "a line is an estimate when it lies below the newest answer's first recorded line");
    assert!(asked(times.next("t", Some((0, 10)), 40)).is_some());
    assert_eq!(times.json()["approx_below"], Value::Null, "another session starts again");
}

mod drawing {
    use super::super::*;
    use crate::gui::transcript::{ROLE_BOX, ROLE_BOX_TOP, ROLE_USER_BG, STYLE_AGENT, STYLE_CMD, STYLE_CMD_OUT, STYLE_USER};
    use crate::gui::BACKGROUND;

    const REPLY: u16 = STYLE_AGENT;
    const USER: u16 = STYLE_USER | ROLE_USER_BG;
    const CMD_TOP: u16 = STYLE_CMD | ROLE_BOX | ROLE_BOX_TOP;
    const CMD_MID: u16 = STYLE_CMD_OUT | ROLE_BOX;

    fn lines(roles: &[u16]) -> Vec<(f32, f32, usize, u16)> {
        roles.iter().enumerate().map(|(i, &role)| (i as f32 * 20.0, i as f32 * 20.0 + 20.0, i, role)).collect()
    }

    fn rows_of(roles: &[u16], known: impl Fn(i64) -> bool) -> Vec<(f32, String)> {
        let at = |seq: i64| known(seq).then_some((seq as u64 + 1000, false));
        stamp_rows(&lines(roles), &|raw| Some(raw as i64 * 10), &at, &|secs| format!("t{secs}")).into_iter().map(|(top, _, text)| (top, text)).collect()
    }

    #[test]
    fn the_first_line_of_every_block_and_of_the_screen_carries_its_time() {
        let roles = [REPLY, REPLY, USER, USER, CMD_TOP, CMD_MID, REPLY];
        assert_eq!(
            rows_of(&roles, |_| true),
            [(0.0, "t1000".to_string()), (40.0, "t1020".to_string()), (80.0, "t1040".to_string()), (120.0, "t1060".to_string())],
            "a reply, the person, a command with its output, a reply"
        );
        assert_eq!(rows_of(&[CMD_MID, CMD_MID, REPLY], |_| true), [(0.0, "t1000".to_string()), (40.0, "t1020".to_string())], "the first line on the screen counts even in the middle of a block");
        assert_eq!(rows_of(&[], |_| true), []);
    }

    #[test]
    fn a_line_with_no_known_time_shows_none_and_the_others_still_do() {
        let roles = [REPLY, USER, REPLY];
        assert_eq!(rows_of(&roles, |seq| seq != 10), [(0.0, "t1000".to_string()), (40.0, "t1020".to_string())]);
        assert_eq!(rows_of(&roles, |_| false), []);
        let none = stamp_rows(&lines(&roles), &|_| None, &|_| Some((1, false)), &|_| "x".to_string());
        assert!(none.is_empty(), "a line that has no sequence number has no time");
    }

    #[test]
    fn the_rows_keep_the_top_and_the_bottom_of_the_line() {
        let rows = stamp_rows(&[(10.5, 31.25, 0, REPLY)], &|_| Some(0), &|_| Some((7, false)), &|s| s.to_string());
        assert_eq!(rows, [(10.5, 31.25, "7".to_string())]);
    }

    #[test]
    fn an_estimate_is_written_with_a_tilde_before_its_time_and_a_recorded_time_without() {
        let at = |seq: i64| Some((seq as u64 + 1000, seq < 20));
        let rows = stamp_rows(&lines(&[REPLY, USER, REPLY]), &|raw| Some(raw as i64 * 10), &at, &|secs| format!("t{secs}"));
        assert_eq!(rows.iter().map(|r| r.2.as_str()).collect::<Vec<_>>(), ["~t1000", "~t1010", "t1020"], "the lines 0 and 10 are below 20, the line 20 is not");
    }

    const W: usize = 300;
    const H: usize = 100;
    const MARK: u32 = 0xFF0A_0B0C;

    #[test]
    fn the_gutter_of_every_line_on_the_screen_is_cleared_and_nothing_else() {
        let mut buf = vec![MARK; W * H];
        gutter_clear(&mut buf, W, H, 60, 0, 80, &lines(&[REPLY, USER]));
        let px = |x: usize, y: usize| buf[y * W + x];
        assert_eq!((px(239, 5), px(240, 5), px(W - 1, 5), px(W - 1, 39)), (MARK, BACKGROUND, BACKGROUND, BACKGROUND), "from the gutter to the edge, both lines");
        assert_eq!((px(W - 1, 40), px(240, 99)), (MARK, MARK), "rows that are no line of the screen are not touched");
        let mut none = vec![MARK; W * H];
        gutter_clear(&mut none, W, H, 0, 0, 80, &lines(&[REPLY]));
        assert!(none.iter().all(|&p| p == MARK), "no gutter, nothing to clear");
    }

    #[test]
    fn the_clearing_stays_inside_the_text_area() {
        let mut buf = vec![MARK; W * H];
        gutter_clear(&mut buf, W, H, 60, 20, 50, &lines(&[REPLY, USER, REPLY, USER]));
        let px = |y: usize| buf[y * W + W - 1];
        assert_eq!((px(19), px(20), px(49), px(50)), (MARK, BACKGROUND, BACKGROUND, MARK), "rows above the top of the area and from the hint line on are not touched");
        let mut small = vec![MARK; 40 * 30];
        gutter_clear(&mut small, 40, 30, 500, 0, 30, &lines(&[REPLY, USER]));
        assert!(small.iter().all(|&p| p == BACKGROUND || p == MARK), "a gutter wider than the window does not go outside it");
        assert_eq!(small[39], BACKGROUND);
    }

    fn drawn_at(scale: f32, rows: &[(f32, f32, String)], top: i32, hint: i32) -> Vec<u32> {
        let mut fs = crate::gui::fonts::system(false);
        let mut swash = cosmic_text::SwashCache::new();
        let mut controls = crate::gui::controls::Controls::default();
        controls.begin(scale, 14.0);
        let mut buf = vec![BACKGROUND; W * H];
        gutter_draw(&mut controls, &mut fs, &mut swash, &mut buf, W, H, 60, 8.5 * scale, top, hint, rows);
        buf
    }

    fn drawn(rows: &[(f32, f32, String)], top: i32, hint: i32) -> Vec<u32> {
        drawn_at(1.0, rows, top, hint)
    }

    fn lit(buf: &[u32], x0: usize, x1: usize, y0: usize, y1: usize) -> usize {
        (y0..y1).map(|y| buf[y * W + x0..y * W + x1].iter().filter(|&&p| p != BACKGROUND).count()).sum()
    }

    #[test]
    fn the_tilde_stands_in_the_free_column_before_the_time_and_the_time_does_not_move() {
        let plain = drawn(&[(20.0, 38.0, "14:03:07".to_string())], 0, 100);
        let marked = drawn(&[(20.0, 38.0, "~14:03:07".to_string())], 0, 100);
        assert!(lit(&marked, 240, 248, 20, 38) > 3, "the tilde is in the column that the gutter keeps free at its left");
        assert_eq!(lit(&plain, 240, 248, 20, 38), 0, "that column is empty without it");
        let same = (20..38).all(|y| (252..W).all(|x| plain[y * W + x] == marked[y * W + x]));
        assert!(same, "the digits are where they were: the mark takes no room from the time");
    }

    #[test]
    fn a_time_is_drawn_inside_the_gutter_at_the_row_of_its_line() {
        let buf = drawn(&[(20.0, 38.0, "2026-11-25 14:03:07".to_string())], 0, 100);
        assert!(lit(&buf, 240, W, 20, 38) > 20, "the digits are in the gutter, in the rows of the line");
        assert_eq!(lit(&buf, 0, 240, 0, H), 0, "none of it is left of the gutter");
        assert_eq!(lit(&buf, 240, W, 0, 20) + lit(&buf, 240, W, 38, H), 0, "and none above or below the line");
        assert_eq!(lit(&buf, 240, 248, 20, 38), 0, "it starts a column in (8.5 pixels)");
    }

    #[test]
    fn a_time_is_drawn_exactly_as_a_label_of_that_text_would_be_at_the_start_of_its_row() {
        let (text, ink) = ("2026-11-25 14:03:07", cosmic_text::Color::rgb(0x7A, 0x84, 0x96));
        for scale in [1.0f32, 2.0] {
            let mut fs = crate::gui::fonts::system(false);
            let mut swash = cosmic_text::SwashCache::new();
            let mut controls = crate::gui::controls::Controls::default();
            controls.begin(scale, 14.0);
            let mut expected = vec![BACKGROUND; W * H];
            controls.text(&mut fs, &mut swash, &mut expected, W, H, 240.0 + 8.5 * scale, 20.0, W as i32, text, ink);
            assert!(expected.iter().any(|&p| p != BACKGROUND));
            assert!(drawn_at(scale, &[(20.0, 20.0 + 18.0 * scale, text.to_string())], 0, 100) == expected, "at scale {scale}: the same pixels, a column in and at the row of its line");
        }
    }

    #[test]
    fn the_height_of_a_label_follows_the_scale_when_the_area_is_judged() {
        let row = [(20.0, 56.0, "2026-11-25 14:03:07".to_string())];
        assert!(lit(&drawn_at(2.0, &row, 0, 56), 240, W, 0, H) > 0, "a label of 36 pixels at scale 2 fits in rows 20 to 56");
        assert_eq!(lit(&drawn_at(2.0, &row, 0, 55), 240, W, 0, H), 0, "and not when the area ends a row sooner");
    }

    #[test]
    fn a_time_whose_line_is_not_wholly_in_the_text_area_is_not_drawn() {
        let row = [(20.0, 38.0, "2026-11-25 14:03:07".to_string())];
        assert_eq!(lit(&drawn(&row, 21, 100), 240, W, 0, H), 0, "its top is above the area");
        assert_eq!(lit(&drawn(&row, 0, 37), 240, W, 0, H), 0, "its bottom is under the area");
        assert!(lit(&drawn(&row, 20, 38), 240, W, 0, H) > 0, "a line that fills the area exactly is drawn");
        assert_eq!(lit(&drawn(&[], 0, 100), 0, W, 0, H), 0);
    }
}

mod room {
    use super::super::*;
    use crate::gui::panel::Step;
    use crate::gui::scroll_oracle::app;

    #[test]
    fn a_line_that_starts_or_ends_inside_a_row_clears_that_row_too() {
        let mut buf = vec![7u32; 100 * 60];
        gutter_clear(&mut buf, 100, 60, 20, 0, 60, &[(10.5, 31.25, 0, 0)]);
        let rows: Vec<usize> = (0..60).filter(|y| buf[y * 100 + 99] == crate::gui::BACKGROUND).collect();
        assert_eq!((rows.first(), rows.last(), rows.len()), (Some(&10), Some(&31), 22), "rows 10 to 31, the ones the line touches");
        assert_eq!(buf[10 * 100 + 79], 7, "left of the gutter nothing is touched");
    }

    #[test]
    fn a_hint_row_below_the_window_does_not_take_the_clearing_out_of_it() {
        let mut buf = vec![7u32; 100 * 60];
        gutter_clear(&mut buf, 100, 60, 20, 0, 500, &[(40.0, 90.0, 0, 0)]);
        let rows: Vec<usize> = (0..60).filter(|y| buf[y * 100 + 99] == crate::gui::BACKGROUND).collect();
        assert_eq!((rows.first(), rows.last()), (Some(&40), Some(&59)), "to the last row of the window and no further");
    }

    #[test]
    fn a_time_below_the_first_recorded_line_is_painted_with_a_tilde_in_the_free_column_and_the_dump_says_so() {
        let mut times = Times::seeded("s", 0, 10, &[(0, 1_795_000_000)]).below(1);
        let mut fs = crate::gui::fonts::system(false);
        let mut swash = cosmic_text::SwashCache::new();
        let mut controls = crate::gui::controls::Controls::default();
        controls.begin(1.0, 14.0);
        let mut buf = vec![7u32; 400 * 100];
        let hit = [(0.0, 18.0, 0usize, 0u16), (18.0, 36.0, 1, crate::gui::transcript::ROLE_USER_BG | crate::gui::transcript::STYLE_USER)];
        times.paint(&mut controls, &mut fs, &mut swash, &mut buf, (400, 100), 200, 8.5, (0, 100), &hit, &|raw| Some(raw as i64));
        let json = times.json();
        let texts: Vec<&str> = json["drawn"].as_array().unwrap().iter().map(|r| r[2].as_str().unwrap()).collect();
        assert_eq!(texts.len(), 2);
        assert!(texts[0].starts_with("~20") && !texts[1].starts_with('~'), "the line 0 is below the line 1 that is recorded: {texts:?}");
        assert_eq!(texts[0][1..], *texts[1], "the same second, the same text: the tilde is all that differs");
        let lit = |y0: usize, y1: usize| (y0..y1).map(|y| buf[y * 400 + 200..y * 400 + 207].iter().filter(|&&p| p != 7).count()).sum::<usize>();
        assert!(lit(0, 18) > 3 && lit(18, 36) == 0, "ink in the free column for the estimate only");
    }

    #[test]
    fn nothing_is_drawn_and_nothing_remembered_when_there_is_no_gutter() {
        let mut times = Times::seeded("s", 0, 10, &[(0, 1_795_000_000)]);
        let mut fs = crate::gui::fonts::system(false);
        let mut swash = cosmic_text::SwashCache::new();
        let mut controls = crate::gui::controls::Controls::default();
        controls.begin(1.0, 14.0);
        let mut buf = vec![7u32; 400 * 100];
        let hit = [(0.0, 18.0, 0usize, 0u16), (18.0, 36.0, 1, crate::gui::transcript::ROLE_USER_BG | crate::gui::transcript::STYLE_USER)];
        times.paint(&mut controls, &mut fs, &mut swash, &mut buf, (400, 100), 0, 8.5, (0, 100), &hit, &|raw| Some(raw as i64));
        assert!(buf.iter().all(|&p| p == 7), "no gutter, no pixel touched");
        assert_eq!(times.json()["drawn"].as_array().map(Vec::len), Some(0));
        times.paint(&mut controls, &mut fs, &mut swash, &mut buf, (400, 100), 90, 8.5, (0, 100), &hit, &|raw| Some(raw as i64));
        let json = times.json();
        assert_eq!((json["gutter"].as_u64(), json["drawn"].as_array().map(Vec::len)), (Some(90), Some(2)), "with a gutter the first line and the person's line each carry a time");
        assert!(buf.iter().any(|&p| p != 7));
    }

    #[test]
    fn the_padding_counts_when_the_columns_are_counted() {
        let app = app(1.0, false);
        assert_eq!((app.times_gutter(602, 1.0).0 > 0, app.times_gutter(603, 1.0).0 > 0), (false, true), "70 columns of 8.5 pixels and the 8 of padding: 602 pixels are 69 columns, 603 are 70, the room the shortest form needs");
        assert_eq!((app.times_gutter(1205, 2.0).0 > 0, app.times_gutter(1206, 2.0).0 > 0), (false, true), "at scale 2 the padding and the column are twice as wide");
    }

    /// The first two lines of a session drawn into a window of `main_w` pixels at the right of a list: the state dump of the times.
    fn drawn_json(main_w: usize) -> (usize, Value) {
        let app = app(1.0, false);
        let (gutter, col_px) = app.times_gutter(main_w, 1.0);
        let mut times = Times::seeded("s", 0, 10, &[(0, 1_795_000_000)]);
        let mut fs = crate::gui::fonts::system(false);
        let mut swash = cosmic_text::SwashCache::new();
        let mut controls = crate::gui::controls::Controls::default();
        controls.begin(1.0, 14.0);
        let mut buf = vec![7u32; 2400 * 100];
        let hit = [(0.0, 18.0, 0usize, 0u16), (18.0, 36.0, 1, crate::gui::transcript::ROLE_USER_BG | crate::gui::transcript::STYLE_USER)];
        times.paint(&mut controls, &mut fs, &mut swash, &mut buf, (2400, 100), gutter, col_px, (0, 100), &hit, &|raw| Some(raw as i64));
        let left = buf.chunks(2400).flat_map(|row| row.iter().enumerate().filter(|(_, &p)| p != 7).map(|(x, _)| x)).min();
        assert!(left.map_or(true, |x| x >= 2400 - gutter), "no pixel of a time is left of its gutter ({left:?}, gutter {gutter})");
        (gutter, times.json())
    }

    #[test]
    fn the_form_of_the_time_that_is_drawn_follows_the_width_of_the_window() {
        for (main_w, want) in [(2200, Some(("full", 19))), (697, Some(("full", 19))), (696, Some(("short", 14))), (654, Some(("short", 14))), (653, Some(("clock", 8))), (603, Some(("clock", 8))), (602, None)] {
            let (gutter, json) = drawn_json(main_w);
            let rows = json["drawn"].as_array().unwrap();
            match want {
                Some((word, chars)) => {
                    assert!(gutter > 0, "a gutter at {main_w}");
                    assert_eq!(json["form"], word, "the form at {main_w}");
                    assert_eq!(rows.len(), 2, "the first line and the person's line each carry a time at {main_w}");
                    assert!(rows.iter().all(|r| r[2].as_str().map(str::len) == Some(chars)), "every time has the {chars} characters of the {word} form: {rows:?}");
                }
                None => assert_eq!((gutter, json["form"].as_str(), rows.len()), (0, Some("none"), 0), "nothing at {main_w}"),
            }
        }
    }

    #[test]
    fn the_short_and_the_clock_forms_are_the_full_time_without_its_year_and_its_date() {
        let secs = 1_795_000_000;
        let full = crate::gui::blocks::stamp_text(secs, crate::transfer::utc_offset(secs as i64));
        for (main_w, from) in [(2200, 0), (696, 5), (603, 11)] {
            let (_, json) = drawn_json(main_w);
            assert_eq!(json["drawn"][0][2].as_str(), Some(&full[from..]), "at {main_w} the time is the full one from character {from}");
        }
    }

    #[test]
    fn the_gutter_is_kept_for_a_wide_session_transcript_only() {
        let wide = app(1.0, false);
        let (px, col) = wide.times_gutter(2200, 1.0);
        assert_eq!((col, px), (wide.char_w(), crate::gui::blocks::gutter_px(2200 - 8, wide.char_w())));
        assert!(px >= 21 * 8, "a window wide enough keeps the room");
        assert_eq!(wide.times_gutter(500, 1.0).0, 0, "a window with fewer than 70 columns keeps none");
        assert!(wide.times_gutter(800, 1.0).0 > 0, "93 columns keep the full form (the first rule wanted 101)");
        assert_eq!(wide.times_gutter(2200, 2.0).1, wide.char_w() * 2.0, "the column is as wide as the window draws it");
        let mut empty = app(1.0, false);
        empty.session.clear();
        assert_eq!(empty.times_gutter(2200, 1.0).0, 0, "no session, no transcript: the start lines have no times");
        let mut panel = app(1.0, false);
        panel.panel_open(Step::Label);
        assert_eq!(panel.times_gutter(2200, 1.0).0, 0, "a panel is no transcript");
    }

    #[test]
    fn the_answer_that_was_awaited_draws_the_window_again_and_any_other_does_not() {
        let mut app = app(1.0, false);
        let ask = app.ui.times.next("session-s0", Some((1, 4)), 0).expect("a request goes out");
        let value: Value = serde_json::from_str(&ask).unwrap();
        assert_eq!((app.dirty, value["op"].as_str()), (false, Some("stamps")));
        app.ui_stamps(&json!({"session": "session-s0", "from": 99, "until": 100, "stamps": []}).to_string());
        assert!(!app.dirty, "an answer to another question changes nothing");
        app.ui_stamps(&json!({"session": "session-s0", "from": 1, "until": 4, "stamps": [[1, 1_795_000_000u64]]}).to_string());
        assert!(app.dirty, "the times it brought are shown at the next frame");
    }
}
