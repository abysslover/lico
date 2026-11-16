//! `lico browser ...` (M17-P06): the command line builds the request of the four browser operations from its arguments and sends it over the socket; the host answers it
//! exactly as it answers the same request over `POST /v1`. This module only builds the request (it names no other module of the program).

use serde_json::{json, Value};
use std::path::Path;

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
        _ => Err("usage"),
    }
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
        let cwd = Path::new("/home/u");
        for (args, want) in [
            (vec!["open", "https://a.test/"], json!({"op": "browser.open", "url": "https://a.test/"})),
            (vec!["open", "http://localhost:8000/", "--local"], json!({"op": "browser.open", "url": "http://localhost:8000/", "local": true})),
            (vec!["open", "--local", "a.test"], json!({"op": "browser.open", "url": "a.test", "local": true})),
            (vec!["list"], json!({"op": "browser.list"})),
            (vec!["close", "b3"], json!({"op": "browser.close", "tab": "b3"})),
            (vec!["shot", "b3", "/tmp/a.png"], json!({"op": "browser.shot", "tab": "b3", "path": "/tmp/a.png"})),
            (vec!["shot", "b3", "pics/a.png"], json!({"op": "browser.shot", "tab": "b3", "path": "/home/u/pics/a.png"})),
        ] {
            let got = request(&args, cwd).unwrap();
            assert_eq!(got, want, "{args:?}");
            assert!(crate::ipc::valid_request(got["op"].as_str().unwrap(), &map(&got)), "{args:?}: the host accepts what the command builds");
        }
    }

    #[test]
    fn anything_else_is_a_usage_error() {
        let cwd = Path::new("/home/u");
        for bad in [vec![], vec!["open"], vec!["open", "a", "b"], vec!["open", "a", "--local", "--local"], vec!["open", "a", "--other"], vec!["list", "x"], vec!["close"], vec!["close", "b1", "b2"], vec!["shot", "b1"], vec!["shot"], vec!["nothing"]] {
            assert_eq!(request(&bad, cwd), Err("usage"), "{bad:?}");
        }
    }
}
