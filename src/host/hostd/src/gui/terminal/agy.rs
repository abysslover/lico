//! Whether the program `agy` is on this machine (`M23-P04c`, kept by `M25-P05`): the picture of New session is grey when it is not found. Since M25-P05 that picture starts a session of
//! Lico's (`newsession_wizard.rs`) and a click on the card of a conversation of `agy` brings it back as one (`sidebar_chats.rs`); this file only looks for the program, so that the person is told
//! before the start and not after it.

use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::super::App;

/// Whether a program named `agy` can be started here: an executable regular file (a link to one counts) in a folder of `path` (a `PATH`; an empty or a relative entry is not looked in) or in
/// `~/.local/bin` or `~/bin` of `home`, where `agy` installs itself and a launcher's `PATH` may not reach. A program the shell finds by a rule of its own (an alias, a function) is not found.
pub(crate) fn installed(path: Option<&OsStr>, home: Option<&Path>) -> bool {
    let mut dirs: Vec<PathBuf> = path.map(|p| std::env::split_paths(p).collect()).unwrap_or_default();
    if let Some(home) = home {
        dirs.extend([home.join(".local").join("bin"), home.join("bin")]);
    }
    dirs.iter().filter(|d| d.is_absolute()).any(|d| std::fs::metadata(d.join("agy")).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
}

/// `installed` for this window: its own `PATH` and home.
pub(crate) fn installed_here() -> bool {
    installed(std::env::var_os("PATH").as_deref(), std::env::var_os("HOME").map(PathBuf::from).as_deref())
}

impl App {
    /// Looks for `agy` (the picture of New session is grey when it is not found). The unit tests start with none found: what they say must not depend on the machine they run on.
    pub(crate) fn agy_look(&mut self) {
        let found = !cfg!(test) && installed_here();
        if let Some(p) = self.ui.panel.as_mut() {
            p.agy = found;
        }
    }
}
