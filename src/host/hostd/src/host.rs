//! The Lico session host (synthetic phase): owns agent processes, their output and
//! the durable log independently of any GUI, and serves protocol v1 on an
//! owner-only Unix socket, plus an optional loopback HTTP listener.

use crate::{accounts, core, http, ipc, registry, session};
use serde_json::json;
use std::collections::HashMap;
use std::io::Write;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use crate::core::{Config, Shared};

#[repr(C)]
struct Rlimit {
    cur: u64,
    max: u64,
}

extern "C" {
    fn umask(mask: u32) -> u32;
    fn getrlimit(resource: i32, limit: *mut Rlimit) -> i32;
    fn setrlimit(resource: i32, limit: *const Rlimit) -> i32;
    fn getuid() -> u32;
    fn flock(fd: i32, operation: i32) -> i32;
}

/// Raise the descriptor limit to its hard maximum: every session and connection holds a few.
fn raise_descriptor_limit() {
    let mut limit = Rlimit { cur: 0, max: 0 };
    unsafe {
        if getrlimit(7, &mut limit) == 0 && limit.cur < limit.max {
            limit.cur = limit.max;
            setrlimit(7, &limit);
        }
    }
}

fn refuse(message: &str) -> ! {
    eprintln!("lico-host: {message}");
    std::process::exit(2);
}

pub fn run(args: Vec<String>) {
    let arg = |name: &str, default: &str| args.windows(2).find(|w| w[0] == name).map_or(default.to_string(), |w| w[1].clone());
    let socket = arg("--socket", "");
    if socket.is_empty() {
        eprintln!("usage: lico-host --socket PATH [--log-dir DIR] [--agent PATH] [--sessions N] [--producing N] [--rate R] [--history N] [--retain N] [--window N] [--queue-bytes N] [--payload N] [--max-connections N] [--max-sessions N] [--codex PATH] [--codex-arg ARG]... [--text | --realistic-text] [--http-port P --http-token-file FILE]");
        std::process::exit(2);
    }
    let socket_dir = std::path::Path::new(&socket).parent().map(|p| p.to_path_buf()).unwrap_or_default();
    // every directory of the host defaults to a place under --state-dir, or else beside the socket
    let state_dir = arg("--state-dir", "");
    let base = if state_dir.is_empty() { socket_dir.clone() } else { PathBuf::from(&state_dir) };
    let log_dir = PathBuf::from(arg("--log-dir", base.join("lico-log").to_str().unwrap_or("lico-log")));
    let agent_default = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("lico-synth-agent"))).unwrap_or_default();
    let cfg = Config {
        log_dir: log_dir.clone(),
        login_bin: if args.iter().any(|a| a == "--login-bin") { Some(PathBuf::from(arg("--login-bin", ""))) } else { None },
        login_timeout: arg("--login-timeout", "300").parse().expect("login-timeout"),
        login_args: args.windows(2).filter(|w| w[0] == "--login-arg").map(|w| w[1].clone()).collect(),
        agent: PathBuf::from(arg("--agent", agent_default.to_str().unwrap_or(""))),
        retain: arg("--retain", "2048").parse().expect("retain"),
        window: arg("--window", "64").parse::<usize>().expect("window").max(session::SNAPSHOT_LINES),
        queue_bytes: arg("--queue-bytes", "1048576").parse().expect("queue-bytes"),
        payload: arg("--payload", "1024").parse().expect("payload"),
        max_sessions: arg("--max-sessions", "256").parse().expect("max-sessions"),
        codex: PathBuf::from(arg("--codex", "codex")),
        codex_args: {
            let given: Vec<String> = args.windows(2).filter(|w| w[0] == "--codex-arg").map(|w| w[1].clone()).collect();
            if given.is_empty() { vec!["app-server".to_string()] } else { given }
        },
        workspace_root: base.join("workspaces"),
        claude: PathBuf::from(arg("--claude", "claude")),
        claude_args: args.windows(2).filter(|w| w[0] == "--claude-arg").map(|w| w[1].clone()).collect(),
        cred_bin: if args.iter().any(|a| a == "--cred-bin") { PathBuf::from(arg("--cred-bin", "")) } else { std::env::current_exe().unwrap_or_default() },
        cred_args: if args.iter().any(|a| a == "--cred-bin") { Vec::new() } else { vec!["cred".to_string()] }, // this program is its own credential command
        cred_dir: PathBuf::from(arg("--cred-dir", base.join("credentials").to_str().unwrap_or("credentials"))),
        accounts_dir: PathBuf::from(arg("--accounts-dir", base.join("accounts").to_str().unwrap_or("accounts"))),
        registry_dir: PathBuf::from(arg("--registry-dir", base.join("registry").to_str().unwrap_or("registry"))),
        text: args.iter().any(|a| a == "--text" || a == "--realistic-text"),
        realistic_text: args.iter().any(|a| a == "--realistic-text"),
    };
    {
        use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
        // the socket lives in a directory only its owner can enter
        match std::fs::metadata(&socket_dir) {
            Ok(meta) if meta.is_dir() && meta.mode() & 0o077 == 0 && meta.uid() == unsafe { getuid() } => {}
            _ => refuse("the socket directory must exist, belong to this user and have mode 0700"),
        }
        // one host per socket: a live one is never replaced from behind (it would lose its socket and its segments). A host of any build answers on
        // the socket; two that start at the same moment are told apart by a lock that lives as long as the process.
        if std::os::unix::net::UnixStream::connect(&socket).is_ok() {
            refuse("a host is already running on this socket");
        }
        {
            use std::os::unix::fs::OpenOptionsExt;
            let lock = std::fs::OpenOptions::new().create(true).truncate(false).write(true).mode(0o600).open(format!("{socket}.lock")).unwrap_or_else(|_| refuse("cannot open the lock beside the socket"));
            if unsafe { flock(std::os::fd::AsRawFd::as_raw_fd(&lock), 2 | 4) } != 0 {
                refuse("a host is already starting on this socket");
            }
            std::mem::forget(lock); // closed (and so released) only when the process ends
        }
        // the segments of registered sessions are kept (they are recovered below); the segments of anything else
        // (synthetic sessions of an earlier run) are removed, and a directory that holds other files is never touched
        let keep = registry::registered_ids(&cfg.registry_dir).unwrap_or_else(|why| refuse(&why));
        if log_dir.exists() {
            let name_of = |e: &std::fs::DirEntry| e.file_name().to_string_lossy().to_string();
            let entries: Vec<std::fs::DirEntry> = std::fs::read_dir(&log_dir).map_or_else(|_| refuse("cannot read the log directory"), |it| it.flatten().collect());
            if !log_dir.is_dir() || entries.iter().any(|e| !name_of(e).ends_with(".seg") && !name_of(e).ends_with(".damaged")) {
                refuse("the log directory holds files that are not session segments; refusing to clear it");
            }
            for entry in &entries {
                let name = name_of(entry);
                let owner = name.split('.').next().unwrap_or("");
                if (name.ends_with(".seg") || name.ends_with(".damaged")) && !keep.contains(owner) {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&log_dir).expect("log dir");
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&cfg.registry_dir).expect("registry dir");
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&cfg.workspace_root).expect("workspace root");
        let _ = std::fs::set_permissions(&log_dir, std::fs::Permissions::from_mode(0o700));
    }
    raise_descriptor_limit();
    core::init_spawner();
    let sessions: usize = arg("--sessions", "1").parse().expect("sessions");
    let producing: usize = arg("--producing", "1").parse().expect("producing");
    let rate: u64 = arg("--rate", "100").parse::<f64>().expect("rate") as u64;
    let history: u64 = arg("--history", "5000").parse().expect("history");
    let size = cfg.payload;
    let accounts_dir = cfg.accounts_dir.clone();
    let shared = Arc::new(Shared { cfg, sessions: RwLock::new(Vec::new()), views: Mutex::new(HashMap::new()), started: Instant::now(), pending_creates: AtomicUsize::new(0), accounts: Mutex::new(accounts::Accounts::load(&accounts_dir).unwrap_or_else(|why| refuse(&why))), logins: Mutex::new(HashMap::new()) });
    registry::recover(&shared).unwrap_or_else(|why| refuse(&why)); // sessions of an earlier run come back as disconnected
    {
        // durability policy: the logs are synced to disk about once a second while records arrive, and the registry is
        // rewritten within a second of a change; a power cut can lose the last second, never leave a torn record read as data
        let shared = shared.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(1));
            let sessions: Vec<_> = shared.sessions.read().unwrap().clone();
            for session in sessions {
                session.lock().unwrap().sync_log();
            }
            if registry::take_dirty() {
                registry::save(&shared);
            }
        });
    }
    for index in 0..sessions {
        let live = index < producing;
        if core::start_session(&shared, live, if live { rate } else { 0 }, history, size, !live).is_err() {
            refuse("cannot start the first sessions");
        }
    }
    let ids: Vec<String> = shared.sessions.read().unwrap().iter().map(|s| s.lock().unwrap().id.clone()).collect();
    let _ = std::fs::remove_file(&socket);
    let previous = unsafe { umask(0o077) }; // never group- or world-accessible, even briefly
    let listener = UnixListener::bind(&socket).expect("bind");
    unsafe { umask(previous) };
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).expect("socket mode");
    }
    // the control credential for the loopback listener goes to a private file, never to a log, argv or the ready line
    let port: u16 = arg("--http-port", "0").parse().expect("http-port");
    if port != 0 {
        use std::os::unix::fs::OpenOptionsExt;
        let token_file = arg("--http-token-file", "");
        assert!(!token_file.is_empty(), "--http-token-file is required with --http-port");
        let token = core::random_hex(16);
        let _ = std::fs::remove_file(&token_file); // never reuse an existing file or follow a link planted there
        let mut file = std::fs::OpenOptions::new().create_new(true).write(true).mode(0o600).open(&token_file).expect("token file");
        file.write_all(token.as_bytes()).expect("token file");
        let shared = shared.clone();
        thread::spawn(move || http::serve(shared, port, token));
    }
    println!("{}", json!({"ready": 1, "pid": std::process::id(), "sessions": ids}));
    let _ = std::io::stdout().flush();
    let cap: usize = arg("--max-connections", "256").parse().expect("max-connections");
    let active = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else {
            thread::sleep(std::time::Duration::from_millis(50)); // out of descriptors or a failed accept: do not spin
            continue;
        };
        if active.load(Ordering::Relaxed) >= cap {
            // a bounded number of local connections: the rest are told the host is busy and closed
            let _ = stream.write_all(b"{\"v\":1,\"ok\":0,\"error\":\"busy\"}\n");
            continue;
        }
        active.fetch_add(1, Ordering::Relaxed);
        let (shared, active) = (shared.clone(), active.clone());
        thread::spawn(move || {
            ipc::connection(stream, shared);
            active.fetch_sub(1, Ordering::Relaxed);
        });
    }
}
