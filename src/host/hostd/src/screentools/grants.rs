//! The yes of the person for a `capture` or a `clipboard_read` (plan M02-P04, S07a; D5): a GRANT, kept in the memory of the asking session.
//!
//! The yes does not capture and does not read: the result of those two is the person's data going to the agent, as the answer of a tool call, which a
//! notice cannot carry. So the yes only stores `(tool, target, expires)`, and the agent's NEXT call with the same canonical target takes it. A grant is
//! single use, lives `VALID_SECS`, at most `MAX_GRANTS` are held for a session (the oldest goes), and it is dropped with the session (it is a field of
//! `Pending`, which `drop_requests` replaces) and by a restart of the host (nothing is written to disk). The clock is a parameter, so a test can age one.

/// How long a grant can be taken after the yes, in seconds.
pub const VALID_SECS: u64 = 600;
/// How many grants one session holds at once.
pub const MAX_GRANTS: usize = 4;

#[derive(Debug)]
struct Grant {
    tool: &'static str,
    target: String,
    expires: u64,
}

#[derive(Debug, Default)]
pub struct Grants {
    list: Vec<Grant>,
}

impl Grants {
    /// Store a yes made at `now`. The same tool and target again is one grant (a yes is one use, never two); the oldest goes when there are `MAX_GRANTS`.
    pub fn give(&mut self, tool: &'static str, target: &str, now: u64) {
        self.list.retain(|g| g.expires > now && !(g.tool == tool && g.target == target));
        if self.list.len() >= MAX_GRANTS {
            self.list.remove(0);
        }
        self.list.push(Grant { tool, target: target.to_string(), expires: now + VALID_SECS });
    }

    /// Whether a live grant for exactly this tool and target is held (nothing is used).
    pub fn has(&self, tool: &str, target: &str, now: u64) -> bool {
        self.list.iter().any(|g| g.tool == tool && g.target == target && g.expires > now)
    }

    /// Use the grant for exactly this tool and target: true once, then false. The ones that have run out go with it.
    pub fn take(&mut self, tool: &str, target: &str, now: u64) -> bool {
        self.list.retain(|g| g.expires > now);
        match self.list.iter().position(|g| g.tool == tool && g.target == target) {
            Some(at) => {
                self.list.remove(at);
                true
            }
            None => false,
        }
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.list.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_grant_is_taken_once_and_only_for_the_tool_and_the_target_it_was_given_for() {
        let mut g = Grants::default();
        g.give("capture", "screen", 1000);
        assert!(g.has("capture", "screen", 1000));
        assert!(!g.take("capture", "region:0,0,10,10", 1000), "another region needs its own yes");
        assert!(!g.take("capture", "window:5", 1000) && !g.take("clipboard_read", "screen", 1000), "another window, another tool");
        assert!(g.take("capture", "screen", 1000));
        assert!(!g.take("capture", "screen", 1000) && !g.has("capture", "screen", 1000), "single use");
    }

    #[test]
    fn a_yes_given_twice_for_one_target_is_still_one_use() {
        let mut g = Grants::default();
        g.give("capture", "screen", 1000);
        g.give("capture", "screen", 1005);
        assert_eq!(g.len(), 1);
        assert!(g.take("capture", "screen", 1006) && !g.take("capture", "screen", 1006));
    }

    #[test]
    fn a_grant_lives_600_seconds_and_not_one_second_more() {
        let mut g = Grants::default();
        g.give("capture", "screen", 1000);
        assert!(g.has("capture", "screen", 1599) && !g.has("capture", "screen", 1600), "the last second it is alive is the 599th");
        assert!(!g.take("capture", "screen", 1600), "dead at 600");
        assert_eq!(g.len(), 0, "and gone");
        g.give("capture", "screen", 2000);
        assert!(g.take("capture", "screen", 2599), "alive at 599");
    }

    #[test]
    fn at_most_four_grants_are_held_and_the_oldest_goes() {
        let mut g = Grants::default();
        for (k, target) in ["window:1", "window:2", "window:3", "window:4", "window:5"].into_iter().enumerate() {
            g.give("capture", target, 1000 + k as u64);
        }
        assert_eq!(g.len(), MAX_GRANTS);
        assert!(!g.has("capture", "window:1", 1010), "the oldest went");
        assert!(["window:2", "window:3", "window:4", "window:5"].iter().all(|t| g.has("capture", t, 1010)));
    }
}
