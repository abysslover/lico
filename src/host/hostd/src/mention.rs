//! `@AgentName` mention parser, session resolver, and recent-context bundler (`M08-P01`).
//!
//! Supports:
//! - Leading mentions (`@Pri-Avatar 오늘 날씨 알려줘`, `@"Pri Avatar": ...`, `@s3f20f6 ...`) in
//!   human session prompts: immediately authorizes the pair (`owner_authorized = true`), attaches
//!   up to the last 8 user/assistant turns (`<= 16 KiB`) from the calling session + Call Room
//!   transcript, records a clickable Call Room jump card in the caller, and dispatches Round 1/4.
//! - Inline mentions (`Pri-Lico야, @Pri-Avatar 에게 물어봐줘`): immediately connects & pre-authorizes
//!   the Call Room (`keep = true` both ways, zero approval modal) and informs the caller's LLM of
//!   the target session ID and active owner authorization.

use serde_json::json;

use crate::codex::{clip, sanitize};
use crate::core::{self, check_generation, Reply, Shared};
use crate::leads;
use crate::session::{now_secs, obj, InputState, Origin, State};
use crate::transfer;

#[derive(Clone, Debug)]
struct Candidate {
    id: String,
    name: Option<String>,
    auto_title: Option<String>,
    label: String,
    running: bool,
}

fn collect_candidates(shared: &Shared, exclude_id: Option<&str>) -> Vec<Candidate> {
    let list = shared.sessions.read().unwrap().clone();
    let mut out = Vec::new();
    for arc in list {
        let s = arc.lock().unwrap();
        if s.purged || exclude_id == Some(s.id.as_str()) {
            continue;
        }
        out.push(Candidate {
            id: s.id.clone(),
            name: s.settings.name.clone().filter(|n| !n.trim().is_empty()),
            auto_title: s.auto_title.clone().filter(|t| !t.trim().is_empty()),
            label: transfer::session_label(&s),
            running: s.state == State::Running,
        });
    }
    // Prefer running sessions first when resolving case-insensitive names.
    out.sort_by(|a, b| b.running.cmp(&a.running));
    out
}

pub fn resolve_session_token(shared: &Shared, exclude_id: Option<&str>, raw_token: &str) -> Option<String> {
    let token = raw_token.trim();
    if token.is_empty() {
        return None;
    }
    let lower = token.to_lowercase();
    let cands = collect_candidates(shared, exclude_id);

    // 1. Exact case-insensitive match on explicit `settings.name` or `label`.
    if let Some(c) = cands.iter().find(|c| {
        c.name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(token) || n.to_lowercase() == lower)
            || c.label.to_lowercase() == lower
    }) {
        return Some(c.id.clone());
    }
    // 2. Exact match or >= 5-char prefix on session id (`s...`).
    if token.starts_with('s') && token.len() >= 5 && token.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
        let id_matches: Vec<&Candidate> = cands.iter().filter(|c| c.id == token || c.id.starts_with(token)).collect();
        if id_matches.len() == 1 {
            return Some(id_matches[0].id.clone());
        }
    }
    // 3. Exact case-insensitive match on `auto_title`.
    if let Some(c) = cands.iter().find(|c| c.auto_title.as_deref().is_some_and(|t| t.to_lowercase() == lower)) {
        return Some(c.id.clone());
    }
    // 4. Unambiguous >= 3-char case-insensitive prefix on explicit `settings.name`.
    if token.chars().count() >= 3 {
        let pref: Vec<&Candidate> = cands
            .iter()
            .filter(|c| c.name.as_deref().is_some_and(|n| n.to_lowercase().starts_with(&lower)))
            .collect();
        if pref.len() == 1 {
            return Some(pref[0].id.clone());
        }
    }
    None
}

fn is_mention_boundary_prev(prev: Option<char>) -> bool {
    match prev {
        None => true,
        Some(c) => c.is_whitespace() || "([<{,:;\"'".contains(c),
    }
}

fn is_token_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '-' | '_' | '.')
}

/// Consume one `@<token>` or `@"<quoted>"` at the start of `s`, returning `(token, rest_after_token)`.
fn consume_at_token(s: &str) -> Option<(&str, &str)> {
    let after_at = s.strip_prefix('@')?;
    if let Some(q) = after_at.chars().next().filter(|&c| c == '"' || c == '\'') {
        let inner = &after_at[q.len_utf8()..];
        let end = inner.find(q)?;
        let tok = &inner[..end];
        if tok.trim().is_empty() {
            return None;
        }
        return Some((tok, &inner[end + q.len_utf8()..]));
    }
    let end = after_at
        .char_indices()
        .find(|&(_, c)| !is_token_char(c))
        .map_or(after_at.len(), |(i, _)| i);
    if end == 0 {
        return None;
    }
    Some((&after_at[..end], &after_at[end..]))
}

/// Strip Korean postpositions commonly attached to `@AgentName` when no exact match exists
/// (e.g. `@Pri-Avatar에게`, `@Pri-Avatar한테`, `@Pri-Avatar야`, `@Pri-Avatar님`).
fn resolve_with_particles(shared: &Shared, exclude_id: Option<&str>, tok: &str) -> Option<String> {
    if let Some(id) = resolve_session_token(shared, exclude_id, tok) {
        return Some(id);
    }
    for suffix in ["에게서", "한테서", "에게", "한테", "으로", "로", "님", "야", "아", "은", "는", "이", "가", "을", "를"] {
        if let Some(stem) = tok.strip_suffix(suffix) {
            if !stem.is_empty() {
                if let Some(id) = resolve_session_token(shared, exclude_id, stem) {
                    return Some(id);
                }
            }
        }
    }
    None
}

/// Parse leading `@Target` mentions at the start of `text`, returning `(target_ids, cleaned_body)`.
pub fn parse_leading_mentions(shared: &Shared, exclude_id: Option<&str>, text: &str) -> Option<(Vec<String>, String)> {
    let mut cur = text.trim_start();
    let mut targets: Vec<String> = Vec::new();
    while let Some((tok, mut rest)) = consume_at_token(cur) {
        let Some(id) = resolve_with_particles(shared, exclude_id, tok) else {
            break;
        };
        if !targets.contains(&id) {
            targets.push(id);
        }
        rest = rest.trim_start_matches([':', ',']);
        cur = rest.trim_start();
    }
    if targets.is_empty() {
        return None;
    }
    let body = if cur.is_empty() { text.trim().to_string() } else { cur.to_string() };
    Some((targets, body))
}

/// Scan `text` for inline `@Target` mentions anywhere after the start.
pub fn find_inline_mentions(shared: &Shared, exclude_id: Option<&str>, text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut prev: Option<char> = None;
    for (idx, ch) in text.char_indices() {
        if ch == '@' && is_mention_boundary_prev(prev) {
            if let Some((tok, _)) = consume_at_token(&text[idx..]) {
                if let Some(id) = resolve_with_particles(shared, exclude_id, tok) {
                    if !out.contains(&id) {
                        out.push(id);
                    }
                }
            }
        }
        prev = Some(ch);
    }
    out
}

/// Resolve which participant(s) `call.say` inside Call Room `(sa, sb)` addresses.
pub fn resolve_call_room_targets(
    shared: &Shared,
    sa: &str,
    la: &str,
    sb: &str,
    lb: &str,
    text: &str,
) -> Vec<String> {
    let trimmed = text.trim_start();
    if trimmed.starts_with("@all") || trimmed.starts_with("@both") || trimmed.starts_with("@모두") {
        return vec![sa.to_string(), sb.to_string()];
    }
    let mut matched = Vec::new();
    let mut prev = None;
    for (idx, ch) in text.char_indices() {
        if ch == '@' && is_mention_boundary_prev(prev) {
            if let Some((tok, _)) = consume_at_token(&text[idx..]) {
                let low = tok.to_lowercase();
                if la.to_lowercase() == low || sa.starts_with(tok) {
                    if !matched.contains(&sa.to_string()) {
                        matched.push(sa.to_string());
                    }
                } else if lb.to_lowercase() == low || sb.starts_with(tok) {
                    if !matched.contains(&sb.to_string()) {
                        matched.push(sb.to_string());
                    }
                } else if let Some(id) = resolve_with_particles(shared, None, tok) {
                    if (id == sa || id == sb) && !matched.contains(&id) {
                        matched.push(id);
                    }
                }
            }
        }
        prev = Some(ch);
    }
    if !matched.is_empty() {
        return matched;
    }
    // Default in Call Room `⇄ A · B`: route to `B` if running, else `A`.
    let b_running = core::find(shared, sb).is_some_and(|f| f.lock().unwrap().state == State::Running);
    if b_running {
        vec![sb.to_string()]
    } else {
        vec![sa.to_string()]
    }
}

/// Extract up to the last `max_turns` user+assistant turns (`2 * max_turns` role blocks, `<= max_bytes`)
/// from `session_id`'s durable `.seg` log, formatted cleanly and `leads::defuse`d for safe inclusion.
pub fn build_recent_context(shared: &Shared, session_id: &str, max_turns: usize, max_bytes: usize) -> String {
    let Some(found) = core::find(shared, session_id) else {
        return String::new();
    };
    let (view, text_mode) = {
        let s = found.lock().unwrap();
        if s.purged || !s.text_mode {
            return String::new();
        }
        (s.log_view(), s.text_mode)
    };
    if !text_mode {
        return String::new();
    }
    let mut blocks: Vec<(&'static str, String)> = Vec::new();
    let mut in_peer_turn = false;
    let mut in_host_notice = false;
    let _ = view.scan_each(|_, bytes| {
        let Ok(line) = std::str::from_utf8(bytes) else { return true };
        if !line.starts_with(leads::TAG) {
            if line == "[peer turn started]" {
                in_peer_turn = true;
                return true;
            }
            if in_peer_turn {
                if line == "[turn completed]"
                    || line == "[turn interrupted]"
                    || line == "[session closed]"
                    || line == "[provider disconnected]"
                    || line.starts_with("[turn failed")
                {
                    in_peer_turn = false;
                }
                return true;
            }
            if line == "[host notice]" || line == "[call summary]" {
                in_host_notice = true;
                return true;
            }
            if in_host_notice {
                if line == ">>" || line.starts_with(">> ") {
                    return true;
                }
                in_host_notice = false;
            }
            if line == "[peer reply]"
                || line.starts_with("[turn ")
                || line.starts_with("[exit ")
                || line.starts_with("[session ")
                || line.starts_with("[provider ")
                || line.starts_with("[tool")
                || line.starts_with("[patch]")
                || line.starts_with("[account ")
                || line.starts_with("[log recovered:")
                || line.starts_with("$ ")
                || line.starts_with("> ")
                || line == "$"
                || line == ">"
            {
                return true;
            }
            if let Some(u) = line.strip_prefix(">> ") {
                if u.starts_with("[from lico |") || u.starts_with("[\u{21c4} call ") {
                    return true;
                }
                let role = if u.starts_with("[peer reply from ") || u.starts_with("[from session ") {
                    "Peer"
                } else {
                    "User"
                };
                if let Some((last_role, buf)) = blocks.last_mut().filter(|(r, _)| *r == role) {
                    let _ = last_role;
                    if buf.len() < 3072 {
                        buf.push('\n');
                        buf.push_str(&clip(u, 3072 - buf.len()));
                    }
                } else {
                    blocks.push((role, clip(u, 3072)));
                }
                return true;
            }
            if line.starts_with('[') && line.ends_with("not shown]") {
                return true;
            }
        }
        let clean = line.strip_prefix(leads::TAG).unwrap_or(line);
        if clean.trim().is_empty() {
            return true;
        }
        if let Some((_, buf)) = blocks.last_mut().filter(|(r, _)| *r == "Assistant") {
            if buf.len() < 3072 {
                buf.push('\n');
                buf.push_str(&clip(clean, 3072 - buf.len()));
            }
        } else {
            blocks.push(("Assistant", clip(clean, 3072)));
        }
        true
    });

    let keep_blocks = max_turns.saturating_mul(2).max(2);
    let start_idx = blocks.len().saturating_sub(keep_blocks);
    let chosen = &blocks[start_idx..];
    let mut rendered: Vec<String> = Vec::new();
    let mut total = 0usize;
    for (role, text) in chosen.iter().rev() {
        let mut piece = String::new();
        for (i, ln) in text.lines().enumerate() {
            let formatted = if i == 0 {
                format!("  {role}: {}", sanitize(ln))
            } else {
                format!("    {}", sanitize(ln))
            };
            let safe = leads::defuse(&formatted);
            if !piece.is_empty() {
                piece.push('\n');
            }
            piece.push_str(&safe);
        }
        if total + piece.len() + 1 > max_bytes && !rendered.is_empty() {
            break;
        }
        total += piece.len() + 1;
        rendered.push(piece);
    }
    rendered.reverse();
    rendered.join("\n")
}

pub enum PersonRoute {
    Handled(Reply),
    Augmented { llm_text: String },
    Plain,
}

/// Inspect a human `Origin::Person` input to `session`:
/// - Leading `@Target <msg>`: records the prompt in `session`, bundles recent context before this prompt,
///   dispatches the `OWNER-AUTHORIZED` Call Room turn to `Target`, and returns `PersonRoute::Handled`.
/// - Inline `@Target`: connects & pre-authorizes the Call Room (`keep = true` both ways) and appends a
///   concise `[host verification: ...]` hint for `session`'s LLM (`PersonRoute::Augmented`).
pub fn route_person_input(
    shared: &Shared,
    session: &str,
    generation: Option<i64>,
    request_id: &str,
    text: &str,
) -> PersonRoute {
    if !text.contains('@') {
        return PersonRoute::Plain;
    }
    if let Some((targets, clean_body)) = parse_leading_mentions(shared, Some(session), text) {
        let Some(found) = core::find(shared, session) else {
            return PersonRoute::Handled(Err("unknown_session"));
        };
        {
            let s = found.lock().unwrap();
            if let Err(code) = check_generation(&s, generation) {
                return PersonRoute::Handled(Err(code));
            }
            if let Some(state) = s.input_state(request_id) {
                return PersonRoute::Handled(Ok(obj(json!({
                    "session": s.id,
                    "request_id": request_id,
                    "state": state.name(),
                    "duplicate": true,
                }))));
            }
            if s.state == State::Disconnected {
                return PersonRoute::Handled(Err("session_disconnected"));
            }
            if s.state == State::Exited {
                return PersonRoute::Handled(Err("session_exited"));
            }
        }
        // Read recent context BEFORE recording this `@mention` prompt so it is not duplicated in context.
        let context_bundle = build_recent_context(shared, session, 8, 16 * 1024);
        {
            let mut s = found.lock().unwrap();
            if let Err(code) = check_generation(&s, generation) {
                return PersonRoute::Handled(Err(code));
            }
            if let Some(state) = s.input_state(request_id) {
                return PersonRoute::Handled(Ok(obj(json!({
                    "session": s.id,
                    "request_id": request_id,
                    "state": state.name(),
                    "duplicate": true,
                }))));
            }
            s.record_input(request_id, text, Origin::Person, now_secs());
            s.advance_input(request_id, InputState::Observed);
            if s.tab_closed {
                s.tab_closed = false;
                crate::registry::mark_dirty();
            }
        }
        match crate::call_room::dispatch_leading_mentions(shared, session, &targets, &clean_body, &context_bundle) {
            Ok(_calls) => {
                return PersonRoute::Handled(Ok(obj(json!({
                    "session": session,
                    "request_id": request_id,
                    "state": "observed",
                    "duplicate": false,
                }))));
            }
            Err(code) => return PersonRoute::Handled(Err(code)),
        }
    }

    let inline = find_inline_mentions(shared, Some(session), text);
    if inline.is_empty() {
        return PersonRoute::Plain;
    }
    let mut notes = Vec::new();
    for tid in &inline {
        if let Some((cid, title, tlabel)) = crate::call_room::connect_inline_mention(shared, session, tid, text) {
            notes.push(format!(
                "[host verification: Lico's human owner mentioned @{tlabel} (session id '{tid}') in this prompt. Call Room '{title}' ({}) is now owner-authorized: calling mcp__lico__send(to=\"{tid}\", text=...) executes immediately without waiting for approval, and '{tlabel}'s reply will return here automatically.]",
                crate::call_room::call_id7(&cid)
            ));
        }
    }
    if notes.is_empty() {
        PersonRoute::Plain
    } else {
        PersonRoute::Augmented {
            llm_text: format!("{text}\n\n{}", notes.join("\n")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transfer::tests::{world, ONE, TWO};

    #[test]
    fn resolves_agent_mentions_case_insensitively_with_korean_particles_and_quotes() {
        let w = world(&[ONE, TWO]);
        w.with(ONE, |s| {
            s.provider = "claude";
            s.text_mode = true;
            s.settings.name = Some("Pri-Lico".into());
        });
        w.with(TWO, |s| {
            s.provider = "codex";
            s.text_mode = true;
            s.settings.name = Some("Pri-Avatar".into());
        });
        let (targets, body) = parse_leading_mentions(&w.shared, Some(ONE), "@pri-avatar 오늘 서울 날씨 알려줘").unwrap();
        assert_eq!(targets, vec![TWO.to_string()]);
        assert_eq!(body, "오늘 서울 날씨 알려줘");

        let inline = find_inline_mentions(&w.shared, Some(ONE), "Pri-Lico야 이 내용을 @Pri-Avatar에게 전달해줘");
        assert_eq!(inline, vec![TWO.to_string()]);
    }

    #[test]
    fn builds_recent_context_from_durable_log_skipping_peer_turns_and_tool_noise() {
        let w = world(&[ONE, TWO]);
        w.with(ONE, |s| {
            s.provider = "claude";
            s.text_mode = true;
            s.append(b"[session started] model=claude-sonnet".to_vec());
            s.append(b">> 8\xeb\x8b\xa8\xea\xb3\x84 \xea\xb2\x80\xc2\xa0\xea\xb2\xb0\xea\xb3\xbc \xec\xa0\x95\xeb\xa6\xac".to_vec());
            s.append(b"$ git status".to_vec());
            s.append(b"> clean".to_vec());
            s.append(b"[exit 0]".to_vec());
            s.append(b"BCN:1\xea\xb3\xbc SLM-EMR \xec\x8a\xb9\xec\x9d\xb8\xec\x9d\x84 \xeb\xb0\x98\xec\x98\x81\xed\x96\x88\xec\x8a\xb5\xeb\x8b\x88\xeb\x8b\xa4.".to_vec());
            s.append(b"[turn completed]".to_vec());
            s.append(b"[peer turn started]".to_vec());
            s.append(b">> secret hidden peer turn".to_vec());
            s.append(b"hidden peer answer".to_vec());
            s.append(b"[turn completed]".to_vec());
        });
        let ctx = build_recent_context(&w.shared, ONE, 8, 16 * 1024);
        assert!(ctx.contains("User: ") && ctx.contains("Assistant: BCN:1"), "{ctx}");
        assert!(!ctx.contains("git status") && !ctx.contains("secret hidden peer turn"), "{ctx}");
    }
}
