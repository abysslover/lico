//! The real-screen control warning bar and virtual-display controls in `lico gui`
//! (`screens.md` section 7; `CU-05`, `CU-06`, `SF-01`, `SF-05`).
//!
//! Whenever any session holds an active time-bounded real-screen grant (`status["real_screen"]`),
//! `lico gui` draws a crimson bar right below the menu bar:
//!   `!! REAL SCREEN: '<name>' is controlling this screen    MM:SS left   [ Stop now ]`
//! Clicking `[ Stop now ]` or pressing `Ctrl+Alt+Esc` anywhere in `lico gui` sends
//! `{"op":"real_screen.stop"}` immediately.

use cosmic_text::{Color, FontSystem, SwashCache};
use serde_json::{json, Value};
use winit::event::KeyEvent;
use winit::keyboard::{Key, NamedKey};

use super::controls::{self, Action, Controls};
use super::{clean, App, LINE_HEIGHT};

const BAR_BG: u32 = 0xFF7A_1C1C;
const BTN_BG: u32 = 0xFF3A_0E0E;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RealEntry {
    pub session: String,
    pub name: String,
    pub seconds_left: u64,
}

impl RealEntry {
    pub fn of_json(v: &Value) -> Option<RealEntry> {
        Some(RealEntry {
            session: v["session"].as_str()?.to_string(),
            name: clean(v["name"].as_str().unwrap_or("")),
            seconds_left: v["seconds_left"].as_u64()?,
        })
    }
}

pub fn format_mm_ss(secs: u64) -> String {
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

pub fn banner_text(entries: &[RealEntry]) -> Option<String> {
    let first = entries.first()?;
    let countdown = format_mm_ss(first.seconds_left);
    if entries.len() == 1 {
        Some(format!("!! REAL SCREEN: '{}' is controlling this screen   {countdown} left", first.name))
    } else {
        Some(format!(
            "!! REAL SCREEN: '{}' (+{}) controlling this screen   {countdown} left",
            first.name,
            entries.len() - 1
        ))
    }
}

/// Height of the real-screen warning bar in physical pixels (`0.0` when inactive).
pub fn height_px(entries: &[RealEntry], scale: f32) -> f32 {
    if entries.is_empty() {
        0.0
    } else {
        (LINE_HEIGHT as f32 + 4.0) * scale
    }
}

/// Draw the crimson warning bar at `[0.0, y0, width, y0 + h]` and register the `[ Stop now ]` button.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    entries: &[RealEntry], controls: &mut Controls, fs: &mut FontSystem, swash: &mut SwashCache, buf: &mut [u32], width: usize, height: usize, y0: f32,
) {
    let Some(text) = banner_text(entries) else { return };
    let s = controls.scale.max(1.0);
    let h = height_px(entries, s);
    let lh = LINE_HEIGHT as f32 * s;
    controls::fill(buf, width, height, [0.0, y0, width as f32, y0 + h], BAR_BG);
    let ink = Color::rgb(0xFF, 0xF0, 0xF0);
    let btn_label = "Stop now (Ctrl+Alt+Esc)";
    let btn_w = controls.width(fs, btn_label) + 16.0 * s;
    let btn_x0 = (width as f32 - btn_w - 6.0 * s).max(4.0 * s);
    controls.text(fs, swash, buf, width, height, 8.0 * s, y0 + 2.0 * s, (btn_x0 - 8.0 * s) as i32, &text, ink);
    controls.button(
        fs,
        swash,
        buf,
        width,
        height,
        [btn_x0, y0 + 2.0 * s, (width as f32 - 6.0 * s).max(btn_x0 + 8.0), y0 + 2.0 * s + lh],
        btn_label,
        BTN_BG,
        ink,
        Action::StopRealScreen,
    );
}

impl App {
    /// `Ctrl+Alt+Esc` stops real-screen control from anywhere in `lico gui` (`CU-06`, `SF-05`).
    pub(crate) fn real_screen_hotkey(&mut self, event: &KeyEvent) -> bool {
        if self.ui.mods.control_key() && self.ui.mods.alt_key() && event.logical_key == Key::Named(NamedKey::Escape) {
            if !event.repeat {
                self.stop_real_screen();
            }
            return true;
        }
        false
    }

    pub(crate) fn stop_real_screen(&mut self) {
        let reply = self.host_call(json!({"op": "real_screen.stop"}));
        self.ui.real_screen.clear();
        self.send("{\"op\":\"status\"}");
        if reply["ok"] == json!(1) {
            let n = reply["stopped"].as_u64().unwrap_or(0);
            self.ui.flash = Some(if n > 0 {
                "Real-screen control stopped.".into()
            } else {
                "Real-screen control was already off.".into()
            });
        } else {
            self.ui.flash = Some("Could not stop real-screen control.".into());
        }
        self.dirty = true;
    }

    pub(crate) fn virtual_display_reason(&self) -> Option<&'static str> {
        let Some(row) = self.selected_row() else { return Some("no session is open") };
        if row.state != "running" {
            Some("session is not running")
        } else {
            None
        }
    }

    pub(crate) fn toggle_virtual_display(&mut self) {
        let Some(row) = self.selected_row().cloned() else { return };
        let op = if row.virtual_display.is_empty() { "display.ensure" } else { "display.stop" };
        let reply = self.host_call(json!({"op": op, "session": row.session}));
        self.send("{\"op\":\"status\"}");
        if reply["ok"] == json!(1) {
            if op == "display.ensure" {
                let d = reply["display"].as_str().unwrap_or("");
                self.ui.flash = Some(format!("Virtual display {d} started for '{}'.", row.name()));
            } else {
                self.ui.flash = Some(format!("Virtual display stopped for '{}'.", row.name()));
            }
        } else {
            let err = reply["error"].as_str().unwrap_or("failed");
            self.ui.flash = Some(format!("Virtual display: {err}."));
        }
        self.dirty = true;
    }

    pub(crate) fn real_screen_json(&self) -> Value {
        let stop_rect = self.ui.controls.hits.iter().find(|h| h.action == Action::StopRealScreen).map(|h| h.rect);
        json!({
            "active": !self.ui.real_screen.is_empty(),
            "banner": banner_text(&self.ui.real_screen),
            "stop_rect": stop_rect,
            "entries": self.ui.real_screen.iter().map(|e| json!({"session": e.session, "name": e.name, "seconds_left": e.seconds_left})).collect::<Vec<_>>(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_shows_session_name_and_mm_ss_countdown() {
        assert_eq!(banner_text(&[]), None);
        assert_eq!(height_px(&[], 1.0), 0.0);
        let one = vec![RealEntry { session: "s1".into(), name: "ui-agent".into(), seconds_left: 899 }];
        assert_eq!(banner_text(&one).as_deref(), Some("!! REAL SCREEN: 'ui-agent' is controlling this screen   14:59 left"));
        assert!(height_px(&one, 1.0) > 20.0);
    }
}
