//! The terminal tab of the window (`M23-P03`): the pure parts (`screen`: colours, the screen the host sent, the pieces of a line, the grid, the selection, the words under the grid;
//! `keys`: which keys are the shell's and which are Lico's) and the part that is the window's (`view`: the tab, its watch, its input, its painting). A directory of its own because
//! `gui/mod.rs` has no line to spare.

mod keys;
mod screen;
mod view;

pub(crate) use view::{TermJob, TermState};

#[cfg(test)]
#[path = "keys_tests.rs"]
mod keys_tests;
#[cfg(test)]
#[path = "screen_tests.rs"]
mod screen_tests;
#[cfg(test)]
#[path = "view_tests.rs"]
mod view_tests;
