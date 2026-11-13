//! What an HTTP answer says about the server behind it: an OpenAI-compatible one (`GET /v1/models`, a JSON list under `data`), an Ollama one (`GET /api/tags`, a list under
//! `models`), a server that wants a key (a 401 or 403 with an OpenAI-style error body: it is reported, no key is ever tried), or some other HTTP server. Pure functions of the
//! answer; a list that was cut at the read bound is read by a tolerant scan for its ids.

use serde_json::Value;

/// At most this many model ids are given; `total` counts the ones seen.
pub const MAX_MODELS: usize = 50;
const MAX_ID_CHARS: usize = 120;

/// An answer as the probe read it: the status, and the body up to the read bound (`truncated` when the bound ended the read).
#[derive(Clone, Debug, Default)]
pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
    pub truncated: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Models {
    pub ids: Vec<String>,
    pub total: usize,
    pub truncated: bool,
}

fn clean_id(raw: &str) -> String {
    raw.chars().filter(|c| !c.is_control()).take(MAX_ID_CHARS).collect()
}

fn models_of(ids: Vec<String>, truncated: bool) -> Models {
    let total = ids.len();
    Models { ids: ids.into_iter().take(MAX_MODELS).map(|i| clean_id(&i)).collect(), total, truncated: truncated || total > MAX_MODELS }
}

/// The values of the string field `key` in `body`, found by scanning for `"key": "value"` (for a list that the read bound cut off, which no JSON parser takes).
pub fn scan_strings(body: &[u8], key: &str) -> Vec<String> {
    let text = String::from_utf8_lossy(body);
    let needle = format!("\"{key}\"");
    let mut found = Vec::new();
    let mut rest: &str = &text;
    while let Some(at) = rest.find(&needle) {
        rest = &rest[at + needle.len()..];
        let after = rest.trim_start();
        let Some(after) = after.strip_prefix(':') else { continue };
        let Some(value) = after.trim_start().strip_prefix('"') else { continue };
        let mut out = String::new();
        let mut chars = value.chars();
        let mut closed = false;
        while let Some(c) = chars.next() {
            match c {
                '\\' => {
                    if let Some(n) = chars.next() {
                        out.push(n);
                    }
                }
                '"' => {
                    closed = true;
                    break;
                }
                c => out.push(c),
            }
        }
        if closed {
            found.push(out);
        }
    }
    found
}

fn json(response: &Response) -> Option<Value> {
    if response.truncated {
        return None;
    }
    serde_json::from_slice(&response.body).ok()
}

/// `GET /v1/models` answered like OpenAI: a 200 with a `data` list (ids of its entries).
pub fn openai_models(response: &Response) -> Option<Models> {
    if response.status != 200 {
        return None;
    }
    match json(response) {
        Some(v) => {
            let list = v.get("data")?.as_array()?;
            let ids = list.iter().filter_map(|m| m.get("id").and_then(Value::as_str).map(str::to_string)).collect();
            Some(models_of(ids, false))
        }
        None if response.truncated => {
            let text = String::from_utf8_lossy(&response.body);
            let at = text.find("\"data\"")?;
            text[at + 6..].trim_start().strip_prefix(':')?.trim_start().starts_with('[').then(|| models_of(scan_strings(&response.body, "id"), true))
        }
        None => None,
    }
}

/// `GET /api/tags` answered like Ollama: a 200 with a `models` list (the `name` of its entries).
pub fn ollama_tags(response: &Response) -> Option<Models> {
    if response.status != 200 {
        return None;
    }
    match json(response) {
        Some(v) => {
            let list = v.get("models")?.as_array()?;
            let ids = list.iter().filter_map(|m| m.get("name").or_else(|| m.get("model")).and_then(Value::as_str).map(str::to_string)).collect();
            Some(models_of(ids, false))
        }
        None if response.truncated => {
            let text = String::from_utf8_lossy(&response.body);
            let at = text.find("\"models\"")?;
            text[at + 8..].trim_start().strip_prefix(':')?.trim_start().starts_with('[').then(|| models_of(scan_strings(&response.body, "name"), true))
        }
        None => None,
    }
}

/// A 401 or 403 with an error body in the OpenAI style (an `error` object or text): the server is OpenAI-style and wants a key.
pub fn key_needed(response: &Response) -> bool {
    matches!(response.status, 401 | 403) && json(response).is_some_and(|v| v.get("error").is_some_and(|e| e.is_object() || e.is_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(body: &str) -> Response {
        Response { status: 200, body: body.as_bytes().to_vec(), truncated: false }
    }

    #[test]
    fn an_openai_list_gives_its_ids_and_an_empty_list_is_still_openai() {
        let m = openai_models(&ok(r#"{"object":"list","data":[{"id":"a","object":"model"},{"id":"b"},{"object":"x"}]}"#)).unwrap();
        assert_eq!((m.ids.as_slice(), m.total, m.truncated), (&["a".to_string(), "b".to_string()][..], 2, false));
        assert_eq!(openai_models(&ok(r#"{"object":"list","data":[]}"#)).unwrap().total, 0, "a server with no model loaded is still a server");
        assert!(openai_models(&ok(r#"{"models":[]}"#)).is_none() && openai_models(&ok("<html>")).is_none());
        assert!(openai_models(&Response { status: 404, ..ok(r#"{"data":[]}"#) }).is_none(), "only a 200 counts");
    }

    #[test]
    fn an_ollama_list_gives_its_names() {
        let m = ollama_tags(&ok(r#"{"models":[{"name":"llama3:8b","size":1},{"model":"mistral:7b"}]}"#)).unwrap();
        assert_eq!(m.ids, ["llama3:8b", "mistral:7b"]);
        assert!(ollama_tags(&ok(r#"{"data":[]}"#)).is_none());
    }

    #[test]
    fn a_key_is_needed_only_for_a_401_or_403_with_an_openai_style_error() {
        let err = |status: u16, body: &str| Response { status, body: body.as_bytes().to_vec(), truncated: false };
        assert!(key_needed(&err(401, r#"{"error":{"message":"Incorrect API key","type":"invalid_request_error"}}"#)));
        assert!(key_needed(&err(403, r#"{"error":"forbidden"}"#)));
        assert!(!key_needed(&err(401, "Unauthorized")), "a 401 without an OpenAI-style body says nothing");
        assert!(!key_needed(&err(500, r#"{"error":{"message":"x"}}"#)));
    }

    #[test]
    fn the_ids_are_capped_at_fifty_cleaned_and_counted() {
        let list: Vec<String> = (0..80).map(|i| format!("{{\"id\":\"m{i}\"}}")).collect();
        let m = openai_models(&ok(&format!("{{\"data\":[{}]}}", list.join(",")))).unwrap();
        assert_eq!((m.ids.len(), m.total, m.truncated), (50, 80, true));
        let odd = openai_models(&ok(&format!("{{\"data\":[{{\"id\":\"a\\u0007b{}\"}}]}}", "z".repeat(300)))).unwrap();
        assert!(odd.ids[0].len() <= 120 && !odd.ids[0].contains('\u{7}'));
    }

    #[test]
    fn a_list_cut_at_the_read_bound_is_read_by_scanning_for_its_ids() {
        let cut = Response { status: 200, body: br#"{"object":"list","data":[{"id":"first","object":"model"},{"id":"secon"#.to_vec(), truncated: true };
        let m = openai_models(&cut).unwrap();
        assert_eq!((m.ids.as_slice(), m.truncated), (&["first".to_string()][..], true), "an id that was cut is not given");
        let tags = Response { status: 200, body: br#"{"models":[{"name":"a:1"},{"name":"b:2"},{"na"#.to_vec(), truncated: true };
        assert_eq!(ollama_tags(&tags).unwrap().ids, ["a:1", "b:2"]);
        assert!(openai_models(&Response { status: 200, body: b"<html>".to_vec(), truncated: true }).is_none());
    }

    #[test]
    fn the_string_scan_reads_escapes_and_ignores_keys_without_a_string() {
        assert_eq!(scan_strings(br#"{"id" : "a\\b", "id":3, "id":"c"}"#, "id"), ["a\\b", "c"]);
    }
}
