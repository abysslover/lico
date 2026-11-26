//! The theme in the window (`M24-P11d`, the owner's words of 2026-11-26): the state of the icon that goes round dark, light and system, what it asks of the host and of the desktop, and what it
//! does to the window when the look changes. The colours themselves are `theme.rs`.
//!
//! The choice is a view setting of the host (`settings.set` with `theme`), read again from every `status` answer like the text size. `system` is what the desktop says: winit's
//! `Window::theme()` where the window system tells it (it does not on X11), else `gsettings`, asked **off the window's thread** (a process start is some milliseconds): once when `system`
//! is chosen and then about every `ASK_EVERY` seconds while it stays chosen. An answer that says nothing keeps what was known; until one is known `system` is dark.

use std::process::{Command, Stdio};

use serde_json::{json, Value};

use super::ui::icons::Icon;
use super::theme::{self, Choice, Mode};
use super::App;

/// Seconds between two questions to the desktop while the choice is `system`.
pub(super) const ASK_EVERY: u32 = 30;
/// Seconds after which a question that has not come back is forgotten, so that one that hangs (a desktop that does not answer) does not stop the asking for good.
pub(super) const GIVE_UP: u32 = 4 * ASK_EVERY;

#[derive(Default)]
pub(crate) struct State {
    pub(super) choice: Choice,
    /// What the desktop said last: `Some(true)` dark, `Some(false)` light, none until it has said anything.
    pub(super) system_dark: Option<bool>,
    /// A question is on its way (one at a time).
    asking: bool,
    /// Seconds since the last question, counted only while the choice is `system`.
    quiet_secs: u32,
    /// How many questions to the desktop were started since the window began (the dump says it, and the tests count it).
    asked: u32,
}

impl State {
    /// What the window is painted in.
    pub(crate) fn mode(&self) -> Mode {
        theme::resolve(self.choice, self.system_dark)
    }

    /// The picture on the button: the state the window is in.
    pub(crate) fn icon(&self) -> Icon {
        match self.choice {
            Choice::Dark => Icon::Moon,
            Choice::Light => Icon::Sun,
            Choice::System => Icon::Auto,
        }
    }

    /// The word the button gives when the pointer is over it.
    pub(crate) fn label(&self) -> String {
        match self.choice {
            Choice::System => format!("Theme: system (now {})", self.mode().word()),
            choice => format!("Theme: {}", choice.word()),
        }
    }

    pub(crate) fn json(&self) -> Value {
        let system = match self.system_dark {
            Some(true) => "dark",
            Some(false) => "light",
            None => "unknown",
        };
        json!({"choice": self.choice.word(), "mode": self.mode().word(), "system": system, "asked": self.asked})
    }

    /// One second has passed: true when the desktop is to be asked now (the choice is `system`, the last question is `ASK_EVERY` seconds old and nothing is on its way, or what was on its way is
    /// older than `GIVE_UP`).
    pub(super) fn tick(&mut self) -> bool {
        if self.choice != Choice::System {
            self.quiet_secs = 0;
            return false;
        }
        self.quiet_secs = self.quiet_secs.saturating_add(1);
        self.quiet_secs >= ASK_EVERY && (!self.asking || self.quiet_secs >= GIVE_UP)
    }

    /// A question is about to be sent: false when one is on its way already (and not given up).
    pub(super) fn begin_ask(&mut self) -> bool {
        if self.asking && self.quiet_secs < GIVE_UP {
            return false;
        }
        (self.asking, self.quiet_secs) = (true, 0);
        self.asked = self.asked.saturating_add(1);
        true
    }

    /// The answer came: true when it changes what the desktop was known to say (an answer of none changes nothing).
    pub(super) fn answered(&mut self, dark: Option<bool>) -> bool {
        (self.asking, self.quiet_secs) = (false, 0);
        match dark {
            Some(d) if self.system_dark != Some(d) => {
                self.system_dark = Some(d);
                true
            }
            _ => false,
        }
    }
}

/// The request that keeps a choice.
pub(super) fn request(choice: Choice) -> Value {
    json!({"op": "settings.set", "theme": choice.word()})
}

/// What the desktop prefers, asked of `gsettings` (the keys of GNOME, which the other desktops of the same family keep too); none when it is not there or says nothing.
fn probe() -> Option<bool> {
    probe_with(|key| run("gsettings", &["get", "org.gnome.desktop.interface", key]))
}

/// The two questions, the colour scheme first and then the theme's name, put to `ask`.
fn probe_with(ask: impl Fn(&str) -> String) -> Option<bool> {
    theme::parse_system(&ask("color-scheme"), &ask("gtk-theme"))
}

/// What a program printed when it ended well; nothing when it is not there, ended badly or printed what is not text.
fn run(program: &str, args: &[&str]) -> String {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
        .unwrap_or_default()
}

/// Whether a theme of the window system is the dark one.
pub(super) fn is_dark(theme: winit::window::Theme) -> bool {
    theme == winit::window::Theme::Dark
}

impl App {
    /// A press on the icon: the next state, kept by the host. When the host does not answer the window still takes the look (it is a view), and says that it was not kept.
    pub(crate) fn cycle_theme(&mut self) {
        let next = self.ui.theme.choice.next();
        let reply = self.host_call(request(next));
        if reply["ok"] == json!(1) {
            self.sync_settings(&reply["settings"]);
            self.ui.flash = Some(format!("{}.", self.ui.theme.label()));
        } else {
            self.set_theme_choice(next);
            self.ui.flash = Some(format!("{} (not kept: the host did not answer).", self.ui.theme.label()));
        }
        self.dirty = true;
    }

    /// The choice the host keeps, from the settings it reports (no `theme` is dark).
    pub(crate) fn sync_theme(&mut self, settings: &Value) {
        let choice = settings["theme"].as_str().and_then(Choice::of_word).unwrap_or_default();
        if choice != self.ui.theme.choice {
            self.set_theme_choice(choice);
        }
    }

    fn set_theme_choice(&mut self, choice: Choice) {
        let before = self.ui.theme.mode();
        self.ui.theme.choice = choice;
        if choice == Choice::System {
            self.ask_system();
        }
        self.theme_changed(before);
    }

    /// The look or the picture changed: the window is drawn again, and from scratch when the colours moved (the kept frame is of the other look).
    fn theme_changed(&mut self, before: Mode) {
        if self.ui.theme.mode() != before {
            self.ui.composed = None;
        }
        self.dirty = true;
    }

    /// The desktop is asked: the window system's own answer when there is one, else `gsettings` on a thread of its own, whose answer comes back as `Ev::Theme`.
    fn ask_system(&mut self) {
        let said = self.window.as_ref().and_then(|w| w.theme()).map(is_dark);
        if let Some(proxy) = self.ask_with(said) {
            std::thread::spawn(move || {
                let _ = proxy.send_event(super::Ev::Theme(probe()));
            });
        }
    }

    /// The question apart from the window: what the window system said (`Some`) is the answer and nothing is asked of the desktop; else a question is begun unless one is on its way, and the proxy
    /// its answer is to come back through is given (none when the window has no loop to answer through: the question is then forgotten at once, so that the next is not held back).
    fn ask_with(&mut self, said: Option<bool>) -> Option<winit::event_loop::EventLoopProxy<super::Ev>> {
        if said.is_some() {
            self.on_system_theme(said);
            return None;
        }
        if !self.ui.theme.begin_ask() {
            return None;
        }
        let proxy = self.ui.proxy.clone();
        if proxy.is_none() {
            self.ui.theme.answered(None);
        }
        proxy
    }

    /// The window system said its theme changed.
    pub(crate) fn on_window_theme(&mut self, theme: winit::window::Theme) {
        self.on_system_theme(Some(is_dark(theme)));
    }

    /// What the desktop said (`None`: nothing, keep what was known).
    pub(crate) fn on_system_theme(&mut self, dark: Option<bool>) {
        let before = self.ui.theme.mode();
        if self.ui.theme.answered(dark) {
            self.theme_changed(before);
        }
    }

    /// Once a second: while the choice is `system`, the desktop is asked again every `ASK_EVERY` seconds, so that a change of the desktop reaches the window that stays open.
    pub(crate) fn theme_tick(&mut self) {
        if self.ui.theme.tick() {
            self.ask_system();
        }
    }
}

#[cfg(test)]
#[path = "theme_host_tests.rs"]
mod tests;
