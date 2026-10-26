//! The new-session screen: one form that asks for everything a session needs (provider, account, folder, name, model, effort and
//! permission mode) with what the last session used already filled in. Rows are moved with Tab or Up and Down, a choice is changed
//! with Left and Right, Space or a click, the folder and the name are typed on the input line, and Enter starts the session. The
//! host decides what is valid; the form only says early what it can see without asking the disk (the folder is checked by the host).

use serde_json::{json, Value};
use winit::event::KeyEvent;
use winit::keyboard::{Key, NamedKey};

use super::super::editor::Parked;
use super::super::App;
use super::{Panel, Step};

const RECENT_SHOWN: usize = 5; // of the ten folders the host keeps
const DEFAULT: &str = "default";
const PROVIDERS: [&str; 2] = ["claude", "codex"];

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Field {
    Provider,
    Account,
    Folder,
    Recent(usize),
    Name,
    Model,
    Effort,
    Permission,
    SignIn,
    Start,
}

impl Field {
    fn label(self) -> &'static str {
        match self {
            Field::Provider => "Provider",
            Field::Account => "Account",
            Field::Folder => "Folder",
            Field::Recent(0) => "recent",
            Field::Name => "Name",
            Field::Model => "Model",
            Field::Effort => "Effort",
            Field::Permission => "Permission",
            Field::Recent(_) | Field::SignIn | Field::Start => "",
        }
    }

    fn takes_text(self) -> bool {
        matches!(self, Field::Folder | Field::Name)
    }
}

pub struct Form {
    pub focus: Field,
    pub folder: String, // the text of the two text rows while another row has the focus
    pub name: String,
    pub model: String, // "default" or one of `models`
    pub effort: String,
    pub models: Vec<String>, // what the program names (aliases), and the one the last session used
    pub efforts: Vec<String>,
    pub recent: Vec<String>,
}

impl Form {
    fn fields(&self) -> Vec<Field> {
        let mut rows = vec![Field::Provider, Field::Account, Field::Folder];
        rows.extend((0..self.recent.len().min(RECENT_SHOWN)).map(Field::Recent));
        rows.extend([Field::Name, Field::Model, Field::Effort, Field::Permission, Field::SignIn, Field::Start]);
        rows
    }
}

/// The value after `current` in a list that wraps round, or its first value when `current` is not in it.
fn next<T: PartialEq + Clone>(list: &[T], current: &T, by: isize) -> T {
    let at = list.iter().position(|x| x == current).unwrap_or(0) as isize;
    list[(at + by).rem_euclid(list.len() as isize) as usize].clone()
}

impl Panel {
    /// The focused row takes typed text, so the input line is live.
    pub fn text_field(&self) -> bool {
        self.step == Step::NewSession && self.form.as_ref().map_or(false, |f| f.focus.takes_text())
    }

    /// The account the form would use: one of the chosen provider's, not whatever text an id has.
    fn chosen_account(&self) -> Option<&(String, String, String, bool)> {
        self.accounts.iter().find(|a| a.0 == self.account && a.1 == self.provider)
    }

    /// The provider's default account, else its first, else none (an empty id: "add a new account").
    fn first_account(&self) -> String {
        let mine = || self.accounts.iter().filter(|a| a.1 == self.provider);
        mine().find(|a| a.3).or_else(|| mine().next()).map_or_else(String::new, |a| a.0.clone())
    }

    fn model_choices(&self, form: &Form, efforts: bool) -> Vec<String> {
        let offered = if self.provider != "claude" { &[][..] } else if efforts { &form.efforts[..] } else { &form.models[..] };
        std::iter::once(DEFAULT.to_string()).chain(offered.iter().cloned()).collect()
    }
}

/// The sentence for a word the host refused with.
fn refusal(code: &str) -> String {
    match code {
        "folder_not_absolute" => "That folder is not a full path.".into(),
        "folder_missing" => "That folder does not exist.".into(),
        "folder_not_directory" => "That path is not a folder.".into(),
        "folder_unreadable" => "That folder cannot be read.".into(),
        "folder_protected" => "Lico keeps its own files there: choose another folder.".into(),
        "option_unavailable" => "The installed program does not offer that model or effort.".into(),
        "bad_request" => "The host does not understand this request: it is an older build.".into(),
        other => other.to_string(),
    }
}

impl App {
    /// File > New session, Ctrl+N: the form, filled in from the host's last-used values and its recent folders.
    pub(crate) fn form_open(&mut self) {
        self.panel_open(Step::NewSession);
        self.load_accounts();
        let options = self.host_call_within(json!({"op": "session.options"}), 5);
        let list = |key: &str| -> Vec<String> { options[key].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect() };
        let last = &options["last"];
        let Some(p) = self.ui.panel.as_mut() else { return };
        let kind = match last["kind"].as_str() {
            Some(k) if PROVIDERS.contains(&k) => k,
            _ => PROVIDERS.iter().copied().find(|k| p.accounts.iter().any(|a| a.1 == *k)).unwrap_or(PROVIDERS[0]),
        };
        p.provider = kind.to_string();
        p.account = last["account"].as_str().filter(|id| p.accounts.iter().any(|a| a.0 == *id && a.1 == p.provider)).map_or_else(|| p.first_account(), str::to_string);
        p.mode = if last["mode"] == json!("default") { "default" } else { "auto" };
        let again = p.provider == "claude" && last["kind"] == json!("claude"); // the program's own choices exist for claude only
        let (mut models, mut efforts) = (list("models"), list("efforts"));
        let chosen = |key: &str, offered: &mut Vec<String>| match last[key].as_str() {
            Some(v) if again => {
                if !offered.iter().any(|o| o == v) {
                    offered.push(v.to_string());
                }
                v.to_string()
            }
            _ => DEFAULT.to_string(),
        };
        let (model, effort) = (chosen("model", &mut models), chosen("effort", &mut efforts));
        p.notice = match (options["ok"] == json!(1), options["issue"].as_str()) {
            (false, _) => format!("Could not ask the host for the choices: {}", options["error"].as_str().unwrap_or("unknown")),
            (true, Some("help_unreadable")) => "The claude program's own help could not be read: only the default model and effort are offered.".into(),
            (true, Some(_)) => "The claude program's help was not recognised: only the default model and effort are offered.".into(),
            _ => String::new(),
        };
        p.form = Some(Form { focus: Field::Folder, folder: last["cwd"].as_str().unwrap_or("").to_string(), name: String::new(), model, effort, models, efforts, recent: list("recent") });
        self.form_resume();
    }

    fn with_form<R>(&mut self, act: impl FnOnce(&mut Panel, &mut Form) -> R) -> Option<R> {
        let p = self.ui.panel.as_mut()?;
        let mut form = p.form.take()?;
        let result = act(p, &mut form);
        p.form = Some(form);
        Some(result)
    }

    /// The text of a text row: what is being typed when it has the focus on the form itself, else what it was left with.
    fn live(&self, p: &Panel, f: &Form, field: Field) -> String {
        match field {
            _ if f.focus == field && p.step == Step::NewSession => self.ed.text().to_string(),
            Field::Folder => f.folder.clone(),
            Field::Name => f.name.clone(),
            _ => String::new(),
        }
    }

    /// Why the session cannot be started yet, from what the form can see.
    fn form_problem(&self, p: &Panel, f: &Form) -> Option<String> {
        let folder = self.live(p, f, Field::Folder);
        if p.chosen_account().is_none() {
            Some("add an account first (Enter on the Account row)".into())
        } else if !folder.trim().is_empty() && !(folder.trim().starts_with('/') && crate::folders::valid_path_shape(folder.trim())) {
            Some("the folder must be a full path that starts with /".into())
        } else if !crate::folders::valid_name(self.live(p, f, Field::Name).trim()) {
            Some("the name has more than 60 characters".into())
        } else {
            None
        }
    }

    /// The rows of the form, one line each, then the line of keys.
    pub(crate) fn form_lines(&self, p: &Panel) -> Vec<String> {
        let Some(f) = p.form.as_ref() else { return Vec::new() };
        let mut lines: Vec<String> = f
            .fields()
            .into_iter()
            .map(|field| {
                let value = match field {
                    Field::Provider => format!("< {} >", p.provider),
                    Field::Account => p.chosen_account().map_or("< add a new account >".to_string(), |a| format!("< {}{} >", a.2, if a.3 { " (default)" } else { "" })),
                    Field::Folder | Field::Name => match self.live(p, f, field) {
                        t if t.is_empty() && field == Field::Folder => "(none: the session gets a scratch workspace of its own)".into(),
                        t if t.is_empty() => "(optional)".into(),
                        t => t,
                    },
                    Field::Recent(i) => f.recent[i].clone(),
                    Field::Model => format!("< {} >", f.model),
                    Field::Effort => format!("< {} >", f.effort),
                    Field::Permission => format!("< {} >", if p.mode == "auto" { "Skip agent prompts" } else { "Ask for approval" }),
                    Field::SignIn => "Sign in to this account first (Ctrl+L)".into(),
                    Field::Start => self.form_problem(p, f).map_or("Start session".into(), |why| format!("Start session (not yet: {why})")),
                };
                format!("{} {:<11}{value}", if f.focus == field { '>' } else { ' ' }, field.label())
            })
            .collect();
        lines.push(String::new());
        lines.push("Tab, Up, Down: move   Left, Right, Space or a click: change   Enter: start".into());
        lines
    }

    /// The form as the dump shows it (never the input line itself while a sign-in code is typed).
    pub(crate) fn form_json(&self) -> Value {
        let Some((p, f)) = self.ui.panel.as_ref().and_then(|p| Some((p, p.form.as_ref()?))) else { return Value::Null };
        json!({
            "focus": format!("{:?}", f.focus), "rows": f.fields().iter().map(|x| format!("{x:?}")).collect::<Vec<_>>(), "provider": p.provider, "account": p.account,
            "folder": self.live(p, f, Field::Folder), "name": self.live(p, f, Field::Name), "model": f.model, "effort": f.effort, "mode": p.mode,
            "models": f.models, "efforts": f.efforts, "recent": f.recent, "problem": self.form_problem(p, f),
        })
    }

    /// The input line takes the text of the focused row (and nothing on a row that takes none).
    fn form_resume(&mut self) {
        let text = self.ui.panel.as_ref().and_then(|p| p.form.as_ref().map(|f| (f, f.focus))).map(|(f, focus)| match focus {
            Field::Folder => f.folder.clone(),
            Field::Name => f.name.clone(),
            _ => String::new(),
        });
        self.ed.restore(Parked::typed(&text.unwrap_or_default()));
        self.preedit.clear();
        self.dirty = true;
    }

    /// The input line gives its text back to the row it belongs to; before any other screen takes the line.
    fn form_store(&mut self) {
        let text = self.ed.take().text();
        self.with_form(|_, f| match f.focus {
            Field::Folder => f.folder = text,
            Field::Name => f.name = text,
            _ => {}
        });
    }

    fn form_focus(&mut self, to: Field) {
        self.form_store();
        self.with_form(|_, f| f.focus = to);
        self.form_resume();
    }

    fn form_move(&mut self, by: isize) {
        let to = self.with_form(|_, f| {
            let rows = f.fields();
            let at = rows.iter().position(|x| *x == f.focus).unwrap_or(0) as isize;
            rows[(at + by).rem_euclid(rows.len() as isize) as usize]
        });
        if let Some(to) = to {
            self.form_focus(to);
        }
    }

    /// Left and Right (and Space and a click): the next or previous value of a choice.
    fn form_cycle(&mut self, by: isize) {
        self.with_form(|p, f| match f.focus {
            Field::Provider => {
                let to = next(&PROVIDERS.map(String::from), &p.provider, by);
                if to != p.provider {
                    p.provider = to;
                    p.account = p.first_account();
                    (f.model, f.effort) = (DEFAULT.into(), DEFAULT.into()); // the program's choices are claude's
                }
            }
            Field::Account => {
                let mut ids: Vec<String> = p.accounts.iter().filter(|a| a.1 == p.provider).map(|a| a.0.clone()).collect();
                ids.push(String::new());
                p.account = next(&ids, &p.account, by);
            }
            Field::Model => f.model = next(&p.model_choices(f, false), &f.model, by),
            Field::Effort => f.effort = next(&p.model_choices(f, true), &f.effort, by),
            Field::Permission => p.mode = if p.mode == "auto" { "default" } else { "auto" },
            _ => {}
        });
        self.dirty = true;
    }

    fn focused(&self) -> Option<Field> {
        self.ui.panel.as_ref().and_then(|p| p.form.as_ref()).map(|f| f.focus)
    }

    /// Space and a click on the focused row: a choice moves on, a folder is taken, the sign-in or the start is done.
    fn form_act(&mut self) {
        match self.focused() {
            Some(Field::Recent(_)) => self.form_pick(),
            Some(Field::SignIn) => self.form_sign_in(),
            Some(Field::Start) => self.form_start(),
            Some(Field::Provider | Field::Account | Field::Model | Field::Effort | Field::Permission) => self.form_cycle(1),
            _ => {}
        }
    }

    /// Enter: start the session, except where the row has its own meaning.
    fn form_enter(&mut self) {
        let adding = self.ui.panel.as_ref().map_or(false, |p| p.chosen_account().is_none());
        match self.focused() {
            Some(Field::Recent(_)) => self.form_pick(),
            Some(Field::SignIn) => self.form_sign_in(),
            Some(Field::Account) if adding => self.form_new_account(),
            _ => self.form_start(),
        }
    }

    /// A click on a line of the form; false when the line is not a row (the click is then a selection of text).
    pub(crate) fn form_click(&mut self, line: usize) -> bool {
        let row = self.ui.panel.as_ref().filter(|p| p.step == Step::NewSession).and_then(|p| p.form.as_ref()).and_then(|f| f.fields().get(line).copied());
        let Some(row) = row else { return false };
        self.form_focus(row);
        self.form_act();
        true
    }

    /// A recent folder is taken into the folder row.
    fn form_pick(&mut self) {
        let Some(Field::Recent(i)) = self.focused() else { return };
        self.with_form(|_, f| {
            f.folder = f.recent[i].clone();
            f.focus = Field::Folder;
        });
        self.form_resume();
    }

    fn form_new_account(&mut self) {
        self.form_store();
        if let Some(p) = self.ui.panel.as_mut() {
            p.step = Step::Label;
            p.notice.clear();
        }
        self.dirty = true;
    }

    /// The account was made: back to the form with it chosen, and the sign-in next.
    pub(crate) fn form_account_added(&mut self, account: String) {
        self.with_form(|p, f| {
            p.account = account;
            p.step = Step::NewSession;
            p.notice = "Account added. Sign in to it (Enter on the Sign in row), then start.".into();
            f.focus = Field::SignIn;
        });
        self.form_resume();
    }

    fn form_sign_in(&mut self) {
        if self.ui.panel.as_ref().map_or(true, |p| p.chosen_account().is_none()) {
            if let Some(p) = self.ui.panel.as_mut() {
                p.notice = "Choose an account, or add one, before signing in.".into();
            }
            return;
        }
        self.form_store(); // the line is a code's from here on, and the folder is not
        self.start_login("browser");
    }

    /// Back from the sign-in or the label entry to the form: an unfinished sign-in ends, a code that was typed is dropped.
    pub(crate) fn form_back(&mut self) {
        let signing_in = self.ui.panel.as_ref().filter(|p| p.step == Step::Login && !p.account.is_empty()).map(|p| p.account.clone());
        if let Some(account) = signing_in {
            self.host_call(json!({"op": "account.login.cancel", "account": account}));
        }
        self.ed.take();
        if let Some(p) = self.ui.panel.as_mut() {
            p.step = Step::NewSession;
            p.login = Value::Null;
            p.notice.clear();
            p.opened = false;
        }
        self.form_resume();
    }

    /// The sign-in succeeded in a form: back to it.
    pub(crate) fn form_signed_in(&mut self) {
        self.ed.take();
        if let Some(p) = self.ui.panel.as_mut() {
            p.step = Step::NewSession;
            p.notice = "Signed in.".into();
        }
        self.form_resume();
    }

    fn form_start(&mut self) {
        self.form_store();
        self.form_resume(); // (the text stays on the input line while the form is shown)
        let Some((problem, request)) = self.ui.panel.as_ref().and_then(|p| Some((p, p.form.as_ref()?))).map(|(p, f)| {
            let mut request = json!({"op": "session.create", "kind": p.provider, "account": p.account, "mode": p.mode});
            for (key, value) in [("cwd", f.folder.trim()), ("name", f.name.trim()), ("model", f.model.as_str()), ("effort", f.effort.as_str())] {
                if !value.is_empty() && value != DEFAULT {
                    request[key] = json!(value);
                }
            }
            (self.form_problem(p, f), request)
        }) else {
            return;
        };
        if let Some(why) = problem {
            if let Some(p) = self.ui.panel.as_mut() {
                p.notice = format!("Not started yet: {why}.");
            }
            return;
        }
        let reply = self.host_call(request);
        if reply["ok"] == json!(1) {
            let id = reply["session"].as_str().unwrap_or("").to_string();
            self.panel_close();
            self.send("{\"op\":\"status\"}");
            self.ui.scroll = 0;
            self.show(&id, None);
        } else if let Some(p) = self.ui.panel.as_mut() {
            p.notice = format!("The session was not started. {}", refusal(reply["error"].as_str().unwrap_or("unknown")));
        }
    }

    /// The keys of the form, and of the two screens it sends the person to (the label of a new account, the sign-in): true when taken.
    pub(crate) fn form_key(&mut self, event: &KeyEvent) -> bool {
        let Some(p) = self.ui.panel.as_ref().filter(|p| p.form.is_some()) else { return false };
        let (step, text_row) = (p.step, p.text_field());
        let (ctrl, shift) = (self.ui.mods.control_key(), self.ui.mods.shift_key());
        let key = &event.logical_key;
        match step {
            Step::Label | Step::Login if *key == Key::Named(NamedKey::Escape) => self.form_back(),
            Step::NewSession => {
                match key {
                    Key::Named(NamedKey::Escape) => return false, // the panel's own: close
                    Key::Named(NamedKey::Tab) => self.form_move(if shift { -1 } else { 1 }),
                    Key::Named(NamedKey::ArrowDown) => self.form_move(1),
                    Key::Named(NamedKey::ArrowUp) => self.form_move(-1),
                    Key::Named(NamedKey::ArrowLeft) if !text_row => self.form_cycle(-1),
                    Key::Named(NamedKey::ArrowRight) if !text_row => self.form_cycle(1),
                    Key::Named(NamedKey::Space) if !text_row && !event.repeat => self.form_act(),
                    Key::Named(NamedKey::Enter) if !event.repeat => self.form_enter(),
                    Key::Character(c) if ctrl && c.eq_ignore_ascii_case("l") && !event.repeat => self.form_sign_in(),
                    _ if text_row || ctrl => return false, // typing, the input method, paste and the other chords
                    _ => {}
                }
                self.dirty = true;
            }
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
#[path = "newsession_tests.rs"]
mod tests;
