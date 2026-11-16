//! The operations the window uses for a shown tab (M17-P07): `browser.show`, `browser.resize`, `browser.hide`, `browser.go` (an address), `browser.nav` (back, forward,
//! reload) and the blocking `browser.watch`. They are the model's own intents (`lico-browser-view`) and the effects the model answers with go to the tab's engine; the socket
//! and `POST /v1` answer them alike. `go` on a tab whose engine ended (or was never installed) starts the engine again: the person's next action is the restart.

use super::ops::Browser;
use super::supervisor::Cmd;
use super::{map, watch};
use lico_browser_view::{Effect, Phase, UrlError};
use serde_json::{json, Map, Value};

pub const WINDOW_NAMES: [&str; 6] = ["browser.show", "browser.resize", "browser.hide", "browser.go", "browser.nav", "browser.watch"];

type Reply = Result<Map<String, Value>, &'static str>;

fn obj(v: Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap_or_default()
}

/// The protocol code of an address the model refuses.
pub fn address_code(e: UrlError) -> &'static str {
    match e {
        UrlError::Empty => "address_empty",
        UrlError::TooLong => "address_too_long",
        UrlError::Scheme => "address_scheme",
        UrlError::Credentials => "address_credentials",
        UrlError::Invalid => "address_invalid",
    }
}

fn int(request: &Map<String, Value>, key: &str) -> u64 {
    request.get(key).and_then(Value::as_u64).unwrap_or(0)
}

/// The request fields of the window's operations: unknown fields are refused, as for the others.
pub fn valid(op: &str, request: &Map<String, Value>) -> bool {
    let allowed: &[&str] = match op {
        "browser.show" | "browser.resize" => &["tab", "width", "height"],
        "browser.hide" => &["tab"],
        "browser.go" => &["tab", "address"],
        "browser.nav" => &["tab", "action"],
        "browser.watch" => &["tab", "seq", "frame_seq", "frame"],
        _ => return false,
    };
    let required: &[&str] = match op {
        "browser.show" | "browser.resize" => &["tab", "width", "height"],
        "browser.go" => &["tab", "address"],
        "browser.nav" => &["tab", "action"],
        "browser.watch" => &["tab", "seq"],
        _ => &["tab"],
    };
    request.iter().all(|(k, v)| {
        matches!(k.as_str(), "op" | "v" | "id") && super::ops::valid_common(k, v)
            || allowed.contains(&k.as_str())
                && match k.as_str() {
                    "tab" => super::ops::valid_tab_id(v),
                    "width" | "height" => v.as_u64().is_some_and(|n| (1..=16384).contains(&n)),
                    "address" => matches!(v, Value::String(s) if !s.is_empty() && s.len() <= lico_browser_view::MAX_URL),
                    "action" => matches!(v.as_str(), Some("back" | "forward" | "reload")),
                    "seq" | "frame_seq" => v.as_u64().is_some(),
                    "frame" => v.is_boolean(),
                    _ => false,
                }
    }) && required.iter().all(|k| request.contains_key(*k))
}

impl Browser {
    fn now_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }

    /// Runs `f` on the model of tab `id`, marks the change, and hands the effects it answers with to the tab's engine.
    fn intent<T>(&self, id: &str, f: impl FnOnce(&mut super::supervisor::Entry) -> (T, Vec<Effect>)) -> Result<T, &'static str> {
        let engines = self.engines.lock().unwrap();
        let (value, fx, engine) = {
            let mut tabs = self.hub.tabs.lock().unwrap();
            let e = tabs.iter_mut().find(|e| e.id == id).ok_or("unknown_tab")?;
            let (value, fx) = f(e);
            self.hub.touch(e);
            (value, fx, e.engine)
        };
        if !fx.is_empty() {
            if let Some(engine) = engines[engine].as_ref() {
                let _ = engine.tx.send(Cmd::Effects(id.to_string(), fx));
            }
        }
        Ok(value)
    }

    fn sized(&self, request: &Map<String, Value>, resize: bool) -> Reply {
        let id = request.get("tab").and_then(Value::as_str).unwrap_or("");
        let area = (int(request, "width") as u32, int(request, "height") as u32);
        let now = self.now_ms();
        self.intent(id, |e| {
            let fx = if resize { e.tab.resize(area) } else { e.tab.show(area, now) };
            let (w, h) = e.tab.viewport();
            (obj(json!({"tab": id, "viewport": [w, h], "viewport_capped": e.tab.viewport_capped()})), fx)
        })
    }

    pub(super) fn show(&self, request: &Map<String, Value>) -> Reply {
        self.sized(request, false)
    }

    pub(super) fn resize(&self, request: &Map<String, Value>) -> Reply {
        self.sized(request, true)
    }

    pub(super) fn hide(&self, request: &Map<String, Value>) -> Reply {
        let id = request.get("tab").and_then(Value::as_str).unwrap_or("");
        self.intent(id, |e| (obj(json!({"tab": id})), e.tab.hide()))
    }

    pub(super) fn nav(&self, request: &Map<String, Value>) -> Reply {
        let id = request.get("tab").and_then(Value::as_str).unwrap_or("");
        let action = request.get("action").and_then(Value::as_str).unwrap_or("");
        self.intent(id, |e| {
            let fx = match action {
                "back" => e.tab.back(),
                "forward" => e.tab.forward(),
                _ => e.tab.reload(),
            };
            (obj(json!({"tab": id})), fx)
        })
    }

    pub(super) fn go(&self, shared: &crate::driver::Opaque, request: &Map<String, Value>) -> Reply {
        let id = request.get("tab").and_then(Value::as_str).unwrap_or("");
        let typed = request.get("address").and_then(Value::as_str).unwrap_or("");
        let url = lico_browser_view::parse_address(typed).map_err(address_code)?;
        let restart = {
            let tabs = self.hub.tabs.lock().unwrap();
            let e = tabs.iter().find(|e| e.id == id).ok_or("unknown_tab")?;
            matches!(e.tab.phase(), Phase::Failed(_) | Phase::Absent { .. })
        };
        if !restart {
            return self.intent(id, |e| {
                let fx = e.tab.submit_address(&url).unwrap_or_default();
                (obj(json!({"tab": id, "url": url})), fx)
            });
        }
        // the engine ended, or was not installed: the person's next action starts it again, and the tab begins afresh (its history is the old engine's)
        let mut engines = self.engines.lock().unwrap();
        let (index, local, area) = {
            let tabs = self.hub.tabs.lock().unwrap();
            let e = tabs.iter().find(|e| e.id == id).ok_or("unknown_tab")?;
            (e.engine, e.local, e.tab.viewport())
        };
        let prepared = self.ensure_engine(&mut engines, shared, index)?;
        let tab = Browser::new_tab(&prepared, local, area, &url);
        {
            let mut tabs = self.hub.tabs.lock().unwrap();
            let e = tabs.iter_mut().find(|e| e.id == id).ok_or("unknown_tab")?;
            e.tab = tab;
            e.target = None;
            e.session = None;
            self.hub.touch(e);
        }
        if prepared.absent.is_none() {
            if let Some(engine) = engines[index].as_ref() {
                let _ = engine.tx.send(Cmd::Open(id.to_string()));
            }
        }
        Ok(obj(json!({"tab": id, "url": url, "restarted": true, "phase": if prepared.absent.is_some() { "absent" } else { "starting" }})))
    }

    pub(super) fn watch(&self, request: &Map<String, Value>) -> Reply {
        let id = request.get("tab").and_then(Value::as_str).unwrap_or("");
        let (seq, frame_seq) = (int(request, "seq"), int(request, "frame_seq"));
        let want = request.get("frame").and_then(Value::as_bool).unwrap_or(false);
        // no engines lock here: the call may wait for ever, and the rest of the browser must not wait with it
        Ok(match watch::watch(&self.hub, id, seq, frame_seq, want, &Browser::row) {
            watch::Watch::Gone => obj(json!({"gone": true})),
            watch::Watch::Superseded => obj(json!({"superseded": true})),
            watch::Watch::Hidden => obj(json!({"hidden": true})),
            watch::Watch::Now { seq, row, frame } => {
                let mut r = obj(json!({"seq": seq, "tab": row}));
                if let Some((n, bytes)) = frame {
                    r.insert("frame_seq".into(), json!(n));
                    r.insert("png".into(), json!(map::base64_encode(&bytes)));
                }
                r
            }
        })
    }
}
