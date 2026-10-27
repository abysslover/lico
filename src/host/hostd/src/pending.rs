//! What an agent asks for that the person must allow first (S04): the request waits in the host's memory, a card on the agent's own row shows it, and it happens
//! when the person answers that card. Nothing here is stored on disk: a restart of Lico drops the requests, and the agent's next call asks again.
//!
//! The state is kept per session, in the `Session` of the agent that asked, so that no lock but that session's is held while the request is read or changed. What the
//! decision does (send the text, make a session) is done after every lock is dropped, by `calls::perform`; the agent is told what became of its request by a
//! notice from the host (`Origin::Host`), once per decision.

use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use crate::core::{self, Reply, Shared};
use crate::session::{encode, now_secs, obj, Origin, Session, State, MAX_APPROVALS};

/// Requests of one session that wait for the person at the same time.
pub const PER_SESSION: usize = 8;
/// Bytes of request text that wait for the person, over all sessions.
pub const TOTAL_TEXT: usize = 1 << 20;
/// How long a request waits (seconds). A request not answered by then is dropped, and the agent is told.
pub const EXPIRY: u64 = 12 * 3600;
const MAX_KEPT: usize = 8;

/// The bytes of text held for decisions, over the whole host.
pub fn held_text() -> &'static Arc<AtomicUsize> {
    static HELD: OnceLock<Arc<AtomicUsize>> = OnceLock::new();
    HELD.get_or_init(Default::default)
}

pub fn now() -> u64 {
    now_secs()
}

/// A share of the held bytes; the bytes are given back when the request is gone, whichever way it went.
struct Held {
    total: Arc<AtomicUsize>,
    bytes: usize,
}

impl Drop for Held {
    fn drop(&mut self) {
        self.total.fetch_sub(self.bytes, Ordering::Relaxed);
    }
}

struct Record {
    id: i64,
    tool: &'static str,
    target: String, // what it is for: a session id for `send`
    who: String,    // how the person was shown that target, in the words the asker is told with
    text: String,
    expires: u64,
    _held: Held,
}

#[derive(Default)]
pub struct Pending {
    records: Vec<Record>,
    keeps: Vec<(&'static str, String)>, // what the person allowed for as long as this session lives: (tool, target)
}

impl Pending {
    fn has(&self, id: i64) -> bool {
        self.records.iter().any(|r| r.id == id)
    }
}

impl Record {
    /// What the request was, as the asker is told: `send to session 'reader' (s111111)`; a creation is named by what it would make.
    fn named(&self) -> String {
        if self.tool == "send" { format!("send to {}", self.who) } else { self.who.clone() }
    }

    fn declined_words(&self) -> String {
        format!("declined: the person did not allow this {}. Do not ask again unless the person tells you to.", self.named())
    }

    fn expired_words(&self) -> String {
        format!("expired: the person did not decide within {} hours, so the {} was dropped and nothing was done. Ask again only if it still matters.", EXPIRY / 3600, self.named())
    }
}

/// What an agent asks for.
pub struct Request<'a> {
    pub tool: &'static str,
    pub target: &'a str,
    pub who: &'a str,  // the target as the person is shown it, e.g. `session 'reader' (s111111)`; for a creation, what it would make
    pub card: &'a str, // the line on the card
    pub text: &'a str,
}

#[derive(Debug, PartialEq)]
pub enum Asked {
    New(i64),
    Again(i64),
}

/// Whether the person has allowed `caller` this tool towards this target for as long as the session lives.
pub fn kept(shared: &Shared, caller: &str, tool: &str, target: &str) -> bool {
    core::find(shared, caller).is_some_and(|found| found.lock().unwrap().pending.keeps.iter().any(|(kept, to)| *kept == tool && to == target))
}

/// Put a request on the row of `caller`: a card the person can answer, and the text kept until they do. An identical request that already waits is that one.
/// Err: the refusal, in the words the model reads.
pub fn ask(shared: &Shared, caller: &str, request: &Request, now: u64, held: &Arc<AtomicUsize>) -> Result<Asked, String> {
    let found = core::find(shared, caller).ok_or("session_exited: this session has ended")?;
    let mut s = found.lock().unwrap();
    if s.purged || s.state != State::Running {
        return Err("session_exited: this session has ended".into());
    }
    if let Some(same) = s.pending.records.iter().find(|r| r.tool == request.tool && r.target == request.target && r.text == request.text) {
        return Ok(Asked::Again(same.id));
    }
    if s.pending.records.len() >= PER_SESSION {
        return Err(format!("send_limit: {PER_SESSION} requests of yours already wait for the person; wait until they have decided, or tell them"));
    }
    if s.approvals.len() >= MAX_APPROVALS {
        return Err("send_limit: too many questions wait for the person on this session; try again after they have answered".into());
    }
    let bytes = request.text.len();
    if held.fetch_add(bytes, Ordering::Relaxed) + bytes > TOTAL_TEXT {
        held.fetch_sub(bytes, Ordering::Relaxed);
        return Err("send_limit: too much text already waits for the person (1 MiB over all sessions); try again after they have decided".into());
    }
    let held = Held { total: held.clone(), bytes };
    s.approval_counter += 1;
    let id = s.approval_counter;
    s.add_card(id, request.tool, request.card);
    s.pending.records.push(Record { id, tool: request.tool, target: request.target.to_string(), who: request.who.to_string(), text: request.text.to_string(), expires: now + EXPIRY, _held: held });
    eprintln!("{}", log_line(caller, request.tool, id, "asked", bytes));
    Ok(Asked::New(id))
}

/// The person's answer to a card. None: the card is not one of these requests (the program's own approvals go the other way).
pub fn answer(shared: &Arc<Shared>, session: &str, generation: i64, id: i64, answer: &str, keep: bool) -> Option<Reply> {
    answer_at(shared, session, generation, id, answer, keep, now())
}

fn answer_at(shared: &Arc<Shared>, session: &str, generation: i64, id: i64, answer: &str, keep: bool, now: u64) -> Option<Reply> {
    let found = core::find(shared, session)?;
    let (record, expired) = {
        let mut s = found.lock().unwrap();
        let Some(position) = s.pending.records.iter().position(|r| r.id == id) else {
            return (keep && s.approvals.iter().any(|(n, _)| *n == id)).then_some(Err("bad_request")); // only an agent's request can be remembered, not the program's own question
        };
        if let Err(code) = core::check_generation(&s, Some(generation)) {
            return Some(Err(code));
        }
        let expired = s.pending.records[position].expires <= now;
        if !expired {
            if s.state == State::Disconnected {
                return Some(Err("session_disconnected")); // the word could not reach it; the request waits for it to come back
            }
            if keep && (answer != "allow" || !matches!(s.pending.records[position].tool, "send" | "history")) {
                return Some(Err("bad_request")); // only a yes can be remembered, and only for a send or a history: a session is made once for each yes
            }
        }
        let record = s.pending.records.remove(position);
        s.approvals.retain(|(n, _)| *n != id);
        if keep && !expired {
            let allowed = (record.tool, record.target.clone());
            if !s.pending.keeps.contains(&allowed) {
                if s.pending.keeps.len() >= MAX_KEPT {
                    s.pending.keeps.remove(0);
                }
                s.pending.keeps.push(allowed);
            }
        }
        (record, expired)
    };
    // every lock is dropped: what the yes does takes the locks of the sessions it touches, one at a time
    if expired {
        eprintln!("{}", log_line(session, record.tool, id, "expired", record.text.len()));
        notify(shared, session, id, &record.expired_words());
        return Some(Err("expired"));
    }
    let (outcome, reason, words) = if answer != "allow" {
        ("declined", None, record.declined_words())
    } else {
        match crate::calls::perform(shared, session, record.tool, &record.target, &record.who, &record.text) {
            Ok(done) => ("done", None, format!("approved and done: {done}")),
            Err(refused) => ("not_done", refused.split(": ").next().map(str::to_string), format!("not done: {refused}")),
        }
    };
    eprintln!("{}", log_line(session, record.tool, id, outcome, record.text.len()));
    notify(shared, session, id, &words);
    let mut reply = obj(json!({"session": session, "approval_id": id, "state": "answered", "outcome": outcome}));
    if let Some(reason) = reason {
        reply.insert("reason".into(), json!(reason));
    }
    Some(Ok(reply))
}

/// The longest page of a request's text that one `approval.text` returns, in bytes.
pub const PAGE: usize = 16 * 1024;

/// One page of the text that waits behind a request, for the person who has to decide on it.
/// A page is cut on a character boundary (so `limit` of 4 always holds one character) and `next` is where the following page starts, null at the end.
pub fn text_page(shared: &Shared, session: &str, generation: i64, id: i64, offset: usize, limit: usize) -> Reply {
    let found = core::find(shared, session).ok_or("unknown_session")?;
    let s = found.lock().unwrap();
    core::check_generation(&s, Some(generation))?;
    let record = s.pending.records.iter().find(|r| r.id == id).ok_or("unknown_approval")?;
    if record.expires <= now() {
        return Err("expired");
    }
    let (text, total) = (&record.text, record.text.len());
    if offset > total || !text.is_char_boundary(offset) {
        return Err("bad_request");
    }
    let mut end = (offset + limit.min(PAGE)).min(total);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Ok(obj(json!({"session": s.id, "approval_id": id, "text": &text[offset..end], "offset": offset, "next": (end < total).then_some(end), "total": total})))
}

/// The host's word to the session that asked, once per decision. It is input like a person's but written by the host: it opens no card and is answered by no notice.
fn notify(shared: &Shared, session: &str, id: i64, words: &str) {
    let text = format!("[from lico | {} | approval {id}]\n{words}", crate::transfer::hhmm_now());
    if let Err(code) = core::input(shared, session, None, &format!("n-{}", core::random_hex(8)), &text, Origin::Host) {
        eprintln!("lico-host: notice dropped session={session} approval={id} cause={code}");
    }
}

/// The one line the host logs for a request: who, what, which, and how it stands; never the text.
fn log_line(session: &str, tool: &str, id: i64, state: &str, size: usize) -> String {
    format!("lico-host: request session={session} tool={tool} approval={id} state={state} size={size}")
}

/// Drop the requests of `session` that have waited too long, and tell it.
pub fn sweep(shared: &Shared, session: &str) {
    sweep_at(shared, session, now());
}

pub fn sweep_all(shared: &Shared) {
    let list: Vec<Arc<Mutex<Session>>> = shared.sessions.read().unwrap().clone();
    let at = now();
    for found in list {
        sweep_in(shared, &found, at);
    }
}

fn sweep_at(shared: &Shared, session: &str, now: u64) {
    if let Some(found) = core::find(shared, session) {
        sweep_in(shared, &found, now);
    }
}

fn sweep_in(shared: &Shared, found: &Arc<Mutex<Session>>, now: u64) {
    let (session, gone) = {
        let mut s = found.lock().unwrap();
        if !s.pending.records.iter().any(|r| r.expires <= now) {
            return;
        }
        let (gone, rest): (Vec<Record>, Vec<Record>) = std::mem::take(&mut s.pending.records).into_iter().partition(|r| r.expires <= now);
        s.pending.records = rest;
        s.approvals.retain(|(n, _)| !gone.iter().any(|r| r.id == *n));
        (s.id.clone(), gone)
    };
    for record in gone {
        eprintln!("{}", log_line(&session, record.tool, record.id, "expired", record.text.len()));
        notify(shared, &session, record.id, &record.expired_words());
    }
}

/// Whether the session's program waits for the person: a card of the host's own question does not make it wait.
pub fn waits(s: &Session) -> bool {
    s.approvals.iter().any(|(n, _)| !s.pending.has(*n))
}

/// The approvals of a status entry: the program's, and the host's own marked with the tool they are for; and what is allowed for the session.
pub fn annotate(entry: &mut Value, s: &Session) {
    let card = |(n, prompt): &(i64, String)| {
        let mut card = json!({"approval_id": n, "prompt": prompt});
        if let Some(tool) = s.pending.records.iter().find(|r| r.id == *n).map(|r| r.tool) {
            card["tool"] = json!(tool);
        }
        card
    };
    if !s.approvals.is_empty() {
        entry["approvals"] = Value::Array(s.approvals.iter().map(card).collect());
    }
    if !s.pending.keeps.is_empty() {
        entry["allowed"] = Value::Array(s.pending.keeps.iter().map(|(tool, to)| json!({"tool": tool, "to": to})).collect()); // what the person said yes to for as long as the session lives
    }
}

impl Session {
    /// A card for a question of the host's own: shown like the program's, marked with its tool, and counted with them against `MAX_APPROVALS`.
    fn add_card(&mut self, number: i64, tool: &str, prompt: &str) {
        let prompt: String = prompt.chars().take(200).collect();
        let line = encode(obj(json!({"ev": "approval", "session": self.id, "approval_id": number, "prompt": prompt, "tool": tool})));
        self.approvals.push((number, prompt));
        for sub in &self.subs {
            sub.push(line.clone());
        }
    }

    /// The program ended: its own requests are stale. The cards of the host's questions stay, they are not the program's.
    pub fn drop_program_approvals(&mut self) {
        let host = &self.pending;
        self.approvals.retain(|(n, _)| host.has(*n));
    }

    /// The session ended for good: nobody is left to tell what became of its requests.
    pub fn drop_requests(&mut self) {
        let ids: Vec<i64> = self.pending.records.iter().map(|r| r.id).collect();
        self.approvals.retain(|(n, _)| !ids.contains(n));
        self.pending = Pending::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calls::tests::{ask as arguments, call_counted};
    use crate::transfer::tests::{world, World, ONE, THREE, TWO};
    use std::sync::mpsc;
    use std::time::Duration;

    /// A host with three Claude sessions (`writer`, `reader`, `third`) and a count of the held bytes of its own.
    struct Room {
        w: World,
        held: Arc<AtomicUsize>,
    }

    fn room() -> Room {
        let w = world(&[ONE, TWO, THREE]);
        for (id, name) in [(ONE, "writer"), (TWO, "reader"), (THREE, "third")] {
            w.with(id, |s| {
                s.provider = "claude";
                s.settings.name = Some(name.into());
            });
        }
        Room { w, held: Arc::default() }
    }

    impl Room {
        fn call(&self, from: &str, to: &str, text: &str) -> Result<Value, String> {
            call_counted(&self.w, from, &arguments(to, text), &self.held).map(|got| serde_json::from_str(&got).unwrap())
        }
        /// A send the call of which has to wait for the person: its approval id.
        fn pend(&self, from: &str, to: &str, text: &str) -> i64 {
            let got = self.call(from, to, text).unwrap();
            assert_eq!(got["status"], "pending", "{got}");
            got["approval_id"].as_i64().unwrap()
        }
        fn decide(&self, from: &str, id: i64, what: &str, keep: bool) -> Reply {
            let generation = self.w.with(from, |s| s.generation as i64);
            core::approval_answer(&self.w.shared, from, generation, id, what, keep)
        }
        fn inbox(&self, id: &str) -> Vec<String> {
            std::iter::from_fn(|| self.w.delivered(id)).collect()
        }
        fn status_of(&self, id: &str) -> Value {
            let all = core::status(&self.w.shared).unwrap();
            all["sessions"].as_array().unwrap().iter().find(|s| s["session"] == id).unwrap().clone()
        }
        fn waiting(&self, id: &str) -> usize {
            self.w.with(id, |s| s.pending.records.len())
        }
    }

    #[test]
    fn a_request_to_make_a_session_is_named_by_what_it_would_make_in_the_words_of_a_no_and_of_an_expiry() {
        let record = |tool, who: &str| Record { id: 1, tool, target: String::new(), who: who.into(), text: String::new(), expires: 0, _held: Held { total: Arc::default(), bytes: 0 } };
        let make = record("create", "creation of a new session 'reviewer' in '/work/api'");
        assert_eq!(make.declined_words(), "declined: the person did not allow this creation of a new session 'reviewer' in '/work/api'. Do not ask again unless the person tells you to.");
        assert_eq!(make.expired_words(), "expired: the person did not decide within 12 hours, so the creation of a new session 'reviewer' in '/work/api' was dropped and nothing was done. Ask again only if it still matters.");
        let send = record("send", "session 'reader' (s111111)");
        assert_eq!(send.declined_words(), "declined: the person did not allow this send to session 'reader' (s111111). Do not ask again unless the person tells you to.");
        assert!(send.expired_words().contains("so the send to session 'reader' (s111111) was dropped"));
    }

    fn head_and_body(note: &str) -> (&str, &str) {
        note.split_once('\n').unwrap()
    }

    #[test]
    fn a_send_waits_for_the_person_nothing_is_delivered_and_the_card_is_on_the_row_of_the_asker() {
        let r = room();
        let got = r.call(ONE, TWO, "hello").unwrap();
        assert_eq!(got["status"], "pending");
        assert_eq!(got["approval_id"], 1);
        assert!(got["message"].as_str().unwrap().starts_with("Nothing was sent yet. The person has been asked to approve this send to session 'reader' (s111111)."), "{got}");
        assert!(got.get("already_pending").is_none() && got.get("sent").is_none());
        assert!(r.inbox(TWO).is_empty() && r.inbox(ONE).is_empty() && r.w.files(TWO).is_empty(), "nothing reached anyone");
        assert_eq!(r.status_of(ONE)["approvals"], json!([{"approval_id": 1, "prompt": "send to 'reader' (s111111) | 5 B | \"hello\"", "tool": "send"}]));
        assert!(r.status_of(TWO).get("approvals").is_none(), "the card is on the row of the one who asked");
    }

    #[test]
    fn a_yes_sends_exactly_the_text_that_was_asked_and_the_asker_is_told_once_and_it_is_not_a_step_on_a_chain() {
        let r = room();
        r.w.with(ONE, |s| s.incoming_hops = 2);
        let id = r.pend(ONE, TWO, "hello");
        let done = r.decide(ONE, id, "allow", false).unwrap();
        assert_eq!((done["state"].as_str(), done["outcome"].as_str(), done["approval_id"].as_i64()), (Some("answered"), Some("done"), Some(id)));
        let at_two = r.inbox(TWO);
        assert_eq!(at_two.len(), 1);
        let (head, body) = head_and_body(&at_two[0]);
        assert!(head.starts_with("[from session 'writer' (s012345) | ") && head.ends_with(" | message | 5 B]"), "{head}");
        assert_eq!(body, "hello");
        let at_one = r.inbox(ONE);
        assert_eq!(at_one.len(), 1, "one notice for one decision");
        let (head, body) = head_and_body(&at_one[0]);
        assert!(head.starts_with("[from lico | ") && head.ends_with(&format!(" | approval {id}]")), "{head}");
        assert!(body.starts_with("approved and done: ") && body.contains("session 'reader' (s111111)"), "{body}");
        assert_eq!(r.w.with(ONE, |s| s.incoming_hops), 2, "the host's word is not a step on a chain");
        assert_eq!(r.w.with(TWO, |s| s.incoming_hops), 3, "the text of the agent is");
        assert!(r.status_of(ONE).get("approvals").is_none() && r.waiting(ONE) == 0 && r.held.load(Ordering::Relaxed) == 0);
    }

    #[test]
    fn a_no_sends_nothing_and_the_asker_is_told_in_words_that_say_it_was_the_persons_choice() {
        let r = room();
        let id = r.pend(ONE, TWO, "hello");
        let done = r.decide(ONE, id, "deny", false).unwrap();
        assert_eq!(done["outcome"], "declined");
        assert!(r.inbox(TWO).is_empty() && r.w.files(TWO).is_empty());
        let at_one = r.inbox(ONE);
        assert_eq!(at_one.len(), 1);
        assert!(head_and_body(&at_one[0]).1.starts_with("declined: the person did not allow "), "{}", at_one[0]);
        assert!(r.status_of(ONE).get("approvals").is_none() && r.waiting(ONE) == 0 && r.held.load(Ordering::Relaxed) == 0);
    }

    #[test]
    fn a_yes_for_the_session_lets_the_later_sends_to_that_receiver_through_without_a_question_and_no_other() {
        let r = room();
        let id = r.pend(ONE, TWO, "first");
        assert_eq!(r.decide(ONE, id, "allow", true).unwrap()["outcome"], "done");
        assert_eq!((r.inbox(TWO).len(), r.inbox(ONE).len()), (1, 1), "the first one is told as every one");
        let second = r.call(ONE, TWO, "second").unwrap();
        assert_eq!(second, json!({"sent": true, "to": TWO, "delivery": "inline", "size": 6, "receiver": "idle"}));
        let at_two = r.inbox(TWO);
        assert_eq!(at_two.len(), 1);
        assert_eq!(head_and_body(&at_two[0]).1, "second");
        assert!(r.inbox(ONE).is_empty(), "no card and no notice for a send that was allowed before");
        assert!(r.status_of(ONE).get("approvals").is_none());
        assert_eq!(r.call(ONE, THREE, "third").unwrap()["status"], "pending", "another receiver is another question");
        assert_eq!(r.call(TWO, ONE, "back").unwrap()["status"], "pending", "and the allowance is the asker's own");
    }

    #[test]
    fn a_yes_for_the_session_is_only_for_a_yes_and_only_for_a_request_of_this_kind() {
        let r = room();
        let id = r.pend(ONE, TWO, "first");
        assert_eq!(r.decide(ONE, id, "deny", true), Err("bad_request"), "a no cannot be remembered");
        assert_eq!(r.waiting(ONE), 1, "and the request is still there");
        assert_eq!(r.decide(ONE, 99, "allow", true), Err("unknown_approval"), "and for a card that is not there it is the same as without it");
        r.w.with(ONE, |s| s.approvals.push((7, "ls".into())));
        assert_eq!(r.decide(ONE, 7, "allow", true), Err("bad_request"), "the program's own question is not one");
        assert_eq!(r.call(ONE, TWO, "second").unwrap()["status"], "pending", "nothing was remembered");
        assert_eq!(r.decide(ONE, id, "allow", false).unwrap()["outcome"], "done");
    }

    #[test]
    fn a_request_nobody_answers_expires_and_the_asker_is_told_once_and_a_late_yes_does_nothing() {
        let r = room();
        let (first, second, third) = (r.pend(ONE, TWO, "first"), r.pend(ONE, TWO, "second"), r.pend(ONE, TWO, "third"));
        let at = r.w.with(ONE, |s| s.pending.records[1].expires);
        let generation = r.w.with(ONE, |s| s.generation as i64);
        assert!(at > now() + EXPIRY - 10 && at <= now() + EXPIRY, "twelve hours from the call");
        let early = answer_at(&r.w.shared, ONE, generation, first, "allow", false, at - 1).unwrap();
        assert_eq!(early.unwrap()["outcome"], "done", "one second before the end it still counts");
        assert_eq!(answer_at(&r.w.shared, ONE, generation, second, "allow", false, at), Some(Err("expired")), "at the end it does not");
        assert_eq!(r.inbox(TWO).len(), 1, "only the first reached the receiver");
        let told = r.inbox(ONE);
        assert_eq!(told.len(), 2);
        assert!(head_and_body(&told[0]).1.starts_with("approved and done: ") && head_and_body(&told[1]).1.starts_with("expired: "), "{told:?}");
        assert!(head_and_body(&told[1]).0.ends_with(&format!(" | approval {second}]")));
        assert_eq!(r.waiting(ONE), 1, "the third is still there");
        assert_eq!(r.decide(ONE, second, "allow", false), Err("unknown_approval"), "and the expired one is gone for good");
        sweep_at(&r.w.shared, ONE, at - 5);
        assert!(r.inbox(ONE).is_empty() && r.waiting(ONE) == 1, "a sweep before the time touches nothing");
        sweep_at(&r.w.shared, ONE, at + 1);
        sweep_at(&r.w.shared, ONE, at + 2);
        let told = r.inbox(ONE);
        assert_eq!(told.len(), 1, "told once, however many sweeps");
        assert!(head_and_body(&told[0]).0.ends_with(&format!(" | approval {third}]")) && head_and_body(&told[0]).1.starts_with("expired: "));
        assert_eq!((r.waiting(ONE), r.held.load(Ordering::Relaxed)), (0, 0), "the text is gone from memory");
        assert!(r.status_of(ONE).get("approvals").is_none());
    }

    #[test]
    fn eight_requests_wait_at_a_time_and_a_repeat_of_one_of_them_is_the_same_request() {
        let r = room();
        let mut ids: Vec<i64> = vec![r.pend(ONE, TWO, "text 0")];
        assert_eq!(r.pend(ONE, THREE, "text 0"), 2, "the same text to another receiver is another request");
        ids.push(2);
        ids.extend((2..PER_SESSION).map(|k| r.pend(ONE, TWO, &format!("text {k}"))));
        assert_eq!(ids, (1..=PER_SESSION as i64).collect::<Vec<_>>(), "ids are counted, never reused");
        let again = r.call(ONE, TWO, "text 3").unwrap();
        assert_eq!((again["status"].as_str(), again["approval_id"].as_i64(), again["already_pending"].as_bool()), (Some("pending"), Some(4), Some(true)));
        assert_eq!(r.status_of(ONE)["approvals"].as_array().unwrap().len(), PER_SESSION, "no second card");
        let text = r.call(ONE, TWO, "text 8").unwrap_err();
        assert!(text.starts_with("send_limit: ") && text.contains("8 requests"), "{text}");
        assert_eq!(r.call(TWO, ONE, "mine").unwrap()["status"], "pending", "the other sessions have their own eight");
        r.decide(ONE, 1, "deny", false).unwrap();
        assert_eq!(r.call(ONE, TWO, "text 8").unwrap()["status"], "pending", "a decision makes room");
    }

    #[test]
    fn the_text_that_waits_is_bounded_over_all_sessions_and_given_back_when_it_is_decided() {
        let r = room();
        let big = |k: u8| format!("{k}{}", "x".repeat(128 * 1024 - 1));
        for k in 0..5 {
            r.pend(ONE, TWO, &big(k));
        }
        r.pend(THREE, TWO, &big(5));
        r.pend(THREE, TWO, &big(6));
        assert_eq!(r.held.load(Ordering::Relaxed), 7 * 128 * 1024, "seven of 128 KiB");
        r.pend(THREE, TWO, &big(7)); // exactly the limit
        assert_eq!(r.held.load(Ordering::Relaxed), TOTAL_TEXT);
        let text = r.call(TWO, ONE, "one byte more").unwrap_err();
        assert!(text.starts_with("send_limit: ") && text.contains("1 MiB"), "{text}");
        assert_eq!(r.held.load(Ordering::Relaxed), TOTAL_TEXT, "a refused request holds nothing");
        r.decide(ONE, 1, "deny", false).unwrap();
        assert_eq!(r.held.load(Ordering::Relaxed), TOTAL_TEXT - 128 * 1024);
        r.pend(TWO, ONE, "one byte more");
    }

    #[test]
    fn a_session_that_is_purged_or_ends_loses_its_requests_and_their_text_and_cards() {
        let r = room();
        r.pend(ONE, TWO, &"y".repeat(1000));
        r.pend(ONE, THREE, "z");
        r.pend(TWO, ONE, "other");
        assert_eq!(r.held.load(Ordering::Relaxed), 1000 + 1 + 5);
        let generation = r.w.with(ONE, |s| s.generation as i64);
        let kept = core::find(&r.w.shared, ONE).unwrap(); // a purged session is no longer found: look at it through this
        core::session_purge(&r.w.shared, ONE, generation).unwrap();
        assert_eq!((kept.lock().unwrap().pending.records.len(), r.held.load(Ordering::Relaxed)), (0, 5), "only the other session's text is held now");
        assert!(kept.lock().unwrap().approvals.is_empty());
        r.w.with(TWO, |s| s.set_state(State::Exited));
        assert_eq!((r.waiting(TWO), r.held.load(Ordering::Relaxed)), (0, 0));
        assert!(r.w.with(TWO, |s| s.approvals.is_empty()), "the card goes with the request");
    }

    #[test]
    fn a_session_whose_program_is_gone_keeps_its_requests_but_they_cannot_be_decided_until_it_is_back() {
        let r = room();
        let id = r.pend(ONE, TWO, "hello");
        r.w.with(ONE, |s| s.approvals.push((9, "ls".into())));
        r.w.with(ONE, |s| {
            s.set_state(State::Disconnected);
            s.drop_program_approvals();
        });
        assert_eq!(r.w.with(ONE, |s| s.approvals.iter().map(|(n, _)| *n).collect::<Vec<_>>()), [id], "the program's question is stale, the host's is not");
        assert_eq!(r.decide(ONE, id, "allow", false), Err("session_disconnected"));
        assert_eq!(r.waiting(ONE), 1);
        assert!(r.inbox(TWO).is_empty());
        r.w.with(ONE, |s| s.set_state(State::Running));
        assert_eq!(r.decide(ONE, id, "allow", false).unwrap()["outcome"], "done");
    }

    #[test]
    fn what_cannot_happen_any_more_when_the_person_says_yes_is_not_done_and_the_asker_is_told_why() {
        let r = room();
        // the depth is read again at the decision: the count of the asker is whatever the texts it received made it (the deepest of the last ten minutes)
        r.w.with(ONE, |s| s.incoming_hops = 3);
        let deep = r.pend(ONE, TWO, "deep");
        r.w.with(ONE, |s| s.incoming_hops = 4);
        let got = r.decide(ONE, deep, "allow", false).unwrap();
        assert_eq!((got["outcome"].as_str(), got["reason"].as_str()), (Some("not_done"), Some("chain_depth")));
        let told = r.inbox(ONE);
        assert!(head_and_body(&told[0]).1.starts_with("not done: chain_depth: ") && told[0].contains("step 5"), "{told:?}");
        // and the receiver is read again: it was running when asked
        r.w.with(ONE, |s| s.incoming_hops = 0);
        let gone = r.pend(ONE, TWO, "gone");
        r.w.with(TWO, |s| s.set_state(State::Disconnected));
        let got = r.decide(ONE, gone, "allow", false).unwrap();
        assert_eq!((got["outcome"].as_str(), got["reason"].as_str()), (Some("not_done"), Some("session_disconnected")));
        assert!(head_and_body(&r.inbox(ONE)[0]).1.starts_with("not done: session_disconnected: "));
        r.w.with(TWO, |s| s.set_state(State::Running));
        assert!(r.inbox(TWO).is_empty(), "nothing reached it in all this");
        // a receiver that is busy now is no reason to refuse: the answer says what it was doing
        let busy = r.pend(ONE, TWO, "busy");
        r.w.with(TWO, |s| s.busy = true);
        r.decide(ONE, busy, "allow", false).unwrap();
        assert!(head_and_body(&r.inbox(ONE)[0]).1.contains("the receiver was busy"));
    }

    #[test]
    fn the_decision_is_for_the_request_that_was_shown_and_for_no_other_text_or_session() {
        let r = room();
        let (a, b) = (r.pend(ONE, TWO, "the text on the first card"), r.pend(ONE, TWO, "the text on the second card"));
        let other = r.pend(TWO, ONE, "from the other session");
        r.decide(ONE, b, "allow", false).unwrap();
        let at_two = r.inbox(TWO);
        assert_eq!(at_two.len(), 1);
        assert_eq!(head_and_body(&at_two[0]).1, "the text on the second card");
        assert_eq!(r.waiting(ONE), 1, "the first is still waiting");
        assert!(r.inbox(ONE).iter().all(|n| !n.contains("first card")));
        // an id of another session's request is not one of this session's
        let generation = r.w.with(THREE, |s| s.generation as i64);
        assert_eq!(core::approval_answer(&r.w.shared, THREE, generation, other, "allow", false), Err("unknown_approval"));
        assert_eq!(core::approval_answer(&r.w.shared, THREE, generation, a, "allow", false), Err("unknown_approval"));
        assert_eq!(r.waiting(TWO), 1, "and nothing of it was done");
        // a second answer to a decided one finds nothing, and nothing is sent twice
        assert_eq!(r.decide(ONE, b, "allow", false), Err("unknown_approval"));
        assert_eq!(r.decide(ONE, b, "deny", false), Err("unknown_approval"));
        assert!(r.inbox(TWO).is_empty());
        // and an answer for a run that is not this one
        let generation = r.w.with(ONE, |s| s.generation as i64);
        assert_eq!(core::approval_answer(&r.w.shared, ONE, generation + 1, a, "allow", false), Err("stale_generation"));
        assert_eq!(r.waiting(ONE), 1);
    }

    #[test]
    fn a_decision_makes_one_notice_and_a_notice_makes_neither_a_card_nor_a_notice_nor_a_step() {
        let r = room();
        for what in ["allow", "deny"] {
            let id = r.pend(ONE, TWO, "x");
            r.decide(ONE, id, what, false).unwrap();
            assert_eq!(r.inbox(ONE).len(), 1, "{what}");
            assert!(r.status_of(ONE).get("approvals").is_none() && r.status_of(TWO).get("approvals").is_none());
            assert_eq!((r.waiting(ONE), r.waiting(TWO), r.waiting(THREE)), (0, 0, 0));
            assert!(r.inbox(THREE).is_empty());
        }
        assert_eq!(r.w.with(ONE, |s| s.approval_counter), 2, "two requests, two ids, and nothing else took one");
        assert_eq!(r.w.with(ONE, |s| s.incoming_hops), 0);
    }

    #[test]
    fn two_sessions_that_each_wait_to_send_to_the_other_are_answered_at_the_same_time_and_both_get_through() {
        let r = room();
        let (a, b) = (r.pend(ONE, TWO, "from one"), r.pend(TWO, ONE, "from two"));
        let (tx, rx) = mpsc::channel();
        for (from, id) in [(ONE, a), (TWO, b)] {
            let (shared, tx) = (r.w.shared.clone(), tx.clone());
            std::thread::spawn(move || {
                let generation = core::find(&shared, from).unwrap().lock().unwrap().generation as i64;
                tx.send((from, core::approval_answer(&shared, from, generation, id, "allow", false))).unwrap();
            });
        }
        for _ in 0..2 {
            // why 5 s: a lock held across the other session's lock would hang both answers for ever, and this is the end of that wait
            let (from, got) = rx.recv_timeout(Duration::from_secs(5)).expect("both answers return");
            assert_eq!(got.unwrap()["outcome"], "done", "{from}");
        }
        let (at_one, at_two) = (r.inbox(ONE), r.inbox(TWO));
        assert_eq!((at_one.len(), at_two.len()), (2, 2), "the other's text and the notice of its own");
        assert!(at_one.iter().any(|m| m.ends_with("\nfrom two")) && at_two.iter().any(|m| m.ends_with("\nfrom one")));
    }

    #[test]
    fn the_twentieth_call_of_a_minute_is_performed_when_it_is_allowed_for_the_decision_is_not_a_call() {
        let r = room();
        let (window, now) = (Default::default(), std::time::Instant::now());
        let held: Arc<AtomicUsize> = Arc::default();
        for k in 0..19 {
            let text = crate::calls::tests::call_in(&r.w, ONE, &json!({"to": TWO}).as_object().unwrap().clone(), &window, now, &held).unwrap_err();
            assert!(text.starts_with("bad_request: "), "call {k}: {text}");
        }
        let got = crate::calls::tests::call_in(&r.w, ONE, &arguments(TWO, "the twentieth"), &window, now, &held).unwrap();
        let id = serde_json::from_str::<Value>(&got).unwrap()["approval_id"].as_i64().unwrap();
        let text = crate::calls::tests::call_in(&r.w, ONE, &arguments(TWO, "the twenty-first"), &window, now, &held).unwrap_err();
        assert!(text.starts_with("send_limit: "), "{text}");
        assert_eq!(r.decide(ONE, id, "allow", false).unwrap()["outcome"], "done");
        assert_eq!(head_and_body(&r.inbox(TWO)[0]).1, "the twentieth");
    }

    #[test]
    fn a_card_that_waits_is_not_the_programs_question_and_does_not_make_the_session_look_blocked() {
        let r = room();
        r.pend(ONE, TWO, "hello");
        r.w.with(ONE, |s| {
            s.busy = true;
            s.busy_since = 1;
            s.last_event_at = 1;
        });
        assert!(!r.w.with(ONE, |s| waits(s)));
        assert_eq!(crate::transfer::send(&r.w.shared, Some(TWO), ONE, "text", "a", crate::transfer::By::Person).unwrap()["receiver"], "busy");
        assert_eq!(r.status_of(ONE)["stalled"], true, "a turn that has been silent for long is a suspect hang, card or no card");
        r.w.with(ONE, |s| s.approvals.push((50, "ls".into())));
        assert!(r.w.with(ONE, |s| waits(s)));
        assert_eq!(crate::transfer::send(&r.w.shared, Some(TWO), ONE, "text", "b", crate::transfer::By::Person).unwrap()["receiver"], "waiting_for_approval");
        let entry = r.status_of(ONE);
        assert_eq!(entry["stalled"], false, "a turn that waits for a person's answer is not a hang");
        assert_eq!(entry["approvals"].as_array().unwrap().iter().map(|a| a.get("tool").and_then(Value::as_str)).collect::<Vec<_>>(), [Some("send"), None]);
    }

    #[test]
    fn the_card_is_one_clean_line_within_200_characters_that_ends_with_its_quote() {
        let r = room();
        let text = format!("\n  {} red \"quoted\"\nsecond line", "a".repeat(300));
        r.pend(ONE, TWO, &text);
        let card = r.status_of(ONE)["approvals"][0]["prompt"].as_str().unwrap().to_string();
        assert!(card.chars().count() <= 200, "{}", card.chars().count());
        assert!(card.starts_with("send to 'reader' (s111111) | 3") && card.contains(" B, 3 lines | \"") && card.ends_with("a\u{2026}\""), "{card}");
        assert!(!card.contains('\n'), "{card:?}");
        r.pend(ONE, THREE, "say \"hi\"\u{1b}[31m\u{202e}!");
        let card = r.status_of(ONE)["approvals"][1]["prompt"].as_str().unwrap().to_string();
        assert!(card.starts_with("send to 'third' (s222222) | ") && card.ends_with("\"say 'hi'!\""), "{card:?}");
        assert!(!card.chars().any(|c| c.is_control() || c == '\u{202e}'), "{card:?}");
    }

    /// The pages of a request read one after the other, as the window and `lico approval show` read them: the text, and how many pages it took.
    fn read_all(r: &Room, from: &str, id: i64, limit: usize) -> (String, usize) {
        let generation = r.w.with(from, |s| s.generation as i64);
        let (mut text, mut at, mut pages) = (String::new(), 0, 0);
        loop {
            let page = text_page(&r.w.shared, from, generation, id, at, limit).unwrap();
            assert_eq!(page["offset"], at);
            assert!(page["text"].as_str().unwrap().len() <= limit.min(PAGE));
            text.push_str(page["text"].as_str().unwrap());
            pages += 1;
            match page["next"].as_u64() {
                Some(next) => {
                    assert!(next as usize > at, "every page moves on");
                    at = next as usize;
                }
                None => return (text, pages),
            }
        }
    }

    #[test]
    fn a_long_text_is_read_in_pages_that_join_to_the_whole_and_are_cut_between_characters() {
        let r = room();
        let text = "\u{e9}\u{d55c}\u{1f600}".repeat(7000); // 2, 3 and 4 bytes: 63000 bytes
        let id = r.pend(ONE, TWO, &text);
        let generation = r.w.with(ONE, |s| s.generation as i64);
        let first = text_page(&r.w.shared, ONE, generation, id, 0, PAGE).unwrap();
        assert_eq!((first["offset"].as_u64(), first["total"].as_u64()), (Some(0), Some(63000)));
        assert!(first["next"].as_u64().unwrap() <= PAGE as u64 && first["next"].as_u64().unwrap() > PAGE as u64 - 4, "{first:?}");
        assert_eq!(read_all(&r, ONE, id, PAGE), (text.clone(), 4));
        assert_eq!(read_all(&r, ONE, id, 4).0, text, "four bytes always hold a character");
        assert_eq!(read_all(&r, ONE, id, 1 << 40), (text.clone(), 4), "a longer page than the host gives is asked for and is not given");
        let small = r.pend(ONE, TWO, "short");
        let only = text_page(&r.w.shared, ONE, generation, small, 0, PAGE).unwrap();
        assert_eq!((only["text"].as_str(), only["next"].is_null(), only["total"].as_u64()), (Some("short"), true, Some(5)));
        assert_eq!(r.waiting(ONE), 2, "reading takes nothing away");
        assert!(r.inbox(TWO).is_empty() && r.inbox(ONE).is_empty());
    }

    #[test]
    fn a_page_is_refused_for_what_is_not_a_request_that_waits_for_this_person_here_and_now() {
        let r = room();
        let id = r.pend(ONE, TWO, "h\u{e9}llo");
        let generation = r.w.with(ONE, |s| s.generation as i64);
        let page = |session: &str, generation: i64, id: i64, offset: usize| text_page(&r.w.shared, session, generation, id, offset, PAGE);
        assert_eq!(page("s999999", 1, id, 0).unwrap_err(), "unknown_session");
        assert_eq!(page(ONE, generation + 1, id, 0).unwrap_err(), "stale_generation");
        assert_eq!(page(ONE, generation, 99, 0).unwrap_err(), "unknown_approval");
        assert_eq!(page(TWO, r.w.with(TWO, |s| s.generation as i64), id, 0).unwrap_err(), "unknown_approval", "the request of another session");
        r.w.with(ONE, |s| s.approvals.push((7, "ls".into())));
        assert_eq!(page(ONE, generation, 7, 0).unwrap_err(), "unknown_approval", "the program's own question has no text of ours");
        assert_eq!(page(ONE, generation, id, 7).unwrap_err(), "bad_request", "past the end");
        assert_eq!(page(ONE, generation, id, 2).unwrap_err(), "bad_request", "inside a character");
        assert_eq!(page(ONE, generation, id, 6).unwrap()["text"], "", "the end itself is an empty page");
        r.w.with(ONE, |s| s.pending.records[0].expires = 0);
        assert_eq!(page(ONE, generation, id, 0).unwrap_err(), "expired");
        r.w.with(ONE, |s| s.pending.records[0].expires = now() + 60);
        assert_eq!(r.decide(ONE, id, "deny", false).unwrap()["outcome"], "declined");
        assert_eq!(page(ONE, generation, id, 0).unwrap_err(), "unknown_approval", "a decided request is gone");
    }

    #[test]
    fn a_page_that_the_json_makes_longer_still_leaves_far_under_the_queue_of_a_connection() {
        let r = room();
        let id = r.pend(ONE, TWO, &"\n\"\\".repeat(20000)); // a control character does not get here (the text is cleaned), the worst is the two bytes of an escape
        let page = text_page(&r.w.shared, ONE, r.w.with(ONE, |s| s.generation as i64), id, 0, PAGE).unwrap();
        let line = encode(page);
        assert!(line.len() > PAGE && line.len() < 6 * PAGE + 1024 && line.len() < 100_000, "{} bytes for one page", line.len());
    }

    #[test]
    fn what_the_person_allowed_for_the_session_is_listed_on_the_row_and_nothing_else_is() {
        let r = room();
        assert!(r.status_of(ONE).get("allowed").is_none());
        let id = r.pend(ONE, TWO, "first");
        assert!(r.status_of(ONE).get("allowed").is_none(), "a question is not an allowance");
        r.decide(ONE, id, "allow", false).unwrap();
        assert!(r.status_of(ONE).get("allowed").is_none(), "a yes without the keep is not one either");
        let id = r.pend(ONE, TWO, "second");
        r.decide(ONE, id, "allow", true).unwrap();
        assert_eq!(r.status_of(ONE)["allowed"], json!([{"tool": "send", "to": TWO}]));
        assert!(r.status_of(ONE).get("approvals").is_none() && r.status_of(TWO).get("allowed").is_none(), "an allowance is the asker's own");
        r.pend(ONE, THREE, "third");
        let both = r.status_of(ONE);
        assert_eq!((both["approvals"].as_array().map(Vec::len), both["allowed"].as_array().map(Vec::len)), (Some(1), Some(1)));
    }
}
