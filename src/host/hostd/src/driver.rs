//! The driver port (M16-P06): how a kind of session that is not a provider program (today: `local`, a model server on this machine) plugs into the host without the
//! hubs (`core`, `session`, `folders`, ...) naming it. The hubs ask this module for the driver of a session's kind; the driver registers itself once, at start-up, from
//! the root. This module depends on nothing of the program (it names no module path of the program): the host's own types reach a driver as `Opaque` handles that the driver, which
//! knows them, downcasts. Keeping the port this small is what keeps the driver's own dependencies on the hubs out of the cycle the hubs form.

use serde_json::{Map, Value};
use std::any::Any;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

/// The host's `Arc<Shared>` and `Arc<Mutex<Session>>`, type-erased.
pub type Opaque = Arc<dyn Any + Send + Sync>;

/// The wire format of a model server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Api {
    OpenAi,
    Ollama,
}

/// Where a local session's model is: a loopback address, the format, the model, the window the person chose (0: not chosen).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub addr: SocketAddr,
    pub api: Api,
    pub model: String,
    pub context_tokens: u32,
}

/// What a session of a driver kind holds while it is alive.
pub trait Link: Send {
    /// Queues one input of the person (or a notice of the host); never blocks and never takes the session's lock, which the caller holds.
    fn send_input(&mut self, request_id: &str, text: &str) -> Result<(), &'static str>;
    /// Ends the work (a closed session, a stopping host); what the session keeps on disk stays.
    fn stop(&mut self);
    /// Removes what the driver keeps for the session on disk (the session is deleted).
    fn purge(&mut self);
    /// Whether the work is still there: a driver whose program ended before the host kept the link says no, and the host then does not take the session for a running one.
    fn alive(&self) -> bool {
        true
    }
}

pub trait Driver: Send + Sync {
    /// The `kind` of `session.create` this driver serves.
    fn kind(&self) -> &'static str;
    /// Whether the person's choices are enough to make a session (the code is the answer of `session.create`).
    fn accepts(&self, target: Option<&Target>) -> Result<(), &'static str>;
    /// Whether a session of this kind runs in a folder, as a provider session does: the one the person chose, else a scratch workspace of its own. Then it takes no target and no model.
    fn takes_folder(&self) -> bool {
        false
    }
    /// Starts the session's work (also after a restart or a resume); `shared` and `session` are the host's `Arc<Shared>` and `Arc<Mutex<Session>>`.
    fn start(&self, shared: &Opaque, session: &Opaque) -> Result<Box<dyn Link>, &'static str>;
}

/// `ADDRESS:PORT` with a loopback address (`127.0.0.1`, `[::1]`) and a port from 1 to 65535: nothing else is an endpoint of a local session.
pub fn parse_endpoint(text: &str) -> Option<SocketAddr> {
    if text.len() > 64 || !text.is_ascii() {
        return None;
    }
    let addr: SocketAddr = text.parse().ok()?;
    (addr.ip().is_loopback() && addr.port() != 0).then_some(addr)
}

/// `openai` or `ollama`.
pub fn parse_api(text: &str) -> Option<Api> {
    match text {
        "openai" => Some(Api::OpenAi),
        "ollama" => Some(Api::Ollama),
        _ => None,
    }
}

impl Api {
    pub fn word(self) -> &'static str {
        match self {
            Api::OpenAi => "openai",
            Api::Ollama => "ollama",
        }
    }
}

static DRIVERS: Mutex<Vec<Arc<dyn Driver>>> = Mutex::new(Vec::new());

/// Makes `driver` the one of its kind (a second registration of a kind replaces the first).
pub fn register(driver: Arc<dyn Driver>) {
    let mut all = DRIVERS.lock().unwrap();
    all.retain(|d| d.kind() != driver.kind());
    all.push(driver);
}

pub fn get(kind: &str) -> Option<Arc<dyn Driver>> {
    DRIVERS.lock().unwrap().iter().find(|d| d.kind() == kind).cloned()
}

/// Operations that an adapter serves besides its sessions (the browser's `browser.open` and the like): the hubs hand a request whose name an adapter lists to the adapter, for
/// the socket and for `POST /v1` alike, and tell it when the host stops. The port names no operation itself.
pub trait Ops: Send + Sync {
    /// Every operation name this adapter serves.
    fn names(&self) -> &'static [&'static str];
    /// Whether `request` is a well-formed `op` (unknown fields are not): the one check both doors use.
    fn valid(&self, op: &str, request: &Map<String, Value>) -> bool;
    /// Runs a valid `op`; `shared` is the host's `Arc<Shared>`. The error is a code of the protocol.
    fn call(&self, shared: &Opaque, op: &str, request: &Map<String, Value>) -> Result<Map<String, Value>, &'static str>;
    /// The host is stopping: end whatever this adapter runs (processes, threads, files it made).
    fn stop(&self);
}

static OPS: Mutex<Vec<Arc<dyn Ops>>> = Mutex::new(Vec::new());

/// Registers an adapter's operations (a second registration that lists a name replaces the first one that listed it).
pub fn register_ops(ops: Arc<dyn Ops>) {
    let mut all = OPS.lock().unwrap();
    all.retain(|o| !o.names().iter().any(|n| ops.names().contains(n)));
    all.push(ops);
}

fn serving(op: &str) -> Option<Arc<dyn Ops>> {
    OPS.lock().unwrap().iter().find(|o| o.names().contains(&op)).cloned()
}

/// `Some(valid)` when an adapter serves `op`, `None` when none does.
pub fn ops_valid(op: &str, request: &Map<String, Value>) -> Option<bool> {
    serving(op).map(|o| o.valid(op, request))
}

/// `Some(reply)` when an adapter serves `op`, `None` when none does.
pub fn ops_call(shared: &Opaque, op: &str, request: &Map<String, Value>) -> Option<Result<Map<String, Value>, &'static str>> {
    serving(op).map(|o| o.call(shared, op, request))
}

static LISTENING: Mutex<Vec<u16>> = Mutex::new(Vec::new());

/// The host tells the port which TCP port it listens on (its loopback HTTP door): an adapter that lets a page reach loopback must never let it reach these.
pub fn note_listening(port: u16) {
    let mut all = LISTENING.lock().unwrap();
    if !all.contains(&port) {
        all.push(port);
    }
}

/// The TCP ports this host listens on.
pub fn listening() -> Vec<u16> {
    LISTENING.lock().unwrap().clone()
}

/// The host is stopping: every adapter is told (the list is copied first, so that an adapter may register or take its time without holding the port's lock).
pub fn stop_all() {
    let all: Vec<Arc<dyn Ops>> = OPS.lock().unwrap().clone();
    for ops in all {
        ops.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dummy(&'static str);

    struct Idle;

    impl Link for Idle {
        fn send_input(&mut self, _request_id: &str, _text: &str) -> Result<(), &'static str> {
            Ok(())
        }
        fn stop(&mut self) {}
        fn purge(&mut self) {}
    }

    impl Driver for Dummy {
        fn kind(&self) -> &'static str {
            self.0
        }
        fn accepts(&self, _target: Option<&Target>) -> Result<(), &'static str> {
            Ok(())
        }
        fn start(&self, _shared: &Opaque, _session: &Opaque) -> Result<Box<dyn Link>, &'static str> {
            Ok(Box::new(Idle))
        }
    }

    #[test]
    fn a_registered_kind_is_found_a_second_registration_replaces_it_and_another_kind_is_not_found() {
        register(Arc::new(Dummy("port-test-a")));
        assert_eq!(get("port-test-a").map(|d| d.kind()), Some("port-test-a"));
        assert!(get("port-test-b").is_none() && get("").is_none() && get("claude").is_none() && get("codex").is_none());
        let first = get("port-test-a").unwrap();
        register(Arc::new(Dummy("port-test-a")));
        let second = get("port-test-a").unwrap();
        assert!(!Arc::ptr_eq(&first, &second), "the second registration replaced the first");
        assert_eq!(DRIVERS.lock().unwrap().iter().filter(|d| d.kind() == "port-test-a").count(), 1);
        let started = second.start(&(Arc::new(0u8) as Opaque), &(Arc::new(0u8) as Opaque));
        assert!(started.is_ok());
    }

    /// A driver that says nothing of a folder takes none, and a link that says nothing of its process is alive: the older kinds are unchanged.
    #[test]
    fn a_driver_takes_no_folder_and_a_link_is_alive_unless_it_says_otherwise() {
        assert!(!Dummy("port-test-c").takes_folder());
        assert!(Idle.alive());
    }

    #[test]
    fn an_endpoint_is_a_loopback_address_with_a_port_and_nothing_else() {
        for ok in ["127.0.0.1:11434", "127.0.0.1:1", "127.8.9.10:65535", "[::1]:8080"] {
            assert!(parse_endpoint(ok).is_some(), "{ok}");
        }
        for bad in ["", "localhost:11434", "0.0.0.0:80", "192.168.1.5:80", "10.0.0.1:80", "8.8.8.8:443", "[::]:80", "[fe80::1]:80", "127.0.0.1", "127.0.0.1:0", "127.0.0.1:70000", "127.0.0.1:80 ", " 127.0.0.1:80", "http://127.0.0.1:80", "127.0.0.1:80/x", "\u{ff11}27.0.0.1:80", &"1".repeat(80)] {
            assert!(parse_endpoint(bad).is_none(), "{bad:?}");
        }
        assert_eq!((parse_api("openai"), parse_api("ollama"), parse_api("OpenAI"), parse_api("")), (Some(Api::OpenAi), Some(Api::Ollama), None, None));
        assert_eq!((Api::OpenAi.word(), Api::Ollama.word()), ("openai", "ollama"));
    }

    /// The port names nothing of the program: no path into another module, so it can sit below the hubs and the adapter can sit above them.
    #[test]
    fn the_port_names_no_other_module_of_the_program() {
        let source = include_str!("driver.rs");
        let code: String = source.split("#[cfg(test)]").next().unwrap().lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
        let (own, parent) = (["crate", "::"].concat(), ["super", "::"].concat());
        assert!(!code.contains(&own) && !code.contains(&parent), "the driver port must depend on nothing of the program");
    }
}
