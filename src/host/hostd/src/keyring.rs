//! A Secret Service client (the `org.freedesktop.secrets` interface) for credentials Lico owns.
//! It fails closed: if the bus, the service or the default keyring is missing or locked, nothing
//! is stored anywhere else. Secrets travel to the local service with the `plain` transfer
//! algorithm over the owner-only session bus; they are never put in an argument, a file or a log.

use crate::dbus::{byte_array, wipe, Conn, Error as BusError, Value};

const SERVICE: &str = "org.freedesktop.secrets";
const ROOT: &str = "/org/freedesktop/secrets";
const SERVICE_IFACE: &str = "org.freedesktop.Secret.Service";
const COLLECTION_IFACE: &str = "org.freedesktop.Secret.Collection";
const ITEM_IFACE: &str = "org.freedesktop.Secret.Item";

#[derive(Debug, PartialEq)]
pub enum Error {
    NoBus,
    NoService,
    NoDefault,
    Locked,
    NotFound,
    Failed(String),
}

impl Error {
    /// What the person can do about it.
    pub fn action(&self) -> String {
        match self {
            Error::NoBus => "No session bus was found: start a desktop session or a session bus (DBUS_SESSION_BUS_ADDRESS) with a Secret Service, then retry.".into(),
            Error::NoService => "No Secret Service answers on the session bus: start one (for example gnome-keyring or KeePassXC) and retry.".into(),
            Error::NoDefault => "The Secret Service has no default keyring: create one in your keyring manager (the usual name is Login) and unlock it, then retry.".into(),
            Error::Locked => "The default keyring is locked: unlock it in your keyring manager, then retry.".into(),
            Error::NotFound => "No such credential.".into(),
            Error::Failed(why) => format!("The Secret Service failed: {why}."),
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            Error::NoBus | Error::NoService | Error::NoDefault | Error::Locked => 3,
            Error::NotFound => 4,
            Error::Failed(_) => 5,
        }
    }
}

fn remote(error: BusError, locked_hint: bool) -> Error {
    match error {
        BusError::NoBus => Error::NoBus,
        BusError::Remote(name) if name.ends_with("ServiceUnknown") || name.ends_with("NameHasNoOwner") => Error::NoService,
        BusError::Remote(name) if name.ends_with("IsLocked") || locked_hint => Error::Locked,
        BusError::Remote(name) => Error::Failed(name.chars().filter(|c| c.is_ascii_graphic()).take(80).collect()), // a peer's text is shown plain and short
        BusError::Protocol(why) => Error::Failed(why.into()),
    }
}

pub struct Store {
    conn: Conn,
    session: String,
    collection: String,
}

fn attributes(pairs: &[(&str, &str)]) -> Value {
    Value::Array(
        "{ss}".into(),
        pairs.iter().map(|(k, v)| Value::DictEntry(Box::new(Value::Str((*k).into())), Box::new(Value::Str((*v).into())))).collect(),
    )
}

impl Store {
    /// Open a session with the default keyring; every way of not being able to is a distinct error.
    pub fn open() -> Result<Store, Error> {
        let mut conn = Conn::session().map_err(|e| remote(e, false))?;
        let opened = conn
            .call(SERVICE, ROOT, SERVICE_IFACE, "OpenSession", &[Value::Str("plain".into()), Value::Variant(Box::new(Value::Str(String::new())))])
            .map_err(|e| remote(e, false))?;
        let session = opened.get(1).and_then(Value::string).ok_or_else(|| Error::Failed("no session".into()))?.to_string();
        let alias = conn.call(SERVICE, ROOT, SERVICE_IFACE, "ReadAlias", &[Value::Str("default".into())]).map_err(|e| remote(e, false))?;
        let collection = alias.first().and_then(Value::string).unwrap_or("/").to_string();
        if collection == "/" {
            return Err(Error::NoDefault);
        }
        let locked = conn
            .call(SERVICE, &collection, "org.freedesktop.DBus.Properties", "Get", &[Value::Str(COLLECTION_IFACE.into()), Value::Str("Locked".into())])
            .map_err(|e| remote(e, false))?;
        if matches!(locked.first(), Some(Value::Variant(inner)) if **inner == Value::Bool(true)) {
            return Err(Error::Locked);
        }
        Ok(Store { conn, session, collection })
    }

    fn find(&mut self, pairs: &[(&str, &str)]) -> Result<Option<String>, Error> {
        let found = self.conn.call(SERVICE, &self.collection, COLLECTION_IFACE, "SearchItems", &[attributes(pairs)]).map_err(|e| remote(e, false))?;
        Ok(found.first().and_then(Value::items).and_then(|items| items.first()).and_then(Value::string).map(str::to_string))
    }

    /// Store a secret under the attributes (replacing one with the same attributes). The label is
    /// shown in keyring managers, so it must never hold the secret.
    pub fn set(&mut self, pairs: &[(&str, &str)], label: &str, secret: &[u8]) -> Result<(), Error> {
        let properties = Value::Array(
            "{sv}".into(),
            vec![
                Value::DictEntry(Box::new(Value::Str(format!("{ITEM_IFACE}.Label"))), Box::new(Value::Variant(Box::new(Value::Str(label.into()))))),
                Value::DictEntry(Box::new(Value::Str(format!("{ITEM_IFACE}.Attributes"))), Box::new(Value::Variant(Box::new(attributes(pairs))))),
            ],
        );
        let value = Value::Struct(vec![Value::Path(self.session.clone()), byte_array(&[]), byte_array(secret), Value::Str("text/plain".into())]);
        let created = self
            .conn
            .call(SERVICE, &self.collection, COLLECTION_IFACE, "CreateItem", &[properties, value, Value::Bool(true)])
            .map_err(|e| remote(e, false))?;
        match created.get(1).and_then(Value::string) {
            Some("/") => Ok(()),
            Some(_) => Err(Error::Locked), // the service wants a prompt: the keyring is not ready
            None => Err(Error::Failed("no reply".into())),
        }
    }

    /// The secret stored under the attributes.
    pub fn get(&mut self, pairs: &[(&str, &str)]) -> Result<Vec<u8>, Error> {
        let item = self.find(pairs)?.ok_or(Error::NotFound)?;
        let reply = self.conn.call(SERVICE, &item, ITEM_IFACE, "GetSecret", &[Value::Path(self.session.clone())]).map_err(|e| remote(e, false))?;
        reply.first().and_then(Value::items).and_then(|s| s.get(2)).and_then(Value::bytes).ok_or_else(|| Error::Failed("no secret".into()))
    }

    pub fn delete(&mut self, pairs: &[(&str, &str)]) -> Result<(), Error> {
        let item = self.find(pairs)?.ok_or(Error::NotFound)?;
        let reply = self.conn.call(SERVICE, &item, ITEM_IFACE, "Delete", &[]).map_err(|e| remote(e, false))?;
        match reply.first().and_then(Value::string) {
            Some("/") => Ok(()),
            _ => Err(Error::Locked),
        }
    }
}

pub fn wipe_secret(secret: &mut Vec<u8>) {
    wipe(secret);
    secret.clear();
}
