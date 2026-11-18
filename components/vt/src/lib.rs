//! `lico-vt`: the emulator of a terminal and the encoding of its input (M23-P01). Bytes a program writes become a grid of styled cells with a scrollback, a cursor, a title
//! and modes; keys, a paste and the mouse become the bytes the program expects. No process, no pty, no file, no clock, no environment variable, no thread, no `unsafe`.
//! The contract is `SPEC.md`.

mod csi;
mod grid;
mod keys;
mod parser;
mod term;
mod types;
mod width;

pub use keys::{encode_key, encode_mouse, encode_paste, Button, Key, Mods, MouseEvent, MouseKind};
pub use term::Term;
pub use types::{Attrs, Cell, Color, Modes, MouseMode, View};
pub use width::char_width;

/// The widest screen: wider than any window the program draws.
pub const MAX_COLS: u16 = 500;
/// The tallest screen.
pub const MAX_ROWS: u16 = 200;
/// The lines of scrollback a terminal keeps unless it is asked for another number.
pub const DEFAULT_SCROLLBACK: usize = 2_000;
/// The most lines of scrollback a caller may ask for.
pub const MAX_SCROLLBACK: usize = 20_000;
/// The most cells of scrollback, together: about 10 MB.
pub const MAX_SCROLLBACK_CELLS: usize = 600_000;
/// The most parameters of one sequence that are kept.
pub const MAX_PARAMS: usize = 32;
/// The longest OSC string that is read; a longer one is ignored whole.
pub const MAX_OSC: usize = 4_096;
/// The longest title, in characters.
pub const MAX_TITLE: usize = 256;
/// The most bytes of replies that wait to be taken.
pub const MAX_REPLY: usize = 4_096;
