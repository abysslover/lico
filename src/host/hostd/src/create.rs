//! The tool `create` (`S04c`, extended in `M09-P02`): an agent asks to open a new `Claude` or `Codex` session tab, optionally with an initial prompt and optionally in
//! `"global"` scope. Like `send`, it does nothing at the call: the call leaves an approval card on the caller's row (`pending`), and the session is made once for each
//! human `allow`. Arbitrary `cwd`, `mode`, `account`, `model`, `effort`, and `from` arguments from the agent are ignored (`mode` is always `"auto"`, matching Lico's
//! standard bypass + in-window prompt-tool session mode). Same-provider creation inherits the caller's account, model, and effort; cross-provider creation (`Claude` -> `Codex`
//! or `Codex` -> `Claude`) resolves the target provider's matching default account and uses that provider's default model/effort. Every running agent-created session counts
//! against `PER_CALLER` (4) and `IN_ALL` (8), and advances chain depth (`MAX_HOPS` = 4) whether or not an initial prompt is sent.

use serde_json::{json, Map, Value};
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use crate::calls::{self, Window};
use crate::core::{self, Shared};
use crate::folders::{self, Settings};
use crate::leads;
use crate::pending::{self, Asked, Request};
use crate::session::State;
use crate::transfer::{self, By, MAX_HOPS};

/// Sessions of one caller that are running, and of all callers.
pub const PER_CALLER: usize = 4;
pub const IN_ALL: usize = 8;
/// What the name an agent gives may hold, so that the name with `made by` and the caller's label (cut to `MADE_BY_CHARS`) stays within `folders::MAX_NAME`.
pub const NAME_CHARS: usize = 36;
const MADE_BY_CHARS: usize = 12;
const FOLDER_CHARS: usize = 40;
const FOLDER_LEAST: usize = 16; // what the card shows of a folder at the least, when it has to give way
const DEFAULT: &str = "the program's default";
const STARTS: &str = "starts at once on the account's allowance";
const OPENS_READY: &str = "opens an empty ready tab";

/// What a new session is started with.
#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
    pub kind: &'static str,
    pub settings: Settings,
    pub account: Option<String>,
    pub mode: &'static str,
}

/// The tool `create`, for the session that holds the token.
pub fn create(shared: &Shared, caller: &str, arguments: &Map<String, Value>) -> Result<String, String> {
    crate::pending::sweep(shared, caller);
    call(shared, caller, arguments, calls::window(), Instant::now(), crate::pending::held_text())
}

/// What the model reads when the arguments are not a valid `create`.
fn needs() -> String {
    format!(
        "bad_request: create accepts optional `kind` ('claude' or 'codex'), `scope` ('folder' or 'global'), `prompt` (not only characters that draw nothing), and a plain `name` of at most {NAME_CHARS} characters"
    )
}

fn call(shared: &Shared, caller: &str, arguments: &Map<String, Value>, window: &Mutex<Window>, now: Instant, held: &Arc<AtomicUsize>) -> Result<String, String> {
    let done = ask(shared, caller, arguments, window, now, held);
    eprintln!("{}", audit_line(caller, &done.as_ref().map_or_else(|e| e.split(':').next().unwrap_or_default(), |_| "asked")));
    done
}

fn parse_optional_kind(value: Option<&Value>) -> Result<Option<&'static str>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.trim().is_empty() => Ok(None),
        Some(Value::String(s)) if s.trim().eq_ignore_ascii_case("claude") => Ok(Some("claude")),
        Some(Value::String(s)) if s.trim().eq_ignore_ascii_case("codex") => Ok(Some("codex")),
        Some(_) => Err(needs()),
    }
}

fn parse_optional_scope(value: Option<&Value>) -> Result<&'static str, String> {
    match value {
        None | Some(Value::Null) => Ok("folder"),
        Some(Value::String(s)) if s.trim().is_empty() || s.trim().eq_ignore_ascii_case("folder") => Ok("folder"),
        Some(Value::String(s)) if s.trim().eq_ignore_ascii_case("global") => Ok("global"),
        Some(_) => Err(needs()),
    }
}

fn encode_target(kind: &str, scope: &str, name: &str) -> String {
    format!("{kind}\t{scope}\t{name}")
}

fn decode_target(target: &str) -> (Option<&'static str>, &'static str, &str) {
    let mut parts = target.splitn(3, '\t');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(k), Some(sc), Some(name)) => {
            let kind = if k == "codex" { "codex" } else { "claude" };
            let scope = if sc == "global" { "global" } else { "folder" };
            (Some(kind), scope, name)
        }
        _ => (None, "folder", target),
    }
}

fn kind_word(caller_provider: &str, target_kind: &str) -> &'static str {
    match (caller_provider, target_kind) {
        (_, "codex") => "codex session",
        ("codex", "claude") => "claude session",
        _ => "session",
    }
}

fn shown_place(scope: &str, cwd: Option<&str>, folder_limit: usize) -> String {
    match (scope, cwd) {
        ("global", _) => "in global scope".to_string(),
        (_, Some(folder)) => format!("in '{}'", cut_start(folder, folder_limit)),
        (_, None) => "in a scratch folder".to_string(),
    }
}

fn resolve_target_choices(
    shared: &Shared,
    caller_provider: &str,
    caller_account: Option<&str>,
    caller_settings: &Settings,
    target_kind: &'static str,
) -> (Option<String>, Option<String>, Option<String>) {
    if target_kind == caller_provider {
        return (
            caller_account.map(str::to_string),
            caller_settings.model.clone(),
            caller_settings.effort.clone(),
        );
    }
    let accs = shared.accounts.lock().unwrap();
    let caller_ssh = caller_account.and_then(|id| accs.ssh_host(id));
    let snapshot = accs.list_snapshot();
    let picked = accs
        .default_for(target_kind)
        .filter(|a| a.ssh_host == caller_ssh)
        .map(|a| a.id.clone())
        .or_else(|| snapshot.iter().find(|a| a.provider == target_kind && a.ssh_host == caller_ssh).map(|a| a.id.clone()))
        .or_else(|| accs.default_for(target_kind).map(|a| a.id.clone()))
        .or_else(|| snapshot.iter().find(|a| a.provider == target_kind).map(|a| a.id.clone()));
    (picked, None, None)
}

fn ask(shared: &Shared, caller: &str, arguments: &Map<String, Value>, window: &Mutex<Window>, now: Instant, held: &Arc<AtomicUsize>) -> Result<String, String> {
    calls::counted(window, caller, now)?;
    let requested_kind = parse_optional_kind(arguments.get("kind"))?;
    let scope = parse_optional_scope(arguments.get("scope"))?;
    let (cleaned_prompt, removed) = match arguments.get("prompt") {
        None | Some(Value::Null) => (String::new(), 0),
        Some(Value::String(raw)) if raw.is_empty() => (String::new(), 0),
        Some(Value::String(raw)) => {
            let cleaned = leads::agent_text(raw);
            if cleaned.text.is_empty() || folders::shows_nothing(&cleaned.text) {
                return Err(needs());
            }
            (cleaned.text, cleaned.removed)
        }
        Some(_) => return Err(needs()),
    };
    let name = match arguments.get("name") {
        None | Some(Value::Null) => "",
        Some(Value::String(name)) => name.trim(),
        Some(_) => return Err(needs()),
    };
    if name.chars().count() > NAME_CHARS || (!name.is_empty() && (!folders::valid_name(name) || leads::agent_text(name).removed > 0)) {
        return Err(needs());
    }
    let name = plain(name);
    let found = core::find(shared, caller).ok_or("session_exited: this session has ended")?;
    let (caller_provider, parent_cwd, caller_account, hops, caller_settings) = {
        let s = found.lock().unwrap();
        let provider = if matches!(s.provider, "claude" | "codex") { s.provider } else { "claude" };
        (
            provider,
            s.settings.worktree_repo.clone().or_else(|| s.settings.cwd.clone()),
            s.account.clone(),
            s.incoming_hops,
            s.settings.clone(),
        )
    };
    if hops + 1 > MAX_HOPS {
        return Err(calls::refusal(shared, caller, "chain_depth"));
    }
    within_caps(shared, caller)?;
    let target_kind = requested_kind.unwrap_or(caller_provider);
    let cwd = if scope == "global" { None } else { parent_cwd };
    let (_, model, effort) = resolve_target_choices(shared, caller_provider, caller_account.as_deref(), &caller_settings, target_kind);
    let chosen = chosen_words_for(model.as_deref(), effort.as_deref());
    let kword = kind_word(caller_provider, target_kind);
    let shown = shown_place(scope, cwd.as_deref(), FOLDER_CHARS);
    let named = if name.is_empty() { String::new() } else { format!(" '{name}'") };
    let who = format!("creation of a new {kword}{named} {shown}");
    let prompt = cleaned_prompt.as_str();
    let card = card_of_spec(kword, scope, &name, cwd.as_deref(), &chosen, prompt, removed);
    let encoded_target = encode_target(target_kind, scope, &name);
    let (id, again) = match pending::ask(shared, caller, &Request { tool: "create", target: &encoded_target, who: &who, card: &card, text: prompt }, pending::now(), held)? {
        Asked::New(id) => (id, false),
        Asked::Again(id) => (id, true),
    };
    let mut reply = json!({
        "status": "pending", "approval_id": id,
        "message": format!("Nothing was made yet. The person has been asked to approve the {who}. Lico will tell you here when they decide; you can end your turn. A restart of Lico drops the request."),
    });
    if again {
        reply["already_pending"] = json!(true);
    }
    Ok(reply.to_string())
}

fn chosen_words_for(model: Option<&str>, effort: Option<&str>) -> String {
    match (model, effort) {
        (None, None) => "model: the program's default".to_string(),
        (m, e) => format!("model: {}, effort: {}", m.unwrap_or(DEFAULT), e.unwrap_or(DEFAULT)),
    }
}

#[cfg(test)]
fn chosen_words(settings: &Settings) -> String {
    chosen_words_for(settings.model.as_deref(), settings.effort.as_deref())
}

#[cfg(test)]
fn card_of(name: &str, cwd: Option<&str>, chosen: &str, prompt: &str, removed: usize) -> String {
    card_of_spec("session", "folder", name, cwd, chosen, prompt, removed)
}

fn card_of_spec(kword: &str, scope: &str, name: &str, cwd: Option<&str>, chosen: &str, prompt: &str, removed: usize) -> String {
    let length = |text: &str| text.chars().count();
    let action = if prompt.is_empty() { OPENS_READY } else { STARTS };
    let words = format!(" | mode: auto (bypass) | {chosen} | {action}");
    let head = |name: &str, folder: usize| {
        let named = if name.is_empty() { String::new() } else { format!(" '{name}'") };
        let shown = shown_place(scope, cwd, folder);
        format!("create {kword}{named} {shown}{words}")
    };
    let whole = head(name, FOLDER_CHARS);
    if !prompt.is_empty() && length(&whole) + length(&calls::card_tail(prompt, removed)) + 2 <= calls::CARD_CHARS {
        return calls::card_line(&whole, prompt, removed);
    }
    if length(&whole) <= calls::CARD_CHARS {
        return whole;
    }
    let bare = head("", FOLDER_CHARS);
    let room = calls::CARD_CHARS.saturating_sub(length(&bare) + 3);
    if room >= 2 {
        let kept: String = name.chars().take(room - 1).collect();
        return head(&format!("{}\u{2026}", kept.trim_end()), FOLDER_CHARS);
    }
    if length(&bare) <= calls::CARD_CHARS {
        return bare;
    }
    (FOLDER_LEAST..FOLDER_CHARS).rev().map(|count| head("", count)).find(|card| length(card) <= calls::CARD_CHARS).unwrap_or_else(|| head("", FOLDER_LEAST))
}

/// A name as the card shows it and the session gets it: the characters that would end the card's quote or split its fields (and a header) become `_`, the way a label does.
fn plain(name: &str) -> String {
    if name.is_empty() { String::new() } else { transfer::label("", Some(name), None) }
}

/// A folder cut from its start so that its end shows, in at most `count` characters.
fn cut_start(folder: &str, count: usize) -> String {
    let length = folder.chars().count();
    if length <= count {
        return folder.to_string();
    }
    format!("\u{2026}{}", folder.chars().skip(length - (count - 1)).collect::<String>())
}

/// How many running sessions were made by agents: (by `caller`, by anyone). A session that ended, was closed or was deleted is not running.
fn running_made(shared: &Shared, caller: &str) -> (usize, usize) {
    let all: Vec<_> = shared.sessions.read().unwrap().clone();
    let (mut mine, mut anyone) = (0, 0);
    for found in all {
        let s = found.lock().unwrap();
        if let (false, State::Running, Some(by)) = (s.purged, s.state, &s.made_by) {
            anyone += 1;
            mine += usize::from(by == caller);
        }
    }
    (mine, anyone)
}

/// Err: the refusal when `caller` may not have another session made, in the words the model reads.
fn within_caps(shared: &Shared, caller: &str) -> Result<(), String> {
    let (mine, anyone) = running_made(shared, caller);
    if mine >= PER_CALLER {
        return Err(format!("send_limit: {mine} sessions that you made are running; the limit is {PER_CALLER} for one session and {IN_ALL} in all; ask the person to close one, or wait until one has ended"));
    }
    if anyone >= IN_ALL {
        return Err(format!("send_limit: {anyone} sessions that agents made are running; the limit is {IN_ALL} in all and {PER_CALLER} for one session; ask the person to close one, or wait until one has ended"));
    }
    Ok(())
}

/// What the person's yes does: make the session, once.
pub(crate) fn perform(shared: &Arc<Shared>, caller: &str, target: &str, prompt: &str) -> Result<String, String> {
    perform_with(shared, caller, target, prompt, &|spec| {
        let made = core::session_create(shared, spec.kind, 0, 0, 0, spec.account.as_deref(), spec.mode, spec.settings.clone())?;
        Ok(made["session"].as_str().unwrap_or_default().to_string())
    })
}

/// The yes, with the start of the program as `start` (the host's own, or a stand-in in a test). One at a time, so that the caps checked here hold for the session made here.
fn perform_with(shared: &Arc<Shared>, caller: &str, target: &str, prompt: &str, start: &dyn Fn(&Spec) -> Result<String, &'static str>) -> Result<String, String> {
    let _one = creating().lock().unwrap_or_else(|e| e.into_inner());
    let done = make(shared, caller, target, prompt, start);
    eprintln!("{}", audit_line(caller, &done.as_ref().map_or_else(|e| e.split(':').next().unwrap_or_default(), |_| "created")));
    done
}

fn make(shared: &Arc<Shared>, caller: &str, target: &str, prompt: &str, start: &dyn Fn(&Spec) -> Result<String, &'static str>) -> Result<String, String> {
    let (requested_kind, scope, name) = decode_target(target);
    let found = core::find(shared, caller).filter(|found| {
        let s = found.lock().unwrap();
        !s.purged && s.state == State::Running
    });
    let Some(found) = found else { return Err(calls::refusal(shared, caller, "session_exited")) };
    let (caller_provider, parent_cwd, caller_account, hops, label, caller_settings) = {
        let s = found.lock().unwrap();
        let parent_cwd = s.settings.worktree_repo.clone().or_else(|| s.settings.cwd.clone());
        let provider = if matches!(s.provider, "claude" | "codex") { s.provider } else { "claude" };
        (
            provider,
            parent_cwd.clone(),
            s.account.clone(),
            s.incoming_hops,
            transfer::label(&s.id, s.settings.name.as_deref(), parent_cwd.as_deref()),
            s.settings.clone(),
        )
    };
    if hops + 1 > MAX_HOPS {
        return Err(calls::refusal(shared, caller, "chain_depth"));
    }
    within_caps(shared, caller)?;
    let target_kind = requested_kind.unwrap_or(caller_provider);
    let cwd = if scope == "global" { None } else { parent_cwd };
    let (account, model, effort) = resolve_target_choices(shared, caller_provider, caller_account.as_deref(), &caller_settings, target_kind);
    let made_by = label.chars().take(MADE_BY_CHARS).collect::<String>().trim_end().to_string();
    let composed = if name.is_empty() { format!("made by {made_by}") } else { format!("{name} - made by {made_by}") };
    let spec = Spec {
        kind: target_kind,
        settings: Settings { cwd: cwd.clone(), name: Some(composed.clone()), model, effort, ..Settings::default() },
        account,
        mode: "auto",
    };
    let id = start(&spec).map_err(start_words)?;
    let Some(new) = core::find(shared, &id) else { return Err(calls::refusal(shared, caller, "unknown_session")) };
    let place = match (scope, &cwd) {
        ("global", _) => "in global scope".to_string(),
        (_, Some(folder)) => format!("in '{folder}'"),
        (_, None) => "in a scratch folder".to_string(),
    };
    if prompt.is_empty() {
        {
            let mut s = new.lock().unwrap();
            s.made_by = Some(caller.to_string());
            s.incoming_hops = hops + 1;
            s.hops_set_at = crate::session::now_secs();
        }
        return Ok(format!(
            "created {target_kind} session {id} '{composed}' {place}, mode: auto (bypass); the new tab is open and ready (no initial prompt sent). Send to it with its id to talk to it."
        ));
    }
    let generation = {
        let mut s = new.lock().unwrap();
        s.made_by = Some(caller.to_string());
        s.generation as i64
    };
    match calls::with_create_prompt(|| transfer::send(shared, Some(caller), &id, "text", prompt, By::Session)) {
        Ok(done) => {
            let kword = kind_word(caller_provider, target_kind);
            Ok(format!(
                "created {kword} {id} '{composed}' {place}, mode: auto (bypass); the first prompt ({}) was delivered ({}). It is working on it; send to it with its id to talk to it.",
                transfer::size_text(prompt.len()),
                done["delivery"].as_str().unwrap_or("inline")
            ))
        }
        Err(code) => {
            let _ = core::session_purge(shared, &id, generation);
            Err(calls::refusal(shared, caller, code))
        }
    }
}

/// What the model reads when the program could not be started.
fn start_words(code: &str) -> String {
    let what = match code {
        "busy" => "the host could not start the session just now; try again in a moment",
        "unknown_account" => "the account of this session is not there any more",
        "option_unavailable" => "the model or the effort of this session is not offered by the program any more",
        folder if folder.starts_with("folder_") => "the folder of this session is not usable any more",
        _ => "the host refused it",
    };
    format!("{code}: {what}")
}

fn creating() -> &'static Mutex<()> {
    static ONE: OnceLock<Mutex<()>> = OnceLock::new();
    ONE.get_or_init(Mutex::default)
}

/// The one line the host logs for a call of `create`: the tool, the session and how it came out; never a word of the prompt or the name.
fn audit_line(session: &str, result: &str) -> String {
    format!("lico-host: call session={session} tool=create result={result}")
}

#[cfg(test)]
#[path = "create/tests.rs"]
mod tests;
