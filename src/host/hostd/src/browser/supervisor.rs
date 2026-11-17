//! One engine and the one thread that runs it (M17-P06). The thread exists only while the engine does: it makes a scratch directory, starts the fetch guard, starts the engine
//! behind `lico-sandbox` from the open, hashed file (it is the spawning thread, so the engine's death-with-parent signal follows this thread, which lives as long as the engine),
//! owns the calling half of the CDP link, feeds the tab models and executes their effects, and at the end stops the engine, the guard and removes the scratch directory.
//! Two threads, no timer: a reader that blocks in the CDP socket and hands each message on, and the command thread that blocks on one queue (commands of the operations, the
//! engine's events, the guard's refusals) and writes to the engine at once through the calling half of the link. The only time it wakes without a message is the one half second
//! after a tab is shown, for the model's fallback screenshot.

use super::engine::Located;
use super::map;
use super::profile::{self, Params};
use lico_browser_view::{Effect, EngineEvent, Refusal, Tab};
use lico_cdp::{link, Caller, Error as CdpError, Limits, Reader};
use lico_proxy::{Config as ProxyConfig, EventKind, Policy, Proxy, Resolver, StdConnector, StdResolver, Why};
use serde_json::{json, Value};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// How long after a tab is shown the model may ask for one screenshot if no frame came (the only timer: one wake each time a tab is shown).
const FALLBACK: Duration = Duration::from_millis(550);
/// The idle limit of the engine's connection: a reader with no message waits through it (once an hour it wakes and goes on).
const QUIET: Duration = Duration::from_secs(3600);
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
    pub local: bool,
    /// Grows with every change the person can see (a frame, a phase, a title, an address, a notice): what `browser.watch` waits on.
    pub change: u64,
    /// The number of the newest watch of this tab: a newer watch supersedes the older one, so that at most one waits.
    pub watch_gen: u64,
}

impl Entry {
    pub fn new(id: String, tab: Tab, engine: usize, local: bool) -> Entry {
        Entry { id, tab, engine, target: None, session: None, local, change: 1, watch_gen: 0 }
    }
}

pub struct Hub {
    pub tabs: Mutex<Vec<Entry>>,
    /// Told after every change of a tab, a close, a hide and a stop: the watchers of the tabs wake and look.
    pub changed: std::sync::Condvar,
}

impl Hub {
    pub fn new() -> Hub {
        Hub { tabs: Mutex::new(Vec::new()), changed: std::sync::Condvar::new() }
    }

    /// Marks a change of `entry` (the caller holds the lock of the tabs) and wakes the watchers.
    pub fn touch(&self, entry: &mut Entry) {
        entry.change += 1;
        self.changed.notify_all();
    }
}

pub enum Cmd {
    Open(String),
    /// Effects the model asked for after an operation of the person (show, hide, resize, go, nav): the worker runs them for that tab.
    Effects(String, Vec<Effect>),
    /// Close the page with this target id (the manager has already taken the tab out of the table).
    Close(String),
    Shutdown,
    /// A message of the engine (the reader hands each one on).
    Event(lico_cdp::Event),
    /// The guard served or refused something (the counts are refreshed; a refusal is told to the tabs).
    Guard(Option<Refused>),
    /// The engine's socket ended.
    Died,
}

pub struct Engine {
    pub tx: Sender<Cmd>,
    join: Option<JoinHandle<()>>,
    pub dead: Arc<AtomicBool>,
    pub unpinned: bool,
    /// What the engine says its product is (`Browser.getVersion`), cleaned: information for the person, never used to decide anything.
    pub reported: Arc<Mutex<String>>,
    /// What the fetch guard has served: connections accepted and refused (counts only).
    pub guard: Arc<Mutex<(u64, u64)>>,
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

/// What the fetch guard told the worker: a refusal, with its destination and its reason.
pub struct Refused {
    pub status: u16,
    pub host: String,
    pub port: u16,
    pub why: Refusal,
}

fn why_of(w: Why) -> Refusal {
    match w {
        Why::NotPublic => Refusal::NotPublic,
        Why::LoopbackOnly => Refusal::LoopbackOnly,
        Why::Port => Refusal::Port,
        Why::Denied => Refusal::Denied,
        Why::Other | Why::None => Refusal::Other,
    }
}

pub struct StartError(pub &'static str);

pub struct Setup {
    /// The browser's clock: the model's times (when a tab was shown) count from it.
    pub epoch: Instant,
    pub index: usize,
    pub local: bool,
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
    // what an earlier host that was killed left behind: the folders of OTHER processes (the name holds the host's process number). The folder of an engine of this host that still
    // runs is not touched: the engine for this machine's own services starts while the ordinary one runs.
    let mine = format!("scratch-{}-", std::process::id());
    if let Ok(read) = std::fs::read_dir(&parent) {
        for e in read.filter_map(|e| e.ok()) {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with("scratch-") && !name.starts_with(&mine) {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }
    let dir = parent.join(format!("{mine}{}", crate::core::random_hex(6)));
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
    let guard_counts = Arc::new(Mutex::new((0u64, 0u64)));
    let unpinned = setup.located.unpinned;
    let (d2, r2, g2) = (dead.clone(), reported.clone(), guard_counts.clone());
    let tx2 = tx.clone();
    let join = thread::Builder::new()
        .name("lico-browser-engine".into())
        .spawn(move || {
            run(setup, rx, tx2, ready_tx, &r2, &g2);
            d2.store(true, Ordering::SeqCst);
        })
        .map_err(|_| StartError("engine_failed"))?;
    match ready_rx.recv_timeout(READY_WAIT) {
        Ok(Ok(())) => Ok(Engine { tx, join: Some(join), dead, unpinned, reported, guard: guard_counts }),
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
    caller: Caller,
    child: Child,
    guard: Option<Proxy>,
    scratch: PathBuf,
    /// The ports this engine reserved (its guard's and its CDP port): given back when it ends.
    ports: Vec<u16>,
}

fn release(ports: &[u16]) {
    for p in ports {
        super::reserved::remove(*p);
    }
}

fn run(setup: Setup, rx: Receiver<Cmd>, tx: Sender<Cmd>, ready: Sender<Result<(), &'static str>>, reported: &Arc<Mutex<String>>, counts: &Arc<Mutex<(u64, u64)>>) {
    let (mut running, reader) = match bring_up(&setup, tx.clone()) {
        Ok(r) => r,
        Err(why) => {
            let _ = ready.send(Err(why));
            return;
        }
    };
    log(if setup.located.unpinned { "engine started (not the pinned release)" } else { "engine started" });
    // the reader: it blocks in the socket and hands each message to the command thread; it makes no call
    let rtx = tx.clone();
    let reading = thread::Builder::new().name("lico-browser-reader".into()).spawn(move || {
        let mut reader = reader;
        loop {
            match reader.next_event(None) {
                Ok(Some(e)) => {
                    if rtx.send(Cmd::Event(e)).is_err() {
                        break;
                    }
                }
                Ok(None) => {}
                Err(_) => {
                    let _ = rtx.send(Cmd::Died);
                    break;
                }
            }
        }
    });
    if let Ok(v) = running.caller.call("Browser.getVersion", json!({}), None, CALL) {
        if let Some(p) = v.get("product").and_then(Value::as_str) {
            *reported.lock().unwrap() = lico_browser_view::clean(p, 100);
        }
    }
    let _ = ready.send(Ok(()));
    let crashed = serve(&setup, &running, &rx, counts);
    let status = tear_down(&mut running);
    running.caller.shutdown();
    if let Ok(h) = reading {
        let _ = h.join();
    }
    match status {
        Some(code) => log(&format!("engine ended, exit code {code}")),
        None => log("engine ended"),
    }
    {
        let mut tabs = setup.hub.tabs.lock().unwrap();
        for e in tabs.iter_mut().filter(|e| e.engine == setup.index) {
            // the page and the session names belong to the engine that ended: an engine that starts its session numbers again (Moli does, MEASURED in M20) would give a new tab the name
            // of this dead one's, and the new tab's events would go to the dead tab
            e.target = None;
            e.session = None;
            if crashed {
                e.tab.engine_event(EngineEvent::Crashed);
                setup.hub.touch(e);
            }
        }
    }
}

/// The path of the engine's browser socket, from its discovery page (`GET /json/version`): one plain request to the loopback port the engine was told to bind, a bounded answer, and the
/// pure part (`map::socket_path`) judges it. `None` while the engine is not up yet or says something else.
pub(super) fn discover_path(addr: SocketAddr) -> Option<String> {
    use std::io::{Read, Write};
    let mut s = std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(500)).ok()?;
    s.set_read_timeout(Some(Duration::from_millis(800))).ok()?;
    s.write_all(b"GET /json/version HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n").ok()?;
    let mut body = Vec::new();
    let _ = (&mut s).take(map::DISCOVERY_MAX as u64).read_to_end(&mut body);
    map::socket_path(&String::from_utf8_lossy(&body))
}

fn bring_up(setup: &Setup, gtx: Sender<Cmd>) -> Result<(Running, Reader), &'static str> {
    let scratch = scratch_dir(&setup.state_dir).map_err(|_| "engine_failed")?;
    // the engine's font library reads a file of the host's making (it has no option for a font directory, and the system's own configuration is outside the fence)
    if std::fs::write(profile::fontconfig_path(&scratch), profile::fontconfig_file(setup.font_dir.as_deref(), &profile::font_cache_dir(&scratch))).is_err() {
        let _ = std::fs::remove_dir_all(&scratch);
        return Err("engine_failed");
    }
    let ca_bundle = profile::find_ca_bundle(&|p| std::fs::metadata(p).is_ok_and(|m| m.is_file()));
    if ca_bundle.is_none() {
        log("no bundle of certificate authorities was found: secure sites cannot be checked");
    }
    let guard_token = crate::core::random_hex(16);
    let observe: Arc<dyn Fn(&lico_proxy::Event) + Send + Sync> = Arc::new(move |e| {
        let refused = if let EventKind::Refused(status) = e.kind { Some(Refused { status, host: e.host.clone(), port: e.port, why: why_of(e.why) }) } else { None };
        let _ = gtx.send(Cmd::Guard(refused));
    });
    let cfg = ProxyConfig { token: Some(guard_token.clone()), policy: if setup.local { Policy { allow_loopback: true, only_loopback: true } } else { Policy::default() }, observe: Some(observe), deny_port: Some(Arc::new(super::reserved::denied)), ..ProxyConfig::default() };
    let guard = match Proxy::start(cfg, Arc::new(Named(StdResolver)), Arc::new(StdConnector)) {
        Ok(g) => g,
        Err(_) => {
            let _ = std::fs::remove_dir_all(&scratch);
            return Err("engine_failed");
        }
    };
    let guard_port = guard.addr().port();
    super::reserved::add(guard_port);
    let mut mine = vec![guard_port];
    let proxy_url = format!("http://lico:{guard_token}@127.0.0.1:{guard_port}");
    let system = profile::existing_system_dirs();
    let mut last = "engine_failed";
    for attempt in 0..BIND_TRIES {
        if attempt > 0 {
            log("the engine could not bind its port, trying again");
        }
        let Ok(cdp_port) = free_port() else { break };
        super::reserved::add(cdp_port);
        mine.push(cdp_port);
        let params = Params { engine_dir: &setup.located.dir, font_dir: setup.font_dir.as_deref(), scratch: &scratch, cdp_port, guard_port, proxy_url: &proxy_url, ca_bundle: ca_bundle.as_deref() };
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
            // the socket's path is the engine's to give, per start: asked of the discovery page, never assumed (engine contract, point 2)
            let linked = discover_path(addr).ok_or(()).and_then(|path| {
                let limits = Limits { idle: QUIET, ..Limits::default() };
                link(addr, &path, None, &limits).map_err(|_| ())
            });
            match linked {
                Ok((reader, caller)) => return Ok((Running { caller, child, guard: Some(guard), scratch, ports: mine }, reader)),
                Err(()) if child.try_wait().ok().flatten().is_some() || Instant::now() >= end => break,
                Err(()) => thread::sleep(Duration::from_millis(50)),
            }
        }
        kill_group(&mut child);
    }
    guard.stop();
    release(&mine);
    let _ = std::fs::remove_dir_all(&scratch);
    Err(last)
}

/// The loop of the command thread: it blocks on the queue, with no timer except the half second after a tab is shown. True when the engine died on its own (its socket
/// ended), false when it was asked to end.
fn serve(setup: &Setup, r: &Running, rx: &Receiver<Cmd>, counts: &Arc<Mutex<(u64, u64)>>) -> bool {
    let caller = &r.caller;
    let mut due: Option<Instant> = None;
    loop {
        let cmd = match due {
            Some(at) => match rx.recv_timeout(at.saturating_duration_since(Instant::now())) {
                Ok(c) => Some(c),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => return false,
            },
            None => match rx.recv() {
                Ok(c) => Some(c),
                Err(_) => return false,
            },
        };
        match cmd {
            Some(Cmd::Open(id)) => {
                open_tab(setup, caller, &id);
                due = Some(Instant::now() + FALLBACK);
            }
            Some(Cmd::Effects(id, fx)) => {
                if fx.iter().any(|e| matches!(e, Effect::StartScreencast)) {
                    due = Some(Instant::now() + FALLBACK);
                }
                run_effects(setup, caller, &id, fx);
            }
            Some(Cmd::Close(target)) => close_target(caller, &target),
            Some(Cmd::Shutdown) => return false,
            Some(Cmd::Died) => return true,
            Some(Cmd::Guard(refused)) => {
                if let Some(refused) = refused {
                    refusal(setup, caller, refused);
                }
            }
            Some(Cmd::Event(event)) => on_event(setup, caller, &event),
            None => {
                // the half second came: a shown tab with no frame yet is asked for one screenshot by the model, once
                due = None;
                let now = setup.epoch.elapsed().as_millis() as u64;
                let ids: Vec<String> = setup.hub.tabs.lock().unwrap().iter().filter(|e| e.engine == setup.index).map(|e| e.id.clone()).collect();
                for id in ids {
                    let fx = {
                        let mut tabs = setup.hub.tabs.lock().unwrap();
                        tabs.iter_mut().find(|e| e.id == id).map(|e| e.tab.tick(now)).unwrap_or_default()
                    };
                    run_effects(setup, caller, &id, fx);
                }
            }
        }
        if let Some(g) = &r.guard {
            let st = g.stats();
            *counts.lock().unwrap() = (st.accepted, st.refused);
        }
    }
}

fn tear_down(r: &mut Running) -> Option<i32> {
    let _ = r.caller.call("Browser.close", json!({}), None, Duration::from_secs(1));
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
    release(&r.ports);
    let _ = std::fs::remove_dir_all(&r.scratch);
    status.code()
}

fn with_entry<T>(setup: &Setup, id: &str, f: impl FnOnce(&mut Entry) -> T) -> Option<T> {
    setup.hub.tabs.lock().unwrap().iter_mut().find(|e| e.id == id && e.engine == setup.index).map(f)
}

fn open_tab(setup: &Setup, caller: &Caller, id: &str) {
    let made = caller.call("Target.createTarget", json!({"url": "about:blank"}), None, CALL).ok().and_then(|v| v.get("targetId").and_then(Value::as_str).map(str::to_string));
    let session = made.as_ref().and_then(|t| caller.call("Target.attachToTarget", json!({"targetId": t, "flatten": true}), None, CALL).ok()).and_then(|v| v.get("sessionId").and_then(Value::as_str).map(str::to_string));
    let (Some(target), Some(session)) = (made, session) else {
        with_entry(setup, id, |e| {
            e.tab.engine_event(EngineEvent::TargetGone);
            setup.hub.touch(e);
        });
        return;
    };
    with_entry(setup, id, |e| {
        e.target = Some(target);
        e.session = Some(session.clone());
    });
    // the page is told the honest user-agent
    let calls = [("Page.enable", json!({})), ("Runtime.enable", json!({})), ("Emulation.setUserAgentOverride", json!({"userAgent": map::user_agent(&setup.lico_version, &setup.release)}))];
    for (method, params) in calls {
        let _ = caller.call(method, params, Some(&session), CALL);
    }
    deliver(setup, caller, id, EngineEvent::Started);
}

fn close_target(caller: &Caller, target: &str) {
    let _ = caller.call("Target.closeTarget", json!({"targetId": target}), None, CALL);
}

fn refusal(setup: &Setup, caller: &Caller, refused: Refused) {
    let Refused { status, host, port, why } = refused;
    let host = host.as_str();
    let targets = {
        let tabs = setup.hub.tabs.lock().unwrap();
        let views: Vec<map::TabView<'_>> = tabs.iter().filter(|e| e.engine == setup.index).map(|e| map::TabView { id: &e.id, loading: e.tab.loading(), url: e.tab.url() }).collect();
        map::refusal_targets(&views, host)
    };
    for id in targets {
        deliver(setup, caller, &id, EngineEvent::FetchRefused { status, host: host.to_string(), port, why });
    }
}

fn on_event(setup: &Setup, caller: &Caller, event: &lico_cdp::Event) {
    let Some(session) = event.session_id.as_deref() else { return };
    let id = setup.hub.tabs.lock().unwrap().iter().find(|e| e.engine == setup.index && e.session.as_deref() == Some(session)).map(|e| e.id.clone());
    let Some(id) = id else { return };
    for ev in map::engine_events(event) {
        let finished = ev == EngineEvent::LoadFinished;
        let navigated = if let EngineEvent::Navigated(u) = &ev { Some(map::host_of(u)) } else { None };
        deliver(setup, caller, &id, ev);
        if let Some(host) = navigated.filter(|h| !h.is_empty()) {
            log(&format!("page of {host}"));
        }
        if finished {
            ask_title(setup, caller, &id, session);
        }
    }
}

/// The title of the page, asked once the page has loaded (text, cleaned and cut by the model; the engine may also tell it by itself).
fn ask_title(setup: &Setup, caller: &Caller, id: &str, session: &str) {
    let r = caller.call("Runtime.evaluate", json!({"expression": "document.title", "returnByValue": true}), Some(session), CALL);
    if let Some(t) = r.ok().and_then(|v| v["result"].get("value").and_then(Value::as_str).map(str::to_string)) {
        deliver(setup, caller, id, EngineEvent::Title(t));
    }
}

/// Gives an event to a tab's model and executes what it asks for.
fn deliver(setup: &Setup, caller: &Caller, id: &str, event: EngineEvent) {
    let fx = with_entry(setup, id, |e| {
        let fx = e.tab.engine_event(event);
        setup.hub.touch(e);
        fx
    })
    .unwrap_or_default();
    run_effects(setup, caller, id, fx);
}

fn run_effects(setup: &Setup, caller: &Caller, id: &str, fx: Vec<Effect>) {
    for effect in fx {
        let Some((session, viewport)) = with_entry(setup, id, |e| (e.session.clone(), e.tab.viewport())).and_then(|(s, v)| s.map(|s| (s, v))) else { return };
        let wire = map::wire_of(&effect, viewport);
        // only a navigation (its refusal is told in the reply) and a screenshot (its data is the reply) wait for an answer; every other command is written at once and forgotten
        if !matches!(effect, Effect::Navigate(_) | Effect::CaptureScreenshot) {
            if caller.fire(wire.method, wire.params, Some(&session)).is_err() {
                return;
            }
            continue;
        }
        match caller.call(wire.method, wire.params, Some(&session), wire.timeout) {
            Ok(reply) => {
                if matches!(effect, Effect::CaptureScreenshot) {
                    if let Some(png) = reply.get("data").and_then(Value::as_str).and_then(map::base64_decode) {
                        deliver(setup, caller, id, EngineEvent::Screenshot(png));
                    }
                }
            }
            Err(CdpError::Remote { message, .. }) if matches!(effect, Effect::Navigate(_)) && map::is_local_refusal(&message) => {
                deliver(setup, caller, id, EngineEvent::LocalAddressRefused);
                deliver(setup, caller, id, EngineEvent::LoadFinished);
            }
            Err(CdpError::Remote { .. }) | Err(CdpError::Timeout) => log(&format!("a call of the engine failed ({})", wire.method)),
            Err(_) => return, // the connection is gone: the loop sees it on its next read
        }
    }
}
