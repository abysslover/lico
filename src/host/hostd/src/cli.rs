//! The `lico` command line: a client of the host's local socket.
//!
//! Exit codes: 0 success, 1 usage error, 2 the host answered with an error,
//! 3 the host could not be reached, 4 timed out. Output is JSON, one object per
//! line, so scripts and people read the same thing. No secret is ever an argument.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

const USAGE: &str = "usage: lico [--socket PATH] <command>\n\
  status                         host and session status\n\
  resources                      host resource report\n\
  session list                   list sessions\n\
  session create [--kind synthetic|codex|claude] [--account ID] [--mode auto|default] [--rate N] [--history N]\n\
                 [--cwd DIR] [--name TEXT] [--model NAME] [--effort LEVEL]   (DIR: the folder it runs in; none = a scratch workspace)\n\
  session options                models and efforts the installed Claude Code offers, and the recent folders\n\
  session rename ID [NAME]       give a session a name (no name = take it away)\n\
  session close ID               end a session's process and keep it, disconnected (`session resume` brings it back)\n\
  session resume ID              bring a disconnected session back (a new generation)\n\
  session rebind ID ACCOUNT      move a session whose provider is gone to another account\n\
  account create --provider codex|claude --label TEXT\n\
  account list | default ID | affected ID | credential ID REF | remove ID detach|end\n\
  account login ID [--device]    sign in with the provider's own login (shows the address, asks for a code when one is needed)\n\
  account auth ID                whether the provider says the account is signed in (Codex only; Claude Code: null)\n\
  host stop [--force]            end the host of this socket (refused while sessions or a sign-in exist, unless forced)\n\
  history ID BEFORE [LIMIT]      older records of a session\n\
  session show ID                one session
  session delete ID              delete a session: its process, log and workspace (a folder you chose is never touched; `purge` is the same)\n\
  session send ID TEXT|- [--request-id RID]
                                 send input (- reads it from standard input; at most 64 KiB, newlines and tabs allowed);
                                 the request id is printed first, so after a timeout the
                                 same command with --request-id RID cannot send the text twice
  input status ID RID            state of an earlier request id\n\
  session watch ID [--after N] [--seconds S]\n\
  approval answer SESSION N allow|deny\n\
  view attach SESSION [--kind gui|cli]\n\
  view detach VIEW\n\
The socket defaults to $LICO_SOCKET, then to $XDG_RUNTIME_DIR/lico/host.sock.";

struct Conn {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}

fn die(code: i32, message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(code);
}

fn connect(path: &str) -> Conn {
    let stream = UnixStream::connect(path).unwrap_or_else(|e| die(3, &format!("cannot reach the host: {e}")));
    stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
    Conn { reader: BufReader::new(stream.try_clone().expect("clone")), writer: stream }
}

fn read_line(conn: &mut Conn) -> Option<Value> {
    let mut line = String::new();
    match conn.reader.read_line(&mut line) {
        Ok(0) => None,
        Ok(_) => serde_json::from_str(&line).ok(),
        Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => die(4, "timed out"),
        Err(_) => None,
    }
}

/// Send one request and return its reply, skipping events that arrive first.
fn call(conn: &mut Conn, request: Value) -> Value {
    let mut text = request.to_string();
    text.push('\n');
    conn.writer.write_all(text.as_bytes()).unwrap_or_else(|e| die(3, &format!("write failed: {e}")));
    loop {
        match read_line(conn) {
            Some(message) if message.get("ok").is_some() => return message,
            Some(_) => continue,
            None => die(3, "the host closed the connection"),
        }
    }
}

fn expect_ok(reply: Value) -> Value {
    if reply["ok"] == json!(1) {
        println!("{reply}");
        reply
    } else {
        println!("{reply}");
        std::process::exit(2);
    }
}

/// Like `expect_ok` but prints nothing: for the repeated polls of a login.
fn quiet_ok(reply: Value) -> Value {
    if reply["ok"] != json!(1) {
        die(2, &reply.to_string());
    }
    reply
}

/// Line ends as a paste in the window has them: CRLF and a lone CR become LF.
fn with_lf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// The text of an `input` read from standard input, with LF line ends. Up to twice the limit is read, because a file with Windows line ends is
/// longer as it comes than once they are LF; the limit applies to the LF text, as the host counts it.
fn text_from_stdin() -> String {
    let limit = crate::ipc::MAX_INPUT_TEXT;
    let mut bytes = Vec::new();
    std::io::stdin().take(2 * limit as u64 + 1).read_to_end(&mut bytes).unwrap_or_else(|e| die(1, &format!("cannot read standard input: {e}")));
    let text = with_lf(&String::from_utf8(bytes).unwrap_or_else(|_| die(1, "standard input is not UTF-8 text")));
    if text.len() > limit {
        die(1, "standard input is longer than 64 KiB; nothing was sent");
    }
    text
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == name).map(|w| w[1].clone())
}

fn random_token() -> String {
    let mut bytes = [0u8; 6];
    std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes)).unwrap_or_else(|_| die(3, "no randomness"));
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn session_generation(conn: &mut Conn, session: &str) -> i64 {
    let listing = call(conn, json!({"op": "session.list"}));
    listing["sessions"]
        .as_array()
        .and_then(|all| all.iter().find(|s| s["session"] == json!(session)))
        .and_then(|s| s["generation"].as_i64())
        .unwrap_or_else(|| die(2, "unknown session"))
}

pub fn run(args: Vec<String>) {
    let mut args: Vec<String> = args.into_iter().skip(1).collect();
    let socket = match args.iter().position(|a| a == "--socket") {
        Some(i) if i + 1 < args.len() => {
            let value = args[i + 1].clone();
            args.drain(i..=i + 1);
            value
        }
        _ => std::env::var("LICO_SOCKET").ok().filter(|s| !s.is_empty()).or_else(|| std::env::var("XDG_RUNTIME_DIR").ok().map(|d| format!("{d}/lico/host.sock"))).unwrap_or_default(),
    };
    if socket.is_empty() || args.is_empty() {
        die(1, USAGE);
    }
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["status"] => {
            expect_ok(call(&mut connect(&socket), json!({"op": "status"})));
        }
        ["resources"] => {
            expect_ok(call(&mut connect(&socket), json!({"op": "resources"})));
        }
        ["session", "list"] => {
            expect_ok(call(&mut connect(&socket), json!({"op": "session.list"})));
        }
        ["session", "create", ..] => {
            let mut request = json!({"op": "session.create", "kind": flag(&args, "--kind").unwrap_or_else(|| "synthetic".to_string())});
            if let Some(account) = flag(&args, "--account") {
                request["account"] = json!(account);
            }
            if let Some(mode) = flag(&args, "--mode") {
                request["mode"] = json!(mode);
            }
            for key in ["rate", "history"] {
                if let Some(v) = flag(&args, &format!("--{key}")) {
                    request[key] = json!(v.parse::<i64>().unwrap_or_else(|_| die(1, USAGE)));
                }
            }
            for key in ["name", "model", "effort"] {
                if let Some(v) = flag(&args, &format!("--{key}")) {
                    request[key] = json!(v);
                }
            }
            if let Some(folder) = flag(&args, "--cwd") {
                // the host takes absolute paths only: a relative one is relative to where this command runs
                let absolute = std::env::current_dir().map(|here| here.join(&folder)).unwrap_or_else(|_| folder.clone().into());
                request["cwd"] = json!(absolute.to_string_lossy());
            }
            expect_ok(call(&mut connect(&socket), request));
        }
        ["session", "options"] => {
            expect_ok(call(&mut connect(&socket), json!({"op": "session.options"})));
        }
        ["session", "rename", id, rest @ ..] if rest.len() <= 1 => {
            expect_ok(call(&mut connect(&socket), json!({"op": "session.rename", "session": id, "name": rest.first().copied().unwrap_or("")})));
        }
        ["session", "resume", id] => {
            let mut conn = connect(&socket);
            let generation = session_generation(&mut conn, id);
            expect_ok(call(&mut conn, json!({"op": "session.resume", "session": id, "generation": generation})));
        }
        ["session", "rebind", id, account] => {
            let mut conn = connect(&socket);
            let generation = session_generation(&mut conn, id);
            expect_ok(call(&mut conn, json!({"op": "session.rebind", "session": id, "generation": generation, "account": account})));
        }
        ["account", "create", ..] => {
            let (Some(provider), Some(label)) = (flag(&args, "--provider"), flag(&args, "--label")) else { die(1, USAGE) };
            expect_ok(call(&mut connect(&socket), json!({"op": "account.create", "provider": provider, "label": label})));
        }
        ["account", "login", id, ..] => {
            let mut conn = connect(&socket);
            let method = if args.iter().any(|a| a == "--device") { "device" } else { "browser" };
            quiet_ok(call(&mut conn, json!({"op": "account.login.start", "account": id, "method": method})));
            let mut shown = false;
            loop {
                let status = quiet_ok(call(&mut conn, json!({"op": "account.login.status", "account": id})));
                if let (false, Some(url)) = (shown, status["url"].as_str()) {
                    eprintln!("Open this address in your browser and sign in:\n{url}");
                    shown = true;
                }
                if status["needs_code"] == json!(true) {
                    eprint!("Paste the code the browser shows, then press Enter: ");
                    let mut code = String::new();
                    std::io::stdin().read_line(&mut code).ok();
                    quiet_ok(call(&mut conn, json!({"op": "account.login.input", "account": id, "code": code.trim()})));
                }
                match status["state"].as_str().unwrap_or("") {
                    "succeeded" => return eprintln!("signed in"),
                    state @ ("failed" | "cancelled" | "timed_out") => die(1, &format!("login {state}")),
                    _ => std::thread::sleep(Duration::from_millis(500)),
                }
            }
        }
        ["account", "auth", id] => {
            expect_ok(call(&mut connect(&socket), json!({"op": "account.login.status", "account": id, "check": true})));
        }
        ["host", "stop", ..] => {
            let force = args.iter().any(|a| a == "--force");
            expect_ok(call(&mut connect(&socket), json!({"op": "host.stop", "force": force})));
        }
        ["account", "list"] => {
            expect_ok(call(&mut connect(&socket), json!({"op": "account.list"})));
        }
        ["account", "default", id] => {
            expect_ok(call(&mut connect(&socket), json!({"op": "account.default", "account": id})));
        }
        ["account", "affected", id] => {
            expect_ok(call(&mut connect(&socket), json!({"op": "account.affected", "account": id})));
        }
        ["account", "credential", id, reference] => {
            expect_ok(call(&mut connect(&socket), json!({"op": "account.credential", "account": id, "credential_ref": reference})));
        }
        ["account", "remove", id, choice] => {
            expect_ok(call(&mut connect(&socket), json!({"op": "account.remove", "account": id, "sessions": choice})));
        }
        ["history", id, before, ..] => {
            let limit: i64 = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(100);
            let before: i64 = before.parse().unwrap_or_else(|_| die(1, USAGE));
            expect_ok(call(&mut connect(&socket), json!({"op": "history", "session": id, "before": before, "limit": limit})));
        }
        ["session", "purge" | "delete", id] => {
            let mut conn = connect(&socket);
            let generation = session_generation(&mut conn, id);
            expect_ok(call(&mut conn, json!({"op": "session.purge", "session": id, "generation": generation})));
        }
        ["session", "close", id] => {
            let mut conn = connect(&socket);
            let generation = session_generation(&mut conn, id);
            expect_ok(call(&mut conn, json!({"op": "session.close", "session": id, "generation": generation})));
        }
        ["session", "show", id] => {
            let mut conn = connect(&socket);
            let status = call(&mut conn, json!({"op": "status"}));
            match status["sessions"].as_array().and_then(|all| all.iter().find(|s| s["session"] == json!(id))) {
                Some(entry) => println!("{entry}"),
                None => die(2, "unknown session"),
            }
        }
        ["input", "status", id, rid] => {
            expect_ok(call(&mut connect(&socket), json!({"op": "input.status", "session": id, "request_id": rid})));
        }
        ["session", "send", id, text, ..] => {
            let mut conn = connect(&socket);
            let generation = session_generation(&mut conn, id);
            let request_id = flag(&args, "--request-id").unwrap_or_else(|| format!("cli-{}", random_token()));
            eprintln!("request id {request_id}");
            let text = if *text == "-" { text_from_stdin() } else { text.to_string() };
            expect_ok(call(&mut conn, json!({"op": "input", "session": id, "generation": generation, "request_id": request_id, "text": text})));
        }
        ["session", "watch", id, ..] => {
            let mut conn = connect(&socket);
            let after = flag(&args, "--after").and_then(|v| v.parse::<i64>().ok());
            let seconds: u64 = flag(&args, "--seconds").and_then(|v| v.parse().ok()).unwrap_or(5);
            let mut request = json!({"op": "subscribe", "session": id, "after": after.unwrap_or(-1)});
            if after.is_none() {
                request["snapshot"] = json!(true);
            }
            expect_ok(call(&mut conn, request));
            let until = Instant::now() + Duration::from_secs(seconds);
            conn.writer.set_read_timeout(Some(Duration::from_millis(250))).ok();
            while Instant::now() < until {
                let mut line = String::new();
                match conn.reader.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) => print!("{line}"),
                    Err(_) => continue,
                }
            }
        }
        ["approval", "answer", session, number, answer] => {
            let mut conn = connect(&socket);
            let generation = session_generation(&mut conn, session);
            let approval: i64 = number.parse().unwrap_or_else(|_| die(1, USAGE));
            expect_ok(call(&mut conn, json!({"op": "approval.answer", "session": session, "generation": generation, "approval_id": approval, "answer": answer})));
        }
        ["view", "attach", session, ..] => {
            let kind = flag(&args, "--kind").unwrap_or_else(|| "cli".to_string());
            expect_ok(call(&mut connect(&socket), json!({"op": "view.attach", "session": session, "kind": kind})));
        }
        ["view", "detach", view] => {
            expect_ok(call(&mut connect(&socket), json!({"op": "view.detach", "view": view})));
        }
        _ => die(1, USAGE),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn windows_and_old_mac_line_ends_become_lf() {
        assert_eq!(super::with_lf("a\r\nb\rc\n\r\n\r\rd"), "a\nb\nc\n\n\n\nd");
        assert_eq!(super::with_lf("plain\ttext 한글\n"), "plain\ttext 한글\n");
    }
}
