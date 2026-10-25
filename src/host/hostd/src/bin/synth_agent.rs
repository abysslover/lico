//! A synthetic agent. Standard output lines:
//!   `R <hex>`           a record (history first, then `--rate` per second; rate 0 ends it after the history)
//!   `E <request_id>`    an input line was read and acted on
//!   `A <n> <prompt>`    an approval request
//! Standard input lines: `I <request_id> <hex of text>` (echoed back as a record, and a text
//! beginning with `ask` raises an approval) and `P <n> <allow|deny>` (an approval answer).
//! It uses no network, credential or provider.

#[path = "../payload.rs"]
mod payload;

use std::io::{BufRead, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

type Out = Arc<Mutex<std::io::Stdout>>;

fn emit(out: &Out, line: &str) -> bool {
    let guard = out.lock().unwrap();
    let mut lock = guard.lock();
    writeln!(lock, "{line}").and_then(|_| lock.flush()).is_ok()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |name: &str, default: &str| args.windows(2).find(|w| w[0] == name).map_or(default.to_string(), |w| w[1].clone());
    let history: i64 = arg("--history", "0").parse().unwrap_or(0);
    let rate: f64 = arg("--rate", "0").parse().unwrap_or(0.0);
    let size: usize = arg("--payload", "1024").parse().unwrap_or(1024);
    let realistic = args.iter().any(|a| a == "--realistic-text"); // the same lines with ordinary words only
    let text_mode = realistic || args.iter().any(|a| a == "--text"); // records are UTF-8 lines shaped like agent output
    let line_for = move |seq: i64| match (text_mode, realistic) {
        (true, true) => payload::realistic_line(seq, size).into_bytes(),
        (true, false) => payload::text_line(seq, size).into_bytes(),
        _ => payload::payload(seq, size),
    };
    let stay = args.iter().any(|a| a == "--stay"); // an idle agent keeps handling input after its history
    let out: Out = Arc::new(Mutex::new(std::io::stdout()));
    let writer = out.clone();
    let reader = std::thread::spawn(move || {
        let mut approvals = 0u32;
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            let parts: Vec<&str> = line.splitn(3, ' ').collect();
            match parts.as_slice() {
                ["I", rid, text_hex] => {
                    let text = payload::unhex(text_hex).and_then(|b| String::from_utf8(b).ok()).unwrap_or_default();
                    if !emit(&writer, &format!("R {}", payload::hex(format!("echo {text}").as_bytes()))) {
                        return;
                    }
                    if text.starts_with("ask") {
                        approvals += 1;
                        emit(&writer, &format!("A {approvals} may the agent {text}?"));
                    }
                    emit(&writer, &format!("E {rid}"));
                }
                ["P", n, answer] => {
                    emit(&writer, &format!("R {}", payload::hex(format!("approval {n} {answer}").as_bytes())));
                }
                _ => {}
            }
        }
    });
    let mut seq = 0i64;
    while seq < history {
        if !emit(&out, &format!("R {}", payload::hex(&line_for(seq)))) {
            return;
        }
        seq += 1;
    }
    if rate <= 0.0 {
        if stay {
            let _ = reader.join();
        }
        return;
    }
    let interval = Duration::from_secs_f64(1.0 / rate);
    let mut next_at = Instant::now();
    loop {
        next_at += interval;
        if !emit(&out, &format!("R {}", payload::hex(&line_for(seq)))) {
            return;
        }
        seq += 1;
        let now = Instant::now();
        if next_at > now {
            std::thread::sleep(next_at - now);
        }
    }
}
