//! Host operations shared by the local IPC and the optional loopback HTTP listener.
//! Every function validates its target and returns either a reply body or a v1
//! error code; nothing here depends on how the request arrived.

use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use crate::payload;
use crate::session::{obj, rec_line, InputState, LogView, Session, State, Sub};

pub const MAX_REQUEST: usize = 4096;
pub const OPS: [&str; 28] = [
    "hello", "status", "snapshot", "subscribe", "unsubscribe", "session.create", "session.list", "session.resume", "session.purge", "input",
    "input.status", "approval.answer", "view.attach", "view.detach", "resources", "account.create", "account.list", "account.default", "session.rebind", "account.credential", "account.affected", "account.remove", "history",
    "account.login.start", "account.login.input", "account.login.status", "account.login.cancel", "host.stop",
];
pub const REPLAY_CHUNK: usize = 256;
const MAX_EVENTS: usize = 256;

pub struct Config {
    pub login_bin: Option<PathBuf>, // a test double for the providers' login commands
    pub login_args: Vec<String>,
    pub login_timeout: u64, // seconds a login may last
    pub log_dir: PathBuf,
    pub agent: PathBuf,
    pub retain: usize,
    pub window: usize,
    pub queue_bytes: usize,
    pub payload: usize,
    pub max_sessions: usize,
    pub codex: PathBuf,
    pub codex_args: Vec<String>,
    pub claude: PathBuf,
    pub claude_args: Vec<String>, // placed before the flags the adapter adds (a simulator is started this way)
    pub cred_bin: PathBuf,        // the credential command a provider calls as its key helper
    pub cred_args: Vec<String>,   // its leading arguments (`cred`, when the command is this very program)
    pub cred_dir: PathBuf,        // the credential metadata directory
    pub workspace_root: PathBuf,
    pub accounts_dir: PathBuf,
    pub registry_dir: PathBuf,
    pub text: bool, // synthetic sessions produce UTF-8 text lines (a measurement option)
    pub realistic_text: bool, // ... with ordinary words only: no right-to-left word, no combining accent (implies text)
}

const MAX_AGENT_LINE: usize = 16 * 1024;
const MAX_VIEWS_PER_CONNECTION: usize = 64;

pub struct ViewEntry {
    pub session: String,
    pub conn: Option<Arc<Sub>>,
    pub created: Instant,
}

pub struct Shared {
    pub cfg: Config,
    pub sessions: RwLock<Vec<Arc<Mutex<Session>>>>,
    pub views: Mutex<HashMap<String, ViewEntry>>,
    pub started: Instant,
    pub pending_creates: AtomicUsize,
    pub accounts: Mutex<crate::accounts::Accounts>,
    pub logins: crate::login::Logins,
}

pub type Reply = Result<Map<String, Value>, &'static str>;

type SpawnJob = (Command, mpsc::Sender<std::io::Result<Child>>);
static SPAWNER: std::sync::OnceLock<Mutex<mpsc::Sender<SpawnJob>>> = std::sync::OnceLock::new();

/// Agents are all spawned from one long-lived thread: the parent-death signal follows the
/// thread that created the child, so an agent created from a request thread would be killed
/// when that thread (a closed connection) ends.
pub fn init_spawner() {
    let (tx, rx) = mpsc::channel::<SpawnJob>();
    thread::spawn(move || {
        for (mut command, reply) in rx {
            let _ = reply.send(command.spawn());
        }
    });
    let _ = SPAWNER.set(Mutex::new(tx));
}

pub fn spawn_provider_process(mut command: Command) -> std::io::Result<Child> {
    unsafe {
        command.pre_exec(|| {
            prctl(1, 9, 0, 0, 0); // PR_SET_PDEATHSIG, SIGKILL: no provider outlives its host
            setsid(); // its own process group, so a stop reaches the helpers it starts too
            Ok(())
        });
    }
    let child = spawn_agent(command)?;
    note_provider(child.id());
    Ok(child)
}

/// The provider processes of this host with the start time of each, kept OUTSIDE every session lock: the stop reaches a provider even when its
/// session is stuck behind a lock that is held for long or poisoned.
static PROVIDERS: Mutex<Vec<(u32, String)>> = Mutex::new(Vec::new());

/// (start time, process group) of a process; the fields are counted after the last `)` because the name may hold spaces and brackets.
fn stat_start_group(pid: u32) -> Option<(String, u32)> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let fields: Vec<&str> = stat.rsplit_once(") ")?.1.split(' ').collect();
    Some((fields.get(19)?.to_string(), fields.get(2)?.parse().ok()?))
}

pub(crate) fn note_provider(pid: u32) {
    let mut list = PROVIDERS.lock().unwrap_or_else(|e| e.into_inner());
    list.retain(|(p, start)| stat_start_group(*p).map_or(false, |(s, _)| s == *start)); // the ones that ended are forgotten
    if let Some((start, _)) = stat_start_group(pid) {
        list.push((pid, start));
    }
}

/// Kill the group of every provider that is still the process that was noted (a pid that was reused is not signalled) and still leads its group.
fn end_listed_groups() {
    let list = PROVIDERS.lock().unwrap_or_else(|e| e.into_inner()).clone();
    for (pid, start) in list {
        if stat_start_group(pid).map_or(false, |(s, group)| s == start && group == pid) {
            unsafe { kill(-(pid as i32), 9) };
        }
    }
}

/// End every provider of this host. It returns whatever state the sessions are in: the groups are killed by the pids kept outside the locks first,
/// then each session is taken with `try_lock` (a poisoned lock is used through its inner value; a lock that stays held is given up after `wait`).
pub fn end_providers(sessions: &[Arc<Mutex<Session>>], wait: Duration) {
    end_listed_groups();
    let until = Instant::now() + wait;
    for session in sessions {
        loop {
            match session.try_lock() {
                Ok(mut s) => {
                    s.stop_process();
                    break;
                }
                Err(std::sync::TryLockError::Poisoned(p)) => {
                    p.into_inner().stop_process();
                    break;
                }
                Err(std::sync::TryLockError::WouldBlock) if Instant::now() < until => thread::sleep(Duration::from_millis(10)),
                Err(std::sync::TryLockError::WouldBlock) => break,
            }
        }
    }
}

fn spawn_agent(command: Command) -> std::io::Result<Child> {
    let (reply, wait) = mpsc::channel();
    SPAWNER
        .get()
        .expect("spawner")
        .lock()
        .unwrap()
        .send((command, reply))
        .map_err(|_| std::io::Error::other("spawner stopped"))?;
    wait.recv().map_err(|_| std::io::Error::other("spawner stopped"))?
}

extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
    fn prctl(option: i32, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> i32;
    fn setsid() -> i32;
}

pub fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut buf)).expect("urandom");
    payload::hex(&buf)
}

pub fn state_name(s: &Session) -> &'static str {
    match s.state {
        State::Running => "running",
        State::Exited => "exited",
        State::Disconnected => "disconnected",
    }
}

// ------------------------------------------------------------------- agents
/// Start a synthetic agent owned by this host; it ends when the host does.
pub fn start_session(shared: &Arc<Shared>, producing: bool, rate: u64, history: u64, size: usize, stay: bool) -> Result<Arc<Mutex<Session>>, &'static str> {
    let id = format!("s{}", random_hex(8));
    let session = Arc::new(Mutex::new(Session::new(id, producing, &shared.cfg.log_dir, shared.cfg.retain, shared.cfg.window)));
    let mut command = Command::new(&shared.cfg.agent);
    command
        .args(["--history", &history.to_string(), "--rate", &rate.to_string(), "--payload", &size.to_string()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if stay {
        command.arg("--stay");
    }
    if shared.cfg.text {
        command.arg(if shared.cfg.realistic_text { "--realistic-text" } else { "--text" });
        session.lock().unwrap().text_mode = true;
    }
    command.env_clear(); // an agent gets no part of the host's environment
    for key in ["PATH", "LANG"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    unsafe {
        command.pre_exec(|| {
            prctl(1, 9, 0, 0, 0); // PR_SET_PDEATHSIG, SIGKILL: no agent outlives its host
            Ok(())
        });
    }
    let mut child = spawn_agent(command).map_err(|_| "busy")?;
    let stdout = child.stdout.take().ok_or("busy")?;
    let mut stdin = child.stdin.take().ok_or("busy")?;
    let (tx, rx) = mpsc::sync_channel::<String>(1024);
    {
        let mut s = session.lock().unwrap();
        s.agent_pid = child.id();
        s.input_tx = Some(tx);
    }
    // a writer thread owns the agent's input pipe, so a stalled agent never blocks the host
    let writer_session = session.clone();
    thread::spawn(move || {
        for line in rx {
            let request_id = line.strip_prefix("I ").and_then(|rest| rest.split(' ').next()).map(str::to_string);
            if stdin.write_all(line.as_bytes()).and_then(|_| stdin.write_all(b"\n")).and_then(|_| stdin.flush()).is_err() {
                return;
            }
            if let Some(rid) = request_id {
                writer_session.lock().unwrap().advance_input(&rid, InputState::Submitted);
            }
        }
    });
    let owner = session.clone();
    thread::spawn(move || {
        for line in bounded_lines(stdout, MAX_AGENT_LINE) {
            let mut parts = line.splitn(3, ' ');
            match (parts.next(), parts.next(), parts.next()) {
                (Some("R"), Some(data), None) => {
                    if let Some(bytes) = payload::unhex(data) {
                        owner.lock().unwrap().append(bytes);
                    }
                }
                (Some("E"), Some(rid), None) => owner.lock().unwrap().advance_input(rid, InputState::Observed),
                (Some("A"), Some(n), Some(prompt)) => {
                    if let Ok(number) = n.parse::<i64>() {
                        owner.lock().unwrap().add_approval(number, prompt.to_string());
                    }
                }
                _ => {}
            }
        }
        let _ = child.wait();
        let mut s = owner.lock().unwrap();
        s.set_state(State::Exited);
        s.input_tx = None;
    });
    // the first records exist before the session is announced
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        let ready = {
            let s = session.lock().unwrap();
            s.next_seq as u64 >= history || s.state == State::Exited
        };
        if ready {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    shared.sessions.write().unwrap().push(session.clone());
    Ok(session)
}

/// Lines of an agent's output with a length limit: a longer line is dropped whole, so an
/// agent that never ends a line cannot grow the host.
pub fn bounded_lines(stream: std::process::ChildStdout, limit: usize) -> impl Iterator<Item = String> {
    let mut reader = BufReader::new(stream);
    std::iter::from_fn(move || loop {
        let mut buf = Vec::new();
        let n = reader.by_ref().take(limit as u64).read_until(b'\n', &mut buf).ok()?;
        if n == 0 {
            return None;
        }
        if buf.last() != Some(&b'\n') {
            // too long without a newline: skip to the end of this line
            let mut skip = Vec::new();
            loop {
                skip.clear();
                match reader.by_ref().take(limit as u64).read_until(b'\n', &mut skip) {
                    Ok(0) | Err(_) => return None,
                    Ok(_) if skip.last() == Some(&b'\n') => break,
                    Ok(_) => continue,
                }
            }
            continue;
        }
        buf.pop();
        if let Ok(text) = String::from_utf8(buf) {
            return Some(text);
        }
    })
}

pub fn find(shared: &Shared, wanted: &str) -> Option<Arc<Mutex<Session>>> {
    shared.sessions.read().unwrap().iter().find(|s| s.lock().unwrap().id == wanted).cloned()
}

/// A target of another execution generation is stale: nothing is dispatched to it.
fn check_generation(s: &Session, generation: Option<i64>) -> Result<(), &'static str> {
    match generation {
        Some(g) if g != s.generation as i64 => Err("stale_generation"),
        _ => Ok(()),
    }
}

// --------------------------------------------------------------- operations
pub fn hello(shared: &Shared) -> Reply {
    Ok(obj(json!({"protocol": 1, "host": "lico-host", "max_request": MAX_REQUEST, "retain": shared.cfg.retain, "ops": OPS})))
}

fn status_entry(s: &Session) -> Value {
    let mut entry = json!({
        "session": s.id, "generation": s.generation, "seq": s.last_seq(), "producing": s.producing,
        "subscribers": s.subs.len(), "bytes_total": s.bytes_total, "oldest": s.oldest(), "state": state_name(s), "state_at": s.state_at,
    });
    if !s.provider.is_empty() {
        entry["provider"] = json!(s.provider);
        entry["mode"] = json!(s.mode);
    }
    if let Some(model) = &s.model {
        entry["model"] = json!(model);
    }
    if !s.provider.is_empty() && s.state == State::Running {
        entry["activity"] = json!(if s.busy { "working" } else { "idle" });
    }
    if let Some(account) = &s.account {
        entry["account"] = json!(account);
        entry["credential_generation"] = json!(s.credential_generation);
    }
    if let Some(issue) = &s.account_issue {
        entry["account_issue"] = json!(issue);
    }
    if !s.approvals.is_empty() {
        entry["approvals"] = Value::Array(s.approvals.iter().map(|(n, p)| json!({"approval_id": n, "prompt": p})).collect());
    }
    entry
}

pub fn status(shared: &Shared) -> Reply {
    let list: Vec<Arc<Mutex<Session>>> = shared.sessions.read().unwrap().clone();
    let entries: Vec<Value> = list.iter().map(|s| status_entry(&s.lock().unwrap())).collect();
    Ok(obj(json!({"pid": std::process::id(), "build": build_id(), "logins_running": crate::login::running_count(shared), "sessions": entries})))
}

/// Which executable this process runs: a hash of its own file as it was started (a replaced file on disk does not change it), so a window and a host
/// can tell whether they come from the same build.
pub fn build_id() -> &'static str {
    static ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ID.get_or_init(|| {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in std::fs::read("/proc/self/exe").unwrap_or_default() {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
        format!("{h:016x}")
    })
}

/// End this host (the explicit way to replace it): refused while sessions or a sign-in exist, unless forced. With `build` the stop is a
/// compare-and-stop: only a host of that build ends (`build_changed` otherwise), so a window that saw an old host cannot end the new one that another
/// window started in its place. A second stop while one is under way is refused (`stopping`). The registry is written first.
pub fn host_stop(shared: &Arc<Shared>, force: bool, build: Option<&str>) -> Reply {
    static STOPPING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if build.map_or(false, |b| b != build_id()) {
        return Err("build_changed");
    }
    let sessions = shared.sessions.read().unwrap_or_else(|e| e.into_inner()).len();
    if !force && (sessions > 0 || crate::login::running_count(shared) > 0) {
        return Err("busy");
    }
    if STOPPING.swap(true, Ordering::SeqCst) {
        return Err("stopping");
    }
    // The stop is the way out of a stuck host, so the reply must not wait for the state it gets out of: the registry save takes every session lock and
    // syncs a file, which can take seconds (a lock held by the log's fsync) or panic (a poisoned lock). It gets a budget of its own; a save that is
    // still running when the process ends changes nothing, the registry is renamed into place only at its end.
    let for_save = shared.clone();
    within(STOP_SAVE_BUDGET, move || crate::registry::save(&for_save));
    let shared = shared.clone();
    std::thread::spawn(move || {
        std::thread::sleep(STOP_REPLY_DELAY); // the reply goes out first
        // a provider left behind goes on with its turn (claude does, after its input ends) and would meet the one that resumes the session
        let until = Instant::now() + STOP_DEADLINE / 2;
        let sessions: Vec<_> = loop {
            match shared.sessions.try_read() {
                Ok(list) => break list.clone(),
                Err(std::sync::TryLockError::Poisoned(p)) => break p.into_inner().clone(),
                Err(std::sync::TryLockError::WouldBlock) if Instant::now() < until => thread::sleep(Duration::from_millis(10)),
                Err(std::sync::TryLockError::WouldBlock) => break Vec::new(),
            }
        };
        end_providers(&sessions, STOP_DEADLINE / 2);
        std::process::exit(0);
    });
    // Whatever the work above waits for (a session lock held for seconds, a lock that a panic poisoned), the process ends after at most this long
    // from the reply.
    std::thread::spawn(|| {
        std::thread::sleep(STOP_REPLY_DELAY + STOP_DEADLINE);
        std::process::exit(0);
    });
    Ok(obj(json!({"stopping": true, "sessions": sessions})))
}

/// Run `work` on a thread of its own and wait for it at most `budget`: true when it finished, false when it was still running (it goes on without the
/// caller) or panicked (nothing of the caller is poisoned). The caller is never held longer than the budget.
pub fn within(budget: Duration, work: impl FnOnce() + Send + 'static) -> bool {
    let (done, finished) = mpsc::channel();
    thread::spawn(move || {
        let _ = done.send(std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).is_ok());
    });
    finished.recv_timeout(budget).unwrap_or(false)
}

/// How long the reply of host.stop waits for the registry to be written.
const STOP_SAVE_BUDGET: Duration = Duration::from_millis(1500);

const STOP_REPLY_DELAY: Duration = Duration::from_millis(300);
/// The longest the stop takes after the reply, below the 3 s that a window waits for the old host to end.
const STOP_DEADLINE: Duration = Duration::from_millis(2400);

pub fn session_one(shared: &Shared, session: &str) -> Reply {
    let found = find(shared, session).ok_or("unknown_session")?;
    let s = found.lock().unwrap();
    Ok(obj(status_entry(&s)))
}

pub fn session_list(shared: &Shared) -> Reply {
    let list: Vec<Arc<Mutex<Session>>> = shared.sessions.read().unwrap().clone();
    let entries: Vec<Value> = list
        .iter()
        .map(|s| {
            let s = s.lock().unwrap();
            json!({"session": s.id, "generation": s.generation, "seq": s.last_seq(), "state": state_name(&s)})
        })
        .collect();
    Ok(obj(json!({"sessions": entries})))
}

pub fn session_create(shared: &Arc<Shared>, kind: &str, rate: u64, history: u64, size: usize, account: Option<&str>, mode: &str) -> Reply {
    let mode: &'static str = if mode == "default" { "default" } else { "auto" };
    // a bounded number of sessions, and so of agent processes; concurrent creators reserve a slot first
    let bound = match resolve_account(shared, kind, account) {
        Ok(bound) => bound,
        Err(code) => return Err(code), // refused before anything starts
    };
    let reserved = shared.pending_creates.fetch_add(1, Ordering::SeqCst);
    let total = shared.sessions.read().unwrap().len() + reserved;
    let created = if total >= shared.cfg.max_sessions {
        Err("busy")
    } else if kind == "codex" {
        start_codex_session(shared, bound, mode)
    } else if kind == "claude" {
        start_claude_session(shared, bound, mode)
    } else {
        start_session(shared, rate > 0, rate, history, size, false)
    };
    shared.pending_creates.fetch_sub(1, Ordering::SeqCst);
    let created = created?;
    let (id, generation) = {
        let s = created.lock().unwrap();
        (s.id.clone(), s.generation)
    };
    Ok(obj(json!({"session": id, "generation": generation})))
}

/// The account a new session is bound to: the one asked for, else the provider's default, else none
/// (a session without an account runs with the host's own provider home, as before accounts existed).
fn resolve_account(shared: &Shared, kind: &str, requested: Option<&str>) -> Result<Option<(String, u64)>, &'static str> {
    let accounts = shared.accounts.lock().unwrap();
    match (kind, requested) {
        ("codex" | "claude", Some(id)) => match accounts.get(id) {
            Some(a) if a.provider == kind => Ok(Some((a.id.clone(), a.credential_generation))),
            _ => Err("unknown_account"),
        },
        ("codex" | "claude", None) => match accounts.default_for(kind) {
            Some(a) => Ok(Some((a.id.clone(), a.credential_generation))),
            // accounts exist but none is the default: never fall back to an unbound session (the host's own provider home)
            None if accounts.any_for(kind) => Err("unknown_account"),
            None => Ok(None),
        },
        (_, Some(_)) => Err("bad_request"),
        (_, None) => Ok(None),
    }
}

pub fn account_create(shared: &Shared, provider: &str, label: &str) -> Reply {
    let account = shared.accounts.lock().unwrap().create(provider, label)?;
    Ok(obj(json!({"account": account.id, "provider": account.provider, "label": account.label})))
}

pub fn account_list(shared: &Shared) -> Reply {
    let mut entries = shared.accounts.lock().unwrap().entries();
    let sessions: Vec<Arc<Mutex<Session>>> = shared.sessions.read().unwrap().clone();
    for entry in entries.iter_mut() {
        let id = entry["account"].as_str().unwrap_or("").to_string();
        let mut affected: Vec<(String, String)> = Vec::new();
        for session in &sessions {
            let s = session.lock().unwrap();
            if s.account.as_deref() == Some(id.as_str()) {
                if let Some(issue) = &s.account_issue {
                    affected.push((s.id.clone(), issue.clone()));
                }
            }
        }
        if let Some((_, issue)) = affected.first() {
            entry["issue"] = json!(issue);
            entry["issue_sessions"] = json!(affected.iter().map(|(s, _)| s.clone()).collect::<Vec<_>>());
        }
        entry["usage"] = Value::Null; // no provider source reports usage yet: unknown is not zero
    }
    Ok(obj(json!({"accounts": entries})))
}

fn sessions_of(shared: &Shared, account: &str) -> Vec<Arc<Mutex<Session>>> {
    let all: Vec<Arc<Mutex<Session>>> = shared.sessions.read().unwrap().clone();
    all.into_iter().filter(|s| s.lock().unwrap().account.as_deref() == Some(account)).collect()
}

/// The sessions bound to an account, named before anything is removed.
pub fn account_affected(shared: &Shared, account: &str) -> Reply {
    shared.accounts.lock().unwrap().get(account).ok_or("unknown_account")?;
    let entries: Vec<Value> = sessions_of(shared, account).iter().map(|s| {
        let s = s.lock().unwrap();
        json!({"session": s.id, "state": state_name(&s)})
    }).collect();
    Ok(obj(json!({"account": account, "sessions": entries})))
}

/// Remove an account. `detach` leaves its sessions running (their histories and work stay, and the account's
/// home is kept for them); `end` purges them first. Removing the local record and the key is one outcome;
/// revoking the login at the provider is another and is not performed here.
pub fn account_remove(shared: &Arc<Shared>, account: &str, sessions: &str) -> Reply {
    let (reference, home_exists) = {
        let accounts = shared.accounts.lock().unwrap();
        accounts.get(account).ok_or("unknown_account")?;
        (accounts.credential_ref(account), accounts.homes(account).is_some())
    };
    // nothing is removed if the credential store cannot be reached: its state is checked before anything changes
    if reference.is_some() && !credential_command(shared, &["status"]) {
        return Err("credential_store_unavailable");
    }
    let _ = crate::login::cancel(shared, account); // a sign-in in progress ends with the account
    let affected: Vec<(String, u64)> = sessions_of(shared, account).iter().map(|s| {
        let s = s.lock().unwrap();
        (s.id.clone(), s.generation)
    }).collect();
    if sessions == "end" {
        for (id, generation) in &affected {
            let _ = session_purge(shared, id, *generation as i64);
        }
        // a session that resumed, or was created, meanwhile is still bound: the account stays rather than lose its home under it
        if !sessions_of(shared, account).is_empty() {
            return Err("busy");
        }
    }
    let mut credential = "none";
    if let Some(reference) = &reference {
        if !credential_command(shared, &["remove", "--ref", reference]) {
            return Err("credential_store_unavailable");
        }
        credential = "removed";
    }
    let keep_home = sessions == "detach" && !sessions_of(shared, account).is_empty();
    shared.accounts.lock().unwrap().remove(account, keep_home)?;
    Ok(obj(json!({
        "account": account, "metadata_removed": true, "credential": credential, "remote_revocation": "not_performed",
        "home": if !home_exists { "none" } else if keep_home { "kept" } else { "removed" },
        "affected": affected.iter().map(|(s, _)| s.clone()).collect::<Vec<_>>(),
    })))
}

/// Run the credential command (the keyring's bus is passed on); true when it succeeded.
fn credential_command(shared: &Shared, args: &[&str]) -> bool {
    let mut command = Command::new(&shared.cfg.cred_bin);
    command.args(&shared.cfg.cred_args).arg("--data-dir").arg(&shared.cfg.cred_dir).args(args).env_clear().stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    for key in ["PATH", "DBUS_SESSION_BUS_ADDRESS", "XDG_RUNTIME_DIR"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    command.status().map_or(false, |status| status.success())
}

/// Attach a credential of the store to an account: it must exist, belong to this account and its provider.
pub fn account_credential(shared: &Shared, account: &str, reference: &str) -> Reply {
    let provider = shared.accounts.lock().unwrap().get(account).ok_or("unknown_account")?.provider.clone();
    let known = crate::creds::find(&shared.cfg.cred_dir, reference).ok().flatten();
    match known {
        Some(entry) if entry["account"] == account && entry["provider"] == provider => {}
        _ => return Err("bad_request"),
    }
    let old = shared.accounts.lock().unwrap().credential_ref(account);
    shared.accounts.lock().unwrap().set_credential(account, reference)?;
    // a replaced key is not left behind in the keyring
    if let Some(old) = old.filter(|old| old != reference) {
        credential_command(shared, &["remove", "--ref", &old]);
    }
    Ok(obj(json!({"account": account})))
}

pub fn account_default(shared: &Shared, account: &str) -> Reply {
    shared.accounts.lock().unwrap().set_default(account)?;
    Ok(obj(json!({"account": account})))
}

/// Move a session whose provider is gone to another account: a new thread, a new generation. A
/// session that is running (in a turn or idle) is never rebound: its provider holds its account's login.
pub fn session_rebind(shared: &Arc<Shared>, session: &str, generation: i64, account: &str) -> Reply {
    let found = find(shared, session).ok_or("unknown_session")?;
    let bound = {
        let accounts = shared.accounts.lock().unwrap();
        let wanted = accounts.get(account).ok_or("unknown_account")?;
        (wanted.id.clone(), wanted.credential_generation, wanted.provider.clone())
    };
    let previous = {
        let mut s = found.lock().unwrap();
        check_generation(&s, Some(generation))?;
        if s.state != State::Disconnected || s.codex_thread.is_none() {
            return Err("busy");
        }
        if bound.2 != s.provider {
            return Err("unknown_account");
        }
        let previous = (s.account.clone(), s.credential_generation, s.codex_thread.take());
        s.account = Some(bound.0.clone());
        s.credential_generation = bound.1;
        s.account_issue = None; // a failure of the old account is not shown against the new one
        s.generation += 1; // every target of the old execution is stale from now on
        previous
    };
    if let Err(code) = start_provider(shared, &found) {
        let mut s = found.lock().unwrap();
        s.generation -= 1;
        s.account = previous.0;
        s.credential_generation = previous.1;
        s.codex_thread = previous.2;
        s.set_state(State::Disconnected);
        return Err(code);
    }
    let mut s = found.lock().unwrap();
    if s.provider == "claude" {
        crate::claude::note(&mut s, "[account changed]");
    } else {
        crate::codex::note(&mut s, "[account changed]");
    }
    Ok(obj(json!({"session": s.id, "generation": s.generation})))
}

fn start_provider(shared: &Arc<Shared>, session: &Arc<Mutex<Session>>) -> Result<(), &'static str> {
    let provider = session.lock().unwrap().provider;
    if provider == "claude" {
        crate::claude::start(shared, session)
    } else {
        crate::codex::start(shared, session)
    }
}

/// A session backed by the Claude Code program (or its simulator): one provider process, one provider session.
fn start_claude_session(shared: &Arc<Shared>, bound: Option<(String, u64)>, mode: &'static str) -> Result<Arc<Mutex<Session>>, &'static str> {
    let id = format!("s{}", random_hex(8));
    let session = Arc::new(Mutex::new(Session::new(id, false, &shared.cfg.log_dir, shared.cfg.retain, shared.cfg.window)));
    {
        let mut s = session.lock().unwrap();
        s.text_mode = true;
        s.mode = mode;
        s.provider = "claude";
        if let Some((account, credential_generation)) = bound {
            s.account = Some(account);
            s.credential_generation = credential_generation;
        }
    }
    if let Err(code) = crate::claude::start(shared, &session) {
        let mut s = session.lock().unwrap();
        s.purged = true;
        s.stop_process();
        s.purge_log();
        let _ = std::fs::remove_dir_all(shared.cfg.workspace_root.join(&s.id));
        return Err(code);
    }
    shared.sessions.write().unwrap().push(session.clone());
    crate::registry::save(shared);
    Ok(session)
}

/// A session backed by the Codex app-server (or its simulator): one provider process, one thread.
fn start_codex_session(shared: &Arc<Shared>, bound: Option<(String, u64)>, mode: &'static str) -> Result<Arc<Mutex<Session>>, &'static str> {
    let id = format!("s{}", random_hex(8));
    let session = Arc::new(Mutex::new(Session::new(id, false, &shared.cfg.log_dir, shared.cfg.retain, shared.cfg.window)));
    session.lock().unwrap().text_mode = true;
    session.lock().unwrap().mode = mode;
    if let Some((account, credential_generation)) = bound {
        let mut s = session.lock().unwrap();
        s.account = Some(account);
        s.credential_generation = credential_generation;
    }
    if let Err(code) = crate::codex::start(shared, &session) {
        // nothing of a session that never became ready may remain: no process, no log, no workspace
        let mut s = session.lock().unwrap();
        s.purged = true;
        s.stop_process();
        s.purge_log();
        let _ = std::fs::remove_dir_all(shared.cfg.workspace_root.join(&s.id));
        return Err(code);
    }
    shared.sessions.write().unwrap().push(session.clone());
    crate::registry::save(shared);
    Ok(session)
}

/// Delete a session: end its process, remove its log and its workspace, and forget it. Nothing of
/// another session is touched, and nothing of this one can be written again.
pub fn session_purge(shared: &Arc<Shared>, session: &str, generation: i64) -> Reply {
    let found = find(shared, session).ok_or("unknown_session")?;
    let (id, pid, subs) = {
        let mut s = found.lock().unwrap();
        check_generation(&s, Some(generation))?;
        s.purged = true;
        s.set_state(State::Exited);
        s.input_tx = None;
        s.codex = None;
        s.claude = None;
        s.approvals.clear();
        let pid = s.agent_pid;
        s.stop_process();
        s.purge_log();
        let subs: Vec<Arc<Sub>> = s.subs.drain(..).collect();
        (s.id.clone(), pid, subs)
    };
    let _ = pid;
    for sub in subs {
        // viewers are told to resynchronise; their next request finds no such session
        sub.push(crate::session::encode(obj(json!({"ev": "overflow", "session": id}))));
    }
    shared.sessions.write().unwrap().retain(|s| s.lock().unwrap().id != id);
    shared.views.lock().unwrap().retain(|_, v| v.session != id);
    let _ = std::fs::remove_dir_all(shared.cfg.workspace_root.join(&id));
    crate::registry::save(shared);
    Ok(obj(json!({"session": id})))
}

/// Bring a disconnected session back with a new provider process and a new generation.
/// Older records of a session, read from its log on demand: up to `limit` records before sequence `before`, oldest first.
pub fn history(shared: &Shared, session: &str, before: i64, limit: i64) -> Reply {
    let found = find(shared, session).ok_or("unknown_session")?;
    let (view, text_mode, oldest) = {
        let s = found.lock().unwrap();
        (s.log_view(), s.text_mode, s.oldest())
    };
    let limit = limit.clamp(1, 200);
    let from = (before - limit).max(oldest.min(before));
    let records = view.read(from, before, limit as usize).map_err(|_| "busy")?;
    let lines: Vec<Value> = records
        .iter()
        .map(|(seq, data)| {
            if text_mode {
                json!([seq, String::from_utf8_lossy(data)])
            } else {
                json!([seq, format!("{:08} {}", seq, payload::hex(&data[..data.len().min(44)]))])
            }
        })
        .collect();
    Ok(obj(json!({"session": session, "lines": lines, "oldest": oldest})))
}

pub fn session_resume(shared: &Arc<Shared>, session: &str, generation: i64) -> Reply {
    let found = find(shared, session).ok_or("unknown_session")?;
    {
        let mut s = found.lock().unwrap();
        check_generation(&s, Some(generation))?;
        if s.codex_thread.is_none() {
            return Err("bad_request");
        }
        s.generation += 1; // every target of the old execution is stale from now on
    }
    let provider = found.lock().unwrap().provider;
    let resumed = if provider == "claude" { crate::claude::resume(shared, &found) } else { crate::codex::resume(shared, &found) };
    if let Err(code) = resumed {
        found.lock().unwrap().generation -= 1;
        return Err(code);
    }
    let s = found.lock().unwrap();
    Ok(obj(json!({"session": s.id, "generation": s.generation})))
}

pub fn snapshot(shared: &Shared, session: &str, generation: Option<i64>) -> Reply {
    let found = find(shared, session).ok_or("unknown_session")?;
    let s = found.lock().unwrap();
    check_generation(&s, generation)?;
    Ok(s.snapshot())
}

/// An input is dispatched at most once per request id: a repeat returns its state.
pub fn input(shared: &Shared, session: &str, generation: i64, request_id: &str, text: &str) -> Reply {
    let found = find(shared, session).ok_or("unknown_session")?;
    let mut s = found.lock().unwrap();
    check_generation(&s, Some(generation))?;
    if let Some(state) = s.input_state(request_id) {
        return Ok(obj(json!({"session": s.id, "request_id": request_id, "state": state.name(), "duplicate": true})));
    }
    if s.state == State::Disconnected {
        return Err("session_disconnected");
    }
    if s.state == State::Exited {
        return Err("session_exited");
    }
    if s.codex.is_some() {
        crate::codex::send_input(&mut s, request_id, text)?;
        s.record_input(request_id);
    } else if s.claude.is_some() {
        crate::claude::send_input(&mut s, request_id, text)?;
        s.record_input(request_id);
    } else {
        if s.input_tx.is_none() {
            return Err("session_exited");
        }
        let line = format!("I {request_id} {}", payload::hex(text.as_bytes()));
        match s.input_tx.as_ref().map(|tx| tx.try_send(line)) {
            Some(Ok(())) => s.record_input(request_id),
            Some(Err(mpsc::TrySendError::Full(_))) => return Err("busy"),
            _ => return Err("session_exited"),
        }
    }
    let state = s.input_state(request_id).unwrap_or(InputState::Accepted);
    Ok(obj(json!({"session": s.id, "request_id": request_id, "state": state.name(), "duplicate": false})))
}

pub fn input_status(shared: &Shared, session: &str, request_id: &str) -> Reply {
    let found = find(shared, session).ok_or("unknown_session")?;
    let s = found.lock().unwrap();
    let state = s.input_state(request_id).ok_or("unknown_request")?;
    Ok(obj(json!({"session": s.id, "request_id": request_id, "state": state.name()})))
}

pub fn approval_answer(shared: &Shared, session: &str, generation: i64, approval_id: i64, answer: &str) -> Reply {
    let found = find(shared, session).ok_or("unknown_session")?;
    let mut s = found.lock().unwrap();
    check_generation(&s, Some(generation))?;
    let position = s.approvals.iter().position(|(n, _)| *n == approval_id).ok_or("unknown_approval")?;
    if s.codex.is_some() {
        crate::codex::answer(&mut s, approval_id, answer == "allow")?;
    } else if s.claude.is_some() {
        crate::claude::answer(&mut s, approval_id, answer == "allow")?;
    } else {
        match s.input_tx.as_ref().map(|tx| tx.try_send(format!("P {approval_id} {answer}"))) {
            Some(Ok(())) => {}
            Some(Err(mpsc::TrySendError::Full(_))) => return Err("busy"),
            _ => return Err("session_exited"),
        }
    }
    s.approvals.remove(position);
    Ok(obj(json!({"session": s.id, "approval_id": approval_id, "state": "answered"})))
}

/// Attach a view: the latest screen now and, for a connected view, live records after it.
/// For a connected view the reply is queued under the session lock, before the view can
/// receive a record, so a record never overtakes its own attach reply.
pub fn view_attach(shared: &Shared, conn: Option<(&Arc<Sub>, &Option<Value>)>, session: &str, generation: Option<i64>, after: i64) -> Reply {
    let found = find(shared, session).ok_or("unknown_session")?;
    if let Some((sub, _)) = conn {
        if shared.views.lock().unwrap().values().filter(|v| v.conn.as_ref().map_or(false, |c| Arc::ptr_eq(c, sub))).count() >= MAX_VIEWS_PER_CONNECTION {
            return Err("busy");
        }
    }
    let mut s = found.lock().unwrap();
    check_generation(&s, generation)?;
    let (oldest, last) = (s.oldest(), s.last_seq());
    let valid = oldest <= after + 1 && after <= last;
    let view = format!("w{}", random_hex(8));
    let mut body = obj(json!({"view": view, "session": s.id, "generation": s.generation, "gap": !valid, "from": last + 1}));
    body.insert("snapshot".into(), Value::Object(s.snapshot()));
    body.insert("missed".into(), if valid { json!(last - after) } else { Value::Null });
    {
        // a view that has no connection (the HTTP listener) expires if it is never detached
        let mut views = shared.views.lock().unwrap();
        views.retain(|_, v| v.conn.is_some() || v.created.elapsed() < Duration::from_secs(600));
    }
    shared.views.lock().unwrap().insert(view, ViewEntry { session: s.id.clone(), conn: conn.map(|(sub, _)| sub.clone()), created: Instant::now() });
    if let Some((sub, rid)) = conn {
        let mut all = obj(json!({"ok": 1}));
        if let Some(id) = rid {
            all.insert("id".into(), id.clone());
        }
        all.extend(body.clone());
        sub.push(crate::session::encode(all));
        sub.clear_overflow(&s.id);
        s.add_sub(sub);
    }
    Ok(body)
}

pub fn view_detach(shared: &Shared, view: &str) -> Reply {
    let entry = shared.views.lock().unwrap().remove(view).ok_or("unknown_view")?;
    let found = find(shared, &entry.session).ok_or("unknown_session")?;
    let mut s = found.lock().unwrap();
    if let Some(sub) = &entry.conn {
        s.subs.retain(|x| !Arc::ptr_eq(x, sub));
        sub.clear_overflow(&s.id);
    }
    Ok(obj(json!({"view": view, "session": s.id, "seq": s.last_seq()})))
}

/// Views of a vanished connection are removed; the sessions and their agents go on.
pub fn drop_connection(shared: &Shared, conn: &Arc<Sub>) {
    shared.views.lock().unwrap().retain(|_, v| v.conn.as_ref().map_or(true, |c| !Arc::ptr_eq(c, conn)));
    for session in shared.sessions.read().unwrap().iter() {
        session.lock().unwrap().subs.retain(|s| !Arc::ptr_eq(s, conn));
    }
}

fn proc_kb(path: &str, key: &str) -> Option<i64> {
    std::fs::read_to_string(path).ok()?.lines().find(|l| l.starts_with(key))?.split_whitespace().nth(1)?.parse().ok()
}

pub fn resources(shared: &Shared) -> Reply {
    let list: Vec<Arc<Mutex<Session>>> = shared.sessions.read().unwrap().clone();
    let (mut running, mut subscribers, mut log_bytes) = (0, 0, 0usize);
    let mut agents = Vec::new();
    for s in &list {
        let s = s.lock().unwrap();
        if s.state == State::Running {
            running += 1;
            if let Some(rss) = proc_kb(&format!("/proc/{}/status", s.agent_pid), "VmRSS:") {
                agents.push(json!({"session": s.id, "pid": s.agent_pid, "rss_kb": rss}));
            }
        }
        subscribers += s.subs.len();
        log_bytes += s.disk_bytes();
    }
    let views = shared.views.lock().unwrap().len();
    Ok(obj(json!({
        "pid": std::process::id(), "uptime_s": shared.started.elapsed().as_secs(),
        "rss_kb": proc_kb("/proc/self/smaps_rollup", "Rss:").unwrap_or(0),
        "pss_kb": proc_kb("/proc/self/smaps_rollup", "Pss:").unwrap_or(0),
        "threads": proc_kb("/proc/self/status", "Threads:").unwrap_or(1).max(1),
        "sessions": list.len(), "running_sessions": running, "subscribers": subscribers, "views": views,
        "log_bytes": log_bytes, "agents": agents,
    })))
}

/// Records after `after` for a caller that cannot hold a stream (the HTTP listener):
/// at most one bounded page, or a gap with a snapshot when the cursor is unusable.
pub fn events(shared: &Shared, session: &str, after: i64) -> Reply {
    let found = find(shared, session).ok_or("unknown_session")?;
    let s = found.lock().unwrap();
    let (oldest, last, mem_start, id, generation, text_mode) = (s.oldest(), s.last_seq(), s.mem_start(), s.id.clone(), s.generation, s.text_mode);
    let valid = oldest <= after + 1 && after <= last;
    let gap = |s: &Session| {
        Ok(obj(json!({"session": s.id, "generation": s.generation, "gap": true, "from": s.last_seq() + 1,
                      "snapshot": Value::Object(s.snapshot()), "events": []})))
    };
    if !valid {
        return gap(&s);
    }
    let records: Vec<(i64, Vec<u8>)> = if after + 1 >= mem_start {
        s.window_after(after).into_iter().take(MAX_EVENTS).collect()
    } else {
        let view = s.log_view();
        drop(s);
        match view.read(after + 1, mem_start, MAX_EVENTS) {
            Ok(records) if !records.is_empty() => records,
            _ => {
                // pruned or unreadable while reading: the cursor is no longer usable
                let s = found.lock().unwrap();
                return gap(&s);
            }
        }
    };
    let events: Vec<Value> = records
        .iter()
        .map(|(seq, data)| {
            let mut event = json!({"v": 1, "ev": "rec", "session": id, "generation": generation, "seq": seq, "data": payload::hex(data)});
            if text_mode {
                event["text"] = Value::String(String::from_utf8_lossy(data).into_owned());
            }
            event
        })
        .collect();
    Ok(obj(json!({"session": id, "generation": generation, "gap": false, "from": after + 1, "events": events})))
}

/// Send every record after `cursor`, then join the live stream with no gap: older
/// records come from the log in chunks (outside the session lock), the recent ones
/// from the in-memory window, and the join happens under the lock.
pub fn replay(session: &Arc<Mutex<Session>>, sub: &Arc<Sub>, id: &str, mut cursor: i64) {
    let (generation, text_mode) = {
        let s = session.lock().unwrap();
        (s.generation, s.text_mode)
    };
    loop {
        let (mem_start, view): (i64, LogView) = {
            let mut s = session.lock().unwrap();
            let mem_start = s.mem_start();
            if cursor + 1 >= mem_start {
                for (seq, data) in s.window_after(cursor) {
                    if sub.offer(id, rec_line(id, generation, seq, &data, text_mode)) {
                        return;
                    }
                }
                s.add_sub(sub); // live records follow with no gap
                return;
            }
            (mem_start, s.log_view())
        };
        if !sub.wait_room() {
            // a reader that does not drain a replay is told to resynchronise
            sub.push(crate::session::encode(obj(json!({"ev": "overflow", "session": id}))));
            return;
        }
        let chunk = match view.read(cursor + 1, mem_start, REPLAY_CHUNK) {
            Ok(chunk) if !chunk.is_empty() => chunk,
            _ => {
                // the records were pruned while replaying: tell the reader to resynchronise
                sub.push(crate::session::encode(obj(json!({"ev": "overflow", "session": id}))));
                return;
            }
        };
        for (seq, data) in chunk {
            if sub.offer(id, rec_line(id, generation, seq, &data, text_mode)) {
                return;
            }
            cursor = seq;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider() -> Child {
        let mut command = Command::new("sleep");
        command.arg("60");
        unsafe {
            command.pre_exec(|| {
                setsid(); // its own group, like a provider
                Ok(())
            });
        }
        let child = command.spawn().expect("sleep");
        note_provider(child.id());
        child
    }

    #[test]
    fn the_stop_returns_and_ends_the_providers_whatever_state_a_session_lock_is_in() {
        let dir = std::env::temp_dir().join(format!("lico_core_stop_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let session = |id: &str| Arc::new(Mutex::new(Session::new(id.to_string(), false, &dir, 100, 100)));
        let (held, poisoned, free) = (session("held"), session("poisoned"), session("free"));
        let p = poisoned.clone();
        let _ = thread::spawn(move || {
            let _guard = p.lock().unwrap();
            panic!("a session lock poisoned on purpose");
        })
        .join();
        assert!(poisoned.is_poisoned());
        let (h, held_now) = (held.clone(), mpsc::channel());
        thread::spawn(move || {
            let _guard = h.lock().unwrap(); // held for long, like the log's fsync under load
            held_now.0.send(()).unwrap();
            thread::sleep(Duration::from_secs(6));
        });
        held_now.1.recv().unwrap();
        let mut providers = [provider(), provider(), provider()]; // none is a session's own pid: only the list outside the locks can reach them
        let from = Instant::now();
        end_providers(&[held, poisoned, free], Duration::from_millis(500));
        assert!(from.elapsed() < Duration::from_secs(3), "the stop took {:?}", from.elapsed());
        for provider in providers.iter_mut() {
            let until = Instant::now() + Duration::from_secs(2);
            while provider.try_wait().unwrap().is_none() {
                assert!(Instant::now() < until, "a provider survived the stop");
                thread::sleep(Duration::from_millis(10));
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn within_returns_by_the_budget_whatever_the_work_does() {
        let from = Instant::now();
        assert!(within(Duration::from_secs(2), || {}), "a quick job finishes inside the budget");
        assert!(from.elapsed() < Duration::from_millis(500));
        let from = Instant::now();
        assert!(!within(Duration::from_millis(300), || thread::sleep(Duration::from_secs(3))), "a slow job is given up");
        assert!(from.elapsed() >= Duration::from_millis(300) && from.elapsed() < Duration::from_millis(1500), "it took {:?}", from.elapsed());
        let from = Instant::now();
        assert!(!within(Duration::from_secs(2), || panic!("the work panicked on purpose")), "a panic is not a finished job");
        assert!(from.elapsed() < Duration::from_millis(500), "a panic returns at once, it took {:?}", from.elapsed());
        assert!(within(Duration::from_secs(2), || {}), "the caller is not poisoned by it");
    }

    const CHILD: &str = "LICO_STOP_REPLY_CHILD";

    /// The stop ends the process it runs in, so this test runs itself again in a child process (the first half) and the child (the second half) asks
    /// host_stop for a stop while one session lock is held for 8 s, then prints how long the reply took.
    #[test]
    fn the_stop_reply_does_not_wait_for_a_session_lock_that_is_held() {
        if std::env::var(CHILD).is_err() {
            let from = Instant::now();
            let done = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "core::tests::the_stop_reply_does_not_wait_for_a_session_lock_that_is_held", "--nocapture", "--test-threads=1"])
                .env(CHILD, "1")
                .output()
                .unwrap();
            let text = String::from_utf8_lossy(&done.stdout).to_string();
            let reply = text.lines().find_map(|l| l.split("REPLY ms=").nth(1)).unwrap_or_else(|| panic!("the process ended without a reply: {text}"));
            assert!(reply.ends_with("ok=true"), "the stop was refused: {reply}");
            let reply: u64 = reply.split_whitespace().next().unwrap().parse().unwrap();
            assert!(reply < 2000, "the reply took {reply} ms");
            assert!(from.elapsed() < Duration::from_secs(7), "the process did not end by itself: {:?}", from.elapsed());
            return;
        }
        let dir = std::env::temp_dir().join(format!("lico_core_reply_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg = Config {
            login_bin: None, login_args: Vec::new(), login_timeout: 60, log_dir: dir.join("log"), agent: PathBuf::new(), retain: 100, window: 100,
            queue_bytes: 1 << 20, payload: 4096, max_sessions: 8, codex: PathBuf::new(), codex_args: Vec::new(), claude: PathBuf::new(),
            claude_args: Vec::new(), cred_bin: PathBuf::new(), cred_args: Vec::new(), cred_dir: dir.join("cred"), workspace_root: dir.join("work"),
            accounts_dir: dir.join("accounts"), registry_dir: dir.join("registry"), text: false, realistic_text: false,
        };
        let accounts = crate::accounts::Accounts::load(&cfg.accounts_dir).unwrap_or_else(|_| panic!("accounts"));
        let shared = Arc::new(Shared {
            cfg, sessions: RwLock::new(Vec::new()), views: Mutex::new(HashMap::new()), started: Instant::now(), pending_creates: AtomicUsize::new(0),
            accounts: Mutex::new(accounts), logins: Mutex::new(HashMap::new()),
        });
        let held = Arc::new(Mutex::new(Session::new("held".to_string(), false, &dir, 100, 100)));
        shared.sessions.write().unwrap().push(held.clone());
        let (locked, now) = mpsc::channel();
        thread::spawn(move || {
            let _guard = held.lock().unwrap(); // held for long, like the log's fsync under load
            locked.send(()).unwrap();
            thread::sleep(Duration::from_secs(8));
        });
        now.recv().unwrap();
        let from = Instant::now();
        let reply = host_stop(&shared, true, None);
        println!("REPLY ms={} ok={}", from.elapsed().as_millis(), reply.is_ok());
        thread::sleep(Duration::from_secs(20)); // the stop ends this process long before
    }
}
