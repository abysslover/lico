//! The person's side of the agents' control of the panes (`M24-P05b`): the cue in a pane's header while an agent acts there, the switch that turns the control off and on, and what the stop key
//! (`Ctrl+Alt+Esc`, which also stops real-screen control) says. The host decides what is current (a touch is told for eight seconds) and sends it with each `status`, so the window keeps no clock:
//! a cue is shown until the next status that does not carry it.

use serde_json::{json, Value};

use super::App;

/// The longest name of a session in a cue, in characters.
const MAX_BY: usize = 24;

/// One agent's last act in one pane tab, as the host's `status` says (`pane_cues`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PaneCue {
    pub tab: String,
    pub by: String,
    pub did: String,
}

impl PaneCue {
    /// A cue from one entry of the host's list; an entry with no tab or no words is no cue.
    pub(crate) fn of_json(entry: &Value) -> Option<Self> {
        let words = |key: &str, most: usize| -> Option<String> {
            let text: String = entry[key].as_str()?.chars().filter(|c| !c.is_control()).take(most).collect();
            Some(text.trim().to_string()).filter(|t| !t.is_empty())
        };
        Some(Self { tab: words("tab", 16)?, by: words("by", MAX_BY)?, did: words("did", 32)? })
    }

    /// What the header says: who did what, and the one key that stops it.
    pub(crate) fn text(&self) -> String {
        format!("{} {} (Ctrl+Alt+Esc stops agents)", self.by, self.did)
    }
}

/// What the stop key did to the agents' control of the panes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PanesNow {
    Stopped,
    WasOff,
    Failed,
}

/// The notice after the stop key: what happened to real-screen control (`real`: how many grants were stopped, none when the host did not answer) and to the panes.
pub(crate) fn stop_note(real: Option<u64>, panes: PanesNow) -> String {
    let real_part = match real {
        Some(0) => "",
        Some(_) => "Real-screen control stopped.",
        None => "Could not stop real-screen control.",
    };
    let panes_part = match panes {
        PanesNow::Stopped => "Agents can no longer type or click in your panes (More > Settings turns it on again).",
        PanesNow::WasOff => "",
        PanesNow::Failed => "Could not switch the agents' control of your panes off.",
    };
    let parts: Vec<&str> = [real_part, panes_part].into_iter().filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        "Nothing to stop: real-screen control and the agents' control of your panes are off.".into()
    } else {
        parts.join(" ")
    }
}

impl App {
    /// Take the host's list of what agents did lately in the panes (`status`); a change is a reason to paint.
    pub(crate) fn sync_pane_cues(&mut self, value: &Value) {
        let next: Vec<PaneCue> = value["pane_cues"].as_array().into_iter().flatten().filter_map(PaneCue::of_json).collect();
        if next != self.ui.pane_cues {
            self.ui.pane_cues = next;
            self.dirty = true;
        }
    }

    /// The words for the header of the pane `tab` when an agent acted there lately.
    pub(crate) fn pane_cue_text(&self, tab: &str) -> Option<String> {
        self.ui.pane_cues.iter().find(|c| c.tab == tab).map(PaneCue::text)
    }

    /// `header` with the cue of `tab` after it, when there is one.
    pub(crate) fn with_pane_cue(&self, header: String, tab: &str) -> String {
        match self.pane_cue_text(tab) {
            Some(cue) => format!("{header} | {cue}"),
            None => header,
        }
    }

    /// Switch the agents' control of the panes off at the host and drop the cues; the person's stop is at once, the host reads it at each call.
    pub(crate) fn stop_agent_panes(&mut self) -> PanesNow {
        let was_on = !self.ui.agent_panes_off;
        let reply = self.host_call(json!({"op": "settings.set", "agent_panes": false}));
        self.ui.pane_cues.clear();
        if reply["ok"] != json!(1) {
            return PanesNow::Failed;
        }
        self.sync_settings(&reply["settings"]);
        if was_on { PanesNow::Stopped } else { PanesNow::WasOff }
    }

    /// The menu entry `Agent pane control`: the other way from what it is now.
    pub(crate) fn toggle_agent_panes(&mut self) {
        let next = self.ui.agent_panes_off; // off now: on next
        let reply = self.host_call(json!({"op": "settings.set", "agent_panes": next}));
        if reply["ok"] == json!(1) {
            self.sync_settings(&reply["settings"]);
            self.send("{\"op\":\"status\"}");
            self.ui.flash = Some(if self.ui.agent_panes_off {
                "Agents cannot type or click in your panes now.".into()
            } else {
                "Agents can type and click in your panes (Ctrl+Alt+Esc stops them at once).".into()
            });
        } else {
            self.ui.flash = Some("Could not change the agents' control of your panes.".into());
        }
        self.dirty = true;
    }
}
