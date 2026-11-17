//! The operations the window uses for a shown tab (M17-P07): `browser.show`, `browser.resize`, `browser.hide`, `browser.go` (an address), `browser.nav` (back, forward,
//! reload) and the blocking `browser.watch`. They are the model's own intents (`lico-browser-view`) and the effects the model answers with go to the tab's engine; the socket
//! and `POST /v1` answer them alike. `go` on a tab whose engine ended (or was never installed) starts the engine again: the person's next action is the restart.

use super::ops::Browser;
use super::supervisor::Cmd;
use super::{map, watch};
use lico_browser_view::{Effect, Phase, UrlError};
use serde_json::{json, Map, Value};

pub const WINDOW_NAMES: [&str; 7] = ["browser.show", "browser.resize", "browser.hide", "browser.go", "browser.nav", "browser.watch", "browser.input"];

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
        "browser.input" => &["tab", "kind", "x", "y", "button", "clicks", "delta", "key", "code", "text"],
        _ => return false,
    };
    let required: &[&str] = match op {
        "browser.show" | "browser.resize" => &["tab", "width", "height"],
        "browser.go" => &["tab", "address"],
        "browser.nav" => &["tab", "action"],
        "browser.watch" => &["tab", "seq"],
        "browser.input" => &["tab", "kind"],
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
                    "kind" => matches!(v.as_str(), Some("move" | "down" | "up" | "wheel" | "key_down" | "key_up" | "text")),
                    "x" | "y" => v.as_i64().is_some_and(|n| (-100_000..=100_000).contains(&n)),
                    "button" => matches!(v.as_str(), Some("left" | "middle" | "right")),
                    "clicks" => v.as_u64().is_some_and(|n| (1..=3).contains(&n)),
                    "delta" => v.as_f64().is_some_and(|d| d.abs() <= 10_000.0),
                    "key" | "code" => matches!(v, Value::String(s) if !s.is_empty() && s.chars().count() <= 32 && !s.chars().any(char::is_control)),
                    "text" => matches!(v, Value::String(s) if !s.is_empty() && s.len() <= 4 * lico_browser_view::MAX_INSERT),
                    "frame" => v.is_boolean(),
                    _ => false,
                }
    }) && required.iter().all(|k| request.contains_key(*k)) && input_shape(op, request)
}

/// What each kind of input needs: a point for the mouse, a key for a key, text for text, and no more than a key event's own text.
fn input_shape(op: &str, r: &Map<String, Value>) -> bool {
    if op != "browser.input" {
        return true;
    }
    let has = |k: &str| r.contains_key(k);
    match r.get("kind").and_then(Value::as_str) {
        Some("move") => has("x") && has("y") && !has("text") && !has("key"),
        Some("down" | "up") => has("x") && has("y") && has("button") && !has("text") && !has("key"),
        Some("wheel") => has("x") && has("y") && has("delta") && !has("text") && !has("key"),
        Some("key_down" | "key_up") => has("key") && !has("x") && !has("delta") && r.get("text").and_then(Value::as_str).map_or(true, |t| t.chars().count() <= 8),
        Some("text") => has("text") && !has("key") && !has("x"),
        _ => false,
    }
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
        let prepared = self.ensure_engine(&mut engines, shared, index, true)?;
        let tab = Browser::new_tab(&prepared, &self.source, local, area, &url);
        {
            let mut tabs = self.hub.tabs.lock().unwrap();
            let e = tabs.iter_mut().find(|e| e.id == id).ok_or("unknown_tab")?;
            e.tab = tab;
            e.target = None;
            e.session = None;
            self.hub.touch(e);
        }
        if prepared.absent.is_none() && !prepared.installing {
            if let Some(engine) = engines[index].as_ref() {
                let _ = engine.tx.send(Cmd::Open(id.to_string()));
            }
        }
        Ok(obj(json!({"tab": id, "url": url, "restarted": true, "phase": Browser::phase_word(&prepared)})))
    }

    /// The person's input (a mouse move, a press, a wheel step, a key, committed text) for the page: the model decides (no input to a tab that is not shown, not active or has the
    /// address row's focus), the effect is written to the engine at once. `sent` says whether it was.
    pub(super) fn input(&self, request: &Map<String, Value>) -> Reply {
        use lico_browser_view::{Button, KeyKind, MouseKind};
        let id = request.get("tab").and_then(Value::as_str).unwrap_or("");
        let s = |k: &str| request.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let (x, y) = (request.get("x").and_then(Value::as_i64).unwrap_or(0) as i32, request.get("y").and_then(Value::as_i64).unwrap_or(0) as i32);
        let button = match request.get("button").and_then(Value::as_str) {
            Some("middle") => Button::Middle,
            Some("right") => Button::Right,
            Some("left") => Button::Left,
            _ => Button::None,
        };
        let clicks = int(request, "clicks").max(1) as u32;
        let delta = request.get("delta").and_then(Value::as_f64).unwrap_or(0.0);
        let engines = self.engines.lock().unwrap();
        let (fx, engine) = {
            let mut tabs = self.hub.tabs.lock().unwrap();
            let e = tabs.iter_mut().find(|e| e.id == id).ok_or("unknown_tab")?;
            let fx = match s("kind").as_str() {
                "move" => e.tab.mouse(MouseKind::Moved, x, y, Button::None, 0, 0.0),
                "down" => e.tab.mouse(MouseKind::Pressed, x, y, button, clicks, 0.0),
                "up" => e.tab.mouse(MouseKind::Released, x, y, button, clicks, 0.0),
                "wheel" => e.tab.mouse(MouseKind::Wheel, x, y, Button::None, 0, delta),
                "key_down" => e.tab.key(KeyKind::Down, &s("key"), &s("code"), &s("text")),
                "key_up" => e.tab.key(KeyKind::Up, &s("key"), &s("code"), ""),
                _ => e.tab.text(&s("text")),
            };
            (fx, e.engine)
        };
        let sent = fx.is_some();
        if let (Some(effect), Some(engine)) = (fx, engines[engine].as_ref()) {
            let _ = engine.tx.send(Cmd::Effects(id.to_string(), vec![effect]));
        }
        Ok(obj(json!({"tab": id, "sent": sent})))
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
                    // the key keeps its first name; the bytes are the frame as the engine sent it (JPEG now, PNG when a test double sends one)
                    r.insert("png".into(), json!(map::base64_encode(&bytes)));
                }
                r
            }
        })
    }
}
