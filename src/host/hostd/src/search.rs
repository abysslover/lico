//! On-demand conversation search across one session or all sessions (M02-P06, S03; SR-01, SR-02).
//!
//! No index stays resident: each `session.search` call streams `.seg` files one segment at a time outside the session lock, matches case-insensitively,
//! and drops the temporary segment buffer before returning.

use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

use crate::core::{self, Reply, Shared};
use crate::session::{obj, LogView, Session};

pub const MAX_QUERY_CHARS: usize = 256;
pub const DEFAULT_LIMIT: usize = 200;
pub const MAX_LIMIT: usize = 500;
const LABEL_CHARS: usize = 40;
const LINE_SNIPPET_CHARS: usize = 240;

pub fn valid_query(value: &Value) -> bool {
    matches!(value, Value::String(s) if !s.is_empty() && s.chars().count() <= MAX_QUERY_CHARS && !s.chars().any(|c| c.is_control() || crate::codex::dropped(c as u32)))
}

struct Target {
    id: String,
    name: String,
    view: LogView,
}

fn target_of(s: &Session) -> Option<Target> {
    if s.purged || !s.text_mode || s.provider.is_empty() {
        return None;
    }
    let name: String = crate::transfer::label(&s.id, s.settings.name.as_deref(), s.settings.cwd.as_deref()).chars().take(LABEL_CHARS).collect();
    Some(Target { id: s.id.clone(), name, view: s.log_view() })
}

fn clean_snippet(raw: &[u8]) -> String {
    String::from_utf8_lossy(raw)
        .chars()
        .filter(|c| !c.is_control() && !crate::codex::dropped(*c as u32))
        .take(LINE_SNIPPET_CHARS)
        .collect()
}

pub fn matches_query(raw: &[u8], needle_lower: &str, ascii_needle: bool) -> bool {
    if ascii_needle {
        let nb = needle_lower.as_bytes();
        return raw.len() >= nb.len() && raw.windows(nb.len()).any(|w| w.eq_ignore_ascii_case(nb));
    }
    String::from_utf8_lossy(raw).to_lowercase().contains(needle_lower)
}

/// Socket op `session.search`: searches `target_session` (when non-empty) or all non-purged provider sessions.
pub fn search_op(shared: &Shared, query: &str, target_session: Option<&str>, limit: usize) -> Reply {
    if query.is_empty() || query.chars().count() > MAX_QUERY_CHARS || query.chars().any(|c| c.is_control() || crate::codex::dropped(c as u32)) {
        return Err("bad_request");
    }
    let limit = limit.clamp(1, MAX_LIMIT);
    let scoped = target_session.filter(|s| !s.is_empty());
    let targets: Vec<Target> = match scoped {
        Some(id) => {
            let found = core::find(shared, id).ok_or("unknown_session")?;
            let s = found.lock().unwrap();
            if s.purged {
                return Err("unknown_session");
            }
            if !s.text_mode {
                Vec::new()
            } else {
                let name: String = crate::transfer::label(&s.id, s.settings.name.as_deref(), s.settings.cwd.as_deref()).chars().take(LABEL_CHARS).collect();
                vec![Target { id: s.id.clone(), name, view: s.log_view() }]
            }
        }
        None => {
            let list: Vec<Arc<Mutex<Session>>> = shared.sessions.read().unwrap().clone();
            list.iter().filter_map(|arc| target_of(&arc.lock().unwrap())).collect()
        }
    };

    let needle_lower = query.to_lowercase();
    let ascii_needle = needle_lower.is_ascii();
    let mut matches: Vec<Value> = Vec::new();
    let mut total: usize = 0;
    for target in &targets {
        let _ = target.view.scan_each(|seq, bytes| {
            if matches_query(bytes, &needle_lower, ascii_needle) {
                total += 1;
                if matches.len() < limit {
                    matches.push(json!({
                        "session": target.id,
                        "name": target.name,
                        "seq": seq,
                        "text": clean_snippet(bytes),
                    }));
                }
            }
            true
        });
    }
    Ok(obj(json!({
        "query": query,
        "scope": if scoped.is_some() { "session" } else { "all" },
        "matches": matches,
        "total": total,
        "truncated": total > limit,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Config;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicUsize;
    use std::sync::RwLock;
    use std::time::Instant;

    fn test_shared(dir: &std::path::Path) -> Shared {
        std::fs::create_dir_all(dir.join("log")).unwrap();
        std::fs::create_dir_all(dir.join("accounts")).unwrap();
        let cfg = Config {
            login_bin: None, login_args: Vec::new(), login_timeout: 60, log_dir: dir.join("log"), agent: PathBuf::new(),
            retain: 500, window: 20, queue_bytes: 1 << 20, payload: 4096, max_sessions: 8, codex: PathBuf::new(), codex_args: Vec::new(),
            claude: PathBuf::new(), claude_args: Vec::new(), cred_bin: PathBuf::new(), cred_args: Vec::new(), job_bin: PathBuf::new(),
            cred_dir: dir.join("cred"), workspace_root: dir.join("work"), handoff_dir: dir.join("handoff"),
            accounts_dir: dir.join("accounts"), registry_dir: dir.join("registry"), text: true, realistic_text: false,
        };
        let accounts = crate::accounts::Accounts::load(&cfg.accounts_dir).unwrap();
        Shared {
            cfg, sessions: RwLock::new(Vec::new()), views: Mutex::new(HashMap::new()), started: Instant::now(),
            pending_creates: AtomicUsize::new(0), accounts: Mutex::new(accounts), logins: Mutex::new(HashMap::new()), clip: Default::default(),
        }
    }

    #[test]
    fn search_scans_multiple_segments_across_sessions_with_ascii_and_unicode_and_skips_purged_or_synthetic() {
        let dir = std::env::temp_dir().join(format!("lico_search_{}", crate::core::random_hex(6)));
        let shared = test_shared(&dir);
        let make = |id: &str, name: &str, provider: &'static str, text_mode: bool| {
            let mut s = Session::new(id.to_string(), false, &shared.cfg.log_dir, 64, 8);
            s.text_mode = text_mode;
            s.provider = provider;
            s.settings.name = Some(name.to_string());
            Arc::new(Mutex::new(s))
        };
        let a = make("s11111111", "api", "claude", true);
        let b = make("s22222222", "docs", "codex", true);
        let syn = make("s33333333", "raw", "", false);
        {
            let mut sa = a.lock().unwrap();
            for i in 0..45 {
                sa.append(format!("line {i} ordinary output").into_bytes());
            }
            sa.append(b"Found LOGIN_TOKEN mismatch in auth.py".to_vec()); // seq 45 (older than the 8-line in-memory window once more lines follow)
            for i in 46..60 {
                sa.append(format!("later line {i}").into_bytes());
            }
        }
        {
            let mut sb = b.lock().unwrap();
            sb.append("한글 검색 테스트 login_token second session".as_bytes().to_vec());
        }
        syn.lock().unwrap().append(b"login_token in synthetic".to_vec());
        shared.sessions.write().unwrap().extend([a.clone(), b.clone(), syn]);

        let one = search_op(&shared, "login_token", Some("s11111111"), 50).unwrap();
        assert_eq!(one["scope"], json!("session"));
        assert_eq!(one["total"], json!(1));
        assert_eq!(one["matches"][0]["seq"], json!(45));

        let all = search_op(&shared, "login_token", None, 50).unwrap();
        assert_eq!(all["scope"], json!("all"));
        assert_eq!(all["total"], json!(2));
        assert_eq!(all["matches"][0]["session"], json!("s11111111"));
        assert_eq!(all["matches"][1]["session"], json!("s22222222"));

        let kor = search_op(&shared, "검색 테스트", None, 50).unwrap();
        assert_eq!(kor["total"], json!(1));
        assert_eq!(kor["matches"][0]["name"], json!("docs"));

        // Purged session disappears from search.
        b.lock().unwrap().purged = true;
        let after_purge = search_op(&shared, "login_token", None, 50).unwrap();
        assert_eq!(after_purge["total"], json!(1));
        assert_eq!(search_op(&shared, "login_token", Some("s22222222"), 50), Err("unknown_session"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
