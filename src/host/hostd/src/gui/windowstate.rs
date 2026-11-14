//! The window's maximized state (`M14-P02`): asking for it at the first start in the way a window manager is bound to read it, and the in-window icon that switches between
//! maximized and windowed. On X11 a window that is not mapped yet states what it wants in the `_NET_WM_STATE` property (the standard's way: a window manager reads it when it takes
//! the window); the client message that winit sends right after the map can arrive before the window manager has taken the window and is then lost (the owner's mutter dropped it:
//! no maximized bit, the size as requested). So the window is created hidden, the property is set, and only then is it shown. The icon sits at the right end of the menu row, draws
//! the state the window is in (one square: click to maximize; two squares: click to restore), and goes through the ordinary click and pointer path: it draws again only when the pointer
//! enters or leaves it or the state changes.

use serde_json::{json, Value};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

use super::controls::{self, Action, Controls, Rect};
use super::App;

/// Sets `_NET_WM_STATE` = maximized (both atoms) on a window that is not mapped yet. True when it was set (X11 only: on Wayland the request of the window attributes stands).
pub(super) fn set_initial_maximized(window: &Window) -> bool {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _, PropMode};
    use x11rb::wrapper::ConnectionExt as _;
    let id = match window.window_handle().ok().map(|h| h.as_raw()) {
        Some(RawWindowHandle::Xlib(h)) => h.window as u32,
        Some(RawWindowHandle::Xcb(h)) => h.window.get(),
        _ => return false,
    };
    let Ok((conn, _)) = x11rb::connect(None) else { return false };
    let atom = |name: &[u8]| conn.intern_atom(false, name).ok()?.reply().ok().map(|r| r.atom);
    let (Some(state), Some(vert), Some(horz)) = (atom(b"_NET_WM_STATE"), atom(b"_NET_WM_STATE_MAXIMIZED_VERT"), atom(b"_NET_WM_STATE_MAXIMIZED_HORZ")) else { return false };
    let sent = conn.change_property32(PropMode::REPLACE, id, state, AtomEnum::ATOM, &[vert, horz]).is_ok() && conn.flush().is_ok();
    // a round trip: the server has the property before this returns, and so before the window is shown on the other connection (requests of two connections have no order)
    sent && conn.get_input_focus().ok().and_then(|c| c.reply().ok()).is_some()
}

#[derive(Default)]
pub(crate) struct WindowBtn {
    pub(super) rect: Option<Rect>,
    pub(super) hover: bool,
}

/// The icon's place: a square at the right end of the menu row.
pub(super) fn button_rect(width: usize, menu_h: f32, s: f32) -> Rect {
    let side = (menu_h - 6.0 * s).max(10.0);
    [width as f32 - side - 6.0 * s, 3.0 * s, width as f32 - 6.0 * s, 3.0 * s + side]
}

fn outline(buf: &mut [u32], width: usize, height: usize, r: Rect, t: f32, color: u32) {
    controls::fill(buf, width, height, [r[0], r[1], r[2], r[1] + t], color);
    controls::fill(buf, width, height, [r[0], r[3] - t, r[2], r[3]], color);
    controls::fill(buf, width, height, [r[0], r[1], r[0] + t, r[3]], color);
    controls::fill(buf, width, height, [r[2] - t, r[1], r[2], r[3]], color);
}

/// Draws the icon in `r`: one square when the window is windowed, two overlapping squares when it is maximized.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_icon(controls: &mut Controls, buf: &mut [u32], width: usize, height: usize, r: Rect, maximized: bool, hover: bool, s: f32) {
    let (bg, ink) = (if hover { 0xFF2C_4A78 } else { 0xFF1C_2430 }, 0xFFE0_E0E0);
    controls::fill(buf, width, height, r, bg);
    let t = s.round().max(1.0);
    let side = r[2] - r[0];
    let (inset, size) = (side * 0.27, side * 0.46);
    if maximized {
        let off = (side * 0.14).round().max(2.0);
        let back = [r[0] + inset + off, r[1] + inset - off, r[0] + inset + off + size, r[1] + inset - off + size];
        let front = [r[0] + inset, r[1] + inset + off, r[0] + inset + size, r[1] + inset + off + size];
        outline(buf, width, height, back, t, ink);
        controls::fill(buf, width, height, front, bg);
        outline(buf, width, height, front, t, ink);
    } else {
        outline(buf, width, height, [r[0] + inset, r[1] + inset, r[0] + inset + size, r[1] + inset + size], t, ink);
    }
    controls.target(r, Action::WindowMax);
}

impl App {
    pub(crate) fn window_is_maximized(&self) -> bool {
        self.window.as_ref().is_some_and(|w| w.is_maximized())
    }

    /// The pointer moved: true when it entered or left the icon, so that the icon is drawn again.
    pub(crate) fn window_btn_hover_moved(&mut self) -> bool {
        let (x, y) = (self.ui.mouse.0 as f32, self.ui.mouse.1 as f32);
        let over = self.ui.win_btn.rect.is_some_and(|r| x >= r[0] && x < r[2] && y >= r[1] && y < r[3]);
        let changed = over != self.ui.win_btn.hover;
        self.ui.win_btn.hover = over;
        changed
    }

    /// A click on the icon: maximized -> windowed, windowed -> maximized.
    pub(crate) fn window_btn_click(&mut self, action: &Action) -> bool {
        if *action != Action::WindowMax {
            return false;
        }
        let on = self.window_is_maximized();
        if let Some(w) = &self.window {
            w.set_maximized(!on);
        }
        self.dirty = true;
        true
    }

    pub(super) fn window_btn_json(&self) -> Value {
        let maximized = self.window_is_maximized();
        json!({"maximized": maximized, "label": if maximized { "Restore window" } else { "Maximize window" }, "hover": self.ui.win_btn.hover, "rect": self.ui.win_btn.rect})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_is_a_square_inside_the_menu_row_at_its_right_end() {
        for s in [1.0, 2.0] {
            let menu_h = 22.0 * s;
            let r = button_rect(1200, menu_h, s);
            assert!(r[2] <= 1200.0 && r[2] >= 1200.0 - 8.0 * s && r[1] >= 0.0 && r[3] <= menu_h, "{r:?}");
            assert!(((r[2] - r[0]) - (r[3] - r[1])).abs() < 0.01, "a square: {r:?}");
            assert!(r[2] - r[0] >= 10.0);
        }
        assert!(button_rect(300, 22.0, 1.0)[0] > 200.0, "narrow window: still at the right end, clear of the menu titles");
    }
}
