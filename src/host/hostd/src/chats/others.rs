//! Where the other chats of a folder are (`M11-P02`): when `Resume` finds no chat for the chosen account, the person is told how many chats of the same folder exist
//! under other accounts and under the provider's own default homes. Counts and names of accounts only: no title, no text. The default homes are counted even before
//! `Native` is chosen, so that the hint can lead the person to the choice; that is the same look at the first lines of the transcripts that listing needs, and nothing of
//! it is shown but the number.

use serde_json::{json, Map, Value};

use super::collect;
use crate::core::Shared;

/// The chats of `cwd` under every account and default home except `exclude`: `{"total": n, "by_account": [{"account", "label", "chats"}]}`, the most chats first.
pub fn others(shared: &Shared, cwd: &str, ssh_host: Option<&str>, exclude: Option<&str>) -> Result<Value, &'static str> {
    let mut request = Map::new();
    request.insert("cwd".into(), json!(cwd));
    if let Some(host) = ssh_host {
        request.insert("ssh_host".into(), json!(host));
    }
    request.insert("_all_native".into(), json!(true)); // an internal request: not a field of the wire, which the validation refuses
    let collected = collect(shared, &request)?;
    let mut counts: Vec<(Option<String>, String, usize)> = Vec::new();
    for item in collected.items.iter().filter(|i| i.recorded && i.account.as_deref() != exclude) {
        let label = item.account_label.clone().unwrap_or_else(|| "other".into());
        match counts.iter_mut().find(|(a, _, _)| *a == item.account) {
            Some(entry) => entry.2 += 1,
            None => counts.push((item.account.clone(), label, 1)),
        }
    }
    counts.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.1.cmp(&b.1)));
    let total: usize = counts.iter().map(|c| c.2).sum();
    Ok(json!({"total": total, "by_account": counts.iter().map(|(a, l, n)| json!({"account": a, "label": l, "chats": n})).collect::<Vec<_>>()}))
}

/// The sentence for a creation that found no chat to continue: `3 chats of this folder exist under other accounts or homes (Busy: 2; Native (~/.claude): 1)`.
pub fn sentence(others: &Value) -> Option<String> {
    let total = others["total"].as_u64().filter(|n| *n > 0)?;
    let list: Vec<String> = others["by_account"].as_array()?.iter().take(3).map(|o| format!("{}: {}", o["label"].as_str().unwrap_or("?"), o["chats"].as_u64().unwrap_or(0))).collect();
    let more = if others["by_account"].as_array().map_or(0, Vec::len) > 3 { "; ..." } else { "" };
    Some(format!("{total} chat{} of this folder exist{} under other accounts or homes ({}{more})", if total == 1 { "" } else { "s" }, if total == 1 { "s" } else { "" }, list.join("; ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sentence_names_the_counts_of_the_most_filled_accounts_and_nothing_else() {
        let o = json!({"total": 3, "by_account": [{"account": "a1", "label": "Busy", "chats": 2}, {"account": "native-claude", "label": "Native (~/.claude)", "chats": 1}]});
        assert_eq!(sentence(&o).unwrap(), "3 chats of this folder exist under other accounts or homes (Busy: 2; Native (~/.claude): 1)");
        assert_eq!(sentence(&json!({"total": 1, "by_account": [{"account": "a1", "label": "Solo", "chats": 1}]})).unwrap(), "1 chat of this folder exists under other accounts or homes (Solo: 1)");
        assert_eq!(sentence(&json!({"total": 0, "by_account": []})), None);
    }
}
