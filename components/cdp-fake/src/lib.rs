//! `lico-cdp-fake` (SPEC.md): a scripted fake of a Chrome DevTools Protocol endpoint on `127.0.0.1:0`, for tests. Plain HTTP rules (discovery), the WebSocket upgrade with an
//! optional bearer token, scripted replies and events by method, scripted failures, and every head and call recorded.

use lico_ws::{accept, read_request, Conn, Head, Limits, Message};
use serde_json::{json, Value};
use std::io::Write;
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// The longest message a client may send: the same as the default of `lico-ws`, so that a screencast acknowledgement or a navigation is never near it; a larger one ends the connection.
pub const MAX_MESSAGE: usize = lico_ws::DEFAULT_MAX_MESSAGE;
/// How long a connection may say nothing before the fake ends it: a test that forgot to close cannot keep a thread for ever.
pub const CONNECTION_IDLE: Duration = Duration::from_secs(5);
/// How often the accept loop looks at the stop flag.
const POLL: Duration = Duration::from_millis(20);

#[derive(Clone, Debug)]
enum Kind {
    Result(Value),
    Error { code: i64, message: String },
    Silent,
    Close,
}

/// What the fake does with a call.
#[derive(Clone, Debug)]
pub struct Answer {
    kind: Kind,
    events: Vec<(Option<String>, String, Value)>,
    delay: Duration,
}

impl Answer {
    fn of(kind: Kind) -> Answer {
        Answer { kind, events: Vec::new(), delay: Duration::ZERO }
    }
    pub fn result(v: Value) -> Answer {
        Answer::of(Kind::Result(v))
    }
    pub fn error(code: i64, message: &str) -> Answer {
        Answer::of(Kind::Error { code, message: message.to_string() })
    }
    /// Reads the call and never answers.
    pub fn silent() -> Answer {
        Answer::of(Kind::Silent)
    }
    /// Ends the connection without a close frame.
    pub fn close() -> Answer {
        Answer::of(Kind::Close)
    }
    /// An event sent after the reply.
    pub fn event(mut self, method: &str, params: Value) -> Answer {
        self.events.push((None, method.to_string(), params));
        self
    }
    /// An event of a session, sent after the reply.
    pub fn event_in(mut self, session: &str, method: &str, params: Value) -> Answer {
        self.events.push((Some(session.to_string()), method.to_string(), params));
        self
    }
    /// The reply waits this many milliseconds.
    pub fn after(mut self, ms: u64) -> Answer {
        self.delay = Duration::from_millis(ms);
        self
    }
}

struct Rule {
    method: String,
    answer: Answer,
    left: Option<usize>,
}

struct Plain {
    path: String,
    status: u16,
    body: String,
}

#[derive(Default)]
pub struct Script {
    token: Option<String>,
    rules: Vec<Rule>,
    plain: Vec<Plain>,
    idle: Option<Duration>,
    max_message: Option<usize>,
}

impl Script {
    pub fn new() -> Script {
        Script::default()
    }
    /// Every head must carry `Authorization: Bearer TOKEN`, else the fake answers 401 (before any upgrade).
    pub fn token(mut self, token: &str) -> Script {
        self.token = Some(token.to_string());
        self
    }
    pub fn http(mut self, path: &str, status: u16, body: &str) -> Script {
        self.plain.push(Plain { path: path.to_string(), status, body: body.to_string() });
        self
    }
    pub fn on(mut self, method: &str, answer: Answer) -> Script {
        self.rules.push(Rule { method: method.to_string(), answer, left: None });
        self
    }
    pub fn times(mut self, n: usize, method: &str, answer: Answer) -> Script {
        self.rules.push(Rule { method: method.to_string(), answer, left: Some(n) });
        self
    }
    pub fn idle(mut self, idle: Duration) -> Script {
        self.idle = Some(idle);
        self
    }
    pub fn max_message(mut self, n: usize) -> Script {
        self.max_message = Some(n);
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Call {
    pub method: String,
    pub params: Value,
    pub session_id: Option<String>,
}

/// What was received in a head. `Debug` hides the values of secret headers.
#[derive(Clone)]
pub struct Seen {
    pub method: String,
    pub path: String,
    head: Head,
}

impl Seen {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.head.header(name)
    }
    pub fn authorization(&self) -> Option<&str> {
        self.head.header("authorization")
    }
}

impl std::fmt::Debug for Seen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Seen {{ {:?} }}", self.head)
    }
}

struct State {
    token: Option<String>,
    rules: Mutex<Vec<Rule>>,
    plain: Vec<Plain>,
    idle: Duration,
    max_message: usize,
    calls: Mutex<Vec<Call>>,
    heads: Mutex<Vec<Seen>>,
    stop: AtomicBool,
    streams: Mutex<Vec<TcpStream>>,
}

pub struct Fake {
    addr: SocketAddr,
    state: Arc<State>,
    accept: Option<thread::JoinHandle<()>>,
}

impl Fake {
    pub fn start(script: Script) -> std::io::Result<Fake> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let state = Arc::new(State {
            token: script.token,
            rules: Mutex::new(script.rules),
            plain: script.plain,
            idle: script.idle.unwrap_or(CONNECTION_IDLE),
            max_message: script.max_message.unwrap_or(MAX_MESSAGE),
            calls: Mutex::new(Vec::new()),
            heads: Mutex::new(Vec::new()),
            stop: AtomicBool::new(false),
            streams: Mutex::new(Vec::new()),
        });
        let st = state.clone();
        let accept = thread::spawn(move || {
            while !st.stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        if let Ok(copy) = stream.try_clone() {
                            st.streams.lock().unwrap().push(copy);
                        }
                        let st2 = st.clone();
                        thread::spawn(move || serve(stream, &st2));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => thread::sleep(POLL),
                    Err(_) => thread::sleep(POLL),
                }
            }
        });
        Ok(Fake { addr, state, accept: Some(accept) })
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Every call that arrived, in order, answered by a rule or not.
    pub fn calls(&self) -> Vec<Call> {
        self.state.calls.lock().unwrap().clone()
    }

    /// Every head that arrived (a refused one too), in order.
    pub fn heads(&self) -> Vec<Seen> {
        self.state.heads.lock().unwrap().clone()
    }

    /// Ends the accept loop and every connection; returns within about 2 s whatever the connections do.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.state.stop.store(true, Ordering::SeqCst);
        for s in self.state.streams.lock().unwrap().iter() {
            let _ = s.shutdown(Shutdown::Both);
        }
        if let Some(h) = self.accept.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// The params of a `Page.screencastFrame` event in the Chrome shape (**PROPOSED** for the engine: fitted after its measurement): `frame_id` is the number to acknowledge.
pub fn screencast_frame(frame_id: u64, data_b64: &str, width: u32, height: u32) -> Value {
    json!({"data": data_b64, "sessionId": frame_id, "metadata": {"offsetTop": 0, "pageScaleFactor": 1, "deviceWidth": width, "deviceHeight": height, "scrollOffsetX": 0, "scrollOffsetY": 0, "timestamp": 0}})
}

fn reply_plain(stream: &mut TcpStream, status: u16, body: &str) {
    let reason = match status {
        200 => "OK",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Status",
    };
    let _ = stream.write_all(format!("HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes());
    let _ = stream.shutdown(Shutdown::Both); // the accept loop keeps a duplicate of the socket, so dropping this one would not end it
}

fn serve(mut stream: TcpStream, st: &Arc<State>) {
    let limits = Limits { connect: Duration::from_secs(2), handshake: Duration::from_secs(2), idle: st.idle, max_message: st.max_message };
    let Ok(head) = read_request(&mut stream, &limits) else {
        let _ = stream.shutdown(Shutdown::Both);
        return;
    };
    st.heads.lock().unwrap().push(Seen { method: head.method.clone(), path: head.path.clone(), head: head.clone() });
    if let Some(token) = &st.token {
        if head.header("authorization") != Some(format!("Bearer {token}").as_str()) {
            reply_plain(&mut stream, 401, "");
            return;
        }
    }
    if !head.is_upgrade() {
        let path = head.path.split('?').next().unwrap_or("");
        match st.plain.iter().find(|p| p.path == path) {
            Some(p) => reply_plain(&mut stream, p.status, &p.body),
            None => reply_plain(&mut stream, 404, ""),
        }
        return;
    }
    let Ok(mut conn) = accept(stream, &head, &limits) else { return };
    conn.set_read_timeout(Some(st.idle));
    loop {
        if st.stop.load(Ordering::SeqCst) {
            return;
        }
        let text = match conn.recv() {
            Ok(Message::Text(t)) => t,
            Ok(Message::Binary(_)) => {
                let _ = conn.close(1003);
                return;
            }
            Ok(Message::Close { .. }) | Err(_) => return,
        };
        let Some((id, method, params, session)) = parse(&text) else {
            let _ = conn.close(1007);
            return;
        };
        st.calls.lock().unwrap().push(Call { method: method.clone(), params, session_id: session.clone() });
        let answer = take_rule(st, &method);
        if !run(&mut conn, id, &method, session.as_deref(), answer) {
            return;
        }
    }
}

fn parse(text: &str) -> Option<(u64, String, Value, Option<String>)> {
    let v: Value = serde_json::from_str(text).ok()?;
    let o = v.as_object()?;
    let id = o.get("id")?.as_u64()?;
    let method = o.get("method")?.as_str()?.to_string();
    let params = o.get("params").cloned().unwrap_or_else(|| json!({}));
    let session = o.get("sessionId").and_then(Value::as_str).map(str::to_string);
    Some((id, method, params, session))
}

fn take_rule(st: &Arc<State>, method: &str) -> Option<Answer> {
    let mut rules = st.rules.lock().unwrap();
    let rule = rules.iter_mut().find(|r| r.method == method && r.left != Some(0))?;
    if let Some(n) = rule.left.as_mut() {
        *n -= 1;
    }
    Some(rule.answer.clone())
}

/// False when the connection is to end.
fn run(conn: &mut Conn, id: u64, method: &str, session: Option<&str>, answer: Option<Answer>) -> bool {
    let answer = answer.unwrap_or_else(|| Answer::error(-32601, &format!("'{method}' wasn't found")));
    thread::sleep(answer.delay);
    let mut reply = match &answer.kind {
        Kind::Silent => return true,
        Kind::Close => return false,
        Kind::Result(v) => json!({"id": id, "result": v}),
        Kind::Error { code, message } => json!({"id": id, "error": {"code": code, "message": message}}),
    };
    if let Some(s) = session {
        reply["sessionId"] = json!(s);
    }
    if conn.send_text(&reply.to_string()).is_err() {
        return false;
    }
    for (session, method, params) in &answer.events {
        let mut event = json!({"method": method, "params": params});
        if let Some(s) = session {
            event["sessionId"] = json!(s);
        }
        if conn.send_text(&event.to_string()).is_err() {
            return false;
        }
    }
    true
}

