//! The module graph of a crate read from its source text, no compiler: the module tree from the `mod x;` declarations of the root file (with `#[path]` and `#[cfg(test)]`),
//! comments and string literals blanked, `use` trees and `crate::`, `super::`, `self::` paths and `alias::item` names of modules imported by `use` resolved to the longest
//! known module prefix, and the edges between top-level modules counted apart for production and for test code (`#[cfg(test)]` modules and items, and files declared under
//! `#[cfg(test)]`). Then the strongly connected components (Tarjan), the layers and the number of two-way pairs. The method is the one of the Phase 0 report of M15.
//!
//! It reads text, so it can only miss edges (calls by method syntax or through trait objects leave no path), never invent them; glob imports other than `use super::*` and
//! `macro_rules!` are counted in `uncertain`.

use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const EXT: [&str; 6] = ["winit", "softbuffer", "cosmic_text", "unicode_linebreak", "x11rb", "serde_json"];

#[derive(Debug, Default)]
pub struct Graph {
    pub nodes: Vec<String>,
    pub prod: BTreeMap<(String, String), u32>,
    pub test: BTreeMap<(String, String), u32>,
    pub sccs: Vec<Vec<String>>,
    pub layers: BTreeMap<String, usize>,
    pub mutual_pairs: usize,
    pub modules: usize,
    /// top-level module -> lines of its files
    pub lines: BTreeMap<String, usize>,
    /// top-level module -> external crate -> uses
    pub ext: BTreeMap<String, BTreeMap<String, u32>>,
    /// (source, target) -> item names used -> count
    pub items: BTreeMap<(String, String), BTreeMap<String, u32>>,
    pub glob_uses: usize,
    pub macro_files: usize,
}

type Text = Vec<char>;

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn s(text: &[char]) -> String {
    text.iter().collect()
}

fn starts_with(text: &[char], at: usize, pat: &str) -> bool {
    let p: Vec<char> = pat.chars().collect();
    text.len() >= at + p.len() && text[at..at + p.len()] == p[..]
}

fn find_str(text: &[char], from: usize, pat: &str) -> Option<usize> {
    (from..text.len()).find(|&i| starts_with(text, i, pat))
}

/// The text with comments, string literals and char literals blanked; newlines and the length are kept.
fn strip(text: &[char]) -> Text {
    let n = text.len();
    let mut out = text.to_vec();
    let blank = |out: &mut Text, a: usize, b: usize| {
        for c in out.iter_mut().take(b.min(n)).skip(a) {
            if *c != '\n' {
                *c = ' ';
            }
        }
    };
    let mut i = 0;
    while i < n {
        let c = text[i];
        if starts_with(text, i, "//") {
            let j = (i..n).find(|&k| text[k] == '\n').unwrap_or(n);
            blank(&mut out, i, j);
            i = j;
        } else if starts_with(text, i, "/*") {
            let (mut depth, mut j) = (1, i + 2);
            while j < n && depth > 0 {
                if starts_with(text, j, "/*") {
                    depth += 1;
                    j += 2;
                } else if starts_with(text, j, "*/") {
                    depth -= 1;
                    j += 2;
                } else {
                    j += 1;
                }
            }
            blank(&mut out, i, j);
            i = j;
        } else if c == 'r' && (i == 0 || !is_ident(text[i - 1])) && raw_string_start(text, i).is_some() {
            let hashes = raw_string_start(text, i).unwrap();
            let open = i + 1 + hashes + 1;
            let close: Vec<char> = std::iter::once('"').chain(std::iter::repeat('#').take(hashes)).collect();
            let j = (open..n).find(|&k| text[k..].starts_with(&close[..])).map_or(n, |k| k + close.len());
            blank(&mut out, i, j);
            i = j;
        } else if c == '"' {
            let mut j = i + 1;
            while j < n && text[j] != '"' {
                j += if text[j] == '\\' { 2 } else { 1 };
            }
            let j = (j + 1).min(n);
            blank(&mut out, i, j);
            i = j;
        } else if c == '\'' {
            // a char literal ('x', '\n', '\u{1F600}') or a lifetime ('a)
            let end = if i + 1 < n && text[i + 1] == '\\' {
                (i + 2..(i + 12).min(n)).find(|&k| text[k] == '\'')
            } else if i + 2 < n && text[i + 1] != '\'' && text[i + 2] == '\'' {
                Some(i + 2)
            } else {
                None
            };
            match end {
                Some(e) => {
                    blank(&mut out, i, e + 1);
                    i = e + 1;
                }
                None => i += 1,
            }
        } else {
            i += 1;
        }
    }
    out
}

/// `r"`, `r#"` ...: the number of hashes if a raw string starts at `i`.
fn raw_string_start(text: &[char], i: usize) -> Option<usize> {
    let mut j = i + 1;
    while j < text.len() && text[j] == '#' {
        j += 1;
    }
    (j < text.len() && text[j] == '"').then_some(j - i - 1)
}

fn match_brace(text: &[char], open: usize) -> usize {
    let mut depth = 0i64;
    for (k, &c) in text.iter().enumerate().skip(open) {
        if c == '{' {
            depth += 1;
        } else if c == '}' {
            depth -= 1;
            if depth == 0 {
                return k;
            }
        }
    }
    text.len().saturating_sub(1)
}

/// The end of the item that starts at `at`: its `;` or its closing brace.
fn item_end(text: &[char], at: usize) -> usize {
    for k in at..text.len() {
        if text[k] == ';' {
            return k;
        }
        if text[k] == '{' {
            return match_brace(text, k);
        }
    }
    text.len()
}

struct Module {
    file: PathBuf,
    test: bool,
}

/// `mod name;` / `mod name {` declarations: (name, `#[path]`, under `#[cfg(test)]`, inline, position).
fn declared_mods(raw: &[char], text: &[char]) -> Vec<(String, Option<String>, bool, bool, usize)> {
    let mut out = Vec::new();
    let n = text.len();
    let mut i = 0;
    while i + 3 < n {
        if starts_with(text, i, "mod") && (i == 0 || !is_ident(text[i - 1])) && text[i + 3].is_whitespace() {
            let mut j = i + 3;
            while j < n && text[j].is_whitespace() {
                j += 1;
            }
            let name_start = j;
            while j < n && is_ident(text[j]) {
                j += 1;
            }
            let name = s(&text[name_start..j]);
            while j < n && text[j].is_whitespace() {
                j += 1;
            }
            if !name.is_empty() && j < n && (text[j] == ';' || text[j] == '{') {
                // back over `pub` / `pub(...)` and the attributes
                let mut start = i;
                let back = |mut p: usize| {
                    while p > 0 && text[p - 1].is_whitespace() {
                        p -= 1;
                    }
                    p
                };
                let mut p = back(start);
                if p > 0 && text[p - 1] == ')' {
                    if let Some(open) = (0..p).rev().find(|&k| text[k] == '(') {
                        let q = back(open);
                        if q >= 3 && s(&text[q - 3..q]) == "pub" {
                            start = q - 3;
                        }
                    }
                } else if p >= 3 && s(&text[p - 3..p]) == "pub" && (p == 3 || !is_ident(text[p - 4])) {
                    start = p - 3;
                }
                let mut attrs = String::new();
                p = back(start);
                while p > 0 && text[p - 1] == ']' {
                    let mut depth = 0i32;
                    let mut k = p - 1;
                    loop {
                        if text[k] == ']' {
                            depth += 1;
                        } else if text[k] == '[' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        if k == 0 {
                            break;
                        }
                        k -= 1;
                    }
                    if k == 0 || text[k - 1] != '#' {
                        break;
                    }
                    attrs.push_str(&s(&raw[k - 1..p]));
                    p = back(k - 1);
                }
                let path_attr = attrs.find("path").and_then(|at| {
                    let rest = &attrs[at..];
                    let q1 = rest.find('"')?;
                    let q2 = rest[q1 + 1..].find('"')?;
                    Some(rest[q1 + 1..q1 + 1 + q2].to_string())
                });
                out.push((name, path_attr, attrs.contains("cfg(test)"), text[j] == '{', i));
            }
            i = j.max(i + 3);
        } else {
            i += 1;
        }
    }
    out
}

fn build_tree(root_file: &Path) -> Result<(BTreeMap<String, Module>, BTreeMap<PathBuf, (Text, Text)>), String> {
    let mut modules = BTreeMap::new();
    let mut texts = BTreeMap::new();
    modules.insert(String::new(), Module { file: root_file.to_path_buf(), test: false });
    let mut todo = vec![String::new()];
    while let Some(path) = todo.pop() {
        let (file, test) = {
            let m = &modules[&path];
            (m.file.clone(), m.test)
        };
        if !texts.contains_key(&file) {
            let raw: Text = std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?.chars().collect();
            let stripped = strip(&raw);
            texts.insert(file.clone(), (raw, stripped));
        }
        let (raw, text) = &texts[&file];
        let dir_file = path.is_empty() || file.file_name().is_some_and(|n| n == "mod.rs");
        let parent = file.parent().unwrap_or(Path::new("."));
        let base = if dir_file { parent.to_path_buf() } else { parent.join(file.file_stem().unwrap_or_default()) };
        for (name, path_attr, cfg_test, inline, _) in declared_mods(raw, text) {
            if inline {
                continue;
            }
            let child_file = match path_attr {
                Some(p) => parent.join(p),
                None => {
                    let a = base.join(format!("{name}.rs"));
                    let b = base.join(&name).join("mod.rs");
                    if a.exists() {
                        a
                    } else if b.exists() {
                        b
                    } else {
                        continue;
                    }
                }
            };
            if !child_file.exists() {
                continue;
            }
            let cp = if path.is_empty() { name.clone() } else { format!("{path}::{name}") };
            modules.insert(cp.clone(), Module { file: child_file, test: test || cfg_test });
            todo.push(cp);
        }
    }
    Ok((modules, texts))
}

/// Inline `mod name { .. }` blocks: (start, end, name).
fn inline_blocks(text: &[char]) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    for (name, _, _, inline, pos) in declared_mods(text, text) {
        if inline {
            if let Some(open) = (pos..text.len()).find(|&k| text[k] == '{') {
                out.push((pos, match_brace(text, open), name));
            }
        }
    }
    out
}

fn test_regions(text: &[char]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(at) = find_str(text, i, "#[cfg(test)]") {
        let mut k = at + "#[cfg(test)]".len();
        loop {
            let mut p = k;
            while p < text.len() && text[p].is_whitespace() {
                p += 1;
            }
            if starts_with(text, p, "#[") {
                match (p..text.len()).find(|&q| text[q] == ']') {
                    Some(q) => k = q + 1,
                    None => break,
                }
            } else {
                break;
            }
        }
        out.push((at, item_end(text, k)));
        i = at + 1;
    }
    out
}

/// A `use` tree as leaves: (segments, alias, glob).
fn parse_use(body: &str) -> Vec<(Vec<String>, Option<String>, bool)> {
    fn split_top(s: &str) -> Vec<String> {
        let (mut parts, mut depth, mut cur) = (Vec::new(), 0i32, String::new());
        for ch in s.chars() {
            if ch == '{' {
                depth += 1;
            } else if ch == '}' {
                depth -= 1;
            }
            if ch == ',' && depth == 0 {
                parts.push(std::mem::take(&mut cur));
            } else {
                cur.push(ch);
            }
        }
        parts.push(cur);
        parts
    }
    fn rec(prefix: &[String], body: &str, out: &mut Vec<(Vec<String>, Option<String>, bool)>) {
        for part in split_top(body.trim()) {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            if let Some(brace) = part.find('{') {
                if part.ends_with('}') {
                    let head: Vec<String> = part[..brace].split("::").map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect();
                    let mut pre = prefix.to_vec();
                    pre.extend(head);
                    rec(&pre, &part[brace + 1..part.len() - 1], out);
                    continue;
                }
            }
            let (path_text, alias) = match part.rsplit_once(" as ") {
                Some((p, a)) => (p.trim(), Some(a.trim().to_string())),
                None => (part, None),
            };
            let mut segs: Vec<String> = path_text.split("::").map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect();
            let glob = segs.last().is_some_and(|l| l == "*");
            if glob {
                segs.pop();
            }
            if segs.last().is_some_and(|l| l == "self") {
                segs.pop();
            }
            let mut full = prefix.to_vec();
            full.extend(segs);
            out.push((full, alias, glob));
        }
    }
    let mut out = Vec::new();
    rec(&[], body, &mut out);
    out
}

#[derive(Default)]
struct Edge {
    prod: u32,
    test: u32,
    items: BTreeMap<String, u32>,
}

pub fn analyse(root_file: &Path) -> Result<Graph, String> {
    let (modules, texts) = build_tree(root_file)?;
    let root_name = root_file.file_stem().and_then(|n| n.to_str()).unwrap_or("root").to_string();
    let top_names: BTreeSet<String> = modules.keys().filter(|k| !k.is_empty() && !k.contains("::")).cloned().collect();
    let top = |m: &str| -> String { if m.is_empty() { root_name.clone() } else { m.split("::").next().unwrap().to_string() } };
    let mut edges: BTreeMap<(String, String), Edge> = BTreeMap::new();
    let mut g = Graph { modules: modules.len(), ..Graph::default() };
    for (path, module) in &modules {
        let (raw, text) = &texts[&module.file];
        g.lines.entry(top(path)).and_modify(|n| *n += raw.iter().filter(|&&c| c == '\n').count() + 1).or_insert(raw.iter().filter(|&&c| c == '\n').count() + 1);
        if s(text).contains("macro_rules!") {
            g.macro_files += 1;
        }
        let blocks = inline_blocks(text);
        let spans = test_regions(text);
        let file_base: Vec<String> = path.split("::").filter(|x| !x.is_empty()).map(str::to_string).collect();
        let base_of = |pos: usize| -> (Vec<String>, bool) {
            let mut stack: Vec<&(usize, usize, String)> = blocks.iter().filter(|b| b.0 <= pos && pos <= b.1).collect();
            stack.sort();
            let mut base = file_base.clone();
            base.extend(stack.iter().map(|b| b.2.clone()));
            (base, module.test || spans.iter().any(|&(a, b)| a <= pos && pos <= b))
        };
        // use statements, then the code with them blanked
        let mut code = text.clone();
        let mut uses: Vec<(usize, String)> = Vec::new();
        let n = text.len();
        let mut i = 0;
        while i + 3 < n {
            if starts_with(text, i, "use") && (i == 0 || !(is_ident(text[i - 1]) || text[i - 1] == ':')) && text[i + 3].is_whitespace() {
                if let Some(end) = (i..n).find(|&k| text[k] == ';') {
                    uses.push((i, s(&text[i + 3..end])));
                    for c in code.iter_mut().take(end + 1).skip(i) {
                        if *c != '\n' {
                            *c = ' ';
                        }
                    }
                    i = end + 1;
                    continue;
                }
            }
            i += 1;
        }
        enum Hit {
            Ext(String),
            Module { target: String, item: Option<String>, is_test: bool },
        }
        let resolve = |segs: &[String], pos: usize| -> Option<Hit> {
            let (base, is_test) = base_of(pos);
            let first = segs.first()?.as_str();
            let absolute: Vec<String> = match first {
                "crate" => segs[1..].to_vec(),
                "self" => base.iter().cloned().chain(segs[1..].iter().cloned()).collect(),
                "super" => {
                    let mut cur = base.clone();
                    let mut k = 0;
                    while k < segs.len() && segs[k] == "super" {
                        cur.pop();
                        k += 1;
                    }
                    cur.into_iter().chain(segs[k..].iter().cloned()).collect()
                }
                f if EXT.contains(&f) || ["std", "core", "alloc"].contains(&f) => return Some(Hit::Ext(f.to_string())),
                f => {
                    let child = file_base.iter().cloned().chain(std::iter::once(f.to_string())).collect::<Vec<_>>().join("::");
                    if modules.contains_key(&child) {
                        file_base.iter().cloned().chain(segs.iter().cloned()).collect()
                    } else if modules.contains_key(f) {
                        segs.to_vec()
                    } else {
                        return None;
                    }
                }
            };
            for k in (0..=absolute.len()).rev() {
                let cand = absolute[..k].join("::");
                if modules.contains_key(&cand) {
                    return Some(Hit::Module { target: cand, item: absolute.get(k).cloned(), is_test });
                }
            }
            None
        };
        let add = |edges: &mut BTreeMap<(String, String), Edge>, target: &str, item: Option<String>, is_test: bool| {
            if target == path {
                return;
            }
            let e = edges.entry((path.clone(), target.to_string())).or_default();
            if is_test {
                e.test += 1;
            } else {
                e.prod += 1;
            }
            if let Some(it) = item {
                *e.items.entry(it).or_default() += 1;
            }
        };
        let mut aliases: BTreeMap<String, String> = BTreeMap::new();
        for (pos, body) in &uses {
            for (segs, alias, glob) in parse_use(body) {
                if glob && !(segs.len() == 1 && segs[0] == "super") {
                    g.glob_uses += 1;
                }
                match resolve(&segs, *pos) {
                    Some(Hit::Ext(name)) => *g.ext.entry(top(path)).or_default().entry(name).or_default() += 1,
                    Some(Hit::Module { target, item, is_test }) => {
                        let exact = item.is_none();
                        add(&mut edges, &target, item, is_test);
                        if !glob && exact && !segs.is_empty() {
                            aliases.insert(alias.unwrap_or_else(|| segs[segs.len() - 1].clone()), target);
                        }
                    }
                    None => {}
                }
            }
        }
        // `crate::a::b`, `super::x`, `self::y` in the code
        let mut i = 0;
        while i < n {
            let word_start = is_ident(code[i]) && (i == 0 || !(is_ident(code[i - 1]) || code[i - 1] == ':'));
            if !word_start {
                i += 1;
                continue;
            }
            let mut j = i;
            while j < n && is_ident(code[j]) {
                j += 1;
            }
            let word = s(&code[i..j]);
            if ["crate", "super", "self"].contains(&word.as_str()) && starts_with(&code, j, "::") {
                let mut segs = vec![word];
                let mut k = j;
                while starts_with(&code, k, "::") {
                    let mut e = k + 2;
                    if e < n && (code[e].is_alphabetic() || code[e] == '_') {
                        while e < n && is_ident(code[e]) {
                            e += 1;
                        }
                        segs.push(s(&code[k + 2..e]));
                        k = e;
                    } else {
                        break;
                    }
                }
                if segs.len() > 1 {
                    if let Some(Hit::Module { target, item, is_test }) = resolve(&segs, i) {
                        add(&mut edges, &target, item, is_test);
                    }
                }
                i = k.max(j);
            } else if starts_with(&code, j, "::") {
                // `alias::item` of a module imported by `use`; the root's bare child names; an external crate
                let mut e = j + 2;
                while e < n && is_ident(code[e]) {
                    e += 1;
                }
                let item = s(&code[j + 2..e]);
                let prev_dot = i > 0 && code[i - 1] == '.';
                if let Some(target) = aliases.get(&word) {
                    if !item.is_empty() {
                        let (_, is_test) = base_of(i);
                        add(&mut edges, &target.clone(), Some(item), is_test);
                    }
                } else if path.is_empty() && !prev_dot && top_names.contains(&word) && word != "core" && !item.is_empty() {
                    let e = edges.entry((path.clone(), word.clone())).or_default();
                    e.prod += 1;
                    *e.items.entry(item).or_default() += 1;
                }
                if EXT.contains(&word.as_str()) {
                    *g.ext.entry(top(path)).or_default().entry(word).or_default() += 1;
                }
                i = j;
            } else {
                i = j;
            }
        }
    }
    // collapse to top-level modules
    for ((a, b), e) in &edges {
        let (ta, tb) = (top(a), top(b));
        if ta == tb {
            continue;
        }
        if e.prod > 0 {
            *g.prod.entry((ta.clone(), tb.clone())).or_default() += e.prod;
        }
        if e.test > 0 {
            *g.test.entry((ta.clone(), tb.clone())).or_default() += e.test;
        }
        for (it, n) in &e.items {
            *g.items.entry((ta.clone(), tb.clone())).or_default().entry(it.clone()).or_default() += n;
        }
    }
    let mut nodes: BTreeSet<String> = modules.keys().map(|m| top(m)).collect();
    nodes.insert(root_name);
    g.nodes = nodes.into_iter().collect();
    analyse_shape(&mut g);
    Ok(g)
}

/// Strongly connected components (of production edges), layers, two-way pairs.
fn analyse_shape(g: &mut Graph) {
    let idx: BTreeMap<&String, usize> = g.nodes.iter().enumerate().map(|(i, n)| (n, i)).collect();
    let n = g.nodes.len();
    let mut adj = vec![Vec::new(); n];
    for (a, b) in g.prod.keys() {
        adj[idx[a]].push(idx[b]);
    }
    // Tarjan
    struct T<'a> {
        adj: &'a [Vec<usize>],
        index: Vec<Option<usize>>,
        low: Vec<usize>,
        on: Vec<bool>,
        stack: Vec<usize>,
        counter: usize,
        comps: Vec<Vec<usize>>,
    }
    fn visit(t: &mut T, v: usize) {
        t.index[v] = Some(t.counter);
        t.low[v] = t.counter;
        t.counter += 1;
        t.stack.push(v);
        t.on[v] = true;
        for &w in &t.adj[v].clone() {
            match t.index[w] {
                None => {
                    visit(t, w);
                    t.low[v] = t.low[v].min(t.low[w]);
                }
                Some(iw) if t.on[w] => t.low[v] = t.low[v].min(iw),
                _ => {}
            }
        }
        if Some(t.low[v]) == t.index[v] {
            let mut comp = Vec::new();
            loop {
                let w = t.stack.pop().unwrap();
                t.on[w] = false;
                comp.push(w);
                if w == v {
                    break;
                }
            }
            t.comps.push(comp);
        }
    }
    let mut t = T { adj: &adj, index: vec![None; n], low: vec![0; n], on: vec![false; n], stack: Vec::new(), counter: 0, comps: Vec::new() };
    for v in 0..n {
        if t.index[v].is_none() {
            visit(&mut t, v);
        }
    }
    let comps = t.comps;
    let mut comp_of = vec![0; n];
    for (c, members) in comps.iter().enumerate() {
        for &m in members {
            comp_of[m] = c;
        }
    }
    let mut cdeps = vec![BTreeSet::new(); comps.len()];
    for v in 0..n {
        for &w in &adj[v] {
            if comp_of[v] != comp_of[w] {
                cdeps[comp_of[v]].insert(comp_of[w]);
            }
        }
    }
    let mut layer: Vec<Option<usize>> = vec![None; comps.len()];
    fn layer_of(c: usize, cdeps: &[BTreeSet<usize>], memo: &mut Vec<Option<usize>>) -> usize {
        if let Some(l) = memo[c] {
            return l;
        }
        let l = cdeps[c].iter().map(|&d| 1 + layer_of(d, cdeps, memo)).max().unwrap_or(0);
        memo[c] = Some(l);
        l
    }
    for v in 0..n {
        let l = layer_of(comp_of[v], &cdeps, &mut layer);
        g.layers.insert(g.nodes[v].clone(), l);
    }
    let mut sccs: Vec<Vec<String>> = comps.iter().filter(|c| c.len() > 1).map(|c| { let mut v: Vec<String> = c.iter().map(|&i| g.nodes[i].clone()).collect(); v.sort(); v }).collect();
    sccs.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
    g.mutual_pairs = g.prod.keys().filter(|(a, b)| a < b && g.prod.contains_key(&(b.clone(), a.clone())) && sccs.iter().any(|c| c.contains(a))).count();
    g.sccs = sccs;
}

impl Graph {
    /// An upper bound of the smallest set of edges whose removal makes the biggest cycle acyclic (lowest first order, each module moved to its best place, 300 deterministic
    /// restarts); the lower bound is `mutual_pairs`. Returns the edges cut.
    pub fn best_cut(&self) -> Vec<(String, String)> {
        let Some(scc) = self.sccs.first() else { return Vec::new() };
        let ix: BTreeMap<&String, usize> = scc.iter().enumerate().map(|(i, n)| (n, i)).collect();
        let m = scc.len();
        let mut outs = vec![Vec::new(); m];
        let mut ins = vec![Vec::new(); m];
        let mut edges = Vec::new();
        for (a, b) in self.prod.keys() {
            if let (Some(&x), Some(&y)) = (ix.get(a), ix.get(b)) {
                outs[x].push(y);
                ins[y].push(x);
                edges.push((x, y));
            }
        }
        let cost = |order: &[usize]| -> usize {
            let mut pos = vec![0; m];
            for (i, &v) in order.iter().enumerate() {
                pos[v] = i;
            }
            edges.iter().filter(|&&(a, b)| pos[b] > pos[a]).count()
        };
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let (mut best_cost, mut best_order) = (usize::MAX, Vec::new());
        for restart in 0..300 {
            let mut order: Vec<usize> = (0..m).collect();
            if restart == 0 {
                order.sort_by_key(|&v| outs[v].len());
            } else {
                for i in (1..m).rev() {
                    order.swap(i, (next() % (i as u64 + 1)) as usize);
                }
            }
            let mut cur = cost(&order);
            let mut improved = true;
            while improved {
                improved = false;
                for v in 0..m {
                    let i = order.iter().position(|&x| x == v).unwrap();
                    let rest: Vec<usize> = order.iter().copied().filter(|&x| x != v).collect();
                    // cost of v's edges for each insertion place j: out-edges to modules placed at or after j, in-edges from modules placed before j
                    let out_set: BTreeSet<usize> = outs[v].iter().copied().filter(|&x| x != v).collect();
                    let in_set: BTreeSet<usize> = ins[v].iter().copied().filter(|&x| x != v).collect();
                    let mut cost_at = vec![0usize; rest.len() + 1];
                    let mut after_out = out_set.iter().filter(|x| rest.contains(x)).count();
                    let mut before_in = 0;
                    for j in 0..=rest.len() {
                        cost_at[j] = after_out + before_in;
                        if j < rest.len() {
                            if out_set.contains(&rest[j]) {
                                after_out -= 1;
                            }
                            if in_set.contains(&rest[j]) {
                                before_in += 1;
                            }
                        }
                    }
                    let current_place = i;
                    let (best_j, &best_c) = cost_at.iter().enumerate().min_by_key(|&(_, c)| *c).unwrap();
                    if best_c < cost_at[current_place] {
                        let mut o = rest.clone();
                        o.insert(best_j, v);
                        order = o;
                        cur = cost(&order);
                        improved = true;
                    }
                }
            }
            if cur < best_cost {
                best_cost = cur;
                best_order = order;
            }
        }
        let mut pos = vec![0; m];
        for (i, &v) in best_order.iter().enumerate() {
            pos[v] = i;
        }
        edges.iter().filter(|&&(a, b)| pos[b] > pos[a]).map(|&(a, b)| (scc[a].clone(), scc[b].clone())).collect()
    }

    pub fn to_json(&self) -> Value {
        let edge_map = |m: &BTreeMap<(String, String), u32>| m.iter().map(|((a, b), n)| (format!("{a}->{b}"), json!(n))).collect::<serde_json::Map<_, _>>();
        let mut layers: BTreeMap<usize, Vec<&String>> = BTreeMap::new();
        for (n, l) in &self.layers {
            layers.entry(*l).or_default().push(n);
        }
        json!({
            "modules": self.modules,
            "nodes": self.nodes,
            "production_edges": self.prod.len(),
            "test_edges": self.test.len(),
            "use_sites": self.prod.values().sum::<u32>() + self.test.values().sum::<u32>(),
            "sccs": self.sccs,
            "mutual_pairs": self.mutual_pairs,
            "layers": layers.iter().map(|(l, v)| (l.to_string(), json!(v))).collect::<serde_json::Map<_, _>>(),
            "uncertain": {"glob_uses": self.glob_uses, "macro_files": self.macro_files},
            "prod": edge_map(&self.prod),
            "test": edge_map(&self.test),
            "lines": self.lines,
            "ext": self.ext,
            "items": self.items.iter().map(|((a, b), v)| (format!("{a}->{b}"), json!(v))).collect::<serde_json::Map<_, _>>(),
        })
    }
}
