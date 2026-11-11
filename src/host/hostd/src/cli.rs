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
  capabilities                   protocol, operations, HTTP routes, MCP tools and limits\n\
  status                         host and session status\n\
  resources                      host resource report\n\
  session list                   list sessions\n\
  session create [--kind synthetic|codex|claude] [--account ID] [--mode auto|default] [--rate N] [--history N]\n\
                 [--cwd DIR] [--worktree BRANCH] [--name TEXT] [--model NAME] [--effort LEVEL]\n\
                 (DIR: the folder it runs in; none = a scratch workspace; --worktree creates a new git worktree and branch)\n\
  session options                models and efforts the installed Claude Code offers, and the recent folders\n\
  session rename ID [NAME]       give a session a name (no name = take it away)\n\
  session close ID [--keep-worktree|--remove-worktree] [--force-dirty]\n\
                                 end a session's process and keep it, disconnected (`session resume` brings it back)\n\
  session resume ID              bring a disconnected session back (a new generation)\n\
  session rebind ID ACCOUNT      move a session whose provider is gone to another account\n\
  session mode ID auto|default   switch a session's permission mode (restarts an idle Claude session cleanly)\n\
  account create --provider codex|claude --label TEXT\n\
  account rename ID LABEL        rename an account's label\n\
  account list | default ID | affected ID | credential ID REF | remove ID detach|end\n\
  account login ID [--device]    sign in with the provider's own login (shows the address, asks for a code when one is needed)\n\
  account auth ID                whether the provider says the account is signed in (Codex only; Claude Code: null)\n\
  host stop [--force]            end the host of this socket (refused while sessions or a sign-in exist, unless forced)\n\
  history ID BEFORE [LIMIT]      older records of a session\n\
  search QUERY [--session ID] [--limit N]   search conversation text across one session or all sessions\n\
  files [DIR] | files --read FILE   list a working directory or read up to 64 KiB of a regular file (on demand)\n\
  settings [--clipboard-write on|off] [--notifications on|off] [--group-by account|workspace] [--font-size 12..20]\n\
           [--last-cwd DIR] [--last-view start|files|session] [--last-session ID]\n\
                                 show or update host and window settings\n\
  session show ID                one session
  session delete ID [--keep-worktree|--remove-worktree] [--force-dirty]\n\
                                 delete a session: its process, log and workspace (a folder you chose is never touched; `purge` is the same)\n\
  session send ID --conversation --from ID2   hand the whole conversation of Claude session ID2 to ID, as a file in its folder (the host reads the program's record)
  session send ID TEXT|- [--from ID2 | --request-id RID]
                                 send input (- reads it from standard input; at most 64 KiB, newlines and tabs allowed);
                                 the request id is printed first, so after a timeout the
                                 same command with --request-id RID cannot send the text twice;
                                 with --from ID2 it is that session's text to ID: the host adds the header
                                 [from session 'NAME' (ID7) | HH:MM | message | SIZE] and saves a long text as a file
  input status ID RID            state of an earlier request id\n\
  session watch ID [--after N] [--seconds S]\n\
  approval answer SESSION N allow|deny [--keep]   (--keep: with allow, a send of this agent to the same session needs no more questions while the session lives; a request to make a session is asked each time)\n\
  approval show SESSION N [OFFSET]   one page (16 KiB at most) of the whole text an agent's request waits with; `next` is the offset of the following page\n\
  capture [--screen | --window ID | --region x,y,w,h] --out FILE   save the screen, a window or a region as a PNG (and FILE.agent.png, at most 1568 px)\n\
          one JSON line: path, width, height, bytes, offset, server, agent {path, width, height, bytes, scale}; needs no host\n\
  clipboard read [--selection clipboard|primary] [--kind text|image|auto] [--out FILE] [--max-bytes N] [--timeout-ms N]\n\
          one JSON line: a text in the line, a PNG picture in FILE (required for image and auto); needs no host\n\
  clipboard write (--text STRING | --text-file FILE | --image FILE) [--timeout-ms N]\n\
          put a text (at most 64 KiB) or a PNG picture (at most 3 MiB, 8000 px a side) on the clipboard through the window; one JSON line;\n\
          the host and a window must run (the window owns the selection)\n\
  display [--session ID] [--json] | display status [--session ID] | display stop [--session ID] | display run [--session ID] [--] CMD [ARGS...]\n\
          start, inspect, stop, or run a program on a session's private virtual display\n\
  real-screen stop [--session ID]  immediately revoke time-bounded real-screen control\n\
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

pub(crate) fn call_once(socket: &str, request: Value) -> Value {
    call(&mut connect(socket), request)
}

pub(crate) fn expect_ok_pub(reply: Value) -> Value {
    expect_ok(reply)
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
    if args.first().map(String::as_str) == Some("capture") {
        // the screen is read by this process, not by the host: no socket is needed
        crate::capture::command(&args[1..]);
    }
    if args.first().map(String::as_str) == Some("clipboard") {
        // `read`: the selection is read by this process, not by the host, so no socket is needed. `write`: a client of the host (the window owns the selection)
        crate::clipboard::job::cli(&args[1..], &socket);
    }
    if args.first().map(String::as_str) == Some("display") {
        crate::vdisplay::cli(&args[1..], &socket);
    }
    if args.first().map(String::as_str) == Some("real-screen") {
        crate::computer::real::cli(&args[1..], &socket);
    }
    if socket.is_empty() || args.is_empty() {
        die(1, USAGE);
    }
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["capabilities"] => {
            expect_ok(call(&mut connect(&socket), json!({"op": "capabilities"})));
        }
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
            let mut request = json!({"op": "session.create", "kind": flag(&args, "--kind").or_else(|| flag(&args, "--provider")).unwrap_or_else(|| "synthetic".to_string())});
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
            if let Some(branch) = flag(&args, "--worktree") {
                request["worktree_branch"] = json!(branch);
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
        ["session", "mode", id, mode] => {
            let mut conn = connect(&socket);
            let generation = session_generation(&mut conn, id);
            expect_ok(call(&mut conn, json!({"op": "session.mode", "session": id, "generation": generation, "mode": mode})));
        }
        ["account", "create", ..] => {
            let (Some(provider), Some(label)) = (flag(&args, "--provider"), flag(&args, "--label")) else { die(1, USAGE) };
            expect_ok(call(&mut connect(&socket), json!({"op": "account.create", "provider": provider, "label": label})));
        }
        ["account", "rename", id, label] => {
            expect_ok(call(&mut connect(&socket), json!({"op": "account.rename", "account": id, "label": label})));
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
        ["search", query, ..] => {
            let mut request = json!({"op": "session.search", "query": query});
            if let Some(id) = flag(&args, "--session") {
                request["session"] = json!(id);
            }
            if let Some(v) = flag(&args, "--limit") {
                request["limit"] = json!(v.parse::<i64>().unwrap_or_else(|_| die(1, USAGE)));
            }
            expect_ok(call(&mut connect(&socket), request));
        }
        ["files", "--read", file] => {
            let p = std::path::Path::new(file);
            let abs = if p.is_absolute() { p.to_path_buf() } else { std::env::current_dir().map(|d| d.join(p)).unwrap_or_else(|_| p.to_path_buf()) };
            expect_ok(call(&mut connect(&socket), json!({"op": "folder.read", "path": abs.to_string_lossy()})));
        }
        ["files", rest @ ..] if rest.len() <= 1 && rest.first().is_none_or(|a| !a.starts_with('-')) => {
            let raw = rest.first().copied().unwrap_or(".");
            let p = std::path::Path::new(raw);
            let abs = if p.is_absolute() { p.to_path_buf() } else { std::env::current_dir().map(|d| d.join(p)).unwrap_or_else(|_| p.to_path_buf()) };
            expect_ok(call(&mut connect(&socket), json!({"op": "folder.list", "cwd": abs.to_string_lossy()})));
        }
        ["settings", rest @ ..] => {
            if rest.is_empty() {
                let status = quiet_ok(call(&mut connect(&socket), json!({"op": "status"})));
                println!("{}", json!({"ok": 1, "settings": status["settings"]}));
            } else {
                let parse_on_off = |v: String| match v.as_str() {
                    "on" | "true" | "1" => true,
                    "off" | "false" | "0" => false,
                    _ => die(1, USAGE),
                };
                let mut request = json!({"op": "settings.set"});
                let mut any = false;
                if let Some(v) = flag(&args, "--clipboard-write") {
                    request["clipboard_write"] = json!(parse_on_off(v));
                    any = true;
                }
                if let Some(v) = flag(&args, "--notifications") {
                    request["notifications"] = json!(parse_on_off(v));
                    any = true;
                }
                if let Some(v) = flag(&args, "--group-by") {
                    if !matches!(v.as_str(), "account" | "workspace") {
                        die(1, USAGE);
                    }
                    request["group_by"] = json!(v);
                    any = true;
                }
                if let Some(v) = flag(&args, "--font-size") {
                    let px: i64 = v.parse().unwrap_or_else(|_| die(1, USAGE));
                    request["font_px"] = json!(px);
                    any = true;
                }
                if let Some(v) = flag(&args, "--last-cwd") {
                    if v.is_empty() {
                        request["last_cwd"] = json!("");
                    } else {
                        let p = std::path::Path::new(&v);
                        let abs = if p.is_absolute() { p.to_path_buf() } else { std::env::current_dir().map(|d| d.join(p)).unwrap_or_else(|_| p.to_path_buf()) };
                        request["last_cwd"] = json!(abs.to_string_lossy());
                    }
                    any = true;
                }
                if let Some(v) = flag(&args, "--last-view") {
                    if !matches!(v.as_str(), "start" | "files" | "session") {
                        die(1, USAGE);
                    }
                    request["last_view"] = json!(v);
                    any = true;
                }
                if let Some(v) = flag(&args, "--last-session") {
                    request["last_session"] = json!(v);
                    any = true;
                }
                if !any {
                    die(1, USAGE);
                }
                expect_ok(call(&mut connect(&socket), request));
            }
        }
        ["session", op @ ("purge" | "delete" | "close"), id, rest @ ..] => {
            let host_op = if *op == "close" { "session.close" } else { "session.purge" };
            let mut wt_policy: Option<&str> = None;
            let mut force_dirty = false;
            for tok in rest {
                match *tok {
                    "--keep-worktree" if wt_policy.is_none() => wt_policy = Some("keep"),
                    "--remove-worktree" if wt_policy.is_none() => wt_policy = Some("remove"),
                    "--force-dirty" => force_dirty = true,
                    _ => die(1, USAGE),
                }
            }
            let mut conn = connect(&socket);
            let generation = session_generation(&mut conn, id);
            let mut request = json!({"op": host_op, "session": id, "generation": generation});
            if let Some(policy) = wt_policy {
                request["worktree"] = json!(policy);
            }
            if force_dirty {
                request["force_dirty"] = json!(true);
            }
            expect_ok(call(&mut conn, request));
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
        ["session", "send", id, ..] if args.iter().any(|a| a == "--conversation") => {
            // the whole conversation of the session named by --from, read by the host from the program's own record: no text, and a file in the receiver's folder
            let from = flag(&args, "--from").unwrap_or_else(|| die(1, "--conversation needs --from ID2, the session whose conversation it is"));
            let reply = call(&mut connect(&socket), json!({"op": "transfer.send", "from": from, "to": id, "kind": "conversation"}));
            if reply["ok"] != json!(1) {
                eprintln!("{}", reply["error"].as_str().unwrap_or("refused"));
            }
            expect_ok(reply);
        }
        ["session", "send", id, text, ..] if args.iter().any(|a| a == "--from") => {
            // one session's text to another: the host writes the header and decides inline or file; no generation, no request id to keep
            let from = flag(&args, "--from").unwrap_or_else(|| die(1, "--from needs the id of the sending session"));
            let text = if *text == "-" { text_from_stdin() } else { with_lf(text) };
            let reply = call(&mut connect(&socket), json!({"op": "transfer.send", "from": from, "to": id, "kind": "text", "text": text}));
            if reply["ok"] != json!(1) {
                eprintln!("{}", reply["error"].as_str().unwrap_or("refused"));
            }
            expect_ok(reply);
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
        ["approval", "answer", session, number, answer, rest @ ..] if matches!(rest, [] | ["--keep"]) => {
            let mut conn = connect(&socket);
            let generation = session_generation(&mut conn, session);
            let approval: i64 = number.parse().unwrap_or_else(|_| die(1, USAGE));
            let mut request = json!({"op": "approval.answer", "session": session, "generation": generation, "approval_id": approval, "answer": answer});
            if !rest.is_empty() {
                request["keep"] = json!(true);
            }
            expect_ok(call(&mut conn, request));
        }
        ["approval", "show", session, number, rest @ ..] if rest.len() <= 1 => {
            let mut conn = connect(&socket);
            let generation = session_generation(&mut conn, session);
            let approval: i64 = number.parse().unwrap_or_else(|_| die(1, USAGE));
            let offset: i64 = rest.first().map_or(0, |at| at.parse().unwrap_or_else(|_| die(1, USAGE)));
            expect_ok(call(&mut conn, json!({"op": "approval.text", "session": session, "generation": generation, "approval_id": approval, "offset": offset})));
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
