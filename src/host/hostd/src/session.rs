//! Sessions, the durable sequenced log and subscriber queues.
//!
//! A session keeps a small bounded window of recent records in memory and writes
//! every record to an owner-only segment file before it can leave the window.
//! Older replay is read from the segments, so memory does not grow with history.

use serde_json::{json, Value};
use std::collections::{HashSet, VecDeque};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::payload::hex;

pub const SNAPSHOT_LINES: usize = 40;
const HEADER: usize = 12; // u32 length + u64 sequence

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
            let notice = encode(json!({"ev": "overflow", "session": session}).as_object().cloned().unwrap());
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

pub struct Log {
    view: LogView,
    segment_records: usize,
    retain: usize,
    writer: Option<File>,
}

impl Log {
    pub fn new(dir: &Path, id: &str, retain: usize) -> Log {
        Log {
            view: LogView { dir: dir.to_path_buf(), id: id.to_string(), segments: Vec::new() },
            segment_records: (retain / 4).clamp(16, 4096),
            retain,
            writer: None,
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

    pub fn append(&mut self, seq: i64, data: &[u8]) -> std::io::Result<()> {
        let full = self.view.segments.last().map_or(true, |s| s.1 >= self.segment_records);
        if full || self.writer.is_none() {
            self.view.segments.push((seq, 0));
            self.writer = Some(OpenOptions::new().create_new(true).append(true).mode(0o600).open(Self::path(&self.view, seq))?);
            self.prune();
        }
        let mut record = Vec::with_capacity(HEADER + data.len());
        record.extend_from_slice(&(data.len() as u32).to_le_bytes());
        record.extend_from_slice(&(seq as u64).to_le_bytes());
        record.extend_from_slice(data);
        self.writer.as_mut().expect("writer").write_all(&record)?;
        if let Some(last) = self.view.segments.last_mut() {
            last.1 += 1;
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
}

pub struct Session {
    pub id: String,
    pub producing: bool,
    pub state: State,
    window: VecDeque<(i64, Vec<u8>)>,
    window_cap: usize,
    pub next_seq: i64,
    pub bytes_total: i64,
    log: Log,
    pub subs: Vec<Arc<Sub>>,
}

pub fn rec_line(session: &str, seq: i64, data: &[u8]) -> Arc<Vec<u8>> {
    encode(json!({"ev": "rec", "session": session, "generation": 1, "seq": seq, "data": hex(data)}).as_object().cloned().unwrap())
}

impl Session {
    pub fn new(id: String, producing: bool, dir: &Path, retain: usize, window_cap: usize) -> Session {
        let log = Log::new(dir, &id, retain);
        Session { id, producing, state: State::Running, window: VecDeque::new(), window_cap, next_seq: 0, bytes_total: 0, log, subs: Vec::new() }
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

    pub fn append(&mut self, data: Vec<u8>) {
        let seq = self.next_seq;
        if let Err(error) = self.log.append(seq, &data) {
            eprintln!("log write failed for {}: {error}", self.id);
        }
        self.next_seq += 1;
        self.bytes_total += data.len() as i64;
        let line = rec_line(&self.id, seq, &data);
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
            .map(|(seq, data)| json!([seq, format!("{:08} {}", seq, hex(&data[..data.len().min(44)]))]))
            .collect();
        json!({"session": self.id, "generation": 1, "seq": self.last_seq(), "lines": lines}).as_object().cloned().unwrap()
    }
}
