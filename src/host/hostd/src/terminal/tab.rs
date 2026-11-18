//! One terminal (M23-P02): a shell on a pty, the emulator (`lico-vt`) that reads what the shell writes, and a change number that a watch waits on. Two threads serve it: a reader
//! (polls the master, feeds the emulator, writes the emulator's answers back to the shell) and a waiter (the shell's end). Nothing here polls on a timer for a client: a watch
//! sleeps on the condition variable until the number moves.

use super::encode;
use super::pty::{self, Master, ReadResult, WriteError};
use crate::procs;
use lico_vt::Term;
use serde_json::{json, Value};
use std::os::unix::process::ExitStatusExt;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Lines of scrollback each terminal keeps (the component bounds it at about 10 MB in cells).
pub const SCROLLBACK: usize = 2000;
/// How long an input waits for the pty to take it.
pub const WRITE_PATIENCE: Duration = Duration::from_secs(2);
/// After a close's hang-up, how long the shell's group has before it is killed.
pub const KILL_AFTER: Duration = Duration::from_secs(1);
/// After the shell ended, how long the reader waits for more output before it stops (a background program may still hold the pty).
const LINGER: Duration = Duration::from_millis(400);
const SIGHUP: i32 = 1;
const SIGKILL: i32 = 9;
const SIGCONT: i32 = 18;

pub struct State {
    pub term: Term,
    /// Moves whenever something a watch shows moved; starts at 1 so that a watch with `seq` 0 is answered at once.
    pub change: u64,
    pub running: bool,
    pub exit: Option<i32>,
}

pub struct Tab {
    pub id: String,
    pub cwd: String,
    pub shell: String,
    pub pid: u32,
    /// Seconds since the epoch.
    pub started: u64,
    start: String,
    master: Master,
    state: Mutex<State>,
    changed: Condvar,
    closed: AtomicBool,
}

/// The shell's end as a number: its code, or 128 plus the signal that ended it.
fn exit_code(child: &mut Child) -> i32 {
    match child.wait() {
        Ok(status) => status.code().or_else(|| status.signal().map(|s| 128 + s)).unwrap_or(-1),
        Err(_) => -1,
    }
}

/// Starts `command` on a new pty of `cols` x `rows`. `cwd` and `shell` are what the row shows (the command already has them).
pub fn start(id: String, command: Command, cwd: String, shell: String, cols: u16, rows: u16) -> Result<Arc<Tab>, &'static str> {
    let (master, slave) = pty::open_pair(cols, rows).map_err(|_| "pty_failed")?;
    let mut child = pty::spawn(command, slave).map_err(|_| "spawn_failed")?;
    let pid = child.id();
    let started = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let tab = Arc::new(Tab {
        id,
        cwd,
        shell,
        pid,
        started,
        start: procs::start_time(pid).unwrap_or_default(),
        master,
        state: Mutex::new(State { term: Term::with_scrollback(cols, rows, SCROLLBACK), change: 1, running: true, exit: None }),
        changed: Condvar::new(),
        closed: AtomicBool::new(false),
    });
    let reader = tab.clone();
    std::thread::Builder::new().name(format!("term-read-{}", tab.id)).spawn(move || reader.read_loop()).map_err(|_| "spawn_failed")?;
    let waiter = tab.clone();
    std::thread::Builder::new()
        .name(format!("term-wait-{}", tab.id))
        .spawn(move || {
            let code = exit_code(&mut child);
            waiter.ended(code);
        })
        .map_err(|_| "spawn_failed")?;
    Ok(tab)
}

impl Tab {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    fn ended(&self, code: i32) {
        let mut st = self.lock();
        st.running = false;
        st.exit = Some(code);
        st.change += 1;
        self.changed.notify_all();
    }

    fn feed(&self, bytes: &[u8]) {
        let reply = {
            let mut st = self.lock();
            st.term.feed(bytes);
            st.change += 1;
            self.changed.notify_all();
            st.term.take_reply()
        };
        // the emulator's answers (the cursor position, the device attributes) go back to the program as if typed; a program that does not read them cannot stop the reader
        if !reply.is_empty() {
            let _ = self.master.write(&reply, Duration::from_millis(200));
        }
    }

    fn read_loop(&self) {
        let mut buf = vec![0u8; 16 * 1024];
        let mut quiet_since: Option<Instant> = None;
        loop {
            if self.is_closed() {
                return;
            }
            if !self.master.wait_readable(200) {
                if !self.lock().running && quiet_since.get_or_insert_with(Instant::now).elapsed() >= LINGER {
                    return;
                }
                continue;
            }
            match self.master.read(&mut buf) {
                ReadResult::Data(n) => {
                    quiet_since = None;
                    self.feed(&buf[..n]);
                }
                ReadResult::Wait => {}
                ReadResult::End => return,
            }
        }
    }

    /// Sends `bytes` to the program. `terminal_ended` when the shell has ended, `busy` when the pty took nothing for `WRITE_PATIENCE`.
    pub fn input(&self, bytes: &[u8]) -> Result<usize, &'static str> {
        if self.is_closed() || !self.lock().running {
            return Err("terminal_ended");
        }
        match self.master.write(bytes, WRITE_PATIENCE) {
            Ok(()) => Ok(bytes.len()),
            Err(WriteError::Timeout) => Err("busy"),
            Err(WriteError::Closed) => Err("terminal_ended"),
        }
    }

    /// Sets the size of the pty (the program gets its window-size signal) and of the emulator; nothing happens, and no change is announced, when the size is the one it has.
    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), &'static str> {
        if self.lock().term.size() == (cols, rows) {
            return Ok(());
        }
        self.master.set_size(cols, rows).map_err(|_| "pty_failed")?;
        let mut st = self.lock();
        st.term.resize(cols, rows);
        st.change += 1;
        self.changed.notify_all();
        Ok(())
    }

    /// Hangs up the shell's group (and wakes it if it was stopped); after `KILL_AFTER` a group that is still there is killed. Wakes every watch. Done once.
    pub fn close(&self) {
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        procs::signal_group(self.pid, &self.start, SIGHUP);
        procs::signal_group(self.pid, &self.start, SIGCONT);
        drop(self.lock()); // a watch that checked `closed` just before is waiting now, and the notice below reaches it
        self.changed.notify_all();
        let (pid, start) = (self.pid, self.start.clone()); // not the tab itself: its pty is freed as soon as its two threads end
        std::thread::spawn(move || {
            std::thread::sleep(KILL_AFTER);
            procs::signal_group(pid, &start, SIGKILL);
        });
    }

    /// The row of the list.
    pub fn row(&self) -> Value {
        let st = self.lock();
        let (cols, rows) = st.term.size();
        json!({"tab": self.id, "cwd": self.cwd, "shell": self.shell, "pid": self.pid, "title": st.term.title(), "cols": cols, "rows": rows, "running": st.running, "exit": st.exit, "started": self.started})
    }

    /// Waits (with no timeout) until the change number is above `seq`, then tells the screen `scroll` rows above the bottom. A closed tab is `unknown_tab`.
    pub fn watch(&self, seq: u64, scroll: usize) -> Result<Value, &'static str> {
        let mut st = self.lock();
        loop {
            if self.is_closed() {
                return Err("unknown_tab");
            }
            if st.change > seq {
                return Ok(self.snapshot(&st, scroll));
            }
            st = self.changed.wait(st).unwrap_or_else(|e| e.into_inner());
        }
    }

    fn snapshot(&self, st: &State, scroll: usize) -> Value {
        let view = st.term.view(scroll);
        let (cols, rows) = st.term.size();
        let lines: Vec<Value> = view.lines.iter().map(|l| encode::runs(l)).collect();
        json!({
            "tab": self.id, "seq": st.change, "cols": cols, "rows": rows, "title": st.term.title(),
            "cursor": view.cursor.map(|(r, c)| json!([r, c])),
            "modes": encode::modes(st.term.modes()),
            "scroll": view.scroll, "top": view.top, "total": view.total,
            "lines": lines, "wrapped": view.wrapped, "bells": st.term.bells(),
            "running": st.running, "exit": st.exit,
        })
    }

    /// The text from `a` to `b` (absolute line, column), either order, as the emulator reads it now.
    pub fn text(&self, a: (usize, u16), b: (usize, u16)) -> String {
        self.lock().term.text_between(a, b)
    }
}
