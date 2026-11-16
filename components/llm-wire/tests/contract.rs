//! The contract tests of `lico-llm-wire`: each states lines of `SPEC.md` and uses only the public names. The samples are written by hand from the public
//! documentation of the two APIs (KNOWN FROM DOCUMENTATION); no server was contacted.

use lico_llm_wire::*;
use serde_json::{json, Value};

fn body(e: &Encoded) -> Value {
    serde_json::from_slice(&e.body).expect("the body is JSON")
}

fn msgs() -> Vec<Message> {
    vec![Message::System("be brief".into()), Message::User("hello".into())]
}

fn request<'a>(model: &'a str, messages: &'a [Message], tools: &'a [ToolDef]) -> Request<'a> {
    Request { model, messages, tools, stream: true, context_tokens: None, max_output_tokens: None, include_usage: false }
}

fn tool(name: &str) -> ToolDef {
    ToolDef { name: name.into(), description: format!("the tool {name}"), parameters: json!({"type": "object", "properties": {"q": {"type": "string"}}, "required": ["q"]}) }
}

fn round() -> Vec<Message> {
    vec![
        Message::User("look".into()),
        Message::Assistant { text: String::new(), calls: vec![ToolCall { id: "call_1".into(), name: "sessions".into(), arguments: json!({"q": "x"}) }] },
        Message::Tool { call_id: "call_1".into(), name: "sessions".into(), content: "[]".into() },
    ]
}

// ---- encode: the OpenAI-compatible request ---------------------------------------------------------------------------------

#[test]
fn an_openai_request_has_the_model_the_messages_and_the_stream_flag_at_the_documented_path() {
    let m = msgs();
    let e = encode(Api::OpenAi, &request("qwen", &m, &[])).unwrap();
    assert_eq!(e.path, "/v1/chat/completions");
    assert_eq!(body(&e), json!({"model": "qwen", "messages": [{"role": "system", "content": "be brief"}, {"role": "user", "content": "hello"}], "stream": true}));
    let mut r = request("qwen", &m, &[]);
    r.stream = false;
    assert_eq!(body(&encode(Api::OpenAi, &r).unwrap())["stream"], json!(false));
    assert_eq!(CONTENT_TYPE, "application/json");
}

#[test]
fn an_openai_request_asks_for_usage_only_when_streaming_and_asked_and_carries_max_tokens() {
    let m = msgs();
    let mut r = request("qwen", &m, &[]);
    r.include_usage = true;
    r.max_output_tokens = Some(512);
    r.context_tokens = Some(8192);
    let b = body(&encode(Api::OpenAi, &r).unwrap());
    assert_eq!(b["stream_options"], json!({"include_usage": true}));
    assert_eq!(b["max_tokens"], json!(512));
    assert!(b.get("options").is_none() && b.get("num_ctx").is_none(), "the OpenAI format has no field for the context size: {b}");
    r.stream = false;
    assert!(body(&encode(Api::OpenAi, &r).unwrap()).get("stream_options").is_none(), "usage options belong to a stream");
}

#[test]
fn openai_tools_and_a_tool_round_use_the_documented_shapes() {
    let (m, t) = (round(), [tool("sessions"), tool("send")]);
    let b = body(&encode(Api::OpenAi, &request("qwen", &m, &t)).unwrap());
    assert_eq!(b["tools"][0], json!({"type": "function", "function": {"name": "sessions", "description": "the tool sessions", "parameters": t[0].parameters}}));
    assert_eq!(b["tools"].as_array().unwrap().len(), 2);
    assert_eq!(b["messages"][1], json!({"role": "assistant", "content": null, "tool_calls": [{"id": "call_1", "type": "function", "function": {"name": "sessions", "arguments": "{\"q\":\"x\"}"}}]}));
    assert_eq!(b["messages"][2], json!({"role": "tool", "tool_call_id": "call_1", "content": "[]"}));
    let none = body(&encode(Api::OpenAi, &request("qwen", &m, &[])).unwrap());
    assert!(none.get("tools").is_none(), "no tools, no field");
}

#[test]
fn an_assistant_with_text_and_calls_keeps_both_and_a_plain_answer_has_no_tool_calls_field() {
    let m = vec![
        Message::User("a".into()),
        Message::Assistant { text: "one moment".into(), calls: vec![ToolCall { id: "c".into(), name: "n".into(), arguments: json!({}) }] },
        Message::Assistant { text: "done".into(), calls: vec![] },
        Message::Assistant { text: String::new(), calls: vec![] },
    ];
    let b = body(&encode(Api::OpenAi, &request("m", &m, &[])).unwrap());
    assert_eq!(b["messages"][1]["content"], json!("one moment"));
    assert_eq!(b["messages"][2], json!({"role": "assistant", "content": "done"}));
    assert_eq!(b["messages"][3], json!({"role": "assistant", "content": ""}));
}

// ---- encode: the Ollama request --------------------------------------------------------------------------------------------

#[test]
fn an_ollama_request_has_options_only_when_given_and_arguments_as_an_object() {
    let (m, t) = (round(), [tool("sessions")]);
    let e = encode(Api::Ollama, &request("llama", &m, &t)).unwrap();
    assert_eq!(e.path, "/api/chat");
    let b = body(&e);
    assert_eq!(b["model"], json!("llama"));
    assert_eq!(b["stream"], json!(true));
    assert!(b.get("options").is_none() && b.get("stream_options").is_none());
    assert_eq!(b["tools"][0]["function"]["name"], json!("sessions"));
    assert_eq!(b["messages"][1], json!({"role": "assistant", "content": "", "tool_calls": [{"function": {"name": "sessions", "arguments": {"q": "x"}}}]}));
    assert_eq!(b["messages"][2], json!({"role": "tool", "tool_name": "sessions", "content": "[]"}));
    let mut r = request("llama", &m, &t);
    r.context_tokens = Some(4096);
    r.max_output_tokens = Some(256);
    r.include_usage = true;
    let b = body(&encode(Api::Ollama, &r).unwrap());
    assert_eq!(b["options"], json!({"num_ctx": 4096, "num_predict": 256}));
    r.context_tokens = None;
    assert_eq!(body(&encode(Api::Ollama, &r).unwrap())["options"], json!({"num_predict": 256}));
}

// ---- encode: wrong use and limits ------------------------------------------------------------------------------------------

#[test]
fn wrong_use_is_an_error_and_never_a_panic() {
    let m = msgs();
    for api in [Api::OpenAi, Api::Ollama] {
        assert_eq!(encode(api, &request("", &m, &[])).unwrap_err(), EncodeError::EmptyModel);
        assert_eq!(encode(api, &request("m", &[], &[])).unwrap_err(), EncodeError::NoMessages);
        let twice = [tool("a"), tool("b"), tool("a")];
        assert_eq!(encode(api, &request("m", &m, &twice)).unwrap_err(), EncodeError::DuplicateTool("a".into()));
        let huge = vec![Message::User("x".repeat(MAX_REQUEST_BYTES))];
        assert_eq!(encode(api, &request("m", &huge, &[])).unwrap_err(), EncodeError::TooLarge);
        let edge = vec![Message::User("x".repeat(MAX_REQUEST_BYTES - 1024))];
        assert!(encode(api, &request("m", &edge, &[])).unwrap().body.len() <= MAX_REQUEST_BYTES);
    }
}

#[test]
fn text_survives_the_round_trip_byte_for_byte_escapes_emoji_korean_and_control_characters() {
    let text = "quote \" backslash \\ newline \n tab \t nul \u{0} bell \u{7} esc \u{1b}[31m 한국어 😀 \u{2028} end";
    let m = vec![Message::User(text.into()), Message::Tool { call_id: "i".into(), name: "n".into(), content: text.into() }];
    for api in [Api::OpenAi, Api::Ollama] {
        let b = body(&encode(api, &request("m", &m, &[])).unwrap());
        assert_eq!(b["messages"][0]["content"].as_str().unwrap(), text, "{api:?}");
        assert_eq!(b["messages"][1]["content"].as_str().unwrap(), text, "{api:?}");
    }
}

#[test]
fn encoding_is_deterministic() {
    let (m, t) = (round(), [tool("a"), tool("b")]);
    for api in [Api::OpenAi, Api::Ollama] {
        let first = encode(api, &request("m", &m, &t)).unwrap();
        for _ in 0..3 {
            assert_eq!(encode(api, &request("m", &m, &t)).unwrap(), first);
        }
    }
}

// ---- the public names ------------------------------------------------------------------------------------------------------

/// The API lines of the specification, exactly: the names `src/lib.rs` makes public are these and no others.
#[test]
fn the_public_names_are_exactly_these() {
    let source = include_str!("../src/lib.rs");
    let mut found: Vec<String> = Vec::new();
    for line in source.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("pub use ") {
            let names = rest.split_once('{').map(|(_, n)| n.trim_end_matches(';').trim_end_matches('}')).unwrap_or(rest);
            found.extend(names.split(',').map(|n| n.trim().rsplit("::").next().unwrap().to_string()).filter(|n| !n.is_empty()));
        } else if line.starts_with("pub ") {
            panic!("an item is public outside the re-export list: {line}");
        }
    }
    found.sort();
    let mut expected: Vec<String> = ["encode", "CONTENT_TYPE", "MAX_REQUEST_BYTES", "Api", "EncodeError", "Encoded", "Message", "Request", "ToolCall", "ToolDef"].iter().map(|s| s.to_string()).collect();
    expected.sort();
    assert_eq!(found, expected);
}

#[test]
fn the_limits_have_the_values_of_the_specification() {
    assert_eq!(MAX_REQUEST_BYTES, 4 * 1024 * 1024);
}
