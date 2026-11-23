//! The operations of the terminal (`terminal.open`, `.list`, `.close`, `.input`, `.resize`, `.watch`, `.text`) and the table of terminals behind them. Registered through the driver
//! port; the hubs never name this module. All seven are socket-only (`admin::SOCKET_ONLY_OPS`): a terminal is a shell that runs as the person, and `POST /v1` does not open one.

use super::tab;
use crate::driver::{Opaque, Ops};
use serde_json::{json, Map, Value};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

/// The most terminals the host holds: the same bound as the browser's tabs.
pub const MAX_TERMINALS: usize = 8;
/// The most bytes one `terminal.input` carries.
pub const MAX_INPUT: usize = 64 * 1024;
/// The size of a terminal opened with no size.
pub const DEFAULT_SIZE: (u16, u16) = (80, 24);

pub const NAMES: [&str; 7] = ["terminal.open", "terminal.list", "terminal.close", "terminal.input", "terminal.resize", "terminal.watch", "terminal.text"];

type Reply = Result<Map<String, Value>, &'static str>;

/// What a terminal is started from, given to `run` so that a test can give its own: the host's check of a folder (what `session.create` uses) and the host's environment.
pub struct Host<'a> {
    pub folder: &'a dyn Fn(&str) -> Result<String, &'static str>,
    pub vars: Vec<(String, String)>,
}

impl<'a> Host<'a> {
    /// The host as it is: its own environment (a name or a value that is not text is left out).
    pub fn of_process(folder: &'a dyn Fn(&str) -> Result<String, &'static str>) -> Host<'a> {
        Host { folder, vars: std::env::vars_os().filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?))).collect() }
    }

    fn var(&self, name: &str) -> Option<&str> {
        self.vars.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }
}

fn obj(v: Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap_or_default()
}

/// `t1`, `t2`, ...: a letter and the number, no leading zero.
pub fn valid_tab_id(v: &Value) -> bool {
    matches!(v, Value::String(s) if s.len() >= 2 && s.len() <= 7 && s.starts_with('t') && s[1..].bytes().all(|b| b.is_ascii_digit()) && !s[1..].starts_with('0'))
}

fn in_range(v: &Value, lo: u64, hi: u64) -> bool {
    v.as_u64().is_some_and(|n| (lo..=hi).contains(&n))
}

fn position(v: &Value) -> bool {
    matches!(v, Value::Array(a) if a.len() == 2 && in_range(&a[0], 0, 1_000_000) && in_range(&a[1], 0, 1_000))
}

pub fn valid(op: &str, request: &Map<String, Value>) -> bool {
    let allowed: &[&str] = match op {
        "terminal.open" => &["op", "v", "id", "cwd", "shell", "cols", "rows"],
        "terminal.list" => &["op", "v", "id"],
        "terminal.close" => &["op", "v", "id", "tab"],
        "terminal.input" => &["op", "v", "id", "tab", "text", "data"],
        "terminal.resize" => &["op", "v", "id", "tab", "cols", "rows"],
        "terminal.watch" => &["op", "v", "id", "tab", "seq", "scroll"],
        "terminal.text" => &["op", "v", "id", "tab", "from", "to"],
        _ => return false,
    };
    let required: &[&str] = match op {
        "terminal.close" | "terminal.watch" => &["tab"],
        "terminal.input" => &["tab"],
        "terminal.resize" => &["tab", "cols", "rows"],
        "terminal.text" => &["tab", "from", "to"],
        _ => &[],
    };
    let fields = request.iter().all(|(k, v)| {
        allowed.contains(&k.as_str())
            && match k.as_str() {
                "op" => true,
                "v" => v.as_i64() == Some(1),
                "id" => matches!(v, Value::String(s) if !s.is_empty() && s.len() <= 64),
                "tab" => valid_tab_id(v),
                "cwd" => matches!(v, Value::String(s) if crate::folders::valid_path_shape(s)),
                "shell" => matches!(v, Value::String(s) if s.starts_with('/') && crate::folders::valid_path_shape(s)),
                "cols" => in_range(v, 1, u64::from(lico_vt::MAX_COLS)),
                "rows" => in_range(v, 1, u64::from(lico_vt::MAX_ROWS)),
                "seq" => v.is_u64(),
                "scroll" => in_range(v, 0, 1_000_000),
                "text" => matches!(v, Value::String(s) if !s.is_empty() && s.len() <= MAX_INPUT),
                "data" => matches!(v, Value::String(s) if crate::base64::base64_decode(s).is_some_and(|b| !b.is_empty() && b.len() <= MAX_INPUT)),
                "from" | "to" => position(v),
                _ => false,
            }
    });
    // an input carries its bytes as text or as base64, one of the two
    let one_payload = op != "terminal.input" || (request.contains_key("text") != request.contains_key("data"));
    fields && one_payload && required.iter().all(|k| request.contains_key(*k))
}

/// The names of the host's environment that a shell is given (M23-P09): where things are, who and what language the person is, the display and the input method, the ssh agent's
/// socket, the person's chosen editor and pager, and the names that point an interactive shell at the person's own start-up files (`ENV` of `sh`, `ZDOTDIR` of `zsh`, `INPUTRC` of the
/// line editor; `BASH_ENV` is left out on purpose: it names a file that every non-interactive `bash` a program starts would run). Everything else the host was started with (the keys a person exported for one program, the tokens a launcher hands its children, a
/// proxy address with a password in it) stays out; the person's own start-up files add what they want to every shell.
const SHELL_ENV_NAMES: [&str; 24] = [
    "PATH", "HOME", "USER", "LOGNAME", "SHELL", "LANG", "LANGUAGE", "TZ", "TMPDIR", "DISPLAY", "WAYLAND_DISPLAY", "XAUTHORITY", "DBUS_SESSION_BUS_ADDRESS", "SSH_AUTH_SOCK", "EDITOR", "VISUAL", "PAGER",
    "DESKTOP_SESSION", "XMODIFIERS", "GTK_IM_MODULE", "QT_IM_MODULE", "ENV", "ZDOTDIR", "INPUTRC",
];
/// The locale categories and the base directories of the desktop are whole families of names that name places and languages, not secrets.
const SHELL_ENV_PREFIXES: [&str; 2] = ["LC_", "XDG_"];

/// Whether a shell is given the host's variable of this name: a listed name or a name of a listed family, matched exactly (in its case), and `LICO_SOCKET` (so that `lico` finds its host
/// from inside: it is the person's own socket, which a program that runs as the person reaches by its path anyway).
fn given_to_shell(name: &str) -> bool {
    name == "LICO_SOCKET" || SHELL_ENV_NAMES.contains(&name) || SHELL_ENV_PREFIXES.iter().any(|prefix| name.starts_with(prefix))
}

/// The environment of the shell: the names of the short list above out of the host's own (no other `LICO_*` name: a terminal is not given what the host was given), with `TERM` and
/// `COLORTERM` as the emulator speaks them.
pub fn child_env(vars: impl IntoIterator<Item = (String, String)>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = vars.into_iter().filter(|(k, _)| given_to_shell(k)).collect();
    out.push(("TERM".into(), "xterm-256color".into()));
    out.push(("COLORTERM".into(), "truecolor".into()));
    out
}

/// The shell to start: the one asked for, else the person's (`$SHELL`), else `/bin/sh`; each must be an executable file. A shell that was asked for and is not there is not
/// replaced by another one.
pub fn choose_shell(asked: Option<&str>, env_shell: Option<&str>, is_exec: &dyn Fn(&str) -> bool) -> Result<String, &'static str> {
    if let Some(shell) = asked {
        return if is_exec(shell) { Ok(shell.to_string()) } else { Err("shell_missing") };
    }
    [env_shell.filter(|s| s.starts_with('/')), Some("/bin/sh")].into_iter().flatten().find(|s| is_exec(s)).map(str::to_string).ok_or("shell_missing")
}

fn executable_file(path: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

pub struct Terminals {
    tabs: Mutex<Vec<Arc<tab::Tab>>>,
    next: AtomicU32,
}

impl Terminals {
    pub fn new() -> Arc<Terminals> {
        Arc::new(Terminals { tabs: Mutex::new(Vec::new()), next: AtomicU32::new(1) })
    }

    fn find(&self, request: &Map<String, Value>) -> Result<Arc<tab::Tab>, &'static str> {
        let id = request.get("tab").and_then(Value::as_str).unwrap_or("");
        self.tabs.lock().unwrap_or_else(|e| e.into_inner()).iter().find(|t| t.id == id).cloned().ok_or("unknown_tab")
    }

    fn size_of(request: &Map<String, Value>) -> (u16, u16) {
        let n = |k: &str, d: u16| request.get(k).and_then(Value::as_u64).map_or(d, |n| n as u16);
        (n("cols", DEFAULT_SIZE.0), n("rows", DEFAULT_SIZE.1))
    }

    /// Runs a valid `op`.
    pub fn run(&self, op: &str, request: &Map<String, Value>, host: &Host<'_>) -> Reply {
        match op {
            "terminal.open" => self.open(request, host),
            "terminal.list" => {
                let rows: Vec<Value> = self.tabs.lock().unwrap_or_else(|e| e.into_inner()).iter().map(|t| t.row()).collect();
                Ok(obj(json!({"terminals": rows, "max_terminals": MAX_TERMINALS})))
            }
            "terminal.close" => {
                let t = self.find(request)?;
                self.tabs.lock().unwrap_or_else(|e| e.into_inner()).retain(|x| x.id != t.id);
                t.close();
                Ok(obj(json!({"closed": true})))
            }
            "terminal.input" => {
                let t = self.find(request)?;
                let bytes = match request.get("text").and_then(Value::as_str) {
                    Some(s) => s.as_bytes().to_vec(),
                    None => request.get("data").and_then(Value::as_str).and_then(crate::base64::base64_decode).ok_or("bad_request")?,
                };
                Ok(obj(json!({"written": t.input(&bytes)?})))
            }
            "terminal.resize" => {
                let t = self.find(request)?;
                let (cols, rows) = Self::size_of(request);
                t.resize(cols, rows)?;
                Ok(obj(json!({"cols": cols, "rows": rows})))
            }
            "terminal.watch" => {
                let t = self.find(request)?; // the table's lock is not held while the watch waits
                let seq = request.get("seq").and_then(Value::as_u64).unwrap_or(0);
                let scroll = request.get("scroll").and_then(Value::as_u64).unwrap_or(0) as usize;
                Ok(obj(t.watch(seq, scroll)?))
            }
            "terminal.text" => {
                let t = self.find(request)?;
                let at = |k: &str| {
                    let a = request.get(k).and_then(Value::as_array).cloned().unwrap_or_default();
                    (a.first().and_then(Value::as_u64).unwrap_or(0) as usize, a.get(1).and_then(Value::as_u64).unwrap_or(0) as u16)
                };
                Ok(obj(json!({"text": t.text(at("from"), at("to"))})))
            }
            _ => Err("unknown_op"),
        }
    }

    fn open(&self, request: &Map<String, Value>, host: &Host<'_>) -> Reply {
        let mut tabs = self.tabs.lock().unwrap_or_else(|e| e.into_inner());
        if tabs.len() >= MAX_TERMINALS {
            return Err("tab_limit");
        }
        let cwd = match request.get("cwd").and_then(Value::as_str) {
            Some(path) => (host.folder)(path)?,
            None => (host.folder)(host.var("HOME").unwrap_or_default())?,
        };
        let shell = choose_shell(request.get("shell").and_then(Value::as_str), host.var("SHELL"), &executable_file)?;
        let (cols, rows) = Self::size_of(request);
        let mut command = Command::new(&shell);
        command.env_clear().current_dir(&cwd).envs(child_env(host.vars.iter().cloned()));
        let id = format!("t{}", self.next.fetch_add(1, Ordering::SeqCst));
        let t = tab::start(id.clone(), command, cwd.clone(), shell.clone(), cols, rows)?;
        let pid = t.pid;
        tabs.push(t);
        Ok(obj(json!({"tab": id, "pid": pid, "cwd": cwd, "shell": shell, "cols": cols, "rows": rows})))
    }

    /// Ends every terminal (the host is stopping).
    pub fn close_all(&self) {
        let all: Vec<Arc<tab::Tab>> = std::mem::take(&mut *self.tabs.lock().unwrap_or_else(|e| e.into_inner()));
        for t in all {
            t.close();
        }
    }
}

impl Ops for Terminals {
    fn names(&self) -> &'static [&'static str] {
        &NAMES
    }
    fn valid(&self, op: &str, request: &Map<String, Value>) -> bool {
        valid(op, request)
    }
    fn call(&self, shared: &Opaque, op: &str, request: &Map<String, Value>) -> Reply {
        let shared = shared.clone().downcast::<crate::core::Shared>().map_err(|_| "spawn_failed")?;
        self.run(op, request, &Host::of_process(&|path| crate::folders::check_folder(&shared, path)))
    }
    fn stop(&self) {
        self.close_all()
    }
}
