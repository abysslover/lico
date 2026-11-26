//! The light look and the scroll step (`M24-P11d`): the oracle of `scroll_oracle.rs` run again with every window in the light look, and a census of the colours.
//!
//! The scroll step keeps rows of the last frame, so a colour that reached the pixels by a way the look does not know would show here as rows of the other look among kept ones; and a light frame
//! is compared with the dark frame of the same scene, pixel by pixel, so that a colour that is still the dark one is found whatever put it there.

use super::*;
use crate::gui::theme::{tone_with, Choice, Mode};
use crate::gui::transcript::{PEER_BG, RESPONSE_BG, USER_BG};

#[test]
fn a_scripted_session_draws_the_same_pixels_with_the_scroll_step_as_with_a_full_repaint_at_scale_1_in_the_light_look() {
    in_light(|| script(1.0));
}

#[test]
fn a_scripted_session_draws_the_same_pixels_with_the_scroll_step_as_with_a_full_repaint_at_scale_2_in_the_light_look() {
    in_light(|| script(2.0));
}

#[test]
fn a_scripted_session_draws_the_same_pixels_with_the_scroll_step_as_with_a_full_repaint_at_scale_one_and_a_quarter_in_the_light_look() {
    in_light(|| script(1.25));
}

#[test]
fn the_roles_the_boxes_the_gutter_and_the_folds_match_full_repaint_in_the_light_look_too() {
    in_light(|| {
        user_prompt_backgrounds_role_colors_and_hidden_control_events_match_full_repaint_across_scales();
        command_execution_boxes_exit_errors_mcp_cards_and_peer_banners_match_full_repaint_across_scales();
        the_gutter_with_the_times_draws_the_same_pixels_with_the_scroll_step_as_with_a_full_repaint_across_scales();
        a_block_that_is_folded_draws_the_same_pixels_with_the_scroll_step_as_with_a_full_repaint_across_scales();
    });
}

/// A window of the light look draws the light look from its very first frame (the oracle's windows are told by a thread-local, not by the host).
#[test]
fn the_oracle_in_light_really_draws_light() {
    let mut dark = Side::new((600, 400), 1.0, false);
    dark.frame();
    let mut light = in_light_value(|| Side::new((600, 400), 1.0, false));
    light.frame();
    assert_eq!(light.app.ui.theme.choice, Choice::Light);
    assert!(dark.buf.iter().any(|&c| c == BACKGROUND) && !light.buf.iter().any(|&c| c == BACKGROUND));
}

fn in_light_value<T>(f: impl FnOnce() -> T) -> T {
    let mut out = None;
    in_light(|| out = Some(f()));
    out.expect("the closure ran")
}

/// What a frame of a rich scene is, in the dark look (`p.on`) and in the light look (`p.off`).
fn scene(menu: bool) -> Pair {
    let mut p = Pair::new((2400, 800), 1.0);
    p.off.app.ui.theme.choice = Choice::Light;
    p.on.frame();
    p.off.frame();
    for turn in 0..6 {
        p.push(&[
            format!(">> user prompt {turn} 한글"),
            format!(">> [from session 'peer' (s123456) | 14:{turn:02} | message | 24 B]"),
            format!(">> cross-session check {turn}"),
            format!("Agent answer {turn} with details and wrapped prose, and a second line of it."),
            "$ cargo test --quiet".into(),
            "> test result: ok.".into(),
            "[exit 0]".into(),
            "$ cargo build".into(),
            "[exit 1]".into(),
            "[tool] mcp__lico__sessions".into(),
            "> {\"sessions\":[]}".into(),
            "[tool error]".into(),
            "> receiver busy".into(),
            "[patch] 1 change(s)".into(),
            "[turn started]".into(),
            "[error] transient warning".into(),
            format!("Agent prose after the command {turn} 한글 요약. {}", "word ".repeat(40)),
        ]);
    }
    let to = p.seq + 1;
    know(&mut p, to);
    fold(&mut p, 17 * 2 + 4);
    p.both(|app| app.toggle_show_events());
    let len = p.on.app.view.as_ref().unwrap().lines.len();
    p.both(|app| app.ui.sel = Some((len - 30, len - 24)));
    if menu {
        p.both(|app| (app.ui.menu_open, app.ui.menu_sel) = (Some(1), 1));
    }
    p.on.frame();
    p.off.frame();
    p
}

/// The rectangle of the theme button in the toolbar, where the two looks differ by the picture (a moon and a sun).
fn theme_button(side: &Side) -> [f32; 4] {
    let at = side.app.toolbar_entries().iter().position(|b| b.id == "theme").expect("the theme button");
    side.app.ui.toolbar_geom.rects[at]
}

/// How many pixels of the light frame are not the dark frame's pixel in the light look (by more than `tolerance` in a channel), and the first of them.
fn leaks(p: &Pair, tolerance: i64) -> (usize, Option<(usize, usize, u32, u32)>) {
    let (width, _) = p.on.size;
    let r = theme_button(&p.on);
    let mut count = 0;
    let mut first = None;
    for (i, (&d, &l)) in p.on.buf.iter().zip(&p.off.buf).enumerate() {
        let (x, y) = (i % width, i / width);
        if (x as f32) >= r[0] && (x as f32) < r[2] && (y as f32) >= r[1] && (y as f32) < r[3] {
            continue;
        }
        let want = tone_with(Mode::Light, d);
        let off = (0..3).map(|k| (((want >> (8 * k)) & 0xFF) as i64 - ((l >> (8 * k)) & 0xFF) as i64).abs()).max().unwrap();
        if off > tolerance {
            count += 1;
            first.get_or_insert((x, y, d, l));
        }
    }
    (count, first)
}

#[test]
fn a_light_frame_is_the_dark_frame_of_the_same_scene_in_the_light_look_and_no_dark_chrome_is_left() {
    for menu in [false, true] {
        let p = scene(menu);
        for chrome in [BACKGROUND, USER_BG, RESPONSE_BG, PEER_BG] {
            assert!(p.on.buf.contains(&chrome), "the scene has the colour {chrome:#010x} in the dark (menu {menu}): it covers what it is meant to cover");
            assert!(p.off.buf.contains(&tone_with(Mode::Light, chrome)), "and the light frame has its mirror {chrome:#010x}");
            assert!(!p.off.buf.contains(&chrome) || chrome == tone_with(Mode::Light, chrome), "no pixel of the light frame is still {chrome:#010x} (menu {menu})");
        }
        let (count, first) = leaks(&p, 40);
        assert_eq!(count, 0, "pixels of the light frame that are not the mirror of the dark frame's (menu {menu}), the first: {first:?}");
    }
}

#[test]
fn the_census_sees_a_leak_when_there_is_one() {
    let mut p = scene(false);
    assert_eq!(leaks(&p, 40).0, 0);
    let background = tone_with(Mode::Light, BACKGROUND);
    let at = p.off.buf.iter().position(|&c| c == background).expect("a background pixel in the light frame");
    p.off.buf[at] = BACKGROUND; // one pixel left in the dark look
    assert_eq!(leaks(&p, 40).0, 1, "the census finds the one pixel");
}
