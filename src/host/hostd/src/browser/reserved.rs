//! The ports on this machine that no page may reach through the fetch guard (a confused deputy: the guard runs in the host process and could be asked to connect to the host's own
//! door). The set holds the ports of every engine that runs (its CDP port and its guard's port) and, through the driver port, every port the host itself listens on (the
//! loopback HTTP door). The guard asks it for a loopback destination only.

use std::collections::HashSet;
use std::sync::Mutex;

static PORTS: Mutex<Option<HashSet<u16>>> = Mutex::new(None);

pub fn add(port: u16) {
    PORTS.lock().unwrap().get_or_insert_with(HashSet::new).insert(port);
}

pub fn remove(port: u16) {
    if let Some(set) = PORTS.lock().unwrap().as_mut() {
        set.remove(&port);
    }
}

/// Whether a loopback destination on `port` is refused: an engine's port, or a port the host listens on.
pub fn denied(port: u16) -> bool {
    PORTS.lock().unwrap().as_ref().is_some_and(|s| s.contains(&port)) || crate::driver::listening().contains(&port)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_port_is_denied_from_the_moment_it_is_added_until_it_is_removed_and_the_hosts_own_listeners_always_are() {
        assert!(!denied(61001));
        add(61001);
        assert!(denied(61001) && !denied(61002));
        remove(61001);
        assert!(!denied(61001), "an engine that ended frees its numbers");
        remove(61001); // removing twice is harmless
        crate::driver::note_listening(61003);
        assert!(denied(61003), "a port the host listens on is denied without any engine");
    }
}
