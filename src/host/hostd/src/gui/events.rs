//! What the window says on standard output on its own. `ready` and `state` carry an id and counters. The `ime` events carry what a person typed, composed,
//! pasted-then-edited or sent, so they are printed only when a measurement rig asked for them with `LICO_GUI_EVENTS=1`: the output of the product window may
//! be kept by a launcher in a log. The other answers (`ui`, `lines`, the host status) are given to a command on standard input, so only to whoever holds it.
//! While a panel is open the input line may hold a sign-in code: nothing typed is printed then, switch or not.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

pub(super) static QUIET_INPUT: AtomicBool = AtomicBool::new(false);

fn switched_on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("LICO_GUI_EVENTS").is_some_and(|v| v == "1"))
}

pub(super) fn emit_ime(kind: &str, text: &str) {
    if QUIET_INPUT.load(Ordering::Relaxed) || !switched_on() {
        return;
    }
    println!("{{\"ev\":\"ime\",\"kind\":\"{}\",\"text\":\"{}\"}}", kind, super::json_escape(text));
    let _ = std::io::stdout().flush();
}
