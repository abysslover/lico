//! The operations of the browser (`browser.open`, `browser.list`, `browser.close`, `browser.shot`) and the manager behind them: the table of tabs, at most two engines (one for
//! ordinary tabs, one for tabs that opted into `local`), and nothing running when there is no tab. Registered through the driver port; the hubs never name this module.

use super::engine::{self, Problem, Where, ARCHIVE_SHA256, ARCHIVE_URL, ENGINE_SHA256, RELEASE};
use super::fonts;
use super::installer::{self, Source};
use super::supervisor::{self, Cmd, Engine, Entry, Hub, Setup};
use crate::driver::{Opaque, Ops};
use lico_browser_view::{EngineEvent, Phase, Tab};
use lico_engine_pack::{self as pack, Failure};
use serde_json::{json, Map, Value};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

/// The most tabs the browser holds (all engines together): each is a page in the engine, and on Lico's side at most one compressed frame (4 MiB) and, while it is painted, one
/// decoded picture (9.2 MB at the largest size).
pub const MAX_TABS: usize = 8;
/// The size a tab opened from the command line is shown at (no window yet, so the screencast has a size to run at).
pub const DEFAULT_AREA: (u32, u32) = (1024, 768);
/// How long `browser.shot` waits for a first frame.
pub const SHOT_WAIT: Duration = Duration::from_secs(3);

pub const NAMES: [&str; 11] = ["browser.open", "browser.list", "browser.close", "browser.shot", "browser.show", "browser.resize", "browser.hide", "browser.go", "browser.nav", "browser.watch", "browser.input"];

/// `op`, `v` and `id` are the fields every request may carry.
pub fn valid_common(key: &str, v: &Value) -> bool {
    match key {
        "op" => true,
        "v" => v.as_i64() == Some(1),
        _ => matches!(v, Value::String(s) if !s.is_empty() && s.len() <= 64),
    }
}

type Reply = Result<Map<String, Value>, &'static str>;

fn obj(v: Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap_or_default()
}

pub fn valid_tab_id(v: &Value) -> bool {
    matches!(v, Value::String(s) if s.len() >= 2 && s.len() <= 7 && s.starts_with('b') && s[1..].bytes().all(|b| b.is_ascii_digit()) && !s[1..].starts_with('0'))
}

/// An absolute path for a picture: no control character, no `..`, at most 1024 bytes.
pub fn valid_shot_path(v: &Value) -> bool {
    matches!(v, Value::String(s) if s.starts_with('/') && s.len() <= 1024 && !s.chars().any(char::is_control) && !s.split('/').any(|p| p == ".." || p == ".") && !s.ends_with('/'))
}

pub fn valid(op: &str, request: &Map<String, Value>) -> bool {
    if super::window_ops::WINDOW_NAMES.contains(&op) {
        return super::window_ops::valid(op, request);
    }
    let allowed: &[&str] = match op {
        "browser.open" => &["op", "v", "id", "url", "local"],
        "browser.list" => &["op", "v", "id"],
        "browser.close" => &["op", "v", "id", "tab"],
        "browser.shot" => &["op", "v", "id", "tab", "path"],
        _ => return false,
    };
    let required: &[&str] = match op {
        "browser.open" => &["url"],
        "browser.close" => &["tab"],
        "browser.shot" => &["tab", "path"],
        _ => &[],
    };
    request.iter().all(|(k, v)| {
        allowed.contains(&k.as_str())
            && match k.as_str() {
                "op" => true,
                "v" => v.as_i64() == Some(1),
                "id" => matches!(v, Value::String(s) if !s.is_empty() && s.len() <= 64),
                "url" => matches!(v, Value::String(s) if !s.is_empty() && s.len() <= lico_browser_view::MAX_URL),
                "local" => v.is_boolean(),
                "tab" => valid_tab_id(v),
                "path" => valid_shot_path(v),
                _ => false,
            }
    }) && required.iter().all(|k| request.contains_key(*k))
}

/// What `ensure_engine` found out for a new tab.
pub(super) struct Prepared {
    pub absent: Option<Value>,
    pub font_missing: bool,
    pub unpinned: bool,
    /// The engine is being installed (a tab opened with no engine there): the tab says so and is started when the install ends.
    pub installing: bool,
}

pub struct Browser {
    pub(super) hub: Arc<Hub>,
    pub(super) engines: Mutex<[Option<Engine>; 2]>,
    next: Mutex<u32>,
    /// The moment the browser began: the model's clock (milliseconds since it).
    pub(super) epoch: std::time::Instant,
    /// The state directory of the host (the scratch directories and the owner's setting live in it).
    state_dir: Mutex<Option<PathBuf>>,
    /// Where to look for the engine instead of the process's own home (a test gives its own; the product never does).
    at: Option<Where>,
    /// This browser, for the install thread that restarts the tabs that waited for it.
    me: Weak<Browser>,
    /// An install is running (the engines lock is held when it is read or written: two tabs opened together install once).
    installing: Mutex<bool>,
    /// What is fetched (the pinned release; only a test build can have another).
    pub(super) source: Source,
}

impl Browser {
    pub fn new() -> Arc<Browser> {
        Self::build(None, installer::source_of_process())
    }

    fn build(at: Option<Where>, source: Source) -> Arc<Browser> {
        Arc::new_cyclic(|me| Browser {
            hub: Arc::new(Hub::new()),
            engines: Mutex::new([None, None]),
            next: Mutex::new(0),
            epoch: std::time::Instant::now(),
            state_dir: Mutex::new(None),
            at,
            me: me.clone(),
            installing: Mutex::new(false),
            source,
        })
    }

    #[cfg(test)]
    pub(super) fn looking_in(at: Where) -> Arc<Browser> {
        Self::build(Some(at), Source::pinned())
    }

    #[cfg(test)]
    pub(super) fn installing_from(at: Where, source: Source) -> Arc<Browser> {
        Self::build(Some(at), source)
    }

    fn state_dir_of(&self, shared: &Opaque) -> Option<PathBuf> {
        let mut slot = self.state_dir.lock().unwrap();
        if slot.is_none() {
            let s = shared.clone().downcast::<crate::core::Shared>().ok()?;
            *slot = s.cfg.registry_dir.parent().map(PathBuf::from);
        }
        slot.clone()
    }

    fn absent_fields(dir: &std::path::Path) -> Value {
        json!({"release": RELEASE, "download": ARCHIVE_URL, "archive_sha256": ARCHIVE_SHA256, "engine_sha256": ENGINE_SHA256, "directory": dir.to_string_lossy(), "put": "extract the archive and put the files obscura and obscura-worker in the directory"})
    }

    /// Makes sure the engine of class `index` runs (starting it, or replacing one that ended by itself: the restart), and says what a new tab of it must know. An engine that
    /// is not installed is not an error: the tab is shown in the `absent` phase.
    pub(super) fn ensure_engine(&self, engines: &mut [Option<Engine>; 2], shared: &Opaque, index: usize, may_install: bool) -> Result<Prepared, &'static str> {
        let local = index == 1;
        let state_dir = self.state_dir_of(shared).ok_or("engine_failed")?;
        if engines[index].as_ref().is_some_and(Engine::finished) {
            if let Some(old) = engines[index].take() {
                old.reap();
            }
        }
        let mut prepared = Prepared { absent: None, font_missing: false, unpinned: false, installing: false };
        if engines[index].is_none() {
            let at = match &self.at {
                Some(at) => Where { state_dir: state_dir.clone(), ..at.clone() },
                None => Where::of_process(state_dir.clone()).ok_or("engine_failed")?,
            };
            match engine::locate(&at, &engine::Pins { engine: &self.source.engine_sha256, worker: &self.source.worker_sha256 }) {
                Ok(located) => {
                    let font = fonts::find_on_this_machine();
                    prepared.font_missing = font.is_none();
                    let release = if located.unpinned { "unpinned".to_string() } else { RELEASE.trim_start_matches('v').to_string() };
                    let setup = Setup { epoch: self.epoch, index, local, located, font_dir: font, state_dir, hub: self.hub.clone(), lico_version: env!("CARGO_PKG_VERSION").to_string(), release };
                    match supervisor::start(setup) {
                        Ok(e) => engines[index] = Some(e),
                        Err(e) => return Err(e.0),
                    }
                }
                Err(Problem::Absent { dir }) => {
                    if may_install && !engine::auto_install_off(&at.state_dir) {
                        if !self.begin_install(shared, at.clone()) {
                            return Err("engine_failed");
                        }
                        prepared.installing = true;
                    } else {
                        prepared.absent = Some(Self::absent_fields(&dir));
                    }
                }
                Err(Problem::Unsafe(_)) => return Err("engine_unsafe"),
                Err(Problem::Mismatch) => return Err("engine_not_pinned"),
                Err(Problem::Setting(_)) => return Err("engine_setting"),
                Err(Problem::Io(_)) => return Err("engine_unreadable"),
            }
        } else if self.font_known_missing() {
            prepared.font_missing = true;
        }
        prepared.unpinned = engines[index].as_ref().is_some_and(|e| e.unpinned);
        Ok(prepared)
    }

    /// A new model for a tab of class `index`, shown at `area`, with what the engine's state says about it (absent, unpinned, no font, local) and the address typed.
    pub(super) fn new_tab(prepared: &Prepared, source: &Source, local: bool, area: (u32, u32), url: &str) -> Tab {
        let mut tab = Tab::new();
        tab.show(area, 0);
        let _ = tab.submit_address(url); // first: an absent engine ends the tab's ability to take an address, but the address typed stays
        if prepared.absent.is_some() {
            tab.engine_event(EngineEvent::Missing { expected_sha256: ENGINE_SHA256.to_string(), release: RELEASE.to_string() });
        }
        if prepared.installing {
            tab.engine_event(Self::installing_event(source));
        }
        if prepared.unpinned {
            tab.engine_event(EngineEvent::UnpinnedEngine);
        }
        if prepared.font_missing {
            tab.engine_event(EngineEvent::NoFontFound);
        }
        if local {
            tab.engine_event(EngineEvent::LocalTab);
        }
        tab
    }

    fn open(&self, shared: &Opaque, request: &Map<String, Value>) -> Reply {
        let typed = request.get("url").and_then(Value::as_str).unwrap_or("");
        let url = lico_browser_view::parse_address(typed).map_err(|_| "bad_request")?;
        let local = request.get("local").and_then(Value::as_bool).unwrap_or(false);
        let index = local as usize;
        let mut engines = self.engines.lock().unwrap();
        if self.hub.tabs.lock().unwrap().len() >= MAX_TABS {
            return Err("tab_limit");
        }
        let id = {
            let mut n = self.next.lock().unwrap();
            *n += 1;
            format!("b{n}")
        };
        let prepared = self.ensure_engine(&mut engines, shared, index, true)?;
        let tab = Self::new_tab(&prepared, &self.source, local, DEFAULT_AREA, &url);
        self.hub.tabs.lock().unwrap().push(Entry::new(id.clone(), tab, index, local));
        let mut reply = obj(json!({"tab": id, "phase": Self::phase_word(&prepared), "local": local, "unpinned": prepared.unpinned}));
        if prepared.installing {
            reply.extend(obj(json!({"size_mb": pack::size_mb(), "source": installer::host_of(&self.source.url), "sha256_start": pack::hash12(&self.source.archive_sha256)})));
        }
        match prepared.absent {
            Some(fields) => reply.extend(obj(fields)),
            None if prepared.installing => {} // the install thread opens the page when it has the engine
            None => {
                if let Some(e) = &engines[index] {
                    let _ = e.tx.send(Cmd::Open(id));
                }
            }
        }
        Ok(reply)
    }

    pub(super) fn phase_word(prepared: &Prepared) -> &'static str {
        if prepared.absent.is_some() {
            "absent"
        } else if prepared.installing {
            "installing"
        } else {
            "starting"
        }
    }

    fn installing_event(source: &Source) -> EngineEvent {
        EngineEvent::Installing { size_mb: pack::size_mb(), source: installer::host_of(&source.url), hash12: pack::hash12(&source.archive_sha256).to_string() }
    }

    /// Starts the install on its own thread unless one runs (the caller holds the engines lock). False: the thread could not be made.
    fn begin_install(&self, shared: &Opaque, at: Where) -> bool {
        let mut running = self.installing.lock().unwrap();
        if *running {
            return true;
        }
        let Some(me) = self.me.upgrade() else { return false };
        let (shared, source) = (shared.clone(), self.source.clone());
        let spawned = std::thread::Builder::new().name("lico-install".into()).spawn(move || {
            let result = installer::install(&at, &source);
            me.finish_install(&shared, result);
        });
        *running = spawned.is_ok();
        spawned.is_ok()
    }

    /// The install ended: the tabs that waited are started, or told why not. Woken watchers see it at once (no timer).
    fn finish_install(&self, shared: &Opaque, result: Result<(), Failure>) {
        let mut engines = self.engines.lock().unwrap();
        *self.installing.lock().unwrap() = false;
        let waiting: Vec<(String, usize)> = self.hub.tabs.lock().unwrap().iter().filter(|e| matches!(e.tab.phase(), Phase::Installing { .. })).map(|e| (e.id.clone(), e.engine)).collect();
        for (id, index) in waiting {
            let failure = match &result {
                Err(f) => Some(f.clone()),
                Ok(()) => match self.ensure_engine(&mut engines, shared, index, false) {
                    Ok(p) if p.absent.is_none() => None,
                    Ok(_) => Some(Failure::Cannot("the engine is not there after the install")),
                    Err(code) => Some(Failure::Other(format!("the engine could not be started: {code}"))),
                },
            };
            let mut tabs = self.hub.tabs.lock().unwrap();
            let Some(entry) = tabs.iter_mut().find(|e| e.id == id) else { continue };
            match failure {
                None => {
                    entry.tab.engine_event(EngineEvent::Starting);
                    if let Some(e) = &engines[index] {
                        let _ = e.tx.send(Cmd::Open(id.clone()));
                    }
                }
                Some(f) => {
                    entry.tab.engine_event(EngineEvent::InstallFailed { why: f.text(), security: f.security(), expected_sha256: ENGINE_SHA256.to_string(), release: RELEASE.to_string() });
                }
            }
            self.hub.touch(entry);
        }
        self.hub.changed.notify_all();
    }

    fn font_known_missing(&self) -> bool {
        fonts::find_on_this_machine().is_none()
    }

    pub(super) fn row(entry: &Entry) -> Value {
        let (w, h) = entry.tab.viewport();
        let mut row = obj(json!({
            "tab": entry.id, "url": entry.tab.url(), "title": entry.tab.title(), "loading": entry.tab.loading(), "local": entry.local,
            "viewport": [w, h], "viewport_capped": entry.tab.viewport_capped(), "has_frame": entry.tab.kept_bytes() > 0, "dropped_frames": entry.tab.dropped_frames(),
            "no_font": entry.tab.no_font(), "unpinned": entry.tab.unpinned(), "frame_seq": entry.tab.frame_seq(), "shown": entry.tab.shown(),
        }));
        match entry.tab.phase() {
            Phase::Starting => row.insert("phase".into(), json!("starting")),
            Phase::Ready => row.insert("phase".into(), json!("ready")),
            Phase::Loading => row.insert("phase".into(), json!("loading")),
            Phase::Absent { expected_sha256, release } => {
                row.insert("expected_sha256".into(), json!(expected_sha256));
                row.insert("release".into(), json!(release));
                row.insert("download".into(), json!(ARCHIVE_URL));
                row.insert("phase".into(), json!("absent"))
            }
            Phase::Installing { size_mb, source, hash12 } => {
                row.insert("size_mb".into(), json!(size_mb));
                row.insert("source".into(), json!(source));
                row.insert("sha256_start".into(), json!(hash12));
                row.insert("phase".into(), json!("installing"))
            }
            Phase::Failed(why) => {
                row.insert("reason".into(), json!(why));
                row.insert("phase".into(), json!("failed"))
            }
        };
        if let Some(n) = entry.tab.notice() {
            row.insert("notice".into(), json!(n));
        }
        Value::Object(row)
    }

    fn list(&self) -> Reply {
        let engines = self.engines.lock().unwrap();
        let tabs: Vec<Value> = self.hub.tabs.lock().unwrap().iter().map(Self::row).collect();
        let status: Vec<Value> = engines
            .iter()
            .enumerate()
            .map(|(i, e)| json!({"local": i == 1, "running": e.as_ref().is_some_and(|e| !e.finished()), "unpinned": e.as_ref().is_some_and(|e| e.unpinned), "guard_accepted": e.as_ref().map(|e| e.guard.lock().unwrap().0).unwrap_or(0), "guard_refused": e.as_ref().map(|e| e.guard.lock().unwrap().1).unwrap_or(0), "reported": e.as_ref().map(|e| e.reported.lock().unwrap().clone()).unwrap_or_default()}))
            .collect();
        Ok(obj(json!({"tabs": tabs, "engines": status, "max_tabs": MAX_TABS})))
    }

    fn close(&self, request: &Map<String, Value>) -> Reply {
        let id = request.get("tab").and_then(Value::as_str).unwrap_or("");
        let mut engines = self.engines.lock().unwrap();
        let (entry, left) = {
            let mut tabs = self.hub.tabs.lock().unwrap();
            let at = tabs.iter().position(|e| e.id == id).ok_or("unknown_tab")?;
            let entry = tabs.remove(at);
            let left = tabs.iter().filter(|e| e.engine == entry.engine).count();
            (entry, left)
        };
        let mut stopped = false;
        if let Some(engine) = engines[entry.engine].as_ref() {
            if let Some(target) = &entry.target {
                let _ = engine.tx.send(Cmd::Close(target.clone()));
            }
            if left == 0 || engine.finished() {
                if let Some(engine) = engines[entry.engine].take() {
                    engine.end();
                    stopped = true;
                }
            }
        }
        self.hub.changed.notify_all(); // a watch of this tab is released
        Ok(obj(json!({"closed": id, "engine_stopped": stopped})))
    }

    fn shot(&self, request: &Map<String, Value>) -> Reply {
        let id = request.get("tab").and_then(Value::as_str).unwrap_or("");
        let path = request.get("path").and_then(Value::as_str).unwrap_or("");
        let end = Instant::now() + SHOT_WAIT;
        let image = loop {
            {
                let mut tabs = self.hub.tabs.lock().unwrap();
                let entry = tabs.iter_mut().find(|e| e.id == id).ok_or("unknown_tab")?;
                if matches!(entry.tab.phase(), Phase::Absent { .. } | Phase::Failed(_)) {
                    return Err("no_frame");
                }
                if let Some(img) = entry.tab.paint() {
                    break img.clone();
                }
            }
            if Instant::now() >= end {
                return Err("no_frame");
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        write_png(path, &image).map(|(w, h)| obj(json!({"tab": id, "path": path, "width": w, "height": h})))
    }

    pub fn stop(&self) {
        let mut engines = self.engines.lock().unwrap();
        for slot in engines.iter_mut() {
            if let Some(e) = slot.take() {
                e.end();
            }
        }
        self.hub.tabs.lock().unwrap().clear();
        self.hub.changed.notify_all(); // every watch is released
    }
}

/// Writes `image` as a PNG to a new file (mode 0600, never over an existing file); returns its size.
fn write_png(path: &str, image: &lico_frame::Image) -> Result<(u32, u32), &'static str> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::AlreadyExists => "exists",
        std::io::ErrorKind::NotFound => "bad_request",
        _ => "write_failed",
    })?;
    let mut sink = std::io::BufWriter::new(file);
    let done = (|| -> std::io::Result<()> {
        let mut enc = lico_png::PngEncoder::new(&mut sink, image.width, image.height)?;
        let mut row = Vec::with_capacity(image.width as usize * 3);
        for line in image.pixels.chunks(image.width as usize) {
            row.clear();
            for p in line {
                row.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
            }
            enc.push_row(&row)?;
        }
        enc.finish()?;
        sink.flush()
    })();
    if done.is_err() {
        let _ = std::fs::remove_file(path);
        return Err("write_failed");
    }
    Ok((image.width, image.height))
}

impl Ops for Browser {
    fn names(&self) -> &'static [&'static str] {
        &NAMES
    }
    fn valid(&self, op: &str, request: &Map<String, Value>) -> bool {
        valid(op, request)
    }
    fn call(&self, shared: &Opaque, op: &str, request: &Map<String, Value>) -> Reply {
        match op {
            "browser.open" => self.open(shared, request),
            "browser.list" => self.list(),
            "browser.close" => self.close(request),
            "browser.shot" => self.shot(request),
            "browser.show" => self.show(request),
            "browser.resize" => self.resize(request),
            "browser.hide" => self.hide(request),
            "browser.go" => self.go(shared, request),
            "browser.nav" => self.nav(request),
            "browser.watch" => self.watch(request),
            "browser.input" => self.input(request),
            _ => Err("unknown_op"),
        }
    }
    fn stop(&self) {
        Browser::stop(self)
    }
}


#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn req(v: Value) -> Map<String, Value> {
        v.as_object().cloned().unwrap()
    }

    #[test]
    fn the_four_operations_accept_exactly_their_fields_and_refuse_unknown_ones() {
        assert!(valid("browser.open", &req(json!({"op": "browser.open", "url": "https://a.test/"}))));
        assert!(valid("browser.open", &req(json!({"op": "browser.open", "v": 1, "id": "x", "url": "https://a.test/", "local": true}))));
        assert!(valid("browser.list", &req(json!({"op": "browser.list"}))));
        assert!(valid("browser.close", &req(json!({"op": "browser.close", "tab": "b12"}))));
        assert!(valid("browser.shot", &req(json!({"op": "browser.shot", "tab": "b1", "path": "/tmp/a.png"}))));
        for (op, bad) in [
            ("browser.open", json!({"op": "browser.open"})),
            ("browser.open", json!({"op": "browser.open", "url": ""})),
            ("browser.open", json!({"op": "browser.open", "url": 5})),
            ("browser.open", json!({"op": "browser.open", "url": "https://a.test/", "local": "yes"})),
            ("browser.open", json!({"op": "browser.open", "url": "https://a.test/", "extra": 1})),
            ("browser.open", json!({"op": "browser.open", "url": "x".repeat(2049)})),
            ("browser.list", json!({"op": "browser.list", "tab": "b1"})),
            ("browser.close", json!({"op": "browser.close"})),
            ("browser.close", json!({"op": "browser.close", "tab": "b0"})),
            ("browser.shot", json!({"op": "browser.shot", "tab": "b1"})),
            ("browser.shot", json!({"op": "browser.shot", "path": "/tmp/a.png"})),
            ("browser.shot", json!({"op": "browser.shot", "tab": "b1", "path": "a.png"})),
            ("browser.nothing", json!({"op": "browser.nothing"})),
        ] {
            assert!(!valid(op, &req(bad.clone())), "{bad}");
        }
        assert!(!valid("browser.open", &req(json!({"op": "browser.open", "v": 2, "url": "https://a.test/"}))));
    }

    #[test]
    fn a_tab_id_and_a_picture_path_have_one_shape() {
        for ok in ["b1", "b9", "b10", "b999999"] {
            assert!(valid_tab_id(&json!(ok)), "{ok}");
        }
        for bad in ["", "b", "b0", "b01", "B1", "b-1", "b1 ", "tab1", "b1234567", "../b1"] {
            assert!(!valid_tab_id(&json!(bad)), "{bad:?}");
        }
        assert!(!valid_tab_id(&json!(1)) && !valid_tab_id(&Value::Null));
        for ok in ["/tmp/a.png", "/a", "/home/u/pics/page one.png"] {
            assert!(valid_shot_path(&json!(ok)), "{ok}");
        }
        for bad in ["", "a.png", "./a.png", "/tmp/../etc/x", "/tmp/./x", "/tmp/", "/tmp/a\n.png", "/tmp/a\u{0}.png", &format!("/{}", "a".repeat(1024))] {
            assert!(!valid_shot_path(&json!(bad)), "{bad:?}");
        }
    }

    pub(in crate::browser) fn world(name: &str) -> (std::path::PathBuf, Arc<Browser>, Opaque) {
        world_with(name, None)
    }

    /// A private home and state. With no `source` the self-install is switched off in the owner's setting file (so that no test downloads anything); with one, the browser installs from it.
    pub(in crate::browser) fn world_with(name: &str, source: Option<Source>) -> (std::path::PathBuf, Arc<Browser>, Opaque) {
        let dir = std::env::temp_dir().join(format!("lico_browser_ops_{name}_{}", crate::core::random_hex(6)));
        std::fs::create_dir_all(&dir).unwrap();
        let shared = crate::driver_hubs_tests::shared(&dir.join("state_parent_unused"));
        let state = std::env::temp_dir().join(format!("{}_state", dir.file_name().unwrap().to_string_lossy()));
        std::fs::create_dir_all(&state).unwrap();
        // the state directory is the parent of the registry directory: give the host a registry inside `state`
        let mut shared = Arc::try_unwrap(shared).ok().expect("the only owner");
        shared.cfg.registry_dir = state.join("registry");
        let at = Where { home: dir.clone(), xdg_data: None, state_dir: state.clone() };
        let browser = match source {
            Some(src) => Browser::installing_from(at, src),
            None => {
                std::fs::write(state.join("browser.json"), r#"{"auto_install": false}"#).unwrap();
                std::fs::set_permissions(state.join("browser.json"), std::fs::Permissions::from_mode(0o600)).unwrap();
                Browser::looking_in(at)
            }
        };
        (dir, browser, Arc::new(shared) as Opaque)
    }

    #[test]
    fn an_engine_that_is_not_installed_is_a_tab_in_the_absent_phase_with_the_message_and_never_a_process() {
        let (home, browser, shared) = world("absent");
        let reply = browser.call(&shared, "browser.open", &req(json!({"op": "browser.open", "url": "https://a.test/"}))).unwrap();
        assert_eq!(reply["phase"], json!("absent"));
        assert_eq!((reply["release"].clone(), reply["download"].clone(), reply["archive_sha256"].clone(), reply["engine_sha256"].clone()), (json!("v0.2.6"), json!(ARCHIVE_URL), json!(ARCHIVE_SHA256), json!(ENGINE_SHA256)));
        assert_eq!(reply["directory"], json!(home.join(".local/share/lico/browser/v0.2.6").to_string_lossy()), "where to put it");
        let listed = browser.call(&shared, "browser.list", &req(json!({"op": "browser.list"}))).unwrap();
        let tab = &listed["tabs"][0];
        assert_eq!((tab["phase"].clone(), tab["expected_sha256"].clone(), tab["download"].clone()), (json!("absent"), json!(ENGINE_SHA256), json!(ARCHIVE_URL)));
        assert!(listed["engines"].as_array().unwrap().iter().all(|e| e["running"] == json!(false)), "no engine was started");
        assert_eq!(browser.call(&shared, "browser.shot", &req(json!({"op": "browser.shot", "tab": "b1", "path": "/tmp/never.png"}))), Err("no_frame"));
        let closed = browser.call(&shared, "browser.close", &req(json!({"op": "browser.close", "tab": "b1"}))).unwrap();
        assert_eq!(closed["engine_stopped"], json!(false));
        assert_eq!(browser.call(&shared, "browser.close", &req(json!({"op": "browser.close", "tab": "b1"}))), Err("unknown_tab"));
    }

    #[test]
    fn an_engine_that_is_unsafe_or_not_the_release_or_whose_setting_is_damaged_is_an_error_and_no_tab() {
        let (home, browser, shared) = world("refused");
        let open = |b: &Browser| b.call(&shared, "browser.open", &req(json!({"op": "browser.open", "url": "https://a.test/"})));
        // a file that is not the release: the hash does not match
        let dir = home.join(".local/share/lico/browser/v0.2.6");
        std::fs::create_dir_all(&dir).unwrap();
        let mut at = home.clone();
        for part in dir.strip_prefix(&home).unwrap().components() {
            at.push(part);
            std::fs::set_permissions(&at, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        for name in ["obscura", "obscura-worker"] {
            std::fs::write(dir.join(name), b"not the release").unwrap();
            std::fs::set_permissions(dir.join(name), std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        assert_eq!(open(&browser), Err("engine_not_pinned"));
        // a directory that others can write
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777)).unwrap();
        assert_eq!(open(&browser), Err("engine_unsafe"));
        // a damaged setting is an error, not a fall back to the pinned engine
        let state = home.parent().unwrap().join(format!("{}_state", home.file_name().unwrap().to_string_lossy()));
        std::fs::write(state.join("browser.json"), "not json").unwrap();
        std::fs::set_permissions(state.join("browser.json"), std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(open(&browser), Err("engine_setting"));
        let listed = browser.call(&shared, "browser.list", &req(json!({"op": "browser.list"}))).unwrap();
        assert!(listed["tabs"].as_array().unwrap().is_empty(), "no tab is left behind by a refused open");
    }

    #[test]
    fn an_address_the_model_refuses_is_a_bad_request_before_anything_starts() {
        let (_home, browser, shared) = world("badurl");
        for bad in ["ftp://x.test/", "javascript:alert(1)", "file:///etc/passwd", "http://user:pw@a.test/"] {
            assert_eq!(browser.call(&shared, "browser.open", &req(json!({"op": "browser.open", "url": bad}))), Err("bad_request"), "{bad}");
        }
    }

    fn open_absent(b: &Browser, shared: &Opaque) -> String {
        let r = b.call(shared, "browser.open", &req(json!({"op": "browser.open", "url": "https://a.test/"}))).unwrap();
        r["tab"].as_str().unwrap().to_string()
    }

    #[test]
    fn the_window_operations_accept_exactly_their_fields() {
        let ok = |v: Value| valid(v["op"].as_str().unwrap(), &req(v.clone()));
        assert!(ok(json!({"op": "browser.show", "tab": "b1", "width": 800, "height": 600})));
        assert!(ok(json!({"op": "browser.resize", "tab": "b1", "width": 1, "height": 16384})));
        assert!(ok(json!({"op": "browser.hide", "tab": "b1"})));
        assert!(ok(json!({"op": "browser.go", "tab": "b1", "address": "a.test"})));
        assert!(ok(json!({"op": "browser.nav", "tab": "b1", "action": "reload"})));
        assert!(ok(json!({"op": "browser.watch", "tab": "b1", "seq": 0})));
        assert!(ok(json!({"op": "browser.watch", "tab": "b1", "seq": 5, "frame_seq": 3, "frame": true, "v": 1, "id": "x"})));
        for bad in [
            json!({"op": "browser.show", "tab": "b1", "width": 800}),
            json!({"op": "browser.show", "tab": "b1", "width": 0, "height": 600}),
            json!({"op": "browser.show", "tab": "b1", "width": 20000, "height": 600}),
            json!({"op": "browser.show", "tab": "b1", "width": "8", "height": 6}),
            json!({"op": "browser.hide"}),
            json!({"op": "browser.hide", "tab": "b1", "extra": 1}),
            json!({"op": "browser.go", "tab": "b1"}),
            json!({"op": "browser.go", "tab": "b1", "address": ""}),
            json!({"op": "browser.nav", "tab": "b1", "action": "stop"}),
            json!({"op": "browser.nav", "tab": "bx", "action": "back"}),
            json!({"op": "browser.watch", "tab": "b1"}),
            json!({"op": "browser.watch", "tab": "b1", "seq": -1}),
            json!({"op": "browser.watch", "tab": "b1", "seq": 1, "frame": 1}),
            json!({"op": "browser.watch", "tab": "b1", "seq": 1, "timeout": 5}),
        ] {
            assert!(!ok(bad.clone()), "{bad}");
        }
    }

    #[test]
    fn show_resize_hide_and_nav_are_the_models_intents_and_the_viewport_is_capped_and_said() {
        let (_home, browser, shared) = world("window");
        let id = open_absent(&browser, &shared);
        let call = |op: &str, mut v: Value| {
            v["op"] = json!(op);
            v["tab"] = json!(id);
            browser.call(&shared, op, &req(v))
        };
        let r = call("browser.show", json!({"width": 3000, "height": 2000})).unwrap();
        assert_eq!((r["viewport"].clone(), r["viewport_capped"].clone()), (json!([1920, 1200]), json!(true)), "{r:?}");
        let r = call("browser.resize", json!({"width": 800, "height": 600})).unwrap();
        assert_eq!((r["viewport"].clone(), r["viewport_capped"].clone()), (json!([800, 600]), json!(false)));
        assert!(call("browser.hide", json!({})).is_ok());
        let row = &browser.call(&shared, "browser.list", &req(json!({"op": "browser.list"}))).unwrap()["tabs"][0];
        assert_eq!(row["viewport"], json!([800, 600]));
        assert!(call("browser.nav", json!({"action": "back"})).is_ok() && call("browser.nav", json!({"action": "reload"})).is_ok());
        assert_eq!(browser.call(&shared, "browser.hide", &req(json!({"op": "browser.hide", "tab": "b99"}))), Err("unknown_tab"));
        assert_eq!(browser.call(&shared, "browser.show", &req(json!({"op": "browser.show", "tab": "b99", "width": 9, "height": 9}))), Err("unknown_tab"));
    }

    #[test]
    fn go_names_the_models_refusal_and_on_a_tab_without_an_engine_it_tries_the_engine_again() {
        let (_home, browser, shared) = world("go");
        let id = open_absent(&browser, &shared);
        let go = |address: &str| browser.call(&shared, "browser.go", &req(json!({"op": "browser.go", "tab": id, "address": address})));
        assert_eq!(go("   "), Err("address_empty"));
        assert_eq!(go("ftp://x.test/"), Err("address_scheme"));
        assert_eq!(go("http://user:pw@x.test/"), Err("address_credentials"));
        assert_eq!(go("http://a b/"), Err("address_invalid"));
        let r = go("https://b.test/").unwrap();
        assert_eq!((r["restarted"].clone(), r["phase"].clone(), r["url"].clone()), (json!(true), json!("absent"), json!("https://b.test/")), "the engine is still not installed: the message again, with the new address");
        let row = browser.call(&shared, "browser.list", &req(json!({"op": "browser.list"}))).unwrap()["tabs"][0].clone();
        assert_eq!((row["url"].clone(), row["phase"].clone()), (json!("https://b.test/"), json!("absent")));
    }

    #[test]
    fn watch_through_the_operation_answers_hidden_gone_and_what_changed() {
        let (_home, browser, shared) = world("watchop");
        let id = open_absent(&browser, &shared);
        let watch = |seq: u64| browser.call(&shared, "browser.watch", &req(json!({"op": "browser.watch", "tab": id, "seq": seq, "frame": true}))).unwrap();
        let first = watch(0);
        assert!(first["seq"].as_u64().unwrap() >= 1 && first["tab"]["tab"] == json!(id) && first.get("png").is_none(), "{first:?}");
        browser.call(&shared, "browser.hide", &req(json!({"op": "browser.hide", "tab": id}))).unwrap();
        assert_eq!(watch(99)["hidden"], json!(true));
        browser.call(&shared, "browser.close", &req(json!({"op": "browser.close", "tab": id}))).unwrap();
        assert_eq!(watch(0)["gone"], json!(true));
    }

    #[test]
    fn the_input_operation_accepts_exactly_what_a_kind_needs() {
        let ok = |v: Value| valid("browser.input", &req(v));
        assert!(ok(json!({"op": "browser.input", "tab": "b1", "kind": "move", "x": 10, "y": 20})));
        assert!(ok(json!({"op": "browser.input", "tab": "b1", "kind": "down", "x": 1, "y": 2, "button": "left", "clicks": 2})));
        assert!(ok(json!({"op": "browser.input", "tab": "b1", "kind": "up", "x": 1, "y": 2, "button": "right"})));
        assert!(ok(json!({"op": "browser.input", "tab": "b1", "kind": "wheel", "x": 1, "y": 2, "delta": -120.0})));
        assert!(ok(json!({"op": "browser.input", "tab": "b1", "kind": "key_down", "key": "Enter", "code": "Enter"})));
        assert!(ok(json!({"op": "browser.input", "tab": "b1", "kind": "key_down", "key": "a", "code": "KeyA", "text": "a"})));
        assert!(ok(json!({"op": "browser.input", "tab": "b1", "kind": "text", "text": "안녕"})));
        for bad in [
            json!({"op": "browser.input", "tab": "b1"}),
            json!({"op": "browser.input", "tab": "b1", "kind": "click", "x": 1, "y": 1}),
            json!({"op": "browser.input", "tab": "b1", "kind": "move", "x": 1}),
            json!({"op": "browser.input", "tab": "b1", "kind": "move", "x": 1, "y": 1, "text": "a"}),
            json!({"op": "browser.input", "tab": "b1", "kind": "down", "x": 1, "y": 1}),
            json!({"op": "browser.input", "tab": "b1", "kind": "down", "x": 1, "y": 1, "button": "left", "clicks": 4}),
            json!({"op": "browser.input", "tab": "b1", "kind": "wheel", "x": 1, "y": 1}),
            json!({"op": "browser.input", "tab": "b1", "kind": "wheel", "x": 1, "y": 1, "delta": 1e9}),
            json!({"op": "browser.input", "tab": "b1", "kind": "key_down"}),
            json!({"op": "browser.input", "tab": "b1", "kind": "key_down", "key": "a\nb"}),
            json!({"op": "browser.input", "tab": "b1", "kind": "key_down", "key": "a", "text": "123456789"}),
            json!({"op": "browser.input", "tab": "b1", "kind": "key_down", "key": "a", "x": 1}),
            json!({"op": "browser.input", "tab": "b1", "kind": "text"}),
            json!({"op": "browser.input", "tab": "b1", "kind": "text", "text": "a", "key": "a"}),
            json!({"op": "browser.input", "tab": "b1", "kind": "text", "text": "x".repeat(16_385)}),
            json!({"op": "browser.input", "tab": "b1", "kind": "move", "x": 1, "y": 1, "extra": 1}),
            json!({"op": "browser.input", "tab": "b1", "kind": "move", "x": 1_000_000, "y": 1}),
        ] {
            assert!(!ok(bad.clone()), "{bad}");
        }
    }

    #[test]
    fn no_input_goes_to_a_tab_that_is_absent_hidden_or_not_there() {
        let (_home, browser, shared) = world("input");
        let id = open_absent(&browser, &shared);
        let input = |v: Value| browser.call(&shared, "browser.input", &req(v));
        let kinds = [json!({"kind": "move", "x": 5, "y": 5}), json!({"kind": "down", "x": 5, "y": 5, "button": "left"}), json!({"kind": "key_down", "key": "a"}), json!({"kind": "text", "text": "a"})];
        for mut k in kinds.clone() {
            k["op"] = json!("browser.input");
            k["tab"] = json!(id);
            assert_eq!(input(k.clone()).unwrap()["sent"], json!(false), "an absent engine takes no input: {k}");
        }
        assert_eq!(input(json!({"op": "browser.input", "tab": "b99", "kind": "text", "text": "a"})), Err("unknown_tab"));
    }

    #[test]
    fn a_picture_is_written_to_a_new_owner_only_file_and_never_over_another() {
        let dir = std::env::temp_dir().join(format!("lico_browser_shot_{}", crate::core::random_hex(6)));
        std::fs::create_dir_all(&dir).unwrap();
        let image = lico_frame::Image { width: 3, height: 2, pixels: vec![0xFF_10_20_30, 0xFF_40_50_60, 0xFF_70_80_90, 0xFF_00_00_00, 0xFF_FF_FF_FF, 0xFF_12_34_56] };
        let path = dir.join("a.png");
        assert_eq!(write_png(path.to_str().unwrap(), &image), Ok((3, 2)));
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        let back = lico_frame::decode(&std::fs::read(&path).unwrap(), &lico_frame::Limits::default()).unwrap();
        assert_eq!(back.pixels, image.pixels, "the pixels come back as they went in (the order of the colours is right)");
        assert_eq!(write_png(path.to_str().unwrap(), &image), Err("exists"));
        assert_eq!(write_png(dir.join("no/such/dir/a.png").to_str().unwrap(), &image), Err("bad_request"));
    }
}
