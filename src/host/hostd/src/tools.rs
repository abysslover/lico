//! The tool connection of a Claude session: a small JSON-RPC server on the loopback that the program in the session is told about by
//! `--mcp-config FILE`. The host binds it at the first Claude session and keeps it for its life. Every provider process gets a key of its own
//! (a key id and a secret, in memory and in the config file, nowhere else); the key ends with the process, the session or its generation.
//! The checks come in the order of `http.rs`: the head first (Host, Origin, credential, rate), the body only for a request that has passed.

use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use crate::core::{self, Shared};
use crate::http::{self, Bucket};
use crate::mcp::{self, Outcome};

const MAX_BODY: usize = 128 * 1024;
const MAX_CONNECTIONS: usize = 32;
const STACK: usize = 256 * 1024; // a connection only parses a small message and writes a small answer
const DEADLINE: Duration = Duration::from_secs(5);
const RATE_PER_SECOND: f64 = 100.0;
const BURST: f64 = 50.0;
const FAILED_PER_SECOND: f64 = 20.0;
const CONFIG_SUFFIX: &str = ".mcp.json";

struct Key {
    session: String,
    generation: u64,
    secret: String,
}

/// The keys that are valid now, by key id. A session has at most one: a new key replaces the one before it.
#[derive(Default)]
struct Table {
    keys: HashMap<String, Key>,
}

pub struct Grant {
    pub key: String,
    pub path: PathBuf,
}

/// The file a session's config is written to: the id of a session is plain (letters, digits, dash, underscore), so it cannot leave the folder.
fn file_of(dir: &Path, session: &str) -> std::io::Result<PathBuf> {
    if session.is_empty() || session.len() > 64 || !session.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
        return Err(std::io::ErrorKind::InvalidInput.into());
    }
    Ok(dir.join(format!("{session}{CONFIG_SUFFIX}")))
}

fn write_config(path: &Path, port: u16, key: &str, secret: &str) -> std::io::Result<()> {
    let config = json!({"mcpServers": {"lico": {"type": "http", "url": format!("http://127.0.0.1:{port}/mcp"), "headers": {"Authorization": format!("Bearer {key}.{secret}")}}}});
    // O_EXCL: a file that is there is never followed or written into
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)?;
    file.write_all(config.to_string().as_bytes())
}

/// The key id and the secret of an `Authorization` header, when they have the shape the host makes.
fn parse(header: &str) -> Option<(&str, &str)> {
    let (key, secret) = header.strip_prefix("Bearer ")?.split_once('.')?;
    (key.len() == 32 && secret.len() == 64).then_some((key, secret))
}

impl Table {
    fn issue(&mut self, dir: &Path, port: u16, session: &str, generation: u64) -> std::io::Result<Grant> {
        let path = file_of(dir, session)?;
        self.keys.retain(|_, k| k.session != session);
        let _ = std::fs::remove_file(&path);
        let (key, secret) = (core::random_hex(16), core::random_hex(32));
        write_config(&path, port, &key, &secret)?;
        self.keys.insert(key.clone(), Key { session: session.to_string(), generation, secret });
        Ok(Grant { key, path })
    }

    /// The session and generation a credential belongs to.
    fn lookup(&self, header: &str) -> Option<(String, u64)> {
        let (key, secret) = parse(header)?;
        let known = self.keys.get(key)?;
        http::constant_time_eq(secret.as_bytes(), known.secret.as_bytes()).then(|| (known.session.clone(), known.generation))
    }

    /// The end of the process a key was made for. A key that a newer one has replaced is gone already, and what the newer one made is left alone.
    fn release(&mut self, dir: &Path, key: &str) {
        if let Some(gone) = self.keys.remove(key) {
            if let Ok(path) = file_of(dir, &gone.session) {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    fn forget(&mut self, dir: &Path, session: &str) {
        self.keys.retain(|_, k| k.session != session);
        if let Ok(path) = file_of(dir, session) {
            let _ = std::fs::remove_file(path);
        }
    }

    fn clear(&mut self, dir: &Path) {
        self.keys.clear();
        remove_configs(dir);
    }
}

fn remove_configs(dir: &Path) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if entry.file_name().to_string_lossy().ends_with(CONFIG_SUFFIX) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[derive(Default)]
struct State {
    table: Table,
    dir: Option<PathBuf>,
    port: Option<u16>,
}

fn state() -> MutexGuard<'static, State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(Mutex::default).lock().unwrap_or_else(|e| e.into_inner())
}

/// At host start: the folder of the config files exists, owner-only, and holds none of an earlier host's (their keys died with it).
pub fn reset(dir: PathBuf) -> std::io::Result<()> {
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir)?;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    remove_configs(&dir);
    state().dir = Some(dir);
    Ok(())
}

/// A new key for the provider process that is about to start, and its config file. The listener is bound here, the first time.
pub fn issue(shared: &Arc<Shared>, session: &str, generation: u64) -> Result<Grant, &'static str> {
    let mut state = state();
    let dir = state.dir.clone().ok_or("busy")?;
    let port = match state.port {
        Some(port) => port,
        None => {
            let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|_| "busy")?;
            let port = listener.local_addr().map_err(|_| "busy")?.port();
            let shared = shared.clone();
            thread::Builder::new().stack_size(STACK).spawn(move || serve(listener, shared, port)).map_err(|_| "busy")?;
            state.port = Some(port);
            port
        }
    };
    state.table.issue(&dir, port, session, generation).map_err(|_| "busy")
}

/// The provider process a key was made for has ended (or never started).
pub fn release(key: &str) {
    let mut state = state();
    if let Some(dir) = state.dir.clone() {
        state.table.release(&dir, key);
    }
}

/// A session is closed or deleted: whatever keys it has end now.
pub fn forget(session: &str) {
    let mut state = state();
    if let Some(dir) = state.dir.clone() {
        state.table.forget(&dir, session);
    }
}

/// The host stops: no key and no config file is left.
pub fn clear() {
    let mut state = state();
    if let Some(dir) = state.dir.clone() {
        state.table.clear(&dir);
    }
}

/// The caller of a credential, when it is still the live process of its session: the session exists, is not deleted, and is of the generation the key was made for.
fn caller(shared: &Shared, header: &str) -> Option<String> {
    let (session, generation) = state().table.lookup(header)?;
    let found = core::find(shared, &session)?;
    let s = found.lock().unwrap();
    (!s.purged && s.generation == generation).then_some(session)
}

/// The other sessions of the host that run a provider: five fields each, nothing of their folder, account, model or history.
fn sessions(shared: &Shared, caller: &str) -> String {
    let list: Vec<_> = shared.sessions.read().unwrap().clone();
    let rows: Vec<Value> = list
        .iter()
        .filter_map(|s| {
            let s = s.lock().unwrap();
            // a name was checked where it came in (create, rename, the registry): at most 60 characters, no control or hidden character
            (s.id != caller && !s.purged && !s.provider.is_empty()).then(|| json!({"id": s.id, "name": s.settings.name, "kind": s.provider, "state": core::state_name(&s), "busy": s.busy}))
        })
        .collect();
    json!({"sessions": rows}).to_string()
}

fn run(shared: &Shared, caller: &str, name: &str, _arguments: &Map<String, Value>) -> Result<String, String> {
    match name {
        "sessions" => Ok(sessions(shared, caller)),
        _ => Err("unknown tool".into()),
    }
}

struct Limits {
    rate: Mutex<Bucket>,
    failed: Mutex<Bucket>,
}

/// Answer without reading more of the request, then read what the client still sends so that it can see the answer.
fn refuse(stream: &mut TcpStream, status: u16, extra: &str, message: &str) {
    http::write_response(stream, status, &mcp::failure(Value::Null, -32000, message), extra);
    http::drain(stream);
}

fn serve_one(mut stream: TcpStream, shared: &Shared, port: u16, limits: &Limits) {
    let _ = stream.set_read_timeout(Some(DEADLINE));
    let _ = stream.set_write_timeout(Some(DEADLINE));
    let head = match http::read_head(&mut stream) {
        Ok(head) => head,
        Err(status) => return refuse(&mut stream, status, "", "bad request"),
    };
    // Host and Origin, then the credential, then the rate, all on the head: the body (up to 128 KiB, and the client may never send it) is read only for
    // a request that has passed. A program does not send an Origin; a page in a browser does, so any Origin is refused.
    let request = &head.request;
    if !matches!(http::header(request, "host"), Some(h) if h == format!("127.0.0.1:{port}") || h == format!("localhost:{port}")) || http::header(request, "origin").is_some() {
        return refuse(&mut stream, 403, "", "forbidden");
    }
    let Some(session) = caller(shared, http::header(request, "authorization").unwrap_or("")) else {
        // failed attempts have a small budget of their own, so guessing is slow and cannot starve a real caller
        return if http::take(&limits.failed, FAILED_PER_SECOND, FAILED_PER_SECOND) {
            refuse(&mut stream, 401, "WWW-Authenticate: Bearer\r\n", "unauthorized")
        } else {
            refuse(&mut stream, 429, "Retry-After: 1\r\n", "too many requests")
        };
    };
    if !http::take(&limits.rate, RATE_PER_SECOND, BURST) {
        return refuse(&mut stream, 429, "Retry-After: 1\r\n", "too many requests");
    }
    if request.path != "/mcp" {
        return refuse(&mut stream, 404, "", "not found");
    }
    if request.method != "POST" {
        return refuse(&mut stream, 405, "Allow: POST\r\n", "method not allowed");
    }
    let request = match http::read_body(&mut stream, head, MAX_BODY) {
        Ok(request) => request,
        Err(status) => return refuse(&mut stream, status, "", if status == 413 { "too large" } else { "bad request" }),
    };
    match mcp::handle(&request.body, &mut |name, arguments| run(shared, &session, name, arguments)) {
        Outcome::Accepted => http::write_response(&mut stream, 202, "", ""),
        Outcome::Reply(text) => http::write_response(&mut stream, 200, &text, ""),
        Outcome::Malformed(text) => http::write_response(&mut stream, 400, &text, ""),
    }
}

fn serve(listener: TcpListener, shared: Arc<Shared>, port: u16) {
    let active = Arc::new(AtomicUsize::new(0));
    let limits = Arc::new(Limits {
        rate: Mutex::new(Bucket { tokens: BURST, at: Instant::now() }),
        failed: Mutex::new(Bucket { tokens: FAILED_PER_SECOND, at: Instant::now() }),
    });
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else {
            thread::sleep(Duration::from_millis(50)); // out of descriptors or a failed accept: do not spin
            continue;
        };
        if active.load(Ordering::Relaxed) >= MAX_CONNECTIONS {
            http::write_response(&mut stream, 503, &mcp::failure(Value::Null, -32000, "busy"), "");
            continue;
        }
        active.fetch_add(1, Ordering::Relaxed);
        let (shared, limits, counter) = (shared.clone(), limits.clone(), active.clone());
        let spawned = thread::Builder::new().stack_size(STACK).spawn(move || {
            serve_one(stream, &shared, port, &limits);
            counter.fetch_sub(1, Ordering::Relaxed);
        });
        if spawned.is_err() {
            active.fetch_sub(1, Ordering::Relaxed); // no thread to be had: the connection is dropped
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    /// A private folder that is gone when the test ends, whether it passes or not.
    struct Folder(PathBuf);

    impl Folder {
        fn new() -> Folder {
            let path = std::env::temp_dir().join(format!("lico-tools-test-{}", core::random_hex(6)));
            std::fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
            Folder(path)
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn header_of(grant: &Grant, table: &Table) -> String {
        let key = table.keys.get(&grant.key).unwrap();
        format!("Bearer {}.{}", grant.key, key.secret)
    }

    fn config_of(grant: &Grant) -> Value {
        serde_json::from_str(&std::fs::read_to_string(&grant.path).unwrap()).unwrap()
    }

    #[test]
    fn a_key_comes_with_a_private_config_file_of_the_shape_the_program_reads() {
        let folder = Folder::new();
        let mut table = Table::default();
        let grant = table.issue(&folder.0, 4242, "abc-1_x", 3).unwrap();
        assert_eq!(grant.path, folder.0.join("abc-1_x.mcp.json"));
        assert_eq!(std::fs::metadata(&grant.path).unwrap().mode() & 0o777, 0o600);
        let header = header_of(&grant, &table);
        assert_eq!(config_of(&grant), json!({"mcpServers": {"lico": {"type": "http", "url": "http://127.0.0.1:4242/mcp", "headers": {"Authorization": header}}}}));
        assert_eq!(header.len(), "Bearer ".len() + 32 + 1 + 64);
        assert_eq!(table.lookup(&header), Some(("abc-1_x".to_string(), 3)));
    }

    #[test]
    fn a_credential_is_known_only_whole_and_exactly() {
        let folder = Folder::new();
        let mut table = Table::default();
        let grant = table.issue(&folder.0, 1, "s1", 1).unwrap();
        let good = header_of(&grant, &table);
        let wrong_secret = format!("{}{}", &good[..good.len() - 4], "0000");
        let key = &grant.key;
        for bad in [String::new(), "Bearer".into(), good.replacen("Bearer", "bearer", 1), good[7..].to_string(), wrong_secret, format!("Bearer {key}"), format!("Bearer {key}."), format!("Bearer {key}.{}", "0".repeat(64)),
                    format!("Bearer {}.{}", "0".repeat(32), &good[7 + 33..]), format!("{good} "), format!("{good}0")] {
            assert_eq!(table.lookup(&bad), None, "{bad}");
        }
        assert!(table.lookup(&good).is_some());
    }

    #[test]
    fn a_session_has_one_key_and_a_new_one_takes_the_place_of_the_old_with_its_file() {
        let folder = Folder::new();
        let mut table = Table::default();
        let first = table.issue(&folder.0, 1, "s1", 1).unwrap();
        let other = table.issue(&folder.0, 1, "s2", 1).unwrap();
        let before = header_of(&first, &table);
        let second = table.issue(&folder.0, 1, "s1", 2).unwrap();
        assert_eq!(first.path, second.path);
        assert_eq!(table.lookup(&before), None);
        assert_eq!(table.lookup(&header_of(&second, &table)), Some(("s1".to_string(), 2)));
        assert_eq!(table.keys.len(), 2);
        assert_eq!(config_of(&second)["mcpServers"]["lico"]["headers"]["Authorization"], header_of(&second, &table));
        assert!(other.path.exists());
    }

    #[test]
    fn the_end_of_an_old_process_leaves_what_a_newer_one_made() {
        // F10: a closed process's reader ends after the resume has made its key and its file
        let folder = Folder::new();
        let mut table = Table::default();
        let old = table.issue(&folder.0, 1, "s1", 1).unwrap();
        let new = table.issue(&folder.0, 1, "s1", 2).unwrap();
        table.release(&folder.0, &old.key);
        assert!(new.path.exists() && table.lookup(&header_of(&new, &table)).is_some());
        table.release(&folder.0, &new.key);
        assert!(!new.path.exists() && table.keys.is_empty());
        table.release(&folder.0, &new.key); // twice is nothing
    }

    #[test]
    fn forget_takes_the_keys_and_the_file_of_a_session_and_clear_takes_all_config_files_and_nothing_else() {
        let folder = Folder::new();
        let mut table = Table::default();
        let one = table.issue(&folder.0, 1, "s1", 1).unwrap();
        let two = table.issue(&folder.0, 1, "s2", 1).unwrap();
        std::fs::write(folder.0.join("notes.txt"), "not a config").unwrap();
        table.forget(&folder.0, "s1");
        assert!(!one.path.exists() && two.path.exists() && table.keys.len() == 1);
        table.forget(&folder.0, "never-had-one");
        table.clear(&folder.0);
        assert!(!two.path.exists() && table.keys.is_empty() && folder.0.join("notes.txt").exists());
    }

    #[test]
    fn a_session_id_that_is_not_plain_is_refused_and_writes_nothing() {
        let folder = Folder::new();
        let mut table = Table::default();
        for bad in ["", "../x", "a/b", "a b", ".hidden", "x\0y", &"a".repeat(65), "한글"] {
            assert!(table.issue(&folder.0, 1, bad, 1).is_err(), "{bad:?}");
        }
        assert!(table.keys.is_empty() && std::fs::read_dir(&folder.0).unwrap().next().is_none());
    }

    #[test]
    fn reset_makes_the_folder_owner_only_and_empties_the_config_files_of_an_earlier_host() {
        let folder = Folder::new();
        let dir = folder.0.join("tools");
        std::fs::DirBuilder::new().mode(0o755).create(&dir).unwrap();
        std::fs::write(dir.join("old.mcp.json"), "{}").unwrap();
        std::fs::write(dir.join("keep.txt"), "mine").unwrap();
        reset(dir.clone()).unwrap();
        assert_eq!(std::fs::metadata(&dir).unwrap().mode() & 0o777, 0o700);
        assert!(!dir.join("old.mcp.json").exists() && dir.join("keep.txt").exists());
    }
}
