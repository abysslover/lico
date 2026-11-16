//! A test double for the browser engine (M17-P06): `lico-fake-engine serve --port N [--font-dir D] [--proxy URL]`. It needs `OBSCURA_CDP_TOKEN` (the bearer token it requires), uses
//! `OBSCURA_PROXY` (credentials in the URL) like the real engine does, listens on the loopback port it was given and answers the CDP calls a tab makes. A page is a fake frame
//! (a PNG of a shade that changes with each navigation); an `http://` address is fetched once through the proxy, so that the host's fetch guard sees what a real engine would
//! send. `Browser.close` ends the process. It prints nothing and reads no file. It exists only for the system tests of the host adapter.

use lico_cdp_fake::{Answer, Call, Fake, Script};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;


fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// A 96 x 64 PNG of one colour (`shade`), made with the project's own encoder.
fn frame_png(shade: u8) -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = lico_png::PngEncoder::new(&mut out, 96, 64).expect("an encoder");
    let row: Vec<u8> = (0..96).flat_map(|_| [shade, 200u8.wrapping_sub(shade), 90]).collect();
    for _ in 0..64 {
        enc.push_row(&row).expect("a row");
    }
    enc.finish().expect("a png");
    out
}

fn screencast(n: u64, shade: u8) -> Value {
    lico_cdp_fake::screencast_frame(n, &base64(&frame_png(shade)), 96, 64)
}

/// What a real engine does with `OBSCURA_PROXY` for a plain `http://` page: one absolute-form request with the credentials of the proxy URL; the answer is ignored (a refused
/// fetch is a blank page, no error: measured).
fn fetch_through_proxy(proxy: &str, url: &str) {
    let Some(rest) = proxy.strip_prefix("http://") else { return };
    let Some((credentials, addr)) = rest.split_once('@') else { return };
    let Ok(mut s) = TcpStream::connect(addr) else { return };
    let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
    let host = url.trim_start_matches("http://").split(['/', ':']).next().unwrap_or("");
    let request = format!("GET {url} HTTP/1.1\r\nHost: {host}\r\nProxy-Authorization: Basic {}\r\nConnection: close\r\n\r\n", base64(credentials.as_bytes()));
    let _ = s.write_all(request.as_bytes());
    let mut sink = Vec::new();
    let _ = s.take(64 * 1024).read_to_end(&mut sink);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let port: u16 = args.windows(2).find(|w| w[0] == "--port").and_then(|w| w[1].parse().ok()).filter(|p| *p != 0).unwrap_or_else(|| {
        eprintln!("usage: lico-fake-engine serve --port N");
        std::process::exit(2)
    });
    let token = std::env::var("OBSCURA_CDP_TOKEN").unwrap_or_else(|_| std::process::exit(3));
    let proxy = std::env::var("OBSCURA_PROXY").ok();
    let counter = std::sync::Arc::new(AtomicU64::new(1));
    let (c1, c2, c3) = (counter.clone(), counter.clone(), counter.clone());
    let targets = AtomicU64::new(1);
    let script = Script::new()
        .token(&token)
        .idle(Duration::from_secs(3600))
        .http("/json/version", 200, &json!({"Browser": "Chrome/145.0.0.0", "webSocketDebuggerUrl": format!("ws://127.0.0.1:{port}/devtools/browser")}).to_string())
        .on("Browser.getVersion", Answer::result(json!({"product": "Chrome/145.0.0.0", "userAgent": "fake"})))
        .on_fn("Target.createTarget", move |_| Answer::result(json!({"targetId": format!("page-{}", targets.fetch_add(1, Ordering::SeqCst))})))
        .on_fn("Target.attachToTarget", |call: &Call| Answer::result(json!({"sessionId": format!("S-{}", call.params["targetId"].as_str().unwrap_or("page-0"))})))
        .on("Target.closeTarget", Answer::result(json!({"success": true})))
        .on("Page.enable", Answer::result(json!({})))
        .on("Runtime.enable", Answer::result(json!({})))
        .on("Emulation.setDeviceMetricsOverride", Answer::result(json!({})))
        .on("Emulation.setUserAgentOverride", Answer::result(json!({})))
        .on("Page.stopScreencast", Answer::result(json!({})))
        .on("Page.screencastFrameAck", Answer::result(json!({})))
        .on("Input.dispatchMouseEvent", Answer::result(json!({})))
        .on("Input.dispatchKeyEvent", Answer::result(json!({})))
        .on("Input.insertText", Answer::result(json!({})))
        .on("Runtime.evaluate", Answer::result(json!({"result": {"type": "string", "value": "A fake page"}})))
        .on_fn("Page.startScreencast", move |call: &Call| {
            let n = c1.fetch_add(1, Ordering::SeqCst);
            Answer::result(json!({})).event_in(call.session_id.as_deref().unwrap_or("S"), "Page.screencastFrame", screencast(n, (n * 40 % 256) as u8))
        })
        .on_fn("Page.captureScreenshot", move |_| {
            let n = c2.fetch_add(1, Ordering::SeqCst);
            Answer::result(json!({"data": base64(&frame_png((n * 40 % 256) as u8))}))
        })
        .on_fn("Page.navigate", move |call: &Call| {
            let url = call.params["url"].as_str().unwrap_or("").to_string();
            // like the real engine, the page's fetch is part of the navigation: it is done before the navigation is answered and the load event fires
            if let (Some(p), true) = (proxy.as_deref(), url.starts_with("http://")) {
                fetch_through_proxy(p, &url);
            }
            let n = c3.fetch_add(1, Ordering::SeqCst);
            let session = call.session_id.clone().unwrap_or_else(|| "S".into());
            Answer::result(json!({"frameId": "F1", "loaderId": "L1"}))
                .event_in(&session, "Page.frameNavigated", json!({"frame": {"id": "F1", "url": url}}))
                .event_in(&session, "Page.loadEventFired", json!({"timestamp": 1}))
                .event_in(&session, "Page.screencastFrame", screencast(n, (n * 40 % 256) as u8))
        })
        .on_fn("Browser.close", |_| {
            std::thread::spawn(|| {
                std::thread::sleep(Duration::from_millis(100));
                std::process::exit(0)
            });
            Answer::result(json!({}))
        });
    let _fake = match Fake::start_at(std::net::SocketAddr::from(([127, 0, 0, 1], port)), script) {
        Ok(f) => f,
        Err(_) => std::process::exit(4),
    };
    loop {
        std::thread::sleep(Duration::from_secs(3600));
    }
}
