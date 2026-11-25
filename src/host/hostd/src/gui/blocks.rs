//! The blocks of the conversation and what stands beside them (`M24-P11b`): who spoke, where a block starts, the text of a time and the width of the gutter that holds it.
//! Pure functions of the roles that `transcript.rs` already computes; nothing here draws.

use super::transcript::{ROLE_BOX, ROLE_BOX_TOP, ROLE_PEER_BG, ROLE_USER_BG, STYLE_AGENT, STYLE_MASK, STYLE_USER};

/// Columns the gutter takes: a date and time of 19 characters and two of air.
pub(super) const GUTTER_COLS: usize = 21;
/// The area must hold this many columns for a gutter to be kept (the text keeps 80 at least).
pub(super) const MIN_COLS: usize = 101;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    User,
    Peer,
    Command,
    Response,
    Note,
}

/// Who a line is from, by the role bits of the transcript.
pub(super) fn kind_of(role: u16) -> Kind {
    if role & ROLE_PEER_BG != 0 {
        Kind::Peer
    } else if role & ROLE_USER_BG != 0 || role & STYLE_MASK == STYLE_USER {
        Kind::User
    } else if role & ROLE_BOX != 0 {
        Kind::Command
    } else if role & STYLE_MASK == STYLE_AGENT {
        Kind::Response
    } else {
        Kind::Note
    }
}

/// Whether the line with `role` begins a block, given the role of the line before it (none: it is the first on the screen).
pub(super) fn starts_block(previous: Option<u16>, role: u16) -> bool {
    let Some(before) = previous else { return true };
    role & ROLE_BOX_TOP != 0 || kind_of(role) != kind_of(before) || kind_of(role) == Kind::Note
}

/// The first and last position of the block that holds position `pos` among lines with the `roles` given in their order (the lines of the whole transcript, not of the screen);
/// None when `pos` is past the lines.
pub(super) fn block_span(roles: &[u16], pos: usize) -> Option<(usize, usize)> {
    if pos >= roles.len() {
        return None;
    }
    let mut first = pos;
    while first > 0 && !starts_block(Some(roles[first - 1]), roles[first]) {
        first -= 1;
    }
    let mut last = pos;
    while last + 1 < roles.len() && !starts_block(Some(roles[last]), roles[last + 1]) {
        last += 1;
    }
    Some((first, last))
}

/// The last second that is written with four digits of year (9999-12-31 00:00:00 UTC): a time past it is shown as this one, so that the text keeps its width.
const LAST_SECS: u64 = 253_402_214_400;

/// `YYYY-MM-DD HH:MM:SS` of a unix time shown with `offset` seconds east of UTC.
pub(super) fn stamp_text(secs: u64, offset: i64) -> String {
    let c = crate::transfer::civil(secs.min(LAST_SECS) as i64, offset);
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", c.year, c.month, c.day, c.hour, c.minute, c.second)
}

/// The pixels of the gutter for a text area `text_w` pixels wide with columns `col_px` wide; 0 when the area is too narrow to keep one.
pub(super) fn gutter_px(text_w: usize, col_px: f32) -> usize {
    if ((text_w as f32 / col_px) as usize) < MIN_COLS {
        return 0; // (a column of no width, or less, or no number, gives no columns here, or a gutter of no pixels below)
    }
    (GUTTER_COLS as f32 * col_px).ceil() as usize
}

#[cfg(test)]
#[path = "blocks_tests.rs"]
mod tests;
