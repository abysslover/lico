//! The hubs through the operations port (M17-P06): an operation that an adapter serves is validated and dispatched by the same two functions for the socket and for
//! `POST /v1`, and the host's stop reaches the adapter, proved with a dummy adapter (no engine, no browser).

use crate::driver::{self, Ops, Opaque};
use serde_json::{json, Map, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

static STOPS: AtomicUsize = AtomicUsize::new(0);

struct Echo;

impl Ops for Echo {
    fn names(&self) -> &'static [&'static str] {
        &["dummy.echo", "dummy.other"]
    }
    fn valid(&self, op: &str, request: &Map<String, Value>) -> bool {
        op == "dummy.echo" && request.get("n").is_some_and(Value::is_i64) && request.keys().all(|k| matches!(k.as_str(), "op" | "v" | "id" | "n"))
    }
    fn call(&self, shared: &Opaque, op: &str, request: &Map<String, Value>) -> Result<Map<String, Value>, &'static str> {
        assert!(shared.clone().downcast::<crate::core::Shared>().is_ok(), "the host's own type reaches the adapter");
        assert_eq!(op, "dummy.echo");
        match request["n"].as_i64() {
            Some(99) => Err("busy"),
            Some(n) => Ok(json!({"echoed": n}).as_object().cloned().unwrap()),
            None => Err("bad_request"),
        }
    }
    fn stop(&self) {
        STOPS.fetch_add(1, Ordering::SeqCst);
    }
}

fn request(v: Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap()
}

#[test]
fn an_operation_of_an_adapter_is_validated_and_dispatched_by_the_hubs_for_both_doors() {
    driver::register_ops(Arc::new(Echo));
    let dir = std::env::temp_dir().join(format!("lico_ops_hubs_{}", crate::core::random_hex(6)));
    let shared = crate::driver_hubs_tests::shared(&dir);
    assert!(crate::ipc::valid_request("dummy.echo", &request(json!({"op": "dummy.echo", "n": 3}))));
    assert!(!crate::ipc::valid_request("dummy.echo", &request(json!({"op": "dummy.echo", "n": "3"}))), "the adapter's own validation decides");
    assert!(!crate::ipc::valid_request("dummy.echo", &request(json!({"op": "dummy.echo", "n": 3, "extra": 1}))), "unknown fields are refused");
    assert!(!crate::ipc::valid_request("dummy.other", &request(json!({"op": "dummy.other"}))), "a name the adapter lists but does not accept");
    assert!(!crate::ipc::valid_request("dummy.nobody", &request(json!({"op": "dummy.nobody"}))), "a name nobody serves keeps the old answer (refused)");
    assert!(crate::ipc::valid_request("hello", &request(json!({"op": "hello"}))), "an operation of the host itself is untouched");
    for via_http in [false, true] {
        let ok = crate::ipc::dispatch_request(&shared, "dummy.echo", &request(json!({"op": "dummy.echo", "n": 7})), via_http);
        assert_eq!(ok, Some(Ok(request(json!({"echoed": 7})))), "via_http {via_http}");
        let refused = crate::ipc::dispatch_request(&shared, "dummy.echo", &request(json!({"op": "dummy.echo", "n": 99})), via_http);
        assert_eq!(refused, Some(Err("busy")), "the adapter's error code reaches the person unchanged");
    }
    assert!(crate::ipc::dispatch_request(&shared, "dummy.nobody", &request(json!({"op": "dummy.nobody"})), false).is_none());
}

#[test]
fn the_operations_of_the_browser_are_listed_so_that_capabilities_names_them() {
    for op in ["browser.open", "browser.list", "browser.close", "browser.shot"] {
        assert!(crate::core::OPS.contains(&op), "{op}");
    }
}

#[test]
fn stopping_the_host_stops_every_adapter_once() {
    driver::register_ops(Arc::new(Echo));
    let before = STOPS.load(Ordering::SeqCst);
    driver::stop_all();
    assert!(STOPS.load(Ordering::SeqCst) > before, "stop reached the adapter");
}
