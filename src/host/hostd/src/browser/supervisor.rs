//! One engine and the one thread that runs it (M17-P06). The thread exists only while the engine does: it makes a scratch directory, starts the fetch guard, starts the engine
//! behind `lico-sandbox` from the open, hashed file (it is the spawning thread, so the engine's death-with-parent signal follows this thread, which lives as long as the engine),
//! owns the CDP client, feeds the tab models and executes their effects, and at the end stops the engine, the guard and removes the scratch directory.
//! While it runs it wakes ten times a second (the read timeout of the client): said plainly, measured, and the first thing the input phase removes.

use super::engine::Located;
use super::map;
use super::profile::{self, Params};
use lico_browser_view::{Effect, EngineEvent, Tab};
use lico_cdp::{Client, Error as CdpError, Limits};
use lico_proxy::{Config as ProxyConfig, EventKind, Policy, Proxy, Resolver, StdConnector, StdResolver};
use serde_json::{json, Value};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// How long the worker waits for the engine's events before it looks at its commands again.
pub const POLL: Duration = Duration::from_millis(100);
/// How many times the start is tried when the engine cannot bind the port that was picked (another program took it between the pick and the bind).
pub const BIND_TRIES: usize = 3;
/// How long one try waits for the engine's CDP port to accept a connection.
const CONNECT_WAIT: Duration = Duration::from_secs(3);
/// How long `start` waits for the whole start (all tries) before it gives up.
const READY_WAIT: Duration = Duration::from_secs(15);
/// After `Browser.close` the engine has this long to end by itself.
const GRACE: Duration = Duration::from_secs(1);
const CALL: Duration = Duration::from_secs(5);

extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

/// The host's own log line about the browser: never a token, a port, a credential, a path, a query, page content or a frame.
pub fn log(message: &str) {
    eprintln!("lico-browser: {message}");
}

pub struct Entry {
    pub id: String,
    pub tab: Tab,
    pub engine: usize,
    pub target: Option<String>,
    pub session: Option<String>,
    pub localhost: bool,
}

pub struct Hub {
    pub tabs: Mutex<Vec<Entry>>,
}

pub enum Cmd {
    Open(String),
    /// Close the page with this target id (the manager has already taken the tab out of the table).
    Close(String),
    Shutdown,
}

pub struct Engine {
    pub tx: Sender<Cmd>,
    join: Option<JoinHandle<()>>,
    pub dead: Arc<AtomicBool>,
    pub unpinned: bool,
    /// What the engine says its product is (`Browser.getVersion`), cleaned: information for the person, never used to decide anything.
    pub reported: Arc<Mutex<String>>,
}

impl Engine {
    /// Ends the engine (the thread does the work) and waits for the thread.
    pub fn end(mut self) {
        let _ = self.tx.send(Cmd::Shutdown);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }

    pub fn finished(&self) -> bool {
        self.dead.load(Ordering::SeqCst)
    }

    /// Waits for a thread that has already ended on its own.
    pub fn reap(mut self) {
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

pub struct StartError(pub &'static str);

pub struct Setup {
    pub index: usize,
    pub localhost: bool,
    pub located: Located,
    pub font_dir: Option<PathBuf>,
    pub state_dir: PathBuf,
    pub hub: Arc<Hub>,
    pub lico_version: String,
    pub release: String,
}

/// Names a resolver answers itself, so that the system is never asked for them: `localhost` is the loopback address (the guard then refuses it, or, for a tab that opted in,
/// allows it).
struct Named<R: Resolver>(R);

impl<R: Resolver> Resolver for Named<R> {
    fn resolve(&self, host: &str, timeout: Duration) -> std::io::Result<Vec<IpAddr>> {
        if host.eq_ignore_ascii_case("localhost") {
            return Ok(vec![IpAddr::V4(Ipv4Addr::LOCALHOST)]);
        }
        self.0.resolve(host, timeout)
    }
}

/// A port nobody listens on right now (the engine binds it a moment later; if someone else got there first the start is tried again).
fn free_port() -> std::io::Result<u16> {
    Ok(TcpListener::bind("127.0.0.1:0")?.local_addr()?.port())
}

fn scratch_dir(state_dir: &Path) -> std::io::Result<PathBuf> {
    use std::os::unix::fs::DirBuilderExt;
    let parent = state_dir.join("browser");
    let mut b = std::fs::DirBuilder::new();
    b.recursive(true).mode(0o700);
    b.create(&parent)?;
    // what an earlier host that was killed left behind: no engine of ours runs when the first one starts
    if let Ok(read) = std::fs::read_dir(&parent) {
        for e in read.filter_map(|e| e.ok()) {
            if e.file_name().to_string_lossy().starts_with("scratch-") {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
    let dir = parent.join(format!("scratch-{}", crate::core::random_hex(6)));
    let mut b = std::fs::DirBuilder::new();
    b.mode(0o700);
    b.create(&dir)?;
    Ok(dir)
}

/// Ends the engine and its group. The group is signalled only while the engine has not been reaped (its number is then still its own), so that a number that was given to
/// another process meanwhile is never signalled.
fn kill_group(child: &mut Child) {
    if matches!(child.try_wait(), Ok(None)) {
        unsafe {
            kill(-(child.id() as i32), 9);
        }
        let _ = child.kill();
    }
    let _ = child.wait();
}

fn wait_exit(child: &mut Child, within: Duration) -> Option<std::process::ExitStatus> {
    let end = Instant::now() + within;
    loop {
        match child.try_wait() {
            Ok(Some(s)) => return Some(s),
            Ok(None) if Instant::now() < end => thread::sleep(Duration::from_millis(20)),
            _ => return None,
        }
    }
}

/// Starts the engine and its thread; returns when the engine accepts the CDP connection (or why it could not).
pub fn start(setup: Setup) -> Result<Engine, StartError> {
    let (tx, rx) = mpsc::channel::<Cmd>();
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), &'static str>>();
    let dead = Arc::new(AtomicBool::new(false));
    let reported = Arc::new(Mutex::new(String::new()));
    let unpinned = setup.located.unpinned;
    let (d2, r2) = (dead.clone(), reported.clone());
    let join = thread::Builder::new()
        .name("lico-browser-engine".into())
        .spawn(move || {
            run(setup, rx, ready_tx, &r2);
            d2.store(true, Ordering::SeqCst);
        })
        .map_err(|_| StartError("engine_failed"))?;
    match ready_rx.recv_timeout(READY_WAIT) {
        Ok(Ok(())) => Ok(Engine { tx, join: Some(join), dead, unpinned, reported }),
        Ok(Err(why)) => {
            let _ = join.join();
            Err(StartError(why))
        }
        Err(_) => {
            let _ = tx.send(Cmd::Shutdown);
            let _ = join.join();
            Err(StartError("engine_failed"))
        }
    }
}

struct Running {
    client: Client,
    child: Child,
    guard: Option<Proxy>,
    scratch: PathBuf,
}

fn run(setup: Setup, rx: Receiver<Cmd>, ready: Sender<Result<(), &'static str>>, reported: &Arc<Mutex<String>>) {
    let (gtx, grx) = mpsc::channel::<(u16, String)>();
    let mut running = match bring_up(&setup, gtx) {
        Ok(r) => r,
        Err(why) => {
            let _ = ready.send(Err(why));
            return;
        }
    };
    log(if setup.located.unpinned { "engine started (not the pinned release)" } else { "engine started" });
    if let Ok(v) = running.client.call("Browser.getVersion", json!({}), None, CALL) {
        if let Some(p) = v.get("product").and_then(Value::as_str) {
            *reported.lock().unwrap() = lico_browser_view::clean(p, 100);
        }
    }
    let _ = ready.send(Ok(()));
    let crashed = serve(&setup, &mut running, &rx, &grx);
    let status = tear_down(running);
    match status {
        Some(code) => log(&format!("engine ended, exit code {code}")),
        None => log("engine ended"),
    }
    if crashed {
        let mut tabs = setup.hub.tabs.lock().unwrap();
        for e in tabs.iter_mut().filter(|e| e.engine == setup.index) {
            e.tab.engine_event(EngineEvent::Crashed);
        }
    }
}

fn bring_up(setup: &Setup, gtx: Sender<(u16, String)>) -> Result<Running, &'static str> {
    let scratch = scratch_dir(&setup.state_dir).map_err(|_| "engine_failed")?;
    let guard_token = crate::core::random_hex(16);
    let cdp_token = crate::core::random_hex(16);
    let observe: Arc<dyn Fn(&lico_proxy::Event) + Send + Sync> = Arc::new(move |e| {
        if let EventKind::Refused(status) = e.kind {
            let _ = gtx.send((status, e.host.clone()));
        }
    });
    let cfg = ProxyConfig { token: Some(guard_token.clone()), policy: Policy { allow_loopback: setup.localhost }, observe: Some(observe), ..ProxyConfig::default() };
    let guard = match Proxy::start(cfg, Arc::new(Named(StdResolver)), Arc::new(StdConnector)) {
        Ok(g) => g,
        Err(_) => {
            let _ = std::fs::remove_dir_all(&scratch);
            return Err("engine_failed");
        }
    };
    let guard_port = guard.addr().port();
    let proxy_url = format!("http://lico:{guard_token}@127.0.0.1:{guard_port}");
    let system = profile::existing_system_dirs();
    let mut last = "engine_failed";
    for attempt in 0..BIND_TRIES {
        if attempt > 0 {
            log("the engine could not bind its port, trying again");
        }
        let Ok(cdp_port) = free_port() else { break };
        let params = Params { engine_dir: &setup.located.dir, font_dir: setup.font_dir.as_deref(), scratch: &scratch, cdp_port, guard_port, cdp_token: &cdp_token, proxy_url: &proxy_url };
        let conf = profile::confinement(&params, &system);
        let mut cmd = match lico_sandbox::confine_fd(&setup.located.file, &conf) {
            Ok(c) => c,
            Err(_) => {
                last = "engine_fence";
                break;
            }
        };
        cmd.args(profile::args(&params)).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(_) => {
                last = "engine_failed";
                break;
            }
        };
        let addr = SocketAddr::from(([127, 0, 0, 1], cdp_port));
        let end = Instant::now() + CONNECT_WAIT;
        loop {
            match Client::connect(addr, "/devtools/browser", Some(&cdp_token), &Limits::default()) {
                Ok(client) => return Ok(Running { client, child, guard: Some(guard), scratch }),
                Err(_) if child.try_wait().ok().flatten().is_some() || Instant::now() >= end => break,
                Err(_) => thread::sleep(Duration::from_millis(50)),
            }
        }
        kill_group(&mut child);
    }
    guard.stop();
    let _ = std::fs::remove_dir_all(&scratch);
    Err(last)
}

/// The loop. True when the engine died on its own (the socket closed or the process ended), false when it was asked to end.
fn serve(setup: &Setup, r: &mut Running, rx: &Receiver<Cmd>, grx: &Receiver<(u16, String)>) -> bool {
    let began = Instant::now();
    loop {
        // commands first: a command waits at most one poll (see the top of this file)
        loop {
            match rx.try_recv() {
                Ok(Cmd::Open(id)) => open_tab(setup, &mut r.client, &id),
                Ok(Cmd::Close(target)) => close_target(&mut r.client, &target),
                Ok(Cmd::Shutdown) | Err(mpsc::TryRecvError::Disconnected) => return false,
                Err(mpsc::TryRecvError::Empty) => break,
            }
        }
        while let Ok((status, host)) = grx.try_recv() {
            refusal(setup, &mut r.client, status, &host);
        }
        match r.client.next_event(POLL) {
            Ok(Some(event)) => on_event(setup, &mut r.client, &event),
            Ok(None) => {}
            Err(_) => return true,
        }
        if r.child.try_wait().ok().flatten().is_some() {
            return true;
        }
        let now = began.elapsed().as_millis() as u64;
        let ids: Vec<String> = setup.hub.tabs.lock().unwrap().iter().filter(|e| e.engine == setup.index).map(|e| e.id.clone()).collect();
        for id in ids {
            let fx = {
                let mut tabs = setup.hub.tabs.lock().unwrap();
                tabs.iter_mut().find(|e| e.id == id).map(|e| e.tab.tick(now)).unwrap_or_default()
            };
            run_effects(setup, &mut r.client, &id, fx);
        }
    }
}

fn tear_down(mut r: Running) -> Option<i32> {
    let _ = r.client.call("Browser.close", json!({}), None, Duration::from_secs(1));
    let status = match wait_exit(&mut r.child, GRACE) {
        Some(s) => s,
        None => {
            kill_group(&mut r.child);
            r.child.wait().ok()?
        }
    };
    if let Some(g) = r.guard.take() {
        g.stop();
    }
    let _ = std::fs::remove_dir_all(&r.scratch);
    status.code()
}

fn with_entry<T>(setup: &Setup, id: &str, f: impl FnOnce(&mut Entry) -> T) -> Option<T> {
    setup.hub.tabs.lock().unwrap().iter_mut().find(|e| e.id == id && e.engine == setup.index).map(f)
}

fn open_tab(setup: &Setup, client: &mut Client, id: &str) {
    let made = client.call("Target.createTarget", json!({"url": "about:blank"}), None, CALL).ok().and_then(|v| v.get("targetId").and_then(Value::as_str).map(str::to_string));
    let session = made.as_ref().and_then(|t| client.call("Target.attachToTarget", json!({"targetId": t, "flatten": true}), None, CALL).ok()).and_then(|v| v.get("sessionId").and_then(Value::as_str).map(str::to_string));
    let (Some(target), Some(session)) = (made, session) else {
        with_entry(setup, id, |e| e.tab.engine_event(EngineEvent::TargetGone));
        return;
    };
    with_entry(setup, id, |e| {
        e.target = Some(target);
        e.session = Some(session.clone());
    });
    let ua = map::user_agent(&setup.lico_version, &setup.release);
    for (method, params) in [("Page.enable", json!({})), ("Runtime.enable", json!({})), ("Emulation.setUserAgentOverride", json!({"userAgent": ua}))] {
        let _ = client.call(method, params, Some(&session), CALL);
    }
    deliver(setup, client, id, EngineEvent::Started);
}

fn close_target(client: &mut Client, target: &str) {
    let _ = client.call("Target.closeTarget", json!({"targetId": target}), None, CALL);
}

fn refusal(setup: &Setup, client: &mut Client, status: u16, host: &str) {
    let targets = {
        let tabs = setup.hub.tabs.lock().unwrap();
        let views: Vec<map::TabView<'_>> = tabs.iter().filter(|e| e.engine == setup.index).map(|e| map::TabView { id: &e.id, loading: e.tab.loading(), url: e.tab.url() }).collect();
        map::refusal_targets(&views, host)
    };
    for id in targets {
        deliver(setup, client, &id, EngineEvent::FetchRefused { status, host: host.to_string() });
    }
}

fn on_event(setup: &Setup, client: &mut Client, event: &lico_cdp::Event) {
    let Some(session) = event.session_id.as_deref() else { return };
    let id = setup.hub.tabs.lock().unwrap().iter().find(|e| e.engine == setup.index && e.session.as_deref() == Some(session)).map(|e| e.id.clone());
    let Some(id) = id else { return };
    for ev in map::engine_events(event) {
        let finished = ev == EngineEvent::LoadFinished;
        let navigated = if let EngineEvent::Navigated(u) = &ev { Some(map::host_of(u)) } else { None };
        deliver(setup, client, &id, ev);
        if let Some(host) = navigated.filter(|h| !h.is_empty()) {
            log(&format!("page of {host}"));
        }
        if finished {
            ask_title(setup, client, &id, session);
        }
    }
}

/// The title of the page, asked once the page has loaded (text, cleaned and cut by the model; the engine may also tell it by itself).
fn ask_title(setup: &Setup, client: &mut Client, id: &str, session: &str) {
    let r = client.call("Runtime.evaluate", json!({"expression": "document.title", "returnByValue": true}), Some(session), CALL);
    if let Some(t) = r.ok().and_then(|v| v["result"].get("value").and_then(Value::as_str).map(str::to_string)) {
        deliver(setup, client, id, EngineEvent::Title(t));
    }
}

/// Gives an event to a tab's model and executes what it asks for.
fn deliver(setup: &Setup, client: &mut Client, id: &str, event: EngineEvent) {
    let fx = with_entry(setup, id, |e| e.tab.engine_event(event)).unwrap_or_default();
    run_effects(setup, client, id, fx);
}

fn run_effects(setup: &Setup, client: &mut Client, id: &str, fx: Vec<Effect>) {
    for effect in fx {
        let Some((session, viewport)) = with_entry(setup, id, |e| (e.session.clone(), e.tab.viewport())).and_then(|(s, v)| s.map(|s| (s, v))) else { return };
        let wire = map::wire_of(&effect, viewport);
        match client.call(wire.method, wire.params, Some(&session), wire.timeout) {
            Ok(reply) => {
                if matches!(effect, Effect::CaptureScreenshot) {
                    if let Some(png) = reply.get("data").and_then(Value::as_str).and_then(map::base64_decode) {
                        deliver(setup, client, id, EngineEvent::Screenshot(png));
                    }
                }
            }
            Err(CdpError::Remote { message, .. }) if matches!(effect, Effect::Navigate(_)) && map::is_numeric_local_refusal(&message) => {
                deliver(setup, client, id, EngineEvent::NumericLocalAddress);
                deliver(setup, client, id, EngineEvent::LoadFinished);
            }
            Err(CdpError::Remote { .. }) | Err(CdpError::Timeout) => log("a call of the engine failed"),
            Err(_) => return, // the connection is gone: the loop sees it on its next read
        }
    }
}
