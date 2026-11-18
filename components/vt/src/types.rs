//! The plain types of the contract: a cell, its colours and attributes, the modes, a view.

use std::ops::BitOr;

/// A colour as a name: the window turns it into its palette.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Color {
    Default,
    /// 0 to 7 the dark colours, 8 to 15 the bright ones, 16 to 255 the cube and the greys of xterm.
    Indexed(u8),
    Rgb(u8, u8, u8),
}

/// The flags of a cell.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Attrs(u16);

impl Attrs {
    pub const NONE: Attrs = Attrs(0);
    pub const BOLD: Attrs = Attrs(1);
    pub const DIM: Attrs = Attrs(1 << 1);
    pub const ITALIC: Attrs = Attrs(1 << 2);
    pub const UNDERLINE: Attrs = Attrs(1 << 3);
    pub const BLINK: Attrs = Attrs(1 << 4);
    pub const INVERSE: Attrs = Attrs(1 << 5);
    pub const HIDDEN: Attrs = Attrs(1 << 6);
    pub const STRIKE: Attrs = Attrs(1 << 7);
    /// The cell holds a character two cells wide and the next cell is its tail.
    pub const WIDE: Attrs = Attrs(1 << 8);
    /// The right half of a wide character: nothing to draw.
    pub const TAIL: Attrs = Attrs(1 << 9);

    pub fn contains(self, other: Attrs) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn bits(self) -> u16 {
        self.0
    }

    pub(crate) fn with(self, other: Attrs) -> Attrs {
        Attrs(self.0 | other.0)
    }

    pub(crate) fn without(self, other: Attrs) -> Attrs {
        Attrs(self.0 & !other.0)
    }
}

impl BitOr for Attrs {
    type Output = Attrs;
    fn bitor(self, o: Attrs) -> Attrs {
        Attrs(self.0 | o.0)
    }
}

/// What a place on the screen holds.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub attrs: Attrs,
}

impl Cell {
    /// A space, default colours, no attributes.
    pub const BLANK: Cell = Cell { ch: ' ', fg: Color::Default, bg: Color::Default, attrs: Attrs::NONE };
}

/// What the mouse is reported for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MouseMode {
    Off,
    /// 1000: presses, releases and the wheel.
    Press,
    /// 1002: and a move with a button held.
    Drag,
    /// 1003: and every move.
    Any,
}

/// What the program switched on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Modes {
    pub app_cursor: bool,
    pub app_keypad: bool,
    pub bracketed_paste: bool,
    pub cursor_visible: bool,
    pub alt_screen: bool,
    pub autowrap: bool,
    pub origin: bool,
    pub insert: bool,
    pub focus_events: bool,
    pub mouse: MouseMode,
    pub mouse_sgr: bool,
}

impl Modes {
    pub(crate) const INITIAL: Modes = Modes {
        app_cursor: false,
        app_keypad: false,
        bracketed_paste: false,
        cursor_visible: true,
        alt_screen: false,
        autowrap: true,
        origin: false,
        insert: false,
        focus_events: false,
        mouse: MouseMode::Off,
        mouse_sgr: false,
    };
}

/// The rows to show: exactly `rows` lines of exactly `cols` cells.
#[derive(Clone, PartialEq, Debug)]
pub struct View {
    pub lines: Vec<Vec<Cell>>,
    /// `wrapped[i]`: line `i` goes on in the next (wrapped by the right edge, not ended by the program).
    pub wrapped: Vec<bool>,
    /// The absolute index of the first line (0 is the oldest line of the scrollback).
    pub top: usize,
    /// The absolute count of lines (scrollback + rows).
    pub total: usize,
    /// The distance from the bottom that was used (after the clamping).
    pub scroll: usize,
    /// The cursor's cell; `None` while it is hidden or the view is scrolled away from the bottom.
    pub cursor: Option<(u16, u16)>,
}
