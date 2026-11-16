//! `browser.watch` (M17-P07): the window's one blocking call for a shown tab. The host holds it on a condition variable (no timer) until the tab has changed, then answers with
//! the tab's row and, when the window's frame is older, the newest compressed frame. The window asks again only after it has painted, so a frame that is newer than the painter
//! can use is never sent: the host keeps only the newest one. At most one watch waits for a tab (a newer watch supersedes the older), and a close, a hide and a stop wake and
//! release every watch of the tab.

use super::supervisor::Hub;
use serde_json::Value;

#[derive(Debug, PartialEq)]
pub enum Watch {
    /// The tab is gone (closed, or the host is stopping).
    Gone,
    /// A newer watch of the same tab took over.
    Superseded,
    /// The tab is hidden: nothing is watched until it is shown again.
    Hidden,
    /// What changed: the change number, the tab's row, and the newest frame when it is newer than the window's (its number and its compressed bytes).
    Now { seq: u64, row: Value, frame: Option<(u64, Vec<u8>)> },
}

/// Waits (with no timeout) until tab `tab` has a change number above `seq`, or a frame above `frame_seq` when `want_frame`; see the head of this file.
pub fn watch(hub: &Hub, tab: &str, seq: u64, frame_seq: u64, want_frame: bool, row: &dyn Fn(&super::supervisor::Entry) -> Value) -> Watch {
    let mut tabs = hub.tabs.lock().unwrap();
    let mine = match tabs.iter_mut().find(|e| e.id == tab) {
        Some(e) => {
            e.watch_gen += 1;
            e.watch_gen
        }
        None => return Watch::Gone,
    };
    hub.changed.notify_all(); // the watch that was waiting wakes and sees that it is superseded
    loop {
        let Some(e) = tabs.iter().find(|e| e.id == tab) else { return Watch::Gone };
        if e.watch_gen != mine {
            return Watch::Superseded;
        }
        if !e.tab.shown() {
            return Watch::Hidden;
        }
        let newer_frame = want_frame && e.tab.frame_seq() > frame_seq;
        if e.change > seq || newer_frame {
            let frame = if newer_frame { e.tab.newest_frame().map(|b| (e.tab.frame_seq(), b.to_vec())) } else { None };
            return Watch::Now { seq: e.change, row: row(e), frame };
        }
        tabs = hub.changed.wait(tabs).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::super::supervisor::Entry;
    use super::*;
    use lico_browser_view::{EngineEvent, Tab};
    use serde_json::json;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    fn row(e: &Entry) -> Value {
        json!({"tab": e.id, "change": e.change})
    }

    fn hub_with(ids: &[&str]) -> Arc<Hub> {
        let hub = Arc::new(Hub::new());
        for id in ids {
            let mut tab = Tab::new();
            tab.show((200, 100), 0);
            hub.tabs.lock().unwrap().push(Entry::new(id.to_string(), tab, 0, false));
        }
        hub
    }

    fn change(hub: &Hub, id: &str, f: impl FnOnce(&mut Tab)) {
        let mut tabs = hub.tabs.lock().unwrap();
        let e = tabs.iter_mut().find(|e| e.id == id).unwrap();
        f(&mut e.tab);
        hub.touch(e);
    }

    fn spawn_watch(hub: &Arc<Hub>, tab: &str, seq: u64, frame_seq: u64, frame: bool) -> std::thread::JoinHandle<Watch> {
        let (hub, tab) = (hub.clone(), tab.to_string());
        std::thread::spawn(move || watch(&hub, &tab, seq, frame_seq, frame, &row))
    }

    fn settle() {
        std::thread::sleep(Duration::from_millis(100));
    }

    #[test]
    fn it_answers_at_once_when_the_tab_is_already_ahead_and_waits_otherwise() {
        let hub = hub_with(&["b1"]);
        match watch(&hub, "b1", 0, 0, false, &row) {
            Watch::Now { seq, frame, .. } => assert_eq!((seq, frame), (1, None)),
            other => panic!("{other:?}"),
        }
        let waiting = spawn_watch(&hub, "b1", 1, 0, false);
        settle();
        assert!(!waiting.is_finished(), "nothing changed: it waits, with no timeout");
        change(&hub, "b1", |t| {
            t.engine_event(EngineEvent::Title("T".into()));
        });
        match waiting.join().unwrap() {
            Watch::Now { seq, row, .. } => assert_eq!((seq, row["change"].clone()), (2, json!(2))),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_frame_comes_only_when_it_is_newer_than_the_windows_and_never_an_older_one() {
        let hub = hub_with(&["b1"]);
        change(&hub, "b1", |t| {
            t.engine_event(EngineEvent::Frame { id: 1, png: vec![1, 2, 3] });
        });
        let Watch::Now { frame, .. } = watch(&hub, "b1", 0, 0, true, &row) else { panic!() };
        assert_eq!(frame, Some((1, vec![1, 2, 3])));
        let Watch::Now { frame, .. } = watch(&hub, "b1", 0, 1, true, &row) else { panic!() };
        assert_eq!(frame, None, "the window has it: not sent again");
        let Watch::Now { frame, .. } = watch(&hub, "b1", 0, 0, false, &row) else { panic!() };
        assert_eq!(frame, None, "not asked for");
        // two frames arrive while nobody watches: the newest is sent, the older is not
        for (id, byte) in [(2, 9u8), (3, 7u8)] {
            change(&hub, "b1", |t| {
                t.engine_event(EngineEvent::Frame { id, png: vec![byte; 4] });
            });
        }
        let Watch::Now { frame, seq, .. } = watch(&hub, "b1", 3, 1, true, &row) else { panic!() };
        assert_eq!(frame, Some((3, vec![7; 4])), "only the newest");
        assert!(seq >= 4);
    }

    #[test]
    fn a_frame_alone_wakes_a_watcher_that_asked_for_frames() {
        let hub = hub_with(&["b1"]);
        let Watch::Now { seq, .. } = watch(&hub, "b1", 0, 0, true, &row) else { panic!() };
        let waiting = spawn_watch(&hub, "b1", seq, 0, true);
        settle();
        assert!(!waiting.is_finished());
        change(&hub, "b1", |t| {
            t.engine_event(EngineEvent::Frame { id: 1, png: vec![5] });
        });
        let Watch::Now { frame, .. } = waiting.join().unwrap() else { panic!() };
        assert_eq!(frame, Some((1, vec![5])));
    }

    #[test]
    fn a_newer_watch_supersedes_the_older_and_only_one_waits() {
        let hub = hub_with(&["b1", "b2"]);
        let first = spawn_watch(&hub, "b1", 1, 0, false);
        settle();
        let second = spawn_watch(&hub, "b1", 1, 0, false);
        assert_eq!(first.join().unwrap(), Watch::Superseded, "the older one is released at once");
        settle();
        assert!(!second.is_finished());
        // another tab's watch is not touched
        let other = spawn_watch(&hub, "b2", 1, 0, false);
        settle();
        assert!(!second.is_finished() && !other.is_finished());
        change(&hub, "b1", |_| {});
        assert!(matches!(second.join().unwrap(), Watch::Now { .. }));
        change(&hub, "b2", |_| {});
        assert!(matches!(other.join().unwrap(), Watch::Now { .. }));
    }

    #[test]
    fn a_close_a_hide_and_a_stop_release_the_watchers() {
        let hub = hub_with(&["b1", "b2", "b3"]);
        let (w1, w2, w3) = (spawn_watch(&hub, "b1", 1, 0, false), spawn_watch(&hub, "b2", 1, 0, false), spawn_watch(&hub, "b3", 1, 0, false));
        settle();
        {
            let mut tabs = hub.tabs.lock().unwrap();
            tabs.retain(|e| e.id != "b1");
            let e = tabs.iter_mut().find(|e| e.id == "b2").unwrap();
            e.tab.hide();
            hub.touch(e);
        }
        assert_eq!(w1.join().unwrap(), Watch::Gone, "closed");
        assert_eq!(w2.join().unwrap(), Watch::Hidden, "hidden");
        hub.tabs.lock().unwrap().clear();
        hub.changed.notify_all();
        assert_eq!(w3.join().unwrap(), Watch::Gone, "the host stops");
        assert_eq!(watch(&hub, "nosuch", 0, 0, false, &row), Watch::Gone);
    }

    #[test]
    fn a_tab_that_is_not_shown_is_not_watched() {
        let hub = hub_with(&["b1"]);
        change(&hub, "b1", |t| {
            t.hide();
        });
        let t = Instant::now();
        assert_eq!(watch(&hub, "b1", 0, 0, false, &row), Watch::Hidden);
        assert!(t.elapsed() < Duration::from_secs(1), "it does not wait");
    }
}
