//! A conversation of `agy` brought back in a terminal (`M23-P04b`): the click on its card in Chats. The window types into a new shell what the person could type
//! (`agy --conversation ID`), so no new door is opened on the host; the id is checked again here before it is typed, and a terminal the window opened for the conversation that still
//! shows `agy` is shown instead of a second one.

use super::super::App;
use super::view::TermJob;

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

impl App {
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
