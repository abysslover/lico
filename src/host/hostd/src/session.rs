//! Sessions, the durable sequenced log and subscriber queues.
//!
//! A session keeps a small bounded window of recent records in memory and writes
//! every record to an owner-only segment file before it can leave the window.
//! Older replay is read from the segments, so memory does not grow with history.

use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::payload::hex;

pub const SNAPSHOT_LINES: usize = 40;
const HEADER: usize = 12; // u32 length + u64 sequence
/// Request ids remembered per session (as 64-bit hashes): a repeat inside this window is never
/// dispatched twice; older ids are forgotten, which the protocol note states.
pub const MAX_TRACKED_INPUTS: usize = 65536;
pub const MAX_APPROVALS: usize = 64;

pub fn obj(value: Value) -> serde_json::Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

// ---------------------------------------------------------------- subscribers
struct Queue {
    lines: VecDeque<Arc<Vec<u8>>>,
    bytes: usize,
}

pub struct Sub {
    queue: Mutex<Queue>,
    pub wake: Condvar,
    cap_bytes: usize,
    pub closed: AtomicBool,
    overflowed: Mutex<HashSet<String>>,
}

pub fn encode(mut body: serde_json::Map<String, Value>) -> Arc<Vec<u8>> {
    let mut all = serde_json::Map::new();
    all.insert("v".into(), json!(1));
    all.append(&mut body);
    let mut bytes = serde_json::to_vec(&Value::Object(all)).unwrap_or_default();
    bytes.push(b'\n');
    Arc::new(bytes)
}

impl Sub {
    pub fn new(cap_bytes: usize) -> Arc<Sub> {
        Arc::new(Sub {
            queue: Mutex::new(Queue { lines: VecDeque::new(), bytes: 0 }),
            wake: Condvar::new(),
            cap_bytes,
            closed: AtomicBool::new(false),
            overflowed: Mutex::new(HashSet::new()),
        })
    }

    /// Queue a reply or notice regardless of the cap (replies are small and rare).
    pub fn push(&self, line: Arc<Vec<u8>>) {
        let mut queue = self.queue.lock().unwrap();
        if queue.bytes > self.cap_bytes.saturating_mul(4) {
            // a peer that never reads its replies is cut off instead of growing the host
            self.closed.store(true, std::sync::atomic::Ordering::Relaxed);
            queue.lines.clear();
            queue.bytes = 0;
            self.wake.notify_one();
            return;
        }
        queue.bytes += line.len();
        queue.lines.push_back(line);
        self.wake.notify_one();
    }

    /// Queue a record; true means this subscriber must be dropped (it fell behind).
    /// The caller never waits for a slow reader.
    pub fn offer(&self, session: &str, line: Arc<Vec<u8>>) -> bool {
        if self.overflowed.lock().unwrap().contains(session) {
            return true;
        }
        let mut queue = self.queue.lock().unwrap();
        if queue.bytes + line.len() > self.cap_bytes {
            self.overflowed.lock().unwrap().insert(session.to_string());
            let notice = encode(obj(json!({"ev": "overflow", "session": session})));
            queue.bytes += notice.len();
            queue.lines.push_back(notice);
            self.wake.notify_one();
            return true;
        }
        queue.bytes += line.len();
        queue.lines.push_back(line);
        self.wake.notify_one();
        false
    }

    pub fn clear_overflow(&self, session: &str) {
        self.overflowed.lock().unwrap().remove(session);
    }

    pub fn pop_wait(&self) -> Option<Arc<Vec<u8>>> {
        let mut queue = self.queue.lock().unwrap();
        while queue.lines.is_empty() && !self.closed.load(std::sync::atomic::Ordering::Relaxed) {
            queue = self.wake.wait(queue).unwrap();
        }
        let line = queue.lines.pop_front()?;
        queue.bytes -= line.len();
        Some(line)
    }

    /// Wait (bounded) until the queue is below half its cap; used by replay so that a
    /// long replay flows at the reader's pace instead of overflowing it.
    pub fn wait_room(&self) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if self.queue.lock().unwrap().bytes < self.cap_bytes / 2 {
                return true;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        false
    }
}

// ------------------------------------------------------------- durable log
#[derive(Clone)]
pub struct LogView {
    dir: PathBuf,
    id: String,
    segments: Vec<(i64, usize)>,
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

pub struct Log {
    view: LogView,
    sizes: Vec<usize>, // bytes of each segment, parallel to view.segments
    segment_records: usize,
    retain: usize,
    writer: Option<File>,
    dirty: bool, // appended since the last sync to disk
}

/// What a restart found wrong with a log: bytes of a torn tail set aside, and whole segments set aside.
#[derive(Default)]
pub struct Recovery {
    pub torn_bytes: usize,
    pub segments_set_aside: usize,
}

impl Log {
    pub fn new(dir: &Path, id: &str, retain: usize) -> Log {
        Log {
            view: LogView { dir: dir.to_path_buf(), id: id.to_string(), segments: Vec::new() },
            sizes: Vec::new(),
            segment_records: (retain / 4).clamp(16, 4096),
            retain,
            writer: None,
            dirty: false,
        }
    }

    /// Open the segments a session left on disk. Every frame is checked (its sequence must follow the one before);
    /// at the first frame that is cut short or out of order the rest of that segment, and every later segment, is
    /// set aside in `.damaged` files (owner-only, never read again) and the segment is cut at the last good frame.
    /// Returns the log, the next sequence number and what was found.
    pub fn recover(dir: &Path, id: &str, retain: usize) -> (Log, i64, Recovery) {
        let mut log = Log::new(dir, id, retain);
        let mut report = Recovery::default();
        let mut files: Vec<(i64, PathBuf)> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                let rest = name.strip_prefix(id)?.strip_prefix('.')?.strip_suffix(".seg")?;
                Some((rest.parse::<i64>().ok()?, e.path()))
            })
            .collect();
        files.sort();
        let mut next: i64 = 0;
        let mut broken = false;
        for (first, path) in files {
            if broken || (!log.view.segments.is_empty() && first != next) {
                broken = true;
                let _ = std::fs::rename(&path, path.with_extension("seg.damaged"));
                report.segments_set_aside += 1;
                continue;
            }
            let Ok(data) = std::fs::read(&path) else { continue };
            let (mut at, mut count) = (0usize, 0i64);
            while at + HEADER <= data.len() {
                let length = u32::from_le_bytes(data[at..at + 4].try_into().unwrap()) as usize;
                let seq = u64::from_le_bytes(data[at + 4..at + 12].try_into().unwrap()) as i64;
                if seq != first + count || length > 16 * 1024 * 1024 || at + HEADER + length > data.len() {
                    break;
                }
                at += HEADER + length;
                count += 1;
            }
            if at < data.len() {
                broken = true;
                report.torn_bytes += data.len() - at;
                let aside = path.with_extension("seg.damaged");
                if let Ok(mut out) = OpenOptions::new().create(true).append(true).mode(0o600).open(&aside) {
                    let _ = out.write_all(&data[at..]);
                }
                if let Ok(file) = OpenOptions::new().write(true).open(&path) {
                    let _ = file.set_len(at as u64);
                }
            }
            if count == 0 {
                let _ = std::fs::remove_file(&path);
                continue;
            }
            log.view.segments.push((first, count as usize));
            log.sizes.push(at);
            next = first + count;
        }
        (log, next, report)
    }

    pub fn sync(&mut self) {
        if self.dirty {
            if let Some(writer) = self.writer.as_ref() {
                let _ = writer.sync_data();
            }
            self.dirty = false;
        }
    }

    fn path(view: &LogView, first: i64) -> PathBuf {
        view.dir.join(format!("{}.{:020}.seg", view.id, first))
    }

    pub fn oldest(&self, next_seq: i64) -> i64 {
        self.view.segments.first().map_or(next_seq, |s| s.0)
    }

    pub fn view(&self) -> LogView {
        self.view.clone()
    }

    pub fn disk_bytes(&self) -> usize {
        self.sizes.iter().sum()
    }

    /// Delete every segment file of this session.
    pub fn purge(&mut self) {
        self.writer = None;
        for (first, _) in self.view.segments.drain(..) {
            let _ = std::fs::remove_file(Self::path_for(&self.view.dir, &self.view.id, first));
        }
        self.sizes.clear();
        if let Ok(entries) = std::fs::read_dir(&self.view.dir) { // what was set aside from a damaged tail is part of the conversation too
            let prefix = format!("{}.", self.view.id);
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with(&prefix) && name.ends_with(".seg.damaged") {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }

    fn path_for(dir: &Path, id: &str, first: i64) -> PathBuf {
        dir.join(format!("{}.{:020}.seg", id, first))
    }

    pub fn append(&mut self, seq: i64, data: &[u8]) -> std::io::Result<()> {
        let full = self.view.segments.last().map_or(true, |s| s.1 >= self.segment_records);
        if full || self.writer.is_none() {
            let file = OpenOptions::new().create_new(true).append(true).mode(0o600).open(Self::path(&self.view, seq))?;
            self.view.segments.push((seq, 0)); // bookkeeping only after the file exists
            self.sizes.push(0);
            self.writer = Some(file);
            self.prune();
        }
        let mut record = Vec::with_capacity(HEADER + data.len());
        record.extend_from_slice(&(data.len() as u32).to_le_bytes());
        record.extend_from_slice(&(seq as u64).to_le_bytes());
        record.extend_from_slice(data);
        self.writer.as_mut().expect("writer").write_all(&record)?;
        self.dirty = true;
        if let Some(last) = self.view.segments.last_mut() {
            last.1 += 1;
        }
        if let Some(last) = self.sizes.last_mut() {
            *last += record.len();
        }
        Ok(())
    }

    /// Drop whole oldest segments while the rest still holds the retained count.
    fn prune(&mut self) {
        loop {
            let total: usize = self.view.segments.iter().map(|s| s.1).sum();
            match self.view.segments.first() {
                Some(first) if self.view.segments.len() > 1 && total - first.1 >= self.retain => {
                    let _ = std::fs::remove_file(Self::path(&self.view, first.0));
                    self.view.segments.remove(0);
                    self.sizes.remove(0);
                }
                _ => break,
            }
        }
    }
}

impl LogView {
    /// Records with `from <= seq < until`, at most `limit`, read from the segment files.
    pub fn read(&self, from: i64, until: i64, limit: usize) -> std::io::Result<Vec<(i64, Vec<u8>)>> {
        let mut out = Vec::new();
        for (index, (first, count)) in self.segments.iter().enumerate() {
            let next_first = self.segments.get(index + 1).map_or(i64::MAX, |s| s.0);
            if next_first <= from || *first >= until || *count == 0 {
                continue;
            }
            let mut data = Vec::new();
            File::open(Log::path(self, *first))?.read_to_end(&mut data)?;
            let mut at = 0;
            while at + HEADER <= data.len() {
                let length = u32::from_le_bytes(data[at..at + 4].try_into().unwrap()) as usize;
                let seq = u64::from_le_bytes(data[at + 4..at + 12].try_into().unwrap()) as i64;
                if at + HEADER + length > data.len() {
                    break;
                }
                if seq >= until {
                    return Ok(out);
                }
                if seq >= from {
                    out.push((seq, data[at + HEADER..at + HEADER + length].to_vec()));
                    if out.len() >= limit {
                        return Ok(out);
                    }
                }
                at += HEADER + length;
            }
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------- sessions
#[derive(Clone, Copy, PartialEq)]
pub enum State {
    Running,
    Exited,
    Disconnected, // the provider is gone; the session can be resumed
}

#[derive(Clone, Copy, PartialEq)]
pub enum InputState {
    Accepted,
    Submitted,
    Observed,
    Failed,
}

impl InputState {
    pub fn name(self) -> &'static str {
        match self {
            InputState::Accepted => "accepted",
            InputState::Submitted => "submitted",
            InputState::Observed => "observed",
            InputState::Failed => "failed",
        }
    }
}

pub struct Session {
    pub id: String,
    pub text_mode: bool, // records are UTF-8 lines (a provider session), not bytes to show as hex
    pub purged: bool, // the session was deleted: nothing may be written for it again
    pub generation: u64,
    pub producing: bool,
    pub state: State,
    pub state_at: u64, // when the state last changed (unix seconds); kept after a disconnect
    pub codex: Option<crate::codex::Link>,
    pub claude: Option<crate::claude::Link>,
    pub mode: &'static str, // "auto" (permissions skipped, the default of a new session) or "default" (the provider asks for what needs approval)
    pub provider: &'static str, // "codex" or "claude" for a provider session, empty otherwise
    pub codex_thread: Option<String>,
    window: VecDeque<(i64, Vec<u8>)>,
    window_cap: usize,
    pub next_seq: i64,
    pub bytes_total: i64,
    log: Log,
    pub subs: Vec<Arc<Sub>>,
    hasher: RandomState,
    inputs: HashMap<u64, InputState>,
    input_order: VecDeque<u64>,
    pub log_failed: bool,
    pub approvals: Vec<(i64, String)>,
    pub agent_pid: u32,
    pub account: Option<String>, // the account this session is bound to for its whole life (until a rebind of a dead session)
    pub credential_generation: u64,
    pub busy: bool, // a turn is in progress, as the provider reported it (never inferred from silence)
    pub busy_since: u64, // when the open turn began (unix seconds); meaningful while `busy`
    pub last_event_at: u64, // when the provider last sent anything (unix seconds); meaningful while `busy`
    pub model: Option<String>, // the model the provider reported, never guessed
    pub account_issue: Option<String>, // what the provider reported about the account (authentication, limit); cleared by a successful turn
    pub broker_stop: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>, // ends the key broker of this session's provider
    pub process_group: bool, // the process leads its own group (a provider and its helpers): stop the whole group
    pub approval_counter: i64, // approval ids are never reused within a session, across restarts too
    /// lines for the agent's standard input; a writer thread owns the pipe
    pub input_tx: Option<SyncSender<String>>,
}

extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

pub fn rec_line(session: &str, generation: u64, seq: i64, data: &[u8], text: bool) -> Arc<Vec<u8>> {
    let mut body = obj(json!({"ev": "rec", "session": session, "generation": generation, "seq": seq, "data": hex(data)}));
    if text {
        body.insert("text".into(), Value::String(String::from_utf8_lossy(data).into_owned()));
    }
    encode(body)
}

impl Session {
    pub fn new(id: String, producing: bool, dir: &Path, retain: usize, window_cap: usize) -> Session {
        let log = Log::new(dir, &id, retain);
        Session {
            id,
            text_mode: false,
            purged: false,
            generation: 1,
            codex: None,
            claude: None,
            provider: "",
            mode: "auto",
            codex_thread: None,
            producing,
            state: State::Running,
            state_at: now_secs(),
            window: VecDeque::new(),
            window_cap,
            next_seq: 0,
            bytes_total: 0,
            log,
            subs: Vec::new(),
            hasher: RandomState::new(),
            inputs: HashMap::new(),
            input_order: VecDeque::new(),
            log_failed: false,
            approvals: Vec::new(),
            agent_pid: 0,
            account: None,
            credential_generation: 0,
            busy: false,
            busy_since: 0,
            last_event_at: 0,
            model: None,
            account_issue: None,
            broker_stop: None,
            process_group: false,
            approval_counter: 0,
            input_tx: None,
        }
    }

    /// Change the state and remember when (the last known state keeps its time after a disconnect).
    /// A turn opens or closes, as the provider says; opening stamps the start, closing forgets it.
    pub fn set_busy(&mut self, busy: bool) {
        if busy && !self.busy {
            self.busy_since = now_secs();
            self.last_event_at = self.busy_since;
        }
        self.busy = busy;
    }

    /// The provider said something (any message, whether or not it became a line).
    pub fn heard(&mut self) {
        self.last_event_at = now_secs();
    }

    pub fn set_state(&mut self, state: State) {
        if self.state != state {
            self.state = state;
            self.state_at = now_secs();
            crate::registry::mark_dirty();
        }
    }

    /// A session found on disk after a restart: its history and bindings, no process, state `disconnected`.
    pub fn recovered(entry: &crate::registry::Entry, dir: &Path, retain: usize, window_cap: usize) -> (Session, Recovery) {
        let (log, next, report) = Log::recover(dir, &entry.session, retain);
        let mut s = Session::new(entry.session.clone(), false, dir, retain, window_cap);
        s.log = log;
        s.next_seq = next;
        s.bytes_total = s.log.disk_bytes() as i64;
        s.text_mode = true;
        s.generation = entry.generation;
        s.provider = if entry.provider == "claude" { "claude" } else { "codex" };
        s.codex_thread = entry.provider_session.clone();
        s.account = entry.account.clone();
        s.mode = entry.mode;
        s.credential_generation = entry.credential_generation;
        s.state = State::Disconnected;
        s.state_at = if entry.state_at > 0 { entry.state_at } else { now_secs() };
        let from = (next - window_cap as i64).max(0);
        if let Ok(records) = s.log.view().read(from, next, window_cap) {
            s.window = records.into_iter().collect();
        }
        (s, report)
    }

    pub fn sync_log(&mut self) {
        self.log.sync();
    }

    pub fn last_seq(&self) -> i64 {
        self.next_seq - 1
    }

    pub fn oldest(&self) -> i64 {
        self.log.oldest(self.next_seq)
    }

    pub fn mem_start(&self) -> i64 {
        self.window.front().map_or(self.next_seq, |r| r.0)
    }

    pub fn log_view(&self) -> LogView {
        self.log.view()
    }

    pub fn purge_log(&mut self) {
        self.log.purge();
    }

    pub fn disk_bytes(&self) -> usize {
        self.log.disk_bytes()
    }

    pub fn append(&mut self, data: Vec<u8>) {
        if self.log_failed || self.purged {
            return; // a failed log ends the session; a purged session never writes again
        }
        let seq = self.next_seq;
        if let Err(error) = self.log.append(seq, &data) {
            // the record is not sequenced, shown or replayable without its log entry: end the session
            self.log_failed = true;
            self.set_state(State::Exited);
            self.input_tx = None;
            self.stop_process();
            eprintln!("log write failed for {}: {error}", self.id);
            return;
        }
        self.next_seq += 1;
        self.bytes_total += data.len() as i64;
        let line = rec_line(&self.id, self.generation, seq, &data, self.text_mode);
        if self.window.len() == self.window_cap {
            self.window.pop_front();
        }
        self.window.push_back((seq, data));
        let id = self.id.clone();
        self.subs.retain(|sub| !sub.offer(&id, line.clone()));
    }

    pub fn window_after(&self, cursor: i64) -> Vec<(i64, Vec<u8>)> {
        self.window.iter().filter(|r| r.0 > cursor).cloned().collect()
    }

    pub fn snapshot(&self) -> serde_json::Map<String, Value> {
        let skip = self.window.len().saturating_sub(SNAPSHOT_LINES);
        let lines: Vec<Value> = self
            .window
            .iter()
            .skip(skip)
            .map(|(seq, data)| {
                if self.text_mode {
                    json!([seq, String::from_utf8_lossy(data)])
                } else {
                    json!([seq, format!("{:08} {}", seq, hex(&data[..data.len().min(44)]))])
                }
            })
            .collect();
        let mut body = obj(json!({"session": self.id, "generation": self.generation, "seq": self.last_seq(), "lines": lines}));
        if self.text_mode {
            body.insert("text".into(), json!(true));
        }
        body
    }

    // ---- input bookkeeping: a request id is dispatched at most once within the tracked window
    fn key(&self, request_id: &str) -> u64 {
        self.hasher.hash_one(request_id)
    }

    pub fn input_state(&self, request_id: &str) -> Option<InputState> {
        self.inputs.get(&self.key(request_id)).copied()
    }

    pub fn record_input(&mut self, request_id: &str) {
        let key = self.key(request_id);
        self.inputs.insert(key, InputState::Accepted);
        self.input_order.push_back(key);
        while self.input_order.len() > MAX_TRACKED_INPUTS {
            if let Some(old) = self.input_order.pop_front() {
                self.inputs.remove(&old);
            }
        }
    }

    /// Add a subscriber once: a second subscription of one connection must not double delivery.
    pub fn add_sub(&mut self, sub: &Arc<Sub>) {
        if !self.subs.iter().any(|s| Arc::ptr_eq(s, sub)) {
            self.subs.push(sub.clone());
        }
    }

    /// Advance an input to a later state (never backwards) and tell the viewers.
    pub fn advance_input(&mut self, request_id: &str, to: InputState) {
        let rank = |s: InputState| s as u8;
        let key = self.key(request_id);
        let Some(current) = self.inputs.get(&key).copied() else { return };
        if rank(to) <= rank(current) {
            return;
        }
        self.inputs.insert(key, to);
        if to != InputState::Accepted {
            let line = encode(obj(json!({"ev": "input", "session": self.id, "request_id": request_id, "state": to.name()})));
            for sub in &self.subs {
                sub.push(line.clone());
            }
        }
    }

    /// End the agent or provider process (the whole group for a provider); a pid that was already
    /// reaped is zero and never signalled.
    pub fn stop_process(&mut self) {
        self.stop_broker();
        if self.agent_pid != 0 {
            let target = if self.process_group { -(self.agent_pid as i32) } else { self.agent_pid as i32 };
            unsafe { kill(target, 9) };
            self.agent_pid = 0;
        }
    }

    pub fn stop_broker(&mut self) {
        if let Some(stop) = self.broker_stop.take() {
            stop.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    pub fn add_approval(&mut self, number: i64, prompt: String) {
        if self.approvals.len() >= MAX_APPROVALS {
            return; // a bounded number of pending approvals
        }
        let prompt: String = prompt.chars().take(200).collect();
        let line = encode(obj(json!({"ev": "approval", "session": self.id, "approval_id": number, "prompt": prompt})));
        self.approvals.push((number, prompt));
        for sub in &self.subs {
            sub.push(line.clone());
        }
    }
}
