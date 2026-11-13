//! Test only: a gate that lets a few heavy kinds of test run at the same time and no more. The unit tests run on every core at once; a font database costs
//! about 130 MiB per `FontSystem` and a private X server 65 to 95 MiB, so ten of them together are over a GiB. A test that needs one takes a pass first
//! and waits for its turn when the gate is full: every test still runs, on the same number of threads, and the peak memory of `cargo test` is bounded.
//! A thread that already holds a pass of a gate passes through it again (a test that starts two servers never waits for itself).

use std::cell::RefCell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};

pub struct Gate {
    slots: usize,
    inside: Mutex<usize>,
    turn: Condvar,
    most: AtomicUsize,
}

/// What a test holds while it uses the heavy thing; the slot is free again when it goes away.
pub struct Pass {
    gate: Option<&'static Gate>,
}

thread_local! {
    /// The gates (by address) this thread holds a pass of.
    static HELD: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

/// Private X servers (`Xvfb`).
pub static X_SERVERS: Gate = Gate::new(3);
/// Font databases (`FontSystem`): one at a time.
pub static FONT_SYSTEMS: Gate = Gate::new(1);

impl Gate {
    pub const fn new(slots: usize) -> Gate {
        Gate { slots, inside: Mutex::new(0), turn: Condvar::new(), most: AtomicUsize::new(0) }
    }

    /// Wait for a free slot (unless this thread holds one already) and take it.
    pub fn enter(&'static self) -> Pass {
        let me = self as *const Gate as usize;
        if HELD.with(|h| h.borrow().contains(&me)) {
            return Pass { gate: None };
        }
        let mut inside = self.inside.lock().unwrap_or_else(|e| e.into_inner());
        while *inside >= self.slots {
            inside = self.turn.wait(inside).unwrap_or_else(|e| e.into_inner());
        }
        *inside += 1;
        self.most.fetch_max(*inside, Ordering::SeqCst);
        drop(inside);
        HELD.with(|h| h.borrow_mut().push(me));
        Pass { gate: Some(self) }
    }

    /// The most passes held at one time since the program began.
    pub fn most_at_once(&self) -> usize {
        self.most.load(Ordering::SeqCst)
    }
}

impl Drop for Pass {
    fn drop(&mut self) {
        if let Some(gate) = self.gate {
            let me = gate as *const Gate as usize;
            HELD.with(|h| h.borrow_mut().retain(|g| *g != me));
            let mut inside = gate.inside.lock().unwrap_or_else(|e| e.into_inner());
            *inside -= 1;
            gate.turn.notify_one();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    static THREE: Gate = Gate::new(3);

    #[test]
    fn a_gate_lets_only_its_slots_run_at_once_every_thread_gets_its_turn_and_a_holder_passes_through_again() {
        let done = AtomicUsize::new(0);
        std::thread::scope(|s| {
            for _ in 0..12 {
                s.spawn(|| {
                    let _pass = THREE.enter();
                    let _again = THREE.enter(); // the same thread: no second slot, no wait
                    std::thread::sleep(Duration::from_millis(20));
                    done.fetch_add(1, Ordering::SeqCst);
                });
            }
        });
        assert_eq!(done.load(Ordering::SeqCst), 12, "every thread ran");
        assert!(THREE.most_at_once() <= 3, "{} held a pass at once on a gate of 3", THREE.most_at_once());
        assert!(THREE.most_at_once() >= 2, "the gate must not serialise what it may let through");
    }
}
