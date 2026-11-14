//! What the "Find Local LLM servers..." dialog shows of a result (`M13`): the servers as two-line rows, the fold of the servers that list no model, the full text of the
//! selected row, the words for how old a kept result is. Pure functions of the host's reply and of the width in columns; nothing here draws or asks.

use serde_json::Value;

use super::controls::wrap_cols;
use super::sidebar_render::clip_cols;

pub(super) const NO_MODEL_NOTE: &str = "No model listed: the server answered, but it lists no model, so nothing can be run on it yet (for example nothing is pulled, or a key is needed).";
const DETAIL_LINES: usize = 6;

/// A server (`openai` or `ollama`) of a reply.
pub(super) struct Item {
    pub addr: String,
    pub kind: String,
    pub bind: String,
    pub auth: &'static str,
    pub owner: String,
    pub ids: Vec<String>,
    pub total: usize,
    pub folded: bool,
    pub via_ssh: bool,
}

/// What a line of the list is: a server (its index in the displayed items) or the fold line.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Entry {
    Item(usize),
    More,
}

impl Entry {
    pub(super) fn lines(self) -> usize {
        if self == Entry::More { 1 } else { 2 }
    }
}

impl Item {
    /// The first line of a row: where it listens, what it is, who may use it, whose it is.
    pub(super) fn head(&self) -> String {
        format!("{}  {}  {}  {}{}", self.addr, self.kind, self.bind, self.auth, self.owner)
    }
}

pub(super) fn is_server(e: &Value) -> bool {
    matches!(e["kind"].as_str(), Some("openai" | "ollama"))
}

/// A server that answered but lists no model and does not ask for a key: nothing can be run on it yet. The process name never decides this.
fn is_folded(e: &Value) -> bool {
    let listed = e["models"].as_array().is_some_and(|m| !m.is_empty()) || e["models_total"].as_u64().unwrap_or(0) > 0;
    !listed && e["auth"].as_str() != Some("key_needed")
}

fn auth_word(e: &Value) -> &'static str {
    match e["auth"].as_str() {
        Some("key_needed") => "key needed",
        Some("none_needed") => "open",
        _ => "?",
    }
}

fn owner_of(e: &Value) -> String {
    match e["process"].as_str() {
        Some("ssh") => "  via ssh".to_string(),
        Some(p) => format!("  ({p})"),
        None => String::new(),
    }
}

/// The servers of a reply in the order they are shown: those that list models (or ask for a key) first, then the folded ones, each group as the host sorted them.
pub(super) fn items(reply: &Value) -> Vec<Item> {
    let all: Vec<&Value> = reply["endpoints"].as_array().into_iter().flatten().filter(|e| is_server(e)).collect();
    let make = |e: &Value| Item {
        addr: format!("{}:{}", e["address"].as_str().unwrap_or("?"), e["port"]),
        kind: e["kind"].as_str().unwrap_or("?").to_string(),
        bind: e["bind"].as_str().unwrap_or("?").to_string(),
        auth: auth_word(e),
        owner: owner_of(e),
        ids: e["models"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect(),
        total: e["models_total"].as_u64().unwrap_or(0) as usize,
        folded: is_folded(e),
        via_ssh: e["process"].as_str() == Some("ssh"),
    };
    let (folded, kept): (Vec<&Value>, Vec<&Value>) = all.into_iter().partition(|e| is_folded(e));
    kept.into_iter().chain(folded).map(make).collect()
}

/// The lines of the list: the servers that are shown, the fold line after them when there are folded ones, and the folded servers when it is open.
pub(super) fn entries(items: &[Item], open: bool) -> Vec<Entry> {
    let kept = items.iter().take_while(|i| !i.folded).count();
    let mut out: Vec<Entry> = (0..kept).map(Entry::Item).collect();
    if kept < items.len() {
        out.push(Entry::More);
        if open {
            out.extend((kept..items.len()).map(Entry::Item));
        }
    }
    out
}

/// "N more servers answered but list no model - click or press F to show" (or to hide them).
pub(super) fn fold_label(count: usize, open: bool) -> String {
    let s = if count == 1 { "" } else { "s" };
    if open {
        format!("Hide the {count} server{s} that list no model (click or press F)")
    } else {
        format!("{count} more server{s} answered but list no model - click or press F to show")
    }
}

fn width(text: &str) -> usize {
    text.chars().map(|c| if super::editor::wide(c) { 2 } else { 1 }).sum()
}

/// The models of a row in one line of at most `cols` columns: as many whole ids as fit, then ` +N` for the rest. Whether something is left out is the second value.
pub(super) fn model_line(ids: &[String], total: usize, cols: usize) -> (String, bool) {
    if ids.is_empty() {
        return ("no model listed".to_string(), false);
    }
    let total = total.max(ids.len());
    let mut shown = 0;
    for k in 1..=ids.len() {
        let text = ids[..k].join(", ") + &if total > k { format!(" +{}", total - k) } else { String::new() };
        if width(&text) > cols {
            break;
        }
        shown = k;
    }
    if shown == 0 {
        return (clip_cols(&(ids[0].clone() + &if total > 1 { format!(" +{}", total - 1) } else { String::new() }), cols), true);
    }
    (ids[..shown].join(", ") + &if total > shown { format!(" +{}", total - shown) } else { String::new() }, shown < total)
}

/// The two lines of a row, and whether either of them leaves something out (the detail area then has it whole).
pub(super) fn row_lines(item: &Item, cols: usize) -> ([String; 2], bool) {
    let (models, cut) = model_line(&item.ids, item.total, cols.saturating_sub(2));
    let head = item.head();
    let head_cut = width(&head) > cols;
    ([clip_cols(&head, cols), format!("  {models}")], cut || head_cut)
}

/// The one-line form of a row with the first three models: what the window reports in `rows`.
pub(super) fn row_text(item: &Item) -> String {
    let shown: Vec<&str> = item.ids.iter().take(3).map(String::as_str).collect();
    let total = item.total.max(item.ids.len());
    let list = if shown.is_empty() { "no model listed".to_string() } else { format!("{}{}", shown.join(", "), if total > shown.len() { format!(" +{}", total - shown.len()) } else { String::new() }) };
    format!("{}  {}  {}  {}  {list}{}", item.addr, item.kind, item.bind, item.auth, item.owner)
}

/// The full text of the selected row: its line, then every model id the reply holds, wrapped; at most `DETAIL_LINES` lines, and what is left over is said.
pub(super) fn detail(item: &Item, cols: usize) -> Vec<String> {
    let mut out = wrap_cols(&item.head(), cols);
    let models = if item.ids.is_empty() { "no model listed".to_string() } else { item.ids.join(", ") };
    out.extend(wrap_cols(&models, cols));
    if out.len() > DETAIL_LINES {
        out.truncate(DETAIL_LINES - 1);
        out.push(format!("... {} model{} in all; `lico llm discover` prints every id", item.total.max(item.ids.len()), if item.total.max(item.ids.len()) == 1 { "" } else { "s" }));
    }
    out
}

/// How old a kept result is.
pub(super) fn age_words(secs: u64) -> String {
    match secs {
        0..=4 => "just now".to_string(),
        5..=59 => format!("{secs} s ago"),
        60..=3599 => format!("{} min ago", secs / 60),
        _ => format!("{} h ago", secs / 3600),
    }
}

/// The first entry to show so that entry `selected` is on screen: lines are counted (a server takes two, the fold line one), at most `max_lines` of them.
pub(super) fn scroll_for(list: &[Entry], selected: Option<usize>, scroll: usize, max_lines: usize) -> usize {
    let Some(at) = selected.and_then(|s| list.iter().position(|e| *e == Entry::Item(s))) else { return scroll.min(list.len().saturating_sub(1)) };
    let mut first = scroll.min(at);
    while list[first..=at].iter().map(|e| e.lines()).sum::<usize>() > max_lines && first < at {
        first += 1;
    }
    first
}

/// How many entries from `first` fit in `max_lines`.
pub(super) fn fit(list: &[Entry], first: usize, max_lines: usize) -> usize {
    let mut used = 0;
    list[first.min(list.len())..].iter().take_while(|e| { used += e.lines(); used <= max_lines }).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ep(kind: &str, port: u64, ids: &[&str], total: u64, auth: &str, process: Option<&str>) -> Value {
        json!({"kind": kind, "address": "127.0.0.1", "port": port, "bind": "loopback", "auth": auth, "models": ids, "models_total": total, "process": process})
    }

    fn reply() -> Value {
        json!({"endpoints": [
            ep("openai", 1, &[], 0, "none_needed", Some("ssh")),
            ep("ollama", 2, &["a", "b"], 2, "none_needed", Some("ssh")),
            ep("openai", 3, &[], 0, "key_needed", None),
            ep("not-http", 4, &[], 0, "unknown", None),
            ep("ollama", 5, &[], 0, "none_needed", Some("ollama"))]})
    }

    #[test]
    fn servers_that_list_models_or_need_a_key_come_first_and_the_others_are_folded_whatever_their_process_is_called() {
        let list = items(&reply());
        assert_eq!(list.iter().map(|i| (i.folded, i.via_ssh)).collect::<Vec<_>>(), [(false, true), (false, false), (true, true), (true, false)], "an ssh-owned server with models stays; one with none folds by that rule only");
        assert_eq!(list[0].head(), "127.0.0.1:2  ollama  loopback  open  via ssh");
        assert_eq!(entries(&list, false), [Entry::Item(0), Entry::Item(1), Entry::More]);
        assert_eq!(entries(&list, true), [Entry::Item(0), Entry::Item(1), Entry::More, Entry::Item(2), Entry::Item(3)]);
        assert_eq!(entries(&items(&json!({"endpoints": []})), false), []);
        assert_eq!(fold_label(2, false), "2 more servers answered but list no model - click or press F to show");
        assert_eq!(fold_label(1, false), "1 more server answered but list no model - click or press F to show");
    }

    #[test]
    fn the_models_of_a_row_are_whole_ids_with_a_count_of_the_rest_and_a_line_that_cuts_says_so() {
        let ids: Vec<String> = ["tiny-llama", "qwen-small", "mistral-7b"].iter().map(|s| s.to_string()).collect();
        assert_eq!(model_line(&ids, 3, 80), ("tiny-llama, qwen-small, mistral-7b".to_string(), false));
        assert_eq!(model_line(&ids, 9, 80), ("tiny-llama, qwen-small, mistral-7b +6".to_string(), true));
        assert_eq!(model_line(&ids, 3, 28), ("tiny-llama, qwen-small +1".to_string(), true));
        assert_eq!(model_line(&ids, 3, 8), ("tiny-ll\u{2026}".to_string(), true), "not even one id fits: it is cut, and that is said");
        assert_eq!(model_line(&[], 0, 80), ("no model listed".to_string(), false));
    }

    #[test]
    fn a_row_is_two_lines_and_its_detail_has_every_id_in_full() {
        let long: Vec<String> = (0..3).map(|n| format!("org/Some-Very-Long-Model-Name-{n}-instruct-q4_k_m-{}", "x".repeat(30))).collect();
        let item = make("10.0.0.5:8000", "openai", "all", "  (llama-server)", long.clone(), 3);
        let (lines, cut) = row_lines(&item, 60);
        assert!(cut && lines.iter().all(|l| width(l) <= 60), "{lines:?}");
        let text = detail(&item, 96).join("");
        assert!(long.iter().all(|id| text.replace(' ', "").contains(id)), "{text}");
        let many = make("h:1", "openai", "all", "", (0..50).map(|n| format!("model-number-{n:02}-with-a-long-name")).collect(), 120);
        let lines = detail(&many, 60);
        assert_eq!(lines.len(), DETAIL_LINES);
        assert!(lines.last().unwrap().contains("120 models in all") && lines.last().unwrap().contains("lico llm discover"), "{lines:?}");
        assert_eq!(row_text(&list_of_one()), "127.0.0.1:11434  ollama  loopback  open  llama3:8b, mistral:7b, phi3 +3  (ollama)");
    }

    fn list_of_one() -> Item {
        make("127.0.0.1:11434", "ollama", "loopback", "  (ollama)", ["llama3:8b", "mistral:7b", "phi3", "qwen2"].iter().map(|s| s.to_string()).collect(), 6)
    }

    fn make(addr: &str, kind: &str, bind: &str, owner: &str, ids: Vec<String>, total: usize) -> Item {
        Item { addr: addr.into(), kind: kind.into(), bind: bind.into(), auth: "open", owner: owner.into(), ids, total, folded: false, via_ssh: false }
    }

    #[test]
    fn a_kept_result_has_its_age_in_words_and_the_list_scrolls_by_lines_to_keep_the_selection_in_view() {
        assert_eq!([age_words(0), age_words(4), age_words(5), age_words(59), age_words(60), age_words(125), age_words(3599), age_words(3600), age_words(7300)], ["just now", "just now", "5 s ago", "59 s ago", "1 min ago", "2 min ago", "59 min ago", "1 h ago", "2 h ago"]);
        let list: Vec<Entry> = (0..6).map(Entry::Item).chain([Entry::More]).collect();
        assert_eq!(fit(&list, 0, 8), 4, "two lines a server");
        assert_eq!(scroll_for(&list, Some(3), 0, 8), 0);
        assert_eq!(scroll_for(&list, Some(5), 0, 8), 2, "the selection is the last line shown");
        assert_eq!(scroll_for(&list, Some(1), 3, 8), 1, "back up");
        assert_eq!(scroll_for(&list, None, 99, 8), 6);
    }
}
