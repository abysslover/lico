//! `lico browser ...` (M17-P06, M24-P05): the command line builds the request of a browser operation from its arguments (`open`, `list`, `close`, `shot`, and since M24-P05 `go`, `nav`, `type` and `input`, the person's input: the tool `pane` does the same) and sends it over the socket; the host answers it
//! exactly as it answers the same request over `POST /v1`. This module only builds the request (it names no other module of the program).

use serde_json::{json, Value};
use std::path::Path;

/// The flags of `lico browser open` (the parser below accepts these and no other; the help names them).
pub const OPEN_FLAGS: [&str; 1] = ["--local"];

/// What `lico browser open` says when the host is installing the engine for the tab (before it waits).
pub fn installing_line(reply: &Value) -> Option<String> {
    (reply["phase"] == json!("installing")).then(|| format!("installing the browser engine: about {} MB from {}, SHA-256 starts {} (no question; this waits until it is done)", reply["size_mb"].as_u64().unwrap_or(0), reply["source"].as_str().unwrap_or("the release page"), reply["sha256_start"].as_str().unwrap_or("")))
}

/// What it says when the install ended: the tab's row from the watch.
pub fn finished_line(row: &Value) -> String {
    match row["phase"].as_str().unwrap_or("") {
        "absent" => format!("the engine could not be installed: {}", row["notice"].as_str().unwrap_or("no reason was given")),
        "installing" => "still installing".to_string(),
        other => format!("the engine is installed and the tab is {other}"),
    }
}

/// The request for `lico browser ARGS...`; `Err` is a usage error. A relative `FILE` is made absolute from `cwd` without touching the file system.
pub fn request(args: &[&str], cwd: &Path) -> Result<Value, &'static str> {
    match args {
        ["open", rest @ ..] => {
            let mut url = None;
            let mut local = false;
            for a in rest {
                match *a {
                    "--local" if !local => local = true,
                    other if !other.starts_with("--") && url.is_none() => url = Some(other),
                    _ => return Err("usage"),
                }
            }
            let mut r = json!({"op": "browser.open", "url": url.ok_or("usage")?});
            if local {
                r["local"] = json!(true);
            }
            Ok(r)
        }
        ["list"] => Ok(json!({"op": "browser.list"})),
        ["close", tab] => Ok(json!({"op": "browser.close", "tab": tab})),
        ["shot", tab, file] => {
            let path = if Path::new(file).is_absolute() { Path::new(file).to_path_buf() } else { cwd.join(file) };
            Ok(json!({"op": "browser.shot", "tab": tab, "path": path.to_string_lossy()}))
        }
        ["go", tab, address] => Ok(json!({"op": "browser.go", "tab": tab, "address": address})),
        ["nav", tab, action @ ("back" | "forward" | "reload")] => Ok(json!({"op": "browser.nav", "tab": tab, "action": action})),
        ["type", tab, text] => Ok(json!({"op": "browser.input", "tab": tab, "kind": "text", "text": text})),
        ["input", tab, "move", x, y] => Ok(json!({"op": "browser.input", "tab": tab, "kind": "move", "x": whole(x)?, "y": whole(y)?})),
        ["input", tab, kind @ ("down" | "up"), x, y, button @ ("left" | "middle" | "right")] => {
            Ok(json!({"op": "browser.input", "tab": tab, "kind": kind, "x": whole(x)?, "y": whole(y)?, "button": button, "clicks": 1}))
        }
        ["input", tab, "wheel", x, y, delta] => Ok(json!({"op": "browser.input", "tab": tab, "kind": "wheel", "x": whole(x)?, "y": whole(y)?, "delta": whole(delta)?})),
        ["input", tab, kind @ ("key_down" | "key_up"), key, code] => Ok(json!({"op": "browser.input", "tab": tab, "kind": kind, "key": key, "code": code})),
        _ => Err("usage"),
    }
}

/// A whole number of the command line (a pixel, a wheel step).
fn whole(text: &str) -> Result<i64, &'static str> {
    text.parse::<i64>().map_err(|_| "usage")
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
        crate::browser::register();
        let cwd = Path::new("/work/u");
        for (args, want) in [
            (vec!["open", "https://a.test/"], json!({"op": "browser.open", "url": "https://a.test/"})),
            (vec!["open", "http://localhost:8000/", "--local"], json!({"op": "browser.open", "url": "http://localhost:8000/", "local": true})),
            (vec!["open", "--local", "a.test"], json!({"op": "browser.open", "url": "a.test", "local": true})),
            (vec!["list"], json!({"op": "browser.list"})),
            (vec!["close", "b3"], json!({"op": "browser.close", "tab": "b3"})),
            (vec!["shot", "b3", "/tmp/a.png"], json!({"op": "browser.shot", "tab": "b3", "path": "/tmp/a.png"})),
            (vec!["shot", "b3", "pics/a.png"], json!({"op": "browser.shot", "tab": "b3", "path": "/work/u/pics/a.png"})),
            (vec!["go", "b1", "https://a.test/x"], json!({"op": "browser.go", "tab": "b1", "address": "https://a.test/x"})),
            (vec!["nav", "b1", "back"], json!({"op": "browser.nav", "tab": "b1", "action": "back"})),
            (vec!["nav", "b2", "reload"], json!({"op": "browser.nav", "tab": "b2", "action": "reload"})),
            (vec!["type", "b1", "hello"], json!({"op": "browser.input", "tab": "b1", "kind": "text", "text": "hello"})),
            (vec!["input", "b1", "move", "10", "20"], json!({"op": "browser.input", "tab": "b1", "kind": "move", "x": 10, "y": 20})),
            (vec!["input", "b1", "down", "10", "20", "left"], json!({"op": "browser.input", "tab": "b1", "kind": "down", "x": 10, "y": 20, "button": "left", "clicks": 1})),
            (vec!["input", "b1", "up", "10", "20", "right"], json!({"op": "browser.input", "tab": "b1", "kind": "up", "x": 10, "y": 20, "button": "right", "clicks": 1})),
            (vec!["input", "b1", "wheel", "10", "20", "-120"], json!({"op": "browser.input", "tab": "b1", "kind": "wheel", "x": 10, "y": 20, "delta": -120})),
            (vec!["input", "b1", "key_down", "Enter", "Enter"], json!({"op": "browser.input", "tab": "b1", "kind": "key_down", "key": "Enter", "code": "Enter"})),
            (vec!["input", "b1", "key_up", "Enter", "Enter"], json!({"op": "browser.input", "tab": "b1", "kind": "key_up", "key": "Enter", "code": "Enter"})),
        ] {
            let got = request(&args, cwd).unwrap();
            assert_eq!(got, want, "{args:?}");
            assert!(crate::ipc::valid_request(got["op"].as_str().unwrap(), &map(&got)), "{args:?}: the host accepts what the command builds");
        }
    }

    #[test]
    fn anything_else_is_a_usage_error() {
        let cwd = Path::new("/work/u");
        for bad in [vec![], vec!["open"], vec!["open", "a", "b"], vec!["open", "a", "--local", "--local"], vec!["open", "a", "--other"], vec!["list", "x"], vec!["close"], vec!["close", "b1", "b2"], vec!["go"], vec!["go", "b1"], vec!["go", "b1", "a", "b"], vec!["nav", "b1", "sideways"], vec!["nav", "b1"], vec!["type", "b1"], vec!["type", "b1", "a", "b"], vec!["input", "b1"], vec!["input", "b1", "press"], vec!["input", "b1", "move", "a", "1"], vec!["input", "b1", "move", "1"], vec!["input", "b1", "down", "1", "2"], vec!["input", "b1", "down", "1", "2", "left", "x"], vec!["input", "b1", "down", "1", "2", "thumb"], vec!["input", "b1", "wheel", "1", "2"], vec!["input", "b1", "key_down", "Enter"], vec!["shot", "b1"], vec!["shot"], vec!["nothing"]] {
            assert_eq!(request(&bad, cwd), Err("usage"), "{bad:?}");
        }
    }

    #[test]
    fn the_help_names_exactly_the_flags_the_parser_accepts() {
        let help = crate::cli::USAGE;
        let at = help.find("browser open URL").expect("the help has the browser line");
        let block: String = help[at..].lines().take(2).collect::<Vec<_>>().join("\n");
        let named: Vec<&str> = block.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-')).filter(|w| w.starts_with("--")).collect();
        for flag in &named {
            assert!(request(&["open", "a.test", flag], Path::new("/h")).is_ok(), "the help names {flag}: the parser must accept it");
        }
        for flag in OPEN_FLAGS {
            assert!(named.contains(&flag), "the parser accepts {flag}: the help must name it");
            assert!(request(&["open", "a.test", flag], Path::new("/h")).is_ok());
        }
        let mut names: Vec<&str> = named.clone();
        names.sort();
        names.dedup();
        assert_eq!(names, OPEN_FLAGS, "no flag in the help but the ones the parser accepts");
        assert!(request(&["open", "a.test", "--localhost"], Path::new("/h")).is_err(), "the old name in the help was never accepted");
    }

    #[test]
    fn the_command_says_when_the_engine_is_being_installed_and_how_it_ended() {
        let line = installing_line(&json!({"phase": "installing", "size_mb": 72, "source": "github.com", "sha256_start": "eed986e478c5"})).unwrap();
        assert!(line.contains("about 72 MB from github.com") && line.contains("eed986e478c5") && line.contains("waits"), "{line}");
        assert_eq!(installing_line(&json!({"phase": "starting"})), None);
        assert_eq!(finished_line(&json!({"phase": "starting"})), "the engine is installed and the tab is starting");
        assert_eq!(finished_line(&json!({"phase": "absent", "notice": "SECURITY: the engine could not be installed: x"})), "the engine could not be installed: SECURITY: the engine could not be installed: x");
    }
}
