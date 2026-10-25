//! Provider sign-in supervised by the host. The provider's own login command runs in the account's private directory;
//! Lico reads no token and writes none of the provider's files. What a login prints and what a person pastes live in memory
//! only: the URL is handed to the window on request, the pasted code goes straight to the process, and nothing is logged.

use serde_json::json;
use std::collections::HashMap;
use std::io::Write;
use std::process::{ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::core::{spawn_provider_process, Reply, Shared};
use crate::session::obj;

const MAX_CODE: usize = 4096;

extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

pub struct Login {
    state: &'static str, // waiting, browser, code, succeeded, failed, cancelled, timed_out
    url: Option<String>,
    needs_code: bool,
    exit_code: Option<i32>,
    started: Instant,
    cancel: bool,
    stdin: Option<ChildStdin>,
}

pub type Logins = Mutex<HashMap<String, Arc<Mutex<Login>>>>;

fn running(state: &str) -> bool {
    matches!(state, "waiting" | "browser" | "code")
}

/// The login command line: the provider's own, or the test double given with `--login-bin`.
fn command_of(shared: &Shared, provider: &str, words: &[&str]) -> Command {
    let mut command = if let Some(bin) = &shared.cfg.login_bin {
        let mut c = Command::new(bin);
        c.args(&shared.cfg.login_args);
        c
    } else if provider == "claude" {
        Command::new(&shared.cfg.claude)
    } else {
        Command::new(&shared.cfg.codex)
    };
    if shared.cfg.login_bin.is_none() && provider == "claude" {
        command.arg("auth");
    }
    command.args(words);
    command
}

/// The private environment of a login: the account's directories and nothing else of the host's (no display, so the browser is the person's to open).
fn private_environment(command: &mut Command, shared: &Shared, account: &str) -> Result<(), &'static str> {
    let homes = shared.accounts.lock().unwrap().homes(account).ok_or("unknown_account")?;
    // BROWSER names a harmless command: the provider's own attempt to open the address then does nothing and the window is the only opener (one tab)
    command.env_clear().env("HOME", &homes.home).env(homes.variable, &homes.provider_home).env("NO_COLOR", "1").env("BROWSER", "/usr/bin/true");
    for key in ["PATH", "LANG"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    command.current_dir(&homes.home);
    Ok(())
}

/// Remove terminal escape sequences (CSI, and OSC such as the hyperlinks of OSC 8, ended by BEL or ESC \\) and control characters.
fn strip_ansi(line: &str) -> String {
    let mut out = String::new();
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            match chars.peek() {
                Some('[') => {
                    chars.next();
                    for d in chars.by_ref() {
                        if d.is_ascii_alphabetic() {
                            break;
                        }
                    }
                }
                Some(']') => {
                    chars.next();
                    while let Some(d) = chars.next() {
                        if d == '\u{7}' {
                            break;
                        }
                        if d == '\u{1b}' {
                            chars.next_if_eq(&'\\');
                            break;
                        }
                    }
                }
                _ => {}
            }
        } else if !c.is_control() || c == '\t' {
            out.push(c);
        }
    }
    out
}

/// The sign-in address of a printed line: the target of an OSC 8 hyperlink when there is one, else the first address of the visible text.
/// It stops at a space, an escape or a bell, so what follows a link is never glued to it.
fn address_of(raw: &str) -> Option<String> {
    let take = |s: &str| -> String { s.chars().take_while(|c| !c.is_whitespace() && !c.is_control()).collect() };
    let mut rest = raw;
    while let Some(at) = rest.find("\u{1b}]8;") {
        let after = &rest[at + 4..];
        if let Some(semi) = after.find(';') {
            let target = take(&after[semi + 1..]);
            if target.starts_with("https://") {
                return Some(target);
            }
        }
        rest = &rest[at + 4..];
    }
    let plain = strip_ansi(raw);
    let at = plain.find("https://")?;
    let url = take(&plain[at..]);
    Some(url.trim_end_matches(|c: char| ".,;)".contains(c)).to_string())
}

/// Notes that the provider asks for a code (its wording, with or without a newline after it).
fn asks_for_code(l: &mut Login, text: &str) -> bool {
    let text = strip_ansi(text).to_lowercase();
    if text.contains("paste code") || text.contains("enter the code") {
        l.needs_code = true;
        if running(l.state) {
            l.state = "code";
        }
        return true;
    }
    false
}

fn read_output(login: Arc<Mutex<Login>>, mut stream: impl std::io::Read + Send + 'static) {
    std::thread::spawn(move || {
        // What the provider prints is read as it comes, not by lines: its prompt for a code ends without a newline. At most 8 KiB are
        // kept and the pipe is always drained; a byte that is not UTF-8 never stops the reading.
        let mut pending: Vec<u8> = Vec::new();
        let mut chunk = [0u8; 1024];
        loop {
            let n = match stream.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            pending.extend_from_slice(&chunk[..n]);
            if pending.len() > 8192 {
                pending.drain(..pending.len() - 4096);
            }
            while let Some(newline) = pending.iter().position(|b| *b == b'\n') {
                let raw = String::from_utf8_lossy(&pending[..newline]).into_owned();
                pending.drain(..=newline);
                let mut l = login.lock().unwrap();
                if l.url.is_none() {
                    if let Some(url) = address_of(&raw) {
                        l.url = Some(url);
                        if running(l.state) {
                            l.state = "browser";
                        }
                    }
                }
                asks_for_code(&mut l, &raw);
            }
            // the prompt of a code has no newline: the bytes so far are looked at too
            let mut l = login.lock().unwrap();
            if asks_for_code(&mut l, &String::from_utf8_lossy(&pending)) {
                pending.clear();
            }
        }
    });
}

/// Start the sign-in of an account (`method` is `browser`, or `device` for Codex's device-code flow).
pub fn start(shared: &Arc<Shared>, account: &str, method: &str) -> Reply {
    let provider = shared.accounts.lock().unwrap().get(account).ok_or("unknown_account")?.provider.clone();
    let words: Vec<&str> = match (provider.as_str(), method) {
        ("codex", "browser") => vec!["login"],
        ("codex", "device") => vec!["login", "--device-auth"],
        ("claude", "browser") => vec!["login"],
        _ => return Err("bad_request"),
    };
    let mut table = shared.logins.lock().unwrap();
    if table.get(account).map_or(false, |l| running(l.lock().unwrap().state)) {
        return Err("busy"); // one login per account at a time
    }
    let mut command = command_of(shared, &provider, &words);
    private_environment(&mut command, shared, account)?;
    command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = spawn_provider_process(command).map_err(|_| "login_unavailable")?;
    let pid = child.id();
    let login = Arc::new(Mutex::new(Login { state: "waiting", url: None, needs_code: false, exit_code: None, started: Instant::now(), cancel: false, stdin: child.stdin.take() }));
    if let Some(out) = child.stdout.take() {
        read_output(login.clone(), out);
    }
    if let Some(err) = child.stderr.take() {
        read_output(login.clone(), err);
    }
    table.insert(account.to_string(), login.clone());
    let timeout = Duration::from_secs(shared.cfg.login_timeout);
    std::thread::spawn(move || loop {
        let finished = child.try_wait().ok().flatten();
        let mut l = login.lock().unwrap();
        if let Some(status) = finished {
            l.exit_code = status.code();
            l.stdin = None;
            l.url = None; // what the provider printed is for the sign-in in progress only
            l.needs_code = false;
            unsafe { kill(-(pid as i32), 9) }; // helpers the login started go with it
            if running(l.state) {
                l.state = if status.success() { "succeeded" } else { "failed" };
            }
            return;
        }
        if l.cancel || l.started.elapsed() > timeout {
            l.state = if l.cancel { "cancelled" } else { "timed_out" };
            l.stdin = None;
            l.url = None;
            l.needs_code = false;
            drop(l);
            unsafe { kill(-(pid as i32), 9) }; // the whole group: a wrapper's helper may hold the callback port
            let _ = child.kill();
            let _ = child.wait();
            return;
        }
        drop(l);
        std::thread::sleep(Duration::from_millis(100));
    });
    Ok(obj(json!({"account": account, "state": "waiting"})))
}

/// How many sign-ins are running now.
pub fn running_count(shared: &Shared) -> usize {
    shared.logins.lock().unwrap().values().filter(|l| running(l.lock().unwrap().state)).count()
}

/// A pasted code goes to the process's standard input and nowhere else.
pub fn input(shared: &Shared, account: &str, code: &str) -> Reply {
    if code.is_empty() || code.len() > MAX_CODE || code.chars().any(char::is_control) {
        return Err("bad_request");
    }
    let table = shared.logins.lock().unwrap();
    let login = table.get(account).ok_or("no_login")?;
    let mut l = login.lock().unwrap();
    if !running(l.state) || !l.needs_code {
        return Err("no_code_expected");
    }
    let stdin = l.stdin.as_mut().ok_or("no_login")?;
    stdin.write_all(code.as_bytes()).and_then(|_| stdin.write_all(b"\n")).and_then(|_| stdin.flush()).map_err(|_| "no_login")?;
    l.needs_code = false;
    l.state = "waiting";
    Ok(obj(json!({"account": account})))
}

pub fn cancel(shared: &Shared, account: &str) -> Reply {
    let table = shared.logins.lock().unwrap();
    let login = table.get(account).ok_or("no_login")?;
    login.lock().unwrap().cancel = true;
    Ok(obj(json!({"account": account})))
}

/// The state of the last login of an account. With `check`, Codex's own `login status` also says whether the account is signed in
/// (Claude Code has no status command known for the pinned version: that stays `null`, not a guess).
pub fn status(shared: &Arc<Shared>, account: &str, check: bool) -> Reply {
    let provider = shared.accounts.lock().unwrap().get(account).ok_or("unknown_account")?.provider.clone();
    let mut reply = match shared.logins.lock().unwrap().get(account) {
        Some(login) => {
            let l = login.lock().unwrap();
            obj(json!({"state": l.state, "url": l.url, "needs_code": l.needs_code, "exit_code": l.exit_code, "seconds": l.started.elapsed().as_secs()}))
        }
        None => obj(json!({"state": "none"})),
    };
    reply.insert("account".to_string(), json!(account));
    if check {
        reply.insert("signed_in".to_string(), if provider == "codex" { json!(signed_in(shared, account)) } else { serde_json::Value::Null });
    }
    Ok(reply)
}

fn signed_in(shared: &Shared, account: &str) -> Option<bool> {
    let mut command = command_of(shared, "codex", &["login", "status"]);
    private_environment(&mut command, shared, account).ok()?;
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    let mut child = spawn_provider_process(command).ok()?;
    let began = Instant::now();
    loop {
        if let Some(status) = child.try_wait().ok()? {
            return Some(status.success());
        }
        if began.elapsed() > Duration::from_secs(5) {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
