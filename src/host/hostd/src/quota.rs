//! Per-account provider rate-limit / weekly quota telemetry (`M04-P03`).
//!
//! Captures structured quota snapshots reported by official provider streams:
//! - Codex app-server: `account/rateLimits/read` responses and `account/rateLimits/updated` notifications
//!   (`RateLimitSnapshot` `primary` / `secondary` `usedPercent`, `windowDurationMins`, `resetsAt`).
//! - Claude Code CLI (`stream-json`): `rate_limit_event` (`rate_limit_info` nested `seven_day` / `five_hour`
//!   or flat `rateLimitType` + `utilization` / `status` + `resetsAt`).
//!
//! Unknown is never reported as zero: `usage_json` returns `Value::Null` until a provider actually reports
//! at least one rate-limit window for that account.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Map, Value};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QuotaEntry {
    pub weekly_remaining_pct: Option<u8>,
    pub weekly_resets_at: Option<u64>,
    pub short_remaining_pct: Option<u8>,
    pub short_resets_at: Option<u64>,
    pub updated_at: u64,
}

fn store() -> &'static Mutex<HashMap<String, QuotaEntry>> {
    static STORE: OnceLock<Mutex<HashMap<String, QuotaEntry>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Key under which a session's provider quota is tracked (`account` ID when bound, or a per-provider
/// unbound slot when running against the host's default provider home).
pub fn session_key(provider: &str, account: Option<&str>) -> String {
    match account.filter(|a| !a.is_empty()) {
        Some(id) => id.to_string(),
        None if matches!(provider, "claude" | "codex") => format!("__unbound_{provider}__"),
        None => String::new(),
    }
}

/// Clear stored telemetry when an account is removed.
pub fn forget(account: &str) {
    if !account.is_empty() {
        store().lock().unwrap_or_else(|e| e.into_inner()).remove(account);
    }
}

/// Protocol JSON for `account.list` `usage` and `status` `account_usage`; `Value::Null` when unreported.
pub fn usage_json(key: &str) -> Value {
    if key.is_empty() {
        return Value::Null;
    }
    let guard = store().lock().unwrap_or_else(|e| e.into_inner());
    let Some(entry) = guard.get(key) else { return Value::Null };
    if entry.weekly_remaining_pct.is_none() && entry.short_remaining_pct.is_none() {
        return Value::Null;
    }
    let mut map = Map::new();
    map.insert("updated_at".into(), json!(entry.updated_at));
    if let Some(w) = entry.weekly_remaining_pct {
        map.insert("weekly_remaining_pct".into(), json!(w));
    }
    if let Some(r) = entry.weekly_resets_at {
        map.insert("weekly_resets_at".into(), json!(r));
    }
    if let Some(s) = entry.short_remaining_pct {
        map.insert("short_remaining_pct".into(), json!(s));
    }
    if let Some(r) = entry.short_resets_at {
        map.insert("short_resets_at".into(), json!(r));
    }
    Value::Object(map)
}

fn remaining_from_used_pct(used: i64) -> u8 {
    (100_i64.saturating_sub(used)).clamp(0, 100) as u8
}

fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = y - i64::from(m <= 2);
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let m_adj = u64::from(if m > 2 { m - 3 } else { m + 9 });
    let doy = (153 * m_adj + 2) / 5 + u64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + (doe as i64) - 719_468
}

fn parse_iso8601_ts(raw: &str) -> Option<u64> {
    let s = raw.trim();
    if s.len() < 19 {
        return None;
    }
    let b = s.as_bytes();
    if b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b't' | b' ') || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let year: i64 = s.get(0..4)?.parse().ok()?;
    let month: u32 = s.get(5..7)?.parse().ok()?;
    let day: u32 = s.get(8..10)?.parse().ok()?;
    let hour: i64 = s.get(11..13)?.parse().ok()?;
    let min: i64 = s.get(14..16)?.parse().ok()?;
    let sec: i64 = s.get(17..19)?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || min > 59 || sec > 60 {
        return None;
    }
    let mut rest = s.get(19..)?;
    if let Some(after_dot) = rest.strip_prefix('.') {
        let digits = after_dot.bytes().take_while(|c| c.is_ascii_digit()).count();
        rest = after_dot.get(digits..)?;
    }
    let offset_secs: i64 = if matches!(rest, "Z" | "z" | "") {
        0
    } else {
        let sign: i64 = match rest.as_bytes().first()? {
            b'+' => 1,
            b'-' => -1,
            _ => return None,
        };
        let tz = rest.get(1..)?;
        let (oh, om): (i64, i64) = if tz.len() == 5 && tz.as_bytes()[2] == b':' {
            (tz.get(0..2)?.parse().ok()?, tz.get(3..5)?.parse().ok()?)
        } else if tz.len() == 4 {
            (tz.get(0..2)?.parse().ok()?, tz.get(2..4)?.parse().ok()?)
        } else if tz.len() == 2 {
            (tz.get(0..2)?.parse().ok()?, 0)
        } else {
            return None;
        };
        if oh > 23 || om > 59 {
            return None;
        }
        sign * (oh * 3600 + om * 60)
    };
    let ts = days_from_civil(year, month, day) * 86_400 + hour * 3600 + min * 60 + sec - offset_secs;
    (ts > 0).then_some(ts as u64)
}

fn reset_ts(obj: &Value) -> Option<u64> {
    let raw = obj.get("resetsAt").or_else(|| obj.get("resets_at"))?;
    if let Some(n) = raw.as_i64() {
        return (n > 0).then_some(n as u64);
    }
    if let Some(f) = raw.as_f64() {
        if f.is_finite() && f > 0.0 {
            return Some(f.round() as u64);
        }
    }
    raw.as_str().and_then(parse_iso8601_ts)
}

/// Record a Codex `GetAccountRateLimitsResponse` result or `AccountRateLimitsUpdatedNotification` params.
pub fn observe_codex(key: &str, payload: &Value) {
    if key.is_empty() {
        return;
    }
    let snap = payload
        .get("rateLimits")
        .filter(|v| v.get("primary").is_some() || v.get("secondary").is_some())
        .or_else(|| payload.pointer("/rateLimitsByLimitId/codex"))
        .unwrap_or_else(|| payload.get("rateLimits").unwrap_or(payload));

    let mut weekly: Option<(u8, Option<u64>)> = None;
    let mut short: Option<(u8, Option<u64>)> = None;

    for (slot, default_weekly) in [("primary", false), ("secondary", true)] {
        let Some(win) = snap.get(slot).filter(|v| v.is_object()) else { continue };
        let Some(used) = win.get("usedPercent").and_then(Value::as_i64) else { continue };
        let rem = remaining_from_used_pct(used);
        let resets = reset_ts(win);
        let mins = win.get("windowDurationMins").and_then(Value::as_i64);
        let is_weekly = match mins {
            Some(m) if m >= 1440 => true,
            Some(m) if m > 0 => false,
            _ => default_weekly,
        };
        if is_weekly {
            weekly = Some((rem, resets));
        } else {
            short = Some((rem, resets));
        }
    }

    if weekly.is_none() && short.is_none() {
        return;
    }
    let mut guard = store().lock().unwrap_or_else(|e| e.into_inner());
    let entry = guard.entry(key.to_string()).or_default();
    if let Some((rem, resets)) = weekly {
        entry.weekly_remaining_pct = Some(rem);
        if resets.is_some() {
            entry.weekly_resets_at = resets;
        }
    }
    if let Some((rem, resets)) = short {
        entry.short_remaining_pct = Some(rem);
        if resets.is_some() {
            entry.short_resets_at = resets;
        }
    }
    entry.updated_at = now_secs();
}

fn claude_window_remaining(win: &Value) -> Option<u8> {
    if let Some(u) = win.get("utilization") {
        if let Some(i) = u.as_i64() {
            return Some(remaining_from_used_pct(i));
        }
        if let Some(f) = u.as_f64() {
            if f.is_finite() {
                let used = if f <= 1.0 { (f * 100.0).round() as i64 } else { f.round() as i64 };
                return Some(remaining_from_used_pct(used));
            }
        }
    }
    if win.get("status").and_then(Value::as_str) == Some("rejected") {
        return Some(0);
    }
    None
}

/// Record a Claude Code `rate_limit_event` message.
pub fn observe_claude(key: &str, message: &Value) {
    if key.is_empty() {
        return;
    }
    let info = message.get("rate_limit_info").unwrap_or(message);
    let mut weekly: Option<(u8, Option<u64>)> = None;
    let mut short: Option<(u8, Option<u64>)> = None;

    for wkey in ["seven_day", "seven_day_sonnet", "seven_day_opus"] {
        if let Some(win) = info.get(wkey).filter(|v| v.is_object()) {
            if let Some(rem) = claude_window_remaining(win) {
                weekly = Some((rem, reset_ts(win)));
                break;
            }
        }
    }
    if let Some(win) = info.get("five_hour").filter(|v| v.is_object()) {
        if let Some(rem) = claude_window_remaining(win) {
            short = Some((rem, reset_ts(win)));
        }
    }

    if let Some(kind) = info.get("rateLimitType").or_else(|| info.get("rate_limit_type")).and_then(Value::as_str) {
        if let Some(rem) = claude_window_remaining(info) {
            if kind.starts_with("seven_day") {
                weekly = Some((rem, reset_ts(info)));
            } else if kind.starts_with("five_hour") {
                short = Some((rem, reset_ts(info)));
            }
        }
    }

    if weekly.is_none() && short.is_none() {
        return;
    }
    let mut guard = store().lock().unwrap_or_else(|e| e.into_inner());
    let entry = guard.entry(key.to_string()).or_default();
    if let Some((rem, resets)) = weekly {
        entry.weekly_remaining_pct = Some(rem);
        if resets.is_some() {
            entry.weekly_resets_at = resets;
        }
    }
    if let Some((rem, resets)) = short {
        entry.short_remaining_pct = Some(rem);
        if resets.is_some() {
            entry.short_resets_at = resets;
        }
    }
    entry.updated_at = now_secs();
}

fn claude_usage_pct_remaining(win: &Value) -> Option<u8> {
    let u = win.get("utilization")?;
    if let Some(i) = u.as_i64() {
        return Some(remaining_from_used_pct(i));
    }
    if let Some(f) = u.as_f64() {
        if f.is_finite() {
            return Some(remaining_from_used_pct(f.round() as i64));
        }
    }
    None
}

/// Record a Claude Code `control_request` `get_usage` response (`rate_limits` uses `0..100` `utilization` and ISO-8601 `resets_at`).
pub fn observe_claude_usage(key: &str, payload: &Value) {
    if key.is_empty() {
        return;
    }
    let limits = payload.get("rate_limits").unwrap_or(payload);
    if !limits.is_object() {
        return;
    }
    let mut weekly: Option<(u8, Option<u64>)> = None;
    let mut short: Option<(u8, Option<u64>)> = None;

    for wkey in ["seven_day", "seven_day_sonnet", "seven_day_opus", "seven_day_oauth_apps"] {
        if let Some(win) = limits.get(wkey).filter(|v| v.is_object()) {
            if let Some(rem) = claude_usage_pct_remaining(win) {
                weekly = Some((rem, reset_ts(win)));
                break;
            }
        }
    }
    if weekly.is_none() {
        for item in limits.get("model_scoped").and_then(Value::as_array).into_iter().flatten() {
            if let Some(rem) = claude_usage_pct_remaining(item) {
                weekly = Some((rem, reset_ts(item)));
                break;
            }
        }
    }
    if let Some(win) = limits.get("five_hour").filter(|v| v.is_object()) {
        if let Some(rem) = claude_usage_pct_remaining(win) {
            short = Some((rem, reset_ts(win)));
        }
    }

    if weekly.is_none() && short.is_none() {
        return;
    }
    let mut guard = store().lock().unwrap_or_else(|e| e.into_inner());
    let entry = guard.entry(key.to_string()).or_default();
    if let Some((rem, resets)) = weekly {
        entry.weekly_remaining_pct = Some(rem);
        if resets.is_some() {
            entry.weekly_resets_at = resets;
        }
    }
    if let Some((rem, resets)) = short {
        entry.short_remaining_pct = Some(rem);
        if resets.is_some() {
            entry.short_resets_at = resets;
        }
    }
    entry.updated_at = now_secs();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_get_usage_parses_0_to_100_percent_and_iso8601_resets_at() {
        assert_eq!(parse_iso8601_ts("1970-01-01T00:00:01Z"), Some(1));
        assert_eq!(parse_iso8601_ts("2026-11-12T00:00:00.000000+00:00"), Some(1_794_441_600));
        assert_eq!(parse_iso8601_ts("2026-11-12T09:00:00+09:00"), Some(1_794_441_600));

        observe_claude_usage("ut-cusage-null", &json!({"rate_limits_available": false, "rate_limits": null}));
        assert_eq!(usage_json("ut-cusage-null"), Value::Null);

        // 0.6% used must stay ~99% remaining (never scaled by 100 as if it were a 0..1 fraction!).
        observe_claude_usage(
            "ut-cusage-1",
            &json!({
                "rate_limits_available": true,
                "subscription_type": "max",
                "rate_limits": {
                    "five_hour": {"utilization": 0.6, "resets_at": "2026-11-12T05:00:00.123456+00:00"},
                    "seven_day": {"utilization": 18.4, "resets_at": "2026-11-17T08:00:00Z"}
                }
            }),
        );
        let u = usage_json("ut-cusage-1");
        assert_eq!(u["short_remaining_pct"], json!(99));
        assert_eq!(u["weekly_remaining_pct"], json!(82));
        assert_eq!(u["short_resets_at"], json!(1_794_459_600));
        assert_eq!(u["weekly_resets_at"], json!(1_794_902_400));
        forget("ut-cusage-1");
    }

    #[test]
    fn codex_and_claude_rate_limits_merge_cleanly_while_unreported_stays_null() {
        assert_eq!(usage_json("ut-unreported"), Value::Null);
        observe_codex("ut-empty-codex", &json!({"rateLimits": {}}));
        assert_eq!(usage_json("ut-empty-codex"), Value::Null);

        observe_codex(
            "ut-codex-1",
            &json!({
                "rateLimits": {
                    "primary": {"usedPercent": 12, "windowDurationMins": 300, "resetsAt": 1794400000},
                    "secondary": {"usedPercent": 35, "windowDurationMins": 10080, "resetsAt": 1794900000}
                }
            }),
        );
        let u1 = usage_json("ut-codex-1");
        assert_eq!(u1["weekly_remaining_pct"], json!(65));
        assert_eq!(u1["short_remaining_pct"], json!(88));
        assert_eq!(u1["weekly_resets_at"], json!(1794900000));

        // Sparse rolling update (`primary` only) preserves previously observed `weekly_remaining_pct`.
        observe_codex("ut-codex-1", &json!({"rateLimits": {"primary": {"usedPercent": 25, "windowDurationMins": 300}}}));
        let u2 = usage_json("ut-codex-1");
        assert_eq!(u2["weekly_remaining_pct"], json!(65));
        assert_eq!(u2["short_remaining_pct"], json!(75));
        forget("ut-codex-1");
        assert_eq!(usage_json("ut-codex-1"), Value::Null);

        // Claude allowed event without utilization must not fabricate a percentage.
        observe_claude("ut-claude-1", &json!({"type": "rate_limit_event", "rate_limit_info": {"status": "allowed", "rateLimitType": "seven_day"}}));
        assert_eq!(usage_json("ut-claude-1"), Value::Null);

        // Flat fraction + nested windows + rejected status.
        observe_claude(
            "ut-claude-1",
            &json!({"type": "rate_limit_event", "rate_limit_info": {"status": "allowed_warning", "rateLimitType": "seven_day", "utilization": 0.86, "resetsAt": 1794950000}}),
        );
        assert_eq!(usage_json("ut-claude-1")["weekly_remaining_pct"], json!(14));
        observe_claude(
            "ut-claude-2",
            &json!({"type": "rate_limit_event", "rate_limit_info": {"seven_day": {"utilization": 0.32, "resetsAt": 1794950000}, "five_hour": {"status": "rejected"}}}),
        );
        let u3 = usage_json("ut-claude-2");
        assert_eq!(u3["weekly_remaining_pct"], json!(68));
        assert_eq!(u3["short_remaining_pct"], json!(0));
        forget("ut-claude-1");
        forget("ut-claude-2");
    }
}
