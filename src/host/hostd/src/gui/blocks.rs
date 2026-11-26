//! The blocks of the conversation and what stands beside them (`M24-P11b`): who spoke, where a block starts, the text of a time and the width of the gutter that holds it.
//! Pure functions of the roles that `transcript.rs` already computes; nothing here draws.

use super::transcript::{ROLE_BOX, ROLE_BOX_TOP, ROLE_PEER_BG, ROLE_USER_BG, STYLE_AGENT, STYLE_MASK, STYLE_USER};

/// Columns the full gutter takes: a date and time of 19 characters and two of air.
pub(super) const GUTTER_COLS: usize = 21;
/// The text keeps this many columns at the least: a gutter is kept only when the area holds this many more than the form's columns (`M24-P11h`; `M24-P11b` kept 80).
pub(super) const TEXT_MIN_COLS: usize = 60;

/// The forms of the time, longest first: the date with the year, the date without it, the clock alone; the seconds are in every one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Form {
    Full,
    Short,
    Clock,
}

impl Form {
    pub(super) const ALL: [Form; 3] = [Form::Full, Form::Short, Form::Clock];

    /// Characters of the text of this form.
    pub(super) fn chars(self) -> usize {
        match self {
            Form::Full => 19,
            Form::Short => 14,
            Form::Clock => 8,
        }
    }

    /// Columns of its gutter: the text and two of air.
    pub(super) fn cols(self) -> usize {
        self.chars() + 2
    }

    /// The word the state dump says.
    pub(super) fn word(self) -> &'static str {
        match self {
            Form::Full => "full",
            Form::Short => "short",
            Form::Clock => "clock",
        }
    }

    /// The pixels of the gutter of this form for columns `col_px` wide, in whole pixels.
    fn px(self, col_px: f32) -> usize {
        (self.cols() as f32 * col_px).ceil() as usize
    }

    /// The form whose gutter is the `gutter` pixels that were kept for columns `col_px` wide: the longest whose room is within them; none for no gutter, or less than the shortest form needs.
    pub(super) fn of_gutter(gutter: usize, col_px: f32) -> Option<Form> {
        if !(col_px > 0.0) {
            return None;
        }
        Form::ALL.into_iter().find(|f| f.px(col_px) <= gutter)
    }

    #[cfg(test)]
    pub(super) fn of_word(word: &str) -> Option<Form> {
        Form::ALL.into_iter().find(|f| f.word() == word)
    }
}

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

/// The time of a unix time shown with `offset` seconds east of UTC in the form `form`: `YYYY-MM-DD HH:MM:SS`, `MM-DD HH:MM:SS` or `HH:MM:SS`.
pub(super) fn stamp_form(form: Form, secs: u64, offset: i64) -> String {
    let c = crate::transfer::civil(secs.min(LAST_SECS) as i64, offset);
    match form {
        Form::Full => format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", c.year, c.month, c.day, c.hour, c.minute, c.second),
        Form::Short => format!("{:02}-{:02} {:02}:{:02}:{:02}", c.month, c.day, c.hour, c.minute, c.second),
        Form::Clock => format!("{:02}:{:02}:{:02}", c.hour, c.minute, c.second),
    }
}

/// `YYYY-MM-DD HH:MM:SS` of a unix time shown with `offset` seconds east of UTC.
pub(super) fn stamp_text(secs: u64, offset: i64) -> String {
    stamp_form(Form::Full, secs, offset)
}

/// The form for a text area `text_w` pixels wide with columns `col_px` wide: the longest whose gutter leaves the text `TEXT_MIN_COLS` columns; none when not even the shortest does.
pub(super) fn form_for(text_w: usize, col_px: f32) -> Option<Form> {
    if !(col_px > 0.0) {
        return None; // (a column of no width, or less, or no number, is no layout)
    }
    let cols = (text_w as f32 / col_px) as usize;
    Form::ALL.into_iter().find(|f| cols >= TEXT_MIN_COLS + f.cols())
}

/// The pixels of the gutter for a text area `text_w` pixels wide with columns `col_px` wide; 0 when the area is too narrow to keep one.
pub(super) fn gutter_px(text_w: usize, col_px: f32) -> usize {
    form_for(text_w, col_px).map_or(0, |f| f.px(col_px)) // (no form, no pixels)
}

#[cfg(test)]
#[path = "blocks_tests.rs"]
mod tests;
