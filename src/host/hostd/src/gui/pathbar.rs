//! The status bar across the bottom edge of the window (`M14`): the FULL path of what the person points at (a directory or file row of the sidebar, a chat card's folder), else of the
//! row the Files panel is on or the directory the sidebar shows, else the working directory of the active session; `host:path` on an ssh host. A path too long for the bar keeps its end
//! behind a leading ellipsis (the end of a path says most) and stays whole one click away: a click on the bar copies the whole text. The bar belongs to the window layer and uses
//! nothing but its state: it is drawn again only when the text it shows changes (a change of the row under the pointer, of the selection or of the session), never by a timer.

use serde_json::{json, Value};

use super::controls::{Action, Rect};
use super::sidebar_render::tail_path;
use super::{editor, App};

/// The columns kept at the right edge of a cut path for `click: copy`.
const MARK: &str = "click: copy";

#[derive(Default)]
pub(crate) struct PathBar {
    /// The path of the row under the pointer, kept so that a change of it (and only that) asks for a new frame.
    pointed: Option<String>,
    rect: Option<Rect>,
    hint_rect: Option<Rect>,
    telem_rect: Option<Rect>,
    cols: usize,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Source {
    Pointer,
    Selection,
    Session,
    None,
}

impl Source {
    fn name(self) -> &'static str {
        match self {
            Source::Pointer => "pointer",
            Source::Selection => "selection",
            Source::Session => "session",
            Source::None => "none",
        }
    }
}

/// `dir/name` (or `dir`), with `host:` in front on an ssh host. A root directory does not double its slash.
pub(super) fn path_text(host: Option<&str>, dir: &str, name: Option<&str>) -> String {
    if dir.is_empty() {
        return String::new();
    }
    let base = dir.trim_end_matches('/');
    let path = match name {
        Some(n) => format!("{base}/{n}"),
        None if base.is_empty() => "/".to_string(),
        None => base.to_string(),
    };
    host.filter(|h| !h.is_empty()).map_or(path.clone(), |h| format!("{h}:{path}"))
}

fn width(text: &str) -> usize {
    text.chars().map(|c| if editor::wide(c) { 2 } else { 1 }).sum()
}

/// What the bar draws of `text` in `cols` columns: the whole text, or its end behind `…` with room left for the mark; the second value says it was cut.
pub(super) fn fit(text: &str, cols: usize) -> (String, bool) {
    if width(text) <= cols {
        return (text.to_string(), false);
    }
    (tail_path(text, cols.saturating_sub(MARK.len() + 2).max(8)), true)
}

/// The text and where it came from: the pointer first, then the selection, then the session.
pub(super) fn pick(pointed: Option<String>, selected: Option<String>, session: Option<String>) -> (String, Source) {
    let keep = |s: Option<String>| s.filter(|t| !t.is_empty());
    if let Some(t) = keep(pointed) {
        return (t, Source::Pointer);
    }
    if let Some(t) = keep(selected) {
        return (t, Source::Selection);
    }
    keep(session).map_or((String::new(), Source::None), |t| (t, Source::Session))
}

impl App {
    /// The full path of the row the pointer is on: a row of the sidebar's directory list, or a chat card.
    fn path_pointed(&self) -> Option<String> {
        let sb = &self.ui.sidebar;
        match self.ui.controls.hit_at(self.ui.mouse.0 as f32, self.ui.mouse.1 as f32)? {
            Action::SidebarEntry(i) => sb.entries.get(i).map(|e| path_text(sb.ssh_host.as_deref(), &sb.cwd, Some(&e.name))),
            Action::SidebarChatEntry(i) => {
                let item = sb.chats.items.get(i)?;
                item.cwd.as_deref().map(|cwd| path_text(item.ssh_host.as_deref(), cwd, None))
            }
            _ => None,
        }
    }

    /// The row the Files panel is on, else the directory the sidebar shows.
    fn path_selected(&self) -> Option<String> {
        if let Some(ex) = self.ui.panel.as_ref().and_then(|p| p.files.as_ref()) {
            return Some(path_text(ex.ssh_host.as_deref(), &ex.cwd, ex.selected_entry().map(|e| e.name.as_str())));
        }
        let sb = &self.ui.sidebar;
        (self.sidebar_shown() && !sb.global_scope).then(|| path_text(sb.ssh_host.as_deref(), &sb.cwd, None))
    }

    /// The working directory of the active session (none for a Global session).
    fn path_session(&self) -> Option<String> {
        let row = self.ui.rows.iter().find(|r| r.session == self.session)?;
        (!row.cwd.is_empty()).then(|| path_text(Some(row.ssh_host.as_str()), &row.cwd, None))
    }

    fn path_now(&self) -> (String, Source) {
        pick(self.path_pointed(), self.path_selected(), self.path_session())
    }

    /// The pointer moved: true when the path under it changed and the bar has to be drawn again.
    pub(crate) fn path_hover_moved(&mut self) -> bool {
        let now = self.path_pointed();
        let changed = now != self.ui.path_bar.pointed;
        self.ui.path_bar.pointed = now;
        changed
    }

    /// A click on the bar copies the whole text.
    pub(crate) fn path_click(&mut self, action: &Action) -> bool {
        if *action != Action::PathCopy {
            return false;
        }
        let (text, _) = self.path_now();
        if !text.is_empty() {
            self.copy_text(text);
        }
        true
    }

    pub(super) fn path_bar_json(&self) -> Value {
        let (text, source) = self.path_now();
        let bar = &self.ui.path_bar;
        let (shown, cut) = fit(&text, bar.cols.max(20));
        json!({"text": text, "shown": shown, "cut": cut, "source": source.name(), "cols": bar.cols, "rect": bar.rect, "hint_rect": bar.hint_rect, "telem_rect": bar.telem_rect})
    }

    /// Draws the bar on the last row (`bar_y` to the bottom of the window).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_path_bar(&mut self, buf: &mut [u32], width: usize, height: usize, bar_y: f32, lh: f32, s: f32, telem: Option<Rect>) {
        let (text, _) = self.path_now();
        let char_w = (self.char_w() * s).max(1.0);
        let cols = (((width as f32) - 8.0 * s) / char_w).max(10.0) as usize;
        let (shown, cut) = fit(&text, cols);
        let bar = [0.0, bar_y, width as f32, height as f32];
        self.ui.path_bar.rect = Some(bar);
        self.ui.path_bar.hint_rect = Some([0.0, bar_y - lh, width as f32, bar_y]);
        self.ui.path_bar.telem_rect = telem;
        self.ui.path_bar.cols = cols;
        let (Some(fs), swash) = (self.font_system.as_mut(), self.ui.list_swash.get_or_insert_with(cosmic_text::SwashCache::new)) else { return };
        let controls = &mut self.ui.controls;
        super::controls::fill(buf, width, height, bar, 0xFF10_151C);
        super::controls::fill(buf, width, height, [0.0, bar_y, width as f32, bar_y + 1.0], 0xFF28_3444);
        let (ink, muted) = (cosmic_text::Color::rgb(0xC8, 0xD4, 0xE2), cosmic_text::Color::rgb(0x7C, 0x8C, 0xA0));
        controls.text(fs, swash, buf, width, height, 4.0 * s, bar_y + s, i32::MAX, &shown, ink);
        if cut {
            let mw = controls.width(fs, MARK);
            controls.text(fs, swash, buf, width, height, width as f32 - mw - 6.0 * s, bar_y + s, i32::MAX, MARK, muted);
        }
        controls.target(bar, Action::PathCopy);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_is_dir_slash_name_with_the_host_in_front_on_an_ssh_host() {
        assert_eq!(path_text(None, "/home/u/proj", Some("notes.txt")), "/home/u/proj/notes.txt");
        assert_eq!(path_text(None, "/home/u/proj/", None), "/home/u/proj");
        assert_eq!(path_text(Some("gpu-box"), "/srv/app", Some("src")), "gpu-box:/srv/app/src");
        assert_eq!(path_text(Some(""), "/srv/app", None), "/srv/app", "an empty host is no host");
        assert_eq!(path_text(None, "/", None), "/");
        assert_eq!(path_text(None, "/", Some("etc")), "/etc");
        assert_eq!(path_text(None, "", Some("x")), "", "no folder, no path");
    }

    #[test]
    fn a_path_that_does_not_fit_keeps_its_end_behind_an_ellipsis_with_wide_characters_counted_twice() {
        assert_eq!(fit("/home/u/proj", 40), ("/home/u/proj".to_string(), false));
        let long = "/home/u/\u{d55c}\u{ae00}\u{b85c}-\u{c544}\u{c8fc}-\u{ae34}-\u{b514}\u{b809}\u{d130}\u{b9ac}/\u{b450}\u{bc88}\u{c9f8}-\u{ae34}-\u{b514}\u{b809}\u{d130}\u{b9ac}/end";
        let (shown, cut) = fit(long, 30);
        assert!(cut && shown.starts_with('\u{2026}') && long.ends_with(&shown[3..]), "{shown}");
        assert!(width(&shown) + MARK.len() + 2 <= 30, "{shown}: room is left for the mark");
        assert_eq!(fit("short", 3).1, true);
    }

    #[test]
    fn the_pointer_wins_over_the_selection_and_the_selection_over_the_session() {
        let s = |t: &str| Some(t.to_string());
        assert_eq!(pick(s("/a"), s("/b"), s("/c")), ("/a".to_string(), Source::Pointer));
        assert_eq!(pick(None, s("/b"), s("/c")), ("/b".to_string(), Source::Selection));
        assert_eq!(pick(None, s(""), s("/c")), ("/c".to_string(), Source::Session));
        assert_eq!(pick(None, None, None), (String::new(), Source::None));
    }
}
