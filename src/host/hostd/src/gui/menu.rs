//! The menu bar: File, Session, View, Help. Every entry is an action the window already has, with its key beside it; an entry that cannot act now is
//! greyed and says why. The keyboard (F10, Alt+letter, arrows, Enter, Esc) and the mouse reach the same entries.
//!
//! The menus are trees (`M22-P04a`): the entries of Session and View that the person has no fixed time to use are classified into groups (`Go to`, `This session`, `Share`, `Tools`, `Approvals`,
//! `Edit`, `Find`, `Scroll`, `Text size`, `Show`, `Settings`), each a heading that opens and closes in place; no entry was removed, and every key still works with the menu closed. `menu_entries`
//! stays the flat list of every entry (the rows of an open dropdown are `menu_rows`, headings and the entries of the groups that are open).

use winit::event::KeyEvent;
use winit::keyboard::{Key, NamedKey};

use super::panel::confirm::Act;
use super::panel::Step;
use super::ui::Row;
use super::App;

pub const TITLES: [&str; 4] = ["File", "Session", "View", "Help"];

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Cmd {
    NewSession,
    NewSessionOptions,
    NewGlobalSession,
    NewBrowser,
    NewLocalBrowser,
    ReopenTab,
    ForgetPages,
    NewTerminal,
    Accounts,
    Quit,
    Next,
    Prev,
    Start,
    GoTo(usize),
    Allow,
    Deny,
    Resume,
    Rebind,
    ToggleMode,
    Rename,
    Close,
    Delete,
    SendTo,
    HandOver,
    Copy,
    Cut,
    Paste,
    Older,
    Newer,
    Newest,
    FindConversation,
    SearchAll,
    Files,
    ToggleSidebar,
    RefreshFiles,
    NewInViewedDir,
    FilesPrevPage,
    FilesNextPage,
    ToggleHiddenFiles,
    CycleFilesHost,
    CaptureToInput,
    ToggleGroupBy,
    ToggleEvents,
    FontLarger,
    FontSmaller,
    FontReset,
    ToggleClipWrite,
    ToggleNotifications,
    FindLlm,
    ToggleNativeClaude,
    ToggleNativeCodex,
    ToggleVirtualDisplay,
    StopRealScreen,
    Keys,
    About,
}

pub struct Entry {
    pub label: String,
    pub key: &'static str,
    pub cmd: Cmd,
    pub reason: Option<&'static str>, // why it cannot act now
}

#[derive(Default)]
pub struct MenuGeom {
    pub titles: Vec<[f32; 4]>,
    pub items: Vec<[f32; 4]>,
    /// The groups that are open in the open dropdown (it opens with none).
    pub groups_open: Vec<&'static str>,
}

/// One row of an open dropdown: an entry (at `depth` 1 inside an open group), or the heading of a group.
pub struct MenuRow {
    pub label: String,
    pub key: &'static str,
    pub reason: Option<&'static str>, // why it cannot act now (a heading always can)
    pub depth: u8,
    pub kind: RowKind,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum RowKind {
    Entry(usize),           // the index in `menu_entries`
    Group(&'static str, bool), // the group and whether it is open
}

/// The group an entry is classified into, or none for an entry that stays on the dropdown itself.
pub(crate) fn group_of(cmd: Cmd) -> Option<&'static str> {
    Some(match cmd {
        Cmd::ReopenTab | Cmd::ForgetPages => "Browser pages",
        Cmd::Next | Cmd::Prev | Cmd::GoTo(_) => "Go to",
        Cmd::Resume | Cmd::Rebind | Cmd::ToggleMode | Cmd::Rename | Cmd::Close | Cmd::Delete => "This session",
        Cmd::SendTo | Cmd::HandOver => "Share",
        Cmd::FindLlm | Cmd::ToggleVirtualDisplay | Cmd::StopRealScreen => "Tools",
        Cmd::Allow | Cmd::Deny => "Approvals",
        Cmd::Copy | Cmd::Cut | Cmd::Paste => "Edit",
        Cmd::FindConversation | Cmd::SearchAll => "Find",
        Cmd::Older | Cmd::Newer | Cmd::Newest => "Scroll",
        Cmd::FontLarger | Cmd::FontSmaller | Cmd::FontReset => "Text size",
        Cmd::ToggleHiddenFiles | Cmd::ToggleGroupBy | Cmd::ToggleEvents => "Show",
        Cmd::ToggleClipWrite | Cmd::ToggleNotifications | Cmd::ToggleNativeClaude | Cmd::ToggleNativeCodex => "Settings",
        _ => return None,
    })
}

impl App {
    fn entry(label: &str, key: &'static str, cmd: Cmd, reason: Option<&'static str>) -> Entry {
        Entry { label: label.to_string(), key, cmd, reason }
    }

    /// The entries of one menu, each with what keeps it from acting now (if anything).
    pub(crate) fn menu_entries(&self, menu: usize) -> Vec<Entry> {
        let panel = self.ui.panel.is_some();
        let sessions = self.ui.numbered.len();
        let shown = !self.session.is_empty();
        let panel_reason = if panel { Some("close the open panel first") } else { None };
        let none_open = if shown { None } else { Some("no session is open") };
        match menu {
            0 => vec![
                Self::entry("New session", "Ctrl+N", Cmd::NewSession, None),
                Self::entry("New session options...", "", Cmd::NewSessionOptions, None),
                Self::entry("Accounts", "Ctrl+A", Cmd::Accounts, None),
                Self::entry("Quit", "Ctrl+Q", Cmd::Quit, None),
            ],
            1 => {
                let two = if sessions >= 2 { None } else { Some("needs two sessions") };
                let mut v = vec![
                    Self::entry("New Home session", "Ctrl+Shift+N", Cmd::NewGlobalSession, None),
                    Self::entry("New browser tab", "", Cmd::NewBrowser, panel_reason),
                    Self::entry("New local browser tab", "", Cmd::NewLocalBrowser, panel_reason),
                    Self::entry("New terminal tab", "", Cmd::NewTerminal, panel_reason),
                    Self::entry("Reopen closed tab", "", Cmd::ReopenTab, panel_reason),
                    Self::entry("Clear remembered pages", "", Cmd::ForgetPages, panel_reason),
                    Self::entry("Next session", "Ctrl+Tab", Cmd::Next, two),
                    Self::entry("Previous session", "Ctrl+Shift+Tab", Cmd::Prev, two),
                    Self::entry("Start screen", "Esc", Cmd::Start, panel_reason.or(none_open)),
                ];
                for (i, id) in self.ui.numbered.iter().enumerate().take(9) {
                    let row = self.ui.rows.iter().find(|r| &r.session == id);
                    let title = row.map_or_else(|| id.chars().take(7).collect(), Row::name);
                    let what = row.map_or(String::new(), |r| format!("{} / {}", r.provider, self.row_label(r)));
                    v.push(Entry { label: format!("Session {}  {title}  {what}", i + 1), key: ["Alt+1", "Alt+2", "Alt+3", "Alt+4", "Alt+5", "Alt+6", "Alt+7", "Alt+8", "Alt+9"][i], cmd: Cmd::GoTo(i), reason: None });
                }
                let pending = if self.approval_row_any_panel().is_some() { panel_reason.or(if self.answers_live() { None } else { Some("the card has just come up") }) } else { Some("no approval is pending") };
                let mode_label = match self.selected_row().map(|r| r.mode.as_str()) {
                    Some("default") => "Permission mode: approval",
                    _ => "Permission mode: auto",
                };
                v.push(Self::entry("Resume session", "Ctrl+R", Cmd::Resume, panel_reason.or(self.resume_reason())));
                v.push(Self::entry("Rebind account...", "Ctrl+Shift+B", Cmd::Rebind, panel_reason.or(self.rebind_reason())));
                v.push(Self::entry(mode_label, "Ctrl+Shift+M", Cmd::ToggleMode, panel_reason.or(self.mode_reason())));
                v.push(Self::entry("Rename session", "F2", Cmd::Rename, panel_reason.or(self.ask_reason(Act::Rename))));
                v.push(Self::entry("Close session", "Ctrl+Shift+W", Cmd::Close, panel_reason.or(self.ask_reason(Act::Close))));
                v.push(Self::entry("Delete session", "Ctrl+Delete", Cmd::Delete, panel_reason.or(self.ask_reason(Act::Delete))));
                v.push(Self::entry("Send to session...", "Ctrl+Shift+S", Cmd::SendTo, panel_reason.or(self.send_reason())));
                v.push(Self::entry("Hand over conversation...", "", Cmd::HandOver, panel_reason.or(self.hand_reason())));
                v.push(Self::entry("Find Local LLM servers...", "", Cmd::FindLlm, panel_reason));
                v.push(Self::entry("Allow approval", "Y", Cmd::Allow, pending));
                v.push(Self::entry("Deny approval", "N", Cmd::Deny, pending));
                let copy_label = if self.ui.sel.is_none() && self.ui.panel.is_none() && self.ed.has_selection() { "Copy selected text" } else { "Copy selected lines" };
                v.push(Self::entry(copy_label, "Ctrl+C", Cmd::Copy, if self.can_copy() { None } else { Some("nothing is selected") }));
                v.push(Self::entry("Cut selection", "Ctrl+X", Cmd::Cut, if self.can_cut() { None } else { Some("no prompt text is selected") }));
                v.push(Self::entry("Paste", "Ctrl+V", Cmd::Paste, if self.paste_allowed() { None } else { Some("no text line to paste into") }));
                let vd_label = match self.selected_row().filter(|r| !r.virtual_display.is_empty()) {
                    Some(r) => format!("Virtual display: stop ({})", r.virtual_display),
                    None => "Virtual display: start".into(),
                };
                v.push(Entry { label: vd_label, key: "", cmd: Cmd::ToggleVirtualDisplay, reason: panel_reason.or(self.virtual_display_reason()) });
                v.push(Self::entry(
                    "Stop real-screen control",
                    "Ctrl+Alt+Esc",
                    Cmd::StopRealScreen,
                    if self.ui.real_screen.is_empty() { Some("not active") } else { None },
                ));
                v
            }
            2 => {
                let fpx = self.font_px() as i64;
                let files_or_none = if panel && self.ui.panel.as_ref().is_some_and(|p| p.step != Step::Files) {
                    Some("close the open panel first")
                } else {
                    None
                };
                let mut v = vec![
                    Self::entry("Working directory files", "Ctrl+E", Cmd::Files, None),
                ];
                if self.sidebar_shown() {
                    v.push(Entry {
                        label: format!("Left sidebar: {}", if self.ui.sidebar.collapsed { "hidden" } else { "shown" }),
                        key: "Ctrl+B",
                        cmd: Cmd::ToggleSidebar,
                        reason: None,
                    });
                }
                v.extend([
                    Self::entry("Find in conversation", "Ctrl+F", Cmd::FindConversation, panel_reason.or(none_open)),
                    Self::entry("Search all sessions", "Ctrl+Shift+F", Cmd::SearchAll, panel_reason),
                    Self::entry("Older output", "Page Up", Cmd::Older, panel_reason.or(none_open)),
                    Self::entry("Newer output", "Page Down", Cmd::Newer, panel_reason.or(none_open)),
                    Self::entry("Newest output", "End", Cmd::Newest, panel_reason.or(none_open)),
                    Entry {
                        label: format!("Hidden files: {}", if self.ui.show_hidden { "shown" } else { "hidden" }),
                        key: "Ctrl+H",
                        cmd: Cmd::ToggleHiddenFiles,
                        reason: files_or_none,
                    },
                    Entry {
                        label: format!("Group sessions: {}", if self.ui.group_by_workspace { "workspace" } else { "account" }),
                        key: "Ctrl+Shift+G",
                        cmd: Cmd::ToggleGroupBy,
                        reason: panel_reason,
                    },
                    Entry {
                        label: format!("Internal events: {}", if self.ui.show_events { "shown" } else { "hidden" }),
                        key: "Ctrl+Shift+E",
                        cmd: Cmd::ToggleEvents,
                        reason: panel_reason,
                    },
                    Self::entry(
                        "Text size: larger",
                        "Ctrl+=",
                        Cmd::FontLarger,
                        panel_reason.or(if fpx >= 20 { Some("at maximum") } else { None }),
                    ),
                    Self::entry(
                        "Text size: smaller",
                        "Ctrl+-",
                        Cmd::FontSmaller,
                        panel_reason.or(if fpx <= 12 { Some("at minimum") } else { None }),
                    ),
                    Self::entry(
                        "Text size: 14 px (reset)",
                        "Ctrl+0",
                        Cmd::FontReset,
                        panel_reason.or(if fpx == 14 { Some("default size") } else { None }),
                    ),
                    Self::entry("Capture screen to input", "Ctrl+Shift+P", Cmd::CaptureToInput, panel_reason.or(self.capture_input_reason())),
                    Entry {
                        label: format!("Agent clipboard write: {}", if self.ui.clipboard_write { "on" } else { "off" }),
                        key: "",
                        cmd: Cmd::ToggleClipWrite,
                        reason: panel_reason,
                    },
                    Entry {
                        label: format!("Desktop notifications: {}", if self.ui.notifications_off { "off" } else { "on" }),
                        key: "",
                        cmd: Cmd::ToggleNotifications,
                        reason: panel_reason,
                    },
                    Entry { label: self.native_menu_label("claude"), key: "", cmd: Cmd::ToggleNativeClaude, reason: panel_reason },
                    Entry { label: self.native_menu_label("codex"), key: "", cmd: Cmd::ToggleNativeCodex, reason: panel_reason },
                ]);
                v
            }
            _ => vec![Self::entry("Keys", "", Cmd::Keys, None), Self::entry("About Lico", "", Cmd::About, None)],
        }
    }

    /// The rows of an open dropdown: each entry that is not in a group, and for each group its heading (where its first entry is) and, when it is open, its entries after it.
    pub(crate) fn menu_rows(&self, menu: usize) -> Vec<MenuRow> {
        let entries = self.menu_entries(menu);
        let row = |i: usize, depth: u8| MenuRow { label: entries[i].label.clone(), key: entries[i].key, reason: entries[i].reason, depth, kind: RowKind::Entry(i) };
        let mut rows: Vec<MenuRow> = Vec::new();
        let mut done: Vec<&'static str> = Vec::new();
        for (i, e) in entries.iter().enumerate() {
            match group_of(e.cmd) {
                None => rows.push(row(i, 0)),
                Some(g) if done.contains(&g) => {}
                Some(g) => {
                    done.push(g);
                    let open = self.ui.menu_geom.groups_open.contains(&g);
                    rows.push(MenuRow { label: g.to_string(), key: "", reason: None, depth: 0, kind: RowKind::Group(g, open) });
                    if open {
                        rows.extend((0..entries.len()).filter(|j| group_of(entries[*j].cmd) == Some(g)).map(|j| row(j, 1)));
                    }
                }
            }
        }
        rows
    }

    /// Opens or closes a group of the open dropdown and keeps the selection on its heading.
    pub(crate) fn menu_group_set(&mut self, menu: usize, group: &'static str, open: bool) {
        self.ui.menu_geom.groups_open.retain(|g| *g != group);
        if open {
            self.ui.menu_geom.groups_open.push(group);
        }
        self.ui.menu_sel = self.menu_rows(menu).iter().position(|r| r.kind == RowKind::Group(group, open)).unwrap_or(0);
        self.dirty = true;
    }

    /// A click or Enter on a row of the open dropdown: a heading opens or closes, an entry runs.
    pub(crate) fn menu_row_run(&mut self, menu: usize, row: usize) {
        match self.menu_rows(menu).get(row).map(|r| r.kind) {
            Some(RowKind::Group(g, open)) => self.menu_group_set(menu, g, !open),
            Some(RowKind::Entry(i)) => self.menu_run(menu, i),
            None => {}
        }
    }

    pub(crate) fn menu_close(&mut self) {
        if self.ui.menu_open.take().is_some() {
            self.dirty = true;
        }
    }

    pub(crate) fn menu_open_at(&mut self, menu: usize) {
        self.close_files_host_popup();
        self.ui.menu_open = Some(menu);
        self.ui.menu_geom.groups_open.clear(); // a dropdown opens with its groups closed
        self.ui.menu_sel = self.menu_rows(menu).iter().position(|r| r.reason.is_none()).unwrap_or(0);
        self.dirty = true;
    }

    /// Run an entry. One that cannot act says why on the first row instead.
    pub(crate) fn menu_run(&mut self, menu: usize, index: usize) {
        let entries = self.menu_entries(menu);
        let Some(entry) = entries.get(index) else { return };
        self.menu_close();
        match entry.reason {
            Some(reason) => self.ui.flash = Some(format!("{}: {reason}", entry.label.split("  ").next().unwrap_or(""))),
            None => self.run_cmd(entry.cmd),
        }
        self.dirty = true;
    }

    pub(crate) fn run_cmd(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::NewSession => self.form_open_wizard(false),
            Cmd::NewSessionOptions => self.form_open(),
            Cmd::NewGlobalSession => self.form_open_wizard(true),
            Cmd::NewBrowser => self.browser_prompt(false),
            Cmd::NewLocalBrowser => self.browser_prompt(true),
            Cmd::ReopenTab => self.browser_reopen(),
            Cmd::ForgetPages => self.browser_forget_pages(),
            Cmd::NewTerminal => self.terminal_open_here(),
            Cmd::Accounts => self.panel_open(Step::Accounts),
            Cmd::Quit => self.quit = true,
            Cmd::Next => {
                self.panel_close();
                self.step_session(1);
            }
            Cmd::Prev => {
                self.panel_close();
                self.step_session(-1);
            }
            Cmd::Start => self.go_start(),
            Cmd::GoTo(i) => {
                if let Some(id) = self.ui.numbered.get(i).cloned() {
                    self.panel_close();
                    if id != self.session {
                        self.switch_to(&id);
                    }
                }
            }
            Cmd::Allow => self.answer_approval(true),
            Cmd::Deny => self.answer_approval(false),
            Cmd::Resume => self.resume_session(),
            Cmd::Rebind => self.open_rebind(),
            Cmd::ToggleMode => self.toggle_session_mode(),
            Cmd::Rename => self.ask_open(Act::Rename),
            Cmd::Close => self.ask_open(Act::Close),
            Cmd::Delete => self.ask_open(Act::Delete),
            Cmd::SendTo => self.send_ask(),
            Cmd::HandOver => self.hand_ask(),
            Cmd::Copy => self.copy_selection(),
            Cmd::Cut => {
                let _ = self.cut_editor_selection();
            }
            Cmd::Paste => self.start_paste(b"CLIPBOARD"),
            Cmd::Older => self.scroll_by(20),
            Cmd::Newer => self.scroll_by(-20),
            Cmd::Newest => {
                let back = self.ui.scroll as isize;
                self.scroll_by(-back);
            }
            Cmd::FindConversation => self.open_search(false),
            Cmd::SearchAll => self.open_search(true),
            Cmd::Files => self.files_open(),
            Cmd::ToggleSidebar => self.toggle_sidebar_collapsed(),
            Cmd::RefreshFiles => self.files_refresh(),
            Cmd::NewInViewedDir => self.files_new_session_here(),
            Cmd::FilesPrevPage => self.files_step_page(-1),
            Cmd::FilesNextPage => self.files_step_page(1),
            Cmd::ToggleHiddenFiles => self.toggle_hidden_files(),
            Cmd::CycleFilesHost => self.toggle_files_host_popup(),
            Cmd::CaptureToInput => self.capture_to_input(),
            Cmd::ToggleGroupBy => self.toggle_group_by(),
            Cmd::ToggleEvents => self.toggle_show_events(),
            Cmd::FontLarger => self.step_font_px(1),
            Cmd::FontSmaller => self.step_font_px(-1),
            Cmd::FontReset => self.step_font_px(0),
            Cmd::ToggleClipWrite => self.toggle_clipboard_write(),
            Cmd::ToggleNotifications => self.toggle_notifications(),
            Cmd::FindLlm => self.llm_find_open(),
            Cmd::ToggleNativeClaude => self.toggle_native("claude"),
            Cmd::ToggleNativeCodex => self.toggle_native("codex"),
            Cmd::ToggleVirtualDisplay => self.toggle_virtual_display(),
            Cmd::StopRealScreen => self.stop_real_screen(),
            Cmd::Keys => self.panel_open(Step::Keys),
            Cmd::About => self.panel_open(Step::About),
        }
    }

    /// F10 and Alt+letter open the bar, arrows move, Enter runs, Esc closes; Ctrl+Q quits. True when the key was taken.
    pub(crate) fn menu_key(&mut self, event: &KeyEvent) -> bool {
        let (ctrl, alt) = (self.ui.mods.control_key(), self.ui.mods.alt_key());
        let letter = super::key_letter(event);
        if ctrl && !alt && letter == Some('q') {
            self.quit = true;
            return true;
        }
        if !self.ui.menu {
            return false;
        }
        match &event.logical_key {
            Key::Named(NamedKey::F10) => {
                if self.ui.menu_open.is_some() {
                    self.menu_close();
                } else {
                    self.menu_open_at(0);
                }
                return true;
            }
            _ if alt && !ctrl => {
                if let Some(i) = letter.and_then(|ch| ['f', 's', 'v', 'h'].iter().position(|&l| l == ch)) {
                    self.menu_open_at(i);
                    return true;
                }
            }
            _ => {}
        }
        let Some(open) = self.ui.menu_open else { return false };
        let rows = self.menu_rows(open);
        let entries = self.menu_entries(open);
        let on = rows.get(self.ui.menu_sel).map(|r| r.kind);
        let group_here = match on {
            Some(RowKind::Group(g, o)) => Some((g, o)),
            Some(RowKind::Entry(i)) if rows[self.ui.menu_sel].depth > 0 => group_of(entries[i].cmd).map(|g| (g, true)),
            _ => None,
        };
        match &event.logical_key {
            Key::Named(NamedKey::Escape) => self.menu_close(),
            Key::Named(NamedKey::ArrowLeft) => match group_here {
                Some((g, true)) => self.menu_group_set(open, g, false), // a group that is open closes (from its heading or from an entry in it)
                _ => self.menu_open_at((open + TITLES.len() - 1) % TITLES.len()),
            },
            Key::Named(NamedKey::ArrowRight) => match group_here {
                Some((g, false)) => self.menu_group_set(open, g, true),
                _ => self.menu_open_at((open + 1) % TITLES.len()),
            },
            Key::Named(NamedKey::ArrowDown) | Key::Named(NamedKey::ArrowUp) => {
                let step: isize = if event.logical_key == Key::Named(NamedKey::ArrowDown) { 1 } else { -1 };
                let n = rows.len() as isize;
                let mut i = self.ui.menu_sel as isize;
                for _ in 0..n {
                    i = (i + step).rem_euclid(n);
                    if rows[i as usize].reason.is_none() {
                        break; // entries that cannot act are skipped by the arrows
                    }
                }
                self.ui.menu_sel = i as usize;
                self.dirty = true;
            }
            Key::Named(NamedKey::Enter) => self.menu_row_run(open, self.ui.menu_sel),
            _ => {} // while a menu is open nothing else reaches the window
        }
        true
    }

    fn toggle_clipboard_write(&mut self) {
        let next = !self.ui.clipboard_write;
        let reply = self.host_call(serde_json::json!({"op": "settings.set", "clipboard_write": next}));
        if reply["ok"] == serde_json::json!(1) {
            self.ui.clipboard_write = reply["settings"]["clipboard_write"] == serde_json::json!(true);
            self.send("{\"op\":\"status\"}");
            self.ui.flash = Some(if self.ui.clipboard_write {
                "Agent clipboard write is on (each write still asks you).".into()
            } else {
                "Agent clipboard write is off.".into()
            });
        } else {
            self.ui.flash = Some("Could not change the clipboard setting.".into());
        }
    }

    fn toggle_notifications(&mut self) {
        let next = self.ui.notifications_off;
        let reply = self.host_call(serde_json::json!({"op": "settings.set", "notifications": next}));
        if reply["ok"] == serde_json::json!(1) {
            self.ui.notifications_off = reply["settings"]["notifications"] == serde_json::json!(false);
            self.send("{\"op\":\"status\"}");
            self.ui.flash = Some(if self.ui.notifications_off {
                "Desktop notifications are off.".into()
            } else {
                "Desktop notifications are on (when no window is open).".into()
            });
        } else {
            self.ui.flash = Some("Could not change the notification setting.".into());
        }
    }
}

#[cfg(test)]
#[path = "menu_tests.rs"]
mod tests;
