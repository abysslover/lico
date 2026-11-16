//! The first measurement of the browser engine (M17-P03). NOT part of a normal run: it needs an extracted engine and says so.
//!
//!     LICO_ENGINE_DIR=/abs/dir/with/obscura LICO_PROBE_OUT=/abs/empty/out/dir cargo test -p lico-sandbox --test engine_probe -- --ignored --nocapture
//!
//! The engine runs only through `lico-sandbox` (Landlock: files by path, TCP by port; a cleared environment; an empty HOME and working directory), as the child of `strace`
//! when that is installed (the strace is the confined process, so its tracee inherits the confinement), on pages of this test's own fixture server only. No real website.
//! Every figure is printed as a `MEASURE {json}` line; what the engine tried is in `strace.txt` of the output directory.

use lico_cdp::{cmd, method, Client, Event, ImageFormat, Limits};
use lico_sandbox::{confine, Confinement};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn m(name: &str, v: Value) {
    println!("MEASURE {}", json!({"name": name, "v": v}));
}

// ---- the fixture server: static pages of this test's own, on a loopback port ------------------------------------------------------------------------

fn png() -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = lico_png::PngEncoder::new(&mut out, 120, 80).unwrap();
    for y in 0..80u32 {
        let row: Vec<u8> = (0..120u32).flat_map(|x| [(x * 2) as u8, (y * 3) as u8, if (x / 10 + y / 10) % 2 == 0 { 200 } else { 40 }]).collect();
        enc.push_row(&row).unwrap();
    }
    enc.finish().unwrap();
    out
}

const INDEX: &str = r#"<!doctype html><html lang="ko"><head><meta charset="utf-8"><title>Lico fixture</title><link rel="stylesheet" href="/style.css"><script src="/app.js" defer></script></head><body>
<h1 id="h">Lico fixture 안녕하세요 한글 입력 시험</h1>
<p class="box">A box with a border, rounded corners, a gradient and flex children.</p>
<div class="flex"><span class="a">one</span><span class="b">two</span><span class="c">three</span></div>
<img id="pic" src="/pic.png" width="120" height="80" alt="pic">
<form id="f" onsubmit="return false"><input id="name" name="name" value=""><select id="s"><option>a</option><option>b</option></select><button id="btn" type="button">Go</button></form>
<p><a id="link" href="/second.html">the second page</a></p>
<div id="log">no script yet</div><div style="height:2000px"></div></body></html>"#;

const CSS: &str = "body{font-family:sans-serif;margin:16px;background:#fafafa;color:#222}.box{border:3px solid #36c;border-radius:12px;padding:12px;background:linear-gradient(90deg,#cfe,#fec)}.flex{display:flex;gap:8px;margin:12px 0}.flex span{flex:1;padding:8px;background:#ddd}.b{flex:2!important;background:#bcd!important}#name{width:200px;font-size:18px}";
const JS: &str = "window.__log=[];['mousedown','mouseup','click','keydown','keyup','input','wheel','compositionstart','compositionupdate','compositionend'].forEach(function(t){window.addEventListener(t,function(e){window.__log.push(t+(e.key?':'+e.key:'')+(e.data?':'+e.data:''))},true)});document.getElementById('log').textContent='script ran';document.title='scripted';";
const SECOND: &str = r#"<!doctype html><html lang="ko"><head><meta charset="utf-8"><title>second</title></head><body><h1>두 번째 페이지</h1></body></html>"#;
const ANIM: &str = r#"<!doctype html><html><head><meta charset="utf-8"><title>anim</title><style>@keyframes c{0%{background:#f33}50%{background:#33f}100%{background:#f33}}#b{width:400px;height:300px;animation:c 1s infinite}</style></head><body><div id="b"></div><div id="n">0</div><script>var n=0;(function t(){document.getElementById('n').textContent=++n;requestAnimationFrame(t)})()</script></body></html>"#;

struct Fixture {
    port: u16,
    seen: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
}

fn fixture() -> Fixture {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let (seen, stop) = (Arc::new(Mutex::new(Vec::new())), Arc::new(AtomicBool::new(false)));
    let (s2, st2) = (seen.clone(), stop.clone());
    std::thread::spawn(move || {
        while !st2.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((mut c, _)) => {
                    let s3 = s2.clone();
                    std::thread::spawn(move || {
                        let _ = c.set_nonblocking(false);
                        let _ = c.set_read_timeout(Some(Duration::from_secs(3)));
                        let mut buf = [0u8; 4096];
                        let n = c.read(&mut buf).unwrap_or(0);
                        let head = String::from_utf8_lossy(&buf[..n]).to_string();
                        let path = head.split(' ').nth(1).unwrap_or("").to_string();
                        s3.lock().unwrap().push(path.clone());
                        let (ctype, body): (&str, Vec<u8>) = match path.split('?').next().unwrap_or("") {
                            "/" | "/index.html" => ("text/html; charset=utf-8", INDEX.into()),
                            "/style.css" => ("text/css", CSS.into()),
                            "/app.js" => ("application/javascript", JS.into()),
                            "/pic.png" => ("image/png", png()),
                            "/second.html" => ("text/html; charset=utf-8", SECOND.into()),
                            "/anim.html" => ("text/html; charset=utf-8", ANIM.into()),
                            _ => ("text/plain", b"not found".to_vec()),
                        };
                        let status = if ctype == "text/plain" { "404 Not Found" } else { "200 OK" };
                        let _ = c.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).as_bytes());
                        let _ = c.write_all(&body);
                    });
                }
                Err(_) => std::thread::sleep(Duration::from_millis(10)),
            }
        }
    });
    Fixture { port, seen, stop }
}

// ---- small helpers ------------------------------------------------------------------------------------------------------------------------------

fn b64(data: &str) -> Vec<u8> {
    let val = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let (mut out, mut acc, mut bits) = (Vec::new(), 0u32, 0);
    for c in data.bytes() {
        if let Some(v) = val(c) {
            acc = acc << 6 | v as u32;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((acc >> bits) as u8);
            }
        }
    }
    out
}

fn free_port() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
}

fn token() -> String {
    let mut b = [0u8; 24];
    std::fs::File::open("/dev/urandom").unwrap().read_exact(&mut b).unwrap();
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// (pid, parent, name, rss_kib) of every process.
fn processes() -> Vec<(u32, u32, String, u64)> {
    let mut v = Vec::new();
    for e in std::fs::read_dir("/proc").unwrap().flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else { continue };
        let Ok(status) = std::fs::read_to_string(format!("/proc/{pid}/status")) else { continue };
        let field = |k: &str| status.lines().find_map(|l| l.strip_prefix(k)).map(|s| s.trim().to_string());
        let ppid = field("PPid:").and_then(|s| s.parse().ok()).unwrap_or(0);
        let rss = field("VmRSS:").and_then(|s| s.split_whitespace().next().and_then(|n| n.parse().ok())).unwrap_or(0);
        v.push((pid, ppid, field("Name:").unwrap_or_default(), rss));
    }
    v
}

fn descendants(root: u32) -> Vec<(u32, u32, String, u64)> {
    let all = processes();
    let mut keep: Vec<u32> = vec![root];
    loop {
        let more: Vec<u32> = all.iter().filter(|p| keep.contains(&p.1) && !keep.contains(&p.0)).map(|p| p.0).collect();
        if more.is_empty() {
            break;
        }
        keep.extend(more);
    }
    all.into_iter().filter(|p| keep.contains(&p.0)).collect()
}

fn rss(label: &str, root: u32) {
    let d = descendants(root);
    let engine: Vec<Value> = d.iter().filter(|p| p.2 != "strace").map(|p| json!({"pid": p.0, "name": p.2, "rss_mib": p.3 as f64 / 1024.0})).collect();
    let total: u64 = d.iter().filter(|p| p.2 != "strace").map(|p| p.3).sum();
    m("rss", json!({"at": label, "engine_total_mib": total as f64 / 1024.0, "processes": engine}));
}

/// Clock ticks (1/100 s) of user plus system time of the engine processes (not the strace).
fn cpu_ticks(root: u32) -> u64 {
    descendants(root)
        .iter()
        .filter(|p| p.2 != "strace")
        .filter_map(|p| std::fs::read_to_string(format!("/proc/{}/stat", p.0)).ok())
        .filter_map(|t| {
            let rest = t.rsplit_once(") ")?.1.split(' ').map(str::to_string).collect::<Vec<_>>();
            Some(rest.get(11)?.parse::<u64>().ok()? + rest.get(12)?.parse::<u64>().ok()?)
        })
        .sum()
}

fn kill_all(root: u32) {
    for (pid, _, _, _) in descendants(root).into_iter().rev() {
        unsafe extern "C" {
            fn kill(pid: i32, sig: i32) -> i32;
        }
        unsafe { kill(pid as i32, 9) };
    }
}

fn http_get(addr: SocketAddr, path: &str, token: &str) -> (u16, String) {
    let Ok(mut s) = TcpStream::connect_timeout(&addr, Duration::from_secs(2)) else { return (0, "connect failed".into()) };
    s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    let _ = s.write_all(format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {token}\r\nConnection: close\r\n\r\n", addr.port()).as_bytes());
    let mut text = String::new();
    let _ = s.read_to_string(&mut text);
    let status = text.split(' ').nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    (status, text.split("\r\n\r\n").nth(1).unwrap_or("").chars().take(600).collect())
}

// ---- the engine --------------------------------------------------------------------------------------------------------------------------------

struct Engine {
    child: Child,
    port: u16,
    token: String,
}

fn start(engine_dir: &Path, out: &Path, fixture_port: u16, allow_private: bool, with_fonts: bool, tag: &str) -> Engine {
    let port = free_port();
    let token = token();
    let (home, work) = (out.join(format!("home_{tag}")), out.join(format!("work_{tag}")));
    for d in [&home, &work] {
        let _ = std::fs::remove_dir_all(d);
        std::fs::create_dir_all(d).unwrap();
    }
    let mut env = vec![("OBSCURA_CDP_TOKEN".to_string(), token.clone()), ("HOME".to_string(), home.to_string_lossy().into_owned())];
    if allow_private {
        env.push(("OBSCURA_ALLOW_PRIVATE_NETWORK".into(), "1".into())); // for the measurement only: the fixtures are on 127.0.0.1
    }
    let mut read = vec![PathBuf::from("/etc/ld.so.cache"), PathBuf::from("/dev/urandom")];
    let fonts = Path::new("/usr/share/fonts/truetype/nanum");
    let with_fonts = with_fonts && fonts.exists();
    if with_fonts {
        read.push(fonts.to_path_buf());
    }
    let conf = Confinement {
        read_exec: vec![engine_dir.to_path_buf(), "/usr".into(), "/lib".into(), "/lib64".into()],
        read,
        read_write: vec![out.to_path_buf(), "/dev/null".into()],
        connect_ports: vec![fixture_port],
        bind_ports: vec![port],
        env,
        cwd: work,
        ..Confinement::default()
    };
    let strace = Path::new("/usr/bin/strace");
    let font_arg = with_fonts.then(|| fonts.to_string_lossy().into_owned());
    let mut cmd;
    if strace.exists() {
        cmd = Command::new(strace);
        cmd.args(["-f", "-qq", "-s", "96", "-e", "trace=network,openat,open,execve,bind,listen", "-o"]).arg(out.join(format!("strace_{tag}.txt"))).arg("--").arg(engine_dir.join("obscura"));
    } else {
        cmd = Command::new(engine_dir.join("obscura"));
    }
    cmd.arg("serve").arg("--port").arg(port.to_string());
    if let Some(f) = font_arg {
        cmd.arg("--font-dir").arg(f);
    }
    confine(&mut cmd, &conf).expect("the confinement can be made");
    cmd.stdout(Stdio::from(std::fs::File::create(out.join(format!("engine_stdout_{tag}.txt"))).unwrap())).stderr(Stdio::from(std::fs::File::create(out.join(format!("engine_stderr_{tag}.txt"))).unwrap()));
    let child = cmd.spawn().expect("the confined engine starts");
    Engine { child, port, token }
}

fn wait_port(port: u16, within: Duration) -> Option<Duration> {
    let t = Instant::now();
    while t.elapsed() < within {
        if TcpStream::connect_timeout(&SocketAddr::from(([127, 0, 0, 1], port)), Duration::from_millis(200)).is_ok() {
            return Some(t.elapsed());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    None
}

struct Page {
    c: Client,
    session: Option<String>,
}

impl Page {
    /// The result, or `{"error": "..."}` (what is printed either way).
    fn call(&mut self, method: &str, params: Value) -> Value {
        let s = self.session.clone();
        match self.c.call(method, params, s.as_deref(), Duration::from_secs(20)) {
            Ok(v) => v,
            Err(e) => json!({"error": format!("{e}")}),
        }
    }
    fn eval(&mut self, js: &str) -> Value {
        let v = self.call("Runtime.evaluate", json!({"expression": js, "returnByValue": true}));
        if v.get("error").is_some() { v } else { v["result"]["value"].clone() }
    }
    fn events(&mut self, ms: u64) -> Vec<Event> {
        let end = Instant::now() + Duration::from_millis(ms);
        let mut v = Vec::new();
        while Instant::now() < end {
            match self.c.next_event(end.saturating_duration_since(Instant::now()).min(Duration::from_millis(200))) {
                Ok(Some(e)) => v.push(e),
                Ok(None) => {}
                Err(_) => break,
            }
        }
        v
    }
    fn wait_for(&mut self, name: &str, ms: u64) -> Option<Duration> {
        let t = Instant::now();
        while t.elapsed() < Duration::from_millis(ms) {
            if let Ok(Some(e)) = self.c.next_event(Duration::from_millis(100)) {
                if e.method == name {
                    return Some(t.elapsed());
                }
            }
        }
        None
    }
}

fn screencast(page: &mut Page, out: &Path, label: &str, format: ImageFormat, w: u32, h: u32, ack: bool, ms: u64, root: u32) {
    let cpu0 = cpu_ticks(root);
    let started = page.call(method::START_SCREENCAST, cmd::start_screencast(format, 80, w, h, 1));
    let t = Instant::now();
    let (mut n, mut bytes, mut max, mut first_at, mut times) = (0u64, 0usize, 0usize, None, Vec::new());
    let mut keys = Value::Null;
    while t.elapsed() < Duration::from_millis(ms) {
        let Ok(Some(e)) = page.c.next_event(Duration::from_millis(100)) else { continue };
        if e.method != method::SCREENCAST_FRAME {
            continue;
        }
        let data = e.params["data"].as_str().unwrap_or("");
        n += 1;
        bytes += data.len();
        max = max.max(data.len());
        times.push(t.elapsed().as_millis() as u64);
        first_at.get_or_insert(t.elapsed().as_millis() as u64);
        if n == 1 {
            keys = json!({"params": e.params.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()), "metadata": e.params["metadata"]});
            let ext = if format == ImageFormat::Png { "png" } else { "jpg" };
            let _ = std::fs::write(out.join(format!("frame_{label}.{ext}")), b64(data));
        }
        if ack {
            if let Some(id) = e.params["sessionId"].as_u64() {
                let _ = page.call(method::SCREENCAST_ACK, cmd::screencast_ack(id));
            }
        }
    }
    let _ = page.call("Page.stopScreencast", json!({}));
    m("screencast", json!({"label": label, "format": format!("{format:?}"), "size": [w, h], "ack": ack, "ms": ms, "start_result": started, "frames": n, "frame_b64_bytes_total": bytes, "frame_b64_bytes_max": max,
        "cpu_ms": (cpu_ticks(root) - cpu0) * 10, "first_frame_ms": first_at, "frame_times_ms": times.iter().take(40).collect::<Vec<_>>(), "first_frame_shape": keys}));
}

#[test]
#[ignore = "needs an extracted engine: LICO_ENGINE_DIR and LICO_PROBE_OUT (see the head of this file)"]
fn engine_probe() {
    let engine_dir = PathBuf::from(std::env::var("LICO_ENGINE_DIR").expect("LICO_ENGINE_DIR")).canonicalize().unwrap();
    let out = PathBuf::from(std::env::var("LICO_PROBE_OUT").expect("LICO_PROBE_OUT"));
    std::fs::create_dir_all(&out).unwrap();
    let out = out.canonicalize().unwrap();
    assert!(engine_dir.join("obscura").exists() && engine_dir.join("obscura-worker").exists());
    m("env", json!({"landlock_abi": lico_sandbox::abi_version().ok(), "strace": Path::new("/usr/bin/strace").exists()}));
    let fx = fixture();
    let fx_url = format!("http://127.0.0.1:{}", fx.port);

    // ---- run 1: the private-network switch ON (the fixtures are on 127.0.0.1) ----
    let t0 = Instant::now();
    let mut eng = start(&engine_dir, &out, fx.port, true, true, "run1");
    let root = eng.child.id();
    let up = wait_port(eng.port, Duration::from_secs(20));
    m("start", json!({"port_open_ms": up.map(|d| d.as_millis() as u64), "pid_root": root, "waited_since_spawn_ms": t0.elapsed().as_millis() as u64}));
    let Some(_) = up else {
        std::thread::sleep(Duration::from_millis(500));
        m("failed_to_start", json!({"stderr": std::fs::read_to_string(out.join("engine_stderr_run1.txt")).unwrap_or_default().chars().take(1500).collect::<String>(), "stdout": std::fs::read_to_string(out.join("engine_stdout_run1.txt")).unwrap_or_default().chars().take(500).collect::<String>()}));
        kill_all(root);
        let _ = eng.child.wait();
        panic!("the engine did not open its port: see strace_run1.txt");
    };
    let addr = SocketAddr::from(([127, 0, 0, 1], eng.port));
    rss("idle_after_start", root);
    let mut ws_path = "/devtools/browser".to_string();
    for p in ["/json/version", "/json/list", "/json"] {
        let (status, body) = http_get(addr, p, &eng.token);
        m("discovery", json!({"path": p, "status": status, "body": body}));
        if p == "/json/version" {
            if let Some(u) = serde_json::from_str::<Value>(&body).ok().and_then(|v| v["webSocketDebuggerUrl"].as_str().map(str::to_string)) {
                ws_path = u.splitn(4, '/').nth(3).map(|s| format!("/{s}")).unwrap_or(ws_path);
            }
        }
    }
    let (status_no_token, _) = http_get(addr, "/json/version", "wrong-token");
    m("token_check", json!({"wrong_token_status": status_no_token}));
    let limits = Limits { connect: Duration::from_secs(3), handshake: Duration::from_secs(5), idle: Duration::from_secs(30), max_message: 32 << 20 };
    let client = Client::connect(addr, &ws_path, Some(&eng.token), &limits);
    m("connect", json!({"ws_path": ws_path, "ok": client.is_ok(), "err": client.as_ref().err().map(|e| format!("{e}"))}));
    let Ok(client) = client else {
        kill_all(root);
        let _ = eng.child.wait();
        panic!("no CDP connection");
    };
    let mut page = Page { c: client, session: None };
    let version = page.call("Browser.getVersion", json!({}));
    m("browser_version", json!({"result": version}));
    let target = page.call(method::CREATE_TARGET, cmd::create_target("about:blank"));
    m("create_target", json!({"result": target.clone()}));
    if let Some(id) = target["targetId"].as_str() {
        let attached = page.call(method::ATTACH, cmd::attach(id));
        m("attach", json!({"result": attached.clone()}));
        page.session = attached["sessionId"].as_str().map(str::to_string);
    }
    for (name, params) in [("Page.enable", json!({})), ("Runtime.enable", json!({})), (method::SET_VIEWPORT, cmd::set_viewport(1200, 800, 1.0))] {
        m("setup", json!({"method": name, "result": page.call(name, params)}));
    }
    let navigate_start = Instant::now();
    let nav = page.call(method::NAVIGATE, cmd::navigate(&format!("{fx_url}/index.html")));
    let loaded = page.wait_for("Page.loadEventFired", 10_000);
    m("navigate", json!({"result": nav, "load_event_ms": loaded.map(|d| d.as_millis() as u64), "total_ms": navigate_start.elapsed().as_millis() as u64}));
    std::thread::sleep(Duration::from_millis(500));
    rss("after_one_page", root);
    let ident = page.eval("JSON.stringify({ua:navigator.userAgent,webdriver:navigator.webdriver,platform:navigator.platform,language:navigator.language,languages:navigator.languages,tz:Intl.DateTimeFormat().resolvedOptions().timeZone,w:innerWidth,h:innerHeight,dpr:devicePixelRatio,title:document.title,log:document.getElementById('log').textContent,plugins:navigator.plugins.length,cores:navigator.hardwareConcurrency,memory:navigator.deviceMemory,vendor:navigator.vendor,pic:[document.getElementById('pic').naturalWidth,document.getElementById('pic').complete],boxwidth:document.querySelector('.box').getBoundingClientRect().width,flexdisplay:getComputedStyle(document.querySelector('.flex')).display,h1:document.getElementById('h').getBoundingClientRect().height})");
    m("identity_and_dom", json!({"value": ident}));
    let honest = page.call("Emulation.setUserAgentOverride", json!({"userAgent": "Lico/0.5 (Linux; a Lico browser tab)"}));
    m("user_agent_override", json!({"result": honest, "ua_after": page.eval("navigator.userAgent")}));
    for (label, fmt) in [("png", "png"), ("jpeg", "jpeg")] {
        let t = Instant::now();
        let shot = page.call(method::CAPTURE_SCREENSHOT, json!({"format": fmt}));
        let ms = t.elapsed().as_millis() as u64;
        match if shot.get("error").is_some() { Err(shot["error"].clone()) } else { Ok(shot) } {
            Ok(v) => {
                let data = v["data"].as_str().unwrap_or("");
                let bytes = b64(data);
                let _ = std::fs::write(out.join(format!("shot_{label}.{}", if fmt == "png" { "png" } else { "jpg" })), &bytes);
                m("screenshot", json!({"format": fmt, "bytes": bytes.len(), "ms": ms, "keys": v.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>())}));
            }
            Err(e) => m("screenshot", json!({"format": fmt, "error": e, "ms": ms})),
        }
    }
    screencast(&mut page, &out, "still_jpeg_1200_ack", ImageFormat::Jpeg, 1200, 800, true, 3000, root);
    screencast(&mut page, &out, "still_jpeg_1200_noack", ImageFormat::Jpeg, 1200, 800, false, 3000, root);
    screencast(&mut page, &out, "still_png_1200_ack", ImageFormat::Png, 1200, 800, true, 3000, root);
    m("setup", json!({"method": "viewport 640x560", "result": page.call(method::SET_VIEWPORT, cmd::set_viewport(640, 560, 1.0))}));
    screencast(&mut page, &out, "still_jpeg_640_ack", ImageFormat::Jpeg, 640, 560, true, 2000, root);
    rss("while_screencasting_after", root);

    // input on the fixture
    let _ = page.call(method::SET_VIEWPORT, cmd::set_viewport(1200, 800, 1.0));
    let rect = page.eval("(function(){var r=document.getElementById('btn').getBoundingClientRect();return JSON.stringify([r.x+r.width/2,r.y+r.height/2])})()");
    let (bx, by) = serde_json::from_str::<Value>(rect.as_str().unwrap_or("[0,0]")).map(|v| (v[0].as_f64().unwrap_or(0.0), v[1].as_f64().unwrap_or(0.0))).unwrap_or((0.0, 0.0));
    for (k, p) in [("moved", cmd::mouse(lico_cdp::MouseKind::Moved, bx, by, lico_cdp::Button::None, 0, 0.0)), ("pressed", cmd::mouse(lico_cdp::MouseKind::Pressed, bx, by, lico_cdp::Button::Left, 1, 0.0)), ("released", cmd::mouse(lico_cdp::MouseKind::Released, bx, by, lico_cdp::Button::Left, 1, 0.0))] {
        m("input_mouse", json!({"step": k, "result": page.call(method::MOUSE, p)}));
    }
    let _ = page.eval("document.getElementById('name').focus()");
    let key_down = page.call(method::KEY, cmd::key(lico_cdp::KeyKind::Down, "a", "KeyA", "a"));
    let key_up = page.call(method::KEY, cmd::key(lico_cdp::KeyKind::Up, "a", "KeyA", ""));
    m("input_key", json!({"down": key_down, "up": key_up, "value_after": page.eval("document.getElementById('name').value")}));
    let ch = page.call(method::KEY, cmd::key(lico_cdp::KeyKind::Char, "b", "KeyB", "b"));
    m("input_key_char", json!({"result": ch, "value_after": page.eval("document.getElementById('name').value")}));
    let ins = page.call(method::INSERT_TEXT, cmd::insert_text("안녕"));
    m("input_insert_text", json!({"result": ins, "value_after": page.eval("document.getElementById('name').value")}));
    let ime = page.call("Input.imeSetComposition", json!({"text": "ㅎ", "selectionStart": 1, "selectionEnd": 1}));
    m("input_ime_composition", json!({"result": ime, "value_after": page.eval("document.getElementById('name').value")}));
    let wheel = page.call(method::MOUSE, cmd::mouse(lico_cdp::MouseKind::Wheel, 300.0, 300.0, lico_cdp::Button::None, 0, 400.0));
    m("input_wheel", json!({"result": wheel, "scrollY_after": page.eval("window.scrollY")}));
    m("page_event_log", json!({"log": page.eval("JSON.stringify(window.__log)")}));
    let _ = page.eval("window.scrollTo(0,0)");
    let rect = page.eval("(function(){var r=document.getElementById('link').getBoundingClientRect();return JSON.stringify([r.x+r.width/2,r.y+r.height/2])})()");
    let (lx, ly) = serde_json::from_str::<Value>(rect.as_str().unwrap_or("[0,0]")).map(|v| (v[0].as_f64().unwrap_or(0.0), v[1].as_f64().unwrap_or(0.0))).unwrap_or((0.0, 0.0));
    let _ = page.call(method::MOUSE, cmd::mouse(lico_cdp::MouseKind::Pressed, lx, ly, lico_cdp::Button::Left, 1, 0.0));
    let _ = page.call(method::MOUSE, cmd::mouse(lico_cdp::MouseKind::Released, lx, ly, lico_cdp::Button::Left, 1, 0.0));
    let nav_events = page.events(1500);
    m("link_click", json!({"location_after": page.eval("location.href"), "events": nav_events.iter().map(|e| e.method.clone()).collect::<Vec<_>>()}));

    // an animated page, with and without acknowledgements
    let _ = page.call(method::NAVIGATE, cmd::navigate(&format!("{fx_url}/anim.html")));
    let _ = page.wait_for("Page.loadEventFired", 8000);
    screencast(&mut page, &out, "anim_jpeg_1200_ack", ImageFormat::Jpeg, 1200, 800, true, 3000, root);
    screencast(&mut page, &out, "anim_jpeg_1200_noack", ImageFormat::Jpeg, 1200, 800, false, 3000, root);
    screencast(&mut page, &out, "anim_png_1200_ack", ImageFormat::Png, 1200, 800, true, 3000, root);
    let _ = page.call(method::SET_VIEWPORT, cmd::set_viewport(640, 560, 1.0));
    screencast(&mut page, &out, "anim_jpeg_640_ack", ImageFormat::Jpeg, 640, 560, true, 3000, root);
    m("identity_after_ua_override", json!({"value": page.eval("JSON.stringify({ua:navigator.userAgent,platform:navigator.platform,vendor:navigator.vendor,webdriver:navigator.webdriver})")}));
    rss("after_animation", root);
    let closed = page.call("Target.closeTarget", json!({"targetId": target["targetId"].as_str().unwrap_or("")}));
    m("close_target", json!({"result": closed}));
    std::thread::sleep(Duration::from_millis(800));
    rss("after_close_target", root);
    let _ = page.c.call("Browser.close", json!({}), None, Duration::from_secs(2));
    drop(page);
    std::thread::sleep(Duration::from_millis(500));
    m("fixture_requests_run1", json!({"paths": fx.seen.lock().unwrap().clone()}));
    kill_all(root);
    let _ = eng.child.wait();

    // ---- run 2: the private-network switch OFF: the block must refuse the fixture ----
    fx.seen.lock().unwrap().clear();
    let mut eng = start(&engine_dir, &out, fx.port, false, true, "run2");
    let root = eng.child.id();
    if wait_port(eng.port, Duration::from_secs(20)).is_some() {
        let addr = SocketAddr::from(([127, 0, 0, 1], eng.port));
        let c = Client::connect(addr, &ws_path, Some(&eng.token), &limits).expect("connect run2");
        let mut page = Page { c, session: None };
        let target = page.call(method::CREATE_TARGET, cmd::create_target("about:blank"));
        if let Some(id) = target["targetId"].as_str().map(str::to_string) {
            page.session = page.call(method::ATTACH, cmd::attach(&id))["sessionId"].as_str().map(str::to_string);
            let _ = page.call("Page.enable", json!({}));
            let nav = page.call(method::NAVIGATE, cmd::navigate(&format!("{fx_url}/second.html")));
            let loaded = page.wait_for("Page.loadEventFired", 6000);
            m("private_network_block", json!({"navigate_result": nav, "load_event_ms": loaded.map(|d| d.as_millis() as u64), "body_text": page.eval("document.body ? document.body.innerText : null"), "fixture_requests": fx.seen.lock().unwrap().clone()}));
        }
    } else {
        m("run2_did_not_start", json!({}));
    }
    kill_all(root);
    let _ = eng.child.wait();
    // ---- run 3: no font directory (private-network switch ON again): does Korean text paint with what the machine has, from the allowed files only? ----
    let mut eng = start(&engine_dir, &out, fx.port, true, false, "run3");
    let root = eng.child.id();
    if wait_port(eng.port, Duration::from_secs(20)).is_some() {
        let addr = SocketAddr::from(([127, 0, 0, 1], eng.port));
        let c = Client::connect(addr, &ws_path, Some(&eng.token), &limits).expect("connect run3");
        let mut page = Page { c, session: None };
        let target = page.call(method::CREATE_TARGET, cmd::create_target("about:blank"));
        if let Some(id) = target["targetId"].as_str().map(str::to_string) {
            page.session = page.call(method::ATTACH, cmd::attach(&id))["sessionId"].as_str().map(str::to_string);
            let _ = page.call("Page.enable", json!({}));
            let _ = page.call(method::SET_VIEWPORT, cmd::set_viewport(1200, 800, 1.0));
            let _ = page.call(method::NAVIGATE, cmd::navigate(&format!("{fx_url}/index.html")));
            let _ = page.wait_for("Page.loadEventFired", 8000);
            std::thread::sleep(Duration::from_millis(500));
            let shot = page.call(method::CAPTURE_SCREENSHOT, json!({"format": "png"}));
            if let Some(d) = shot["data"].as_str() {
                let _ = std::fs::write(out.join("shot_no_font_dir.png"), b64(d));
            }
            m("no_font_dir", json!({"shot_bytes": shot["data"].as_str().map(|d| b64(d).len()), "h1_height": page.eval("document.getElementById('h').getBoundingClientRect().height")}));
        }
    }
    kill_all(root);
    let _ = eng.child.wait();
    fx.stop.store(true, Ordering::SeqCst);
    let leftovers: Vec<u32> = processes().into_iter().filter(|p| p.2.starts_with("obscura") || p.2 == "strace").map(|p| p.0).collect();
    m("processes_left_by_name", json!({"pids": leftovers}));
    let _ = HashMap::<u8, u8>::new();
}
