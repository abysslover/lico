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
fn the_gutter_is_kept_only_when_the_area_holds_a_hundred_and_one_columns() {
    assert_eq!(gutter_px(1010, 10.0), 210, "101 columns of 10 pixels: 21 columns of gutter");
    assert_eq!(gutter_px(1009, 10.0), 0, "100 columns: none");
    assert_eq!(gutter_px(5000, 10.0), 210, "a wide window keeps the same gutter, not a wider one");
    assert_eq!(gutter_px(1000, 9.6), 202, "a column of 9.6 pixels: 21 of them, rounded up to whole pixels (201.6)");
    assert_eq!(gutter_px(969, 9.6), 0, "100 columns of 9.6 pixels is 960 pixels, 101 is 969.6");
    assert_eq!(gutter_px(970, 9.6), 202);
    assert_eq!(gutter_px(0, 10.0), 0);
    assert_eq!(gutter_px(2000, 0.0), 0, "a column that has no width is no layout");
    assert_eq!(gutter_px(2000, -3.0), 0);
    assert_eq!(gutter_px(usize::MAX, 10.0), 210, "no overflow");
}

#[test]
fn the_constants_are_what_the_plan_says() {
    assert_eq!((GUTTER_COLS, MIN_COLS), (21, 101));
    assert_eq!(stamp_text(0, 0).len() + 2, GUTTER_COLS, "a date and time and two columns of air");
}
