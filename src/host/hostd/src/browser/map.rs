//! The pure mappings between the tab model and the engine's protocol: what an effect of the model becomes as a call, what an event of the engine becomes for the model, the
//! honest user-agent, the base64 of a frame, and where a refusal the fetch guard saw is shown.

use lico_browser_view::{Button as VButton, Effect, EngineEvent, KeyKind as VKey, MouseKind as VMouse};
use lico_cdp::{cmd, method, Button, Event, ImageFormat, KeyKind, MouseKind};
use serde_json::{json, Value};
use std::time::Duration;

/// The user-agent a page is told: Lico and the engine with its version, claiming no Chrome version (mail 147 item 5). `engine` is the pinned release's number, or `unpinned`.
pub fn user_agent(lico_version: &str, engine: &str) -> String {
    format!("Mozilla/5.0 (X11; Linux x86_64) Lico/{lico_version} Moli/{engine}")
}

/// The most of the discovery page's answer that is read.
pub const DISCOVERY_MAX: usize = 16 << 10;

/// The path of the engine's browser socket from the answer of its discovery page (`GET /json/version`): a whole HTTP response whose status is 200 and whose JSON body (sent whole or in one
/// chunk) holds `webSocketDebuggerUrl` as `ws://HOST/devtools/browser/ID`, ID being letters, digits and hyphens. Only the path is taken: the host connects to the loopback port it told
/// the engine to bind, whatever host the page names. Anything else is `None`.
pub fn socket_path(response: &str) -> Option<String> {
    let (head, body) = response.split_once("\r\n\r\n")?;
    if !(head.starts_with("HTTP/1.1 200") || head.starts_with("HTTP/1.0 200")) {
        return None;
    }
    let (open, close) = (body.find('{')?, body.rfind('}')?);
    let v: Value = serde_json::from_str(body.get(open..=close)?).ok()?;
    let rest = v.get("webSocketDebuggerUrl")?.as_str()?.strip_prefix("ws://")?;
    let path = &rest[rest.find('/')?..];
    let id = path.strip_prefix("/devtools/browser/")?;
    (!id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')).then(|| path.to_string())
}

pub struct Wire {
    pub method: &'static str,
    pub params: Value,
    pub timeout: Duration,
}

const SHORT: Duration = Duration::from_secs(5);
/// A navigation may take as long as the engine allows itself (its own deadline is 30 s); the call gives up a little sooner so that one slow page does not hold the tabs of an engine.
const NAVIGATION: Duration = Duration::from_secs(20);

/// What an effect of the model is to the engine. `viewport` is the tab's current size (the screencast is asked not to be larger).
pub fn wire_of(effect: &Effect, viewport: (u32, u32)) -> Wire {
    let (method, params, timeout) = match effect {
        Effect::SetViewport { width, height } => (method::SET_VIEWPORT, cmd::set_viewport(*width, *height, 1.0), SHORT),
        Effect::StartScreencast => (method::START_SCREENCAST, cmd::start_screencast(ImageFormat::Png, 100, viewport.0, viewport.1, 1), SHORT),
        Effect::StopScreencast => ("Page.stopScreencast", json!({}), SHORT),
        Effect::Ack(id) => (method::SCREENCAST_ACK, cmd::screencast_ack(*id), SHORT),
        Effect::CaptureScreenshot => (method::CAPTURE_SCREENSHOT, cmd::capture_screenshot(ImageFormat::Png), SHORT),
        Effect::Navigate(url) => (method::NAVIGATE, cmd::navigate(url), NAVIGATION),
        Effect::Reload => ("Page.reload", json!({}), NAVIGATION),
        Effect::Mouse { kind, x, y, button, clicks, delta } => (method::MOUSE, cmd::mouse(mouse_kind(*kind), *x, *y, button_of(*button), *clicks, *delta), SHORT),
        Effect::Key { kind, key, code, text } => (method::KEY, cmd::key(key_kind(*kind), key, code, text), SHORT),
        Effect::InsertText(text) => (method::INSERT_TEXT, cmd::insert_text(text), SHORT),
    };
    Wire { method, params, timeout }
}

fn mouse_kind(k: VMouse) -> MouseKind {
    match k {
        VMouse::Pressed => MouseKind::Pressed,
        VMouse::Released => MouseKind::Released,
        VMouse::Moved => MouseKind::Moved,
        VMouse::Wheel => MouseKind::Wheel,
    }
}

fn button_of(b: VButton) -> Button {
    match b {
        VButton::None => Button::None,
        VButton::Left => Button::Left,
        VButton::Middle => Button::Middle,
        VButton::Right => Button::Right,
    }
}

fn key_kind(k: VKey) -> KeyKind {
    match k {
        VKey::Down => KeyKind::Down,
        VKey::Up => KeyKind::Up,
        VKey::Char => KeyKind::Char,
    }
}

pub use crate::base64::{base64_decode, base64_encode};

/// What an event of the engine means for the tab of its session (the host picks the tab by `event.session_id`). A frame whose data is not base64 is an empty frame: it is still
/// acknowledged (the engine waits for that) and counted as a decode error when it is about to be painted.
pub fn engine_events(event: &Event) -> Vec<EngineEvent> {
    let p = &event.params;
    match event.method.as_str() {
        "Page.screencastFrame" => {
            let id = p.get("sessionId").and_then(Value::as_u64).unwrap_or(0);
            let png = p.get("data").and_then(Value::as_str).and_then(base64_decode).unwrap_or_default();
            vec![EngineEvent::Frame { id, png }]
        }
        "Page.frameNavigated" => {
            let frame = &p["frame"];
            match (frame.get("parentId"), frame.get("url").and_then(Value::as_str)) {
                (None, Some(url)) => vec![EngineEvent::Navigated(url.to_string())],
                _ => Vec::new(),
            }
        }
        "Page.navigatedWithinDocument" => p.get("url").and_then(Value::as_str).map(|u| vec![EngineEvent::Navigated(u.to_string())]).unwrap_or_default(),
        "Page.frameStartedLoading" => vec![EngineEvent::LoadStarted],
        "Page.loadEventFired" => vec![EngineEvent::LoadFinished],
        "Target.targetInfoChanged" => p["targetInfo"].get("title").and_then(Value::as_str).map(|t| vec![EngineEvent::Title(t.to_string())]).unwrap_or_default(),
        "Target.targetCrashed" => vec![EngineEvent::Crashed],
        "Target.targetDestroyed" | "Inspector.detached" => vec![EngineEvent::TargetGone],
        _ => Vec::new(),
    }
}

/// The text of the engine's own refusal of an address on this computer or a private network (MEASURED, P05 for a number, P06 for the name `localhost`): matched here once; if
/// the engine's wording changes the person only loses the nicer note.
pub fn is_local_refusal(message: &str) -> bool {
    message.contains("private/internal IP address") || message.contains("localhost domain")
}

/// The host of an address as the guard names it: between `://` and the next `/`, `:`, `?` or `#`, in lowercase. An address with no scheme gives an empty host.
pub fn host_of(url: &str) -> String {
    let Some((_, rest)) = url.split_once("://") else { return String::new() };
    let rest = rest.rsplit_once('@').map_or(rest, |(_, h)| h);
    if let Some(inner) = rest.strip_prefix('[') {
        return inner.split(']').next().unwrap_or("").to_ascii_lowercase();
    }
    rest.split(['/', ':', '?', '#']).next().unwrap_or("").to_ascii_lowercase()
}

pub struct TabView<'a> {
    pub id: &'a str,
    pub loading: bool,
    pub url: &'a str,
}

/// Which tabs a refusal of the fetch guard is told to: the loading tabs whose address has that host, else every loading tab (the guard does not know the tab: a limitation, said
/// in the SPEC). Not a tab that is not loading.
pub fn refusal_targets(tabs: &[TabView<'_>], host: &str) -> Vec<String> {
    let loading: Vec<&TabView<'_>> = tabs.iter().filter(|t| t.loading).collect();
    let same: Vec<&&TabView<'_>> = loading.iter().filter(|t| host_of(t.url) == host.to_ascii_lowercase()).collect();
    if same.is_empty() {
        loading.iter().map(|t| t.id.to_string()).collect()
    } else {
        same.iter().map(|t| t.id.to_string()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(method: &str, params: Value) -> Event {
        Event { method: method.into(), params, session_id: Some("S".into()) }
    }

    #[test]
    fn the_user_agent_names_lico_and_the_engine_and_claims_no_chrome() {
        let ua = user_agent("0.7.0", "1.1.25");
        assert_eq!(ua, "Mozilla/5.0 (X11; Linux x86_64) Lico/0.7.0 Moli/1.1.25");
        assert!(!ua.contains("Chrome") && !ua.contains("Windows") && !ua.contains("Safari") && !ua.contains("Obscura"));
        assert!(user_agent("0.7.0", "unpinned").ends_with("Moli/unpinned"));
    }

    const ID: &str = "4f8a1c2e-1111-2222-3333-444455556666";

    fn answer(body: &str) -> String {
        format!("HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\r\n{body}")
    }

    fn page(url: &str) -> String {
        answer(&format!("{{\"Browser\":\"Moli/1.1.25\",\"webSocketDebuggerUrl\":\"{url}\"}}"))
    }

    #[test]
    fn the_socket_path_is_the_one_the_discovery_page_gives_whole_or_in_a_chunk() {
        let want = Some(format!("/devtools/browser/{ID}"));
        assert_eq!(socket_path(&page(&format!("ws://127.0.0.1:41000/devtools/browser/{ID}"))), want);
        assert_eq!(socket_path(&page(&format!("ws://localhost:9/devtools/browser/{ID}"))), want, "the host the page names is not used");
        let body = format!("{{\"webSocketDebuggerUrl\":\"ws://x/devtools/browser/{ID}\"}}");
        let chunked = format!("HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n{:x}\r\n{body}\r\n0\r\n\r\n", body.len());
        assert_eq!(socket_path(&chunked), want);
        assert_eq!(socket_path(&format!("HTTP/1.0 200 OK\r\n\r\n{body}")), want);
    }

    #[test]
    fn anything_else_on_the_discovery_page_is_no_socket() {
        let id = |s: &str| page(&format!("ws://127.0.0.1:1/devtools/browser/{s}"));
        for (what, response) in [
            ("not 200", format!("HTTP/1.1 404 Not Found\r\n\r\n{{\"webSocketDebuggerUrl\":\"ws://h/devtools/browser/{ID}\"}}")),
            ("no end of headers", "HTTP/1.1 200 OK\r\n{}".to_string()),
            ("nothing", String::new()),
            ("no JSON", answer("hello")),
            ("braces the wrong way round", answer("} {")),
            ("no key", answer("{\"Browser\":\"x\"}")),
            ("key is not text", answer("{\"webSocketDebuggerUrl\": 5}")),
            ("https scheme", page(&format!("https://h/devtools/browser/{ID}"))),
            ("wss scheme", page(&format!("wss://h/devtools/browser/{ID}"))),
            ("no path", page("ws://h")),
            ("the fixed path with no id", page("ws://h/devtools/browser")),
            ("an empty id", id("")),
            ("a page socket", page(&format!("ws://h/devtools/page/{ID}"))),
            ("another path", page("ws://h/evil")),
            ("dots in the id", id("..")),
            ("a slash in the id", id("a/b")),
            ("a query", id("a?b=c")),
            ("a percent", id("a%2e")),
            ("a space", id("a b")),
            ("an id that is too long", id(&"a".repeat(65))),
        ] {
            assert_eq!(socket_path(&response), None, "{what}");
        }
        assert_eq!(socket_path(&id(&"a".repeat(64))), Some(format!("/devtools/browser/{}", "a".repeat(64))), "an id of exactly the longest allowed");
    }

    #[test]
    fn every_effect_is_one_call_with_the_parameters_of_the_cdp_builders() {
        let w = |e: Effect| wire_of(&e, (800, 600));
        assert_eq!(w(Effect::Navigate("https://a.test/".into())).method, "Page.navigate");
        assert_eq!(w(Effect::Navigate("https://a.test/".into())).params, json!({"url": "https://a.test/"}));
        assert_eq!(w(Effect::SetViewport { width: 800, height: 600 }).params["width"], json!(800));
        let s = w(Effect::StartScreencast);
        assert_eq!((s.method, s.params["format"].clone(), s.params["maxWidth"].clone(), s.params["everyNthFrame"].clone()), ("Page.startScreencast", json!("png"), json!(800), json!(1)), "PNG only, never larger than the tab");
        assert_eq!(w(Effect::Ack(7)).params, json!({"sessionId": 7}));
        assert_eq!(w(Effect::StopScreencast).method, "Page.stopScreencast");
        assert_eq!(w(Effect::Reload).method, "Page.reload");
        assert_eq!(w(Effect::CaptureScreenshot).params, json!({"format": "png"}));
        assert_eq!(w(Effect::InsertText("안녕".into())).params, json!({"text": "안녕"}));
        let m = w(Effect::Mouse { kind: VMouse::Pressed, x: 1.0, y: 2.0, button: VButton::Left, clicks: 1, delta: 0.0 });
        assert_eq!((m.method, m.params["type"].clone(), m.params["button"].clone()), ("Input.dispatchMouseEvent", json!("mousePressed"), json!("left")));
        let k = w(Effect::Key { kind: VKey::Down, key: "a".into(), code: "KeyA".into(), text: "a".into() });
        assert_eq!((k.method, k.params["type"].clone()), ("Input.dispatchKeyEvent", json!("keyDown")));
        assert!(w(Effect::Ack(1)).timeout < w(Effect::Navigate("https://a.test/".into())).timeout, "a navigation waits longer than an acknowledgement");
    }

    #[test]
    fn base64_is_standard_and_anything_else_is_refused() {
        assert_eq!(base64_decode("").unwrap(), b"");
        assert_eq!(base64_decode("Zg==").unwrap(), b"f");
        assert_eq!(base64_decode("Zm8=").unwrap(), b"fo");
        assert_eq!(base64_decode("Zm9v").unwrap(), b"foo");
        assert_eq!(base64_decode("Zm9vYmFy").unwrap(), b"foobar");
        assert_eq!(base64_decode("+/+/").unwrap(), vec![0xfb, 0xff, 0xbf]);
        for bad in ["Z", "Zg=", "Zg=a", "Zm9v!", "Zm 9v", "Zm9v\n", "Zm9", "-_-_", "Zg==Zg==", "=Zg="] {
            assert_eq!(base64_decode(bad), None, "{bad:?}");
        }
        assert_eq!(base64_decode("Zh=="), None, "padding bits that are not zero are not a canonical encoding");
    }

    #[test]
    fn engine_events_become_model_events() {
        let frame = engine_events(&ev("Page.screencastFrame", json!({"data": "Zm9v", "sessionId": 9})));
        assert_eq!(frame, vec![EngineEvent::Frame { id: 9, png: b"foo".to_vec() }]);
        let bad = engine_events(&ev("Page.screencastFrame", json!({"data": "***", "sessionId": 10})));
        assert_eq!(bad, vec![EngineEvent::Frame { id: 10, png: Vec::new() }], "still acknowledged");
        assert_eq!(engine_events(&ev("Page.frameNavigated", json!({"frame": {"id": "F", "url": "https://a.test/x"}}))), vec![EngineEvent::Navigated("https://a.test/x".into())]);
        assert!(engine_events(&ev("Page.frameNavigated", json!({"frame": {"id": "G", "parentId": "F", "url": "https://ads.test/"}}))).is_empty(), "a subframe is not the page");
        assert_eq!(engine_events(&ev("Page.navigatedWithinDocument", json!({"url": "https://a.test/#top"}))), vec![EngineEvent::Navigated("https://a.test/#top".into())]);
        assert_eq!(engine_events(&ev("Page.frameStartedLoading", json!({}))), vec![EngineEvent::LoadStarted]);
        assert_eq!(engine_events(&ev("Page.loadEventFired", json!({"timestamp": 1}))), vec![EngineEvent::LoadFinished]);
        assert_eq!(engine_events(&ev("Target.targetInfoChanged", json!({"targetInfo": {"title": "T"}}))), vec![EngineEvent::Title("T".into())]);
        assert_eq!(engine_events(&ev("Target.targetCrashed", json!({}))), vec![EngineEvent::Crashed]);
        assert_eq!(engine_events(&ev("Target.targetDestroyed", json!({}))), vec![EngineEvent::TargetGone]);
        assert!(engine_events(&ev("Network.requestWillBeSent", json!({}))).is_empty() && engine_events(&ev("Page.frameNavigated", json!({}))).is_empty());
    }

    #[test]
    fn the_engines_own_refusal_of_a_numeric_address_is_recognised_only_by_its_words() {
        assert!(is_local_refusal("Access to private/internal IP address 127.0.0.1 is blocked"));
        assert!(is_local_refusal("Network error: Network error: Access to localhost domain 'localhost' is not allowed"), "the name localhost is refused by the engine too (measured in P06)");
        assert!(!is_local_refusal("net::ERR_NAME_NOT_RESOLVED") && !is_local_refusal(""));
    }

    #[test]
    fn the_host_of_an_address_is_what_the_guard_names() {
        for (url, host) in [("https://Example.COM/a?b#c", "example.com"), ("http://a.test:8080/", "a.test"), ("http://user:pw@b.test/x", "b.test"), ("http://[::1]:80/", "::1"), ("https://c.test", "c.test"), ("http://d.test?q=1", "d.test"), ("about:blank", ""), ("", "")] {
            assert_eq!(host_of(url), host, "{url}");
        }
    }

    #[test]
    fn a_refusal_goes_to_the_loading_tabs_of_that_host_else_to_every_loading_tab_and_never_to_an_idle_one() {
        let tabs = [
            TabView { id: "b1", loading: true, url: "http://a.test/" },
            TabView { id: "b2", loading: true, url: "http://b.test/" },
            TabView { id: "b3", loading: false, url: "http://a.test/" },
            TabView { id: "b4", loading: true, url: "http://A.test/other" },
        ];
        assert_eq!(refusal_targets(&tabs, "a.test"), ["b1", "b4"], "the loading tabs with that host; b3 is not loading");
        assert_eq!(refusal_targets(&tabs, "A.TEST"), ["b1", "b4"], "the host is compared without case");
        assert_eq!(refusal_targets(&tabs, "cdn.test"), ["b1", "b2", "b4"], "a subresource host nobody navigated to: every loading tab");
        assert!(refusal_targets(&tabs[2..3], "a.test").is_empty());
        assert!(refusal_targets(&[], "a.test").is_empty());
    }
}

#[cfg(test)]
mod encode_tests {
    use super::*;

    #[test]
    fn base64_encode_meets_the_standard_vectors_and_round_trips() {
        for (plain, coded) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy")] {
            assert_eq!(base64_encode(plain.as_bytes()), coded);
        }
        let all: Vec<u8> = (0..=255u8).collect();
        assert_eq!(base64_decode(&base64_encode(&all)).unwrap(), all, "every byte value survives");
        assert_eq!(base64_encode(&[0xfb, 0xff, 0xbf]), "+/+/");
    }
}
