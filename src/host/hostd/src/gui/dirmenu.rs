//! The right-click menu of the Directories list (`M24-P10a`): what the menu holds for a row, how it is placed and stepped through, and the window's part (opening, drawing, the clicks, the keys).

use cosmic_text::{Color, SwashCache};
use serde_json::{json, Value};
use winit::event::{ElementState, KeyEvent, MouseButton};
use winit::keyboard::{Key, NamedKey};

use super::panel::confirm::{Act, Ask};
use super::controls::{self, Action, Rect};
use super::panel::Step;
use super::App;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kind {
    OpenTerminal,
    NewFolder,
    Rename,
    Delete,
}

/// One line of the menu: what it does, its words, and why it cannot be used now (None: it can).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Item {
    pub kind: Kind,
    pub label: &'static str,
    pub reason: Option<&'static str>,
}

impl Item {
    pub(crate) fn enabled(&self) -> bool {
        self.reason.is_none()
    }
}

/// The open menu: where the pointer was, what it is about, its lines, the selected one, and where the last frame drew it (the window's own record: the tests and the hit boxes read it).
#[derive(Clone)]
pub struct DirMenu {
    pub at: (f32, f32),
    pub target: Target,
    pub items: Vec<Item>,
    pub sel: usize,
    pub rect: Rect,
    pub rects: Vec<Rect>,
}

/// What the menu is about: the folder its items work in, the folder the list shows (where the row stands) and the row the pointer was on (name and kind), if any.
#[derive(Clone, Debug, PartialEq, Default)]
pub(crate) struct Target {
    pub folder: String,
    pub parent: String,
    pub row: Option<(String, String)>,
}

impl Target {
    /// The path of the row itself (a link is the link, not what it points to), if the pointer was on one.
    pub(crate) fn entry(&self) -> Option<String> {
        self.row.as_ref().map(|(name, _)| child_path(&self.parent, name))
    }
}

/// What the window knows that decides which lines are usable.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Ctx {
    pub remote: bool,
    pub listed: bool,
    pub panel_open: bool,
    /// The pointer is on a row of the list (a folder, a file or a link), not on the empty space.
    pub on_row: bool,
    /// That row is the root, the home folder or a folder above it.
    pub protected: bool,
}

/// The path of `name` inside the folder `base` (a folder as the list shows it: with or without a closing slash; the root is `/`).
pub(crate) fn child_path(base: &str, name: &str) -> String {
    format!("{}/{name}", base.trim_end_matches('/'))
}

/// The target of a right click on `row` (name and kind) of the list that shows `cwd`: a folder row is the folder itself; a file, a link and the empty space mean the folder the list shows.
pub(crate) fn target_for(cwd: &str, row: Option<(&str, &str)>) -> Target {
    let folder = match row {
        Some((name, "dir")) => child_path(cwd, name),
        _ => cwd.to_string(),
    };
    Target { folder, parent: cwd.to_string(), row: row.map(|(name, kind)| (name.to_string(), kind.to_string())) }
}

/// The lines of the menu in the owner's order, each with the reason it cannot be used now: the machine first (the file operations are this machine's), then a folder listed, then the screen that is open (the lines that ask a question), then, for a line that works on a row, whether the pointer is on one and whether it may be touched.
pub(crate) fn items(ctx: &Ctx) -> Vec<Item> {
    let base = if ctx.remote { Some("this machine only") } else if !ctx.listed { Some("no folder here") } else { None };
    let asks = base.or(ctx.panel_open.then_some("close the open screen first"));
    let on_row = asks.or((!ctx.on_row).then_some("point at a file or folder")).or(ctx.protected.then_some("protected"));
    vec![
        Item { kind: Kind::OpenTerminal, label: "Open terminal", reason: base },
        Item { kind: Kind::NewFolder, label: "New folder", reason: asks },
        Item { kind: Kind::Rename, label: "Rename", reason: on_row },
        Item { kind: Kind::Delete, label: "Delete", reason: on_row },
    ]
}

pub(crate) fn first_enabled(items: &[Item]) -> Option<usize> {
    items.iter().position(Item::enabled)
}

/// The next usable line from `from` going down or up; `from` when there is none (the selection stops at the ends).
pub(crate) fn step(items: &[Item], from: usize, down: bool) -> usize {
    let mut at = from;
    loop {
        at = if down { at + 1 } else { match at.checked_sub(1) { Some(a) => a, None => return from } };
        match items.get(at) {
            Some(i) if i.enabled() => return at,
            Some(_) => {}
            None => return from,
        }
    }
}

/// The top left of a menu of `size` opened at `at`, moved to keep the whole of it inside `bounds` (its left and top edges win when it is bigger).
pub(crate) fn place(at: (f32, f32), size: (f32, f32), bounds: Rect) -> (f32, f32) {
    (at.0.min(bounds[2] - size.0).max(bounds[0]), at.1.min(bounds[3] - size.1).max(bounds[1]))
}

/// The title and lines of the question that Rename asks on the input line (the old name is on the line).
pub(super) fn rename_entry_view(ask: &Ask) -> (String, Vec<String>) {
    ("Rename".into(), vec![format!("Type the new name of \"{}\" and press Enter.", ask.title), format!("It stays in {}. A name that is taken is refused: nothing is replaced.", ask.cwd), "Escape leaves things as they are.".into()])
}

/// The title and lines of the question that Delete asks: what goes, that it cannot be undone, and the two answers (a click on a line or the key).
pub(super) fn delete_entry_view(ask: &Ask) -> (String, Vec<String>) {
    let what = if ask.detail.starts_with("a link") {
        "Only the link is removed; what it points to is not touched."
    } else if ask.detail.contains("folder") {
        "The folder and all that is in it are removed for good; a link inside goes as a link."
    } else {
        "The file is removed for good."
    };
    ("Delete".into(), vec![format!("Delete \"{}\" ({})? This cannot be undone.", ask.title, ask.detail), what.into(), format!("It is in {}. There is no Trash.", ask.cwd), String::new(), "Y  delete it".into(), "N  keep it".into()])
}

/// The title and lines of the question that New folder asks on the input line.
pub(super) fn new_folder_view(ask: &Ask) -> (String, Vec<String>) {
    ("New folder".into(), vec![format!("Type the name of the new folder in {} and press Enter.", ask.cwd), "Escape leaves things as they are.".into()])
}

impl App {
    /// What the lines depend on: the machine the list is of, whether it lists a folder of this machine, and whether another screen is open.
    fn dirmenu_ctx(&self, target: &Target) -> Ctx {
        let sb = &self.ui.sidebar;
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
        let protected = target.entry().is_some_and(|e| super::dirops::protected(std::path::Path::new(&e), home.as_deref()));
        Ctx { remote: sb.ssh_host.is_some(), listed: sb.cwd.starts_with('/') && !sb.unreachable && sb.error.is_none(), panel_open: self.ui.panel.is_some(), on_row: target.row.is_some(), protected }
    }

    pub(crate) fn dirmenu_close(&mut self) -> bool {
        let was = self.ui.sidebar.dirmenu.take().is_some();
        self.dirty |= was;
        was
    }

    /// A mouse button other than the left one. The right button over the list of Directories opens the menu there; any button while the menu is open closes it first (a right button then opens it again where it is now).
    /// True when the press was the menu's.
    pub(crate) fn dirmenu_button(&mut self, state: ElementState, button: MouseButton) -> bool {
        if state != ElementState::Pressed {
            return false;
        }
        let was = self.dirmenu_close();
        if button != MouseButton::Right {
            return was;
        }
        let (x, y) = (self.ui.mouse.0 as f32, self.ui.mouse.1 as f32);
        let in_list = self.ui.sidebar.geom.list_rect.is_some_and(|r| x >= r[0] && x < r[2] && y >= r[1] && y < r[3]);
        if !in_list || !self.sidebar_shown() || self.ui.sidebar.collapsed || self.ui.sidebar.tab != 0 || self.ui.menu_open.is_some() || self.ui.sidebar.newchat.is_some() {
            return was;
        }
        let row = match self.ui.controls.hit_at(x, y) {
            Some(Action::SidebarEntry(i)) => self.ui.sidebar.entries.get(i).map(|e| (e.name.clone(), e.kind.clone())),
            _ => None,
        };
        let target = target_for(&self.ui.sidebar.cwd, row.as_ref().map(|(n, k)| (n.as_str(), k.as_str())));
        let items = items(&self.dirmenu_ctx(&target));
        let sel = first_enabled(&items).unwrap_or(0);
        self.ui.sidebar.host_popup_open = false;
        self.ui.sidebar.nav.popup_open = false;
        self.ui.sidebar.dirmenu = Some(DirMenu { at: (x, y), target, items, sel, rect: [0.0; 4], rects: Vec::new() });
        self.dirty = true;
        true
    }

    /// A left click while the menu is open: on a line it runs it, anywhere else it closes the menu and goes no further. False when no menu is open.
    pub(crate) fn dirmenu_click(&mut self, hit: Option<Action>) -> bool {
        if self.ui.sidebar.dirmenu.is_none() {
            return false;
        }
        match hit {
            Some(Action::DirMenuItem(i)) => self.dirmenu_run(i),
            _ => {
                self.dirmenu_close();
            }
        }
        true
    }

    pub(crate) fn dirmenu_hover(&mut self, i: usize) -> bool {
        match self.ui.sidebar.dirmenu.as_mut() {
            Some(m) if m.items.get(i).is_some_and(Item::enabled) && m.sel != i => {
                m.sel = i;
                true
            }
            _ => false,
        }
    }

    /// The keys of the open menu: Up and Down over the usable lines, Enter and Space run the selected one, Escape closes; every other key is the menu's too while it is open.
    pub(crate) fn dirmenu_key(&mut self, event: &KeyEvent) -> bool {
        let Some(m) = self.ui.sidebar.dirmenu.as_mut() else { return false };
        match &event.logical_key {
            Key::Named(NamedKey::ArrowUp) => m.sel = step(&m.items, m.sel, false),
            Key::Named(NamedKey::ArrowDown) => m.sel = step(&m.items, m.sel, true),
            Key::Named(NamedKey::Escape) if !event.repeat => {
                self.dirmenu_close();
            }
            Key::Named(NamedKey::Enter | NamedKey::Space) if !event.repeat => {
                let sel = m.sel;
                self.dirmenu_run(sel);
            }
            _ => {}
        }
        self.dirty = true;
        true
    }

    /// Runs line `i`: the menu closes first; a greyed line says why instead.
    fn dirmenu_run(&mut self, i: usize) {
        let Some(menu) = self.ui.sidebar.dirmenu.take() else { return };
        self.dirty = true;
        let Some(item) = menu.items.get(i) else { return };
        if let Some(why) = item.reason {
            self.ui.flash = Some(format!("{}: {why}.", item.label));
            return;
        }
        match item.kind {
            Kind::OpenTerminal => self.terminal_open(Some(menu.target.folder)),
            Kind::NewFolder => self.new_folder_ask(menu.target.folder),
            Kind::Rename => self.entry_ask(Act::RenameEntry, &menu.target),
            Kind::Delete => self.entry_ask(Act::DeleteEntry, &menu.target),
        }
    }

    /// The question of Rename and of Delete about the row the menu was opened on: the name on the input line with the old one in it, or the confirmation that says what goes.
    fn entry_ask(&mut self, act: Act, target: &Target) {
        let (Some((name, _)), Some(path)) = (target.row.clone(), target.entry()) else { return };
        let detail = if act == Act::DeleteEntry {
            match super::dirops::describe(std::path::Path::new(&path)) {
                Ok(what) => what,
                Err(why) => {
                    self.ui.flash = Some(format!("Delete: {why}."));
                    self.sidebar_refresh();
                    return;
                }
            }
        } else {
            String::new()
        };
        let ask = Ask { act, session: String::new(), generation: 0, title: name.clone(), working: false, cwd: target.parent.clone(), worktree_branch: String::new(), worktree_repo: String::new(), worktree_exists: false, dirty_confirm: false, mb: 0, detail };
        self.panel_open(if act == Act::RenameEntry { Step::Rename } else { Step::Confirm });
        if let Some(p) = self.ui.panel.as_mut() {
            p.ask = Some(ask);
        }
        if act == Act::RenameEntry {
            self.ed.put(&name);
        }
    }

    /// Enter on the new name: the row is renamed in place and the list shows it; a name that cannot be used stays on the line with the reason.
    pub(crate) fn rename_entry_submit(&mut self, ask: &Ask, text: &str) {
        match super::dirops::rename(std::path::Path::new(&ask.cwd), &ask.title, text) {
            Ok(to) => {
                self.panel_close();
                self.ui.flash = Some(format!("Renamed to: {}", to.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
                self.sidebar_refresh();
            }
            Err(why) => {
                self.ed.put(text);
                if let Some(p) = self.ui.panel.as_mut() {
                    p.notice = format!("Not renamed: {why}.");
                }
            }
        }
    }

    /// Y on the question of Delete: the row goes (a link as a link), the list shows what is left, also when only a part could be removed.
    pub(crate) fn delete_entry_do(&mut self, ask: &Ask) -> Result<String, (String, String)> {
        let done = super::dirops::remove(std::path::Path::new(&child_path(&ask.cwd, &ask.title)));
        self.sidebar_refresh();
        done.map(|()| format!("Deleted: {}.", ask.title)).map_err(|why| (String::new(), why))
    }

    /// The question of New folder: the name is typed on the input line (the way a session is renamed, so the Korean input works the same), Enter makes the folder.
    fn new_folder_ask(&mut self, folder: String) {
        let ask = Ask { act: Act::NewFolder, session: String::new(), generation: 0, title: String::new(), working: false, cwd: folder, worktree_branch: String::new(), worktree_repo: String::new(), worktree_exists: false, dirty_confirm: false, mb: 0, detail: String::new() };
        self.panel_open(Step::Rename);
        if let Some(p) = self.ui.panel.as_mut() {
            p.ask = Some(ask);
        }
    }

    /// Enter on the name: the folder is made, the list shows it; a name that cannot be used stays on the line with the reason.
    pub(crate) fn new_folder_submit(&mut self, ask: &Ask, text: &str) {
        match super::dirops::make_dir(std::path::Path::new(&ask.cwd), text) {
            Ok(made) => {
                self.panel_close();
                self.ui.flash = Some(format!("Folder made: {}", made.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
                self.sidebar_refresh();
            }
            Err(why) => {
                self.ed.put(text);
                if let Some(p) = self.ui.panel.as_mut() {
                    p.notice = format!("Not made: {why}.");
                }
            }
        }
    }

    /// The menu in the window's dump.
    pub(crate) fn dirmenu_json(&self) -> Value {
        let Some(m) = self.ui.sidebar.dirmenu.as_ref() else { return json!({"open": false}) };
        let lines: Vec<Value> = m.items.iter().enumerate().map(|(i, it)| json!({"label": it.label, "enabled": it.enabled(), "reason": it.reason, "rect": m.rects.get(i)})).collect();
        json!({"open": true, "folder": m.target.folder, "row": m.target.row.as_ref().map(|(n, k)| json!([n, k])), "selected": m.sel, "rect": m.rect, "items": lines})
    }

    /// Draws the menu at the pointer, kept inside the left column and the window, and records where its lines are (for the clicks, the hover and the dump).
    pub(crate) fn draw_dirmenu(&mut self, buf: &mut [u32], width: usize, height: usize) {
        let Some(menu) = self.ui.sidebar.dirmenu.as_ref() else { return };
        let (at, items, sel) = (menu.at, menu.items.clone(), menu.sel);
        let s = self.ui.scale.max(1.0);
        let lh = self.line_h() * s;
        let column = self.effective_list_w(s) as f32;
        let Some(fs) = self.font_system.as_mut() else { return };
        let swash = self.ui.list_swash.get_or_insert_with(SwashCache::new);
        let controls = &mut self.ui.controls;
        let widest = items.iter().map(|i| controls.width(fs, i.label)).fold(0.0f32, f32::max);
        let size = (widest + 20.0 * s, items.len() as f32 * lh + 4.0 * s);
        let (x0, y0) = place(at, size, [0.0, 0.0, column.min(width as f32), height as f32]);
        let rect = [x0, y0, x0 + size.0, y0 + size.1];
        controls::fill(buf, width, height, [x0 - 1.0, y0 - 1.0, rect[2] + 1.0, rect[3] + 1.0], 0xFF3E_5268);
        controls::fill(buf, width, height, rect, 0xFF16_202D);
        let (ink, muted) = (Color::rgb(0xEC, 0xF2, 0xF8), Color::rgb(0x6E, 0x7C, 0x8E));
        let mut rects = Vec::with_capacity(items.len());
        for (i, item) in items.iter().enumerate() {
            let r = [x0 + 2.0 * s, y0 + 2.0 * s + i as f32 * lh, rect[2] - 2.0 * s, y0 + 2.0 * s + (i + 1) as f32 * lh];
            if i == sel && item.enabled() {
                controls::fill(buf, width, height, r, 0xFF28_4260);
            }
            controls.text(fs, swash, buf, width, height, r[0] + 8.0 * s, r[1], r[2] as i32, item.label, if item.enabled() { ink } else { muted });
            controls.target(r, Action::DirMenuItem(i));
            rects.push(r);
        }
        if let Some(m) = self.ui.sidebar.dirmenu.as_mut() {
            (m.rect, m.rects) = (rect, rects);
        }
    }
}

#[cfg(test)]
#[path = "dirmenu_tests.rs"]
mod tests;
