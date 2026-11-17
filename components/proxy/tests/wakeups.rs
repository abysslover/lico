//! The guard wakes for a connection and for nothing else (0.4.0): its own process, so that no other test's threads add to the count. A blocking accept and a stop that wakes it.

use lico_proxy::{Config, Connector, Proxy, Resolver, StdConnector, StdResolver};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Voluntary context switches summed over every thread of this process: each wake-up of a sleeping thread is one.
fn wakeups() -> u64 {
    std::fs::read_dir("/proc/self/task")
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| std::fs::read_to_string(e.path().join("status")).unwrap_or_default().lines().find_map(|l| l.strip_prefix("voluntary_ctxt_switches:")).and_then(|v| v.trim().parse::<u64>().ok()).unwrap_or(0))
        .sum()
}

#[test]
fn an_idle_guard_wakes_nothing_a_connection_still_wakes_it_and_a_stop_ends_it_at_once() {
    let resolver: Arc<dyn Resolver> = Arc::new(StdResolver);
    let connector: Arc<dyn Connector> = Arc::new(StdConnector);
    let proxy = Proxy::start(Config::default(), resolver, connector).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let before = wakeups();
    std::thread::sleep(Duration::from_millis(1500));
    let idle = wakeups() - before;
    assert!(idle <= 6, "an idle guard woke {idle} times in 1.5 s (a poll every 20 ms would be about 75)");
    // a connection is accepted and answered as before (a request line that is refused, with no lookup: a literal address)
    let mut s = TcpStream::connect(proxy.addr()).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    s.write_all(b"CONNECT 203.0.113.9:443 HTTP/1.1\r\n\r\n").unwrap();
    let mut buf = [0u8; 32];
    let n = s.read(&mut buf).unwrap();
    assert!(String::from_utf8_lossy(&buf[..n]).starts_with("HTTP/1.1 403"), "{}", String::from_utf8_lossy(&buf[..n]));
    let accepted_before = proxy.stats().accepted;
    let t = Instant::now();
    proxy.stop();
    assert!(t.elapsed() < Duration::from_millis(800), "a stop wakes the blocked accept: {:?}", t.elapsed());
    assert!(accepted_before >= 1);
}
