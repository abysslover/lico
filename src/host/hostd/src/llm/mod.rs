//! Finding Local LLM servers on this machine (`M12`): which listening TCP sockets answer like an OpenAI-compatible server (`GET /v1/models`) or an Ollama one
//! (`GET /api/tags`), with their model names. Read-only and on demand: a request starts it, nothing runs in the background, nothing is stored and no answer is kept.
//! Only sockets that listen on this machine are looked at, and only at the machine's own address; the default scope is the sockets of the user's own processes
//! (`all_users` lists every listening socket). No configuration or credential file of another program is read (AUTH-08): the process table and HTTP answers only.
//! Bounds: at most 64 sockets (all of them get their first contact at once, with one shared quiet period), 8 questions at a time, 1.5 s for a request, 8 s for the run. The reply carries what a later step that *uses* an endpoint would need: where it
//! listens (`address`, `bind`: a loopback bind on a remote machine needs a tunnel), what it is (`kind`, `apis`), whether it wants a key (`auth`), its model ids and the
//! name of its process.

mod classify;
mod http;
mod net;
mod remote;
mod remote_script;
pub mod tool;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use crate::core::{Reply, Shared};
use classify::{Models, Response};
use http::{Contact, Fault};
use net::Listener;

pub const MAX_PROBED: usize = 64;
const PARALLEL: usize = 8;
pub(crate) const BUDGET: Duration = Duration::from_secs(8);

static PROC_ROOT: OnceLock<PathBuf> = OnceLock::new();

/// A test gives the host a process table of its own (`--llm-proc-root`); a real run reads `/proc`.
pub fn set_proc_root(root: PathBuf) {
    let _ = PROC_ROOT.set(root);
}

/// The folder a test gave the host as its process table, if it did (a real run reads `/proc`).
pub(crate) fn set_root() -> Option<PathBuf> {
    PROC_ROOT.get().cloned()
}

pub(crate) fn proc_root() -> PathBuf {
    PROC_ROOT.get().cloned().unwrap_or_else(|| PathBuf::from("/proc"))
}

/// `llm.discover`: the endpoints of this machine.
pub fn discover(shared: &Shared, request: &Map<String, Value>) -> Reply {
    let all_users = request.get("all_users").and_then(Value::as_bool).unwrap_or(false);
    if let Some(host) = request.get("ssh_host").and_then(Value::as_str) {
        return remote::discover(shared, host, all_users);
    }
    Ok(crate::session::obj(run(&proc_root(), all_users, BUDGET)))
}

fn base(scope: &str) -> Map<String, Value> {
    crate::session::obj(json!({"host": null, "os": std::env::consts::OS, "scope": scope, "supported": true, "source": "proc", "listening": 0, "probed": 0, "skipped": 0, "elapsed_ms": 0, "endpoints": []}))
}

fn unsupported(scope: &str, reason: &str, started: Instant) -> Value {
    let mut out = base(scope);
    out.insert("supported".into(), json!(false));
    out.insert("reason".into(), json!(reason));
    out.insert("elapsed_ms".into(), json!(started.elapsed().as_millis() as u64));
    Value::Object(out)
}

/// One socket's answer.
struct Found {
    kind: &'static str,
    apis: Vec<&'static str>,
    auth: &'static str,
    models: Models,
    detail: Option<&'static str>,
}

fn plain(kind: &'static str, detail: &'static str) -> Found {
    Found { kind, apis: Vec::new(), auth: "unknown", models: Models::default(), detail: Some(detail) }
}

fn fault_detail(f: &Fault) -> (&'static str, &'static str) {
    match f {
        Fault::NotHttp => ("not-http", "not_http"),
        Fault::Timeout => ("unknown", "timeout"),
        Fault::Io => ("unknown", "io"),
    }
}

/// What the first contact with a socket came to: an answer already (it spoke, closed or failed), or a quiet connection ready for the questions.
enum Step {
    Done(Found),
    Quiet(std::net::TcpStream),
}

fn ask(target: std::net::SocketAddr, first: Option<std::net::TcpStream>, path: &str, deadline: Instant) -> Result<Response, Fault> {
    let stream = match first {
        Some(s) => s,
        None => http::connect(target, deadline)?,
    };
    http::get(stream, target, path, deadline)
}

/// The rest of the look at a socket that was found quiet: the two questions (first contact in silence was made by the caller).
fn probe(listener: &Listener, stream: std::net::TcpStream, deadline: Instant) -> Found {
    let target = listener.target();
    let models = match ask(target, Some(stream), "/v1/models", deadline) {
        Ok(r) => r,
        Err(f) => {
            let (kind, detail) = fault_detail(&f);
            return plain(kind, detail);
        }
    };
    let tags = ask(target, None, "/api/tags", deadline).ok();
    let (openai, ollama) = (classify::openai_models(&models), tags.as_ref().and_then(classify::ollama_tags));
    let keyed = classify::key_needed(&models);
    let mut apis = Vec::new();
    if openai.is_some() || keyed {
        apis.push("openai");
    }
    if ollama.is_some() {
        apis.push("ollama");
    }
    let (kind, auth, list) = match (&openai, &ollama) {
        (_, Some(tags)) => ("ollama", "none_needed", tags.clone()),
        (Some(list), None) => ("openai", "none_needed", list.clone()),
        (None, None) if keyed => ("openai", "key_needed", Models::default()),
        _ => ("http-other", "unknown", Models::default()),
    };
    Found { kind, apis, auth, models: list, detail: None }
}

fn endpoint(listener: &Listener, owner: Option<&String>, found: Found, probe_ms: u64) -> Value {
    json!({
        "port": listener.port, "bind": listener.bind.word(), "address": listener.ip.to_string(), "kind": found.kind, "apis": found.apis, "auth": found.auth,
        "models": found.models.ids, "models_total": found.models.total, "models_truncated": found.models.truncated,
        "process": owner, "probe_ms": probe_ms, "detail": found.detail,
    })
}

/// The whole run against the process table under `root`: list, filter by scope, probe at most `MAX_PROBED` sockets `PARALLEL` at a time inside `budget`.
pub fn run(root: &Path, all_users: bool, budget: Duration) -> Value {
    let started = Instant::now();
    let scope = if all_users { "all_users" } else { "same_user" };
    if !cfg!(target_os = "linux") {
        return unsupported(scope, "os_unknown", started);
    }
    let Some(all) = net::listeners(root) else { return unsupported(scope, "proc_unreadable", started) };
    let owners = net::owners(root);
    let mut chosen: Vec<Listener> = all.into_iter().filter(|l| all_users || owners.contains_key(&l.inode)).collect();
    chosen.sort_by(|a, b| (a.port, a.ip).cmp(&(b.port, b.ip)));
    chosen.dedup_by(|a, b| a.port == b.port && a.ip == b.ip);
    let listening = chosen.len();
    let probed = &chosen[..listening.min(MAX_PROBED)];
    let deadline = started + budget;
    // Stage 1: first contact with every socket at once, in silence: the quiet period (a server that speaks first, SSH or a database, says so inside it) is waited for once
    // for all of them. Stage 2: the two questions, `PARALLEL` at a time, only for the sockets that stayed quiet.
    let first: Vec<(Step, u64)> = std::thread::scope(|scope_| {
        let handles: Vec<_> = probed
            .iter()
            .map(|l| {
                scope_.spawn(move || {
                    let t0 = Instant::now();
                    let step = match http::first_contact(l.target(), deadline) {
                        Contact::Speaks => Step::Done(plain("not-http", "speaks_first")),
                        Contact::Closed => Step::Done(plain("not-http", "closed")),
                        Contact::Failed(why) => Step::Done(plain("unknown", why)),
                        Contact::Quiet(stream) => Step::Quiet(stream),
                    };
                    (step, t0.elapsed().as_millis() as u64)
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap_or_else(|_| (Step::Done(plain("unknown", "io")), 0))).collect()
    });
    let waiting: Vec<Mutex<Option<(Step, u64)>>> = first.into_iter().map(|f| Mutex::new(Some(f))).collect();
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<(usize, Value)>> = Mutex::new(Vec::new());
    std::thread::scope(|scope_| {
        for _ in 0..PARALLEL.min(probed.len()) {
            scope_.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                let Some(listener) = probed.get(i) else { break };
                let Some((step, spent)) = waiting[i].lock().unwrap().take() else { continue };
                let t0 = Instant::now();
                let found = match step {
                    Step::Done(found) => found,
                    Step::Quiet(stream) => probe(listener, stream, deadline),
                };
                let value = endpoint(listener, owners.get(&listener.inode), found, spent + t0.elapsed().as_millis() as u64);
                results.lock().unwrap().push((i, value));
            });
        }
    });
    let mut endpoints = results.into_inner().unwrap();
    endpoints.sort_by_key(|(i, v)| (!matches!(v["kind"].as_str(), Some("openai" | "ollama")), *i)); // the servers first, then the other ports
    let mut out = base(scope);
    out.insert("listening".into(), json!(listening));
    out.insert("probed".into(), json!(probed.len()));
    out.insert("skipped".into(), json!(listening - probed.len()));
    out.insert("elapsed_ms".into(), json!(started.elapsed().as_millis() as u64));
    out.insert("endpoints".into(), Value::Array(endpoints.into_iter().map(|(_, v)| v).collect()));
    Value::Object(out)
}
