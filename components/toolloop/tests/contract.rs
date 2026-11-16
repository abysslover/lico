//! The contract tests of `lico-toolloop` (SPEC.md) on scripted ports: no server, no files, no clock. The end-to-end test through the real client is in `end_to_end.rs`.

use lico_llm_wire::{Done, Event, Failure, FailureKind, Message, StopReason, StreamError, ToolCall, ToolDef, Usage};
use lico_toolloop::*;
use serde_json::{json, Value};
use std::cell::Cell;
use std::collections::VecDeque;

// ---- the scripted ports ------------------------------------------------------------------------------------------------------------

enum Reply {
    Events(Vec<Event>),
    Fail(ModelError),
}

#[derive(Default)]
struct Model {
    script: VecDeque<Reply>,
    asked: Vec<(Vec<Message>, usize)>,
    delivered: usize,
}

impl ModelPort for Model {
    fn turn(&mut self, messages: &[Message], tools: &[ToolDef], sink: &mut dyn FnMut(Event) -> bool) -> Result<(), ModelError> {
        self.asked.push((messages.to_vec(), tools.len()));
        match self.script.pop_front().expect("the model was asked more often than the script says") {
            Reply::Fail(e) => Err(e),
            Reply::Events(events) => {
                for e in events {
                    self.delivered += 1;
                    if !sink(e) {
                        break;
                    }
                }
                Ok(())
            }
        }
    }
}

struct Tools<'a> {
    calls: Vec<ToolCall>,
    answer: Box<dyn FnMut(&ToolCall) -> ToolOutcome + 'a>,
}

impl ToolPort for Tools<'_> {
    fn call(&mut self, call: &ToolCall) -> ToolOutcome {
        self.calls.push(call.clone());
        (self.answer)(call)
    }
}

#[derive(Default)]
struct Rec {
    messages: Vec<Message>,
    fail_at: Option<usize>,
}

impl RecordPort for Rec {
    fn append(&mut self, m: &Message) -> Result<(), String> {
        if self.fail_at == Some(self.messages.len()) {
            return Err("disk full".into());
        }
        self.messages.push(m.clone());
        Ok(())
    }
}

#[derive(Default)]
struct Clock {
    now: Cell<u64>,
    cancel: Cell<bool>,
    /// The `n`th question about cancelling (1-based) is answered yes.
    cancel_at_ask: Cell<Option<usize>>,
    asks: Cell<usize>,
}

impl Env for Clock {
    fn cancelled(&self) -> bool {
        self.asks.set(self.asks.get() + 1);
        self.cancel.get() || self.cancel_at_ask.get().is_some_and(|n| self.asks.get() >= n)
    }
    fn now_ms(&self) -> u64 {
        self.now.get()
    }
}

struct Run {
    end: TurnEnd,
    out: Vec<Out>,
    history: Vec<Message>,
    record: Vec<Message>,
    tool_calls: Vec<ToolCall>,
    model: Model,
}

fn text(t: &str) -> Event {
    Event::Text(t.into())
}

fn call(id: &str, name: &str, args: Value) -> Event {
    Event::ToolCall(ToolCall { id: id.into(), name: name.into(), arguments: args })
}

fn done(reason: StopReason) -> Event {
    Event::Done(Done { reason, usage: Some(Usage { prompt_tokens: 1, completion_tokens: 1 }) })
}

fn def(name: &str) -> ToolDef {
    ToolDef { name: name.into(), description: format!("the tool {name}"), parameters: json!({"type": "object"}) }
}

fn config() -> Config {
    Config { tools: vec![def("sessions"), def("send"), def("rename")], limits: Limits::default() }
}

fn user(t: &str) -> Message {
    Message::User(t.into())
}

/// Runs one turn with `script` as the model's answers, `answer` as the host's answers to calls, and the given clock and record.
fn run_with(script: Vec<Reply>, config: &Config, clock: &Clock, rec: Rec, prior: Vec<Message>, answer: impl FnMut(&ToolCall) -> ToolOutcome) -> Run {
    let mut model = Model { script: script.into(), ..Default::default() };
    let mut tools = Tools { calls: Vec::new(), answer: Box::new(answer) };
    let mut rec = rec;
    let mut history = prior;
    let mut out = Vec::new();
    let end = {
        let mut ports = Ports { model: &mut model, tools: &mut tools, record: &mut rec, env: clock };
        run_turn(&mut ports, &mut history, user("hello"), config, &mut |o| out.push(o))
    };
    let tool_calls = tools.calls;
    Run { end, out, history, record: rec.messages, tool_calls, model }
}

fn run(script: Vec<Reply>, answer: impl FnMut(&ToolCall) -> ToolOutcome) -> Run {
    run_with(script, &config(), &Clock::default(), Rec::default(), Vec::new(), answer)
}

fn ok(t: &str) -> ToolOutcome {
    ToolOutcome::Done(t.into())
}

fn results(r: &Run) -> Vec<(String, String)> {
    r.record.iter().filter_map(|m| if let Message::Tool { call_id, content, .. } = m { Some((call_id.clone(), content.clone())) } else { None }).collect()
}

fn calls_of(m: &Message) -> Vec<String> {
    if let Message::Assistant { calls, .. } = m { calls.iter().map(|c| c.id.clone()).collect() } else { Vec::new() }
}

// ---- the plain turn and the tool round ---------------------------------------------------------------------------------------------

#[test]
fn a_plain_answer_is_shown_as_it_comes_recorded_and_ends_the_turn() {
    let r = run(vec![Reply::Events(vec![Event::Thinking, text("Hel"), text("lo"), done(StopReason::Stop)])], |_| ok(""));
    assert_eq!(r.end, TurnEnd::Finished(StopReason::Stop));
    assert_eq!(r.out, vec![Out::Thinking, Out::Text("Hel".into()), Out::Text("lo".into())]);
    let expected = vec![user("hello"), Message::Assistant { text: "Hello".into(), calls: vec![] }];
    assert_eq!((&r.record, &r.history), (&expected, &expected));
    assert_eq!(r.model.asked.len(), 1);
    assert_eq!((r.model.asked[0].0.clone(), r.model.asked[0].1), (vec![user("hello")], 3), "the model gets the history and the offered tools");
    assert!(r.tool_calls.is_empty());
}

#[test]
fn a_tool_round_runs_the_call_records_its_result_and_asks_the_model_again_with_everything() {
    let r = run(
        vec![
            Reply::Events(vec![text("one moment"), call("c1", "sessions", json!({"q": 1})), done(StopReason::ToolCalls)]),
            Reply::Events(vec![text("done"), done(StopReason::Stop)]),
        ],
        |c| {
            assert_eq!((c.name.as_str(), c.id.as_str()), ("sessions", "c1"));
            ok("[]")
        },
    );
    assert_eq!(r.end, TurnEnd::Finished(StopReason::Stop));
    let first = Message::Assistant { text: "one moment".into(), calls: vec![ToolCall { id: "c1".into(), name: "sessions".into(), arguments: json!({"q": 1}) }] };
    let result = Message::Tool { call_id: "c1".into(), name: "sessions".into(), content: "[]".into() };
    let expected = vec![user("hello"), first, result, Message::Assistant { text: "done".into(), calls: vec![] }];
    assert_eq!((&r.record, &r.history), (&expected, &expected));
    assert_eq!(r.model.asked[1].0, expected[..3].to_vec(), "the second request has the call and its result");
    assert_eq!(r.out, vec![Out::Text("one moment".into()), Out::Tool { name: "sessions".into(), call_id: "c1".into(), outcome: Outcome::Done }, Out::Text("done".into())]);
}

#[test]
fn history_before_this_turn_goes_to_the_model_and_the_record_gets_only_what_is_new() {
    let prior = vec![user("earlier"), Message::Assistant { text: "yes".into(), calls: vec![] }];
    let r = run_with(vec![Reply::Events(vec![text("ok"), done(StopReason::Stop)])], &config(), &Clock::default(), Rec::default(), prior.clone(), |_| ok(""));
    assert_eq!(r.model.asked[0].0, [prior.clone(), vec![user("hello")]].concat());
    assert_eq!(r.record.len(), 2, "the record is appended to, not rewritten");
    assert_eq!(r.history.len(), 4);
}

#[test]
fn several_calls_in_one_answer_run_in_order_each_with_one_result() {
    let r = run(
        vec![
            Reply::Events(vec![call("a", "sessions", json!({})), call("b", "rename", json!({"name": "x"})), call("c", "sessions", json!({})), done(StopReason::ToolCalls)]),
            Reply::Events(vec![text("ok"), done(StopReason::Stop)]),
        ],
        |c| ok(&format!("result of {}", c.id)),
    );
    assert_eq!(r.tool_calls.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
    assert_eq!(results(&r), vec![("a".into(), "result of a".into()), ("b".into(), "result of b".into()), ("c".into(), "result of c".into())]);
    assert_eq!(calls_of(&r.record[1]), ["a", "b", "c"]);
}

// ---- waiting for the person -------------------------------------------------------------------------------------------------------------

#[test]
fn a_waiting_tool_ends_the_turn_after_every_call_of_the_answer_has_its_result_and_the_model_is_not_asked_again() {
    let r = run(
        vec![Reply::Events(vec![call("a", "send", json!({"to": "x"})), call("b", "sessions", json!({})), done(StopReason::ToolCalls)])],
        |c| if c.name == "send" { ToolOutcome::Waiting("asked: waiting for the person".into()) } else { ok("[]") },
    );
    assert_eq!(r.end, TurnEnd::Waiting);
    assert_eq!(r.model.asked.len(), 1);
    assert_eq!(results(&r), vec![("a".into(), "asked: waiting for the person".into()), ("b".into(), "[]".into())], "the other call of the answer still ran");
    assert_eq!(r.out[0], Out::Tool { name: "send".into(), call_id: "a".into(), outcome: Outcome::Waiting });
    assert_eq!(r.out[1], Out::Tool { name: "sessions".into(), call_id: "b".into(), outcome: Outcome::Done });
}

#[test]
fn a_refused_tool_gives_its_word_back_to_the_model_and_the_turn_goes_on() {
    let r = run(
        vec![Reply::Events(vec![call("a", "send", json!({})), done(StopReason::ToolCalls)]), Reply::Events(vec![text("ok"), done(StopReason::Stop)])],
        |_| ToolOutcome::Refused("the host says no".into()),
    );
    assert_eq!(r.end, TurnEnd::Finished(StopReason::Stop));
    assert_eq!(results(&r), vec![("a".into(), "the host says no".into())]);
    assert_eq!(r.out[0], Out::Tool { name: "send".into(), call_id: "a".into(), outcome: Outcome::Refused });
}

// ---- what never reaches the host ---------------------------------------------------------------------------------------------------------

#[test]
fn a_tool_that_was_not_offered_and_a_call_with_bad_arguments_never_reach_the_host() {
    let bad = Event::BadCall { id: "b".into(), name: "sessions".into(), raw: "not json".into(), reason: "the arguments are not valid JSON".into() };
    let nameless = Event::BadCall { id: "n".into(), name: String::new(), raw: String::new(), reason: "the call has no name".into() };
    let r = run(
        vec![
            Reply::Events(vec![call("a", "shell", json!({"cmd": "rm -rf /"})), bad, nameless, call("d", "read_file", json!({})), done(StopReason::ToolCalls)]),
            Reply::Events(vec![text("ok"), done(StopReason::Stop)]),
        ],
        |_| panic!("the host must not be called"),
    );
    assert_eq!(r.end, TurnEnd::Finished(StopReason::Stop));
    assert!(r.tool_calls.is_empty());
    let res = results(&r);
    assert_eq!(res.iter().map(|(i, _)| i.as_str()).collect::<Vec<_>>(), ["a", "b", "n", "d"], "every call has exactly one result, in order");
    assert!(res[0].1.contains("shell") && res[0].1.contains("not offered"), "{}", res[0].1);
    assert!(res[1].1.contains("not valid JSON"), "{}", res[1].1);
    assert!(res[2].1.contains("no name"), "{}", res[2].1);
    assert!(r.out.iter().filter(|o| matches!(o, Out::Tool { outcome: Outcome::Refused, .. })).count() == 4);
    assert_eq!(calls_of(&r.record[1]), ["a", "b", "n", "d"]);
    if let Message::Assistant { calls, .. } = &r.record[1] {
        assert_eq!(calls[2].name, "unknown", "a call that has no name is recorded under a plain name");
        assert_eq!(calls[1].arguments, json!({}), "a call with bad arguments is kept with an empty object, not with what the model sent");
    }
}

#[test]
fn chat_only_a_session_with_no_tools_offers_none_and_refuses_every_call() {
    let cfg = Config { tools: vec![], limits: Limits::default() };
    let r = run_with(
        vec![Reply::Events(vec![call("a", "sessions", json!({})), done(StopReason::ToolCalls)]), Reply::Events(vec![text("ok"), done(StopReason::Stop)])],
        &cfg,
        &Clock::default(),
        Rec::default(),
        vec![],
        |_| panic!("no tool is offered"),
    );
    assert_eq!((r.model.asked[0].1, r.model.asked[1].1), (0, 0));
    assert!(results(&r)[0].1.contains("not offered"));
    assert_eq!(r.end, TurnEnd::Finished(StopReason::Stop));
}

// ---- the limits, each at its edge ----------------------------------------------------------------------------------------------------------

fn calling(n: usize) -> Reply {
    Reply::Events(vec![call(&format!("c{n}"), "sessions", json!({})), done(StopReason::ToolCalls)])
}

#[test]
fn the_round_limit_ends_the_turn_after_exactly_that_many_requests() {
    let mut cfg = config();
    cfg.limits.max_rounds = 3;
    let r = run_with((0..5).map(calling).collect(), &cfg, &Clock::default(), Rec::default(), vec![], |_| ok("x"));
    assert_eq!(r.end, TurnEnd::Limit(LimitKind::Rounds));
    assert_eq!(r.model.asked.len(), 3);
    assert_eq!(results(&r).len(), 3, "the calls of the last round got their results");
    let finishing = vec![calling(0), calling(1), Reply::Events(vec![text("done"), done(StopReason::Stop)])];
    let r = run_with(finishing, &cfg, &Clock::default(), Rec::default(), vec![], |_| ok("x"));
    assert_eq!((r.end, r.model.asked.len()), (TurnEnd::Finished(StopReason::Stop), 3), "an answer in the last allowed round is not a limit");
}

#[test]
fn the_calls_of_one_answer_and_of_one_turn_are_limited_and_the_rest_is_refused_with_a_word() {
    let many = |from: usize, n: usize| Reply::Events((from..from + n).map(|i| call(&format!("c{i}"), "sessions", json!({}))).chain([done(StopReason::ToolCalls)]).collect());
    let mut cfg = config();
    cfg.limits.max_calls_per_answer = 4;
    cfg.limits.max_calls_per_turn = 16;
    let r = run_with(vec![many(0, 6), Reply::Events(vec![text("ok"), done(StopReason::Stop)])], &cfg, &Clock::default(), Rec::default(), vec![], |_| ok("ran"));
    assert_eq!(r.tool_calls.len(), 4);
    let res = results(&r);
    assert_eq!(res.len(), 6);
    assert!(res[..4].iter().all(|(_, c)| c == "ran") && res[4].1.contains("not run") && res[5].1.contains("not run"), "{res:?}");
    assert_eq!(r.end, TurnEnd::Finished(StopReason::Stop), "the model can go on and adapt");
    cfg.limits.max_calls_per_turn = 5;
    let r = run_with(vec![many(0, 3), many(3, 3), many(6, 3)], &cfg, &Clock::default(), Rec::default(), vec![], |_| ok("ran"));
    assert_eq!(r.end, TurnEnd::Limit(LimitKind::CallsPerTurn));
    assert_eq!(r.tool_calls.len(), 5, "five calls were run, the sixth was refused");
    assert_eq!(r.model.asked.len(), 2, "the model is not asked a third time");
    assert_eq!(results(&r).len(), 6, "and every call has its result");
}

#[test]
fn a_result_is_cut_at_the_limit_with_a_note_and_never_inside_a_character() {
    let mut cfg = config();
    cfg.limits.max_result_bytes = 100;
    let two = || vec![Reply::Events(vec![call("a", "sessions", json!({})), done(StopReason::ToolCalls)]), Reply::Events(vec![text("ok"), done(StopReason::Stop)])];
    let exact = "x".repeat(100);
    let r = run_with(two(), &cfg, &Clock::default(), Rec::default(), vec![], |_| ok(&exact));
    assert_eq!(results(&r)[0].1, exact, "a result of exactly the limit is kept whole");
    let r = run_with(two(), &cfg, &Clock::default(), Rec::default(), vec![], |_| ok(&"y".repeat(101)));
    let cut = &results(&r)[0].1;
    assert!(cut.len() <= 100 && cut.contains("cut") && cut.starts_with("yyyy"), "{cut:?}");
    let r = run_with(two(), &cfg, &Clock::default(), Rec::default(), vec![], |_| ok(&"한".repeat(100)));
    let cut = &results(&r)[0].1;
    assert!(cut.len() <= 100 && cut.starts_with('한'), "{cut:?}");
    let r = run_with(two(), &cfg, &Clock::default(), Rec::default(), vec![], |_| ToolOutcome::Waiting("w".repeat(500)));
    assert!(results(&r)[0].1.len() <= 100, "a waiting text is cut too");
}

#[test]
fn arguments_over_the_limit_are_refused_and_not_kept_in_the_record() {
    let mut cfg = config();
    cfg.limits.max_args_bytes = 50;
    let big = json!({"q": "z".repeat(60)});
    let fits = json!({"q": "z".repeat(40)});
    let exact = json!({"q": "z".repeat(42)});
    assert!(fits.to_string().len() <= 50 && big.to_string().len() > 50 && exact.to_string().len() == 50);
    let r = run_with(
        vec![Reply::Events(vec![call("a", "sessions", big), call("b", "sessions", fits.clone()), call("e", "sessions", exact.clone()), done(StopReason::ToolCalls)]), Reply::Events(vec![text("ok"), done(StopReason::Stop)])],
        &cfg,
        &Clock::default(),
        Rec::default(),
        vec![],
        |_| ok("ran"),
    );
    assert_eq!(r.tool_calls.len(), 2, "arguments of exactly the limit run");
    assert_eq!((r.tool_calls[0].arguments.clone(), r.tool_calls[1].arguments.clone()), (fits.clone(), exact));
    assert!(results(&r)[0].1.contains("larger"), "{:?}", results(&r));
    if let Message::Assistant { calls, .. } = &r.record[1] {
        assert_eq!((calls[0].arguments.clone(), calls[1].arguments.clone()), (json!({}), fits));
    }
}

#[test]
fn the_time_limit_is_checked_before_each_round_and_each_call_and_the_rest_get_a_result() {
    let clock = Clock::default();
    let cfg = config();
    let r = {
        let clock = &clock;
        run_with(
            vec![Reply::Events(vec![call("a", "sessions", json!({})), call("b", "sessions", json!({})), call("c", "sessions", json!({})), done(StopReason::ToolCalls)])],
            &cfg,
            clock,
            Rec::default(),
            vec![],
            move |_| {
                clock.now.set(clock.now.get() + 300_000);
                ok("ran")
            },
        )
    };
    assert_eq!(r.end, TurnEnd::Limit(LimitKind::Time));
    assert_eq!(r.tool_calls.len(), 2, "300 s each: the third call would start at the limit");
    let res = results(&r);
    assert_eq!(res.len(), 3);
    assert!(res[2].1.contains("not run") && res[2].1.contains("time"), "{res:?}");
    assert_eq!(r.model.asked.len(), 1);
    // one millisecond under the limit the next round still starts; at the limit it does not
    for (at, expected_rounds, finished) in [(599_999u64, 2usize, true), (600_000, 1, false)] {
        let clock = Clock::default();
        let r = {
            let clock = &clock;
            run_with(
                vec![Reply::Events(vec![call("a", "sessions", json!({})), done(StopReason::ToolCalls)]), Reply::Events(vec![text("ok"), done(StopReason::Stop)])],
                &cfg,
                clock,
                Rec::default(),
                vec![],
                move |_| {
                    clock.now.set(at);
                    ok("ran")
                },
            )
        };
        assert_eq!(r.model.asked.len(), expected_rounds, "at {at} ms");
        assert_eq!(r.end, if finished { TurnEnd::Finished(StopReason::Stop) } else { TurnEnd::Limit(LimitKind::Time) });
    }
}

#[test]
fn an_answer_over_the_size_limit_stops_the_read_and_is_not_recorded() {
    let mut cfg = config();
    cfg.limits.max_answer_bytes = 10;
    let r = run_with(vec![Reply::Events(vec![text("12345"), text("67890"), text("x"), text("never read"), done(StopReason::Stop)])], &cfg, &Clock::default(), Rec::default(), vec![], |_| ok(""));
    assert_eq!(r.end, TurnEnd::Limit(LimitKind::AnswerSize));
    assert_eq!(r.model.delivered, 3, "the read stopped at the piece that went over");
    assert_eq!(r.record, vec![user("hello")], "the answer was not kept");
    let exact = run_with(vec![Reply::Events(vec![text("12345"), text("67890"), done(StopReason::Stop)])], &cfg, &Clock::default(), Rec::default(), vec![], |_| ok(""));
    assert_eq!(exact.end, TurnEnd::Finished(StopReason::Stop), "an answer of exactly the limit is fine");
}

// ---- failures, cancel ------------------------------------------------------------------------------------------------------------------------

#[test]
fn a_cut_or_failed_answer_is_shown_as_far_as_it_came_not_recorded_and_runs_no_call() {
    let cases: Vec<Vec<Event>> = vec![
        vec![text("par"), Event::Error(StreamError::Truncated)],
        vec![text("par"), call("a", "sessions", json!({})), Event::Error(StreamError::Truncated)],
        vec![text("par"), call("a", "sessions", json!({})), Event::Error(StreamError::Malformed("x".into()))],
        vec![text("par"), call("a", "sessions", json!({})), Event::Error(StreamError::TooLarge)],
        vec![text("par"), call("a", "sessions", json!({}))],
    ];
    for events in cases {
        let r = run(vec![Reply::Events(events)], |_| panic!("no call may run"));
        assert!(matches!(r.end, TurnEnd::Failed(Failed::Stream(_))), "{:?}", r.end);
        assert_eq!(r.out, vec![Out::Text("par".into())]);
        assert_eq!(r.record, vec![user("hello")]);
        assert_eq!(r.history, vec![user("hello")]);
        assert_eq!(r.model.asked.len(), 1);
    }
    let server = Failure { kind: FailureKind::ModelMissing, message: "no such model".into() };
    let r = run(vec![Reply::Events(vec![Event::Error(StreamError::Server(server.clone()))])], |_| ok(""));
    assert_eq!(r.end, TurnEnd::Failed(Failed::Stream(StreamError::Server(server))));
}

#[test]
fn a_port_error_ends_the_turn_and_a_cancelled_port_is_a_cancel() {
    let f = Failure { kind: FailureKind::KeyNeeded, message: "key".into() };
    let r = run(vec![Reply::Fail(ModelError::Failure(f.clone()))], |_| ok(""));
    assert_eq!(r.end, TurnEnd::Failed(Failed::Model(f)));
    for e in [ModelError::Gone, ModelError::Timeout, ModelError::TooBig { needed: 9000, window: 8192 }, ModelError::Other("x".into())] {
        let r = run(vec![Reply::Fail(e.clone())], |_| ok(""));
        assert_eq!(r.end, TurnEnd::Failed(Failed::Port(e)));
        assert_eq!(r.record, vec![user("hello")]);
    }
    let r = run(vec![Reply::Fail(ModelError::Cancelled)], |_| ok(""));
    assert_eq!(r.end, TurnEnd::Cancelled);
}

#[test]
fn a_failed_record_stops_everything_before_anything_runs() {
    for (fail_at, expect_calls) in [(0usize, 0usize), (1, 0), (2, 1)] {
        let rec = Rec { fail_at: Some(fail_at), ..Default::default() };
        let r = run_with(vec![Reply::Events(vec![call("a", "sessions", json!({})), done(StopReason::ToolCalls)])], &config(), &Clock::default(), rec, vec![], |_| ok("ran"));
        assert_eq!(r.end, TurnEnd::Failed(Failed::Record("disk full".into())), "fail at {fail_at}");
        assert_eq!(r.tool_calls.len(), expect_calls, "fail at {fail_at}: a tool runs only after its request was recorded");
        if fail_at == 0 {
            assert!(r.model.asked.is_empty(), "the person's message was not recorded: the model is not asked");
            assert!(r.history.is_empty());
        }
    }
}

#[test]
fn a_cancel_before_a_round_between_calls_and_during_the_read_ends_the_turn_and_keeps_the_record_whole() {
    // before the first round
    let clock = Clock::default();
    clock.cancel.set(true);
    let r = run_with(vec![], &config(), &clock, Rec::default(), vec![], |_| ok(""));
    assert_eq!((r.end, r.model.asked.len(), r.record.clone()), (TurnEnd::Cancelled, 0, vec![user("hello")]));
    // during the read: the sink is told to stop and the answer is not kept
    let clock = Clock::default();
    clock.cancel_at_ask.set(Some(2));
    let r = run_with(vec![Reply::Events(vec![text("a"), text("b"), text("c"), done(StopReason::Stop)])], &config(), &clock, Rec::default(), vec![], |_| ok(""));
    assert_eq!(r.end, TurnEnd::Cancelled);
    assert!(r.model.delivered < 4 && r.record == vec![user("hello")], "{} {:?}", r.model.delivered, r.record);
    // between calls: the questions about cancelling are 1 (before the round), 2 to 5 (one per event of the answer), 6 to 8 (before each call), 9 (before the next round)
    for at in 6..=9 {
        let clock = Clock::default();
        clock.cancel_at_ask.set(Some(at));
        let r = run_with(
            vec![Reply::Events(vec![call("a", "sessions", json!({})), call("b", "sessions", json!({})), call("c", "sessions", json!({})), done(StopReason::ToolCalls)])],
            &config(),
            &clock,
            Rec::default(),
            vec![],
            |_| ok("ran"),
        );
        assert_eq!(r.end, TurnEnd::Cancelled, "ask {at}");
        assert_eq!(r.tool_calls.len(), at - 6, "ask {at}: the calls before the cancel ran");
        let res = results(&r);
        assert_eq!(res.len(), 3, "ask {at}: every call has a result, so the record can be sent to a model again");
        assert_eq!(res.iter().filter(|(_, c)| c.contains("not run") && c.contains("cancelled")).count(), 9 - at.max(6).min(9) as usize, "ask {at}: {res:?}");
        assert_eq!(r.model.asked.len(), 1);
    }
}

// ---- the whole, the public names, what the crate does not use -----------------------------------------------------------------------------------

#[test]
fn the_same_ports_and_input_give_the_same_turn() {
    let script = || vec![Reply::Events(vec![text("a"), call("c", "sessions", json!({"k": [1, 2]})), done(StopReason::ToolCalls)]), Reply::Events(vec![text("b"), done(StopReason::Stop)])];
    let (a, b) = (run(script(), |_| ok("r")), run(script(), |_| ok("r")));
    assert_eq!((a.end, a.out, a.record), (b.end, b.out, b.record));
}

#[test]
fn the_public_names_and_the_default_limits_are_the_ones_of_the_specification() {
    let source = include_str!("../src/lib.rs");
    let mut found: Vec<String> = Vec::new();
    for line in source.lines() {
        if line.starts_with("pub ") {
            let rest = line.trim_start_matches("pub ").trim_start_matches("const ").trim_start_matches("fn ").trim_start_matches("struct ").trim_start_matches("enum ").trim_start_matches("trait ");
            found.push(rest.split(|c: char| !c.is_alphanumeric() && c != '_').next().unwrap().to_string());
        }
    }
    found.sort();
    let mut expected: Vec<String> = ["Limits", "Config", "ToolOutcome", "Outcome", "ModelError", "ModelPort", "ToolPort", "RecordPort", "Env", "Ports", "Out", "LimitKind", "Failed", "TurnEnd", "run_turn"].iter().map(|s| s.to_string()).collect();
    expected.sort();
    assert_eq!(found, expected);
    let d = Limits::default();
    assert_eq!((d.max_rounds, d.max_calls_per_answer, d.max_calls_per_turn, d.max_result_bytes, d.max_args_bytes, d.max_answer_bytes, d.turn_ms), (8, 4, 16, 8192, 8192, 262_144, 600_000));
}

#[test]
fn the_crate_has_no_way_to_reach_a_file_a_network_a_process_the_environment_or_the_clock() {
    let manifest = include_str!("../Cargo.toml");
    let deps: Vec<&str> = manifest.split("[dependencies]").nth(1).unwrap().lines().skip(1).take_while(|l| !l.starts_with('[')).filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(deps.len(), 2, "{deps:?}");
    assert!(deps.iter().any(|d| d.starts_with("lico-llm-wire ")) && deps.iter().any(|d| d.starts_with("serde_json ")), "{deps:?}");
    for entry in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/src")).unwrap() {
        let path = entry.unwrap().path();
        let code = std::fs::read_to_string(&path).unwrap();
        for (n, line) in code.lines().enumerate() {
            let line = line.trim();
            if line.starts_with("//") {
                continue;
            }
            for word in ["std::fs", "std::net", "std::process", "std::env", "std::thread", "std::time", "std::io", "unsafe", "Instant", "SystemTime", "static ", "thread_local", "File::", "TcpStream", "Command"] {
                assert!(!line.contains(word) || word == "static " && line.contains("'static"), "{}:{}: `{word}` in {line}", path.display(), n + 1);
            }
        }
    }
}
