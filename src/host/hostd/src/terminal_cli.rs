//! `lico terminal ...` (M23-P02): the command line builds the request of a terminal operation from its arguments and sends it over the socket (the terminal's operations are
//! socket-only). This module only builds the request and, for `show --text`, reads a screen as plain text; it names no other module of the program.

use serde_json::{json, Value};
use std::path::Path;

/// `COLSxROWS` as two numbers; the host checks the bounds.
fn size(text: &str) -> Option<(u64, u64)> {
    let (c, r) = text.split_once('x')?;
    Some((c.parse().ok().filter(|n| *n > 0)?, r.parse().ok().filter(|n| *n > 0)?))
}

/// The request for `lico terminal ARGS...`; `Err` is a usage error. A relative `--cwd` is made absolute from `cwd` without touching the file system; no `--cwd` is the folder the
/// command runs in.
pub fn request(args: &[&str], cwd: &Path) -> Result<Value, &'static str> {
    match args {
        ["open", rest @ ..] => {
            let mut r = json!({"op": "terminal.open"});
            let (mut dir, mut shell, mut dims) = (None, None, None);
            let mut it = rest.iter();
            while let Some(flag) = it.next() {
                let slot = match *flag {
                    "--cwd" => &mut dir,
                    "--shell" => &mut shell,
                    "--size" => &mut dims,
                    _ => return Err("usage"),
                };
                if slot.is_some() {
                    return Err("usage");
                }
                *slot = Some(*it.next().ok_or("usage")?);
            }
            let dir = dir.map_or_else(|| cwd.to_path_buf(), |d| if Path::new(d).is_absolute() { Path::new(d).to_path_buf() } else { cwd.join(d) });
            r["cwd"] = json!(dir.to_string_lossy());
            if let Some(s) = shell {
                r["shell"] = json!(s);
            }
            if let Some(d) = dims {
                let (cols, rows) = size(d).ok_or("usage")?;
                r["cols"] = json!(cols);
                r["rows"] = json!(rows);
            }
            Ok(r)
        }
        ["list"] => Ok(json!({"op": "terminal.list"})),
        ["close", tab] => Ok(json!({"op": "terminal.close", "tab": tab})),
        ["send", tab, text, rest @ ..] if matches!(rest, [] | ["--enter"]) => {
            let text = if rest.is_empty() { text.to_string() } else { format!("{text}\r") }; // Enter is a carriage return to a terminal
            Ok(json!({"op": "terminal.input", "tab": tab, "text": text}))
        }
        ["show", tab, rest @ ..] => {
            let mut r = json!({"op": "terminal.watch", "tab": tab, "seq": 0});
            let mut it = rest.iter();
            while let Some(flag) = it.next() {
                match *flag {
                    "--text" => {}
                    "--scroll" => r["scroll"] = json!(it.next().ok_or("usage")?.parse::<u64>().map_err(|_| "usage")?),
                    _ => return Err("usage"),
                }
            }
            Ok(r)
        }
        ["resize", tab, dims] => {
            let (cols, rows) = size(dims).ok_or("usage")?;
            Ok(json!({"op": "terminal.resize", "tab": tab, "cols": cols, "rows": rows}))
        }
        _ => Err("usage"),
    }
}

/// The screen of a `show --text` as plain text: each line's runs joined, the blank lines at the bottom dropped. `None` when the command is not `show --text` or the reply is not a
/// screen.
pub fn plain_text(args: &[&str], reply: &Value) -> Option<String> {
    if !matches!(args, ["show", _, rest @ ..] if rest.contains(&"--text")) || reply["ok"] != json!(1) {
        return None;
    }
    let mut lines: Vec<String> = reply["lines"].as_array()?.iter().map(|l| l.as_array().map(|runs| runs.iter().filter_map(|r| r[0].as_str()).collect()).unwrap_or_default()).collect();
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    Some(lines.iter().map(|l| format!("{l}\n")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Map;

    fn map(v: &Value) -> Map<String, Value> {
        v.as_object().cloned().unwrap()
    }

    #[test]
    fn each_command_builds_the_request_the_operation_accepts() {
        let cwd = Path::new("/work/here");
        for (args, want) in [
            (vec!["open"], json!({"op": "terminal.open", "cwd": "/work/here"})),
            (vec!["open", "--cwd", "/srv/x"], json!({"op": "terminal.open", "cwd": "/srv/x"})),
            (vec!["open", "--cwd", "sub"], json!({"op": "terminal.open", "cwd": "/work/here/sub"})),
            (vec!["open", "--shell", "/bin/zsh", "--size", "100x30"], json!({"op": "terminal.open", "cwd": "/work/here", "shell": "/bin/zsh", "cols": 100, "rows": 30})),
            (vec!["list"], json!({"op": "terminal.list"})),
            (vec!["close", "t3"], json!({"op": "terminal.close", "tab": "t3"})),
            (vec!["send", "t1", "ls"], json!({"op": "terminal.input", "tab": "t1", "text": "ls"})),
            (vec!["send", "t1", "ls", "--enter"], json!({"op": "terminal.input", "tab": "t1", "text": "ls\r"})),
            (vec!["show", "t1"], json!({"op": "terminal.watch", "tab": "t1", "seq": 0})),
            (vec!["show", "t1", "--scroll", "40"], json!({"op": "terminal.watch", "tab": "t1", "seq": 0, "scroll": 40})),
            (vec!["show", "t1", "--text"], json!({"op": "terminal.watch", "tab": "t1", "seq": 0})),
            (vec!["resize", "t1", "120x40"], json!({"op": "terminal.resize", "tab": "t1", "cols": 120, "rows": 40})),
        ] {
            let got = request(&args, cwd).unwrap();
            assert_eq!(got, want, "{args:?}");
            assert!(crate::terminal::ops_accept(got["op"].as_str().unwrap(), &map(&got)), "{args:?}: the host accepts what the command builds");
        }
    }

    #[test]
    fn a_usage_error_is_refused_before_anything_is_sent() {
        let cwd = Path::new("/w");
        for args in [
            vec![],
            vec!["open", "--bogus"],
            vec!["open", "--cwd"],
            vec!["open", "--cwd", "/a", "--cwd", "/b"],
            vec!["open", "--size", "80"],
            vec!["open", "--size", "0x24"],
            vec!["open", "--size", "axb"],
            vec!["list", "extra"],
            vec!["close"],
            vec!["send", "t1"],
            vec!["send", "t1", "x", "--nope"],
            vec!["show"],
            vec!["show", "t1", "--scroll"],
            vec!["show", "t1", "--scroll", "-1"],
            vec!["show", "t1", "--other"],
            vec!["resize", "t1"],
            vec!["resize", "t1", "wide"],
            vec!["nothing"],
        ] {
            assert!(request(&args, cwd).is_err(), "{args:?}");
        }
    }

    #[test]
    fn show_text_prints_the_runs_joined_and_drops_the_blank_bottom() {
        let reply = json!({"ok": 1, "lines": [[["he", -1, -1, 0], ["llo", 1, -1, 1]], [], [["x", -1, -1, 0]], [], []]});
        assert_eq!(plain_text(&["show", "t1", "--text"], &reply).unwrap(), "hello\n\nx\n");
        assert_eq!(plain_text(&["show", "t1"], &reply), None, "without --text the reply is printed as it is");
        assert_eq!(plain_text(&["list"], &reply), None);
        assert_eq!(plain_text(&["show", "t1", "--text"], &json!({"ok": 0, "error": "unknown_tab"})), None);
    }
}
