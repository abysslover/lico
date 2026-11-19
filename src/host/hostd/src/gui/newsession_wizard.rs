//! The new-session wizard (`M22-P04b`, `M23-P03`): the one-page form's rows shared out over three steps. Step one is four icons (Claude, Codex, Local LLM and Terminal: the window draws them, see
//! `toolbar/picker.rs`) and nothing else; the Terminal icon has no step after it: it opens a shell at once, a terminal tab and not a session, in the current folder (`M23-P03a`); step two is the account and the folder (Local LLM: the server); step three is the options and the start. Every step ends with Back and
//! Next (Start on the last). The rows, the checks, the sign-in, the recent folders, the look for servers and the request are the form's own: nothing is done twice and no function of
//! the one-page form is lost (it stays as File > New session options...).

use super::super::super::controls::Action;
use super::*;

/// The blank lines the first step keeps for the icons (the window paints them over these).
const TILE_LINES: usize = 6;
const STEPS: usize = 3;
/// The icons of the first step: the three providers (the form's own `PROVIDERS`, at the same places) and the terminal, which needs no account and has no provider.
pub(super) const PICKS: [&str; 4] = ["claude", "codex", "local", "terminal"];

/// Where the wizard is: the step (`0` the icons, `1` the account and folder or the server, `2` the options and the start) and whether it is for a Home session (no folder).
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct Wiz {
    pub(crate) page: usize,
    pub(crate) home: bool,
}

/// One of the icons of the first step as the window draws it.
pub(crate) struct Tile {
    pub(crate) provider: &'static str,
    pub(crate) word: &'static str,
    pub(crate) enabled: bool,
    pub(crate) focused: bool,
    pub(crate) hovered: bool,
}

fn word(kind: &str) -> &'static str {
    match kind {
        "local" => "Local LLM",
        "terminal" => "Terminal",
        "codex" => "Codex",
        _ => "Claude",
    }
}

/// What a folder's own settings do (the line under the account and folder).
pub(super) fn settings_note(provider: &str) -> &'static str {
    if provider == "codex" {
        "A folder's own Codex settings (.codex: config, hooks, rules) are not used." // the thread is read-only, so Codex does not trust the folder
    } else {
        "A folder's own Claude settings (hooks, environment) are not used; its CLAUDE.md is."
    }
}

/// Another provider: its own account and the default model and effort, and what the form said about the program of the one before is not for it. True when it is Local LLM and no look has been
/// made yet (`M22-P04a`: choosing Local LLM is asking for its candidates, and a result that is there stays).
pub(super) fn set_provider(p: &mut Panel, f: &mut Form, to: &str) -> bool {
    if to == p.provider {
        return false;
    }
    p.provider = to.to_string();
    p.account = p.first_account(); // none for Local
    (f.model, f.effort) = (DEFAULT.into(), DEFAULT.into()); // the program's choices are claude's
    f.local = to == "local";
    p.notice.clear();
    f.local && !f.scanned
}

/// The step of the wizard a row is on. Exhaustive on purpose: a new row of the form must be put on a step.
fn page_of(field: Field) -> usize {
    match field {
        Field::Pick(_) | Field::Provider => 0,
        Field::Server | Field::Account | Field::Folder | Field::Recent(_) | Field::Worktree | Field::Branch | Field::SignIn | Field::Back | Field::Next => 1,
        Field::Name | Field::Model | Field::Effort | Field::Permission | Field::Start => 2,
    }
}

impl Panel {
    /// An icon is for a provider with an account, or for Local LLM and the terminal, which need none.
    fn tile_enabled(&self, kind: &str) -> bool {
        kind == "local" || kind == "terminal" || self.accounts.iter().any(|a| a.1 == kind)
    }
}

impl Form {
    /// The rows of a step: the one-page form's own rows that are on it, then Back and Next (the last step ends with Back and then the Start row). A Home session has no folder.
    pub(super) fn page_rows(&self, w: Wiz) -> Vec<Field> {
        if w.page == 0 {
            return (0..PICKS.len()).map(Field::Pick).collect();
        }
        let mut rows: Vec<Field> = self
            .all_rows()
            .into_iter()
            .filter(|r| page_of(*r) == w.page && !(w.home && matches!(r, Field::Folder | Field::Recent(_) | Field::Worktree | Field::Branch)))
            .collect();
        if w.page == 1 {
            rows.extend([Field::Back, Field::Next]);
        } else {
            rows.insert(rows.len().saturating_sub(1), Field::Back);
        }
        rows
    }

    /// The row a line of the form is: on the first step the lines are the room of the icons, which have targets of their own.
    pub(super) fn row_at(&self, line: usize) -> Option<Field> {
        if self.wiz.is_some_and(|w| w.page == 0) {
            return None;
        }
        self.fields().get(line).copied()
    }
}

/// The row that has the focus when a step is shown.
fn focus_of(p: &Panel, f: &Form, page: usize) -> Field {
    match page {
        0 => Field::Pick(PROVIDERS.iter().position(|k| *k == p.provider).unwrap_or(0)),
        1 if f.local => Field::Server,
        1 if f.wiz.is_some_and(|w| w.home) => Field::Account,
        1 => Field::Folder,
        _ => Field::Name,
    }
}

impl App {
    /// File > New session, Ctrl+N and the plus of the toolbar (`home`: the house, a Home session): the three icons first.
    pub(crate) fn form_open_wizard(&mut self, home: bool) {
        self.form_open(); // the accounts, the choices of the program, what the last session used: the form's own
        self.wizard_begin(home);
    }

    /// The form that is open becomes the wizard, on its first step with the focus on the icon of the provider it opened on, or else on the first one that can be chosen.
    pub(super) fn wizard_begin(&mut self, home: bool) {
        self.with_form(|p, f| {
            f.wiz = Some(Wiz { page: 0, home });
            if home {
                f.folder.clear();
                f.checked_folder = "\x00".to_string();
            }
            let at = PROVIDERS.iter().position(|k| *k == p.provider).unwrap_or(0);
            let first = if p.tile_enabled(PROVIDERS[at]) { at } else { PROVIDERS.iter().position(|k| p.tile_enabled(k)).unwrap_or(PROVIDERS.len() - 1) };
            f.focus = Field::Pick(first);
        });
        self.form_check_git();
        self.form_resume();
    }

    pub(super) fn wizard_page(&self) -> Option<usize> {
        self.ui.panel.as_ref().filter(|p| p.step == Step::NewSession).and_then(|p| p.form.as_ref()).and_then(|f| f.wiz).map(|w| w.page)
    }

    /// The icons of the first step; none on another step.
    pub(crate) fn wizard_tiles(&self) -> Option<Vec<Tile>> {
        let p = self.ui.panel.as_ref().filter(|p| p.step == Step::NewSession)?;
        let f = p.form.as_ref().filter(|f| f.wiz.is_some_and(|w| w.page == 0))?;
        Some(self.tiles_of(p, f))
    }

    fn tiles_of(&self, p: &Panel, f: &Form) -> Vec<Tile> {
        let hover = self.ui.toolbar_geom.tile_hover;
        PICKS.iter().enumerate().map(|(i, k)| Tile { provider: k, word: word(k), enabled: p.tile_enabled(k), focused: f.focus == Field::Pick(i), hovered: hover == Some(i) }).collect()
    }

    /// How many lines at the top of the body the window paints the icons over; none when the first step is not shown.
    pub(crate) fn wizard_tile_lines(&self) -> usize {
        if self.wizard_tiles().is_some() { TILE_LINES } else { 0 }
    }

    /// An icon was chosen (a click, Enter or Space): its provider is the form's, and the wizard goes on to the account and folder (Local LLM: the server, which is looked for at once). The
    /// icon of a provider with no account is not taken: it has the focus and says why. The Terminal icon goes on to nothing: the panel closes and a shell opens in the current folder.
    pub(super) fn wizard_pick(&mut self, i: usize) {
        let Some(kind) = PICKS.get(i).copied().filter(|_| self.wizard_page() == Some(0)) else { return };
        let usable = self.ui.panel.as_ref().is_some_and(|p| p.tile_enabled(kind));
        if !usable {
            let why = format!("{} needs an account first: the person icon in the toolbar adds one.", word(kind));
            self.with_form(|p, f| {
                f.focus = Field::Pick(i);
                p.notice = why;
            });
            self.dirty = true;
            return;
        }
        if kind == "terminal" {
            self.panel_close();
            self.terminal_open_here();
            return;
        }
        let mut look = false;
        self.with_form(|p, f| {
            look = set_provider(p, f, kind) || (kind == "local" && !f.scanned);
            if kind != "local" && p.chosen_account().is_none() {
                p.account = p.first_account(); // the accounts may have changed since the form opened
            }
            p.notice.clear();
        });
        self.wizard_go(1);
        if look {
            self.form_scan();
        }
    }

    /// Goes to a step: the input line gives its text back to its row first, and takes the text of the row that has the focus on the new step.
    fn wizard_go(&mut self, page: usize) {
        self.form_store();
        self.with_form(|p, f| {
            p.notice.clear();
            if let Some(w) = f.wiz.as_mut() {
                w.page = page;
            }
            f.focus = focus_of(p, f, page);
        });
        self.form_check_git();
        self.form_resume();
        self.dirty = true;
    }

    /// Next, from the account and folder (or the server) to the options: only when what the step shows is valid (the name is the last step's).
    pub(super) fn wizard_next(&mut self) {
        if self.wizard_page() != Some(1) {
            return;
        }
        self.form_store();
        self.form_resume(); // (the text stays on the input line while the step is shown)
        let problem = self.ui.panel.as_ref().and_then(|p| Some((p, p.form.as_ref()?))).and_then(|(p, f)| self.rows_problem(p, f));
        if let Some(why) = problem {
            if let Some(p) = self.ui.panel.as_mut() {
                p.notice = format!("Not yet: {why}.");
            }
            self.dirty = true;
            return;
        }
        self.wizard_go(2);
    }

    pub(super) fn wizard_back(&mut self) {
        if let Some(page) = self.wizard_page().filter(|n| *n > 0) {
            self.wizard_go(page - 1);
        }
    }

    /// Esc: a step back; false on the first step (and in the one-page form), where Esc is the panel's own and closes it.
    pub(super) fn wizard_escape(&mut self) -> bool {
        let back = self.wizard_page().is_some_and(|n| n > 0);
        if back {
            self.wizard_back();
        }
        back
    }

    /// Space and a click on the focused row of the wizard's own: an icon, Back or Next.
    pub(super) fn wizard_act(&mut self) {
        match self.focused() {
            Some(Field::Pick(i)) => self.wizard_pick(i),
            Some(Field::Back) => self.wizard_back(),
            Some(Field::Next) => self.wizard_next(),
            _ => {}
        }
    }

    /// Enter in the wizard: an icon is chosen, Back and Next do what they say, any other row of the account and folder step is Next, except the rows that have a meaning of their own
    /// (a recent folder is taken, the sign-in is made, the account is added, the look for servers is made while none is chosen). False: it is the form's Enter (on the last step it starts).
    pub(super) fn wizard_enter(&mut self) -> bool {
        let (Some(page), Some(row)) = (self.wizard_page(), self.focused()) else { return false };
        let adding = self.ui.panel.as_ref().is_some_and(|p| p.chosen_account().is_none());
        let chosen = self.ui.panel.as_ref().and_then(|p| p.form.as_ref()).is_some_and(|f| f.servers.get(f.server).is_some());
        match row {
            Field::Pick(_) | Field::Back | Field::Next => self.wizard_act(),
            Field::Recent(_) | Field::SignIn => return false,
            Field::Server if !chosen => return false,
            Field::Account if adding => return false,
            _ if page == 1 => self.wizard_next(),
            _ => return false,
        }
        true
    }

    /// A click on one of the painted icons (`Action::WizardPick`).
    pub(crate) fn wizard_click(&mut self, action: &Action) -> bool {
        let Action::WizardPick(i) = action else { return false };
        self.wizard_pick(*i);
        true
    }

    /// The text of the title: the step and what it asks for.
    pub(crate) fn form_title(&self, p: &Panel) -> String {
        let Some((w, f)) = p.form.as_ref().and_then(|f| Some((f.wiz?, f))) else { return "New session".into() };
        let name = match w.page {
            0 => "Choose",
            1 if f.local => "Local LLM server",
            1 if w.home => "Account",
            1 => "Account and folder",
            _ => "Options and start",
        };
        format!("{} - step {} of {STEPS} - {name}", if w.home { "New Home session" } else { "New session" }, w.page + 1)
    }

    /// The text of a row of the wizard's own.
    pub(super) fn wizard_value(&self, p: &Panel, f: &Form, field: Field) -> String {
        match field {
            Field::Pick(i) => word(PICKS[i % PICKS.len()]).into(),
            Field::Back => "Back  (Esc)".into(),
            _ => self.rows_problem(p, f).map_or("Next  (Enter)".into(), |why| format!("Next  (not yet: {why})")),
        }
    }

    /// The lines of a step. The first has no rows: blank lines for the icons, the word of the one the pointer or the keys are on, and, while an icon is grey, where an account comes from.
    pub(super) fn wizard_lines(&self, p: &Panel, f: &Form) -> Vec<String> {
        let Some(w) = f.wiz else { return Vec::new() };
        if w.page == 0 {
            let tiles = self.tiles_of(p, f);
            let mut lines = vec![" ".to_string(); TILE_LINES]; // (a space: an empty line would have no height)
            if let Some(t) = tiles.iter().find(|t| t.hovered).or_else(|| tiles.iter().find(|t| t.focused)) {
                lines.push(if t.enabled { t.word.to_string() } else { format!("{}: no account yet", t.word) });
            }
            let grey: Vec<&str> = tiles.iter().filter(|t| !t.enabled).map(|t| t.word).collect();
            if !grey.is_empty() {
                lines.push(String::new());
                lines.push(format!("{} {} an account first: the person icon in the toolbar adds one.", grey.join(" and "), if grey.len() == 1 { "needs" } else { "need" }));
            }
            return lines;
        }
        let mut lines: Vec<String> = f.fields().into_iter().map(|field| self.row_line(p, f, field)).collect();
        lines.push(String::new());
        lines.push(format!("Tab, Up, Down: move   Left, Right, Space or a click: change   Enter: {}", if w.page == 1 { "next" } else { "start" }));
        if f.local {
            if w.page == 1 {
                lines.extend(local::candidate_lines(f));
            }
            lines.extend(f.servers.get(f.server).and_then(local_rows::key_note));
            if w.page == 1 && f.servers.is_empty() && f.scan_req == 0 {
                lines.push(local_rows::empty_hint().into());
            }
            lines.push("A local session asks the model server on this machine only; it has no access to files or a shell.".into());
        } else if w.page == 1 {
            if w.home {
                lines.push("Home session: it gets a scratch workspace of its own, not a folder.".into());
            }
            lines.push(settings_note(&p.provider).into());
        }
        lines
    }

    /// What the state says of the wizard: the step, how many there are, whether it is a Home session, the name of the step.
    pub(super) fn wizard_json(&self, p: &Panel, f: &Form) -> Value {
        let Some(w) = f.wiz else { return Value::Null };
        json!({"page": w.page, "pages": STEPS, "home": w.home, "title": self.form_title(p), "name": self.form_title(p).rsplit(" - ").next().unwrap_or("")})
    }

    /// The icons of the first step as the state says them, with the rectangle the window drew each in.
    pub(super) fn tiles_json(&self) -> Value {
        let Some(tiles) = self.wizard_tiles() else { return Value::Null };
        Value::Array(
            tiles
                .iter()
                .enumerate()
                .map(|(i, t)| json!({"provider": t.provider, "icon": t.provider, "word": t.word, "enabled": t.enabled, "focus": t.focused, "hover": t.hovered, "rect": self.ui.toolbar_geom.tiles.get(i)}))
                .collect(),
        )
    }
}

#[cfg(test)]
#[path = "newsession_wizard_tests.rs"]
mod tests;
