//! Host protocol operations (`call.list`, `call.get`, `call.start`, `call.say`, `call.hangup`,
//! `call.close`) and unit tests for persistent Call Rooms (`M08-P01`).

use serde_json::{json, Map, Value};

use crate::core::{self, Reply, Shared};
use crate::session::{obj, Origin, State};
use crate::transfer;

use super::{
    append_call_summary_card, ensure_loaded_and_sync, ensure_pair_room, format_call_envelope,
    format_transcript_snippet, grant_bidirectional_keep, save_inner, session_meta, settle_call_idle,
    CallRoom, MAX_CALL_ROUNDS,
};

pub fn status_json(shared: &Shared) -> Value {
    let mut guard = shared.calls.inner.lock().unwrap();
    ensure_loaded_and_sync(shared, &mut guard);
    let mut list: Vec<&CallRoom> = guard.rooms.iter().filter(|r| !r.closed).collect();
    list.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Value::Array(list.into_iter().map(CallRoom::summary_json).collect())
}

pub fn op_list(shared: &Shared, include_closed: bool) -> Reply {
    let mut guard = shared.calls.inner.lock().unwrap();
    ensure_loaded_and_sync(shared, &mut guard);
    let mut list: Vec<&CallRoom> = guard.rooms.iter().filter(|r| include_closed || !r.closed).collect();
    list.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(obj(json!({"calls": list.into_iter().map(CallRoom::summary_json).collect::<Vec<_>>()})))
}

pub fn op_get(shared: &Shared, call_id: &str) -> Reply {
    let mut guard = shared.calls.inner.lock().unwrap();
    ensure_loaded_and_sync(shared, &mut guard);
    let room = guard
        .rooms
        .iter()
        .find(|r| r.call_id == call_id || (call_id.len() >= 5 && r.call_id.starts_with(call_id)))
        .ok_or("unknown_call")?;
    Ok(obj(json!({"call": room.detail_json()})))
}

pub fn op_start(shared: &Shared, from: &str, to: &str, text_opt: Option<&str>) -> Reply {
    if from == to {
        return Err("bad_request");
    }
    let (from_label, _, _, _) = session_meta(shared, from).ok_or("unknown_session")?;
    let (to_label, _, to_state, _) = session_meta(shared, to).ok_or("unknown_session")?;
    if let Some(t) = text_opt {
        if t.trim().is_empty() || crate::folders::shows_nothing(t) {
            return Err("bad_request");
        }
        match to_state {
            State::Disconnected => return Err("session_disconnected"),
            State::Exited => return Err("session_exited"),
            State::Running => {}
        }
    }
    grant_bidirectional_keep(shared, from, to);
    let context_bundle = if text_opt.is_some() {
        crate::mention::build_recent_context(shared, from, 8, 16 * 1024)
    } else {
        String::new()
    };
    let (call_id, title, snippet, detail_without_text) = {
        let mut guard = shared.calls.inner.lock().unwrap();
        let idx = ensure_pair_room(shared, &mut guard, from, to, true)?;
        let room = &mut guard.rooms[idx];
        room.owner_authorized = true;
        room.paused = false;
        room.closed = false;
        if let Some(msg) = text_opt {
            room.active = true;
            room.round = 1;
            room.push_entry("owner", &format!("Owner (via {from_label})"), "owner", "owner", 1, msg);
        }
        let snip = format_transcript_snippet(&room.entries, usize::from(text_opt.is_some()));
        let cid = room.call_id.clone();
        let ttl = room.title();
        let det = room.detail_json();
        let snapshot = guard.rooms.clone();
        drop(guard);
        save_inner(&shared.cfg.registry_dir, &snapshot);
        (cid, ttl, snip, det)
    };
    if let Some(msg) = text_opt {
        let envelope = format_call_envelope(&title, &call_id, &from_label, from, 1, MAX_CALL_ROUNDS, &context_bundle, &snippet, msg);
        if let Some(found_to) = core::find(shared, to) {
            found_to.lock().unwrap().peer_turn.begin_call(from, Some(call_id.clone()), 1);
        }
        let req_id = format!("cs-{}", core::random_hex(8));
        if let Err(code) = core::input(shared, to, None, &req_id, &envelope, Origin::Session(1), None) {
            if let Some(found_to) = core::find(shared, to) {
                found_to.lock().unwrap().peer_turn.rollback_failed_send(from);
            }
            settle_call_idle(shared, &call_id);
            return Err(code);
        }
        append_call_summary_card(
            shared,
            from,
            &title,
            &call_id,
            &format!("-> '{to_label}' ({}) | round 1/{MAX_CALL_ROUNDS}", transfer::id7(to)),
        );
        return op_get(shared, &call_id);
    }
    Ok(obj(json!({"call": detail_without_text})))
}

pub fn op_say(shared: &Shared, call_id: &str, text: &str, explicit_to: Option<&str>) -> Reply {
    if text.trim().is_empty() || crate::folders::shows_nothing(text) {
        return Err("bad_request");
    }
    let (resolved_cid, sa, la, sb, lb) = {
        let mut guard = shared.calls.inner.lock().unwrap();
        ensure_loaded_and_sync(shared, &mut guard);
        let room = guard
            .rooms
            .iter()
            .find(|r| r.call_id == call_id || (call_id.len() >= 5 && r.call_id.starts_with(call_id)))
            .ok_or("unknown_call")?;
        (
            room.call_id.clone(),
            room.session_a.clone(),
            room.label_a.clone(),
            room.session_b.clone(),
            room.label_b.clone(),
        )
    };
    let targets: Vec<String> = if let Some(to) = explicit_to {
        if to != sa && to != sb {
            return Err("bad_request");
        }
        vec![to.to_string()]
    } else {
        crate::mention::resolve_call_room_targets(shared, &sa, &la, &sb, &lb, text)
    };
    for t in &targets {
        let (_, _, st, _) = session_meta(shared, t).ok_or("unknown_session")?;
        match st {
            State::Disconnected => return Err("session_disconnected"),
            State::Exited => return Err("session_exited"),
            State::Running => {}
        }
    }
    grant_bidirectional_keep(shared, &sa, &sb);
    let (title, snippet) = {
        let mut guard = shared.calls.inner.lock().unwrap();
        let room = guard.rooms.iter_mut().find(|r| r.call_id == resolved_cid).ok_or("unknown_call")?;
        room.owner_authorized = true;
        room.closed = false;
        room.paused = false;
        room.active = true;
        room.round = 1;
        room.push_entry("owner", "Owner", "owner", "owner", 1, text);
        let snip = format_transcript_snippet(&room.entries, 1);
        let ttl = room.title();
        let snapshot = guard.rooms.clone();
        drop(guard);
        save_inner(&shared.cfg.registry_dir, &snapshot);
        (ttl, snip)
    };
    for target in &targets {
        let partner = if target == &sa { &sb } else { &sa };
        let partner_running = session_meta(shared, partner).is_some_and(|m| m.2 == State::Running);
        let reply_caller = if targets.len() == 1 && partner_running { partner.as_str() } else { "owner" };
        let ctx = if reply_caller != "owner" {
            crate::mention::build_recent_context(shared, partner, 4, 8 * 1024)
        } else {
            String::new()
        };
        let envelope = format_call_envelope(
            &title,
            &resolved_cid,
            "Owner",
            &resolved_cid,
            1,
            MAX_CALL_ROUNDS,
            &ctx,
            &snippet,
            text,
        );
        if let Some(found_t) = core::find(shared, target) {
            found_t.lock().unwrap().peer_turn.begin_call(reply_caller, Some(resolved_cid.clone()), 1);
        }
        let req_id = format!("cy-{}", core::random_hex(8));
        if let Err(code) = core::input(shared, target, None, &req_id, &envelope, Origin::Session(1), None) {
            if let Some(found_t) = core::find(shared, target) {
                found_t.lock().unwrap().peer_turn.rollback_failed_send(reply_caller);
            }
            settle_call_idle(shared, &resolved_cid);
            return Err(code);
        }
    }
    let detail = op_get(shared, &resolved_cid)?;
    Ok(obj(json!({"call": detail["call"], "dispatched": targets})))
}

pub fn op_hangup(shared: &Shared, call_id: &str) -> Reply {
    let (resolved_cid, sa, sb, summary) = {
        let mut guard = shared.calls.inner.lock().unwrap();
        ensure_loaded_and_sync(shared, &mut guard);
        let room = guard
            .rooms
            .iter_mut()
            .find(|r| r.call_id == call_id || (call_id.len() >= 5 && r.call_id.starts_with(call_id)))
            .ok_or("unknown_call")?;
        room.paused = true;
        room.active = false;
        room.push_entry("host", "Lico", "host", "system", room.round, "[call hung up / paused by owner]");
        let out = (room.call_id.clone(), room.session_a.clone(), room.session_b.clone(), room.summary_json());
        let snapshot = guard.rooms.clone();
        drop(guard);
        save_inner(&shared.cfg.registry_dir, &snapshot);
        out
    };
    for sid in [&sa, &sb] {
        if let Some(found) = core::find(shared, sid) {
            found.lock().unwrap().peer_turn.cancel_call(&resolved_cid);
        }
    }
    Ok(obj(json!({"call": summary})))
}

pub fn op_close(shared: &Shared, call_id: &str) -> Reply {
    let (resolved_cid, sa, sb) = {
        let mut guard = shared.calls.inner.lock().unwrap();
        ensure_loaded_and_sync(shared, &mut guard);
        let room = guard
            .rooms
            .iter_mut()
            .find(|r| r.call_id == call_id || (call_id.len() >= 5 && r.call_id.starts_with(call_id)))
            .ok_or("unknown_call")?;
        room.paused = true;
        room.active = false;
        room.closed = true;
        let out = (room.call_id.clone(), room.session_a.clone(), room.session_b.clone());
        let snapshot = guard.rooms.clone();
        drop(guard);
        save_inner(&shared.cfg.registry_dir, &snapshot);
        out
    };
    for sid in [&sa, &sb] {
        if let Some(found) = core::find(shared, sid) {
            found.lock().unwrap().peer_turn.cancel_call(&resolved_cid);
        }
    }
    Ok(obj(json!({"call_id": resolved_cid, "closed": true})))
}

pub fn strip_call_done(text: &str) -> (String, bool) {
    const MARKER: &str = "[call done]";
    let mut found = false;
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if i + MARKER.len() <= bytes.len()
            && bytes[i..i + MARKER.len()].eq_ignore_ascii_case(MARKER.as_bytes())
        {
            found = true;
            i += MARKER.len();
            continue;
        }
        let ch = text[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    let trimmed_end = out.trim_end_matches([' ', '\t']);
    for plen in (6..MARKER.len()).rev() {
        let prefix = &MARKER.as_bytes()[..plen];
        if trimmed_end.len() >= plen
            && trimmed_end.as_bytes()[trimmed_end.len() - plen..].eq_ignore_ascii_case(prefix)
        {
            found = true;
            out.truncate(trimmed_end.len() - plen);
            break;
        }
    }
    if !found {
        return (text.to_string(), false);
    }
    let mut lines: Vec<&str> = out.split('\n').map(|l| l.trim_end_matches([' ', '\t'])).collect();
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    (lines.join("\n"), true)
}

pub fn strip_call_done_display_line(line: &str) -> Option<String> {
    if let Some(rest) = line.strip_prefix(">> ") {
        let (cleaned, found) = strip_call_done(rest);
        if !found {
            return Some(line.to_string());
        }
        return if cleaned.trim().is_empty() { None } else { Some(format!(">> {cleaned}")) };
    }
    let (cleaned, found) = strip_call_done(line);
    if !found {
        return Some(line.to_string());
    }
    if cleaned.trim().is_empty() { None } else { Some(cleaned) }
}

pub fn dispatch_op(shared: &Shared, op: &str, request: &Map<String, Value>) -> Reply {
    match op {
        "call.list" => op_list(shared, request.get("include_closed").and_then(Value::as_bool).unwrap_or(false)),
        "call.get" => op_get(shared, crate::ipc::text(request, "call")),
        "call.start" => op_start(
            shared,
            crate::ipc::text(request, "from"),
            crate::ipc::text(request, "to"),
            request.get("text").and_then(Value::as_str),
        ),
        "call.say" => op_say(
            shared,
            crate::ipc::text(request, "call"),
            crate::ipc::text(request, "text"),
            request.get("to").and_then(Value::as_str),
        ),
        "call.hangup" => op_hangup(shared, crate::ipc::text(request, "call")),
        "call.close" => op_close(shared, crate::ipc::text(request, "call")),
        _ => Err("unknown_op"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handoff::mode_of;
    use crate::transfer::tests::{world, ONE, TWO};

    #[test]
    fn call_room_reuses_pair_persists_0600_and_supports_say_hangup_close_and_reopen() {
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
        let started = op_start(&w.shared, ONE, TWO, Some("오늘 날씨 확인 부탁해")).unwrap();
        let cid = started["call"]["call_id"].as_str().unwrap().to_string();
        assert_eq!(started["call"]["title"], "Pri-Lico \u{00b7} Pri-Avatar");
        assert_eq!(started["call"]["status"], "active");
        assert_eq!(started["call"]["round"], 1);
        assert_eq!(started["call"]["owner_authorized"], true);
        let delivered = w.delivered(TWO).unwrap();
        assert!(delivered.starts_with("[lico call 'Pri-Lico \u{00b7} Pri-Avatar' ("), "{delivered}");
        assert!(delivered.contains("| round 1/4 | OWNER-AUTHORIZED]"), "{delivered}");
        assert!(delivered.contains("[host verification:"), "{delivered}");
        assert!(delivered.contains("오늘 날씨 확인 부탁해"), "{delivered}");

        let calls_file = w.shared.cfg.registry_dir.join("calls.json");
        assert_eq!(mode_of(&calls_file), 0o600);

        let hung = op_hangup(&w.shared, &cid).unwrap();
        assert_eq!(hung["call"]["status"], "paused");

        let closed = op_close(&w.shared, &cid).unwrap();
        assert_eq!(closed["closed"], true);
        assert_eq!(op_list(&w.shared, false).unwrap()["calls"].as_array().unwrap().len(), 0);
        assert_eq!(op_list(&w.shared, true).unwrap()["calls"].as_array().unwrap().len(), 1);

        // Reversing (TWO, ONE) reuses & reopens the exact same canonical Call Room!
        let reopened = op_start(&w.shared, TWO, ONE, None).unwrap();
        assert_eq!(reopened["call"]["call_id"], cid);
        assert_eq!(reopened["call"]["status"], "idle");
    }

    #[test]
    fn strip_call_done_removes_full_and_clipped_markers_and_settles_transfer_room() {
        let clipped_weather = ">> 오늘 서울 날씨(2026년 11월 13일, 한국시간): 조회 시점 현재 흐림, 약 12°C입니다. 오늘 낮 예보는 약 14~17°C, 밤에는 약 10~11°C이며 대체로 흐리겠습니다. 출처: 날씨 조회 도구(web weather)의 서울 실시간·시간별 예보. 조회 결과에 원문 URL이 제공되지 않아 출처 링크는 첨부할 수 없습니다. [call done";
        assert_eq!(
            strip_call_done_display_line(clipped_weather).as_deref(),
            Some(">> 오늘 서울 날씨(2026년 11월 13일, 한국시간): 조회 시점 현재 흐림, 약 12°C입니다. 오늘 낮 예보는 약 14~17°C, 밤에는 약 10~11°C이며 대체로 흐리겠습니다. 출처: 날씨 조회 도구(web weather)의 서울 실시간·시간별 예보. 조회 결과에 원문 URL이 제공되지 않아 출처 링크는 첨부할 수 없습니다.")
        );
        assert_eq!(strip_call_done_display_line(">> [call done]"), None);
        assert_eq!(strip_call_done("plain text without marker"), ("plain text without marker".to_string(), false));
        assert_eq!(strip_call_done("답변 완료했습니다. [CALL DONE]\n"), ("답변 완료했습니다.".to_string(), true));

        let w = world(&[ONE, TWO]);
        w.with(ONE, |s| { s.provider = "codex"; s.settings.name = Some("Codex-L".into()); });
        w.with(TWO, |s| { s.provider = "claude"; s.settings.name = Some("Claude-L".into()); });
        let (cid, _) = crate::call_room::record_session_transfer(&w.shared, ONE, TWO, "서울 날씨 흐림 12도 [call done]", 1, true).unwrap();
        let detail = op_get(&w.shared, &cid).unwrap();
        assert_eq!(detail["call"]["active"], false);
        assert_eq!(detail["call"]["entries"][0]["text"], "서울 날씨 흐림 12도");
    }
}
