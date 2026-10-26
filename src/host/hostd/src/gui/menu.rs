//! The menu bar: File, Session, View, Help. Every entry is an action the window already has, with its key beside it; an entry that cannot act now is
//! greyed and says why. The keyboard (F10, Alt+letter, arrows, Enter, Esc) and the mouse reach the same entries.

use winit::event::KeyEvent;
use winit::keyboard::{Key, NamedKey};

use super::panel::Step;
use super::App;

pub const TITLES: [&str; 4] = ["File", "Session", "View", "Help"];

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Cmd {
    NewSession,
    Accounts,
    Quit,
    Next,
    Prev,
    Start,
    GoTo(usize),
    Allow,
    Deny,
    Resume,
    Copy,
    Paste,
    Older,
    Newer,
    Newest,
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
                Self::entry("Accounts", "Ctrl+A", Cmd::Accounts, None),
                Self::entry("Quit", "Ctrl+Q", Cmd::Quit, None),
            ],
            1 => {
                let two = if sessions >= 2 { panel_reason } else { Some("needs two sessions") };
                let mut v = vec![
                    Self::entry("Next session", "Ctrl+Tab", Cmd::Next, two),
                    Self::entry("Previous session", "Ctrl+Shift+Tab", Cmd::Prev, two),
                    Self::entry("Start screen", "Esc", Cmd::Start, panel_reason.or(none_open)),
                ];
                for (i, id) in self.ui.numbered.iter().enumerate().take(9) {
                    let row = self.ui.rows.iter().find(|r| &r.session == id);
                    let what = row.map_or(String::new(), |r| format!("{} / {}", r.provider, self.label_of(&r.account)));
                    let short: String = id.chars().take(7).collect();
                    v.push(Entry { label: format!("Session {}  {what}  {short}", i + 1), key: ["Alt+1", "Alt+2", "Alt+3", "Alt+4", "Alt+5", "Alt+6", "Alt+7", "Alt+8", "Alt+9"][i], cmd: Cmd::GoTo(i), reason: panel_reason });
                }
                let pending = if self.selected_row().map_or(false, |r| !r.approvals.is_empty()) { panel_reason } else { Some("no approval is pending") };
                v.push(Self::entry("Resume session", "Ctrl+R", Cmd::Resume, panel_reason.or(self.resume_reason())));
                v.push(Self::entry("Allow approval", "Y", Cmd::Allow, pending));
                v.push(Self::entry("Deny approval", "N", Cmd::Deny, pending));
                v.push(Self::entry("Copy selected lines", "Ctrl+Shift+C", Cmd::Copy, if self.ui.sel.is_some() { None } else { Some("nothing is selected") }));
                v.push(Self::entry("Paste", "Ctrl+V", Cmd::Paste, if self.paste_allowed() { None } else { Some("no text line to paste into") }));
                v
            }
            2 => vec![
                Self::entry("Older output", "Page Up", Cmd::Older, panel_reason.or(none_open)),
                Self::entry("Newer output", "Page Down", Cmd::Newer, panel_reason.or(none_open)),
                Self::entry("Newest output", "End", Cmd::Newest, panel_reason.or(none_open)),
            ],
            _ => vec![Self::entry("Keys", "", Cmd::Keys, None), Self::entry("About Lico", "", Cmd::About, None)],
        }
    }

    pub(crate) fn menu_close(&mut self) {
        if self.ui.menu_open.take().is_some() {
            self.dirty = true;
        }
    }

    pub(crate) fn menu_open_at(&mut self, menu: usize) {
        self.ui.menu_open = Some(menu);
        self.ui.menu_sel = self.menu_entries(menu).iter().position(|e| e.reason.is_none()).unwrap_or(0);
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
            Cmd::NewSession => self.form_open(),
            Cmd::Accounts => self.panel_open(Step::Accounts),
            Cmd::Quit => self.quit = true,
            Cmd::Next => self.step_session(1),
            Cmd::Prev => self.step_session(-1),
            Cmd::Start => self.go_start(),
            Cmd::GoTo(i) => {
                if let Some(id) = self.ui.numbered.get(i).cloned() {
                    if id != self.session {
                        self.switch_to(&id);
                    }
                }
            }
            Cmd::Allow => self.answer_approval(true),
            Cmd::Deny => self.answer_approval(false),
            Cmd::Resume => self.resume_session(),
            Cmd::Copy => self.copy_selection(),
            Cmd::Paste => self.start_paste(b"CLIPBOARD"),
            Cmd::Older => self.scroll_by(20),
            Cmd::Newer => self.scroll_by(-20),
            Cmd::Newest => {
                let back = self.ui.scroll as isize;
                self.scroll_by(-back);
            }
            Cmd::Keys => self.panel_open(Step::Keys),
            Cmd::About => self.panel_open(Step::About),
        }
    }

    /// F10 and Alt+letter open the bar, arrows move, Enter runs, Esc closes; Ctrl+Q quits. True when the key was taken.
    pub(crate) fn menu_key(&mut self, event: &KeyEvent) -> bool {
        let (ctrl, alt) = (self.ui.mods.control_key(), self.ui.mods.alt_key());
        if ctrl && !alt {
            if let Key::Character(c) = &event.logical_key {
                if c.eq_ignore_ascii_case("q") {
                    self.quit = true;
                    return true;
                }
            }
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
            Key::Character(c) if alt && !ctrl => {
                if let Some(i) = ["f", "s", "v", "h"].iter().position(|l| c.eq_ignore_ascii_case(l)) {
                    self.menu_open_at(i);
                    return true;
                }
            }
            _ => {}
        }
        let Some(open) = self.ui.menu_open else { return false };
        let entries = self.menu_entries(open);
        match &event.logical_key {
            Key::Named(NamedKey::Escape) => self.menu_close(),
            Key::Named(NamedKey::ArrowLeft) => self.menu_open_at((open + TITLES.len() - 1) % TITLES.len()),
            Key::Named(NamedKey::ArrowRight) => self.menu_open_at((open + 1) % TITLES.len()),
            Key::Named(NamedKey::ArrowDown) | Key::Named(NamedKey::ArrowUp) => {
                let step: isize = if event.logical_key == Key::Named(NamedKey::ArrowDown) { 1 } else { -1 };
                let n = entries.len() as isize;
                let mut i = self.ui.menu_sel as isize;
                for _ in 0..n {
                    i = (i + step).rem_euclid(n);
                    if entries[i as usize].reason.is_none() {
                        break; // entries that cannot act are skipped by the arrows
                    }
                }
                self.ui.menu_sel = i as usize;
                self.dirty = true;
            }
            Key::Named(NamedKey::Enter) => self.menu_run(open, self.ui.menu_sel),
            _ => {} // while a menu is open nothing else reaches the window
        }
        true
    }
}
