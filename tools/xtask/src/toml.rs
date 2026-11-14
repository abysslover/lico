//! A reader for the small subset of TOML that manifests and the lock file use: `[table]` and `[[array of tables]]` headers with dotted names, `key = value` with strings,
//! integers, booleans, arrays (also over several lines) and inline tables, and `#` comments. Anything else is an error, not a guess.
use serde_json::{Map, Value};

pub fn parse(text: &str) -> Result<Value, String> {
    let mut root = Value::Object(Map::new());
    let mut path: Vec<String> = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(raw) = lines.next() {
        let line = strip_comment(raw).trim().to_string();
        if line.is_empty() {
            continue;
        }
        if let Some(inner) = line.strip_prefix("[[").and_then(|l| l.strip_suffix("]]")) {
            path = split_key(inner)?;
            let parent = descend(&mut root, &path[..path.len() - 1])?;
            let last = &path[path.len() - 1];
            let entry = parent.as_object_mut().ok_or("not a table")?.entry(last.clone()).or_insert_with(|| Value::Array(Vec::new()));
            entry.as_array_mut().ok_or("not an array of tables")?.push(Value::Object(Map::new()));
            continue;
        }
        if let Some(inner) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            path = split_key(inner)?;
            descend(&mut root, &path)?;
            continue;
        }
        let (key, rest) = line.split_once('=').ok_or_else(|| format!("not a key and a value: {line}"))?;
        let mut value_text = rest.trim().to_string();
        while !balanced(&value_text) {
            let more = lines.next().ok_or("a value is not closed")?;
            value_text.push(' ');
            value_text.push_str(strip_comment(more).trim());
        }
        let value = parse_value(&value_text)?;
        let keys = split_key(key.trim())?;
        let table = descend(&mut root, &path)?;
        let mut cur = table;
        for k in &keys[..keys.len() - 1] {
            cur = cur.as_object_mut().ok_or("not a table")?.entry(k.clone()).or_insert_with(|| Value::Object(Map::new()));
        }
        cur.as_object_mut().ok_or("not a table")?.insert(keys[keys.len() - 1].clone(), value);
    }
    Ok(root)
}

fn descend<'a>(root: &'a mut Value, path: &[String]) -> Result<&'a mut Value, String> {
    let mut cur = root;
    for key in path {
        let map = cur.as_object_mut().ok_or("not a table")?;
        let next = map.entry(key.clone()).or_insert_with(|| Value::Object(Map::new()));
        cur = if next.is_array() { next.as_array_mut().and_then(|a| a.last_mut()).ok_or("empty array of tables")? } else { next };
    }
    Ok(cur)
}

fn strip_comment(line: &str) -> &str {
    let mut in_str: Option<char> = None;
    for (i, c) in line.char_indices() {
        match (in_str, c) {
            (None, '"') | (None, '\'') => in_str = Some(c),
            (Some(q), c) if c == q => in_str = None,
            (None, '#') => return &line[..i],
            _ => {}
        }
    }
    line
}

fn balanced(text: &str) -> bool {
    let (mut depth, mut in_str) = (0i32, None::<char>);
    for c in text.chars() {
        match (in_str, c) {
            (None, '"') | (None, '\'') => in_str = Some(c),
            (Some(q), c) if c == q => in_str = None,
            (None, '[') | (None, '{') => depth += 1,
            (None, ']') | (None, '}') => depth -= 1,
            _ => {}
        }
    }
    depth <= 0 && in_str.is_none()
}

fn split_key(key: &str) -> Result<Vec<String>, String> {
    let mut parts = Vec::new();
    let (mut cur, mut in_str) = (String::new(), None::<char>);
    for c in key.chars() {
        match (in_str, c) {
            (None, '"') | (None, '\'') => in_str = Some(c),
            (Some(q), c) if c == q => in_str = None,
            (None, '.') => parts.push(std::mem::take(&mut cur).trim().to_string()),
            (_, c) => cur.push(c),
        }
    }
    parts.push(cur.trim().to_string());
    if parts.iter().any(|p| p.is_empty()) {
        return Err(format!("bad key: {key}"));
    }
    Ok(parts)
}

fn split_top(text: &str) -> Vec<String> {
    let (mut out, mut cur, mut depth, mut in_str) = (Vec::new(), String::new(), 0i32, None::<char>);
    for c in text.chars() {
        match (in_str, c) {
            (None, '"') | (None, '\'') => {
                in_str = Some(c);
                cur.push(c);
            }
            (Some(q), c) if c == q => {
                in_str = None;
                cur.push(c);
            }
            (None, '[') | (None, '{') => {
                depth += 1;
                cur.push(c);
            }
            (None, ']') | (None, '}') => {
                depth -= 1;
                cur.push(c);
            }
            (None, ',') if depth == 0 => out.push(std::mem::take(&mut cur)),
            (_, c) => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

fn parse_value(text: &str) -> Result<Value, String> {
    let t = text.trim();
    if let Some(s) = t.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        return Ok(Value::String(s.replace("\\\"", "\"").replace("\\\\", "\\")));
    }
    if let Some(s) = t.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
        return Ok(Value::String(s.to_string()));
    }
    if t == "true" || t == "false" {
        return Ok(Value::Bool(t == "true"));
    }
    if let Some(inner) = t.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        return Ok(Value::Array(split_top(inner).iter().map(|p| parse_value(p)).collect::<Result<_, _>>()?));
    }
    if let Some(inner) = t.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
        let mut map = Map::new();
        for part in split_top(inner) {
            let (k, v) = part.split_once('=').ok_or_else(|| format!("bad inline table entry: {part}"))?;
            map.insert(k.trim().trim_matches('"').to_string(), parse_value(v)?);
        }
        return Ok(Value::Object(map));
    }
    t.replace('_', "").parse::<i64>().map(|n| Value::Number(n.into())).map_err(|_| format!("not a value I read: {t}"))
}
