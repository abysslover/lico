//! The ignored probe of the whole chain with the REAL engine (M17-P06; MEASURED, not a test of a rule). It needs an extracted release of the engine (`LICO_REAL_ENGINE_DIR`, the
//! two files of the pinned release: the host checks their hash, so nothing else runs), the real `lico` binary (`LICO_BIN`) and an empty output directory (`LICO_PROBE_OUT`):
//!
//!     LICO_BIN=... LICO_REAL_ENGINE_DIR=/abs/dir LICO_PROBE_OUT=/abs/empty/dir cargo test --release -p lico --bin lico browser::real_probe -- --ignored --nocapture
//!
//! A fixture server on loopback is the only origin (no real site, no real name lookup: the name `localhost` is answered by the guard's own resolver). What the probe records is
//! in `LICO_PROBE_OUT/p06_probe.txt`. Nothing secret is printed: the engine's token is read from its own process only to ask its discovery page, and is never written.

use super::system_tests::Rig;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const INPUT_PAGE: &str = "<html><head><meta charset='utf-8'><title>input</title></head><body style='margin:0'><input id='f' style='position:absolute;left:10px;top:10px;width:300px;height:40px;font-size:24px' autofocus><script>var n=0;document.getElementById('f').addEventListener('input',function(e){fetch('/v?x='+encodeURIComponent(e.target.value))});document.addEventListener('mousemove',function(e){fetch('/m?x='+e.clientX)});document.addEventListener('wheel',function(e){fetch('/w?d='+e.deltaY)});document.addEventListener('mousedown',function(e){fetch('/d?x='+e.clientX+'&y='+e.clientY)});</script></body></html>";
const PAGE: &str = "<html><head><title>Lico probe page</title></head><body><h1>안녕하세요 Lico</h1><p>a page of the fixture</p></body></html>";
// allocates far more than the data limit if it can
const HOG: &str = "<html><head><title>before</title></head><body><script>var a=[];try{for(var i=0;i<400;i++){a.push(new Array(4000000).fill(i));}}catch(e){}document.title='allocated '+a.length;</script></body></html>";

fn fixture() -> (u16, Arc<Mutex<Vec<String>>>, Arc<Mutex<Vec<(u128, String)>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let stamped: Arc<Mutex<Vec<(u128, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let (s2, t2) = (seen.clone(), stamped.clone());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let (seen, stamped) = (s2.clone(), t2.clone());
            std::thread::spawn(move || {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                let head = String::from_utf8_lossy(&buf[..n]).to_string();
                let path = head.split_whitespace().nth(1).unwrap_or("").to_string();
                stamped.lock().unwrap().push((std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0), path.clone()));
                seen.lock().unwrap().push(path.clone());
                let body = if path.starts_with("/hog") { HOG } else if path.starts_with("/input") { INPUT_PAGE } else { PAGE };
                let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            });
        }
    });
    (port, seen, stamped)
}

fn clock_ticks_of(pid: u32) -> u64 {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
    let after = stat.rsplit_once(')').map(|(_, r)| r.to_string()).unwrap_or_default();
    let f: Vec<&str> = after.split_whitespace().collect();
    f.get(11).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0) + f.get(12).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0)
}

fn rss_kib(pid: u32) -> u64 {
    std::fs::read_to_string(format!("/proc/{pid}/status")).unwrap_or_default().lines().find_map(|l| l.strip_prefix("VmRSS:")).and_then(|v| v.split_whitespace().next()?.parse().ok()).unwrap_or(0)
}

/// Voluntary context switches summed over every thread: each wake-up of a sleeping thread is one.
fn wakeups(pid: u32) -> u64 {
    std::fs::read_dir(format!("/proc/{pid}/task")).map(|r| r.filter_map(|e| e.ok()).map(|e| std::fs::read_to_string(e.path().join("status")).unwrap_or_default().lines().find_map(|l| l.strip_prefix("voluntary_ctxt_switches:")).and_then(|v| v.trim().parse::<u64>().ok()).unwrap_or(0)).sum()).unwrap_or(0)
}

fn limit_of(pid: u32, name: &str) -> String {
    std::fs::read_to_string(format!("/proc/{pid}/limits")).unwrap_or_default().lines().find(|l| l.starts_with(name)).unwrap_or("").split_whitespace().skip(3).take(1).collect()
}

fn descriptors(pid: u32) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(format!("/proc/{pid}/fd")).map(|r| r.filter_map(|e| e.ok()).map(|e| format!("{}:{}", e.file_name().to_string_lossy(), std::fs::read_link(e.path()).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default())).collect()).unwrap_or_default();
    v.sort();
    v
}

/// The discovery page of the engine, asked with the token that only the engine's process and the host have (read from the process, never printed).
fn discovery(pid: u32) -> String {
    let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
    let args: Vec<String> = cmdline.split(|b| *b == 0).map(|a| String::from_utf8_lossy(a).into_owned()).collect();
    let port = args.windows(2).find(|w| w[0] == "--port").map(|w| w[1].clone()).unwrap_or_default();
    let environ = std::fs::read(format!("/proc/{pid}/environ")).unwrap_or_default();
    let token = environ.split(|b| *b == 0).filter_map(|e| std::str::from_utf8(e).ok()?.strip_prefix("OBSCURA_CDP_TOKEN=").map(str::to_string)).next().unwrap_or_default();
    let Ok(mut s) = std::net::TcpStream::connect(format!("127.0.0.1:{port}")) else { return "no connection".into() };
    let _ = s.set_read_timeout(Some(Duration::from_secs(3)));
    let _ = write!(s, "GET /json/version HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nConnection: close\r\n\r\n");
    let mut text = String::new();
    let _ = s.read_to_string(&mut text);
    text.split("\r\n\r\n").nth(1).unwrap_or("").chars().take(400).collect()
}

fn note(out: &mut Vec<String>, line: String) {
    println!("{line}");
    out.push(line);
}

#[test]
#[ignore = "needs the real engine: LICO_BIN, LICO_REAL_ENGINE_DIR and LICO_PROBE_OUT (see the head of this file)"]
fn real_engine_probe() {
    let engine_dir = PathBuf::from(std::env::var("LICO_REAL_ENGINE_DIR").expect("LICO_REAL_ENGINE_DIR"));
    let out_dir = PathBuf::from(std::env::var("LICO_PROBE_OUT").expect("LICO_PROBE_OUT"));
    let mut out: Vec<String> = Vec::new();
    let (port, seen, stamped) = fixture();
    let rig = Rig::start_with("real", Some(&engine_dir));
    let host = rig.child.id();
    std::thread::sleep(Duration::from_millis(500));
    let (rss_idle, cpu_idle0, wake_idle0) = (rss_kib(host), clock_ticks_of(host), wakeups(host));
    std::thread::sleep(Duration::from_secs(5));
    note(&mut out, format!("host with no tab: rss {rss_idle} KiB, threads {}, wake-ups in 5 s {}, cpu ticks in 5 s {}", rig.threads(), wakeups(host) - wake_idle0, clock_ticks_of(host) - cpu_idle0));

    // the pinned engine, hash-checked, opens a page of the fixture through the guard (the opt-in tab: loopback is allowed)
    let started = Instant::now();
    let reply = rig.call(json!({"op": "browser.open", "url": format!("http://localhost:{port}/"), "local": true}));
    note(&mut out, format!("browser.open reply ok={} phase={} unpinned={}", reply["ok"], reply["phase"], reply["unpinned"]));
    let tab = reply["tab"].as_str().unwrap_or("b1").to_string();
    let ready = rig.wait(30, "the first page of the real engine", || Some(rig.tab(&tab)).filter(|t| t["has_frame"] == json!(true) && t["phase"] == json!("ready")));
    note(&mut out, format!("first frame after {} ms; title {:?}; viewport {}; notice {}", started.elapsed().as_millis(), ready["title"], ready["viewport"], ready["notice"]));
    let engines = rig.engines();
    note(&mut out, format!("engine processes: {}", engines.len()));
    let pid = engines[0] as u32;
    let shot = out_dir.join("probe_shot.png");
    let s = rig.call(json!({"op": "browser.shot", "tab": tab, "path": shot.to_str().unwrap()}));
    note(&mut out, format!("browser.shot ok={} {}x{} {} bytes", s["ok"], s["width"], s["height"], std::fs::metadata(&shot).map(|m| m.len()).unwrap_or(0)));
    let listed = rig.call(json!({"op": "browser.list"}));
    note(&mut out, format!("engine says (Browser.getVersion): {}; guard accepted {} refused {}", listed["engines"][1]["reported"], listed["engines"][1]["guard_accepted"], listed["engines"][1]["guard_refused"]));
    note(&mut out, format!("discovery /json/version: {}", discovery(pid)));
    let fds = descriptors(pid);
    note(&mut out, format!("engine descriptors above 2 ({}): {}", fds.iter().filter(|d| !d.starts_with(['0', '1', '2']) || d.len() > 2 && !d[..2].starts_with(':')).count(), fds.join(" ")));
    assert!(!fds.iter().any(|d| d.contains("obscura") || d.contains("host.sock")), "the file that was run and the host's socket are not open in the engine: {fds:?}");
    note(&mut out, format!("limits of the engine: data {} open files {}", limit_of(pid, "Max data size"), limit_of(pid, "Max open files")));
    note(&mut out, format!("host with one engine and one tab: rss {} KiB, threads {}; engine rss {} KiB", rss_kib(host), rig.threads(), rss_kib(pid)));

    // CPU and wake-ups over 60 s with one still page shown
    let (c0, e0, w0, ew0, t0) = (clock_ticks_of(host), clock_ticks_of(pid), wakeups(host), wakeups(pid), Instant::now());
    std::thread::sleep(Duration::from_secs(60));
    let secs = t0.elapsed().as_secs_f64();
    note(&mut out, format!("60 s with a still page: host cpu {:.2} s, engine cpu {:.2} s, host wake-ups {:.1}/s, engine wake-ups {:.1}/s", (clock_ticks_of(host) - c0) as f64 / 100.0, (clock_ticks_of(pid) - e0) as f64 / 100.0, (wakeups(host) - w0) as f64 / secs, (wakeups(pid) - ew0) as f64 / secs));

    // the same name without the opt-in: the fixture must see nothing new, the tab says why
    let before = seen.lock().unwrap().len();
    let plain = rig.open(&format!("http://localhost:{port}/plain"));
    std::thread::sleep(Duration::from_secs(8));
    let t = rig.tab(&plain);
    note(&mut out, format!("ordinary tab after 8 s: {t}"));
    note(&mut out, format!("without the opt-in: fixture saw {} new requests; notice {}", seen.lock().unwrap().len() - before, t["notice"]));
    // a numeric loopback address with the opt-in is refused by the engine before the guard
    let numeric = rig.call(json!({"op": "browser.open", "url": format!("http://127.0.0.1:{port}/num"), "local": true}));
    std::thread::sleep(Duration::from_secs(8));
    let n = rig.tab(numeric["tab"].as_str().unwrap());
    note(&mut out, format!("numeric tab after 8 s: {n}"));
    note(&mut out, format!("numeric loopback address with the opt-in: fixture paths {:?}; notice {}", seen.lock().unwrap(), n["notice"]));
    note(&mut out, format!("engine processes with an ordinary and an opted-in tab: {} (two engines: one per network class)", rig.engines().len()));

    // the data limit: a page that allocates far more than it
    let hog_open = rig.call(json!({"op": "browser.open", "url": format!("http://localhost:{port}/hog"), "local": true}));
    let hog_tab = hog_open["tab"].as_str().unwrap().to_string();
    // in the approved configuration the engine refuses `localhost` before it fetches, so the page never runs: the verdict is what the tab shows after 15 s
    std::thread::sleep(Duration::from_secs(15));
    let verdict = rig.tab(&hog_tab);
    note(&mut out, format!("allocating page (limit 4 GiB data): tab phase {} title {:?} reason {}; engine processes now {}; host alive {}", verdict["phase"], verdict["title"], verdict["reason"], rig.engines().len(), std::fs::metadata(format!("/proc/{host}")).is_ok()));
    // after a death the next action starts the engine again
    let again = rig.open(&format!("http://localhost:{port}/after"));
    let ok = rig.wait(30, "a page after the allocating one", || Some(rig.tab(&again)).filter(|t| t["has_frame"] == json!(true)));
    note(&mut out, format!("a page after it: phase {} frames {}", ok["phase"], ok["has_frame"]));

    // input with the REAL engine, seen by the page itself: a click at the input, Korean text, an editing key, a wheel step, and how long a mouse move takes to show up as the page's own
    // request at the fixture (the command's way into the engine, the page's handler and the guard: an UPPER bound of the time a command takes)
    let input_open = rig.call(json!({"op": "browser.open", "url": format!("http://localhost:{port}/input"), "local": true}));
    let it = input_open["tab"].as_str().unwrap_or("b1").to_string();
    let end = Instant::now() + Duration::from_secs(30);
    while Instant::now() < end && !(rig.tab(&it)["has_frame"] == json!(true) && rig.tab(&it)["phase"] == json!("ready")) {
        std::thread::sleep(Duration::from_millis(100));
    }
    note(&mut out, format!("the input page tab after waiting: {} (open reply {})", rig.tab(&it), input_open));
    std::thread::sleep(Duration::from_millis(500));
    let ask = |v: Value| {
        let mut v = v;
        v["op"] = json!("browser.input");
        v["tab"] = json!(it);
        rig.call(v)
    };
    let v1 = ask(json!({"kind": "down", "x": 50, "y": 30, "button": "left", "clicks": 1}));
    let v2 = ask(json!({"kind": "up", "x": 50, "y": 30, "button": "left", "clicks": 1}));
    std::thread::sleep(Duration::from_millis(600));
    note(&mut out, format!("a click: sent {} and {}; the page saw a mousedown at {:?}", v1["sent"], v2["sent"], stamped.lock().unwrap().iter().find(|e| e.1.starts_with("/d?")).map(|e| e.1.clone())));
    let t = ask(json!({"kind": "text", "text": "안녕하세요"}));
    std::thread::sleep(Duration::from_millis(800));
    let values: Vec<String> = stamped.lock().unwrap().iter().filter(|e| e.1.starts_with("/v?x=")).map(|e| e.1.clone()).collect();
    note(&mut out, format!("committed Korean text: sent {}; the page's input events after it: {} (the last: {:?})", t["sent"], values.len(), values.last()));
    let _ = ask(json!({"kind": "key_down", "key": "Backspace", "code": "Backspace"}));
    let _ = ask(json!({"kind": "key_up", "key": "Backspace", "code": "Backspace"}));
    std::thread::sleep(Duration::from_millis(500));
    let after: Vec<String> = stamped.lock().unwrap().iter().filter(|e| e.1.starts_with("/v?x=")).map(|e| e.1.clone()).collect();
    note(&mut out, format!("a Backspace key: the page's input events {} -> {} (the last: {:?})", values.len(), after.len(), after.last()));
    let _ = ask(json!({"kind": "wheel", "x": 100, "y": 100, "delta": 120.0}));
    std::thread::sleep(Duration::from_millis(500));
    note(&mut out, format!("a wheel step: the page saw {:?}", stamped.lock().unwrap().iter().find(|e| e.1.starts_with("/w?")).map(|e| e.1.clone())));
    let mut lags: Vec<f64> = Vec::new();
    for i in 0..50 {
        let sent_ns = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let _ = ask(json!({"kind": "move", "x": 200 + i, "y": 100}));
        std::thread::sleep(Duration::from_millis(60));
        if let Some(e) = stamped.lock().unwrap().iter().find(|e| e.1 == format!("/m?x={}", 200 + i)) {
            lags.push((e.0 as f64 - sent_ns as f64) / 1e6);
        }
    }
    lags.sort_by(|a, b| a.partial_cmp(b).unwrap());
    if !lags.is_empty() {
        note(&mut out, format!("a mouse move to the page's own request at the fixture, {} of 50 seen: p50 {:.1} ms, p95 {:.1} ms, worst {:.1} ms", lags.len(), lags[lags.len() / 2], lags[lags.len() * 95 / 100], lags[lags.len() - 1]));
    } else {
        note(&mut out, "no mouse move was seen by the page".to_string());
    }

    // end: close everything, nothing is left
    for t in rig.tabs() {
        let _ = rig.call(json!({"op": "browser.close", "tab": t["tab"]}));
    }
    std::thread::sleep(Duration::from_millis(500));
    note(&mut out, format!("after closing every tab: engine processes {}, scratch directories {}, host rss {} KiB, threads {}", rig.engines().len(), rig.scratch_dirs(), rss_kib(host), rig.threads()));
    let log = std::fs::read_to_string(rig.dir.join("host.err")).unwrap_or_default();
    note(&mut out, format!("host log lines of the browser: {}", log.lines().filter(|l| l.starts_with("lico-browser:")).collect::<Vec<_>>().join(" | ")));
    std::fs::write(out_dir.join("p06_probe.txt"), out.join("\n")).unwrap();
    let _: &Path = &out_dir;
    let _: Value = Value::Null;
}
