//! The host's tools for a local session: the same operations and approvals the other sessions have. The loop hands a call here; the tools that need no approval answer
//! at once, every other tool leaves its card on the session's row and answers that it is pending, which ends the turn (the decision comes back as a message).

use lico_llm_wire::{ToolCall, ToolDef};
use lico_toolloop::{ToolOutcome, ToolPort};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::core::Shared;
use crate::mcp::Answer;

pub struct HostTools {
    pub shared: Arc<Shared>,
    pub id: String,
}

/// The tools the host offers now (the table of `mcp.rs`, with `clipboard_write` only while writing is switched on), as definitions for a model.
pub fn tool_defs(shared: &Shared) -> Vec<ToolDef> {
    let writing = crate::clipboard::settings::clipboard_write_enabled(&crate::clipboard::settings::settings_path(&shared.cfg.handoff_dir));
    crate::mcp::list(writing)["tools"]
        .as_array()
        .map(|tools| {
            tools
                .iter()
                .filter_map(|t| Some(ToolDef { name: t["name"].as_str()?.to_string(), description: t["description"].as_str()?.to_string(), parameters: t["inputSchema"].clone() }))
                .collect()
        })
        .unwrap_or_default()
}

impl ToolPort for HostTools {
    fn call(&mut self, call: &ToolCall) -> ToolOutcome {
        let Some(arguments) = call.arguments.as_object() else { return ToolOutcome::Refused("error: the arguments must be a JSON object".into()) };
        match crate::tools::run(&self.shared, &self.id, &call.name, arguments) {
            Ok(Answer::Text(text)) => {
                let pending = serde_json::from_str::<Value>(&text).ok().is_some_and(|v| v["status"] == json!("pending"));
                if pending { ToolOutcome::Waiting(text) } else { ToolOutcome::Done(text) }
            }
            Ok(Answer::Blocks(_)) => ToolOutcome::Done("The result is a picture or another kind of block; a local session reads text only.".into()),
            Err(error) => ToolOutcome::Refused(error),
        }
    }
}
