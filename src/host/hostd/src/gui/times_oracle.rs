//! The time beside each block in its shorter forms and the scroll step (`M24-P11h`): the oracle of `scroll_oracle.rs` run on windows that are not wide enough for the full form.
//!
//! The windows are made as wide as a given number of columns of text (the list at the left, the padding and the columns), so that each form is the one that is drawn; the scroll step must
//! draw the same pixels as a full repaint in each, with the times arriving late, a long wrapped line and a command card.

use super::*;
use crate::gui::blocks::{stamp_form, Form};

/// The width of a window whose text area holds `cols` columns and a little over (a third of a column), at `scale`.
fn width_for(cols: usize, scale: f32) -> usize {
    let probe = app(scale, false);
    probe.effective_list_w(scale) + ((8.0 + 8.5 * cols as f32 + 8.5 / 3.0) * scale) as usize
}

fn turn(p: &mut Pair, n: usize) {
    p.push(&[
        format!(">> user prompt {n} 한글 🙂"),
        format!("Agent answer {n} with details and wrapped prose."),
        "$ cargo test --quiet".into(),
        "> test result: ok.".into(),
        "[exit 0]".into(),
        format!("Agent prose after the command {n}."),
        format!(">> {}", "a long question that fills the width ".repeat(3)),
        "word ".repeat(20),
    ]);
}

fn form_of(side: &Side) -> String {
    side.app.ui.times.json()["form"].as_str().unwrap_or("").to_string()
}

#[test]
fn a_window_that_holds_each_form_draws_the_same_pixels_with_the_scroll_step_as_with_a_full_repaint_across_scales() {
    for scale in [1.0, 1.25, 2.0] {
        for (cols, form) in [(90, Form::Full), (78, Form::Short), (72, Form::Clock)] {
            let mut p = Pair::new((width_for(cols, scale), 700), scale);
            p.full("first", Cause::First);
            assert_eq!(form_of(&p.on), form.word(), "{cols} columns at scale {scale}");
            for n in 0..8 {
                turn(&mut p, n);
                p.step(&format!("turn {n}, no time known yet"));
            }
            let (gutter, rows) = drawn(&p.on);
            assert!(gutter > 0 && rows == 0, "the gutter is kept before the first time is known ({gutter}, {rows})");
            assert!(gutter_plain(&p.on), "no band, box or text reaches the gutter of the {form:?} form");
            let to = p.seq + 1;
            know(&mut p, to);
            p.step("a late answer brings the times");
            assert_eq!(drawn(&p.on).0, gutter, "a time arriving late does not move the text");
            assert!(drawn(&p.on).1 >= 4, "the blocks on the screen show a time, drew {}", drawn(&p.on).1);
            assert_eq!(drawn(&p.off), drawn(&p.on), "a full repaint draws the same texts");
            for side in [&p.on, &p.off] {
                for row in side.app.ui.times.json()["drawn"].as_array().unwrap() {
                    let line = side.app.ui.hit_lines.iter().find(|l| l.0 == row[0].as_f64().unwrap() as f32).expect("a time stands at a line of the screen");
                    let secs = 1_795_000_000 + ((side.app.view.as_ref().unwrap().lines[line.2].0 - 1) / 3 * 3 + 1) as u64 * 7;
                    assert_eq!(row[2].as_str().unwrap(), stamp_form(form, secs, crate::transfer::utc_offset(secs as i64)), "the time of the {form:?} form");
                }
            }
            times_stay_in_the_text_area(&p.on);
            for n in 0..6 {
                p.push(&[format!("one more line {n}")]);
                let to = p.seq + 1;
                know(&mut p, to);
                p.shift(&format!("a line {n} with times in the {form:?} form"));
            }
            p.push(&["$ echo more".into(), "inside the box".into(), "[exit 1]".into(), "prose".into()]);
            let to = p.seq + 1;
            know(&mut p, to);
            p.shift("a command card with times");
        }
    }
}

#[test]
fn a_window_that_is_resized_across_the_edges_of_the_forms_repaints_whole_and_draws_the_same_pixels() {
    let scale = 1.0;
    let mut p = Pair::new((width_for(90, scale), 700), scale);
    p.full("first", Cause::First);
    for n in 0..6 {
        turn(&mut p, n);
    }
    let to = p.seq + 1;
    know(&mut p, to);
    p.step("times known, the full form");
    assert_eq!(form_of(&p.on), "full");
    for (cols, word) in [(78, "short"), (72, "clock"), (66, "none"), (72, "clock"), (78, "short"), (90, "full")] {
        p.resize((width_for(cols, scale), 700), 1);
        p.full(&format!("resize to {cols} columns"), Cause::Size);
        assert_eq!((form_of(&p.on), form_of(&p.off)), (word.to_string(), word.to_string()), "{cols} columns");
        assert_eq!(drawn(&p.on).0 == 0, word == "none", "no gutter exactly when there is no form");
        times_stay_in_the_text_area(&p.on);
    }
}
