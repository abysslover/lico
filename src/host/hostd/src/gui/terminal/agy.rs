//! A conversation of `agy` brought back in a terminal (`M23-P04b`): the click on its card in Chats. The window types into a new shell what the person could type
//! (`agy --conversation ID`), so no new door is opened on the host; the id is checked again here before it is typed, and a terminal the window opened for the conversation that still
//! shows `agy` is shown instead of a second one.

use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::super::App;
use super::view::TermJob;

/// What the `agy` picture of New session types into the new shell: the program's name and Enter, nothing else (never its flag that skips permissions).
pub(crate) const START_LINE: &str = "agy\r";

/// What a click on the card of a conversation of `agy` does.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Plan {
    /// A terminal the window opened for it still shows `agy`: show that one.
    Show(String),
    /// A new terminal in `cwd` (the home when none), and the line typed into it.
    Open { cwd: Option<String>, line: String, conversation: String },
    /// Nothing is started: why.
    Refuse(&'static str),
}

/// Whether `id` is what `agy` calls a conversation and a shell can take as one word: letters, digits, `-` and `_`, at most 64, not starting with `-` (the host's rule for a provider's id).
pub(super) fn valid_conversation(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && !id.starts_with('-') && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

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

    /// The `agy` picture of New session was chosen (`M23-P04c`): a terminal in the folder on the left, as the Terminal picture opens one, with `agy` and Enter typed into it.
    pub(crate) fn agy_start(&mut self) {
        let cwd = self.terminal_start_dir();
        self.terminal_open_as(cwd, TermJob::OpenTyping { line: START_LINE.into(), conversation: None });
    }

    /// What a click on the card of `conversation`, which ran in `folder`, does (nothing is started here: `agy_open` does it).
    pub(super) fn agy_plan(&self, conversation: &str, folder: Option<&str>) -> Plan {
        if !valid_conversation(conversation) {
            return Plan::Refuse("That conversation cannot be opened here.");
        }
        if let Some(tab) = self.ui.terminal.tabs.iter().find(|t| t.conversation.as_deref() == Some(conversation) && t.screen.agent == Some("agy")) {
            return Plan::Show(tab.id.clone());
        }
        let cwd = folder.filter(|d| d.starts_with('/') && std::path::Path::new(d).is_dir()).map(str::to_string);
        Plan::Open { cwd, line: format!("agy --conversation {conversation}\r"), conversation: conversation.to_string() }
    }

    /// The click on the card of a conversation of `agy`.
    pub(crate) fn agy_open(&mut self, conversation: &str, folder: Option<&str>) {
        match self.agy_plan(conversation, folder) {
            Plan::Refuse(why) => self.ui.flash = Some(why.into()),
            Plan::Show(tab) => {
                self.panel_close();
                self.terminal_activate(&tab);
                self.ui.flash = Some("Switched to the terminal where it runs.".into());
            }
            Plan::Open { cwd, line, conversation } => {
                self.panel_close();
                self.terminal_open_as(cwd, TermJob::OpenTyping { line, conversation: Some(conversation) });
            }
        }
        self.dirty = true;
    }

    /// The terminal opened for a conversation is shown: its conversation is kept and the line typed into its shell.
    pub(super) fn agy_started(&mut self, tab: &str, line: String, conversation: Option<String>) {
        if let Some(t) = self.ui.terminal.tabs.iter_mut().find(|t| t.id == tab) {
            t.conversation = conversation;
        }
        self.terminal_send(line.into_bytes());
    }
}
