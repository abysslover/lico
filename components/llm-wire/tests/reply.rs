//! Reading replies and failures (SPEC.md: Decode, Events, Invariants I1 to I8, Failure). The samples in `tests/samples/` are written by hand from the public
//! documentation of the two APIs (KNOWN FROM DOCUMENTATION); no server was contacted.

use lico_llm_wire::*;
use serde_json::{json, Value};

fn sample(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/tests/samples/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn run_pieces<'a>(api: Api, framing: Framing, pieces: impl IntoIterator<Item = &'a [u8]>) -> Vec<Event> {
    let mut d = Decoder::new(api, framing);
    let mut out = Vec::new();
    for p in pieces {
        out.extend(d.feed(p));
    }
    out.extend(d.finish());
    out
}

fn run(api: Api, framing: Framing, bytes: &[u8]) -> Vec<Event> {
    run_pieces(api, framing, [bytes])
}

fn text(s: &str) -> Event {
    Event::Text(s.into())
}

fn call(id: &str, name: &str, arguments: Value) -> Event {
    Event::ToolCall(ToolCall { id: id.into(), name: name.into(), arguments })
}

fn done(reason: StopReason, usage: Option<(u64, u64)>) -> Event {
    Event::Done(Done { reason, usage: usage.map(|(p, c)| Usage { prompt_tokens: p, completion_tokens: c }) })
}

fn bad(events: &[Event]) -> Vec<(String, String, String)> {
    events.iter().filter_map(|e| if let Event::BadCall { id, name, raw, .. } = e { Some((id.clone(), name.clone(), raw.clone())) } else { None }).collect()
}

// ---- the samples, as they are -----------------------------------------------------------------------------------------------

#[test]
fn an_openai_stream_of_text_gives_its_pieces_and_the_usage_of_the_last_chunk() {
    let e = run(Api::OpenAi, Framing::Stream, &sample("openai_text.sse"));
    assert_eq!(e, vec![text("Hel"), text("lo "), text("한국어 😀"), done(StopReason::Stop, Some((12, 5)))]);
}

#[test]
fn openai_tool_calls_arrive_in_pieces_by_index_and_are_given_whole_just_before_the_end() {
    let e = run(Api::OpenAi, Framing::Stream, &sample("openai_tools.sse"));
    assert_eq!(e, vec![call("call_abc", "sessions", json!({"q": "x"})), call("call_def", "send", json!({"q": "y"})), done(StopReason::ToolCalls, None)]);
}

#[test]
fn reasoning_is_never_text_and_makes_one_thinking_marker() {
    for (api, name) in [(Api::OpenAi, "openai_reasoning.sse"), (Api::Ollama, "ollama_thinking.ndjson")] {
        let e = run(api, Framing::Stream, &sample(name));
        assert_eq!(e, vec![Event::Thinking, text("Answer"), done(StopReason::Stop, None)], "{name}");
    }
}

#[test]
fn comments_other_fields_and_a_data_line_without_the_space_are_read_as_server_sent_events() {
    let e = run(Api::OpenAi, Framing::Stream, &sample("openai_comments.sse"));
    assert_eq!(e, vec![text("A"), text("B"), done(StopReason::Stop, None)]);
}

#[test]
fn an_ollama_stream_gives_its_text_and_counts_and_generates_call_ids() {
    assert_eq!(run(Api::Ollama, Framing::Stream, &sample("ollama_text.ndjson")), vec![text("Hel"), text("lo "), text("한국어 😀"), done(StopReason::Stop, Some((12, 5)))]);
    let e = run(Api::Ollama, Framing::Stream, &sample("ollama_tools.ndjson"));
    assert_eq!(e, vec![call("call_1", "sessions", json!({"q": "x"})), call("call_2", "send", json!({"q": "y"})), done(StopReason::ToolCalls, Some((30, 9)))]);
}

#[test]
fn a_whole_body_gives_the_same_vocabulary() {
    let e = run(Api::OpenAi, Framing::Whole, &sample("openai_whole.json"));
    assert_eq!(e, vec![text("Hi there"), call("call_w1", "sessions", json!({"q": "x"})), done(StopReason::ToolCalls, Some((20, 7)))]);
    let e = run(Api::Ollama, Framing::Whole, &sample("ollama_whole.json"));
    assert_eq!(e, vec![text("Hi"), call("call_1", "sessions", json!({"q": "x"})), done(StopReason::ToolCalls, Some((4, 2)))]);
}

#[test]
fn an_error_inside_a_stream_ends_it_with_the_servers_failure() {
    let e = run(Api::OpenAi, Framing::Stream, &sample("openai_error_stream.sse"));
    assert!(matches!(&e[..], [Event::Error(StreamError::Server(f))] if f.kind == FailureKind::ModelMissing && f.message.contains("nope")), "{e:?}");
    let e = run(Api::Ollama, Framing::Stream, &sample("ollama_error.ndjson"));
    assert!(matches!(&e[..], [Event::Error(StreamError::Server(f))] if f.kind == FailureKind::ModelMissing && f.message.contains("nope")), "{e:?}");
}

#[test]
fn calls_with_bad_arguments_or_no_name_are_bad_calls_and_empty_arguments_are_an_empty_object() {
    let e = run(Api::OpenAi, Framing::Stream, &sample("openai_bad_args.sse"));
    assert_eq!(bad(&e), vec![("call_1".into(), "sessions".into(), "not json".into()), ("call_2".into(), "send".into(), "[1, 2]".into())]);
    assert!(e.contains(&call("call_3", "rename", json!({}))), "{e:?}");
    assert_eq!(e.last(), Some(&done(StopReason::ToolCalls, None)));
    let nameless = br#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"x","function":{"arguments":"{}"}}]},"finish_reason":"tool_calls"}]}

"#;
    let e = run(Api::OpenAi, Framing::Stream, nameless);
    assert_eq!(bad(&e).len(), 1, "{e:?}");
}

// ---- I1: any split gives the same events ---------------------------------------------------------------------------------------

fn crlf(bytes: &[u8]) -> Vec<u8> {
    String::from_utf8(bytes.to_vec()).unwrap().replace('\n', "\r\n").into_bytes()
}

fn every_split_equals_the_whole(api: Api, framing: Framing, bytes: &[u8], what: &str) {
    let whole = run(api, framing, bytes);
    for cut in 0..=bytes.len() {
        assert_eq!(run_pieces(api, framing, [&bytes[..cut], &bytes[cut..]]), whole, "{what}: split at {cut}");
    }
    assert_eq!(run_pieces(api, framing, bytes.chunks(1)), whole, "{what}: one byte at a time");
    for size in [2, 3, 7, 64] {
        assert_eq!(run_pieces(api, framing, bytes.chunks(size)), whole, "{what}: pieces of {size}");
    }
}

#[test]
fn every_two_way_split_and_the_one_byte_feed_of_every_sample_equal_the_whole_feed() {
    let streams = [
        (Api::OpenAi, "openai_text.sse"),
        (Api::OpenAi, "openai_tools.sse"),
        (Api::OpenAi, "openai_reasoning.sse"),
        (Api::OpenAi, "openai_comments.sse"),
        (Api::OpenAi, "openai_bad_args.sse"),
        (Api::OpenAi, "openai_error_stream.sse"),
        (Api::Ollama, "ollama_text.ndjson"),
        (Api::Ollama, "ollama_tools.ndjson"),
        (Api::Ollama, "ollama_thinking.ndjson"),
        (Api::Ollama, "ollama_error.ndjson"),
    ];
    for (api, name) in streams {
        let bytes = sample(name);
        every_split_equals_the_whole(api, Framing::Stream, &bytes, name);
        if api == Api::OpenAi || name.contains("ndjson") {
            every_split_equals_the_whole(api, Framing::Stream, &crlf(&bytes), &format!("{name} with CRLF"));
            assert_eq!(run(api, Framing::Stream, &crlf(&bytes)), run(api, Framing::Stream, &bytes), "{name}: CRLF reads as LF");
        }
    }
    for (api, name) in [(Api::OpenAi, "openai_whole.json"), (Api::Ollama, "ollama_whole.json")] {
        every_split_equals_the_whole(api, Framing::Whole, &sample(name), name);
    }
}

// ---- I4: one end, nothing after, and a cut stream gives no calls -------------------------------------------------------------

#[test]
fn a_cut_stream_ends_with_truncated_and_never_gives_a_call() {
    let tools = sample("openai_tools.sse");
    let cut = &tools[..tools.len() - 60];
    let e = run(Api::OpenAi, Framing::Stream, cut);
    assert_eq!(e, vec![Event::Error(StreamError::Truncated)], "no call is given from a stream that was cut");
    let text_stream = sample("ollama_text.ndjson");
    let lines: Vec<&[u8]> = text_stream.split_inclusive(|b| *b == b'\n').collect();
    let cut: Vec<u8> = lines[..3].concat();
    assert_eq!(run(Api::Ollama, Framing::Stream, &cut), vec![text("Hel"), text("lo "), text("한국어 😀"), Event::Error(StreamError::Truncated)]);
    assert_eq!(run(Api::OpenAi, Framing::Stream, b""), vec![Event::Error(StreamError::Truncated)]);
    assert_eq!(run(Api::Ollama, Framing::Whole, b""), vec![Event::Error(StreamError::Truncated)]);
    // an OpenAI stream that has its finish reason but lost [DONE] and the usage chunk is complete
    let full = sample("openai_text.sse");
    let marker = full.windows(8).position(|w| w == b"\"stop\"}]").unwrap();
    let upto = marker + full[marker..].windows(2).position(|w| w == b"\n\n").unwrap() + 2;
    assert_eq!(run(Api::OpenAi, Framing::Stream, &full[..upto]).last(), Some(&done(StopReason::Stop, None)));
}

#[test]
fn after_the_end_nothing_more_is_given() {
    for (api, name) in [(Api::OpenAi, "openai_text.sse"), (Api::Ollama, "ollama_text.ndjson"), (Api::OpenAi, "openai_error_stream.sse")] {
        let mut d = Decoder::new(api, Framing::Stream);
        let events = d.feed(&sample(name));
        assert!(matches!(events.last(), Some(Event::Done(_) | Event::Error(_))), "{name}");
        assert!(d.is_finished(), "{name}");
        assert!(d.feed(b"data: {\"choices\":[{\"delta\":{\"content\":\"late\"}}]}\n\n{\"message\":{\"content\":\"late\"}}\n").is_empty(), "{name}");
        assert!(d.finish().is_empty(), "{name}");
    }
}

#[test]
fn malformed_input_is_an_error_and_ends_the_decoder() {
    let mut d = Decoder::new(Api::OpenAi, Framing::Stream);
    let e = d.feed(b"data: {not json}\n\n");
    assert!(matches!(&e[..], [Event::Error(StreamError::Malformed(_))]), "{e:?}");
    assert!(d.is_finished() && d.finish().is_empty());
    let e = run(Api::Ollama, Framing::Stream, b"this is not json\n");
    assert!(matches!(&e[..], [Event::Error(StreamError::Malformed(_))]), "{e:?}");
    let e = run(Api::OpenAi, Framing::Whole, b"<html>502</html>");
    assert!(matches!(&e[..], [Event::Error(StreamError::Malformed(_))]), "{e:?}");
}

// ---- I2, I3, I5 to I8 ------------------------------------------------------------------------------------------------------------

#[test]
fn text_is_valid_utf8_even_when_the_server_sends_bad_bytes() {
    let mut line = br#"data: {"choices":[{"delta":{"content":"a"#.to_vec();
    line.push(0xFF);
    line.extend_from_slice(b"b\"}}]}\n\n");
    let e = run(Api::OpenAi, Framing::Stream, &line);
    assert_eq!(e[0], text("a\u{FFFD}b"));
    for cut in 0..=line.len() {
        assert_eq!(run_pieces(Api::OpenAi, Framing::Stream, [&line[..cut], &line[cut..]])[0], text("a\u{FFFD}b"));
    }
}

#[test]
fn the_limits_end_the_reply_with_too_large() {
    let long = vec![b'x'; MAX_LINE_BYTES + 1];
    for api in [Api::OpenAi, Api::Ollama] {
        let mut d = Decoder::new(api, Framing::Stream);
        assert_eq!(d.feed(&long), vec![Event::Error(StreamError::TooLarge)], "{api:?}");
        assert!(d.is_finished());
    }
    let whole = vec![b' '; MAX_WHOLE_BYTES + 1];
    assert_eq!(run(Api::OpenAi, Framing::Whole, &whole), vec![Event::Error(StreamError::TooLarge)]);
    // a line right at the limit is accepted
    let mut ok = br#"data: {"choices":[{"delta":{"content":""#.to_vec();
    ok.extend(std::iter::repeat(b'y').take(MAX_LINE_BYTES - 200));
    ok.extend_from_slice(b"\"}}]}\n\ndata: [DONE]\n\n");
    let e = run(Api::OpenAi, Framing::Stream, &ok);
    assert!(matches!(&e[0], Event::Text(t) if t.len() == MAX_LINE_BYTES - 200), "{}", e.len());
    // arguments over the limit, and more calls than the limit
    let piece = format!("{}", "z".repeat(1024));
    let mut args = String::new();
    for _ in 0..=(MAX_CALL_ARGUMENT_BYTES / 1024) {
        args.push_str(&format!("data: {{\"choices\":[{{\"delta\":{{\"tool_calls\":[{{\"index\":0,\"id\":\"a\",\"function\":{{\"name\":\"n\",\"arguments\":\"{piece}\"}}}}]}}}}]}}\n\n"));
    }
    assert!(matches!(run(Api::OpenAi, Framing::Stream, args.as_bytes()).last(), Some(Event::Error(StreamError::TooLarge))));
    let mut many = String::new();
    for i in 0..=MAX_CALLS {
        many.push_str(&format!("data: {{\"choices\":[{{\"delta\":{{\"tool_calls\":[{{\"index\":{i},\"id\":\"c{i}\",\"function\":{{\"name\":\"n\",\"arguments\":\"{{}}\"}}}}]}}}}]}}\n\n"));
    }
    assert!(matches!(run(Api::OpenAi, Framing::Stream, many.as_bytes()).last(), Some(Event::Error(StreamError::TooLarge))));
    let mut exactly = String::new();
    for i in 0..MAX_CALLS {
        exactly.push_str(&format!("data: {{\"choices\":[{{\"delta\":{{\"tool_calls\":[{{\"index\":{i},\"id\":\"c{i}\",\"function\":{{\"name\":\"n\",\"arguments\":\"{{}}\"}}}}]}},\"finish_reason\":\"tool_calls\"}}]}}\n\n"));
    }
    let e = run(Api::OpenAi, Framing::Stream, exactly.as_bytes());
    assert_eq!(e.iter().filter(|e| matches!(e, Event::ToolCall(_))).count(), MAX_CALLS, "the limit itself is allowed");
}

#[test]
fn call_ids_are_kept_when_plain_and_made_when_not_and_are_unique() {
    let long = "a".repeat(65);
    let body = format!(
        "data: {{\"choices\":[{{\"delta\":{{\"tool_calls\":[{{\"index\":0,\"id\":\"ok-1_x\",\"function\":{{\"name\":\"n\",\"arguments\":\"{{}}\"}}}},{{\"index\":1,\"id\":\"{long}\",\"function\":{{\"name\":\"n\",\"arguments\":\"{{}}\"}}}},{{\"index\":2,\"id\":\"has space\",\"function\":{{\"name\":\"n\",\"arguments\":\"{{}}\"}}}},{{\"index\":3,\"id\":\"ok-1_x\",\"function\":{{\"name\":\"n\",\"arguments\":\"{{}}\"}}}},{{\"index\":4,\"function\":{{\"name\":\"n\",\"arguments\":\"{{}}\"}}}}]}},\"finish_reason\":\"tool_calls\"}}]}}\n\n"
    );
    let e = run(Api::OpenAi, Framing::Stream, body.as_bytes());
    let ids: Vec<String> = e.iter().filter_map(|e| if let Event::ToolCall(c) = e { Some(c.id.clone()) } else { None }).collect();
    assert_eq!(ids[0], "ok-1_x");
    assert_eq!(ids.len(), 5);
    assert!(ids[1..].iter().all(|i| i.starts_with("call_")), "{ids:?}");
    let unique: std::collections::BTreeSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), ids.len(), "{ids:?}");
}

#[test]
fn the_reason_comes_from_the_finish_reason_and_calls_make_it_tool_calls() {
    let one = |finish: &str, with_call: bool| {
        let calls = if with_call { r#","tool_calls":[{"index":0,"id":"a","function":{"name":"n","arguments":"{}"}}]"# } else { "" };
        let body = format!("data: {{\"choices\":[{{\"delta\":{{\"content\":\"t\"{calls}}},\"finish_reason\":\"{finish}\"}}]}}\n\ndata: [DONE]\n\n");
        match run(Api::OpenAi, Framing::Stream, body.as_bytes()).last() {
            Some(Event::Done(d)) => d.reason,
            other => panic!("{other:?}"),
        }
    };
    assert_eq!(one("stop", false), StopReason::Stop);
    assert_eq!(one("length", false), StopReason::Length);
    assert_eq!(one("content_filter", false), StopReason::Other);
    assert_eq!(one("tool_calls", true), StopReason::ToolCalls);
    assert_eq!(one("stop", true), StopReason::ToolCalls, "some servers say stop when they made calls");
    let ollama = |reason: &str| {
        let body = format!("{{\"message\":{{\"content\":\"t\"}},\"done\":true,\"done_reason\":\"{reason}\"}}\n");
        match run(Api::Ollama, Framing::Stream, body.as_bytes()).last() {
            Some(Event::Done(d)) => d.reason,
            other => panic!("{other:?}"),
        }
    };
    assert_eq!((ollama("stop"), ollama("length"), ollama("load")), (StopReason::Stop, StopReason::Length, StopReason::Other));
}

#[test]
fn arguments_may_be_an_object_or_a_json_text_and_a_json_text_in_ollama_is_parsed() {
    let whole = br#"{"choices":[{"message":{"content":null,"tool_calls":[{"id":"a","function":{"name":"n","arguments":{"k":1}}}]},"finish_reason":"tool_calls"}]}"#;
    assert_eq!(run(Api::OpenAi, Framing::Whole, whole)[0], call("a", "n", json!({"k": 1})));
    let ollama = br#"{"message":{"content":"","tool_calls":[{"function":{"name":"n","arguments":"{\"k\": 2}"}}]},"done":true,"done_reason":"stop"}
"#;
    assert_eq!(run(Api::Ollama, Framing::Stream, ollama)[0], call("call_1", "n", json!({"k": 2})));
}

#[test]
fn decoding_is_deterministic_and_never_panics_on_any_input() {
    let samples = ["openai_text.sse", "openai_tools.sse", "openai_bad_args.sse", "ollama_text.ndjson", "ollama_tools.ndjson", "openai_whole.json", "ollama_whole.json"];
    for name in samples {
        let bytes = sample(name);
        let api = if name.starts_with("openai") { Api::OpenAi } else { Api::Ollama };
        let framing = if name.ends_with(".json") { Framing::Whole } else { Framing::Stream };
        assert_eq!(run(api, framing, &bytes), run(api, framing, &bytes), "{name}");
        // one byte changed at every place, and the sample cut at every place
        for at in 0..bytes.len() {
            let mut flipped = bytes.clone();
            flipped[at] ^= 0x5A;
            ends_well(&run(api, framing, &flipped));
            ends_well(&run(api, framing, &bytes[..at]));
        }
    }
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    for _ in 0..300 {
        let n = (seed % 400) as usize;
        let junk: Vec<u8> = (0..n).map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 24) as u8
        }).collect();
        for api in [Api::OpenAi, Api::Ollama] {
            for framing in [Framing::Stream, Framing::Whole] {
                ends_well(&run(api, framing, &junk));
            }
        }
    }
}

/// I4: exactly one end, and it is last; calls only just before `Done`.
fn ends_well(events: &[Event]) {
    let ends = events.iter().filter(|e| matches!(e, Event::Done(_) | Event::Error(_))).count();
    assert_eq!(ends, 1, "{events:?}");
    assert!(matches!(events.last(), Some(Event::Done(_) | Event::Error(_))), "{events:?}");
    if matches!(events.last(), Some(Event::Error(_))) {
        assert!(!events.iter().any(|e| matches!(e, Event::ToolCall(_) | Event::BadCall { .. })), "a call before an error: {events:?}");
    }
    assert!(events.iter().filter(|e| matches!(e, Event::Thinking)).count() <= 1);
}

// ---- failures --------------------------------------------------------------------------------------------------------------------

#[test]
fn a_failure_has_a_kind_from_the_status_and_the_words_and_the_servers_text_cleaned() {
    let openai = |msg: &str, code: &str| json!({"error": {"message": msg, "type": "invalid_request_error", "code": code}}).to_string();
    let table: Vec<(Api, u16, String, FailureKind)> = vec![
        (Api::OpenAi, 401, openai("Incorrect API key provided", "invalid_api_key"), FailureKind::KeyNeeded),
        (Api::OpenAi, 403, String::new(), FailureKind::KeyNeeded),
        (Api::OpenAi, 0, openai("nope", "invalid_api_key"), FailureKind::KeyNeeded),
        (Api::Ollama, 404, json!({"error": "model 'x' not found"}).to_string(), FailureKind::ModelMissing),
        (Api::OpenAi, 404, openai("The model `x` does not exist", "model_not_found"), FailureKind::ModelMissing),
        (Api::OpenAi, 400, openai("registry.ollama.ai/library/gemma:latest does not support tools", ""), FailureKind::ToolsUnsupported),
        (Api::Ollama, 400, json!({"error": "this model does not support tools"}).to_string(), FailureKind::ToolsUnsupported),
        (Api::OpenAi, 400, openai("This model's maximum context length is 4096 tokens. However, your messages resulted in 5000 tokens.", "context_length_exceeded"), FailureKind::ContextTooLong),
        (Api::Ollama, 400, json!({"error": "prompt is too long: 9000 tokens > 8192"}).to_string(), FailureKind::ContextTooLong),
        (Api::OpenAi, 429, openai("rate limited", "rate_limit_exceeded"), FailureKind::Busy),
        (Api::Ollama, 503, "server busy".into(), FailureKind::Busy),
        (Api::OpenAi, 500, "boom".into(), FailureKind::ServerError),
        (Api::OpenAi, 400, openai("bad field", ""), FailureKind::BadRequest),
        (Api::Ollama, 422, String::new(), FailureKind::BadRequest),
        (Api::OpenAi, 302, String::new(), FailureKind::Unknown),
        (Api::Ollama, 0, json!({"error": "disk on fire"}).to_string(), FailureKind::Unknown),
    ];
    for (api, status, body, kind) in table {
        assert_eq!(classify(api, status, body.as_bytes()).kind, kind, "{api:?} {status} {body}");
    }
    let f = classify(Api::Ollama, 404, json!({"error": "model 'x' not found"}).to_string().as_bytes());
    assert_eq!(f.message, "model 'x' not found");
    let f = classify(Api::OpenAi, 500, "line one\nline\ttwo \u{1b}[31mred\u{7}".as_bytes());
    assert_eq!(f.message, "line one line two [31mred", "newlines and tabs become spaces, other control characters go");
    let f = classify(Api::OpenAi, 500, "x".repeat(1000).as_bytes());
    assert_eq!(f.message.chars().count(), 300);
    let f = classify(Api::OpenAi, 500, "한".repeat(1000).as_bytes());
    assert_eq!(f.message.chars().count(), 300, "the cut is by characters, never inside one");
    assert_eq!(classify(Api::OpenAi, 500, &[0xFF, b'a']).message, "\u{FFFD}a");
}
