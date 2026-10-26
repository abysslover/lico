//! Sessions in a folder the person picked: the checks of that folder, what a person chose for a session (folder,
//! name, model, effort), the list of recent folders, what the installed Claude Code program offers as models and
//! efforts, and the removal of a scratch workspace. The core only calls these functions.
//!
//! Nothing in a chosen folder is ever created or removed by the host. A scratch workspace (`workspace_root/<id>`,
//! made by the host for a session that was given no folder) is the only directory the host deletes, and only after
//! its name is checked to be a session id and the path to be a real directory, not a link.

use serde_json::{json, Map, Value};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use crate::core::{Reply, Shared};
use crate::session::{obj, Session};

pub const MAX_PATH: usize = 1024; // characters
pub const MAX_NAME: usize = 60; // characters
const RECENT: usize = 10;
const HELP_LIMIT: Duration = Duration::from_secs(3);
const HELP_GRACE: Duration = Duration::from_secs(1);

/// What a person chose for a session. Every field is optional; the default is "not chosen".
#[derive(Clone, Default, Debug, PartialEq)]
pub struct Settings {
    pub cwd: Option<String>,    // a folder the person picked (canonical path); none = a scratch workspace made by the host
    pub name: Option<String>,   // the name the person gave it; none = the default text
    pub model: Option<String>,  // `--model`; none = the program's default
    pub effort: Option<String>, // `--effort`; none = the program's default
}

/// A character a name or a folder path never holds: a control character, or one that draws nothing or reorders text
/// (the list `codex::dropped` removes from provider text), so a name read from a list cannot hide behind one.
fn refused_char(c: char) -> bool {
    c.is_control() || crate::codex::dropped(c as u32)
}

pub fn valid_path_shape(text: &str) -> bool {
    !text.is_empty() && text.chars().count() <= MAX_PATH && !text.chars().any(refused_char)
}

pub fn valid_name(text: &str) -> bool {
    text.chars().count() <= MAX_NAME && !text.chars().any(refused_char)
}

pub fn valid_model(text: &str) -> bool {
    // the first character is a letter or digit, so a value can never read as an option of the program (`-x`)
    text.len() <= 60 && text.bytes().next().is_some_and(|b| b.is_ascii_alphanumeric()) && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
}

pub fn valid_effort(text: &str) -> bool {
    !text.is_empty() && text.len() <= 16 && text.bytes().all(|b| b.is_ascii_lowercase())
}

impl Settings {
    /// The choices of a `session.create` request (its shape was checked by `ipc::valid_request`).
    pub fn of_request(request: &Map<String, Value>) -> Settings {
        let field = |key: &str| request.get(key).and_then(Value::as_str).filter(|v| !v.is_empty()).map(str::to_string);
        Settings { cwd: field("cwd"), name: field("name"), model: field("model"), effort: field("effort") }
    }

    /// The fields of the registry record: `scratch` is always written, the rest only when chosen.
    pub fn record_fields(&self) -> Map<String, Value> {
        let mut fields = obj(json!({"scratch": self.cwd.is_none()}));
        for (key, value) in [("cwd", &self.cwd), ("name", &self.name), ("chosen_model", &self.model), ("chosen_effort", &self.effort)] {
            if let Some(value) = value {
                fields.insert(key.to_string(), json!(value));
            }
        }
        fields
    }

    /// Read the fields back. A record without `scratch` is an older one: a scratch workspace. A field that is
    /// present with the wrong shape stops the load, like every other field of the registry.
    pub fn from_record(value: &Value, session: &str) -> Result<Settings, String> {
        let bad = |what: &str| format!("sessions.json has an invalid {what} for {session}");
        let scratch = match value.get("scratch") {
            None | Some(Value::Null) => true,
            Some(Value::Bool(b)) => *b,
            Some(_) => return Err(bad("scratch flag")),
        };
        let text = |key: &str, ok: fn(&str) -> bool| -> Result<Option<String>, String> {
            match value.get(key) {
                None | Some(Value::Null) => Ok(None),
                Some(Value::String(s)) if ok(s) => Ok(Some(s.clone())),
                Some(_) => Err(bad(key)),
            }
        };
        let cwd = text("cwd", |s| valid_path_shape(s) && Path::new(s).is_absolute())?;
        if scratch == cwd.is_some() {
            return Err(bad("folder"));
        }
        // A name stored before the hidden characters were refused loads without them; one holding a control character stops the load as before.
        let name = match value.get("name") {
            Some(Value::String(s)) if !s.chars().any(char::is_control) => {
                let kept: String = s.chars().filter(|c| !refused_char(*c)).collect();
                if valid_name(&kept) { Some(kept) } else { return Err(bad("name")) }
            }
            _ => text("name", valid_name)?,
        };
        Ok(Settings { cwd, name, model: text("chosen_model", valid_model)?, effort: text("chosen_effort", valid_effort)? })
    }
}

/// The folder a person picked, checked: absolute, present, a directory, listable, and neither equal to nor inside
/// (symbolic links resolved) any directory of the host's own state or the scratch workspaces. A folder that
/// contains them (a home folder) is allowed. Returns the canonical path.
pub fn check_folder(shared: &Shared, text: &str) -> Result<String, &'static str> {
    if !valid_path_shape(text) {
        return Err("bad_request");
    }
    let path = Path::new(text);
    if !path.is_absolute() {
        return Err("folder_not_absolute");
    }
    let canonical = std::fs::canonicalize(path).map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { "folder_missing" } else { "folder_unreadable" })?;
    if !canonical.is_dir() {
        return Err("folder_not_directory");
    }
    if std::fs::read_dir(&canonical).is_err() {
        return Err("folder_unreadable");
    }
    let c = &shared.cfg;
    for own in [&c.registry_dir, &c.log_dir, &c.accounts_dir, &c.cred_dir, &c.workspace_root] {
        let own = std::fs::canonicalize(own).unwrap_or_else(|_| own.to_path_buf());
        if canonical.starts_with(&own) {
            return Err("folder_protected");
        }
    }
    canonical.to_str().filter(|s| valid_path_shape(s)).map(str::to_string).ok_or("folder_unreadable")
}

/// What a `session.create` may ask for, checked before anything starts: the folder (and made canonical), and the
/// model and effort, which only a Claude Code session has (the efforts must be the ones the program lists).
pub fn prepare(shared: &Shared, kind: &str, mut chosen: Settings) -> Result<Settings, &'static str> {
    if let Some(folder) = &chosen.cwd {
        if kind == "synthetic" {
            return Err("option_unavailable");
        }
        chosen.cwd = Some(check_folder(shared, folder)?);
    }
    if chosen.model.is_some() || chosen.effort.is_some() {
        if kind != "claude" {
            return Err("option_unavailable");
        }
        if chosen.effort.as_ref().map_or(false, |e| !claude_help(shared).efforts.contains(e)) {
            return Err("option_unavailable");
        }
    }
    chosen.name = chosen.name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
    Ok(chosen)
}

/// The directory a provider process runs in: the chosen folder, checked again now (it may be gone, or may have become
/// a link into the host's state), else the session's scratch workspace, made if it is not there.
pub fn working_dir(shared: &Shared, session: &Arc<Mutex<Session>>) -> Result<PathBuf, &'static str> {
    let (id, cwd) = {
        let s = session.lock().unwrap();
        (s.id.clone(), s.settings.cwd.clone())
    };
    if let Some(folder) = cwd {
        return check_folder(shared, &folder).map(PathBuf::from);
    }
    let workspace = shared.cfg.workspace_root.join(id);
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&workspace).map_err(|_| "busy")?;
    Ok(workspace)
}

/// Remove the scratch workspace of a session. The path is built from the id alone, never read from a record, and
/// only a session without a chosen folder has one; a link is never followed.
pub fn remove_scratch(shared: &Shared, id: &str, scratch: bool) {
    remove_scratch_in(&shared.cfg.workspace_root, id, scratch);
}

fn remove_scratch_in(root: &Path, id: &str, scratch: bool) {
    if !scratch || !crate::registry::valid_session_id(id) {
        return;
    }
    let path = root.join(id);
    if std::fs::symlink_metadata(&path).map_or(false, |m| m.is_dir()) {
        let _ = std::fs::remove_dir_all(&path);
    }
}

/// A session backed by a provider program (or its simulator): one process, one provider session, running in the chosen
/// folder or in a scratch workspace of its own. Nothing of one that never became ready remains: no process, no log, and
/// no workspace the host made.
pub fn start_provider_session(shared: &Arc<Shared>, kind: &str, bound: Option<(String, u64)>, mode: &'static str, chosen: Settings) -> Result<Arc<Mutex<Session>>, &'static str> {
    let id = format!("s{}", crate::core::random_hex(8));
    let session = Arc::new(Mutex::new(Session::new(id, false, &shared.cfg.log_dir, shared.cfg.retain, shared.cfg.window)));
    {
        let mut s = session.lock().unwrap();
        s.text_mode = true;
        s.mode = mode;
        s.provider = if kind == "claude" { "claude" } else { "codex" };
        s.settings = chosen;
        if let Some((account, credential_generation)) = bound {
            s.account = Some(account);
            s.credential_generation = credential_generation;
        }
    }
    let started = if kind == "claude" { crate::claude::start(shared, &session) } else { crate::codex::start(shared, &session) };
    if let Err(code) = started {
        let mut s = session.lock().unwrap();
        s.purged = true;
        s.stop_process();
        s.purge_log();
        remove_scratch(shared, &s.id, s.settings.cwd.is_none());
        return Err(code);
    }
    shared.sessions.write().unwrap().push(session.clone());
    crate::registry::save(shared);
    let last = {
        let s = session.lock().unwrap();
        last_of(s.provider, s.account.as_deref(), s.mode, &s.settings)
    };
    remember(shared, last);
    Ok(session)
}

/// What the status entry of a session says about its choices.
pub fn annotate(entry: &mut Value, s: &Session) {
    entry["scratch"] = json!(s.settings.cwd.is_none());
    for (key, value) in [("cwd", &s.settings.cwd), ("name", &s.settings.name), ("chosen_model", &s.settings.model), ("chosen_effort", &s.settings.effort)] {
        if let Some(value) = value {
            entry[key] = json!(value);
        }
    }
}

/// Give a session a name, or take it away again (an empty name). Nothing the agent sees changes.
pub fn rename(shared: &Arc<Shared>, session: &str, name: &str) -> Reply {
    let found = crate::core::find(shared, session).ok_or("unknown_session")?;
    let shown = {
        let mut s = found.lock().unwrap();
        if s.purged {
            return Err("unknown_session");
        }
        s.settings.name = Some(name.trim().to_string()).filter(|n| !n.is_empty());
        s.settings.name.clone()
    };
    crate::registry::save(shared);
    Ok(obj(json!({"session": session, "name": shown})))
}

// ----------------------------------------------------------------- recent folders

fn recent_path(shared: &Shared) -> PathBuf {
    shared.cfg.registry_dir.join("recent_folders.json")
}

/// What the last session was started with, for the new-session screen to offer again: only what was chosen.
fn last_of(kind: &str, account: Option<&str>, mode: &str, chosen: &Settings) -> Map<String, Value> {
    let mut last = obj(json!({"kind": kind, "mode": mode}));
    for (key, value) in [("account", account), ("cwd", chosen.cwd.as_deref()), ("model", chosen.model.as_deref()), ("effort", chosen.effort.as_deref())] {
        if let Some(value) = value {
            last.insert(key.to_string(), json!(value));
        }
    }
    last
}

/// The record as stored: the folders, newest first, and what the last session was started with. A file that cannot be read
/// or is not valid is an empty record, and a field of the wrong shape is dropped: it is a convenience, not a record of sessions.
fn stored(shared: &Shared) -> (Vec<String>, Map<String, Value>) {
    parse_record(&std::fs::read_to_string(recent_path(shared)).unwrap_or_default())
}

fn parse_record(text: &str) -> (Vec<String>, Map<String, Value>) {
    let parsed: Value = serde_json::from_str(text).unwrap_or_default();
    let list = parsed["folders"].as_array().cloned().unwrap_or_default();
    let folders = list.iter().filter_map(Value::as_str).filter(|f| valid_path_shape(f) && Path::new(f).is_absolute()).take(RECENT).map(str::to_string).collect();
    let checks: [(&str, fn(&str) -> bool); 6] = [
        ("kind", |s| matches!(s, "claude" | "codex")),
        ("account", |s| !s.is_empty() && s.chars().count() <= 64 && !s.chars().any(char::is_control)),
        ("mode", |s| matches!(s, "auto" | "default")),
        ("cwd", |s| valid_path_shape(s) && Path::new(s).is_absolute()),
        ("model", valid_model),
        ("effort", valid_effort),
    ];
    let last = checks.iter().filter_map(|(key, ok)| parsed["last"][key].as_str().filter(|s| ok(s)).map(|s| (key.to_string(), json!(s)))).collect();
    (folders, last)
}

/// Keep what a session was started with, and its folder first in the list (the list holds ten, each once). The record is
/// replaced atomically with mode 0600.
pub fn remember(shared: &Shared, last: Map<String, Value>) {
    static WRITING: Mutex<()> = Mutex::new(());
    let _one = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    let mut list = stored(shared).0;
    if let Some(folder) = last.get("cwd").and_then(Value::as_str) {
        list.retain(|f| f != folder);
        list.insert(0, folder.to_string());
        list.truncate(RECENT);
    }
    let dir = &shared.cfg.registry_dir;
    let _ = std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir);
    let temporary = dir.join(format!(".recent.{}.tmp", std::process::id()));
    let _ = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&temporary).and_then(|mut out| {
        out.write_all(json!({"folders": list, "last": last}).to_string().as_bytes())?;
        out.sync_all()
    }).and_then(|_| std::fs::rename(&temporary, recent_path(shared)));
}

// ----------------------------------------------------------------- what the program offers

#[derive(Clone, Default, Debug, PartialEq)]
pub struct Help {
    pub models: Vec<String>,  // the aliases the help text names ('fable', 'opus', 'sonnet')
    pub efforts: Vec<String>, // the levels in the parentheses after `--effort`
    pub issue: Option<&'static str>,
}

/// The options of `--help` as (flag, text) pairs: an option starts at a line with two spaces and a dash and goes on
/// over the more indented lines under it.
fn option_blocks(text: &str) -> Vec<(String, String)> {
    let mut blocks: Vec<(String, String)> = Vec::new();
    let mut open = false;
    for line in text.lines() {
        let indent = line.len() - line.trim_start().len();
        if indent == 2 && line.trim_start().starts_with('-') {
            let mut words = line.split_whitespace();
            let flag = words.clone().find(|w| w.starts_with("--")).unwrap_or("").trim_end_matches(',').to_string();
            blocks.push((flag, words.by_ref().collect::<Vec<_>>().join(" ")));
            open = true;
        } else if open && indent > 2 {
            if let Some(last) = blocks.last_mut() {
                last.1.push(' ');
                last.1.push_str(line.trim());
            }
        } else {
            open = false;
        }
    }
    blocks
}

pub fn parse_help(text: &str) -> Help {
    let blocks = option_blocks(text);
    let body = |flag: &str| blocks.iter().find(|(f, _)| f == flag).map(|(_, b)| b.as_str());
    let efforts: Vec<String> = body("--effort")
        .and_then(|b| b.rsplit_once('(').and_then(|(_, rest)| rest.split_once(')')).map(|(list, _)| list.split(',').map(|w| w.trim().to_string()).collect()))
        .filter(|list: &Vec<String>| list.iter().all(|w| valid_effort(w)))
        .unwrap_or_default();
    let mut models: Vec<String> = Vec::new();
    for (n, part) in body("--model").unwrap_or("").split('\'').enumerate() {
        if n % 2 == 1 && part.len() >= 2 && part.len() <= 20 && part.bytes().all(|b| b.is_ascii_lowercase()) && !models.iter().any(|m| m == part) {
            models.push(part.to_string());
        }
    }
    let issue = if blocks.is_empty() { Some("help_unreadable") } else if efforts.is_empty() && models.is_empty() { Some("help_unrecognised") } else { None };
    Help { models, efforts, issue }
}

fn run_help(shared: &Shared) -> Help {
    let mut command = Command::new(&shared.cfg.claude);
    command.args(&shared.cfg.claude_args).arg("--help").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).env_clear();
    for key in ["PATH", "LANG"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    match command.spawn() {
        Ok(child) => read_help(child, HELP_LIMIT, HELP_GRACE),
        Err(_) => Help { issue: Some("help_unreadable"), ..Help::default() },
    }
}

/// What the child prints, within `limit`; then the reader gets `grace` more to finish, because a wrapper program may
/// leave a process behind that keeps the pipe open (`kill` reaches only the child). What was read by then is parsed.
fn read_help(mut child: Child, limit: Duration, grace: Duration) -> Help {
    let Some(stdout) = child.stdout.take() else { return Help { issue: Some("help_unreadable"), ..Help::default() } };
    let text = Arc::new(Mutex::new(Vec::new()));
    let (done, finished) = mpsc::channel();
    let sink = Arc::clone(&text);
    std::thread::spawn(move || {
        let (mut stdout, mut chunk) = (stdout.take(1 << 20), [0u8; 8192]);
        while let Ok(n @ 1..) = stdout.read(&mut chunk) {
            sink.lock().unwrap_or_else(|e| e.into_inner()).extend_from_slice(&chunk[..n]);
        }
        let _ = done.send(());
    });
    let end = Instant::now() + limit;
    loop {
        match child.try_wait() {
            Ok(None) if Instant::now() < end => std::thread::sleep(Duration::from_millis(10)),
            Ok(Some(_)) => break,
            _ => {
                let _ = child.kill(); // too slow, or not askable
                let _ = child.wait();
                break;
            }
        }
    }
    let _ = finished.recv_timeout(grace);
    let read = std::mem::take(&mut *text.lock().unwrap_or_else(|e| e.into_inner()));
    parse_help(&String::from_utf8_lossy(&read))
}

/// The file the program is, for its modification time: the configured path, or the first match on PATH.
fn program_stamp(program: &Path) -> Option<SystemTime> {
    let found = if program.components().count() > 1 {
        Some(program.to_path_buf())
    } else {
        std::env::var_os("PATH").and_then(|paths| std::env::split_paths(&paths).map(|d| d.join(program)).find(|p| p.is_file()))
    };
    found.and_then(|p| std::fs::metadata(p).ok()).and_then(|m| m.modified().ok())
}

/// Ask the installed program once (no account, no network, three seconds at most), and again when it was replaced.
pub fn claude_help(shared: &Shared) -> Help {
    static CACHE: Mutex<Option<(SystemTime, Help)>> = Mutex::new(None);
    let stamp = program_stamp(&shared.cfg.claude);
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let (Some(stamp), Some((seen, help))) = (stamp, cache.as_ref()) {
        if *seen == stamp {
            return help.clone();
        }
    }
    let help = run_help(shared);
    *cache = stamp.filter(|_| help.issue.is_none()).map(|s| (s, help.clone()));
    help
}

/// `session.options`: what the new-session screen offers, and what the last session was started with. Folders that are
/// no longer directories are left out of both.
pub fn options(shared: &Shared) -> Reply {
    let help = claude_help(shared);
    let (folders, mut last) = stored(shared);
    let recent: Vec<String> = folders.into_iter().filter(|f| Path::new(f).is_dir()).collect();
    if last.get("cwd").and_then(Value::as_str).map_or(false, |f| !Path::new(f).is_dir()) {
        last.remove("cwd");
    }
    Ok(obj(json!({"models": help.models, "efforts": help.efforts, "issue": help.issue, "recent": recent, "last": last})))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The part of `claude --help` of Claude Code 2.1.310 that names the two options (wrapped as the program wraps it).
    const HELP: &str = "Usage: claude [options] [command] [prompt]\n\nOptions:\n  --effort <level>                      Effort level for the current session\n                                        (low, medium, high, xhigh, max)\n  --environment <environment_id>        Create a new cloud session that runs on\n                                        the given self-hosted environment\n  --fallback-model <model>              Enable automatic fallback to specified\n                                        model(s) when the default model is\n  --model <model>                       Model for the current session. Provide\n                                        an alias for the latest model (e.g.\n                                        'fable', 'opus', or 'sonnet') or a\n                                        model's full name.\n  -v, --version                         Output the version number\n";

    /// The table of model names that the protocol rule (`lico_protocol/v1.py`) is tested against as well: one rule, two readers.
    #[test]
    fn valid_model_agrees_with_the_shared_table_of_model_names() {
        let table: Value = serde_json::from_str(include_str!("../../../../tests/fixtures/model_names.json")).unwrap();
        let names = |kind: &str| -> Vec<(String, String)> {
            table[kind].as_array().unwrap().iter().map(|e| {
                let text = e["text"].as_str().map(str::to_string).unwrap_or_else(|| e["unit"].as_str().unwrap().repeat(e["times"].as_u64().unwrap() as usize));
                (text, e["why"].as_str().unwrap().to_string())
            }).collect()
        };
        let wrong = |kind: &str, want: bool| names(kind).into_iter().filter(|(text, _)| valid_model(text) != want).map(|(_, why)| why).collect::<Vec<_>>();
        assert_eq!(wrong("valid", true), Vec::<String>::new(), "refused, but valid in the table");
        assert_eq!(wrong("invalid", false), Vec::<String>::new(), "accepted, but invalid in the table");
    }

    /// The table of what a name, a folder path and an input text may hold, that the protocol rule (`lico_protocol/v1.py`) is tested against as well.
    #[test]
    fn the_name_folder_and_text_rules_agree_with_the_shared_table() {
        let table: Value = serde_json::from_str(include_str!("../../../../tests/fixtures/text_rules.json")).unwrap();
        let entries = |rule: &str, kind: &str| -> Vec<(String, String)> {
            table[rule][kind].as_array().unwrap().iter().map(|e| {
                let text = e["text"].as_str().map(str::to_string).unwrap_or_else(|| e["unit"].as_str().unwrap().repeat(e["times"].as_u64().unwrap() as usize));
                (text, e["why"].as_str().unwrap().to_string())
            }).collect()
        };
        let rules: [(&str, fn(&str) -> bool); 3] = [
            ("name", valid_name),
            ("folder", valid_path_shape),
            ("text", |text| crate::ipc::valid_text(&Value::String(text.to_string()))),
        ];
        for (rule, accepts) in rules {
            let wrong = |kind: &str, want: bool| entries(rule, kind).into_iter().filter(|(text, _)| accepts(text) != want).map(|(_, why)| why).collect::<Vec<_>>();
            assert_eq!(wrong("valid", true), Vec::<String>::new(), "{rule}: refused, but valid in the table");
            assert_eq!(wrong("invalid", false), Vec::<String>::new(), "{rule}: accepted, but invalid in the table");
        }
    }

    #[test]
    fn a_stored_record_gives_back_only_fields_of_the_right_shape() {
        let text = |last: Value| json!({"folders": ["/a", "relative/ok-as-a-list-entry?", 7, "/b\0c"], "last": last}).to_string();
        let good = json!({"kind": "claude", "account": "acc-1", "mode": "default", "cwd": "/work/p", "model": "opus", "effort": "high"});
        assert_eq!(parse_record(&text(good.clone())).1, obj(good.clone()));
        assert_eq!(parse_record(&text(good)).0, ["/a"]);          // a list entry is a full path like `cwd` (a relative one would be read from the host's own folder)
        let bad = json!({"kind": "synthetic", "account": "", "mode": "ask", "cwd": "work", "model": "-x", "effort": "High"});
        assert_eq!(parse_record(&text(bad)).1, Map::new());
        assert_eq!(parse_record(&text(json!({"kind": 1, "model": ["opus"], "effort": null}))).1, Map::new());
        assert_eq!(parse_record("not json"), (Vec::new(), Map::new()));
        assert_eq!(parse_record(&json!({"folders": (0..30).map(|n| format!("/f{n}")).collect::<Vec<_>>()}).to_string()).0.len(), RECENT);
    }

    #[test]
    fn a_name_stored_with_a_hidden_character_loads_without_it_and_a_folder_with_one_does_not_load() {
        let record = |fields: Value| { let mut r = json!({"scratch": true}); r.as_object_mut().unwrap().extend(fields.as_object().unwrap().clone()); r };
        let name = |name: &str| Settings::from_record(&record(json!({"name": name})), "s1");
        assert_eq!(name("a\u{200b}b\u{202e}c\u{e0041}").unwrap().name.as_deref(), Some("abc"));
        assert_eq!(name("a\u{200d}b").unwrap().name.as_deref(), Some("a\u{200d}b")); // the joiner stays
        assert!(name("a\u{85}b").is_err() && name("a\nb").is_err());                // a control character stopped the load before, and still does
        assert!(name(&"\u{200b}".repeat(5).chars().chain("x".repeat(MAX_NAME + 1).chars()).collect::<String>()).is_err());
        let folder = Settings::from_record(&json!({"scratch": false, "cwd": "/work/\u{202e}p"}), "s1");
        assert_eq!(folder, Err("sessions.json has an invalid cwd for s1".to_string()));
    }

    #[test]
    fn what_the_last_session_chose_is_kept_and_what_it_left_out_is_not() {
        let chosen = Settings { cwd: None, name: Some("n".into()), model: Some("opus".into()), effort: None };
        assert_eq!(last_of("codex", None, "auto", &chosen), obj(json!({"kind": "codex", "mode": "auto", "model": "opus"})));
        let all = Settings { cwd: Some("/w".into()), name: None, model: Some("sonnet".into()), effort: Some("low".into()) };
        assert_eq!(last_of("claude", Some("a1"), "default", &all), obj(json!({"kind": "claude", "mode": "default", "account": "a1", "cwd": "/w", "model": "sonnet", "effort": "low"})));
    }

    #[test]
    fn the_help_text_gives_the_levels_and_the_aliases_and_nothing_else() {
        let help = parse_help(HELP);
        assert_eq!(help.efforts, ["low", "medium", "high", "xhigh", "max"]);
        assert_eq!(help.models, ["fable", "opus", "sonnet"]);
        assert_eq!(help.issue, None);
    }

    #[test]
    fn a_wrapper_that_leaves_a_process_holding_the_pipe_does_not_hold_the_help_reader() {
        let dir = std::env::temp_dir().join(format!("lico-help-unit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (script, left) = (dir.join("wrapper.sh"), dir.join("left.pid"));
        std::fs::write(&script, format!("cat <<'END'\n{HELP}END\nsleep 30 &\necho $! > {}\n", left.display())).unwrap();
        let child = Command::new("sh").arg(&script).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap();
        let started = Instant::now();
        let help = read_help(child, HELP_LIMIT, HELP_GRACE);
        let took = started.elapsed();
        let _ = Command::new("kill").arg(std::fs::read_to_string(&left).unwrap().trim()).status();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(took < HELP_LIMIT + Duration::from_secs(2), "{took:?}");
        assert_eq!((help.efforts.len(), help.models.len(), help.issue), (5, 3, None));
    }

    #[test]
    fn a_help_text_without_the_entries_offers_nothing_and_says_why() {
        assert_eq!(parse_help("").issue, Some("help_unreadable"));
        let none = parse_help("Usage: tool\n\nOptions:\n  -v, --version   Output the version number\n");
        assert!(none.models.is_empty() && none.efforts.is_empty());
        assert_eq!(none.issue, Some("help_unrecognised"));
    }

    #[test]
    fn only_the_scratch_workspace_of_a_real_session_id_is_ever_removed() {
        let base = std::env::temp_dir().join(format!("lico-folders-unit-{}", std::process::id()));
        let (root, outside) = (base.join("workspaces"), base.join("outside"));
        let file = |dir: &Path| {
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(dir.join("keep.txt"), "x").unwrap();
        };
        let id = "s0123456789abcdef";
        file(&root.join(id));
        file(&outside);
        remove_scratch_in(&root, id, false); // a session with a chosen folder has no scratch workspace to remove
        assert!(root.join(id).join("keep.txt").exists());
        remove_scratch_in(&root, "../outside", true); // not a session id: never joined to the root
        remove_scratch_in(&root, "s0123456789abcdef/../../outside", true);
        remove_scratch_in(&root, "", true);
        assert!(outside.join("keep.txt").exists());
        remove_scratch_in(&root, id, true);
        assert!(!root.join(id).exists());
        std::os::unix::fs::symlink(&outside, root.join(id)).unwrap(); // a link in its place is not followed
        remove_scratch_in(&root, id, true);
        assert!(outside.join("keep.txt").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn the_registry_fields_round_trip_and_an_older_record_is_a_scratch_session() {
        let chosen = Settings { cwd: Some("/work/project".into()), name: Some("api".into()), model: Some("opus".into()), effort: Some("high".into()) };
        let record = Value::Object(chosen.record_fields());
        assert_eq!(Settings::from_record(&record, "s1").unwrap(), chosen);
        assert_eq!(record["scratch"], json!(false));
        assert_eq!(Settings::from_record(&json!({"session": "s1"}), "s1").unwrap(), Settings::default());
        assert_eq!(Value::Object(Settings::default().record_fields()), json!({"scratch": true}));
        for wrong in [json!({"scratch": "yes"}), json!({"scratch": true, "cwd": "/x"}), json!({"scratch": false}), json!({"scratch": false, "cwd": "relative"}), json!({"name": 5}), json!({"chosen_effort": "High"})] {
            assert!(Settings::from_record(&wrong, "s1").is_err(), "{wrong}");
        }
    }
}
