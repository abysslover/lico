//! The local-session choice of the new-session form (M16-P07): the servers a discovery found (and the one last used), the choice of server and model, the words for what is
//! missing, and the request `session.create` makes. Pure functions of the host's replies; nothing here draws, asks or keeps anything. Only loopback servers are offered, and a
//! server's key is never handled here (it is stored with `lico local-key`, and the words say so).

use serde_json::{json, Value};

/// One server the person can choose.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Server {
    /// `ADDRESS:PORT`, a loopback address.
    pub addr: String,
    /// `openai` or `ollama`.
    pub api: String,
    pub models: Vec<String>,
    pub wants_key: bool,
    /// The server the last local session used (offered even when no scan has been made).
    pub remembered: bool,
}

/// The servers of an `llm.discover` reply that a local session can use: an OpenAI-compatible or Ollama server on a loopback address that lists a model (or asks for a key, so
/// that the person is told what to do). Those with models first, in the host's order.
pub(super) fn servers(reply: &Value) -> Vec<Server> {
    let mut listed = Vec::new();
    let mut keyed = Vec::new();
    for e in reply["endpoints"].as_array().into_iter().flatten() {
        let Some(api) = e["kind"].as_str().filter(|k| matches!(*k, "openai" | "ollama")) else { continue };
        if e["process"].as_str() == Some("ssh") {
            continue; // a server of another machine, reached through ssh: not a local session's
        }
        let host = e["address"].as_str().unwrap_or("");
        let text = if host.contains(':') { format!("[{host}]:{}", e["port"]) } else { format!("{host}:{}", e["port"]) };
        let Some(addr) = crate::driver::parse_endpoint(&text) else { continue };
        let models: Vec<String> = e["models"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect();
        let wants_key = e["auth"].as_str() == Some("key_needed");
        let server = Server { addr: addr.to_string(), api: api.to_string(), models, wants_key, remembered: false };
        if !server.models.is_empty() {
            listed.push(server);
        } else if wants_key {
            keyed.push(server);
        }
    }
    listed.extend(keyed);
    listed
}

/// The server of the host's `last` record when the last session was a local one.
pub(super) fn remembered(last: &Value) -> Option<Server> {
    if last["kind"].as_str() != Some("local") {
        return None;
    }
    let addr = crate::driver::parse_endpoint(last["endpoint"].as_str()?)?;
    let api = crate::driver::parse_api(last["api"].as_str()?)?;
    let model = last["model"].as_str().filter(|m| !m.is_empty())?;
    Some(Server { addr: addr.to_string(), api: api.word().to_string(), models: vec![model.to_string()], wants_key: false, remembered: true })
}

/// The scan's servers with the remembered one: first when the scan did not find it, marked when it did.
pub(super) fn merge(found: Vec<Server>, kept: Option<&Server>) -> Vec<Server> {
    let Some(kept) = kept else { return found };
    let mut found = found;
    match found.iter().position(|s| s.addr == kept.addr) {
        Some(at) => {
            found[at].remembered = true;
            let mut first = found.remove(at);
            if !first.models.contains(&kept.models[0]) {
                first.models.push(kept.models[0].clone());
            }
            found.insert(0, first);
            found
        }
        None => std::iter::once(kept.clone()).chain(found).collect(),
    }
}

/// The row's value: where it listens and what it is.
pub(super) fn line(s: &Server) -> String {
    let mut parts = vec![s.api.clone()];
    match (s.models.len(), s.wants_key) {
        (0, true) => parts.push("a key is needed".into()),
        (n, key) => {
            parts.push(format!("{n} model{}", if n == 1 { "" } else { "s" }));
            if key {
                parts.push("a key is needed".into());
            }
        }
    }
    if s.remembered {
        parts.push("used last".into());
    }
    format!("{}  {}", s.addr, parts.join(", "))
}

pub(super) fn empty_hint() -> &'static str {
    "No server yet: press Enter on the Server row to look for model servers on this machine."
}

/// What the person is told about a server's key, if it wants one.
pub(super) fn key_note(s: &Server) -> Option<String> {
    s.wants_key.then(|| format!("This server wants a key: lico local-key set {} (the key on standard input), then scan again.", s.addr))
}

/// Why the session cannot be started from this choice yet.
pub(super) fn problem(servers: &[Server], server: usize, model: &str) -> Option<String> {
    let Some(s) = servers.get(server) else {
        return Some(if servers.is_empty() { "press Enter on the Server row to look for model servers".into() } else { "choose a server".into() });
    };
    if s.wants_key && s.models.is_empty() {
        return Some(format!("this server wants a key before it lists its models: lico local-key set {}, then scan again", s.addr));
    }
    if model.is_empty() {
        return Some("choose a model".into());
    }
    if !s.models.iter().any(|m| m == model) {
        return Some(format!("the model {model} is not listed by this server"));
    }
    None
}

/// The next (or previous) index in a list that wraps; 0 for an empty list.
pub(super) fn step(len: usize, current: usize, by: isize) -> usize {
    if len == 0 { 0 } else { (current as isize + by).rem_euclid(len as isize) as usize }
}

/// The model after `current` in the server's list (the first when `current` is not in it).
pub(super) fn model_after(s: &Server, current: &str, by: isize) -> String {
    match s.models.iter().position(|m| m == current) {
        Some(at) => s.models[step(s.models.len(), at, by)].clone(),
        None => s.models.first().cloned().unwrap_or_default(),
    }
}

/// The `session.create` request: kind `local`, no account, no folder; a name and a window only when given.
pub(super) fn request(s: &Server, model: &str, name: &str, context: u32) -> Value {
    let mut request = json!({"op": "session.create", "kind": "local", "endpoint": s.addr, "api": s.api, "model": model});
    if !name.trim().is_empty() {
        request["name"] = json!(name.trim());
    }
    if context > 0 {
        request["context_tokens"] = json!(context);
    }
    request
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply() -> Value {
        json!({"ok": 1, "endpoints": [
            {"address": "127.0.0.1", "port": 11434, "bind": "loopback", "kind": "ollama", "apis": ["ollama"], "auth": "none_needed", "models": ["llama3", "qwen2"], "models_total": 2, "process": "ollama"},
            {"address": "127.0.0.1", "port": 8080, "bind": "all", "kind": "openai", "auth": "key_needed", "models": [], "models_total": 0, "process": "vllm"},
            {"address": "127.0.0.1", "port": 9000, "bind": "loopback", "kind": "openai", "auth": "none_needed", "models": [], "models_total": 0},
            {"address": "192.168.1.5", "port": 11434, "bind": "specific", "kind": "ollama", "auth": "none_needed", "models": ["x"], "models_total": 1},
            {"address": "127.0.0.1", "port": 22, "bind": "loopback", "kind": "other", "models": []},
            {"address": "127.0.0.1", "port": 1234, "bind": "loopback", "kind": "openai", "auth": "none_needed", "models": ["m1", "m2", "m3"], "models_total": 3, "process": "ssh"},
            {"address": "::1", "port": 5000, "bind": "loopback", "kind": "ollama", "auth": "none_needed", "models": ["z"], "models_total": 1}
        ]})
    }

    #[test]
    fn the_servers_of_a_scan_are_loopback_openai_or_ollama_ones_with_models_first_then_those_that_want_a_key() {
        let found = servers(&reply());
        let names: Vec<(&str, &str, usize, bool)> = found.iter().map(|s| (s.addr.as_str(), s.api.as_str(), s.models.len(), s.wants_key)).collect();
        assert_eq!(names, [("127.0.0.1:11434", "ollama", 2, false), ("[::1]:5000", "ollama", 1, false), ("127.0.0.1:8080", "openai", 0, true)]);
        assert!(found.iter().all(|s| !s.remembered));
        assert_eq!(servers(&json!({"ok": 1, "endpoints": []})), Vec::<Server>::new());
        assert_eq!(servers(&json!({"ok": 0, "error": "busy"})), Vec::<Server>::new());
        assert_eq!(servers(&Value::Null), Vec::<Server>::new());
        // a server of another machine reached by ssh is never offered, whatever it is called
        assert!(!found.iter().any(|s| s.addr.ends_with(":1234")), "an ssh row is not a local server");
    }

    #[test]
    fn the_last_used_server_is_read_from_the_hosts_record_only_for_a_local_session_and_only_when_it_is_whole() {
        let last = json!({"kind": "local", "endpoint": "127.0.0.1:11434", "api": "ollama", "model": "llama3", "context_tokens": 4096});
        let s = remembered(&last).unwrap();
        assert_eq!((s.addr.as_str(), s.api.as_str(), s.models.clone(), s.remembered, s.wants_key), ("127.0.0.1:11434", "ollama", vec!["llama3".to_string()], true, false));
        for broken in [json!({"kind": "claude", "model": "x"}), json!({"kind": "local", "api": "ollama", "model": "m"}), json!({"kind": "local", "endpoint": "192.168.1.5:80", "api": "ollama", "model": "m"}), json!({"kind": "local", "endpoint": "127.0.0.1:1", "api": "grpc", "model": "m"}), json!({"kind": "local", "endpoint": "127.0.0.1:1", "api": "openai"}), Value::Null] {
            assert!(remembered(&broken).is_none(), "{broken}");
        }
    }

    #[test]
    fn a_scan_and_the_remembered_server_are_merged_without_a_duplicate() {
        let found = servers(&reply());
        let kept = remembered(&json!({"kind": "local", "endpoint": "127.0.0.1:11434", "api": "ollama", "model": "qwen2"})).unwrap();
        let merged = merge(found.clone(), Some(&kept));
        assert_eq!(merged.len(), 3);
        assert!(merged[0].remembered && merged[0].addr == "127.0.0.1:11434" && merged[0].models == ["llama3", "qwen2"], "found: it keeps the scan's models and is marked");
        let elsewhere = remembered(&json!({"kind": "local", "endpoint": "127.0.0.1:7777", "api": "openai", "model": "mine"})).unwrap();
        let merged = merge(found.clone(), Some(&elsewhere));
        assert_eq!((merged.len(), merged[0].addr.as_str(), merged[0].remembered), (4, "127.0.0.1:7777", true), "not found by the scan: offered first, as it was");
        assert_eq!(merge(found.clone(), None), found);
        assert_eq!(merge(Vec::new(), Some(&elsewhere)).len(), 1, "without a scan the remembered server alone is offered");
    }

    #[test]
    fn a_row_says_where_the_server_listens_what_it_is_and_what_is_missing() {
        let found = servers(&reply());
        assert_eq!(line(&found[0]), "127.0.0.1:11434  ollama, 2 models");
        assert_eq!(line(&found[2]), "127.0.0.1:8080  openai, a key is needed");
        let one = Server { addr: "127.0.0.1:1".into(), api: "openai".into(), models: vec!["a".into()], wants_key: false, remembered: true };
        assert_eq!(line(&one), "127.0.0.1:1  openai, 1 model, used last");
        assert!(empty_hint().contains("Enter") && empty_hint().contains("look for"), "{}", empty_hint());
        assert_eq!(key_note(&found[0]), None);
        let note = key_note(&found[2]).unwrap();
        assert!(note.contains("lico local-key set 127.0.0.1:8080") && !note.to_lowercase().contains("type"), "{note}");
    }

    #[test]
    fn the_choice_says_why_it_cannot_start_and_nothing_when_it_can() {
        let found = servers(&reply());
        assert!(problem(&[], 0, "").unwrap().contains("Enter on the Server row"));
        assert_eq!(problem(&found, 0, "llama3"), None);
        assert!(problem(&found, 0, "").unwrap().contains("model"));
        assert!(problem(&found, 0, "not-listed").unwrap().contains("not listed"), "{:?}", problem(&found, 0, "not-listed"));
        let key = problem(&found, 2, "anything").unwrap();
        assert!(key.contains("lico local-key set 127.0.0.1:8080") && key.contains("scan again"), "{key}");
        assert!(problem(&found, 9, "x").is_some(), "an index beyond the list is a problem, not a panic");
        let remembered_only = Server { addr: "127.0.0.1:7777".into(), api: "openai".into(), models: vec!["mine".into()], wants_key: false, remembered: true };
        assert_eq!(problem(&[remembered_only.clone()], 0, "mine"), None);
        assert!(problem(&[remembered_only], 0, "other").is_some());
    }

    #[test]
    fn the_server_and_the_model_move_round_and_the_first_model_follows_a_new_server() {
        assert_eq!((step(3, 0, 1), step(3, 2, 1), step(3, 0, -1), step(1, 0, 1), step(0, 0, 1), step(0, 0, -1)), (1, 0, 2, 0, 0, 0));
        let found = servers(&reply());
        assert_eq!((model_after(&found[0], "llama3", 1).as_str(), model_after(&found[0], "qwen2", 1).as_str(), model_after(&found[0], "llama3", -1).as_str(), model_after(&found[0], "gone", 1).as_str()), ("qwen2", "llama3", "qwen2", "llama3"));
        assert_eq!(model_after(&found[2], "x", 1), "", "a server that lists no model has none to offer");
    }

    #[test]
    fn the_request_is_a_local_session_with_a_model_and_only_what_was_given_and_no_account_or_folder() {
        let found = servers(&reply());
        let r = request(&found[0], "qwen2", "  my local  ", 0);
        assert_eq!(r, json!({"op": "session.create", "kind": "local", "endpoint": "127.0.0.1:11434", "api": "ollama", "model": "qwen2", "name": "my local"}));
        let r = request(&found[1], "z", "", 8192);
        assert_eq!(r, json!({"op": "session.create", "kind": "local", "endpoint": "[::1]:5000", "api": "ollama", "model": "z", "context_tokens": 8192}));
        for key in ["account", "cwd", "mode", "effort", "worktree_branch"] {
            assert!(r.get(key).is_none(), "{key}");
        }
    }
}
