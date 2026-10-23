//! A synthetic agent: prints deterministic records, one hex line each. It first
//! prints `--history` records at once, then `--rate` records per second forever
//! (a rate of 0 ends it after the history). It reads its input and ignores it in
//! this phase. It uses no network, credential or provider.

#[path = "../payload.rs"]
mod payload;

use std::io::{Read, Write};
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |name: &str, default: &str| args.windows(2).find(|w| w[0] == name).map_or(default.to_string(), |w| w[1].clone());
    let history: i64 = arg("--history", "0").parse().unwrap_or(0);
    let rate: f64 = arg("--rate", "0").parse().unwrap_or(0.0);
    let size: usize = arg("--payload", "1024").parse().unwrap_or(1024);
    std::thread::spawn(|| {
        let mut sink = [0u8; 4096];
        while std::io::stdin().read(&mut sink).map_or(false, |n| n > 0) {}
    });
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let mut seq = 0i64;
    while seq < history {
        if writeln!(out, "{}", payload::hex(&payload::payload(seq, size))).is_err() {
            return;
        }
        seq += 1;
    }
    let _ = out.flush();
    if rate <= 0.0 {
        return;
    }
    let interval = Duration::from_secs_f64(1.0 / rate);
    let mut next_at = Instant::now();
    loop {
        next_at += interval;
        if writeln!(out, "{}", payload::hex(&payload::payload(seq, size))).and_then(|_| out.flush()).is_err() {
            return;
        }
        seq += 1;
        let now = Instant::now();
        if next_at > now {
            std::thread::sleep(next_at - now);
        }
    }
}
