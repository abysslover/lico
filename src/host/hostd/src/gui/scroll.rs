//! The scroll step: a frame whose transcript only moved up (tail-follow) keeps the rows it has and draws the rows that changed.
//!
//! The previous frame is the softbuffer buffer itself, when its age says that it holds the last presented frame. After a frame the
//! window keeps a `Composed` record of it (size, scale, what was open, and for every line of the text area its text, its rows and the
//! pixel rows its glyph bitmaps reached). `decide` compares the record with the new layout and answers either a `Shift` (which pixel rows
//! are kept, and by how much they move) or the `Cause` of a full repaint, which is the path that always existed.
//!
//! Default-deny: a frame is a shift only if everything that can change pixels is in a state that is known to be harmless. A change
//! to the window that this file does not know about (a new overlay, say) is a full repaint until it is named here.
//!
//! Why the pixel rows are taken from glyph extents and not from line boxes: an emoji, a stacked accent or a descender reaches into
//! the row of its neighbour, and the full repaint blends the lines in order. A kept row must have been touched only by lines that are kept
//! (in both frames), or the move would keep a pixel the full repaint does not have, or lose one it does.

/// Why a frame was drawn the way it was. `Shift` is the cheap path; every other value is a full repaint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    Shift,
    First,
    BufferAge,
    Size,
    Scale,
    Fonts,
    Menu,
    Selection,
    Panel,
    Approval,
    Preedit,
    Scrollback,
    LineChanged,
    NothingInCommon,
    RowHeight,
    Overhang,
    Disabled,
}

impl Cause {
    pub const ALL: [Cause; 17] = [
        Cause::Shift, Cause::First, Cause::BufferAge, Cause::Size, Cause::Scale, Cause::Fonts, Cause::Menu, Cause::Selection, Cause::Panel, Cause::Approval,
        Cause::Preedit, Cause::Scrollback, Cause::LineChanged, Cause::NothingInCommon, Cause::RowHeight, Cause::Overhang, Cause::Disabled,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Cause::Shift => "shift",
            Cause::First => "first",
            Cause::BufferAge => "buffer_age",
            Cause::Size => "size",
            Cause::Scale => "scale",
            Cause::Fonts => "fonts",
            Cause::Menu => "menu",
            Cause::Selection => "selection",
            Cause::Panel => "panel",
            Cause::Approval => "approval",
            Cause::Preedit => "preedit",
            Cause::Scrollback => "scrollback",
            Cause::LineChanged => "line_changed",
            Cause::NothingInCommon => "nothing_in_common",
            Cause::RowHeight => "row_height",
            Cause::Overhang => "overhang",
            Cause::Disabled => "disabled",
        }
    }
}

/// The pixel rows `[top, bottom)` that the glyph bitmaps of one line reached (before any clipping to the window), and the smallest
/// column among them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Extent {
    pub top: i32,
    pub bottom: i32,
    pub left: i32,
}

impl Extent {
    pub const EMPTY: Extent = Extent { top: i32::MAX, bottom: i32::MIN, left: i32::MAX };

    pub fn add(&mut self, x: i32, top: i32, bottom: i32) {
        self.top = self.top.min(top);
        self.bottom = self.bottom.max(bottom);
        self.left = self.left.min(x);
    }

    pub fn is_empty(&self) -> bool {
        self.top >= self.bottom
    }

    pub fn shifted(self, dy: i32) -> Extent {
        if self.is_empty() {
            self
        } else {
            Extent { top: self.top + dy, bottom: self.bottom + dy, left: self.left }
        }
    }

    pub fn touches(&self, rows: &[(i32, i32)]) -> bool {
        !self.is_empty() && rows.iter().any(|&(a, b)| self.top < b && a < self.bottom)
    }
}

/// What a line of the text area is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Header,
    Transcript(i64),
    Status, // the line "… working (37 s)" between the transcript and the prompt; not a record
    Prompt,
}

/// One line of the text area as it was laid out: its text, the pixel rows of its row boxes, and (once drawn) the rows its glyphs reached.
#[derive(Clone, Debug, PartialEq)]
pub struct LineRec {
    pub key: Key,
    pub text: String,
    pub top: f32,
    pub bottom: f32,
    pub rows: usize,
    pub ext: Extent,
}

/// Everything besides the lines that can put other pixels into the text area. All false is the only state in which a shift is allowed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Overlay {
    pub menu: bool,
    pub selection: bool,
    pub panel: bool,
    pub approval: bool,
    pub preedit: bool,
    pub scrollback: bool,
}

impl Overlay {
    /// The cause that names the first thing open now or in the previous frame (opening and closing both repaint everything).
    fn cause(&self, prev: &Overlay) -> Option<Cause> {
        let either = |a: bool, b: bool| a || b;
        if either(self.menu, prev.menu) {
            Some(Cause::Menu)
        } else if either(self.selection, prev.selection) {
            Some(Cause::Selection)
        } else if either(self.panel, prev.panel) {
            Some(Cause::Panel)
        } else if either(self.approval, prev.approval) {
            Some(Cause::Approval)
        } else if either(self.preedit, prev.preedit) {
            Some(Cause::Preedit)
        } else if either(self.scrollback, prev.scrollback) {
            Some(Cause::Scrollback)
        } else {
            None
        }
    }
}

/// The frame as it was left in the buffer.
#[derive(Clone, Debug, PartialEq)]
pub struct Composed {
    pub width: usize,
    pub height: usize,
    pub scale: f32,
    pub fonts: u64,
    pub list_w: usize,
    pub top: i32,    // the first pixel row of the text area
    pub hint_y: i32, // the first pixel row of the hint line: nothing of the text area is kept from there on
    pub corner: i32, // the pixel rows from the top that the measurement client's marker squares cover (0 in the product window): nothing is kept there
    pub overlay: Overlay,
    pub lines: Vec<LineRec>,
}

/// The new frame, as far as the decision needs it.
pub struct Now<'a> {
    pub allowed: bool,
    pub age: u8,
    pub row_height: f32,
    pub composed: &'a Composed, // the layout of the new frame; `ext` is empty except for the header, whose extent is needed
}

/// A shift: the pixel rows `[lo, hi)` of the old frame are kept and move by `dy` rows; `kept` are the new lines that stay as they are
/// (the first of them was line `old_first` of the old frame).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shift {
    pub lo: i32,
    pub hi: i32,
    pub dy: i32,
    pub kept: std::ops::Range<usize>, // indexes into the new frame's `lines`
    pub old_first: usize,
}

impl Shift {
    /// Every row of the new frame that is not kept: above the first kept row and below the last one. They are cleared to the background
    /// and drawn again, whatever is in them (the bar, the hint line and the lines that are new included).
    pub fn cleared(&self, height: i32) -> Vec<(i32, i32)> {
        let mut rows = Vec::new();
        if self.lo + self.dy > 0 {
            rows.push((0, self.lo + self.dy));
        }
        if self.hi + self.dy < height {
            rows.push((self.hi + self.dy, height));
        }
        rows
    }
}

/// One function says how a frame is drawn. `Err(cause)` is a full repaint.
pub fn decide(prev: Option<&Composed>, now: &Now) -> Result<Shift, Cause> {
    let new = now.composed;
    if !now.allowed {
        return Err(Cause::Disabled);
    }
    let Some(old) = prev else { return Err(Cause::First) };
    if now.age != 1 {
        return Err(Cause::BufferAge);
    }
    if old.scale != new.scale {
        return Err(Cause::Scale);
    }
    if old.fonts != new.fonts {
        return Err(Cause::Fonts);
    }
    if old.width != new.width || old.height != new.height || old.list_w != new.list_w || old.top != new.top || old.hint_y != new.hint_y || old.corner != new.corner {
        return Err(Cause::Size);
    }
    if let Some(cause) = new.overlay.cause(&old.overlay) {
        return Err(cause);
    }
    // the transcript lines of both frames
    let old_t: Vec<usize> = (0..old.lines.len()).filter(|&i| matches!(old.lines[i].key, Key::Transcript(_))).collect();
    let new_t: Vec<usize> = (0..new.lines.len()).filter(|&i| matches!(new.lines[i].key, Key::Transcript(_))).collect();
    if old_t.is_empty() || new_t.is_empty() {
        return Err(Cause::NothingInCommon);
    }
    // the old lines from j on are the first lines of the new frame, and everything after them is new
    let same = |a: &LineRec, b: &LineRec| a.key == b.key && a.text == b.text && a.rows == b.rows && a.rows > 0;
    let aligned = |j: usize| {
        let kept = old_t.len() - j;
        kept <= new_t.len() && (0..kept).all(|i| same(&old.lines[old_t[j + i]], &new.lines[new_t[i]]))
    };
    let Some(j) = (0..old_t.len()).find(|&j| aligned(j)) else {
        let first_new = new.lines[new_t[0]].key;
        let last_new = new.lines[*new_t.last().unwrap()].key;
        let overlap = |k: Key| match (k, first_new, last_new) {
            (Key::Transcript(n), Key::Transcript(a), Key::Transcript(b)) => (a..=b).contains(&n),
            _ => false,
        };
        return Err(if old_t.iter().any(|&i| overlap(old.lines[i].key)) { Cause::LineChanged } else { Cause::NothingInCommon });
    };
    let kept = old_t.len() - j;
    if now.row_height <= 0.0 || (now.row_height * 16.0).fract() != 0.0 {
        return Err(Cause::RowHeight); // sums of row heights would round differently in the two frames
    }
    let (a, b) = (&old.lines[old_t[j]], &new.lines[new_t[0]]);
    let dy_f = b.top - a.top;
    if dy_f.fract() != 0.0 {
        return Err(Cause::RowHeight);
    }
    let dy = dy_f as i32;
    if (0..kept).any(|i| new.lines[new_t[i]].top - old.lines[old_t[j + i]].top != dy_f) {
        return Err(Cause::LineChanged);
    }
    // a glyph that reaches left into the list column cannot be kept: that column is drawn again
    let list_w = old.list_w as i32;
    if (0..kept).any(|i| {
        let e = old.lines[old_t[j + i]].ext;
        !e.is_empty() && e.left < list_w
    }) {
        return Err(Cause::Overhang);
    }
    // the kept pixel rows: the row boxes of the kept lines, less every row that a line which is not kept touched
    let last = &old.lines[old_t[old_t.len() - 1]];
    let mut lo = a.top.ceil() as i32;
    let mut hi = (last.bottom.floor() as i32).min(old.hint_y - dy.max(0));
    for line in &old.lines[..old_t[j]] {
        if !line.ext.is_empty() {
            lo = lo.max(line.ext.bottom); // the old header and the dropped lines, in front
        }
    }
    for line in &old.lines[old_t[old_t.len() - 1] + 1..] {
        if !line.ext.is_empty() {
            hi = hi.min(line.ext.top); // the old prompt, behind
        }
    }
    for line in &new.lines[..new_t[0]] {
        if !line.ext.is_empty() {
            lo = lo.max(line.ext.bottom - dy); // the new header, in front
        }
    }
    lo = lo.max(old.top).max(new.corner);
    if lo >= hi || lo + dy < 0 || hi + dy > old.height as i32 {
        return Err(Cause::Overhang);
    }
    Ok(Shift { lo, hi, dy, kept: new_t[0]..new_t[kept - 1] + 1, old_first: old_t[j] })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LH: f32 = 20.0;

    fn rec(key: Key, text: &str, row: usize, rows: usize, ext: Option<(i32, i32)>) -> LineRec {
        let top = row as f32 * LH;
        LineRec { key, text: text.into(), top, bottom: top + rows as f32 * LH, rows, ext: ext.map_or(Extent::EMPTY, |(a, b)| Extent { top: a, bottom: b, left: 100 }) }
    }

    /// A header, lines `first..first+n` of one row each and a prompt, glyphs reaching 2 rows beyond their box at most.
    fn frame(first: i64, n: usize) -> Composed {
        let mut lines = vec![rec(Key::Header, "head", 0, 1, Some((2, 18)))];
        for i in 0..n {
            let row = 1 + i;
            lines.push(rec(Key::Transcript(first + i as i64), &format!("line {}", first + i as i64), row, 1, Some((row as i32 * 20 + 2, row as i32 * 20 + 18))));
        }
        lines.push(rec(Key::Prompt, "> ", 1 + n, 1, Some(((1 + n) as i32 * 20 + 2, (1 + n) as i32 * 20 + 18))));
        Composed { width: 800, height: 600, scale: 1.0, fonts: 1, list_w: 90, top: 0, hint_y: 580, corner: 0, overlay: Overlay::default(), lines }
    }

    fn now(c: &Composed) -> Now<'_> {
        Now { allowed: true, age: 1, row_height: LH, composed: c }
    }

    #[test]
    fn a_frame_whose_lines_moved_up_by_two_rows_keeps_the_rows_between_the_header_and_the_prompt() {
        let (old, new) = (frame(10, 20), frame(12, 20));
        let shift = decide(Some(&old), &now(&new)).expect("a shift");
        assert_eq!(shift.dy, -40);
        assert_eq!(shift.kept, 1..19, "the new lines 12..29 are the old lines 12..29");
        // the dropped lines reach row 58 and the new header reaches row 18 (58 in old rows), so the kept rows start at the box of line 12 (60);
        // the old prompt starts at row 422 but the last kept line's box ends at 420
        assert_eq!(shift.lo, 60);
        assert_eq!(shift.hi, 420);
        assert_eq!(shift.cleared(600), vec![(0, 20), (380, 600)]);
    }

    #[test]
    fn a_frame_with_new_lines_only_at_the_bottom_moves_nothing() {
        let (old, new) = (frame(10, 10), frame(10, 12));
        let shift = decide(Some(&old), &now(&new)).expect("a shift");
        assert_eq!(shift.dy, 0);
        assert_eq!(shift.kept, 1..11);
    }

    #[test]
    fn every_frame_that_is_not_a_shift_names_why() {
        let old = frame(10, 20);
        let expect = |new: &Composed, age: u8, allowed: bool, cause: Cause| {
            let result = decide(Some(&old), &Now { allowed, age, row_height: LH, composed: new });
            assert_eq!(result.err(), Some(cause));
        };
        let base = frame(12, 20);
        assert_eq!(decide(None, &now(&base)).err(), Some(Cause::First));
        expect(&base, 1, false, Cause::Disabled);
        expect(&base, 0, true, Cause::BufferAge);
        expect(&base, 2, true, Cause::BufferAge);
        let mut c = base.clone();
        c.width = 801;
        expect(&c, 1, true, Cause::Size);
        let mut c = base.clone();
        c.scale = 2.0;
        expect(&c, 1, true, Cause::Scale);
        let mut c = base.clone();
        c.fonts = 2;
        expect(&c, 1, true, Cause::Fonts);
        for (set, cause) in [
            (Overlay { menu: true, ..Default::default() }, Cause::Menu),
            (Overlay { selection: true, ..Default::default() }, Cause::Selection),
            (Overlay { panel: true, ..Default::default() }, Cause::Panel),
            (Overlay { approval: true, ..Default::default() }, Cause::Approval),
            (Overlay { preedit: true, ..Default::default() }, Cause::Preedit),
            (Overlay { scrollback: true, ..Default::default() }, Cause::Scrollback),
        ] {
            let mut c = base.clone();
            c.overlay = set;
            expect(&c, 1, true, cause); // opening
            let mut o = old.clone();
            o.overlay = set;
            assert_eq!(decide(Some(&o), &now(&base)).err(), Some(cause), "closing"); // closing
        }
        let mut c = base.clone();
        c.lines[3].text = "line 14, changed".into();
        expect(&c, 1, true, Cause::LineChanged);
        expect(&frame(40, 20), 1, true, Cause::NothingInCommon);
        let mut c = base.clone();
        c.lines[2].rows = 2;
        expect(&c, 1, true, Cause::LineChanged);
        assert_eq!(decide(Some(&old), &Now { allowed: true, age: 1, row_height: 21.3, composed: &base }).err(), Some(Cause::RowHeight));
        let mut o = old.clone();
        o.lines[4].ext.left = 50; // a glyph in the list column
        assert_eq!(decide(Some(&o), &now(&base)).err(), Some(Cause::Overhang));
    }

    #[test]
    fn a_dropped_line_that_reaches_into_a_kept_row_makes_that_row_be_drawn_again() {
        let mut old = frame(10, 20);
        old.lines[2].ext = Extent { top: 40, bottom: 75, left: 100 }; // line 11 (dropped) reaches 15 rows into line 13's box... (box 60..80)
        let new = frame(12, 20);
        let shift = decide(Some(&old), &now(&new)).expect("a shift");
        assert_eq!(shift.lo, 75, "the row 75 is the first that no dropped line touched");
    }

    #[test]
    fn the_old_prompt_and_the_new_header_both_limit_the_kept_rows() {
        let mut old = frame(10, 20);
        let last = old.lines.len() - 1;
        old.lines[last].ext.top = 395; // the prompt's tall glyph reaches up into the last line
        let mut new = frame(12, 20);
        new.lines[0].ext.bottom = 30; // a new header with a descender: 30 + 40 in old rows
        let shift = decide(Some(&old), &now(&new)).expect("a shift");
        assert_eq!(shift.hi, 395);
        assert_eq!(shift.lo, 70);
    }
}
