//! The MCP `computer` tool (`CU-02`..`CU-06`, `SF-01`, `SF-03`, `SF-05`, `SF-06`, `XF-07`).
//!
//! - `target = "virtual"` (default): runs against the calling session's private `Xvfb` (`vdisplay`),
//!   with no real-screen exposure and no approval prompt.
//! - `target = "real"`: OFF by default. Without a live 15-minute grant (`real`), places a host-owned
//!   approval card on the session's row and returns `status: "pending"` immediately (never holding the
//!   HTTP connection open). Once the person allows it, runs inside the time-bounded window and stops
//!   immediately on `real_screen.stop` (`Ctrl+Alt+Esc` or `[ Stop now ]` in `lico gui`).
//! - Every call writes a plain audit line (`tool=computer target=<t> session=<s> action=<a>`), never
//!   logging screen pixels or typed text (`SF-03`).

pub mod real;
pub mod xact;

use serde_json::{json, Map, Value};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::calls::{counted, window};
use crate::capture::out::{fits, AGENT_LIMIT};
use crate::clipboard::open::{open_regular, read_checked};
use crate::clipboard::png_size;
use crate::core::Shared;
use crate::helper::{run_job, JobLimits};
use crate::mcp::{Answer, Block};
use crate::pasted::base64;
use crate::pending::{self, Asked, Request};
use crate::screentools::work::{self, captures_dir};

const USAGE: &str = "bad_request: computer needs `action` (`look`, `click`, `move`, `drag`, `type`, `key`, `scroll` or `wait`), optional `target` (`virtual` default, or `real`), and the action's arguments (`x`/`y`, `from_x`/`from_y`, `button`, `text`, `key`, `direction`, `amount`, `ms`)";
const REAL_CARD: &str = "control your REAL screen (15 min): sees screen, clicks and types anywhere; Ctrl+Alt+Esc stops";
const REAL_WHO: &str = "15-minute real-screen control (sees your screen, clicks and types anywhere)";
const JOB_MAX_SECS: u64 = 8;

/// Entry point for `lico xjob computer ...`.
pub fn job_command(args: &[String]) -> ! {
    match xact::parse_args(args) {
        Err(e) => {
            println!("{}", e.to_json());
            std::process::exit(if e.reason == "usage" { 1 } else { 2 });
        }
        Ok(job) => match xact::execute(&job) {
            Ok(v) => {
                println!("{v}");
                std::process::exit(0);
            }
            Err(e) => {
                println!("{}", e.to_json());
                std::process::exit(2);
            }
        },
    }
}

fn opt_str<'a>(args: &'a Map<String, Value>, key: &str, max: usize) -> Result<Option<&'a str>, String> {
    match args.get(key) {
        None => Ok(None),
        Some(Value::String(s)) if !s.is_empty() && s.chars().count() <= max => Ok(Some(s.as_str())),
        Some(_) => Err(USAGE.to_string()),
    }
}

fn opt_int(args: &Map<String, Value>, key: &str) -> Result<Option<i64>, String> {
    match args.get(key) {
        None => Ok(None),
        Some(Value::Number(n)) => n.as_i64().ok_or_else(|| USAGE.to_string()).map(Some),
        Some(Value::String(s)) => s.parse::<i64>().map(Some).map_err(|_| USAGE.to_string()),
        Some(_) => Err(USAGE.to_string()),
    }
}

#[derive(Debug)]
struct ParsedCall {
    target: &'static str,
    action_name: &'static str,
    job_words: Vec<String>,
}

fn parse_call(arguments: &Map<String, Value>) -> Result<ParsedCall, String> {
    for key in arguments.keys() {
        if !matches!(key.as_str(), "action" | "target" | "x" | "y" | "from_x" | "from_y" | "button" | "text" | "key" | "direction" | "amount" | "ms") {
            return Err(USAGE.to_string());
        }
    }
    let target = match opt_str(arguments, "target", 16)?.unwrap_or("virtual") {
        "virtual" => "virtual",
        "real" => "real",
        _ => return Err(USAGE.to_string()),
    };
    let action_raw = opt_str(arguments, "action", 16)?.ok_or_else(|| USAGE.to_string())?;
    let action_name = match action_raw {
        "look" => "look",
        "click" => "click",
        "move" => "move",
        "drag" => "drag",
        "type" => "type",
        "key" => "key",
        "scroll" => "scroll",
        "wait" => "wait",
        _ => return Err(USAGE.to_string()),
    };
    let mut words = vec!["--action".to_string(), action_name.to_string()];
    for (k, flag) in [("x", "--x"), ("y", "--y"), ("from_x", "--from-x"), ("from_y", "--from-y"), ("amount", "--amount"), ("ms", "--ms")] {
        if let Some(v) = opt_int(arguments, k)? {
            words.extend([flag.to_string(), v.to_string()]);
        }
    }
    if let Some(b) = opt_str(arguments, "button", 16)? {
        words.extend(["--button".to_string(), b.to_string()]);
    }
    if let Some(t) = opt_str(arguments, "text", xact::MAX_TYPE_CHARS)? {
        words.extend(["--text".to_string(), t.to_string()]);
    }
    if let Some(k) = opt_str(arguments, "key", 64)? {
        words.extend(["--key".to_string(), k.to_string()]);
    }
    if let Some(d) = opt_str(arguments, "direction", 16)? {
        words.extend(["--direction".to_string(), d.to_string()]);
    }
    // Validate full action shape before touching any display or opening an approval card.
    let mut check = words.clone();
    check.extend(["--out".to_string(), "/unused".to_string()]);
    xact::parse_args(&check).map_err(|e| format!("{}: {}", if e.reason == "usage" { "bad_request" } else { e.reason }, e.detail))?;
    Ok(ParsedCall { target, action_name, job_words: words })
}

/// Audit line on host stderr (`SF-03`): names the tool, target, session, and action kind, never screen contents or typed text.
fn audit(target: &str, session: &str, action: &str) {
    eprintln!("lico-host: audit tool=computer target={target} session={session} action={action}");
}

/// Called by `calls::perform` when the person allows a pending `computer` (`target = "real"`) card.
pub fn perform(shared: &Arc<Shared>, caller: &str) -> Result<String, String> {
    if std::env::var("DISPLAY").ok().filter(|s| !s.is_empty()).is_none() {
        return Err("no_display: the host has no real DISPLAY to control".into());
    }
    let found = crate::core::find(shared, caller).ok_or_else(|| "unknown_session: session is gone".to_string())?;
    {
        let s = found.lock().unwrap();
        if s.purged || s.state != crate::session::State::Running {
            return Err("session_disconnected: session is no longer running".into());
        }
    }
    let secs = real::grant_at(caller, Instant::now(), real::window_duration());
    eprintln!("lico-host: audit tool=computer target=real session={caller} action=granted seconds={secs}");
    Ok(format!(
        "{REAL_WHO}; active for {} s — call computer with target=\"real\" now (Ctrl+Alt+Esc stops it at any time)",
        secs
    ))
}

/// MCP `computer` tool handler.
pub fn computer(shared: &Arc<Shared>, caller: &str, arguments: &Map<String, Value>) -> Result<Answer, String> {
    pending::sweep(shared, caller);
    counted(window(), caller, Instant::now())?;
    let parsed = parse_call(arguments)?;
    if parsed.target == "real" {
        if std::env::var("DISPLAY").ok().filter(|s| !s.is_empty()).is_none() {
            return Err("no_display: the host has no real DISPLAY; omit target (or set target=\"virtual\") to use your private virtual display".into());
        }
        let now = Instant::now();
        let Some(secs_left) = real::seconds_left_at(caller, now) else {
            let detail = concat!(
                "The agent in this session is asking to control your REAL screen for 15 minutes.\n\n",
                "While active, it can:\n",
                "  - see everything visible on your real desktop,\n",
                "  - move the mouse, click, drag, scroll, and type into any open window.\n\n",
                "Safety controls:\n",
                "  - A crimson banner stays at the top of Lico with a [ Stop now ] button and countdown.\n",
                "  - Pressing Ctrl+Alt+Esc in Lico (or running `lico real-screen stop`) stops it immediately.\n",
                "  - For routine browser or GUI checks, deny this and tell the agent to use its private virtual display (`target: \"virtual\"`)."
            );
            let asked = pending::ask(
                shared,
                caller,
                &Request { tool: "computer", target: "real", who: REAL_WHO, card: REAL_CARD, text: detail },
                pending::now(),
                pending::held_text(),
            )?;
            let (id, again) = match asked {
                Asked::New(id) => (id, false),
                Asked::Again(id) => (id, true),
            };
            let mut reply = json!({
                "status": "pending",
                "approval_id": id,
                "target": "real",
                "message": "Real-screen control is off until the person allows it. End your turn now; Lico will tell you here when they decide, and then repeat your `computer` call with `target: \"real\"` (or use `target: \"virtual\"` now for your private virtual display without waiting).",
            });
            if again {
                reply["already_pending"] = json!(true);
            }
            return Ok(Answer::Text(reply.to_string()));
        };
        let Some(_guard) = work::try_lock() else { return Err(work::busy()) };
        audit("real", caller, parsed.action_name);
        return run_helper(&shared.cfg.job_bin, captures_dir(&shared.cfg.handoff_dir), None, "real", parsed.action_name, &parsed.job_words, Some(secs_left));
    }
    let disp = crate::vdisplay::ensure_for_session(caller)
        .map_err(|code| format!("{code}: could not start the session's private virtual display (check that Xvfb is installed)"))?;
    let Some(_guard) = work::try_lock() else { return Err(work::busy()) };
    audit("virtual", caller, parsed.action_name);
    run_helper(&shared.cfg.job_bin, captures_dir(&shared.cfg.handoff_dir), Some(&disp), "virtual", parsed.action_name, &parsed.job_words, None)
}

fn run_helper(
    bin: &Path, dir: std::path::PathBuf, display: Option<&str>, target: &str, action: &str, job_words: &[String], secs_left: Option<u64>,
) -> Result<Answer, String> {
    let files = work::Files::new(dir)?;
    let mut words = job_words.to_vec();
    if let Some(d) = display {
        words.extend(["--display".to_string(), d.to_string()]);
    }
    words.extend(["--out".to_string(), files.full_arg()]);
    let limits = JobLimits { max_output: 64 * 1024, deadline: Duration::from_secs(JOB_MAX_SECS) };
    let line = run_job(bin, "computer", &words, &limits).map_err(|e| format!("{}: {}", e.reason, crate::leads::agent_text(&e.detail).text))?;
    let width = line["width"].as_u64().ok_or_else(|| "x_error: missing width".to_string())?;
    let height = line["height"].as_u64().ok_or_else(|| "x_error: missing height".to_string())?;
    let agent = &line["agent"];
    if agent["error"].as_str() == Some("too_large") {
        return Err(format!("too_large: the screen image is {} bytes, over {}", agent["bytes"], agent["limit"]));
    }
    let copy = if agent["path"].as_str() == line["path"].as_str() { files.full.clone() } else { files.agent() };
    let picture = open_regular(&copy, 1, AGENT_LIMIT)
        .and_then(|opened| read_checked(opened, AGENT_LIMIT))
        .map_err(|f| format!("{}: {}", f.reason(), f.detail()))?;
    let (aw, ah) = png_size(&picture).map_err(|e| format!("{}: {}", e.reason, e.detail))?;
    debug_assert!(fits(picture.len() as u64, AGENT_LIMIT));
    let mut meta = json!({
        "target": target,
        "action": action,
        "display": line["display"],
        "screen_width": width,
        "screen_height": height,
        "width": aw,
        "height": ah,
        "scale": agent["scale"].as_f64().unwrap_or(1.0),
        "coordinate_space": format!("{aw}x{ah} (pass x in 0..{}, y in 0..{}; Lico scales to {width}x{height})", aw.saturating_sub(1), ah.saturating_sub(1)),
    });
    if let (Some(sx), Some(sy)) = (line["screen_x"].as_i64(), line["screen_y"].as_i64()) {
        meta["screen_x"] = json!(sx);
        meta["screen_y"] = json!(sy);
    }
    if let Some(left) = secs_left {
        meta["seconds_left"] = json!(left);
    }
    Ok(Answer::Blocks(vec![Block::Text(meta.to_string()), Block::Image { data: base64::encode(&picture), mime: "image/png" }]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_call_defaults_to_virtual_and_rejects_unknown_fields_or_bad_actions() {
        let mut m = Map::new();
        m.insert("action".into(), json!("look"));
        let p = parse_call(&m).unwrap();
        assert_eq!((p.target, p.action_name), ("virtual", "look"));

        m.insert("target".into(), json!("real"));
        assert_eq!(parse_call(&m).unwrap().target, "real");

        m.insert("extra".into(), json!("no"));
        assert!(parse_call(&m).unwrap_err().starts_with("bad_request:"));

        let mut click = Map::new();
        click.insert("action".into(), json!("click"));
        click.insert("x".into(), json!("320"));
        click.insert("y".into(), json!(240));
        assert_eq!(parse_call(&click).unwrap().action_name, "click");
    }
}
