//! The Lico session host (synthetic phase): owns agent processes, their output and
//! the durable log independently of any GUI, and serves protocol v1 on an
//! owner-only Unix socket, plus an optional loopback HTTP listener.

mod core;
mod http;
mod ipc;
mod payload;
mod session;

use serde_json::json;
use std::collections::HashMap;
use std::io::Write;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::Instant;

use core::{Config, Shared};

extern "C" {
    fn umask(mask: u32) -> u32;
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |name: &str, default: &str| args.windows(2).find(|w| w[0] == name).map_or(default.to_string(), |w| w[1].clone());
    let socket = arg("--socket", "");
    if socket.is_empty() {
        eprintln!("usage: lico-host --socket PATH [--log-dir DIR] [--agent PATH] [--sessions N] [--producing N] [--rate R] [--history N] [--retain N] [--window N] [--queue-bytes N] [--payload N] [--http-port P --http-token-file FILE]");
        std::process::exit(2);
    }
    let socket_dir = std::path::Path::new(&socket).parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let log_dir = PathBuf::from(arg("--log-dir", socket_dir.join("lico-log").to_str().unwrap_or("lico-log")));
    let agent_default = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("lico-synth-agent"))).unwrap_or_default();
    let cfg = Config {
        log_dir: log_dir.clone(),
        agent: PathBuf::from(arg("--agent", agent_default.to_str().unwrap_or(""))),
        retain: arg("--retain", "2048").parse().expect("retain"),
        window: arg("--window", "64").parse::<usize>().expect("window").max(session::SNAPSHOT_LINES),
        queue_bytes: arg("--queue-bytes", "1048576").parse().expect("queue-bytes"),
        payload: arg("--payload", "1024").parse().expect("payload"),
    };
    {
        use std::os::unix::fs::DirBuilderExt;
        let _ = std::fs::remove_dir_all(&log_dir); // a fresh log for each run of the synthetic host
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&log_dir).expect("log dir");
    }
    let sessions: usize = arg("--sessions", "1").parse().expect("sessions");
    let producing: usize = arg("--producing", "1").parse().expect("producing");
    let rate: u64 = arg("--rate", "100").parse::<f64>().expect("rate") as u64;
    let history: u64 = arg("--history", "5000").parse().expect("history");
    let size = cfg.payload;
    let shared = Arc::new(Shared { cfg, sessions: RwLock::new(Vec::new()), views: Mutex::new(HashMap::new()), started: Instant::now() });
    for index in 0..sessions {
        let live = index < producing;
        core::start_session(&shared, live, if live { rate } else { 0 }, history, size, !live);
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
        let mut file = std::fs::OpenOptions::new().create(true).truncate(true).write(true).mode(0o600).open(&token_file).expect("token file");
        file.write_all(token.as_bytes()).expect("token file");
        let shared = shared.clone();
        thread::spawn(move || http::serve(shared, port, token));
    }
    println!("{}", json!({"ready": 1, "pid": std::process::id(), "sessions": ids}));
    let _ = std::io::stdout().flush();
    for stream in listener.incoming().flatten() {
        let shared = shared.clone();
        thread::spawn(move || ipc::connection(stream, shared));
    }
}
