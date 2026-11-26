//! The new-session wizard (`M22-P04b`, `M23-P03`): the one-page form's rows shared out over three steps. Step one is five icons (Claude, agy, Codex, Local LLM and Terminal: the window draws them, see
//! `toolbar/picker.rs`) and nothing else; the Terminal icon has no step after it: it opens a shell at once, a terminal tab and not a session, in the current folder (`M23-P03a`); step two is the account and the folder (Local LLM: the server); step three is the options and the start. Every step ends with Back and
//! Next (Start on the last). The rows, the checks, the sign-in, the recent folders, the look for servers and the request are the form's own: nothing is done twice and no function of
//! the one-page form is lost (it stays as File > New session options...).

use super::super::super::controls::Action;
use super::*;

/// The blank lines the first step keeps for the icons (the window paints them over these).
const TILE_LINES: usize = 6;
const STEPS: usize = 3;
/// The icons of the first step: the three providers (the form's own `PROVIDERS`), `agy` between Claude and Codex (a program of the person's started in a terminal, `M23-P04c`: no provider of a
/// session) and the terminal, which needs no account and has no provider.
pub(super) const PICKS: [&str; 5] = ["claude", "agy", "codex", "local", "terminal"];

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

/// One box the window paints over a row of the second or third step (`M22-P07`): a chip (one of a row's short values) or a card (a whole line).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CardItem {
    pub(crate) text: String,
    /// What is said after the text in a dimmer ink: a key, or why the box cannot be taken yet.
    pub(crate) hint: String,
    /// The picture at its left (the name of an `Icon`).
    pub(crate) icon: Option<&'static str>,
    pub(crate) chosen: bool,
    pub(crate) enabled: bool,
}

/// A row of a step as boxes: the line it covers, its word, whether the boxes are chips of one choice (the chosen one is filled) or one card, and whether the keys are on the row.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CardRow {
    pub(crate) line: usize,
    pub(crate) label: &'static str,
    pub(crate) chips: bool,
    pub(crate) focused: bool,
    pub(crate) items: Vec<CardItem>,
}

fn item(text: impl Into<String>, icon: Option<&'static str>, chosen: bool) -> CardItem {
    CardItem { text: text.into(), hint: String::new(), icon, chosen, enabled: true }
}

/// What a server's card says after its address: its kind, how many models, a key, the one used last.
fn server_hint(s: &Server) -> String {
    let line = local_rows::line(s);
    line.strip_prefix(&s.addr).unwrap_or(&line).trim().to_string()
}

fn word(kind: &str) -> &'static str {
    match kind {
        "local" => "Local LLM",
        "terminal" => "Terminal",
        "agy" => "agy",
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
    /// An icon is for a provider with an account, or for Local LLM and the terminal, which need none, or for `agy` when the program was found (it has no account of Lico's).
    fn tile_enabled(&self, kind: &str) -> bool {
        kind == "local" || kind == "terminal" || (kind == "agy" && self.agy) || self.accounts.iter().any(|a| a.1 == kind)
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
        0 => Field::Pick(PICKS.iter().position(|k| *k == p.provider).unwrap_or(0)),
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
        self.agy_look(); // is there an `agy` on this machine: looked for each time the wizard opens
        self.with_form(|p, f| {
            f.wiz = Some(Wiz { page: 0, home });
            if home {
                f.folder.clear();
                f.checked_folder = "\x00".to_string();
            }
            let kind = if PROVIDERS.contains(&p.provider.as_str()) && p.tile_enabled(&p.provider) { p.provider.as_str() } else { PROVIDERS.iter().copied().find(|k| p.tile_enabled(k)).unwrap_or("local") };
            f.focus = Field::Pick(PICKS.iter().position(|k| *k == kind).unwrap_or(0)); // (agy and the terminal are never the first focus of a form that opened on a provider)
        });
        self.form_check_git();
        self.form_resume();
    }

    /// A click, Enter or Space on a picture (`M24-P02`, one touch): the session is started at once when the form's own answers allow it (`one_touch`); Shift is the old way, step two.
    pub(super) fn wizard_touch(&mut self, i: usize) {
        let options = self.ui.mods.shift_key();
        self.pick(i, !options);
    }

    /// The old way into step two, which the tests of the steps use (`wizard_touch` with Shift held does the same in the window).
    #[cfg(test)]
    pub(super) fn wizard_pick(&mut self, i: usize) {
        self.pick(i, false);
    }

    /// Whether the session can be started from the form as the pick leaves it without asking the person anything: nothing for the form's own check to say, and an account that is not on an
    /// ssh host (the folder of this machine is no folder of that host, and that start is asynchronous with a notice of its own).
    fn one_touch_ready(&self) -> bool {
        self.ui.panel.as_ref().and_then(|p| Some((p, p.form.as_ref()?))).is_some_and(|(p, f)| !self.ui.ssh_hosts.contains_key(&p.account) && self.form_problem(p, f).is_none())
    }

    /// One touch (`M24-P02`): the folder is the one on the left (the rule of the Terminal and `agy` pictures; a Home session has none, and when the left lists no folder of this machine the form's own
    /// stays), then the start when `one_touch_ready`. A start the host refuses leaves the person on step two with the reason in the notice. True when a start was made.
    fn one_touch(&mut self) -> bool {
        let home = self.ui.panel.as_ref().and_then(|p| p.form.as_ref()).and_then(|f| f.wiz).is_some_and(|w| w.home);
        if let Some(dir) = self.terminal_start_dir().filter(|_| !home) {
            self.with_form(|_, f| f.folder = dir); // (the step the person may be shown next looks at the folder again: `wizard_go`)
        }
        if !self.one_touch_ready() {
            return false;
        }
        self.form_start();
        if self.wizard_page() == Some(0) {
            // a start that worked has closed the panel: this one did not, so the rows are shown with the reason the start left (the step clears the notice, so it is put back)
            let why = self.ui.panel.as_ref().map(|p| p.notice.clone()).unwrap_or_default();
            self.wizard_go(1);
            self.with_form(|p, _| p.notice = why);
        }
        true
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
    fn pick(&mut self, i: usize, touch: bool) {
        let Some(kind) = PICKS.get(i).copied().filter(|_| self.wizard_page() == Some(0)) else { return };
        let usable = self.ui.panel.as_ref().is_some_and(|p| p.tile_enabled(kind));
        if !usable {
            let why = if kind == "agy" { "agy is not installed on this machine.".to_string() } else { format!("{} needs an account first: the person icon in the toolbar adds one.", word(kind)) };
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
        if kind == "agy" {
            self.agy_session_start(); // a session of Lico's at once, no account and no other step (`M25-P05`)
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
        if touch && self.one_touch() {
            return;
        }
        self.wizard_go(1);
        if look {
            self.form_scan();
        }
    }

    /// The `agy` picture was chosen (`M25-P05`): a session of Lico's at once, in the folder the left lists (the rule of the Terminal picture and of `agy` since `M23-P04d`, the Home scope included;
    /// when the left lists none the host gives it a workspace of its own), in `auto` as every program starts (`M25-P09`: the host answers each request of the program with the one-shot yes and says so;
    /// the program's own skip flag is never passed; the Session menu switches to asking). It has no account and no other step. A start the host refuses leaves the person on the pictures with the reason in the notice.
    fn agy_session_start(&mut self) {
        let cwd = self.terminal_start_dir();
        let mut request = json!({"op": "session.create", "kind": "agy", "mode": "auto"});
        if let Some(dir) = &cwd {
            request["cwd"] = json!(dir);
        }
        let reply = self.host_call(request);
        self.finish_form_start(cwd, reply);
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
            Some(Field::Pick(i)) => self.wizard_touch(i),
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
        match action {
            Action::WizardPick(i) => self.wizard_touch(*i),
            Action::WizardChoose(line, i) => drop(self.wizard_choose(*line, *i)), // (a click beside the boxes of a row is no choice, and is not a click on the row's hidden text either)
            _ => return false,
        }
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
                lines.push(match (t.enabled, t.provider) {
                    (true, _) => t.word.to_string(),
                    (false, "agy") => format!("{}: not installed", t.word),
                    (false, _) => format!("{}: no account yet", t.word),
                });
            }
            let grey: Vec<&str> = tiles.iter().filter(|t| !t.enabled && t.provider != "agy").map(|t| t.word).collect();
            let no_agy = tiles.iter().any(|t| t.provider == "agy" && !t.enabled);
            if !grey.is_empty() || no_agy {
                lines.push(String::new());
            }
            if !grey.is_empty() {
                lines.push(format!("{} {} an account first: the person icon in the toolbar adds one.", grey.join(" and "), if grey.len() == 1 { "needs" } else { "need" }));
            }
            if no_agy {
                lines.push("agy is not installed on this machine.".into());
            }
            return lines;
        }
        let mut lines: Vec<String> = f.fields().into_iter().map(|field| self.row_line(p, f, field)).collect();
        lines.push(String::new());
        lines.push(String::new()); // (the one-page form says the keys here; the wizard shows boxes to click and keeps the line, which the places of the lines below count on)
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

    /// The rows of the second and third step as boxes (`M22-P07`): the account, the model, the effort, the permission and the worktree as chips, the recent folders, the servers, the sign-in,
    /// Back, Next and Start as cards. The text rows stay the model (the folder, the branch and the name are typed); the window paints these over the lines of the rows that have them. None on
    /// the first step and in the one-page form.
    pub(crate) fn wizard_cards(&self) -> Vec<CardRow> {
        let Some((p, f)) = self.ui.panel.as_ref().filter(|p| p.step == Step::NewSession).and_then(|p| Some((p, p.form.as_ref()?))) else { return Vec::new() };
        let Some(w) = f.wiz.filter(|w| w.page > 0) else { return Vec::new() };
        let rows = f.fields();
        let mut cards: Vec<CardRow> = rows.iter().enumerate().filter_map(|(line, field)| self.card_row(p, f, line, *field)).collect();
        if f.local && w.page == 1 {
            // the servers are the lines under the rows, the line of keys, a blank line and the heading (`candidate_click`'s own count)
            cards.extend(f.servers.iter().enumerate().map(|(i, s)| CardRow {
                line: rows.len() + 4 + i,
                label: "",
                chips: false,
                focused: f.focus == Field::Server && i == f.server,
                items: vec![CardItem { hint: server_hint(s), ..item(s.addr.clone(), Some("local"), i == f.server) }],
            }));
        }
        cards
    }

    fn card_row(&self, p: &Panel, f: &Form, line: usize, field: Field) -> Option<CardRow> {
        let focused = f.focus == field;
        let chips = |items: Vec<CardItem>| Some(CardRow { line, label: field.label(), chips: true, focused, items });
        let card = |item: CardItem| Some(CardRow { line, label: field.label(), chips: false, focused, items: vec![item] });
        let said = |text: &str, icon: &'static str, hint: String, problem: bool| card(CardItem { hint, enabled: !problem, ..item(text, Some(icon), false) });
        let words = |list: Vec<String>, chosen: &str| chips(list.into_iter().map(|t| item(t.clone(), None, t == chosen)).collect());
        match field {
            Field::Account => {
                let provider = ["claude", "codex"].into_iter().find(|k| *k == p.provider);
                let mut items: Vec<CardItem> =
                    p.accounts.iter().filter(|a| a.1 == p.provider).map(|a| item(format!("{}{}", self.acct_display(a), if a.3 { " (default)" } else { "" }), provider, a.0 == p.account)).collect();
                items.push(item("+ New account", Some("plus"), p.chosen_account().is_none()));
                chips(items)
            }
            Field::Recent(i) => card(item(f.recent[i].clone(), None, self.live(p, f, Field::Folder) == f.recent[i])),
            Field::Worktree => chips(vec![item("Use the folder", None, !f.worktree), item("New worktree", None, f.worktree)]),
            Field::SignIn => said("Sign in to this account", "account", "Ctrl+L".into(), false),
            Field::Back => said("Back", "back", "Esc".into(), false),
            Field::Next => {
                let why = self.rows_problem(p, f);
                said("Next", "forward", why.as_ref().map_or("Enter".into(), |w| format!("not yet: {w}")), why.is_some())
            }
            Field::Start => {
                let why = self.form_problem(p, f);
                said("Start session", "forward", why.as_ref().map_or("Enter".into(), |w| format!("not yet: {w}")), why.is_some())
            }
            Field::Server => {
                let text = if f.scan_req != 0 { "Looking for model servers..." } else if f.servers.is_empty() { "Look for model servers" } else { "Look again" };
                said(text, "reload", String::new(), false)
            }
            Field::Model if f.local => words(f.servers.get(f.server)?.models.clone(), &f.local_model),
            Field::Model => words(p.model_choices(f, false), &f.model),
            Field::Effort => words(p.model_choices(f, true), &f.effort),
            Field::Permission => chips(vec![item("Skip agent prompts", None, p.mode == "auto"), item("Ask for approval", None, p.mode != "auto")]),
            Field::Provider | Field::Folder | Field::Branch | Field::Name | Field::Pick(_) => None,
        }
    }

    /// A click on box `index` of the row at `line` (`Action::WizardChoose`): a chip sets its value, a card does what a click on its line did (a folder is taken, a server is chosen, Back, Next,
    /// the sign-in, Start). A second click on `+ New account` while it is the choice goes to adding one. False when there is no such box.
    pub(crate) fn wizard_choose(&mut self, line: usize, index: usize) -> bool {
        let cards = self.wizard_cards();
        let Some(row) = cards.iter().find(|r| r.line == line).filter(|r| index < r.items.len()) else { return false };
        if !row.chips {
            return self.form_click(line);
        }
        let Some(field) = self.ui.panel.as_ref().and_then(|p| p.form.as_ref()).and_then(|f| f.row_at(line)) else { return false };
        if field == Field::Account && row.items[index].chosen && index + 1 == row.items.len() {
            self.form_new_account();
            return true;
        }
        self.form_focus(field);
        self.with_form(|p, f| match field {
            Field::Account => p.account = p.accounts.iter().filter(|a| a.1 == p.provider).map(|a| a.0.clone()).chain(std::iter::once(String::new())).nth(index).unwrap_or_default(),
            Field::Worktree => f.worktree = index == 1,
            Field::Model if f.local => f.local_model = f.servers.get(f.server).and_then(|s| s.models.get(index)).cloned().unwrap_or_default(),
            Field::Model => f.model = p.model_choices(f, false).swap_remove(index),
            Field::Effort => f.effort = p.model_choices(f, true).swap_remove(index),
            Field::Permission => p.mode = if index == 0 { "auto" } else { "default" },
            _ => {}
        });
        self.form_check_git();
        self.dirty = true;
        true
    }

    /// What the state says of the boxes: each row's line, word and boxes with the rectangle the window drew each in.
    pub(super) fn cards_json(&self) -> Value {
        let rects = &self.ui.toolbar_geom.cards;
        Value::Array(
            self.wizard_cards()
                .iter()
                .map(|r| {
                    let drawn = rects.iter().find(|c| c.0 == r.line).map(|c| &c.1);
                    let items = r.items.iter().enumerate().map(|(i, x)| json!({"text": x.text, "hint": x.hint, "icon": x.icon, "chosen": x.chosen, "enabled": x.enabled, "rect": drawn.and_then(|d| d.get(i))}));
                    json!({"line": r.line, "label": r.label, "chips": r.chips, "focus": r.focused, "items": items.collect::<Vec<_>>()})
                })
                .collect(),
        )
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
#[cfg(test)]
#[path = "newsession_touch_tests.rs"]
mod touch_tests;
