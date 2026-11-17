//! A test double for the browser engine, in the shape of Moli (M17-P06, carried to Moli in M20): `lico-fake-engine serve --host 127.0.0.1 --port N [--layout] [--resource]
//! [--screencast-interval-ms N] [--ca-cert PATH] [--log-level L]`. Like Moli it needs a writable `TMPDIR` and a readable `FONTCONFIG_FILE`, takes its proxy from `HTTP_PROXY`
//! (credentials in the URL), has NO token of its own, listens on the loopback port it was given and answers the CDP calls a tab makes at a socket path made at each start
//! (`/devtools/browser/<id>`, told by `GET /json/version`; an upgrade anywhere else is a 404). A page is a fake frame (a PNG of a shade that changes with each navigation);
//! an `http://` address is fetched once through the proxy, so that the host's fetch guard sees what a real engine would send. `Browser.close` ends the process. It prints
//! nothing; it writes only the logs the tests read, in its `HOME`. It exists only for the system tests of the host adapter.

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

/// What a real engine does with `HTTP_PROXY` for a plain `http://` page: one absolute-form request with the credentials of the proxy URL; the answer is ignored (a refused
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

/// What Moli needs of its environment, and exits 3 when it is missing: a writable `TMPDIR` and a readable `FONTCONFIG_FILE` (the host writes both inside the fence).
fn check_environment() {
    let tmp = std::env::var("TMPDIR").unwrap_or_else(|_| std::process::exit(3));
    let probe = format!("{tmp}/fake-engine-{}", std::process::id());
    if std::fs::write(&probe, b"x").is_err() {
        std::process::exit(3);
    }
    let _ = std::fs::remove_file(&probe);
    let fonts = std::env::var("FONTCONFIG_FILE").unwrap_or_else(|_| std::process::exit(3));
    if std::fs::read(&fonts).is_err() {
        std::process::exit(3);
    }
}

/// An id for the socket path, made at each start (letters, digits and dashes, like the real one).
fn start_id() -> String {
    let ns = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(1);
    format!("{:08x}-{:04x}-{:012x}", (ns >> 32) as u32, std::process::id() & 0xffff, ns & 0xffff_ffff_ffff)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let host_ok = args.windows(2).any(|w| w[0] == "--host" && w[1] == "127.0.0.1");
    let port: u16 = args.windows(2).find(|w| w[0] == "--port").and_then(|w| w[1].parse().ok()).filter(|p| *p != 0).unwrap_or_else(|| {
        eprintln!("usage: lico-fake-engine serve --host 127.0.0.1 --port N");
        std::process::exit(2)
    });
    if args.first().map(String::as_str) != Some("serve") || !host_ok {
        eprintln!("usage: lico-fake-engine serve --host 127.0.0.1 --port N");
        std::process::exit(2);
    }
    check_environment();
    let proxy = std::env::var("HTTP_PROXY").ok();
    let id = start_id();
    let socket_path = format!("/devtools/browser/{id}");
    let counter = std::sync::Arc::new(AtomicU64::new(1));
    let (c1, c2, c3, c4) = (counter.clone(), counter.clone(), counter.clone(), counter.clone());
    let targets = AtomicU64::new(1);
    let home = std::env::var("HOME").ok();
    // what the host started it with, for the tests of the arguments: one line of arguments in its scratch folder
    if let Some(h) = &home {
        let _ = std::fs::write(format!("{h}/argv.log"), format!("{}\n", args.join(" ")));
    }
    let script = Script::new()
        .socket_path(&socket_path)
        .observe(move |call: &Call| {
            // what the person did reaches the engine: one line each, with the time it arrived (nanoseconds since the epoch), for the tests that measure how long it took
            if call.method == "Emulation.setUserAgentOverride" {
                if let Some(h) = &home {
                    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(format!("{h}/ua.log")) {
                        let _ = writeln!(f, "{}", call.params);
                    }
                }
            }
            if call.method.starts_with("Input.") {
                if let Some(h) = &home {
                    let ns = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
                    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(format!("{h}/input.log")) {
                        let _ = writeln!(f, "{ns} {} {}", call.method, call.params);
                    }
                }
            }
        })
        .idle(Duration::from_secs(3600))
        .http("/json/version", 200, &json!({"Browser": "Chrome/145.0.0.0", "webSocketDebuggerUrl": format!("ws://127.0.0.1:{port}{socket_path}")}).to_string())
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
            let session = call.session_id.clone().unwrap_or_else(|| "S".into());
            // an address with /animate paints twenty frames at once, as an animated page does: the host keeps the newest and counts the rest
            let frames = if url.contains("/animate") { 20 } else { 1 };
            let mut answer = Answer::result(json!({"frameId": "F1", "loaderId": "L1"}))
                .event_in(&session, "Page.frameNavigated", json!({"frame": {"id": "F1", "url": url}}))
                .event_in(&session, "Page.loadEventFired", json!({"timestamp": 1}));
            for _ in 0..frames {
                let n = c3.fetch_add(1, Ordering::SeqCst);
                answer = answer.event_in(&session, "Page.screencastFrame", screencast(n, (n * 40 % 256) as u8));
            }
            answer
        })
        .on_fn("Page.reload", move |call: &Call| {
            let n = c4.fetch_add(1, Ordering::SeqCst);
            let session = call.session_id.clone().unwrap_or_else(|| "S".into());
            Answer::result(json!({})).event_in(&session, "Page.loadEventFired", json!({"timestamp": 2})).event_in(&session, "Page.screencastFrame", screencast(n, (n * 40 % 256) as u8))
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
