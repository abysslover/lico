//! The contract tests of `lico-llm-fake` (SPEC.md): what it listens on, what it answers, what it records, how it ends. A plain `TcpStream` is the client, so the fake
//! is tested without any other component.

use lico_llm_fake::*;
use serde_json::Value;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

fn connect(addr: SocketAddr) -> TcpStream {
    let s = TcpStream::connect_timeout(&addr, Duration::from_secs(2)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    s
}

fn post(path: &str, body: &str) -> Vec<u8> {
    format!("POST {path} HTTP/1.1\r\nHost: x\r\nAuthorization: Bearer SECRET-1\r\nContent-Length: {}\r\n\r\n{body}", body.len()).into_bytes()
}

/// Sends a request and reads everything until the connection ends.
fn exchange(addr: SocketAddr, request: &[u8]) -> Vec<u8> {
    let mut s = connect(addr);
    s.write_all(request).unwrap();
    let mut out = Vec::new();
    let _ = s.read_to_end(&mut out);
    out
}

fn split(raw: &[u8]) -> (String, Vec<u8>) {
    let at = raw.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4).unwrap_or(raw.len());
    (String::from_utf8_lossy(&raw[..at]).into_owned(), raw[at..].to_vec())
}

fn dechunk(mut body: &[u8]) -> Vec<Vec<u8>> {
    let mut pieces = Vec::new();
    loop {
        let eol = body.windows(2).position(|w| w == b"\r\n").expect("a chunk size line");
        let size = usize::from_str_radix(std::str::from_utf8(&body[..eol]).unwrap(), 16).unwrap();
        if size == 0 {
            return pieces;
        }
        pieces.push(body[eol + 2..eol + 2 + size].to_vec());
        body = &body[eol + 2 + size + 2..];
    }
}

fn pieces(texts: &[&str]) -> Vec<Vec<u8>> {
    texts.iter().map(|t| t.as_bytes().to_vec()).collect()
}

#[test]
fn it_listens_on_loopback_with_an_ephemeral_port_and_stops_cleanly() {
    let a = Fake::start(Script::new()).unwrap();
    let b = Fake::start(Script::new()).unwrap();
    assert!(a.addr().ip().is_loopback() && a.addr().ip().to_string() == "127.0.0.1", "{}", a.addr());
    assert!(a.addr().port() != 0 && a.addr().port() != b.addr().port());
    let addr = a.addr();
    let started = Instant::now();
    a.stop();
    assert!(started.elapsed() < Duration::from_secs(2), "stopping takes long: {:?}", started.elapsed());
    assert!(TcpStream::connect_timeout(&addr, Duration::from_millis(500)).is_err(), "nothing listens after stop");
    assert!(TcpStream::connect_timeout(&b.addr(), Duration::from_millis(500)).is_ok(), "the other fake is untouched");
}

#[test]
fn a_whole_reply_goes_out_with_its_status_headers_and_length_and_the_request_is_recorded() {
    let script = Script::new().on("POST", "/v1/chat/completions", Behaviour::Answer(Reply::json(200, "{\"ok\":true}").header("X-Test", "1")));
    let fake = Fake::start(script).unwrap();
    let raw = exchange(fake.addr(), &post("/v1/chat/completions", "{\"model\":\"m\"}"));
    let (head, body) = split(&raw);
    assert!(head.starts_with("HTTP/1.1 200 OK\r\n"), "{head}");
    assert!(head.contains("Content-Length: 11\r\n") && head.contains("X-Test: 1\r\n") && head.contains("Content-Type: application/json\r\n") && head.contains("Connection: close\r\n"), "{head}");
    assert_eq!(body, b"{\"ok\":true}");
    let seen = fake.requests();
    assert_eq!(seen.len(), 1);
    assert_eq!((seen[0].method.as_str(), seen[0].path.as_str()), ("POST", "/v1/chat/completions"));
    assert_eq!(seen[0].body, b"{\"model\":\"m\"}");
    assert_eq!(seen[0].header("Content-Length"), Some("13"));
    assert_eq!(seen[0].authorization(), Some("Bearer SECRET-1"));
    assert!(!format!("{:?}", seen[0]).contains("SECRET-1"), "Debug must not show the key: {:?}", seen[0]);
}

#[test]
fn rules_answer_in_order_a_limited_rule_is_used_up_and_an_unscripted_request_is_a_404() {
    let script = Script::new()
        .times(2, "POST", "/x", Behaviour::Answer(Reply::json(429, "busy")))
        .on("POST", "/x", Behaviour::Answer(Reply::json(200, "fine")))
        .on("GET", "/y", Behaviour::Answer(Reply::json(200, "y")));
    let fake = Fake::start(script).unwrap();
    let status = |req: &[u8]| split(&exchange(fake.addr(), req)).0.lines().next().unwrap().to_string();
    assert_eq!(status(&post("/x", "")), "HTTP/1.1 429 Too Many Requests");
    assert_eq!(status(&post("/x", "")), "HTTP/1.1 429 Too Many Requests");
    assert_eq!(status(&post("/x", "")), "HTTP/1.1 200 OK");
    assert_eq!(status(&post("/x", "")), "HTTP/1.1 200 OK");
    assert_eq!(status(b"GET /y HTTP/1.1\r\nHost: x\r\n\r\n"), "HTTP/1.1 200 OK");
    let raw = exchange(fake.addr(), b"GET /nothing HTTP/1.1\r\nHost: x\r\n\r\n");
    let (head, body) = split(&raw);
    assert!(head.starts_with("HTTP/1.1 404"), "{head}");
    assert_eq!(serde_json::from_slice::<Value>(&body).unwrap()["error"], "no script for GET /nothing");
    assert_eq!(fake.requests().len(), 6, "every request is recorded, answered by a rule or not");
}

#[test]
fn a_chunked_reply_sends_each_piece_as_its_own_chunk_with_the_gap_between_them() {
    let p = pieces(&["one", "two", "three"]);
    let script = Script::new().on("POST", "/s", Behaviour::Answer(Reply::chunked(200, "text/plain", p.clone()).gap(Duration::from_millis(80))));
    let fake = Fake::start(script).unwrap();
    let started = Instant::now();
    let raw = exchange(fake.addr(), &post("/s", ""));
    assert!(started.elapsed() >= Duration::from_millis(160), "two gaps of 80 ms: {:?}", started.elapsed());
    let (head, body) = split(&raw);
    assert!(head.contains("Transfer-Encoding: chunked\r\n") && !head.contains("Content-Length"), "{head}");
    assert_eq!(dechunk(&body), p);
    assert!(body.ends_with(b"0\r\n\r\n"));
}

#[test]
fn a_body_without_length_or_chunks_ends_when_the_connection_closes_and_a_raw_body_is_only_its_bytes() {
    let script = Script::new()
        .on("POST", "/u", Behaviour::Answer(Reply { body: Body::UntilClose(pieces(&["ab", "cd"])), ..Reply::json(200, "") }))
        .on("POST", "/r", Behaviour::Answer(Reply { body: Body::Raw(b"garbage\x00\xff".to_vec()), ..Reply::json(200, "") }));
    let fake = Fake::start(script).unwrap();
    let (head, body) = split(&exchange(fake.addr(), &post("/u", "")));
    assert!(!head.contains("Content-Length") && !head.contains("chunked"), "{head}");
    assert_eq!(body, b"abcd");
    assert_eq!(exchange(fake.addr(), &post("/r", "")), b"garbage\x00\xff");
}

#[test]
fn a_cut_ending_closes_before_the_end_and_a_silent_ending_keeps_the_connection_open() {
    let p = pieces(&["a", "b", "c"]);
    let script = Script::new()
        .on("POST", "/cut", Behaviour::Answer(Reply::chunked(200, "text/plain", p.clone()).ending(Ending::Cut(2))))
        .on("POST", "/whole", Behaviour::Answer(Reply::json(200, "0123456789").ending(Ending::Cut(4))))
        .on("POST", "/silent", Behaviour::Answer(Reply::chunked(200, "text/plain", p).ending(Ending::Silent(1))));
    let fake = Fake::start(script).unwrap();
    let (_, body) = split(&exchange(fake.addr(), &post("/cut", "")));
    assert_eq!(body, b"1\r\na\r\n1\r\nb\r\n", "two chunks and no end");
    let (head, body) = split(&exchange(fake.addr(), &post("/whole", "")));
    assert!(head.contains("Content-Length: 10"));
    assert_eq!(body, b"0123", "fewer bytes than promised");
    let mut s = connect(fake.addr());
    s.write_all(&post("/silent", "")).unwrap();
    s.set_read_timeout(Some(Duration::from_millis(400))).unwrap();
    let mut got = Vec::new();
    let mut buf = [0u8; 256];
    let started = Instant::now();
    loop {
        match s.read(&mut buf) {
            Ok(0) => panic!("the connection was closed after {:?}", started.elapsed()),
            Ok(n) => got.extend_from_slice(&buf[..n]),
            Err(_) => break,
        }
    }
    assert!(got.ends_with(b"1\r\na\r\n"), "the first piece arrived: {:?}", String::from_utf8_lossy(&got));
}

#[test]
fn close_answers_nothing_hang_stays_open_and_a_late_first_byte_is_late() {
    let script = Script::new()
        .on("POST", "/gone", Behaviour::Close)
        .on("POST", "/hang", Behaviour::Hang)
        .on("POST", "/slow", Behaviour::Answer(Reply::json(200, "late").first_byte_after(Duration::from_millis(300))));
    let fake = Fake::start(script).unwrap();
    assert_eq!(exchange(fake.addr(), &post("/gone", "")), b"", "no byte, then the end of the connection");
    let mut s = connect(fake.addr());
    s.write_all(&post("/hang", "")).unwrap();
    s.set_read_timeout(Some(Duration::from_millis(300))).unwrap();
    let mut b = [0u8; 8];
    assert!(matches!(s.read(&mut b), Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut)), "still open, nothing said");
    let started = Instant::now();
    let raw = exchange(fake.addr(), &post("/slow", ""));
    assert!(started.elapsed() >= Duration::from_millis(300), "{:?}", started.elapsed());
    assert!(raw.ends_with(b"late"));
    // stopping ends a hanging connection too
    let t = Instant::now();
    fake.stop();
    assert!(t.elapsed() < Duration::from_secs(2));
    s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    assert!(matches!(s.read(&mut b), Ok(0) | Err(_)));
}

#[test]
fn an_oversize_request_is_refused_and_a_bad_head_is_dropped() {
    let fake = Fake::start(Script::new().on("POST", "/x", Behaviour::Answer(Reply::json(200, "ok")))).unwrap();
    let raw = exchange(fake.addr(), b"POST /x HTTP/1.1\r\nContent-Length: 99999999\r\n\r\n");
    assert!(split(&raw).0.starts_with("HTTP/1.1 413"), "{raw:?}");
    assert_eq!(exchange(fake.addr(), b"NONSENSE\r\n\r\n"), b"", "a request line without a path gets no answer");
    let big = "x".repeat(2 * 1024 * 1024);
    assert!(split(&exchange(fake.addr(), &post("/x", &big))).0.starts_with("HTTP/1.1 200"), "a large body within the limit is read");
    assert_eq!(fake.requests().last().unwrap().body.len(), big.len());
}

// ---- the shapes of the two APIs --------------------------------------------------------------------------------------------------

fn text_of(chunks: &[Vec<u8>]) -> String {
    chunks.iter().map(|c| String::from_utf8(c.clone()).unwrap()).collect()
}

#[test]
fn the_openai_shape_is_server_sent_events_with_deltas_calls_in_pieces_usage_and_done() {
    let out = openai_sse(&[
        Piece::Reasoning("hmm".into()),
        Piece::Text("Hi".into()),
        Piece::Call { id: Some("call_1".into()), name: "sessions".into(), args: "{\"q\": \"abc\"}".into(), split: 4 },
        Piece::Call { id: None, name: "send".into(), args: "{}".into(), split: 0 },
        Piece::Finish("tool_calls".into()),
        Piece::Usage { prompt: 10, completion: 3 },
        Piece::End,
    ]);
    let text = text_of(&out);
    assert!(out.iter().all(|c| c.starts_with(b"data: ") && c.ends_with(b"\n\n")));
    assert!(text.ends_with("data: [DONE]\n\n"));
    let json = |c: &Vec<u8>| -> Value { serde_json::from_slice(&c[6..c.len() - 2]).unwrap() };
    let values: Vec<Value> = out[..out.len() - 1].iter().map(json).collect();
    assert_eq!(values[0]["choices"][0]["delta"]["role"], "assistant");
    assert_eq!(values[1]["choices"][0]["delta"]["reasoning_content"], "hmm");
    assert_eq!(values[2]["choices"][0]["delta"]["content"], "Hi");
    let first = &values[3]["choices"][0]["delta"]["tool_calls"][0];
    assert_eq!((first["index"].as_u64(), first["id"].as_str(), first["function"]["name"].as_str(), first["function"]["arguments"].as_str()), (Some(0), Some("call_1"), Some("sessions"), Some("")));
    let args: String = values[4..7].iter().map(|v| v["choices"][0]["delta"]["tool_calls"][0]["function"]["arguments"].as_str().unwrap().to_string()).collect();
    assert_eq!(args, "{\"q\": \"abc\"}", "joined in slices of 4 characters");
    let second = values.iter().map(|v| &v["choices"][0]["delta"]["tool_calls"][0]).find(|c| c["index"] == 1 && c["function"]["name"] == "send").expect("the second call starts");
    assert!(second["id"].is_null(), "a call without an id has none: {second}");
    let finish = values.iter().find(|v| v["choices"][0]["finish_reason"] == "tool_calls");
    assert!(finish.is_some(), "a finish reason");
    let usage = values.last().unwrap();
    assert_eq!((usage["choices"].as_array().unwrap().len(), usage["usage"]["prompt_tokens"].as_u64(), usage["usage"]["completion_tokens"].as_u64()), (0, Some(10), Some(3)));
}

#[test]
fn the_ollama_shape_is_one_json_object_per_line_with_calls_as_objects_and_a_done_line() {
    let out = ollama_ndjson(&[
        Piece::Reasoning("hmm".into()),
        Piece::Text("Hi".into()),
        Piece::Call { id: None, name: "sessions".into(), args: "{\"q\": 1}".into(), split: 3 },
        Piece::Finish("stop".into()),
        Piece::Usage { prompt: 7, completion: 2 },
        Piece::End,
    ]);
    assert!(out.iter().all(|l| l.ends_with(b"\n") && !l[..l.len() - 1].contains(&b'\n')));
    let v: Vec<Value> = out.iter().map(|l| serde_json::from_slice(l).unwrap()).collect();
    assert_eq!(v[0]["message"]["thinking"], "hmm");
    assert_eq!(v[1]["message"]["content"], "Hi");
    assert_eq!(v[2]["message"]["tool_calls"][0]["function"]["name"], "sessions");
    assert_eq!(v[2]["message"]["tool_calls"][0]["function"]["arguments"]["q"], 1, "an object, not a text");
    assert!(v[..3].iter().all(|l| l["done"] == false));
    let last = &v[3];
    assert_eq!((last["done"].as_bool(), last["done_reason"].as_str(), last["prompt_eval_count"].as_u64(), last["eval_count"].as_u64()), (Some(true), Some("stop"), Some(7), Some(2)));
    assert_eq!(v.len(), 4);
}

#[test]
fn the_error_bodies_have_the_documented_shapes() {
    let o: Value = serde_json::from_str(&openai_error("The model `x` does not exist", "model_not_found")).unwrap();
    assert_eq!((o["error"]["message"].as_str(), o["error"]["code"].as_str(), o["error"]["type"].as_str()), (Some("The model `x` does not exist"), Some("model_not_found"), Some("invalid_request_error")));
    let l: Value = serde_json::from_str(&ollama_error("model 'x' not found")).unwrap();
    assert_eq!(l["error"], "model 'x' not found");
}

// ---- the public names -------------------------------------------------------------------------------------------------------------

/// The API lines of the specification, exactly: the names `src/lib.rs` makes public are these and no others.
#[test]
fn the_public_names_are_exactly_these() {
    let source = include_str!("../src/lib.rs");
    let mut found: Vec<String> = Vec::new();
    for line in source.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("pub use ") {
            let names = rest.split_once('{').map(|(_, n)| n.trim_end_matches(';').trim_end_matches('}')).unwrap_or(rest).trim_end_matches(';');
            found.extend(names.split(',').map(|n| n.trim().to_string()).filter(|n| !n.is_empty()));
        } else if line.starts_with("pub ") {
            panic!("an item is public outside the re-export list: {line}");
        }
    }
    found.sort();
    let mut expected: Vec<String> = ["Behaviour", "Body", "Ending", "Reply", "Script", "Fake", "Seen", "ollama_error", "ollama_ndjson", "openai_error", "openai_sse", "Piece"].iter().map(|s| s.to_string()).collect();
    expected.sort();
    assert_eq!(found, expected);
}

/// SPEC.md, Dependencies: only `serde_json`, and nothing of `lico-llm-wire` (the two implementations must stay independent), and no address but loopback in the code.
#[test]
fn it_depends_on_serde_json_only_and_names_no_address_but_loopback() {
    let manifest = include_str!("../Cargo.toml");
    let deps: Vec<&str> = manifest.split("[dependencies]").nth(1).unwrap().lines().skip(1).take_while(|l| !l.starts_with('[')).filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(deps.len(), 1, "{deps:?}");
    assert!(deps[0].starts_with("serde_json ") && !manifest.contains("lico-llm-wire") && !manifest.contains("lico_llm_wire"), "{deps:?}");
    for entry in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/src")).unwrap() {
        let path = entry.unwrap().path();
        let code = std::fs::read_to_string(&path).unwrap();
        assert!(!code.contains("lico_llm_wire") && !code.contains("0.0.0.0") && !code.contains("::]"), "{}", path.display());
    }
}
