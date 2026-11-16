//! The cross tests (SPEC.md): the fake server, this client and the wire formats together. The fake and `lico-llm-wire` were written separately from the public
//! documentation of the two APIs, so each checks the other; this client sits between them. No real server is involved.

use lico_llm_fake::*;
use lico_llm_http::*;
use lico_llm_wire as wire;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::time::Duration;

fn limits() -> Limits {
    Limits { connect: Duration::from_secs(2), first_byte: Duration::from_secs(3), idle: Duration::from_secs(3), total: Duration::from_secs(10), max_body: 1 << 20 }
}

fn messages() -> Vec<wire::Message> {
    vec![wire::Message::System("be brief".into()), wire::Message::User("hello 한국어 😀".into())]
}

fn tools() -> Vec<wire::ToolDef> {
    vec![wire::ToolDef { name: "sessions".into(), description: "list".into(), parameters: json!({"type": "object", "properties": {}}) }]
}

struct Outcome {
    status: Option<u16>,
    http: Result<(), HttpError>,
    body: Vec<u8>,
    events: Vec<wire::Event>,
}

/// Encode with the wire crate, POST with this client, feed every piece that arrives to the wire decoder.
fn ask(addr: SocketAddr, api: wire::Api, framing: wire::Framing, cancel: &Cancel) -> Outcome {
    let (m, t) = (messages(), tools());
    let encoded = wire::encode(api, &wire::Request { model: "fake", messages: &m, tools: &t, stream: framing == wire::Framing::Stream, context_tokens: Some(4096), max_output_tokens: None, include_usage: true }).unwrap();
    let mut decoder = wire::Decoder::new(api, framing);
    let (mut events, mut body) = (Vec::new(), Vec::new());
    let req = Request { addr, method: Method::Post, path: encoded.path, headers: &[("Content-Type", wire::CONTENT_TYPE)], body: &encoded.body };
    let result = send(&req, &limits(), cancel, &mut |b| {
        body.extend_from_slice(b);
        events.extend(decoder.feed(b));
        Flow::Continue
    });
    let (status, http) = match result {
        Ok(h) => (Some(h.status), Ok(())),
        Err(e) => (None, Err(e)),
    };
    if status == Some(200) || http.is_err() {
        events.extend(decoder.finish());
    }
    Outcome { status, http, body, events }
}

fn serve(api: wire::Api, reply: Reply) -> Fake {
    let path = if api == wire::Api::OpenAi { "/v1/chat/completions" } else { "/api/chat" };
    Fake::start(Script::new().on("POST", path, Behaviour::Answer(reply))).unwrap()
}

fn call(id: &str, name: &str, args: Value) -> wire::Event {
    wire::Event::ToolCall(wire::ToolCall { id: id.into(), name: name.into(), arguments: args })
}

fn done(reason: wire::StopReason, usage: Option<(u64, u64)>) -> wire::Event {
    wire::Event::Done(wire::Done { reason, usage: usage.map(|(p, c)| wire::Usage { prompt_tokens: p, completion_tokens: c }) })
}

/// The same bytes cut into pieces of `size`: chunk boundaries in the middle of lines and of characters.
fn rechunk(pieces: Vec<Vec<u8>>, size: usize) -> Vec<Vec<u8>> {
    pieces.concat().chunks(size).map(|c| c.to_vec()).collect()
}

fn text_script() -> Vec<Piece> {
    vec![Piece::Text("Hel".into()), Piece::Text("lo 한국어 😀".into()), Piece::Finish("stop".into()), Piece::Usage { prompt: 12, completion: 5 }, Piece::End]
}

fn tools_script() -> Vec<Piece> {
    vec![
        Piece::Text("one moment".into()),
        Piece::Call { id: Some("call_a".into()), name: "sessions".into(), args: "{\"q\": \"한국\"}".into(), split: 3 },
        Piece::Call { id: None, name: "sessions".into(), args: "{}".into(), split: 0 },
        Piece::Finish("tool_calls".into()),
        Piece::Usage { prompt: 30, completion: 9 },
        Piece::End,
    ]
}

fn text_events(usage: bool) -> Vec<wire::Event> {
    vec![wire::Event::Text("Hel".into()), wire::Event::Text("lo 한국어 😀".into()), done(wire::StopReason::Stop, usage.then_some((12, 5)))]
}

// ---- the three agree on a good answer ----------------------------------------------------------------------------------------------

#[test]
fn an_openai_stream_of_text_goes_from_the_fake_through_the_client_into_the_decoder_whatever_the_chunking() {
    for size in [0usize, 1, 5, 17, 64] {
        let pieces = openai_sse(&text_script());
        let pieces = if size == 0 { pieces } else { rechunk(pieces, size) };
        let f = serve(wire::Api::OpenAi, Reply::sse(pieces));
        let o = ask(f.addr(), wire::Api::OpenAi, wire::Framing::Stream, &Cancel::new());
        assert_eq!((o.status, o.http.clone()), (Some(200), Ok(())), "chunks of {size}");
        let merged: Vec<wire::Event> = o.events.clone();
        assert_eq!(merged, text_events(true), "chunks of {size}: {merged:?}");
    }
}

#[test]
fn an_ollama_stream_of_text_and_of_tool_calls_comes_through_in_pieces_of_any_size() {
    for size in [1usize, 3, 40, 1000] {
        let f = serve(wire::Api::Ollama, Reply::ndjson(rechunk(ollama_ndjson(&text_script()), size)));
        let o = ask(f.addr(), wire::Api::Ollama, wire::Framing::Stream, &Cancel::new());
        assert_eq!(o.events, text_events(true), "chunks of {size}");
        let f = serve(wire::Api::Ollama, Reply::ndjson(rechunk(ollama_ndjson(&tools_script()), size)));
        let o = ask(f.addr(), wire::Api::Ollama, wire::Framing::Stream, &Cancel::new());
        assert_eq!(
            o.events,
            vec![
                wire::Event::Text("one moment".into()),
                call("call_1", "sessions", json!({"q": "한국"})),
                call("call_2", "sessions", json!({})),
                done(wire::StopReason::ToolCalls, Some((30, 9)))
            ],
            "chunks of {size}"
        );
    }
}

#[test]
fn openai_tool_calls_in_slices_come_out_whole_and_just_before_the_end() {
    for size in [1usize, 7, 1 << 20] {
        let f = serve(wire::Api::OpenAi, Reply::sse(rechunk(openai_sse(&tools_script()), size)));
        let o = ask(f.addr(), wire::Api::OpenAi, wire::Framing::Stream, &Cancel::new());
        assert_eq!(
            o.events,
            vec![
                wire::Event::Text("one moment".into()),
                call("call_a", "sessions", json!({"q": "한국"})),
                call("call_1", "sessions", json!({})),
                done(wire::StopReason::ToolCalls, Some((30, 9)))
            ],
            "chunks of {size}"
        );
    }
}

#[test]
fn a_reasoning_model_gives_a_thinking_marker_and_no_reasoning_text_in_either_format() {
    let script = vec![Piece::Reasoning("let me think".into()), Piece::Reasoning(" more".into()), Piece::Text("Answer".into()), Piece::Finish("stop".into()), Piece::End];
    let expected = vec![wire::Event::Thinking, wire::Event::Text("Answer".into()), done(wire::StopReason::Stop, None)];
    let f = serve(wire::Api::OpenAi, Reply::sse(openai_sse(&script)));
    assert_eq!(ask(f.addr(), wire::Api::OpenAi, wire::Framing::Stream, &Cancel::new()).events, expected);
    let f = serve(wire::Api::Ollama, Reply::ndjson(ollama_ndjson(&script)));
    assert_eq!(ask(f.addr(), wire::Api::Ollama, wire::Framing::Stream, &Cancel::new()).events, expected);
}

#[test]
fn a_whole_body_is_read_with_the_same_vocabulary() {
    let body = json!({"choices": [{"message": {"role": "assistant", "content": "Hi", "tool_calls": [{"id": "c1", "type": "function", "function": {"name": "sessions", "arguments": "{}"}}]}, "finish_reason": "tool_calls"}], "usage": {"prompt_tokens": 3, "completion_tokens": 1}});
    let f = serve(wire::Api::OpenAi, Reply::json(200, &body.to_string()));
    let o = ask(f.addr(), wire::Api::OpenAi, wire::Framing::Whole, &Cancel::new());
    assert_eq!(o.events, vec![wire::Event::Text("Hi".into()), call("c1", "sessions", json!({})), done(wire::StopReason::ToolCalls, Some((3, 1)))]);
}

#[test]
fn the_request_the_fake_saw_is_the_one_the_wire_crate_encoded() {
    let f = serve(wire::Api::OpenAi, Reply::sse(openai_sse(&text_script())));
    ask(f.addr(), wire::Api::OpenAi, wire::Framing::Stream, &Cancel::new());
    let seen = &f.requests()[0];
    assert_eq!((seen.method.as_str(), seen.path.as_str()), ("POST", "/v1/chat/completions"));
    let body: Value = serde_json::from_slice(&seen.body).unwrap();
    assert_eq!(body["model"], "fake");
    assert_eq!(body["messages"][1]["content"], "hello 한국어 😀");
    assert_eq!(body["stream"], true);
    assert_eq!(body["tools"][0]["function"]["name"], "sessions");
    assert_eq!(seen.header("content-type"), Some("application/json"));
    let f = serve(wire::Api::Ollama, Reply::ndjson(ollama_ndjson(&text_script())));
    ask(f.addr(), wire::Api::Ollama, wire::Framing::Stream, &Cancel::new());
    let body: Value = serde_json::from_slice(&f.requests()[0].body).unwrap();
    assert_eq!(f.requests()[0].path, "/api/chat");
    assert_eq!(body["options"]["num_ctx"], 4096);
}

// ---- the three agree on a bad one --------------------------------------------------------------------------------------------------

#[test]
fn an_http_error_status_and_body_become_a_failure_the_host_can_act_on() {
    let cases: Vec<(wire::Api, u16, String, wire::FailureKind)> = vec![
        (wire::Api::OpenAi, 401, openai_error("Incorrect API key provided", "invalid_api_key"), wire::FailureKind::KeyNeeded),
        (wire::Api::OpenAi, 404, openai_error("The model `x` does not exist", "model_not_found"), wire::FailureKind::ModelMissing),
        (wire::Api::Ollama, 404, ollama_error("model 'x' not found"), wire::FailureKind::ModelMissing),
        (wire::Api::Ollama, 400, ollama_error("registry.ollama.ai/library/gemma:latest does not support tools"), wire::FailureKind::ToolsUnsupported),
        (wire::Api::OpenAi, 429, openai_error("slow down", "rate_limit_exceeded"), wire::FailureKind::Busy),
        (wire::Api::OpenAi, 500, "boom".into(), wire::FailureKind::ServerError),
    ];
    for (api, status, body, kind) in cases {
        let f = serve(api, Reply::json(status, &body));
        let o = ask(f.addr(), api, wire::Framing::Stream, &Cancel::new());
        assert_eq!(o.status, Some(status));
        let failure = wire::classify(api, status, &o.body);
        assert_eq!(failure.kind, kind, "{api:?} {status}: {}", failure.message);
        assert!(!failure.message.is_empty(), "the server's own words are kept");
    }
}

#[test]
fn an_error_inside_a_stream_and_a_stream_that_was_cut_end_the_decoder_and_give_no_call() {
    let inside = vec![Piece::Text("start".into()), Piece::Call { id: None, name: "sessions".into(), args: "{}".into(), split: 0 }];
    let mut pieces = openai_sse(&inside);
    pieces.push(format!("data: {}\n\n", openai_error("The model `x` does not exist", "model_not_found")).into_bytes());
    let f = serve(wire::Api::OpenAi, Reply::sse(pieces));
    let o = ask(f.addr(), wire::Api::OpenAi, wire::Framing::Stream, &Cancel::new());
    assert!(matches!(o.events.last(), Some(wire::Event::Error(wire::StreamError::Server(f))) if f.kind == wire::FailureKind::ModelMissing), "{:?}", o.events);
    assert!(!o.events.iter().any(|e| matches!(e, wire::Event::ToolCall(_))));
    for (api, pieces) in [(wire::Api::OpenAi, openai_sse(&tools_script())), (wire::Api::Ollama, ollama_ndjson(&tools_script()))] {
        let f = serve(api, Reply::chunked(200, "text/plain", pieces).ending(Ending::Cut(3)));
        let o = ask(f.addr(), api, wire::Framing::Stream, &Cancel::new());
        assert!(matches!(o.http, Err(HttpError::BadResponse(_))), "{:?}", o.http);
        assert!(matches!(o.events.last(), Some(wire::Event::Error(wire::StreamError::Truncated))), "{api:?}: {:?}", o.events);
        assert!(!o.events.iter().any(|e| matches!(e, wire::Event::ToolCall(_))), "a cut reply gives no call: {:?}", o.events);
    }
}

#[test]
fn a_server_that_goes_quiet_is_ended_by_the_cancel_and_the_text_so_far_stays() {
    let script = vec![Piece::Text("so far".into()), Piece::Text(" more".into())];
    let f = serve(wire::Api::Ollama, Reply::ndjson(ollama_ndjson(&script)).ending(Ending::Silent(1)));
    let cancel = Cancel::new();
    let c = cancel.clone();
    let t = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        c.cancel();
    });
    let o = ask(f.addr(), wire::Api::Ollama, wire::Framing::Stream, &cancel);
    t.join().unwrap();
    assert_eq!(o.http, Err(HttpError::Cancelled));
    assert_eq!(o.events, vec![wire::Event::Text("so far".into()), wire::Event::Error(wire::StreamError::Truncated)]);
}

#[test]
fn the_ollama_facts_request_and_reply_work_end_to_end() {
    let body = json!({"capabilities": ["completion", "tools"], "model_info": {"llama.context_length": 8192}});
    let f = Fake::start(Script::new().on("POST", "/api/show", Behaviour::Answer(Reply::json(200, &body.to_string())))).unwrap();
    let e = wire::show_request("llama3");
    let mut got = Vec::new();
    let req = Request { addr: f.addr(), method: Method::Post, path: e.path, headers: &[("Content-Type", wire::CONTENT_TYPE)], body: &e.body };
    send(&req, &limits(), &Cancel::new(), &mut |b| {
        got.extend_from_slice(b);
        Flow::Continue
    })
    .unwrap();
    assert_eq!(wire::read_show(&got), wire::Show { context_tokens: Some(8192), tools: Some(true) });
    assert_eq!(serde_json::from_slice::<Value>(&f.requests()[0].body).unwrap(), json!({"model": "llama3"}));
}
