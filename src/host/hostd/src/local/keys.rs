//! The key of a model server (LLM-05): only in the OS keyring, under the attributes `service=lico`, `kind=local-llm`, `endpoint=ADDRESS:PORT`; read when the server
//! asks for it, sent only as an `Authorization` header, wiped after use. A test gives the adapter a store in memory (no test touches a real keyring).

use std::net::SocketAddr;

pub trait Keys: Send + Sync {
    /// The key stored for this endpoint, if there is one and the keyring can be asked.
    fn get(&self, addr: SocketAddr) -> Option<String>;
}

fn attributes(endpoint: &str) -> [(&str, &str); 3] {
    [("service", "lico"), ("kind", "local-llm"), ("endpoint", endpoint)]
}

/// The Secret Service through `keyring.rs`: it fails closed, so a missing or locked keyring is "no key".
pub struct Keyring;

impl Keys for Keyring {
    fn get(&self, addr: SocketAddr) -> Option<String> {
        let endpoint = addr.to_string();
        let mut store = crate::keyring::Store::open().ok()?;
        let mut secret = store.get(&attributes(&endpoint)).ok()?;
        let key = String::from_utf8(secret.clone()).ok().map(|k| k.trim().to_string()).filter(|k| !k.is_empty());
        crate::keyring::wipe_secret(&mut secret);
        key
    }
}

/// `lico local-key set ADDRESS:PORT` (the key on standard input): stores the key. Returns the exit code of the command line.
pub fn set(addr: SocketAddr, mut key: Vec<u8>) -> Result<(), crate::keyring::Error> {
    let endpoint = addr.to_string();
    let mut store = crate::keyring::Store::open()?;
    let done = store.set(&attributes(&endpoint), &format!("Lico local LLM key for {endpoint}"), &key);
    crate::keyring::wipe_secret(&mut key);
    done
}

pub fn clear(addr: SocketAddr) -> Result<(), crate::keyring::Error> {
    let endpoint = addr.to_string();
    crate::keyring::Store::open()?.delete(&attributes(&endpoint))
}
