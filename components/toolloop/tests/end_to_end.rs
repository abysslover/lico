//! The end-to-end tests (SPEC.md): the loop through the real client, the P04 fake server and the real record. The model port below is the shape of the host adapter
//! of the next phase: encode, send, decode. No real server, no real home: the record is in a scratch folder and the only socket is the fake's own loopback port.

use lico_convo::Convo;
use lico_llm_fake::*;
use lico_llm_http as http;
use lico_llm_wire as wire;
use lico_toolloop::*;
use serde_json::json;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

struct HttpModel {
    addr: SocketAddr,
    api: wire::Api,
}

impl ModelPort for HttpModel {
    fn turn(&mut self, messages: &[wire::Message], tools: &[wire::ToolDef], sink: &mut dyn FnMut(wire::Event) -> bool) -> Result<(), ModelError> {
        let request = wire::Request { model: "fake", messages, tools, stream: true, context_tokens: None, max_output_tokens: None, include_usage: true };
        let encoded = wire::encode(self.api, &request).map_err(|e| ModelError::Other(format!("{e:?}")))?;
        let mut decoder = wire::Decoder::new(self.api, wire::Framing::Stream);
        let mut body = Vec::new();
        let mut stopped = false;
        let limits = http::Limits { first_byte: Duration::from_secs(3), idle: Duration::from_secs(3), total: Duration::from_secs(10), ..http::Limits::default() };
        let req = http::Request { addr: self.addr, method: http::Method::Post, path: encoded.path, headers: &[("Content-Type", wire::CONTENT_TYPE)], body: &encoded.body };
        let result = http::send(&req, &limits, &http::Cancel::new(), &mut |piece| {
            body.extend_from_slice(piece);
            if stopped {
                return http::Flow::Stop;
            }
            for e in decoder.feed(piece) {
                if !sink(e) {
                    stopped = true;
                    return http::Flow::Stop;
                }
            }
            http::Flow::Continue
        });
        match result {
            Ok(head) if head.status == 200 => {
                if !stopped {
                    for e in decoder.finish() {
                        sink(e);
                    }
                }
                Ok(())
            }
            Ok(head) => Err(ModelError::Failure(wire::classify(self.api, head.status, &body))),
            Err(http::HttpError::BadResponse(_)) => {
                for e in decoder.finish() {
                    sink(e);
                }
                Ok(())
            }
            Err(http::HttpError::Connect(_)) | Err(http::HttpError::NotLoopback) => Err(ModelError::Gone),
            Err(http::HttpError::Timeout(_)) => Err(ModelError::Timeout),
            Err(http::HttpError::Cancelled) => Err(ModelError::Cancelled),
            Err(e) => Err(ModelError::Other(format!("{e:?}"))),
        }
    }
}

struct Calls {
    seen: Vec<wire::ToolCall>,
    waiting: bool,
}

impl ToolPort for Calls {
    fn call(&mut self, call: &wire::ToolCall) -> ToolOutcome {
        self.seen.push(call.clone());
        if self.waiting { ToolOutcome::Waiting("asked: waiting for the person".into()) } else { ToolOutcome::Done("[]".into()) }
    }
}

struct Record(Convo);

impl RecordPort for Record {
    fn append(&mut self, m: &wire::Message) -> Result<(), String> {
        self.0.append(m).map_err(|e| format!("{e:?}"))
    }
}

struct Plain;

impl Env for Plain {
    fn cancelled(&self) -> bool {
        false
    }
    fn now_ms(&self) -> u64 {
        0
    }
}

fn path(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("toolloop").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    dir.join("rec").join("a.jsonl")
}

fn cfg() -> Config {
    let def = |n: &str| wire::ToolDef { name: n.into(), description: n.into(), parameters: json!({"type": "object"}) };
    Config { tools: vec![def("sessions"), def("send")], limits: Limits::default() }
}

fn turn(addr: SocketAddr, api: wire::Api, rec: &std::path::Path, waiting: bool, input: &str) -> (TurnEnd, Vec<Out>, Vec<wire::ToolCall>) {
    let (convo, _) = Convo::open(rec).unwrap();
    let mut history = convo.messages().to_vec();
    let mut model = HttpModel { addr, api };
    let mut tools = Calls { seen: Vec::new(), waiting };
    let mut record = Record(convo);
    let mut out = Vec::new();
    let end = {
        let mut ports = Ports { model: &mut model, tools: &mut tools, record: &mut record, env: &Plain };
        run_turn(&mut ports, &mut history, wire::Message::User(input.into()), &cfg(), &mut |o| out.push(o))
    };
    (end, out, tools.seen)
}

fn sse(pieces: &[Piece]) -> Reply {
    Reply::sse(openai_sse(pieces))
}

const CHAT: &str = "/v1/chat/completions";

#[test]
fn a_tool_round_through_the_real_client_the_fake_and_the_record_for_both_formats() {
    for api in [wire::Api::OpenAi, wire::Api::Ollama] {
        let (path_, replies): (&str, [Reply; 2]) = if api == wire::Api::OpenAi {
            (
                CHAT,
                [
                    sse(&[Piece::Text("one moment".into()), Piece::Call { id: Some("c1".into()), name: "sessions".into(), args: "{\"q\": \"x\"}".into(), split: 3 }, Piece::Finish("tool_calls".into()), Piece::End]),
                    sse(&[Piece::Text("done ".into()), Piece::Text("한국어".into()), Piece::Finish("stop".into()), Piece::End]),
                ],
            )
        } else {
            (
                "/api/chat",
                [
                    Reply::ndjson(ollama_ndjson(&[Piece::Text("one moment".into()), Piece::Call { id: None, name: "sessions".into(), args: "{\"q\": \"x\"}".into(), split: 0 }, Piece::End])),
                    Reply::ndjson(ollama_ndjson(&[Piece::Text("done ".into()), Piece::Text("한국어".into()), Piece::End])),
                ],
            )
        };
        let [first, second] = replies;
        let fake = Fake::start(Script::new().times(1, "POST", path_, Behaviour::Answer(first)).on("POST", path_, Behaviour::Answer(second))).unwrap();
        let rec = path(&format!("round_{api:?}"));
        let (end, out, calls) = turn(fake.addr(), api, &rec, false, "list the sessions");
        assert_eq!(end, TurnEnd::Finished(wire::StopReason::Stop), "{api:?}");
        assert_eq!(calls.len(), 1);
        assert_eq!((calls[0].name.as_str(), calls[0].arguments.clone()), ("sessions", json!({"q": "x"})));
        let texts: String = out.iter().filter_map(|o| if let Out::Text(t) = o { Some(t.as_str()) } else { None }).collect();
        assert_eq!(texts, "one momentdone 한국어");
        // the record on disk is what the model must get back
        let (convo, opened) = Convo::open(&rec).unwrap();
        assert_eq!(opened.loaded, 4);
        let m = convo.messages();
        assert!(matches!(&m[0], wire::Message::User(t) if t == "list the sessions"));
        assert!(matches!(&m[1], wire::Message::Assistant { text, calls } if text == "one moment" && calls.len() == 1));
        assert!(matches!(&m[2], wire::Message::Tool { name, content, .. } if name == "sessions" && content == "[]"));
        assert!(matches!(&m[3], wire::Message::Assistant { text, calls } if text == "done 한국어" && calls.is_empty()));
        // the second request carried the call and its result, in the shape of the API
        let seen = fake.requests();
        assert_eq!(seen.len(), 2);
        let body: serde_json::Value = serde_json::from_slice(&seen[1].body).unwrap();
        let roles: Vec<&str> = body["messages"].as_array().unwrap().iter().map(|m| m["role"].as_str().unwrap()).collect();
        assert_eq!(roles, ["user", "assistant", "tool"], "{api:?}");
        assert_eq!(body["tools"].as_array().unwrap().len(), 2);
    }
}

#[test]
fn a_waiting_tool_ends_the_turn_and_the_decision_starts_the_next_one_from_the_record() {
    let fake = Fake::start(
        Script::new()
            .times(1, "POST", CHAT, Behaviour::Answer(sse(&[Piece::Call { id: Some("c1".into()), name: "send".into(), args: "{\"q\": \"hi\"}".into(), split: 0 }, Piece::Finish("tool_calls".into()), Piece::End])))
            .on("POST", CHAT, Behaviour::Answer(sse(&[Piece::Text("sent".into()), Piece::Finish("stop".into()), Piece::End]))),
    )
    .unwrap();
    let rec = path("waiting");
    let (end, _, calls) = turn(fake.addr(), wire::Api::OpenAi, &rec, true, "tell the other session");
    assert_eq!(end, TurnEnd::Waiting);
    assert_eq!(calls.len(), 1);
    assert_eq!(fake.requests().len(), 1, "the model was not asked again while the person decides");
    // later: the person's decision arrives as a message from the host; the conversation continues from the record
    let (end, out, calls) = turn(fake.addr(), wire::Api::OpenAi, &rec, false, "[host] the person allowed the call c1");
    assert_eq!(end, TurnEnd::Finished(wire::StopReason::Stop));
    assert!(calls.is_empty() && out == vec![Out::Text("sent".into())]);
    let body: serde_json::Value = serde_json::from_slice(&fake.requests()[1].body).unwrap();
    let roles: Vec<&str> = body["messages"].as_array().unwrap().iter().map(|m| m["role"].as_str().unwrap()).collect();
    assert_eq!(roles, ["user", "assistant", "tool", "user"], "the call, its waiting result and the decision are all in what the model sees");
}

#[test]
fn a_server_that_is_gone_that_wants_a_key_or_that_cuts_the_stream_ends_the_turn_and_runs_nothing() {
    // gone: nothing listens
    let fake = Fake::start(Script::new()).unwrap();
    let addr = fake.addr();
    fake.stop();
    let rec = path("gone");
    let (end, out, calls) = turn(addr, wire::Api::OpenAi, &rec, false, "hello");
    assert_eq!(end, TurnEnd::Failed(Failed::Port(ModelError::Gone)));
    assert!(out.is_empty() && calls.is_empty());
    assert_eq!(Convo::open(&rec).unwrap().0.messages().len(), 1, "only the person's message was recorded");
    // a key is needed
    let fake = Fake::start(Script::new().on("POST", CHAT, Behaviour::Answer(Reply::json(401, &openai_error("Incorrect API key provided", "invalid_api_key"))))).unwrap();
    let (end, _, _) = turn(fake.addr(), wire::Api::OpenAi, &path("key"), false, "hello");
    assert!(matches!(end, TurnEnd::Failed(Failed::Model(f)) if f.kind == wire::FailureKind::KeyNeeded));
    // the stream is cut after a call was announced: the text stays shown, nothing is recorded or run
    let pieces = openai_sse(&[Piece::Text("par".into()), Piece::Call { id: Some("c1".into()), name: "sessions".into(), args: "{}".into(), split: 0 }, Piece::Finish("tool_calls".into()), Piece::End]);
    let fake = Fake::start(Script::new().on("POST", CHAT, Behaviour::Answer(Reply::sse(pieces).ending(Ending::Cut(4))))).unwrap();
    let rec = path("cut");
    let (end, out, calls) = turn(fake.addr(), wire::Api::OpenAi, &rec, false, "hello");
    assert_eq!(end, TurnEnd::Failed(Failed::Stream(wire::StreamError::Truncated)));
    assert_eq!(out, vec![Out::Text("par".into())]);
    assert!(calls.is_empty(), "a call of a cut stream is never run");
    assert_eq!(Convo::open(&rec).unwrap().0.messages().len(), 1);
}
