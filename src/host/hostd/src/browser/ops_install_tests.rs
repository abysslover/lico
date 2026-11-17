//! The self-install as the browser manager runs it: a tab opened with no engine says it is installing, two tabs install once, a failure ends every waiting tab with the plain reason
//! (a hash that is not the pinned one as a security notice), a new tab is a new attempt, and an engine that is there costs no request. The release server is a local fake and the
//! program in place of curl a script; no real service, no real engine.

use super::installer::tests::{good, serve, source_for, Reply};
use super::ops::tests::world_with;
use super::ops::Browser;
use crate::driver::{Opaque, Ops};
use serde_json::{json, Map, Value};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;

fn req(v: Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap()
}

fn open(b: &Arc<Browser>, shared: &Opaque) -> Result<Map<String, Value>, &'static str> {
    b.call(shared, "browser.open", &req(json!({"op": "browser.open", "url": "https://a.test/"})))
}

fn rows(b: &Arc<Browser>, shared: &Opaque) -> Vec<Value> {
    b.call(shared, "browser.list", &req(json!({"op": "browser.list"}))).unwrap()["tabs"].as_array().unwrap().clone()
}

/// Waits (on the watch, no timer) until the tab is no longer installing; returns its row.
fn done(b: &Arc<Browser>, shared: &Opaque, tab: &str) -> Value {
    let mut seq = 0;
    loop {
        let r = b.call(shared, "browser.watch", &req(json!({"op": "browser.watch", "tab": tab, "seq": seq}))).unwrap();
        seq = r["seq"].as_u64().unwrap();
        if r["tab"]["phase"] != json!("installing") {
            return r["tab"].clone();
        }
    }
}

fn script(dir: &Path, name: &str, body: &str) -> std::path::PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).unwrap();
    p
}

fn left_on_disk(home: &Path) -> Vec<String> {
    let dir = home.join(".local/share/lico/browser");
    std::fs::read_dir(dir).map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default()
}

#[test]
fn a_tab_opened_with_no_engine_is_installing_and_a_wrong_hash_ends_it_as_a_security_notice_with_nothing_on_disk() {
    let fake = good("flow_a");
    let server = serve(Reply::Body(fake.bytes.clone()));
    let mut src = source_for(&fake, &server.url);
    src.archive_sha256 = "0".repeat(64);
    let (home, browser, shared) = world_with("flow_mismatch", Some(src));
    let reply = open(&browser, &shared).unwrap();
    assert_eq!(reply["phase"], json!("installing"));
    assert_eq!((reply["size_mb"].clone(), reply["sha256_start"].clone()), (json!(lico_engine::size_mb()), json!("000000000000")));
    assert!(reply["source"].as_str().unwrap().starts_with("127.0.0.1:"), "the source is named");
    assert!(reply.get("download").is_none(), "no question and no manual steps while it installs");
    let row = done(&browser, &shared, "b1");
    assert_eq!(row["phase"], json!("absent"));
    assert!(row["notice"].as_str().unwrap().starts_with("SECURITY: the engine could not be installed: the downloaded file is not the pinned release"), "{row}");
    assert_eq!(row["download"], json!(super::engine::ARCHIVE_URL), "the manual way is there");
    assert_eq!(server.hits.load(Ordering::SeqCst), 1);
    assert!(left_on_disk(&home).is_empty(), "nothing stays: {:?}", left_on_disk(&home));
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn two_tabs_opened_together_install_once_and_a_tab_opened_later_is_a_new_attempt() {
    let fake = good("flow_b");
    let (home, browser, shared) = {
        let scratch = std::env::temp_dir().join(format!("lico_flow_b_{}", crate::core::random_hex(6)));
        std::fs::create_dir_all(&scratch).unwrap();
        let count = scratch.join("count");
        let gate = scratch.join("gate");
        let curl = script(&scratch, "fake_curl.sh", &format!("echo x >> {}\nwhile [ ! -e {} ]; do sleep 0.05; done\nexit 6", count.display(), gate.display()));
        let mut src = source_for(&fake, "http://127.0.0.1:9/x");
        src.fetcher = Some(curl);
        let w = world_with("flow_once", Some(src));
        std::fs::write(w.0.join("scratch_path"), scratch.to_string_lossy().as_bytes()).unwrap();
        w
    };
    let scratch = std::path::PathBuf::from(std::fs::read_to_string(home.join("scratch_path")).unwrap());
    let (count, gate) = (scratch.join("count"), scratch.join("gate"));
    let first = open(&browser, &shared).unwrap();
    let second = open(&browser, &shared).unwrap();
    assert_eq!((first["phase"].clone(), second["phase"].clone()), (json!("installing"), json!("installing")));
    let listed = rows(&browser, &shared);
    assert!(listed.iter().all(|r| r["phase"] == json!("installing") && r["size_mb"] == json!(lico_engine::size_mb())), "the list says installing: {listed:?}");
    std::fs::write(&gate, b"go").unwrap();
    for tab in ["b1", "b2"] {
        let row = done(&browser, &shared, tab);
        assert_eq!(row["phase"], json!("absent"), "{tab}");
        assert!(row["notice"].as_str().unwrap().contains("no network"), "{row}");
        assert!(!row["notice"].as_str().unwrap().starts_with("SECURITY"), "a missing network is not a security notice");
    }
    assert_eq!(std::fs::read_to_string(&count).unwrap().lines().count(), 1, "one download for two tabs");
    // a tab opened now is a new attempt (the gate is open, so it ends at once)
    let third = open(&browser, &shared).unwrap();
    assert_eq!(third["phase"], json!("installing"));
    done(&browser, &shared, "b3");
    assert_eq!(std::fs::read_to_string(&count).unwrap().lines().count(), 2, "one attempt for each tab that is opened later, never a loop");
    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn an_engine_that_is_already_there_costs_no_request_and_no_install() {
    let fake = good("flow_c");
    let server = serve(Reply::Body(fake.bytes.clone()));
    let src = source_for(&fake, &server.url);
    let (home, browser, shared) = world_with("flow_there", Some(src));
    let dir = home.join(".local/share/lico/browser/moli-v1.1.25");
    let mut at = home.clone();
    for part in dir.strip_prefix(&home).unwrap().components() {
        at.push(part);
        std::fs::create_dir_all(&at).unwrap();
        std::fs::set_permissions(&at, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    // the file whose hash is the source's pin (a shell script is not an engine: what happens after the check is not this test's business)
    std::fs::write(dir.join("moli"), b"#!/bin/sh\necho engine\n").unwrap();
    std::fs::set_permissions(dir.join("moli"), std::fs::Permissions::from_mode(0o700)).unwrap();
    let r = open(&browser, &shared);
    assert_eq!(server.hits.load(Ordering::SeqCst), 0, "no request to the release server");
    if let Ok(reply) = r {
        assert_ne!(reply["phase"], json!("installing"));
    }
    let _ = std::fs::remove_dir_all(&home);
}
