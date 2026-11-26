use super::*;
use crate::gui::transcript::{
    default_role, ROLE_BOX_BOT, ROLE_ERR_ROW, ROLE_TOOL_FOLDED, ROLE_TOOL_HEAD, STYLE_CMD, STYLE_CMD_OUT, STYLE_ERROR, STYLE_EXIT_ERR, STYLE_TOOL, STYLE_TOOL_FOLD,
};

#[test]
fn a_line_is_from_the_person_a_peer_a_command_the_agent_or_a_note() {
    assert_eq!(kind_of(default_role(">> hello")), Kind::User);
    assert_eq!(kind_of(STYLE_USER | ROLE_USER_BG), Kind::User);
    assert_eq!(kind_of(STYLE_USER), Kind::User, "a line of the person that has no band is still the person's");
    assert_eq!(kind_of(ROLE_USER_BG), Kind::User, "a band is the person's whatever its style says");
    assert_eq!(kind_of(STYLE_USER | ROLE_PEER_BG), Kind::Peer);
    assert_eq!(kind_of(STYLE_TOOL | ROLE_PEER_BG | ROLE_BOX_TOP), Kind::Peer, "a peer's card holds tool lines too");
    assert_eq!(kind_of(default_role("$ ls")), Kind::Command);
    assert_eq!(kind_of(STYLE_CMD_OUT | ROLE_BOX), Kind::Command);
    assert_eq!(kind_of(default_role("[tool] x")), Kind::Command);
    assert_eq!(kind_of(STYLE_EXIT_ERR | ROLE_BOX | ROLE_ERR_ROW), Kind::Command);
    assert_eq!(kind_of(STYLE_TOOL_FOLD | ROLE_BOX | ROLE_BOX_TOP | ROLE_BOX_BOT | ROLE_TOOL_FOLDED), Kind::Command, "a folded card is still a command");
    assert_eq!(kind_of(STYLE_TOOL | ROLE_BOX | ROLE_BOX_TOP | ROLE_TOOL_HEAD), Kind::Command);
    assert_eq!(kind_of(default_role("plain words from the agent")), Kind::Response);
    assert_eq!(kind_of(STYLE_AGENT), Kind::Response);
    assert_eq!(kind_of(default_role("[error] boom")), Kind::Note);
    assert_eq!(kind_of(STYLE_ERROR), Kind::Note);
    assert_eq!(kind_of(default_role("[turn started]")), Kind::Note, "an event that is shown");
}

#[test]
fn a_block_starts_at_the_first_line_a_change_of_who_a_card_and_every_note() {
    let user = STYLE_USER | ROLE_USER_BG;
    let reply = STYLE_AGENT;
    let cmd_top = STYLE_CMD | ROLE_BOX | ROLE_BOX_TOP;
    let cmd_mid = STYLE_CMD_OUT | ROLE_BOX;
    let peer_top = STYLE_USER | ROLE_PEER_BG | ROLE_BOX_TOP;
    let peer_mid = STYLE_USER | ROLE_PEER_BG;
    assert!(starts_block(None, reply), "the first line of the screen");
    assert!(starts_block(None, cmd_mid), "even in the middle of a card");
    assert!(!starts_block(Some(user), user), "the lines of one message of the person");
    assert!(!starts_block(Some(reply), reply), "the lines of one reply");
    assert!(starts_block(Some(user), reply), "the person, then the agent");
    assert!(starts_block(Some(reply), user));
    assert!(starts_block(Some(reply), cmd_top));
    assert!(!starts_block(Some(cmd_top), cmd_mid), "the output of a command is the command's");
    assert!(starts_block(Some(cmd_mid), cmd_top), "a second command is a block of its own");
    assert!(starts_block(Some(cmd_mid), reply));
    assert!(starts_block(Some(reply), peer_top));
    assert!(!starts_block(Some(peer_top), peer_mid));
    assert!(starts_block(Some(peer_mid), peer_top), "a second peer card");
    assert!(starts_block(Some(peer_mid), user), "the person after a peer");
    let note = STYLE_ERROR;
    assert!(starts_block(Some(reply), note));
    assert!(starts_block(Some(note), note), "each note stands alone");
}

#[test]
fn the_text_of_a_time_is_a_date_and_a_time_to_the_second_in_the_local_offset() {
    assert_eq!(stamp_text(0, 0), "1970-01-01 00:00:00");
    assert_eq!(stamp_text(86_399, 0), "1970-01-01 23:59:59");
    assert_eq!(stamp_text(1_764_000_000, 0), "2025-11-24 16:00:00");
    assert_eq!(stamp_text(1_764_000_000, 9 * 3600), "2025-11-25 01:00:00", "east of UTC the day can be the next one");
    assert_eq!(stamp_text(1_764_000_000, -17 * 3600), "2025-11-23 23:00:00", "west of it the one before");
    assert_eq!(stamp_text(951_782_400, 0), "2000-02-29 00:00:00", "a leap day");
    assert_eq!(stamp_text(1_735_689_599, 0), "2024-12-31 23:59:59", "the last second of a year");
    assert_eq!(stamp_text(1_735_689_599, 1), "2025-01-01 00:00:00", "and the next one");
    assert_eq!(stamp_text(5, 0).len(), 19, "always the same width: it stands in a gutter of a fixed width");
    assert_eq!(stamp_text(u64::MAX, 14 * 3600).len(), 19, "a time that no clock makes is cut at the last year of four digits");
    assert_eq!(stamp_text(u64::MAX, 0), "9999-12-31 00:00:00");
    assert_eq!(stamp_text(0, -17 * 3600), "1969-12-31 07:00:00");
}

#[test]
fn the_three_forms_of_a_time_keep_the_seconds_and_drop_the_year_then_the_date() {
    let at = 1_764_000_000; // 2025-11-24 16:00:00 UTC
    assert_eq!(stamp_form(Form::Full, at, 0), "2025-11-24 16:00:00");
    assert_eq!(stamp_form(Form::Short, at, 0), "11-24 16:00:00");
    assert_eq!(stamp_form(Form::Clock, at, 0), "16:00:00");
    assert_eq!((stamp_form(Form::Short, at, 9 * 3600), stamp_form(Form::Clock, at, 9 * 3600)), ("11-25 01:00:00".to_string(), "01:00:00".to_string()), "east of UTC the day can be the next one");
    assert_eq!(stamp_form(Form::Short, 1_735_689_599, 1), "01-01 00:00:00", "the year turns, the short form shows the new month and day");
    assert_eq!(stamp_form(Form::Short, 951_782_400, 0), "02-29 00:00:00", "a leap day");
    for secs in [0, 5, 86_399, 1_764_000_000, u64::MAX] {
        assert_eq!(stamp_form(Form::Full, secs, 0), stamp_text(secs, 0), "the full form is the time of M24-P11b");
        for form in Form::ALL {
            assert_eq!(stamp_form(form, secs, 14 * 3600).len(), form.chars(), "always the width of its form: it stands in a gutter of a fixed width");
        }
    }
}

#[test]
fn the_form_is_the_longest_one_that_leaves_the_text_sixty_columns() {
    for (cols, want) in [(500, Some(Form::Full)), (101, Some(Form::Full)), (81, Some(Form::Full)), (80, Some(Form::Short)), (76, Some(Form::Short)), (75, Some(Form::Clock)), (70, Some(Form::Clock)), (69, None), (1, None), (0, None)] {
        assert_eq!(form_for(cols * 10, 10.0), want, "{cols} columns of 10 pixels");
    }
    assert_eq!((form_for(809, 10.0), form_for(810, 10.0)), (Some(Form::Short), Some(Form::Full)), "80 columns and nine tenths are 80; 81 are the edge of the full form");
    assert_eq!((form_for(759, 10.0), form_for(760, 10.0)), (Some(Form::Clock), Some(Form::Short)), "76 columns are the edge of the short form");
    assert_eq!((form_for(699, 10.0), form_for(700, 10.0)), (None, Some(Form::Clock)), "70 columns are the edge of the clock");
    assert_eq!((form_for(688, 8.5), form_for(689, 8.5)), (Some(Form::Short), Some(Form::Full)), "a column of 8.5 pixels: 81 columns are 688.5 pixels");
    assert_eq!((form_for(645, 8.5), form_for(646, 8.5)), (Some(Form::Clock), Some(Form::Short)), "76 columns are 646 pixels");
    assert_eq!((form_for(594, 8.5), form_for(595, 8.5)), (None, Some(Form::Clock)), "70 columns of 8.5 pixels are 595 pixels");
    assert_eq!(form_for(2000, 0.0), None, "a column that has no width is no layout");
    assert_eq!(form_for(2000, -3.0), None);
    assert_eq!(form_for(2000, f32::NAN), None);
    assert_eq!(form_for(usize::MAX, 10.0), Some(Form::Full), "no overflow");
    assert_eq!(form_for(0, 10.0), None);
}

#[test]
fn the_gutter_is_the_room_of_the_form_in_whole_pixels() {
    assert_eq!((gutter_px(810, 10.0), gutter_px(809, 10.0), gutter_px(760, 10.0), gutter_px(759, 10.0), gutter_px(700, 10.0), gutter_px(699, 10.0)), (210, 160, 160, 100, 100, 0));
    assert_eq!(gutter_px(5000, 10.0), 210, "a wide window keeps the same gutter, not a wider one");
    assert_eq!(gutter_px(1000, 9.6), 202, "a column of 9.6 pixels: 21 of them, rounded up to whole pixels (201.6)");
    assert_eq!(gutter_px(777, 9.6), 154, "80 columns of 9.6 pixels: the short form, 16 columns (153.6)");
    assert_eq!(gutter_px(778, 9.6), 202, "81 columns: the full form");
    assert_eq!(gutter_px(1000, 10.2), 215, "21 columns of 10.2 pixels are 214.2: the room is rounded up, never down, or the last digit would stand in the next column");
    assert_eq!(gutter_px(780, 10.2), 164, "76 columns of 10.2 pixels: the short form, 16 columns are 163.2");
    assert_eq!(gutter_px(0, 10.0), 0);
    assert_eq!(gutter_px(2000, 0.0), 0, "a column that has no width is no layout");
    assert_eq!(gutter_px(2000, -3.0), 0);
    assert_eq!(gutter_px(usize::MAX, 10.0), 210, "no overflow");
}

#[test]
fn a_form_is_told_from_the_gutter_that_was_kept() {
    for col in [8.5f32, 10.0, 9.6, 17.0, 12.75] {
        for form in Form::ALL {
            assert_eq!(Form::of_gutter((form.cols() as f32 * col).ceil() as usize, col), Some(form), "the gutter of {form:?} at {col} pixels a column");
        }
    }
    assert_eq!(Form::of_gutter(0, 8.5), None, "no gutter, no form");
    assert_eq!((Form::of_gutter(84, 8.5), Form::of_gutter(85, 8.5)), (None, Some(Form::Clock)), "less than the room of the clock is no form");
    assert_eq!(Form::of_gutter(90, 8.5), Some(Form::Clock), "a gutter that is not any form's own is the longest form that fits in it");
    assert_eq!(Form::of_gutter(137, 8.5), Some(Form::Short));
    assert_eq!(Form::of_gutter(5000, 8.5), Some(Form::Full));
    assert_eq!((Form::of_gutter(100, 0.0), Form::of_gutter(100, -1.0), Form::of_gutter(100, f32::NAN)), (None, None, None));
    for form in Form::ALL {
        assert_eq!(Form::of_word(form.word()), Some(form));
    }
    assert_eq!(Form::of_word("none"), None);
}

#[test]
fn the_constants_are_what_the_plan_says() {
    assert_eq!((GUTTER_COLS, TEXT_MIN_COLS), (21, 60));
    assert_eq!(Form::ALL.map(|f| (f.chars(), f.cols())), [(19, 21), (14, 16), (8, 10)]);
    assert_eq!(Form::ALL.map(Form::word), ["full", "short", "clock"]);
    assert_eq!(stamp_text(0, 0).len() + 2, GUTTER_COLS, "a date and time and two columns of air");
    assert_eq!(Form::Full.cols(), GUTTER_COLS);
}

#[test]
fn the_block_of_a_line_runs_from_its_start_to_the_line_before_the_next() {
    let user = STYLE_USER | ROLE_USER_BG;
    let reply = STYLE_AGENT;
    let cmd_top = STYLE_CMD | ROLE_BOX | ROLE_BOX_TOP;
    let cmd_end = STYLE_CMD_OUT | ROLE_BOX | ROLE_BOX_BOT;
    let peer_top = STYLE_USER | ROLE_PEER_BG | ROLE_BOX_TOP;
    let peer_end = STYLE_USER | ROLE_PEER_BG | ROLE_BOX_BOT;
    let note = STYLE_ERROR;
    // 0..=1 the person, 2..=4 the reply, 5..=6 a command, 7..=8 a peer's card, 9..=10 a second card, 11 a note, 12 a note, 13 the reply, 14 the person
    let roles = [user, user, reply, reply, reply, cmd_top, cmd_end, peer_top, peer_end, peer_top, peer_end, note, note, reply, user];
    let expect = [(0, 1), (0, 1), (2, 4), (2, 4), (2, 4), (5, 6), (5, 6), (7, 8), (7, 8), (9, 10), (9, 10), (11, 11), (12, 12), (13, 13), (14, 14)];
    for (pos, want) in expect.iter().enumerate() {
        assert_eq!(block_span(&roles, pos), Some(*want), "position {pos}");
    }
    assert_eq!(block_span(&roles, roles.len()), None, "past the lines");
    assert_eq!(block_span(&[], 0), None, "no lines");
    assert_eq!(block_span(&[reply], 0), Some((0, 0)), "one line alone");
    assert_eq!(block_span(&[reply, reply, reply], 1), Some((0, 2)), "the whole of a list that is one block");
}

#[test]
fn the_block_of_a_line_is_the_one_the_gutter_gives_a_time_to() {
    let roles = [STYLE_AGENT, STYLE_AGENT, STYLE_USER | ROLE_USER_BG, STYLE_USER | ROLE_USER_BG, STYLE_AGENT];
    for pos in 0..roles.len() {
        let (first, last) = block_span(&roles, pos).expect("a line of the list");
        assert!(first <= pos && pos <= last);
        assert!(starts_block(first.checked_sub(1).map(|i| roles[i]), roles[first]), "the block begins where a block begins");
        assert!((first + 1..=last).all(|i| !starts_block(Some(roles[i - 1]), roles[i])), "and holds no other start");
        assert!(last + 1 == roles.len() || starts_block(Some(roles[last]), roles[last + 1]), "the line after it begins another block");
    }
}
