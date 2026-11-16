//! The system tests of the browser adapter (M17-P06): the real `lico` binary (`LICO_BIN`) as the host on a private home, state folder and socket, with the test double
//! `lico-fake-engine` (`LICO_FAKE_ENGINE`) as the engine, named in the owner's setting file `browser.json` of the private state folder. They are on the short named list
//! `tests/system_tests.toml` and are run alone by `cargo xtask system` (each is ignored in the ordinary unit run). No real engine, no real site, no real name lookup (the only
//! addresses are literals), nothing outside a private temporary folder.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const IGNORED: &str = "a system test of tests/system_tests.toml: run by `cargo xtask system` with LICO_BIN and LICO_FAKE_ENGINE set";

extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

pub(super) struct Rig {
    pub(super) dir: PathBuf,
    pub(super) socket: String,
    http_port: u16,
    token: String,
    pub(super) child: Child,
    _stdout: BufReader<std::process::ChildStdout>,
    /// The file the engine process runs (the fake's copy, or the real engine in its pinned place).
    pub(super) engine_exe: PathBuf,
}

impl Rig {
    fn start(name: &str) -> Rig {
        Rig::start_with(name, None)
    }

    /// `real`: a directory with the real engine's two files, put in the pinned place of the private home (no setting file: the hash is checked). None: the fake, by the setting.
    pub(super) fn start_with(name: &str, real: Option<&Path>) -> Rig {
        let binary = std::env::var("LICO_BIN").unwrap_or_else(|_| panic!("LICO_BIN must name the release binary (cargo xtask system sets it); a system test never skips"));
        let dir = std::env::temp_dir().join(format!("lico_browser_system_{name}_{}", crate::core::random_hex(6)));
        let state = dir.join("state");
        let engine = dir.join("engine");
        for d in [&dir, &state, &engine] {
            std::fs::create_dir_all(d).unwrap();
            std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let engine_exe = match real {
            None => {
                let fake = std::env::var("LICO_FAKE_ENGINE").unwrap_or_else(|_| panic!("LICO_FAKE_ENGINE must name the fake engine (cargo xtask system sets it); a system test never skips"));
                std::fs::copy(&fake, engine.join("obscura")).unwrap();
                std::fs::set_permissions(engine.join("obscura"), std::fs::Permissions::from_mode(0o700)).unwrap();
                std::fs::write(state.join("browser.json"), json!({"engine_dir": engine.to_str().unwrap()}).to_string()).unwrap();
                std::fs::set_permissions(state.join("browser.json"), std::fs::Permissions::from_mode(0o600)).unwrap();
                engine.join("obscura")
            }
            Some(from) => {
                let pinned = dir.join(".local/share/lico/browser/v0.2.6");
                let mut at = dir.clone();
                for part in pinned.strip_prefix(&dir).unwrap().components() {
                    at.push(part);
                    std::fs::create_dir_all(&at).unwrap();
                    std::fs::set_permissions(&at, std::fs::Permissions::from_mode(0o700)).unwrap();
                }
                for name in ["obscura", "obscura-worker"] {
                    std::fs::copy(from.join(name), pinned.join(name)).unwrap();
                    std::fs::set_permissions(pinned.join(name), std::fs::Permissions::from_mode(0o700)).unwrap();
                }
                pinned.join("obscura")
            }
        };
        let socket = dir.join("host.sock").to_string_lossy().into_owned();
        let token_file = dir.join("http.token");
        let mut child = Command::new(binary)
            .args(["host", "--socket", &socket, "--text", "--sessions", "0", "--producing", "0", "--http-port", "auto", "--http-token-file", token_file.to_str().unwrap()])
            .args(["--state-dir", state.to_str().unwrap(), "--log-dir", state.join("log").to_str().unwrap(), "--registry-dir", state.join("registry").to_str().unwrap(), "--accounts-dir", state.join("accounts").to_str().unwrap()])
            .env("HOME", &dir)
            .env("XDG_RUNTIME_DIR", &dir)
            .env_remove("XDG_DATA_HOME")
            .env_remove("DBUS_SESSION_BUS_ADDRESS")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(std::fs::File::create(dir.join("host.err")).unwrap())
            .spawn()
            .expect("the host starts");
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        stdout.read_line(&mut line).expect("the host says it is ready");
        let ready: Value = serde_json::from_str(line.trim()).expect("a ready line");
        let http_port = ready["http_port"].as_u64().expect("the HTTP door is open") as u16;
        let token = std::fs::read_to_string(&token_file).unwrap().trim().to_string();
        Rig { dir, socket, http_port, token, child, _stdout: stdout, engine_exe }
    }

    pub(super) fn call(&self, request: Value) -> Value {
        crate::cli::call_once(&self.socket, request)
    }

    /// `POST /v1` with the same body: the other door.
    fn http(&self, request: &Value) -> Value {
        let body = request.to_string();
        let mut s = std::net::TcpStream::connect(("127.0.0.1", self.http_port)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        write!(s, "POST /v1 HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", self.http_port, self.token, body.len()).unwrap();
        let mut text = String::new();
        let _ = s.read_to_string(&mut text);
        serde_json::from_str(text.split("\r\n\r\n").nth(1).unwrap_or("{}")).unwrap_or(Value::Null)
    }

    pub(super) fn tabs(&self) -> Vec<Value> {
        self.call(json!({"op": "browser.list"}))["tabs"].as_array().cloned().unwrap_or_default()
    }

    pub(super) fn tab(&self, id: &str) -> Value {
        self.tabs().into_iter().find(|t| t["tab"] == json!(id)).unwrap_or(Value::Null)
    }

    pub(super) fn wait<T>(&self, secs: u64, what: &str, mut f: impl FnMut() -> Option<T>) -> T {
        let end = Instant::now() + Duration::from_secs(secs);
        loop {
            if let Some(v) = f() {
                return v;
            }
            assert!(Instant::now() < end, "{what} did not happen in {secs} s\nthe host said: {}", std::fs::read_to_string(self.dir.join("host.err")).unwrap_or_default().chars().take(3000).collect::<String>());
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    pub(super) fn open(&self, url: &str) -> String {
        let reply = self.call(json!({"op": "browser.open", "url": url}));
        assert_eq!(reply["ok"], json!(1), "{reply}");
        reply["tab"].as_str().unwrap().to_string()
    }

    /// The processes that run the fake engine of this rig (by the file they run).
    pub(super) fn engines(&self) -> Vec<i32> {
        let want = self.engine_exe.clone();
        std::fs::read_dir("/proc")
            .unwrap()
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().to_str().and_then(|n| n.parse::<i32>().ok()))
            .filter(|pid| std::fs::read_link(format!("/proc/{pid}/exe")).map(|p| p == want || p.to_string_lossy().starts_with(&format!("{} ", want.to_string_lossy()))).unwrap_or(false))
            .collect()
    }

    pub(super) fn threads(&self) -> usize {
        std::fs::read_to_string(format!("/proc/{}/status", self.child.id())).unwrap().lines().find_map(|l| l.strip_prefix("Threads:")).and_then(|v| v.trim().parse().ok()).unwrap()
    }

    pub(super) fn scratch_dirs(&self) -> usize {
        std::fs::read_dir(self.dir.join("state/browser")).map(|r| r.filter_map(|e| e.ok()).filter(|e| e.file_name().to_string_lossy().starts_with("scratch-")).count()).unwrap_or(0)
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        for pid in self.engines() {
            unsafe {
                kill(pid, 9);
            }
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A server on loopback that counts the requests it gets (the only origin of these tests; no real site).
fn fixture() -> (u16, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let h2 = hits.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let hits = h2.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 2048];
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                if stream.read(&mut buf).unwrap_or(0) > 0 {
                    hits.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
                }
            });
        }
    });
    (port, hits)
}

fn strip(mut v: Value) -> Value {
    if let Some(o) = v.as_object_mut() {
        o.remove("v");
        o.remove("id");
    }
    v
}

#[test]
#[ignore = "a system test of tests/system_tests.toml: run by `cargo xtask system` with LICO_BIN and LICO_FAKE_ENGINE set"]
fn browser_tab_opens_and_paints_a_fake_frame() {
    let _ = IGNORED;
    let rig = Rig::start("paint");
    let first = rig.open("https://pages.test/start");
    let tab = rig.wait(15, "the first page", || Some(rig.tab(&first)).filter(|t| t["has_frame"] == json!(true) && t["title"] == json!("A fake page") && t["phase"] == json!("ready")));
    assert_eq!((tab["url"].as_str(), tab["unpinned"].clone(), tab["local"].clone()), (Some("https://pages.test/start"), json!(true), json!(false)), "{tab}");
    assert_eq!(tab["viewport"], json!([1024, 768]));
    // a picture of the page: the frame of the fake, written again by lico-png to a new owner-only file
    let shot = rig.dir.join("shot.png");
    let reply = rig.call(json!({"op": "browser.shot", "tab": first, "path": shot.to_str().unwrap()}));
    assert_eq!((reply["ok"].clone(), reply["width"].clone(), reply["height"].clone()), (json!(1), json!(96), json!(64)), "{reply}");
    assert_eq!(std::fs::metadata(&shot).unwrap().permissions().mode() & 0o777, 0o600);
    let bytes = std::fs::read(&shot).unwrap();
    let image = lico_frame::decode(&bytes, &lico_frame::Limits::default()).expect("the picture is a PNG that decodes");
    let p = image.pixels[0];
    let (r, g, b) = ((p >> 16) as u8, (p >> 8) as u8, p as u8);
    assert!(b == 90 && r as u16 + g as u16 == 200 && r % 40 == 0, "the pixels are the fake's: {r} {g} {b}");
    assert_eq!(rig.call(json!({"op": "browser.shot", "tab": first, "path": shot.to_str().unwrap()}))["error"], json!("exists"), "never over an existing file");
    // what the guard saw reaches the person: an address that is not public is refused by the guard, and the tab says so (no network, no lookup: a literal address)
    let second = rig.open("http://203.0.113.5/");
    let tab = rig.wait(15, "the refusal notice", || Some(rig.tab(&second)).filter(|t| t["notice"].is_string()));
    let notice = tab["notice"].as_str().unwrap();
    assert!(notice.contains("203.0.113.5") && notice.contains("private network"), "{notice}");
    assert!(rig.tab(&first)["notice"].is_null(), "a tab that is not loading is not told");
    let engines = rig.call(json!({"op": "browser.list"}))["engines"].clone();
    assert!(engines[0]["guard_refused"].as_u64().unwrap() >= 1 && engines[0]["guard_accepted"].as_u64().unwrap() >= 1, "the list says how many connections the guard served and refused: {engines}");
    // the other door answers the same: the list and an error, over the socket and over POST /v1
    for request in [json!({"op": "browser.list"}), json!({"op": "browser.close", "tab": "b99"}), json!({"op": "browser.open", "url": "ftp://x.test/"}), json!({"op": "browser.shot", "tab": "b1", "path": "relative.png"})] {
        let (socket, http) = (strip(rig.call(request.clone())), strip(rig.http(&request)));
        assert_eq!(socket, http, "the same answer from both doors for {request}");
    }
    // the command line is a door of its own: the real `lico browser list` prints what the socket gives
    let cli = Command::new(std::env::var("LICO_BIN").unwrap()).args(["browser", "list", "--socket", &rig.socket]).output().unwrap();
    let printed: Value = serde_json::from_slice(&cli.stdout).unwrap_or(Value::Null);
    assert!(cli.status.success() && printed["tabs"].as_array().is_some_and(|t| t.len() == 2 && t.iter().all(|x| x["local"] == json!(false))), "{}", String::from_utf8_lossy(&cli.stdout));
    let cli = Command::new(std::env::var("LICO_BIN").unwrap()).args(["browser", "close", "b99", "--socket", &rig.socket]).output().unwrap();
    assert!(!cli.status.success(), "an unknown tab is a failure of the command");
    // the tab for this machine's own services (an explicit act of the person) and the ones that are not: what each may reach (the fixture counts what really arrives)
    let (fixture_port, hits) = fixture();
    let ordinary = rig.open(&format!("http://localhost:{fixture_port}/"));
    let t = rig.wait(15, "the refusal of localhost in an ordinary tab", || Some(rig.tab(&ordinary)).filter(|t| t["notice"].is_string()));
    assert!(t["notice"].as_str().unwrap().contains("private network"), "{t}");
    assert_eq!((hits.load(std::sync::atomic::Ordering::SeqCst), t["local"].clone()), (0, json!(false)), "an ordinary tab to localhost reaches nothing");
    let open_local = |url: &str| {
        let reply = rig.call(json!({"op": "browser.open", "url": url, "local": true}));
        assert_eq!((reply["ok"].clone(), reply["local"].clone()), (json!(1), json!(true)), "{reply}");
        reply["tab"].as_str().unwrap().to_string()
    };
    let local = open_local(&format!("http://127.0.0.1:{fixture_port}/"));
    let t = rig.wait(15, "the loopback page in the local tab", || Some(rig.tab(&local)).filter(|t| t["has_frame"] == json!(true) && t["phase"] == json!("ready")));
    assert!(t["notice"].is_null() && t["local"] == json!(true), "{t}");
    assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 1, "the loopback fixture loads in the local tab, through the guard");
    for (what, url, said) in [("a LAN-range address", "http://192.168.1.5/".to_string(), "only this machine's own services"), ("this host's own HTTP door", format!("http://127.0.0.1:{}/", rig.http_port), "belongs to Lico")] {
        let id = open_local(&url);
        let t = rig.wait(15, what, || Some(rig.tab(&id)).filter(|t| t["notice"].is_string()));
        assert!(t["notice"].as_str().unwrap().contains(said), "{what}: {t}");
    }
    // a PUBLIC address on a port that is not one of the four is refused for its port before anything is connected (a literal: no lookup, no connection)
    let port_tab = rig.open("http://93.184.216.34:25/");
    let t = rig.wait(15, "the port refusal in an ordinary tab", || Some(rig.tab(&port_tab)).filter(|t| t["notice"].is_string()));
    assert!(t["notice"].as_str().unwrap().contains("the port 25 is not one of 80, 443, 8080 and 8443") && !t["notice"].as_str().unwrap().contains("private"), "{t}");
    assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 1, "nothing else reached the fixture");
    assert_eq!(rig.engines().len(), 2, "one engine for each network class");
    let list = rig.call(json!({"op": "browser.list"}));
    assert_eq!(list["engines"][1]["local"], json!(true));
    assert!(list["engines"][1]["guard_refused"].as_u64().unwrap() >= 2, "{list}");
    assert_eq!(rig.call(json!({"op": "browser.close", "tab": "b99"}))["error"], json!("unknown_tab"));
    assert_eq!(rig.call(json!({"op": "browser.open", "url": "ftp://x.test/"}))["error"], json!("bad_request"));
    let capabilities = rig.call(json!({"op": "capabilities"}));
    assert!(["browser.open", "browser.list", "browser.close", "browser.shot"].iter().all(|op| capabilities["ops"].to_string().contains(op)), "capabilities names the operations");
    // no secret in the host's own log
    let log = std::fs::read_to_string(rig.dir.join("host.err")).unwrap();
    for line in log.lines().filter(|l| l.starts_with("lico-browser:")) {
        assert!(!line.contains("lico:") && !line.contains("TOKEN") && !line.contains("pages.test/start") && !line.contains("127.0.0.1:") && !line.contains("@"), "{line}");
    }
    assert!(log.contains("lico-browser: engine started (not the pinned release)"), "{log}");
}

#[test]
#[ignore = "a system test of tests/system_tests.toml: run by `cargo xtask system` with LICO_BIN and LICO_FAKE_ENGINE set"]
fn browser_engine_ends_with_the_last_tab_and_never_starts_without_one() {
    let mut rig = Rig::start("lifecycle");
    // nothing without a tab: no engine process, no scratch directory, no thread of the browser
    std::thread::sleep(Duration::from_millis(300));
    let idle_threads = rig.threads();
    assert_eq!((rig.engines().len(), rig.scratch_dirs()), (0, 0));
    assert!(rig.call(json!({"op": "browser.list"}))["engines"].as_array().unwrap().iter().all(|e| e["running"] == json!(false)));
    // one engine serves every tab
    let (a, b) = (rig.open("https://one.test/"), rig.open("https://two.test/"));
    rig.wait(15, "both pages", || Some(()).filter(|_| rig.tab(&a)["has_frame"] == json!(true) && rig.tab(&b)["has_frame"] == json!(true)));
    assert_eq!((rig.engines().len(), rig.scratch_dirs()), (1, 1));
    assert!(rig.threads() > idle_threads, "an engine runs, so its threads exist");
    let closed = rig.call(json!({"op": "browser.close", "tab": a}));
    assert_eq!((closed["ok"].clone(), closed["engine_stopped"].clone(), rig.engines().len()), (json!(1), json!(false), 1), "a tab is left: the engine stays");
    let closed = rig.call(json!({"op": "browser.close", "tab": b}));
    assert_eq!(closed["engine_stopped"], json!(true), "{closed}");
    assert_eq!((rig.engines().len(), rig.scratch_dirs()), (0, 0), "the last tab ended the process and removed the scratch directory");
    rig.wait(5, "the host's threads to be back", || Some(()).filter(|_| rig.threads() <= idle_threads));
    // the engine dies by itself: its tabs say so, and the next action starts it again
    let c = rig.open("https://three.test/");
    rig.wait(15, "the page", || Some(()).filter(|_| rig.tab(&c)["has_frame"] == json!(true)));
    let pids = rig.engines();
    assert_eq!(pids.len(), 1);
    unsafe {
        kill(pids[0], 9);
    }
    let failed = rig.wait(10, "the tab to show the death", || Some(rig.tab(&c)).filter(|t| t["phase"] == json!("failed")));
    assert!(failed["reason"].as_str().unwrap().contains("engine"), "{failed}");
    assert_eq!(rig.scratch_dirs(), 0, "an engine that died is cleaned up too");
    let d = rig.open("https://four.test/");
    rig.wait(15, "the page after the restart", || Some(()).filter(|_| rig.tab(&d)["has_frame"] == json!(true)));
    assert_eq!(rig.engines().len(), 1);
    assert_ne!(rig.engines(), pids, "a new process");
    // the number of tabs is capped
    for i in 0..6 {
        rig.open(&format!("https://more{i}.test/"));
    }
    assert_eq!(rig.tabs().len(), 8);
    assert_eq!(rig.call(json!({"op": "browser.open", "url": "https://ninth.test/"}))["error"], json!("tab_limit"));
    // stopping the host ends the engine and cleans up
    let reply = rig.call(json!({"op": "host.stop", "force": true}));
    assert_eq!(reply["ok"], json!(1), "{reply}");
    let end = Instant::now() + Duration::from_secs(10);
    while rig.child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < end, "the host did not end");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!((rig.engines().len(), rig.scratch_dirs()), (0, 0), "the engine ended with the host and its scratch directory is gone");
    let _: &Path = &rig.dir;
}
