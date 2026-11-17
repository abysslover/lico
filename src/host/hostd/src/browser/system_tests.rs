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

    /// `real`: a directory with the real engine's one file (`moli`), put in the pinned place of the private home (no setting file: the hash is checked). None: the fake, by the setting.
    pub(super) fn start_with(name: &str, real: Option<&Path>) -> Rig {
        Rig::start_full(name, real, None)
    }

    /// `seam`: a file that names the release address and hashes the host takes in place of the pinned ones (the host binary of the system tests is built with the feature that reads it);
    /// no setting file and no engine are put in the private home: the host installs the engine itself, into the pinned place.
    pub(super) fn start_full(name: &str, real: Option<&Path>, seam: Option<&Path>) -> Rig {
        let binary = std::env::var("LICO_BIN").unwrap_or_else(|_| panic!("LICO_BIN must name the release binary (cargo xtask system sets it); a system test never skips"));
        let dir = std::env::temp_dir().join(format!("lico_browser_system_{name}_{}", crate::core::random_hex(6)));
        let state = dir.join("state");
        let engine = dir.join("engine");
        for d in [&dir, &state, &engine] {
            std::fs::create_dir_all(d).unwrap();
            std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let engine_exe = match real {
            None if seam.is_some() => dir.join(".local/share/lico/browser").join(lico_engine::dir_name()).join("moli"),
            None => {
                let fake = std::env::var("LICO_FAKE_ENGINE").unwrap_or_else(|_| panic!("LICO_FAKE_ENGINE must name the fake engine (cargo xtask system sets it); a system test never skips"));
                std::fs::copy(&fake, engine.join("moli")).unwrap();
                std::fs::set_permissions(engine.join("moli"), std::fs::Permissions::from_mode(0o700)).unwrap();
                std::fs::write(state.join("browser.json"), json!({"engine_dir": engine.to_str().unwrap()}).to_string()).unwrap();
                std::fs::set_permissions(state.join("browser.json"), std::fs::Permissions::from_mode(0o600)).unwrap();
                engine.join("moli")
            }
            Some(from) => {
                let pinned = dir.join(".local/share/lico/browser").join(lico_engine::dir_name());
                let mut at = dir.clone();
                for part in pinned.strip_prefix(&dir).unwrap().components() {
                    at.push(part);
                    std::fs::create_dir_all(&at).unwrap();
                    std::fs::set_permissions(&at, std::fs::Permissions::from_mode(0o700)).unwrap();
                }
                std::fs::copy(from.join("moli"), pinned.join("moli")).unwrap();
                std::fs::set_permissions(pinned.join("moli"), std::fs::Permissions::from_mode(0o700)).unwrap();
                pinned.join("moli")
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
            .envs(seam.map(|f| ("LICO_INSTALL_SEAM_FILE", f.to_path_buf())))
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

    /// Voluntary context switches summed over every thread of the host: each wake-up of a sleeping thread is one.
    pub(super) fn wakeups(&self) -> u64 {
        std::fs::read_dir(format!("/proc/{}/task", self.child.id()))
            .map(|r| r.filter_map(|e| e.ok()).map(|e| std::fs::read_to_string(e.path().join("status")).unwrap_or_default().lines().find_map(|l| l.strip_prefix("voluntary_ctxt_switches:")).and_then(|v| v.trim().parse::<u64>().ok()).unwrap_or(0)).sum())
            .unwrap_or(0)
    }

    /// The lines the fake engine wrote for what reached it (time in nanoseconds, method, parameters), from the scratch folder of its run.
    pub(super) fn input_log(&self) -> Vec<(u128, String, Value)> {
        let Ok(dirs) = std::fs::read_dir(self.dir.join("state/browser")) else { return Vec::new() };
        dirs.filter_map(|d| d.ok())
            .filter_map(|d| std::fs::read_to_string(d.path().join("input.log")).ok())
            .flat_map(|t| {
                t.lines()
                    .filter_map(|l| {
                        let mut p = l.splitn(3, ' ');
                        Some((p.next()?.parse::<u128>().ok()?, p.next()?.to_string(), serde_json::from_str(p.next()?).ok()?))
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// The files `name` that the fake engines wrote in their scratch folders, one string for each engine that wrote one.
    fn engine_files(&self, name: &str) -> Vec<String> {
        let Ok(dirs) = std::fs::read_dir(self.dir.join("state/browser")) else { return Vec::new() };
        dirs.filter_map(|d| d.ok()).filter_map(|d| std::fs::read_to_string(d.path().join(name)).ok()).collect()
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
    // the engine was asked for a JPEG screencast at quality 90 (M20-P02: Moli's PNG frames are 20 to 30 times larger)
    let asked = rig.engine_files("frame.log").concat();
    assert!(asked.contains(r#"Page.startScreencast {"everyNthFrame":1,"format":"jpeg","maxHeight":768,"maxWidth":1024,"quality":90}"#), "{asked}");
    // a picture of the page: the newest frame of the fake (a JPEG), decoded and written as a PNG by lico-png to a new owner-only file
    let shot = rig.dir.join("shot.png");
    let reply = rig.call(json!({"op": "browser.shot", "tab": first, "path": shot.to_str().unwrap()}));
    assert_eq!((reply["ok"].clone(), reply["width"].clone(), reply["height"].clone()), (json!(1), json!(96), json!(64)), "{reply}");
    assert_eq!(std::fs::metadata(&shot).unwrap().permissions().mode() & 0o777, 0o600);
    let bytes = std::fs::read(&shot).unwrap();
    assert_eq!(lico_frame::format(&bytes), Some(lico_frame::Format::Png), "the file is a PNG, whatever the engine sent");
    let image = lico_frame::decode(&bytes, &lico_frame::Limits::default()).expect("the picture is a PNG that decodes");
    let p = image.pixels[0];
    let (r, g, b) = ((p >> 16 & 255) as i32, (p >> 8 & 255) as i32, (p & 255) as i32);
    // the fake's colour is (shade, 200 - shade, 90) with a shade that is a multiple of 40; a JPEG gives it back to within the rounding of YCbCr
    assert!((b - 90).abs() <= 4 && (r + g - 200).abs() <= 6 && ((r + 20) % 40 - 20).abs() <= 4, "the pixels are the fake's: {r} {g} {b}");
    // (the shot is the newest frame the tab holds: `Page.captureScreenshot` is asked only when no frame came, and its parameters are tested in `map.rs`)
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
    // the new engine's events belong to the new tab, never to the failed tab of the engine that died: an engine that starts its session numbers again (Moli does, MEASURED in M20: the
    // first tab after a restart stayed "loading" for ever because the dead tab took its load event) gives the new tab the same session name as the dead one's
    let t = rig.wait(15, "the new tab to finish loading", || Some(rig.tab(&d)).filter(|t| t["phase"] == json!("ready")));
    assert_eq!((t["title"].clone(), t["url"].clone()), (json!("A fake page"), json!("https://four.test/")), "{t}");
    let dead = rig.tab(&c);
    assert_eq!((dead["phase"].clone(), dead["url"].clone()), (json!("failed"), json!("https://three.test/")), "the dead tab stays as it was: {dead}");
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

/// A watch the way a window sends it, on a connection of its own: the request is written and the answer is read later (or never).
fn send_watch(socket: &str, tab: &str, seq: u64, frame_seq: u64) -> std::os::unix::net::UnixStream {
    let mut c = std::os::unix::net::UnixStream::connect(socket).unwrap();
    writeln!(c, "{}", json!({"op": "browser.watch", "tab": tab, "seq": seq, "frame_seq": frame_seq, "frame": true})).unwrap();
    c
}

fn read_answer(mut c: std::os::unix::net::UnixStream, secs: u64) -> Option<Value> {
    c.set_read_timeout(Some(Duration::from_secs(secs))).unwrap();
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    while c.read(&mut byte).ok()? == 1 {
        if byte[0] == b'\n' {
            break;
        }
        line.push(byte[0]);
    }
    serde_json::from_slice(&line).ok()
}

#[test]
#[ignore = "a system test of tests/system_tests.toml: run by `cargo xtask system` with LICO_BIN and LICO_FAKE_ENGINE set"]
fn browser_watch_hands_the_window_the_newest_frame_and_nothing_polls() {
    let rig = Rig::start("watch");
    let tab = rig.open("https://anim.test/animate");
    // twenty frames came at once: the host acknowledged all, kept the newest compressed one and counted the rest
    let row = rig.wait(15, "the burst of frames", || Some(rig.tab(&tab)).filter(|t| t["frame_seq"].as_u64().unwrap_or(0) >= 20));
    assert!(row["dropped_frames"].as_u64().unwrap() >= 15, "the frames nobody painted are dropped in the host and counted: {row}");
    // a window that asks now gets the newest one only, as text, and it is a JPEG that decodes (the frame of the fake is what the engine was asked for)
    let first = read_answer(send_watch(&rig.socket, &tab, 0, 0), 10).expect("an answer");
    let newest = row["frame_seq"].as_u64().unwrap();
    assert_eq!(first["frame_seq"].as_u64(), Some(newest), "{}", first.to_string().chars().take(200).collect::<String>());
    let png = super::map::base64_decode(first["png"].as_str().unwrap()).expect("base64");
    assert_eq!(lico_frame::format(&png), Some(lico_frame::Format::Jpeg));
    let image = lico_frame::decode(&png, &lico_frame::Limits::default()).expect("a JPEG");
    assert_eq!((image.width, image.height), (96, 64));
    let seq = first["seq"].as_u64().unwrap();
    // asking again with what it has: the call waits (nothing changed), it does not poll and does not answer
    let waiting = send_watch(&rig.socket, &tab, seq, newest);
    let begun = Instant::now();
    waiting.set_read_timeout(Some(Duration::from_millis(1200))).unwrap();
    let mut probe = [0u8; 1];
    assert!(std::io::Read::read(&mut &waiting, &mut probe).is_err(), "no answer while nothing changes");
    assert!(begun.elapsed() >= Duration::from_millis(1000));
    // a newer watch of the same tab releases the older one
    let newer = send_watch(&rig.socket, &tab, seq, newest);
    let released = read_answer(waiting, 5).expect("the older watch is released");
    assert_eq!(released["superseded"], json!(true), "{released}");
    // a change wakes the newer one with the new frame only (a reload paints one frame)
    let reply = rig.call(json!({"op": "browser.nav", "tab": tab, "action": "reload"}));
    assert_eq!(reply["ok"], json!(1), "{reply}");
    // the first change is the reload itself (the tab is loading: a row, no frame); the window asks again and gets the frame
    let mut woken = read_answer(newer, 10).expect("woken by the change");
    for _ in 0..5 {
        if woken["png"].is_string() {
            break;
        }
        assert!(woken["seq"].as_u64().unwrap() > seq, "a change, not an echo: {woken}");
        woken = read_answer(send_watch(&rig.socket, &tab, woken["seq"].as_u64().unwrap(), newest), 10).expect("the next change");
    }
    assert!(woken["frame_seq"].as_u64().unwrap() > newest && woken["png"].is_string(), "{}", woken.to_string().chars().take(200).collect::<String>());
    // hiding stops the screencast and releases a watch
    let seq = woken["seq"].as_u64().unwrap();
    let held = send_watch(&rig.socket, &tab, seq, woken["frame_seq"].as_u64().unwrap());
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(rig.call(json!({"op": "browser.hide", "tab": tab}))["ok"], json!(1));
    assert_eq!(read_answer(held, 5).expect("released by the hide")["hidden"], json!(true));
    assert_eq!(rig.call(json!({"op": "browser.show", "tab": tab, "width": 640, "height": 480}))["viewport"], json!([640, 480]));
    // a window that vanishes and comes back leaves no thread behind
    let base = rig.threads();
    let mut last = {
        let r = rig.tab(&tab);
        (r["frame_seq"].as_u64().unwrap(), 0u64)
    };
    last.1 = read_answer(send_watch(&rig.socket, &tab, 0, 0), 10).unwrap()["seq"].as_u64().unwrap();
    for _ in 0..4 {
        drop(send_watch(&rig.socket, &tab, last.1, last.0)); // the window goes away while it waits
        std::thread::sleep(Duration::from_millis(100));
    }
    let comes_back = send_watch(&rig.socket, &tab, last.1, last.0); // it comes back: this supersedes the last one, the others ended at once
    std::thread::sleep(Duration::from_millis(200));
    assert!(rig.threads() <= base + 2, "threads: {} against {base}: parked watchers do not pile up", rig.threads());
    assert_eq!(rig.call(json!({"op": "browser.nav", "tab": tab, "action": "reload"}))["ok"], json!(1));
    assert!(read_answer(comes_back, 10).is_some());
    std::thread::sleep(Duration::from_millis(300));
    assert!(rig.threads() <= base + 1, "after the change every thread of a watch has ended: {} against {base}", rig.threads());
    // closing the tab releases a watch too
    let seq_now = rig.tab(&tab);
    let held = send_watch(&rig.socket, &tab, 1000, seq_now["frame_seq"].as_u64().unwrap());
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(rig.call(json!({"op": "browser.close", "tab": tab}))["ok"], json!(1));
    assert_eq!(read_answer(held, 5).expect("released by the close")["gone"], json!(true));
}

fn now_ns() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
}

#[test]
#[ignore = "a system test of tests/system_tests.toml: run by `cargo xtask system` with LICO_BIN and LICO_FAKE_ENGINE set"]
fn browser_input_reaches_the_engine_at_once_and_nothing_wakes_while_a_still_page_is_shown() {
    let rig = Rig::start("input");
    let tab = rig.open("https://pages.test/start");
    rig.wait(15, "the page", || Some(()).filter(|_| rig.tab(&tab)["has_frame"] == json!(true) && rig.tab(&tab)["phase"] == json!("ready")));
    // what the person does reaches the page: a press, a key, committed Korean text
    let input = |v: Value| {
        let mut v = v;
        v["op"] = json!("browser.input");
        v["tab"] = json!(tab);
        rig.call(v)
    };
    assert_eq!(input(json!({"kind": "down", "x": 30, "y": 40, "button": "left", "clicks": 1}))["sent"], json!(true));
    assert_eq!(input(json!({"kind": "up", "x": 30, "y": 40, "button": "left", "clicks": 1}))["sent"], json!(true));
    assert_eq!(input(json!({"kind": "wheel", "x": 30, "y": 40, "delta": -120.0}))["sent"], json!(true));
    assert_eq!(input(json!({"kind": "key_down", "key": "Enter", "code": "Enter"}))["sent"], json!(true));
    assert_eq!(input(json!({"kind": "text", "text": "안녕하세요"}))["sent"], json!(true));
    let log = rig.wait(5, "the input to reach the engine", || Some(rig.input_log()).filter(|l| l.len() >= 5));
    let methods: Vec<&str> = log.iter().map(|l| l.1.as_str()).collect();
    assert_eq!(methods, ["Input.dispatchMouseEvent", "Input.dispatchMouseEvent", "Input.dispatchMouseEvent", "Input.dispatchKeyEvent", "Input.insertText"], "in the order it was given");
    assert_eq!((log[0].2["type"].clone(), log[0].2["button"].clone(), log[2].2["deltaY"].clone(), log[3].2["key"].clone(), log[4].2["text"].clone()), (json!("mousePressed"), json!("left"), json!(-120.0), json!("Enter"), json!("안녕하세요")));
    // how long a command takes to arrive: sixty mouse moves, one after another, each stamped before it is sent and when the engine got it
    let mut sent = Vec::new();
    for i in 0..60 {
        sent.push(now_ns());
        assert_eq!(input(json!({"kind": "move", "x": 100 + i, "y": 50}))["sent"], json!(true));
        std::thread::sleep(Duration::from_millis(15)); // a mouse does not move faster than this in a test; and no two commands wait for each other
    }
    let log = rig.wait(5, "all the moves", || Some(rig.input_log()).filter(|l| l.iter().filter(|e| e.2["type"] == json!("mouseMoved")).count() >= 60));
    let arrived: Vec<u128> = log.iter().filter(|e| e.2["type"] == json!("mouseMoved")).map(|e| e.0).collect();
    let mut lag: Vec<f64> = sent.iter().zip(&arrived).map(|(s, a)| (*a as f64 - *s as f64) / 1e6).collect();
    lag.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let (p50, p95, worst) = (lag[lag.len() / 2], lag[lag.len() * 95 / 100], lag[lag.len() - 1]);
    println!("a command reaches the engine: p50 {p50:.2} ms, p95 {p95:.2} ms, worst {worst:.2} ms");
    assert!(p95 < 20.0, "p95 {p95:.2} ms (the gate is under 20): p50 {p50:.2}, worst {worst:.2}");
    // no input to a tab that is hidden or whose engine died
    assert_eq!(rig.call(json!({"op": "browser.hide", "tab": tab}))["ok"], json!(1));
    assert_eq!(input(json!({"kind": "text", "text": "a"}))["sent"], json!(false), "a hidden tab takes no input");
    assert_eq!(rig.call(json!({"op": "browser.show", "tab": tab, "width": 800, "height": 600}))["ok"], json!(1));
    rig.wait(5, "the screencast again", || Some(()).filter(|_| rig.tab(&tab)["shown"] == json!(true)));
    // nothing wakes while a still page is shown: the host's wake-ups over ten seconds with the page shown are those of the host with no tab, give or take a few
    std::thread::sleep(Duration::from_secs(1)); // let the show settle
    let (a, t0) = (rig.wakeups(), Instant::now());
    std::thread::sleep(Duration::from_secs(10));
    let shown = rig.wakeups() - a;
    assert_eq!(rig.call(json!({"op": "browser.close", "tab": tab}))["engine_stopped"], json!(true));
    std::thread::sleep(Duration::from_secs(1));
    let (b, t1) = (rig.wakeups(), Instant::now());
    std::thread::sleep(Duration::from_secs(10));
    let none = rig.wakeups() - b;
    println!("host wake-ups over {:.0} s: {shown} with a still page shown, {none} with no tab (the host's own background threads)", t0.elapsed().as_secs_f64().min(t1.elapsed().as_secs_f64() + 10.0));
    assert!(shown <= none + 4, "a still page shown woke the host {shown} times in ten seconds against {none} with no tab at all");
}

#[test]
#[ignore = "a system test of tests/system_tests.toml: run by `cargo xtask system` with LICO_BIN and LICO_FAKE_ENGINE set"]
fn browser_engine_installs_itself_when_a_tab_is_opened_and_runs() {
    let _ = IGNORED;
    use super::installer::tests::{serve, Reply};
    use std::sync::atomic::Ordering;
    // a release of the test in the shape of Moli's: the fake engine, a readme and a licence file inside the archive's own folder, made by tar, served by a local fake server that
    // answers when the test says so
    let work = std::env::temp_dir().join(format!("lico_browser_install_{}", crate::core::random_hex(6)));
    let src = work.join("src");
    let root = src.join(lico_engine::ARCHIVE_ROOT);
    std::fs::create_dir_all(root.join("licenses")).unwrap();
    std::fs::copy(std::env::var("LICO_FAKE_ENGINE").expect("LICO_FAKE_ENGINE"), root.join("moli")).unwrap();
    std::fs::write(root.join("README.md"), b"a readme\n").unwrap();
    std::fs::write(root.join("licenses/NOTICE.txt"), b"a notice\n").unwrap();
    let archive = work.join("a.tar.gz");
    assert!(Command::new("tar").arg("-czf").arg(&archive).arg("-C").arg(&src).arg(lico_engine::ARCHIVE_ROOT).status().unwrap().success());
    let bytes = std::fs::read(&archive).unwrap();
    let hex = |p: &Path| super::sha256::hex_of(std::fs::File::open(p).unwrap()).unwrap();
    let (gate, held) = std::sync::mpsc::channel();
    let server = serve(Reply::Held(bytes.clone(), held));
    let seam = work.join("seam.json");
    std::fs::write(&seam, json!({"url": server.url, "archive_sha256": hex(&archive), "engine_sha256": hex(&root.join("moli")), "max_archive": 64 << 20}).to_string()).unwrap();
    let rig = Rig::start_full("install", None, Some(&seam));
    let binary = std::env::var("LICO_BIN").unwrap();
    // the command line: it says it is installing, waits, and ends when the tab has an engine
    let mut cli = Command::new(&binary).args(["browser", "open", "https://pages.test/start", "--socket", &rig.socket]).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let mut said = String::new();
    let mut cli_err = BufReader::new(cli.stderr.take().unwrap()); // kept open: the command writes its next line there when the install ends
    cli_err.read_line(&mut said).unwrap();
    assert!(said.contains("installing the browser engine: about") && said.contains("MB from 127.0.0.1:") && said.contains(&hex(&archive)[..12]), "{said}");
    let tab = rig.tab("b1");
    assert_eq!(tab["phase"], json!("installing"), "{tab}");
    assert!(rig.engines().is_empty() && !pinned_dir(&rig).join("moli").exists(), "no engine and nothing installed while the download has not come");
    gate.send(()).unwrap();
    let mut out = String::new();
    cli.stdout.take().unwrap().read_to_string(&mut out).unwrap();
    let mut rest = String::new();
    cli_err.read_to_string(&mut rest).unwrap();
    assert!(cli.wait().unwrap().success(), "{rest}");
    assert!(rest.contains("the engine is installed and the tab is starting"), "{rest}");
    let printed: Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(printed["phase"], json!("starting"), "the command ends when the engine has been installed and the tab is being started: {out}");
    let tab = rig.wait(20, "the page of the installed engine", || Some(rig.tab("b1")).filter(|t| t["has_frame"] == json!(true) && t["title"] == json!("A fake page") && t["phase"] == json!("ready")));
    assert_eq!(tab["unpinned"], json!(false), "the engine in the pinned place is the one whose hash was checked: {tab}");
    assert_eq!(server.hits.load(Ordering::SeqCst), 1, "one download");
    let dir = pinned_dir(&rig);
    assert_eq!(std::fs::metadata(dir.join("moli")).unwrap().permissions().mode() & 0o777, 0o700);
    let inside: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(inside, ["moli"], "only the one file of the archive is written, not the readme or the licences");
    let parent: Vec<_> = std::fs::read_dir(dir.parent().unwrap()).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(parent, [lico_engine::dir_name()], "no staging folder and no archive is left");
    // closing the tab ends the engine; a tab opened again uses the installed engine and asks the server for nothing
    assert_eq!(rig.call(json!({"op": "browser.close", "tab": "b1"}))["engine_stopped"], json!(true));
    let again = rig.open("https://pages.test/again");
    rig.wait(20, "the second tab", || Some(rig.tab(&again)).filter(|t| t["has_frame"] == json!(true) && t["phase"] == json!("ready")));
    assert_eq!(server.hits.load(Ordering::SeqCst), 1, "the second open made no request");
    assert_eq!(rig.engines().len(), 1);
    let _ = std::fs::remove_dir_all(&work);
}

#[test]
#[ignore = "a system test of tests/system_tests.toml: run by `cargo xtask system` with LICO_BIN and LICO_FAKE_ENGINE set"]
fn browser_engine_is_started_in_the_shape_of_moli_and_the_old_stealth_setting_is_ignored() {
    let _ = IGNORED;
    // the engine is started as Moli is (the fake exits when its TMPDIR is not writable or its FONTCONFIG_FILE is not readable, so a ready tab proves both); the page is told the
    // honest user-agent; the list names the engine and says that its socket has no password
    let rig = Rig::start("moli_shape");
    let id = rig.open("https://pages.test/start");
    let t = rig.wait(15, "the page of the engine", || Some(rig.tab(&id)).filter(|t| t["phase"] == json!("ready")));
    assert!(t.get("stealth").is_none(), "the tab no longer has a stealth state: {t}");
    let argv = rig.engine_files("argv.log");
    assert_eq!(argv.len(), 1, "{argv:?}");
    let line = argv[0].trim();
    assert!(line.starts_with("serve --host 127.0.0.1 --port "), "{line}");
    assert!(line.contains(" --layout --resource --screencast-interval-ms 100"), "{line}");
    assert!(line.ends_with(" --log-level warn"), "{line}");
    assert!(!line.contains("stealth") && !line.contains("--proxy") && !line.contains("--token"), "no argument of the old engine: {line}");
    let has_ca = super::profile::CA_BUNDLES.iter().any(|p| Path::new(p).is_file());
    assert_eq!(line.contains(" --ca-cert /"), has_ca, "the system's CA bundle is passed when the machine has one: {line}");
    let ua = rig.engine_files("ua.log");
    assert!(ua.len() == 1 && ua[0].contains("Lico/") && ua[0].contains("Moli/"), "the honest user-agent is sent: {ua:?}");
    let list = rig.call(json!({"op": "browser.list"}));
    assert_eq!(list["engine"], json!(lico_engine::label()), "{list}");
    assert_eq!((list["engines"][0]["socket_password"].clone(), list["engines"][0].get("stealth").cloned()), (json!(false), None), "{list}");
    // the engine for this machine's own services is started the same way, a second one
    let reply = rig.call(json!({"op": "browser.open", "url": "http://127.0.0.1:9/", "local": true}));
    assert_eq!(reply["ok"], json!(1), "{reply}");
    let local = reply["tab"].as_str().unwrap().to_string();
    rig.wait(15, "the page of the local engine", || Some(rig.tab(&local)).filter(|t| t["phase"] == json!("ready") || t["notice"].is_string()));
    let argv = rig.engine_files("argv.log");
    assert!(argv.len() == 2 && argv.iter().all(|a| a.starts_with("serve --host 127.0.0.1 --port ")), "{argv:?}");
    drop(rig);
    // a setting file of 0.6.2 that still says `"stealth": true` is read and ignored: the engine is the ordinary one, started the same way, and the page gets the honest user-agent
    let rig = Rig::start("stealth_ignored");
    let setting = rig.dir.join("state/browser.json");
    std::fs::write(&setting, json!({"engine_dir": rig.dir.join("engine").to_str().unwrap(), "stealth": true}).to_string()).unwrap();
    std::fs::set_permissions(&setting, std::fs::Permissions::from_mode(0o600)).unwrap();
    let id = rig.open("https://pages.test/start");
    rig.wait(15, "the page with an old setting", || Some(rig.tab(&id)).filter(|t| t["phase"] == json!("ready")));
    let argv = rig.engine_files("argv.log");
    assert!(argv.len() == 1 && !argv[0].contains("stealth"), "{argv:?}");
    assert_eq!(rig.engine_files("ua.log").len(), 1, "the user-agent is still set");
    drop(rig);
    // a setting file that anyone else can read is refused as a whole, as before
    let rig = Rig::start("setting_readable");
    let setting = rig.dir.join("state/browser.json");
    std::fs::write(&setting, json!({"engine_dir": rig.dir.join("engine").to_str().unwrap()}).to_string()).unwrap();
    std::fs::set_permissions(&setting, std::fs::Permissions::from_mode(0o644)).unwrap();
    let reply = rig.call(json!({"op": "browser.open", "url": "https://pages.test/start"}));
    assert_eq!(reply["error"], json!("engine_setting"), "a readable setting is refused as a whole: {reply}");
}

fn pinned_dir(rig: &Rig) -> PathBuf {
    rig.dir.join(".local/share/lico/browser").join(lico_engine::dir_name())
}
