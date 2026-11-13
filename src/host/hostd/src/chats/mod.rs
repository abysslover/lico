//! Full-pagination chat discovery (`session.chats`, zero top-N truncation) and
//! resumed-transcript seeding (`resume_id` on `session.create`) across `claude` and `codex`
//! for both local and `[ssh: <host>]` accounts (`M07-P03`).

mod seed;

pub use seed::seed_resumed_transcript;

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde_json::{json, Map, Value};

use crate::core::{Reply, Shared};
use crate::folders::{check_folder, derive_auto_title, refused_char, valid_path_shape, MAX_NAME};
use crate::provider_record::{valid_provider_session, O_NOFOLLOW, O_NONBLOCK};
use crate::session::{obj, State};

pub const DEFAULT_LIMIT: usize = 20;
pub const MAX_LIMIT: usize = 100;
pub(super) const MAX_SCAN_FILES: usize = 2500;
const MAX_SCAN_LINES: usize = 60;
const MAX_HEAD_BYTES: u64 = 64 * 1024;
pub(super) const MAX_SEED_BYTES: u64 = 256 * 1024;
pub(super) const MAX_SEED_TURNS: usize = 24;
pub(super) const MAX_LINES_PER_TURN: usize = 16;

#[derive(Clone, Debug)]
pub(super) struct ChatItem {
    pub(super) provider: String,
    pub(super) provider_session: String,
    pub(super) account: Option<String>,
    pub(super) account_label: Option<String>,
    pub(super) ssh_host: Option<String>,
    pub(super) cwd: Option<String>,
    pub(super) global: bool,
    pub(super) title: String,
    pub(super) updated_at: u64,
    pub(super) open_session: Option<String>,
    pub(super) open_state: Option<&'static str>,
}

pub fn valid_query(value: &Value) -> bool {
    matches!(value, Value::String(s) if s.chars().count() <= 256 && !s.chars().any(refused_char))
}

pub(super) fn norm_path(p: &str) -> &str {
    let trimmed = p.trim_end_matches('/');
    if trimmed.is_empty() && p.starts_with('/') {
        "/"
    } else {
        trimmed
    }
}

fn paths_eq(a: &str, b: &str) -> bool {
    !a.is_empty() && !b.is_empty() && norm_path(a) == norm_path(b)
}

fn state_word(st: State) -> &'static str {
    match st {
        State::Running => "running",
        State::Disconnected => "disconnected",
        State::Exited => "exited",
    }
}

fn state_rank(st: &'static str) -> u8 {
    match st {
        "running" => 3,
        "disconnected" => 2,
        _ => 1,
    }
}

pub(super) fn real_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_dir())
}

pub(super) fn open_regular_nofollow(path: &Path) -> Option<(std::fs::File, u64, u64)> {
    if !std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_file()) {
        return None;
    }
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(O_NOFOLLOW | O_NONBLOCK)
        .open(path)
        .ok()?;
    let meta = file.metadata().ok()?;
    if !meta.file_type().is_file() {
        return None;
    }
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    Some((file, meta.len(), mtime))
}

pub(super) fn clean_title(raw: &str, provider: &str, prov_sid: &str) -> String {
    if let Some(t) = derive_auto_title(raw) {
        return t;
    }
    let short = &prov_sid[..prov_sid.len().min(8)];
    let fallback = format!("{provider} ({short})");
    fallback.chars().take(MAX_NAME).collect()
}

pub(super) fn is_codex_system_text(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with("<environment_context>")
        || t.starts_with("<permissions instructions>")
        || t.starts_with("<user_instructions>")
        || t.starts_with("<INSTRUCTIONS>")
        || t.starts_with("# AGENTS.md instructions")
}

fn codex_stem_id(stem: &str) -> Option<String> {
    if let Some(rest) = stem.strip_prefix("rollout-") {
        if rest.len() > 36 {
            let tail = &rest[rest.len() - 36..];
            if tail.as_bytes().get(8) == Some(&b'-') && valid_provider_session(tail) {
                return Some(tail.to_string());
            }
        }
    }
    valid_provider_session(stem).then(|| stem.to_string())
}

pub(super) fn claude_msg_text(v: &Value, is_user: bool) -> Option<String> {
    if v.get("isSidechain") == Some(&Value::Bool(true)) || v.get("isMeta") == Some(&Value::Bool(true)) {
        return None;
    }
    match v.pointer("/message/content") {
        Some(Value::String(s)) if !s.trim().is_empty() => Some(s.clone()),
        Some(Value::Array(blocks)) => {
            let mut parts = Vec::new();
            let mut has_non_tool = false;
            for b in blocks {
                let kind = b.get("type").and_then(Value::as_str).unwrap_or("");
                if kind == "text" {
                    has_non_tool = true;
                    if let Some(t) = b.get("text").and_then(Value::as_str).filter(|t| !t.trim().is_empty()) {
                        parts.push(t);
                    }
                } else if kind != "tool_result" {
                    has_non_tool = true;
                }
            }
            if is_user && !has_non_tool {
                return None;
            }
            (!parts.is_empty()).then(|| parts.join("\n"))
        }
        _ => None,
    }
}

pub(super) fn codex_msg_text(v: &Value) -> Option<(bool, String)> {
    match v.get("type").and_then(Value::as_str)? {
        "response_item" => {
            let p = v.get("payload")?;
            if p.get("type").and_then(Value::as_str) != Some("message") {
                return None;
            }
            let role = p.get("role").and_then(Value::as_str)?;
            let is_user = match role {
                "user" => true,
                "assistant" => false,
                _ => return None,
            };
            let mut parts = Vec::new();
            if let Some(blocks) = p.get("content").and_then(Value::as_array) {
                for b in blocks {
                    let btype = b.get("type").and_then(Value::as_str).unwrap_or("");
                    if matches!(btype, "input_text" | "output_text" | "text") {
                        if let Some(t) = b.get("text").and_then(Value::as_str) {
                            if !(is_user && is_codex_system_text(t)) && !t.trim().is_empty() {
                                parts.push(t);
                            }
                        }
                    }
                }
            } else if let Some(s) = p.get("content").and_then(Value::as_str) {
                if !(is_user && is_codex_system_text(s)) && !s.trim().is_empty() {
                    parts.push(s);
                }
            }
            (!parts.is_empty()).then(|| (is_user, parts.join("\n")))
        }
        _ => None,
    }
}

fn scan_claude_file(path: &Path, match_cwd: &dyn Fn(&str) -> bool) -> Option<(String, String, String, u64)> {
    let name = path.file_name()?.to_str()?;
    let stem = name.strip_suffix(".jsonl")?;
    if !valid_provider_session(stem) {
        return None;
    }
    let (file, _, mtime) = open_regular_nofollow(path)?;
    let reader = BufReader::new(file.take(MAX_HEAD_BYTES));
    let mut found_cwd: Option<String> = None;
    let mut first_prompt: Option<String> = None;
    for line in reader.lines().take(MAX_SCAN_LINES).flatten() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        if found_cwd.is_none() {
            if let Some(c) = v.get("cwd").and_then(Value::as_str).filter(|s| s.starts_with('/') && valid_path_shape(s)) {
                if !match_cwd(c) {
                    return None;
                }
                found_cwd = Some(norm_path(c).to_string());
            }
        }
        if first_prompt.is_none() && v.get("type").and_then(Value::as_str) == Some("user") {
            if let Some(txt) = claude_msg_text(&v, true) {
                if derive_auto_title(&txt).is_some() {
                    first_prompt = Some(txt);
                }
            }
        }
        if found_cwd.is_some() && first_prompt.is_some() {
            break;
        }
    }
    let cwd = found_cwd?;
    let title = clean_title(first_prompt.as_deref().unwrap_or(""), "claude", stem);
    Some((stem.to_string(), cwd, title, mtime))
}

fn scan_codex_file(path: &Path, match_cwd: &dyn Fn(&str) -> bool) -> Option<(String, String, String, u64)> {
    let name = path.file_name()?.to_str()?;
    let stem = name.strip_suffix(".jsonl")?;
    let (file, _, mtime) = open_regular_nofollow(path)?;
    let reader = BufReader::new(file.take(MAX_HEAD_BYTES));
    let mut meta_id: Option<String> = None;
    let mut found_cwd: Option<String> = None;
    let mut first_prompt: Option<String> = None;
    for line in reader.lines().take(MAX_SCAN_LINES).flatten() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        let vtype = v.get("type").and_then(Value::as_str).unwrap_or("");
        if vtype == "session_meta" {
            if let Some(id) = v.pointer("/payload/id").and_then(Value::as_str).filter(|s| valid_provider_session(s)) {
                meta_id = Some(id.to_string());
            }
            if let Some(c) = v.pointer("/payload/cwd").and_then(Value::as_str).filter(|s| s.starts_with('/') && valid_path_shape(s)) {
                if !match_cwd(c) {
                    return None;
                }
                found_cwd = Some(norm_path(c).to_string());
            }
        } else if found_cwd.is_none() {
            if let Some(c) = v.get("cwd").and_then(Value::as_str).filter(|s| s.starts_with('/') && valid_path_shape(s)) {
                if !match_cwd(c) {
                    return None;
                }
                found_cwd = Some(norm_path(c).to_string());
            }
        }
        if first_prompt.is_none() {
            if let Some((true, txt)) = codex_msg_text(&v) {
                if derive_auto_title(&txt).is_some() {
                    first_prompt = Some(txt);
                }
            } else if vtype == "event_msg" && v.pointer("/payload/type").and_then(Value::as_str) == Some("user_message") {
                if let Some(m) = v.pointer("/payload/message").and_then(Value::as_str) {
                    if !is_codex_system_text(m) && derive_auto_title(m).is_some() {
                        first_prompt = Some(m.to_string());
                    }
                }
            }
        }
        if found_cwd.is_some() && first_prompt.is_some() && meta_id.is_some() {
            break;
        }
    }
    let cwd = found_cwd?;
    let prov_sid = meta_id.or_else(|| codex_stem_id(stem))?;
    let title = clean_title(first_prompt.as_deref().unwrap_or(""), "codex", &prov_sid);
    Some((prov_sid, cwd, title, mtime))
}

fn scan_claude_home(home: &Path, match_cwd: &dyn Fn(&str) -> bool) -> Vec<(String, String, String, u64)> {
    let mut out = Vec::new();
    let projects = home.join("projects");
    if !real_dir(&projects) {
        return out;
    }
    let Ok(dirs) = std::fs::read_dir(&projects) else { return out };
    let mut seen_files = 0usize;
    for d_entry in dirs.flatten() {
        let dp = d_entry.path();
        if !real_dir(&dp) {
            continue;
        }
        let Ok(files) = std::fs::read_dir(&dp) else { continue };
        for f_entry in files.flatten() {
            if seen_files >= MAX_SCAN_FILES {
                return out;
            }
            seen_files += 1;
            if let Some(rec) = scan_claude_file(&f_entry.path(), match_cwd) {
                out.push(rec);
            }
        }
    }
    out
}

fn walk_codex_dir(dir: &Path, depth: usize, left: &mut usize, match_cwd: &dyn Fn(&str) -> bool, out: &mut Vec<(String, String, String, u64)>) {
    if depth > 5 || *left == 0 || !real_dir(dir) {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        if *left == 0 {
            return;
        }
        let p = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&p) else { continue };
        if meta.file_type().is_dir() {
            walk_codex_dir(&p, depth + 1, left, match_cwd, out);
        } else if meta.file_type().is_file() {
            *left -= 1;
            if let Some(rec) = scan_codex_file(&p, match_cwd) {
                out.push(rec);
            }
        }
    }
}

fn scan_codex_home(home: &Path, match_cwd: &dyn Fn(&str) -> bool) -> Vec<(String, String, String, u64)> {
    let mut out = Vec::new();
    let mut left = MAX_SCAN_FILES;
    walk_codex_dir(&home.join("sessions"), 0, &mut left, match_cwd, &mut out);
    out
}

pub(super) fn unbound_codex_home() -> Option<PathBuf> {
    let p = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".codex")))?;
    p.is_absolute().then_some(p)
}

/// `session.chats`: enumerate all `claude` and `codex` conversations matching `cwd` or `global`
/// across local or `[ssh: <host>]` accounts with full `offset` / `limit` / `total` pagination.
pub fn session_chats(shared: &Shared, request: &Map<String, Value>) -> Reply {
    let cwd_raw = request.get("cwd").and_then(Value::as_str).filter(|s| !s.is_empty());
    let global_flag = request.get("global").and_then(Value::as_bool).unwrap_or(false);
    if cwd_raw.is_some() && global_flag {
        return Err("bad_request");
    }
    let is_global = cwd_raw.is_none();
    let ssh_host_opt = request.get("ssh_host").and_then(Value::as_str).filter(|s| !s.is_empty());
    let account_filter = request.get("account").and_then(Value::as_str).filter(|s| !s.is_empty());
    let query_norm = request
        .get("query")
        .and_then(Value::as_str)
        .map(|s| s.trim().to_lowercase())
        .unwrap_or_default();
    let offset = request.get("offset").and_then(Value::as_i64).unwrap_or(0).max(0) as usize;
    let limit = request
        .get("limit")
        .and_then(Value::as_i64)
        .unwrap_or(DEFAULT_LIMIT as i64)
        .clamp(1, MAX_LIMIT as i64) as usize;

    let (all_accounts, claude_managed, codex_managed, homes_map) = {
        let accounts = shared.accounts.lock().unwrap();
        if let Some(aid) = account_filter {
            if accounts.get(aid).is_none() {
                return Err("unknown_account");
            }
        }
        let snap = accounts.list_snapshot();
        let mut hmap = HashMap::new();
        for a in &snap {
            if a.ssh_host.is_none() {
                if let Some(h) = accounts.homes(&a.id) {
                    hmap.insert(a.id.clone(), h.provider_home);
                }
            }
        }
        (snap, accounts.any_for("claude"), accounts.any_for("codex"), hmap)
    };
    let acc_lookup: HashMap<String, (String, String, Option<String>)> = all_accounts
        .iter()
        .map(|a| (a.id.clone(), (a.provider.clone(), a.label.clone(), a.ssh_host.clone())))
        .collect();

    let mut open_by_prov: HashMap<(String, String, String), (String, &'static str, Option<String>, u64)> = HashMap::new();
    let sessions_list = shared.sessions.read().unwrap().clone();
    let mut host_candidates: Vec<ChatItem> = Vec::new();
    for arc_s in &sessions_list {
        let s = arc_s.lock().unwrap();
        if s.purged || !matches!(s.provider, "claude" | "codex") {
            continue;
        }
        let st = state_word(s.state);
        let acc_key = s.account.clone().unwrap_or_default();
        let prov_sid = s
            .codex_thread
            .clone()
            .filter(|id| valid_provider_session(id))
            .unwrap_or_else(|| s.id.clone());
        let explicit_or_auto = s.settings.name.clone().or_else(|| s.auto_title.clone());
        let updated = s.last_active_at.max(s.state_at);
        let map_key = (s.provider.to_string(), acc_key, prov_sid.clone());
        let replace = match open_by_prov.get(&map_key) {
            None => true,
            Some((_, prev_st, _, prev_u)) => (state_rank(st), updated) > (state_rank(prev_st), *prev_u),
        };
        if replace {
            open_by_prov.insert(map_key, (s.id.clone(), st, explicit_or_auto.clone(), updated));
        }
        let (acc_label, acc_ssh) = match s.account.as_deref().and_then(|id| acc_lookup.get(id)) {
            Some((_, lbl, sh)) => (Some(lbl.clone()), sh.clone()),
            None => (None, None),
        };
        if account_filter.is_some_and(|af| s.account.as_deref() != Some(af)) || acc_ssh.as_deref() != ssh_host_opt {
            continue;
        }
        let eff_cwd = s.settings.worktree_repo.clone().or_else(|| s.settings.cwd.clone());
        host_candidates.push(ChatItem {
            provider: s.provider.to_string(),
            provider_session: prov_sid.clone(),
            account: s.account.clone(),
            account_label: acc_label,
            ssh_host: acc_ssh,
            cwd: eff_cwd,
            global: s.settings.cwd.is_none(),
            title: clean_title(explicit_or_auto.as_deref().unwrap_or(""), s.provider, &prov_sid),
            updated_at: updated,
            open_session: Some(s.id.clone()),
            open_state: Some(st),
        });
    }

    let mut merged: HashMap<(String, String, String), ChatItem> = HashMap::new();
    let canon_cwd: Option<String>;

    if let Some(host) = ssh_host_opt {
        let remote_accts: Vec<(String, String, String)> = all_accounts
            .iter()
            .filter(|a| a.ssh_host.as_deref() == Some(host) && account_filter.map_or(true, |af| a.id == af))
            .map(|a| (a.id.clone(), a.provider.clone(), a.label.clone()))
            .collect();
        let (remote_canon, remote_items) = seed::scan_ssh_chats(shared, host, cwd_raw, &remote_accts)?;
        canon_cwd = remote_canon;
        for item in remote_items {
            let key = (item.provider.clone(), item.account.clone().unwrap_or_default(), item.provider_session.clone());
            merged.insert(key, item);
        }
    } else {
        canon_cwd = match cwd_raw {
            Some(raw) => Some(norm_path(&check_folder(shared, raw)?).to_string()),
            None => None,
        };
        let ws_root = std::fs::canonicalize(&shared.cfg.workspace_root).unwrap_or_else(|_| shared.cfg.workspace_root.clone());
        let ws_prefix = norm_path(&ws_root.to_string_lossy()).to_string();
        let match_cwd = |c: &str| -> bool {
            let nc = norm_path(c);
            if let Some(ref target) = canon_cwd {
                nc == target
            } else {
                nc == ws_prefix || nc.starts_with(&format!("{ws_prefix}/"))
            }
        };

        for acc in &all_accounts {
            if acc.ssh_host.is_some() || account_filter.is_some_and(|af| acc.id != af) {
                continue;
            }
            let Some(phome) = homes_map.get(&acc.id) else { continue };
            let recs = if acc.provider == "claude" {
                scan_claude_home(phome, &match_cwd)
            } else {
                scan_codex_home(phome, &match_cwd)
            };
            for (sid, rec_cwd, title, mtime) in recs {
                let key = (acc.provider.clone(), acc.id.clone(), sid.clone());
                merged.insert(
                    key,
                    ChatItem {
                        provider: acc.provider.clone(),
                        provider_session: sid,
                        account: Some(acc.id.clone()),
                        account_label: Some(acc.label.clone()),
                        ssh_host: None,
                        cwd: if is_global { None } else { Some(rec_cwd) },
                        global: is_global,
                        title,
                        updated_at: mtime,
                        open_session: None,
                        open_state: None,
                    },
                );
            }
        }

        if account_filter.is_none() && !claude_managed {
            if let Some(chome) = crate::provider_record::home_of(shared, None) {
                for (sid, rec_cwd, title, mtime) in scan_claude_home(&chome, &match_cwd) {
                    let key = ("claude".into(), String::new(), sid.clone());
                    merged.entry(key).or_insert(ChatItem {
                        provider: "claude".into(),
                        provider_session: sid,
                        account: None,
                        account_label: None,
                        ssh_host: None,
                        cwd: if is_global { None } else { Some(rec_cwd) },
                        global: is_global,
                        title,
                        updated_at: mtime,
                        open_session: None,
                        open_state: None,
                    });
                }
            }
        }
        if account_filter.is_none() && !codex_managed {
            if let Some(xhome) = unbound_codex_home() {
                for (sid, rec_cwd, title, mtime) in scan_codex_home(&xhome, &match_cwd) {
                    let key = ("codex".into(), String::new(), sid.clone());
                    merged.entry(key).or_insert(ChatItem {
                        provider: "codex".into(),
                        provider_session: sid,
                        account: None,
                        account_label: None,
                        ssh_host: None,
                        cwd: if is_global { None } else { Some(rec_cwd) },
                        global: is_global,
                        title,
                        updated_at: mtime,
                        open_session: None,
                        open_state: None,
                    });
                }
            }
        }
    }

    for cand in host_candidates {
        let scope_ok = if is_global {
            cand.global
        } else if let (Some(ref want), Some(ref got)) = (&canon_cwd, &cand.cwd) {
            paths_eq(want, got)
        } else {
            false
        };
        if !scope_ok {
            continue;
        }
        let key = (
            cand.provider.clone(),
            cand.account.clone().unwrap_or_default(),
            cand.provider_session.clone(),
        );
        merged
            .entry(key)
            .and_modify(|existing| {
                existing.updated_at = existing.updated_at.max(cand.updated_at);
                existing.open_session = cand.open_session.clone();
                existing.open_state = cand.open_state;
            })
            .or_insert(ChatItem {
                cwd: if is_global { None } else { canon_cwd.clone().or(cand.cwd) },
                ..cand
            });
    }

    for (key, item) in merged.iter_mut() {
        if let Some((sid, st, custom_title, upd)) = open_by_prov.get(key) {
            item.open_session = Some(sid.clone());
            item.open_state = Some(st);
            item.updated_at = item.updated_at.max(*upd);
            if let Some(t) = custom_title.as_deref().and_then(derive_auto_title) {
                item.title = t;
            }
        }
    }

    let mut all_items: Vec<ChatItem> = merged
        .into_values()
        .filter(|it| {
            query_norm.is_empty()
                || it.title.to_lowercase().contains(&query_norm)
                || it.provider_session.to_lowercase().contains(&query_norm)
                || it.provider.to_lowercase().contains(&query_norm)
                || it.account_label.as_deref().is_some_and(|l| l.to_lowercase().contains(&query_norm))
        })
        .collect();

    all_items.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| a.provider_session.cmp(&b.provider_session))
            .then_with(|| a.account.cmp(&b.account))
    });

    let total = all_items.len();
    let end = offset.saturating_add(limit).min(total);
    let slice = if offset < total { &all_items[offset..end] } else { &[] };
    let items_json: Vec<Value> = slice
        .iter()
        .map(|it| {
            json!({
                "provider": it.provider,
                "provider_session": it.provider_session,
                "account": it.account,
                "account_label": it.account_label,
                "ssh_host": it.ssh_host,
                "cwd": it.cwd,
                "global": it.global,
                "title": it.title,
                "updated_at": it.updated_at,
                "open_session": it.open_session,
                "open_state": it.open_state,
            })
        })
        .collect();

    Ok(obj(json!({
        "scope": if is_global { "global" } else { "folder" },
        "cwd": canon_cwd,
        "ssh_host": ssh_host_opt,
        "total": total,
        "offset": offset,
        "limit": limit,
        "has_more": end < total,
        "items": items_json,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_stem_and_system_prompt_filters_handle_real_shapes() {
        let uuid = "019a7654-1234-7000-8000-abcdef123456";
        assert_eq!(
            codex_stem_id(&format!("rollout-2026-11-12T10-00-00-{uuid}")).as_deref(),
            Some(uuid)
        );
        assert_eq!(codex_stem_id("th-alpha-1").as_deref(), Some("th-alpha-1"));
        assert_eq!(codex_stem_id("-bad"), None);
        assert!(is_codex_system_text("<environment_context>\n<cwd>/w</cwd>"));
        assert!(!is_codex_system_text("Fix pagination in sidebar"));
    }
}
