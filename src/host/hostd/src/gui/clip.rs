//! The selection code of the window: the text of a selection fetched on a connection of its own, and the owner of the CLIPBOARD selection.
//! (Moved out of `ui.rs` in plan M02-P04, S04a, unchanged; the reader that also takes an image is `crate::clipboard`.)

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// The text of a selection (`CLIPBOARD` or `PRIMARY`) as UTF-8, asked for through our own helper window on a connection of its own, within 500 ms.
/// An incremental transfer or more than `limit` bytes is refused rather than cut.
pub(super) fn fetch_selection(name: &[u8], limit: usize) -> Result<String, &'static str> {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, CreateWindowAux, WindowClass};
    use x11rb::protocol::Event;
    let (conn, screen) = x11rb::connect(None).map_err(|_| "empty")?;
    let root = conn.setup().roots[screen].root;
    let win = conn.generate_id().map_err(|_| "empty")?;
    conn.create_window(0, win, root, 0, 0, 1, 1, 0, WindowClass::INPUT_ONLY, 0, &CreateWindowAux::new()).map_err(|_| "empty")?;
    let atom = |n: &[u8]| conn.intern_atom(false, n).ok().and_then(|c| c.reply().ok()).map(|r| r.atom).ok_or("empty");
    let (selection, utf8, property, incr) = (atom(name)?, atom(b"UTF8_STRING")?, atom(b"LICO_PASTE")?, atom(b"INCR")?);
    let owner = conn.get_selection_owner(selection).map_err(|_| "empty")?.reply().map_err(|_| "empty")?.owner;
    if owner == x11rb::NONE {
        return Err("empty");
    }
    let deadline = std::time::Instant::now() + Duration::from_millis(500);
    for target in [utf8, u32::from(AtomEnum::STRING)] {
        conn.convert_selection(win, selection, target, property, x11rb::CURRENT_TIME).map_err(|_| "empty")?;
        conn.flush().map_err(|_| "empty")?;
        loop {
            match conn.poll_for_event() {
                Ok(Some(Event::SelectionNotify(e))) if e.requestor == win => {
                    if e.property == x11rb::NONE {
                        break; // this form was refused: the next one
                    }
                    let reply = conn.get_property(true, win, property, AtomEnum::ANY, 0, (limit / 4 + 1) as u32).map_err(|_| "empty")?.reply().map_err(|_| "empty")?;
                    if reply.type_ == incr || reply.bytes_after > 0 || reply.value.len() > limit {
                        return Err("too_large");
                    }
                    return Ok(String::from_utf8_lossy(&reply.value).into_owned());
                }
                Ok(Some(_)) => {}
                Ok(None) => {
                    if std::time::Instant::now() > deadline {
                        return Err("timeout");
                    }
                    thread::sleep(Duration::from_millis(5));
                }
                Err(_) => return Err("empty"),
            }
        }
    }
    Err("empty")
}

/// Owns the CLIPBOARD selection with the text it was last given, until another client takes it.
pub(super) fn clipboard_server(rx: mpsc::Receiver<String>) {
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, CreateWindowAux, EventMask, PropMode, SelectionNotifyEvent, WindowClass, SELECTION_NOTIFY_EVENT};
    use x11rb::protocol::Event;
    use x11rb::wrapper::ConnectionExt as _;
    let Ok((conn, screen)) = x11rb::connect(None) else { return };
    let root = conn.setup().roots[screen].root;
    let Ok(win) = conn.generate_id() else { return };
    if conn.create_window(0, win, root, 0, 0, 1, 1, 0, WindowClass::INPUT_ONLY, 0, &CreateWindowAux::new()).is_err() {
        return;
    }
    let atom = |name: &[u8]| conn.intern_atom(false, name).ok().and_then(|c| c.reply().ok()).map(|r| r.atom);
    let (Some(clipboard), Some(utf8), Some(targets), Some(text_atom)) = (atom(b"CLIPBOARD"), atom(b"UTF8_STRING"), atom(b"TARGETS"), atom(b"TEXT")) else { return };
    let mut data: Option<Vec<u8>> = None;
    loop {
        if data.is_none() {
            match rx.recv() {
                Ok(text) => data = Some(text.into_bytes()),
                Err(_) => return,
            }
            let _ = conn.set_selection_owner(win, clipboard, x11rb::CURRENT_TIME);
            let _ = conn.flush();
        }
        while let Ok(text) = rx.try_recv() {
            data = Some(text.into_bytes());
            let _ = conn.set_selection_owner(win, clipboard, x11rb::CURRENT_TIME);
            let _ = conn.flush();
        }
        match conn.poll_for_event() {
            Ok(Some(Event::SelectionRequest(r))) => {
                let mut property = r.property;
                let body = data.clone().unwrap_or_default();
                if r.target == targets {
                    let list = [targets, utf8, AtomEnum::STRING.into(), text_atom];
                    let _ = conn.change_property32(PropMode::REPLACE, r.requestor, r.property, AtomEnum::ATOM, &list);
                } else if r.target == utf8 || r.target == text_atom || r.target == u32::from(AtomEnum::STRING) {
                    let _ = conn.change_property8(PropMode::REPLACE, r.requestor, r.property, utf8, &body);
                } else {
                    property = x11rb::NONE;
                }
                let notify = SelectionNotifyEvent { response_type: SELECTION_NOTIFY_EVENT, sequence: 0, time: r.time, requestor: r.requestor, selection: r.selection, target: r.target, property };
                let _ = conn.send_event(false, r.requestor, EventMask::NO_EVENT, notify);
                let _ = conn.flush();
            }
            Ok(Some(Event::SelectionClear(_))) => data = None, // another client owns the clipboard now
            Ok(Some(_)) => {}
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(_) => return,
        }
    }
}
