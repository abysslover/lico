//! Accounts: stable local ids, editable labels, a default per provider, and one private home per
//! account so that two accounts of one provider never share a login file, a settings directory or
//! a process environment. Nothing here is a secret: a provider keeps its own login inside its
//! account home (the owner signs in there with the provider's own command) and Lico never reads it.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

pub const MAX_ACCOUNTS: usize = 64;

#[derive(Clone)]
pub struct Account {
    pub id: String,
    pub provider: String,
    pub label: String,
    pub credential_generation: u64,
    pub credential_ref: Option<String>, // an opaque reference into the credential store, never a secret
}

pub struct Homes {
    pub home: PathBuf,
    pub provider_home: PathBuf,
    pub variable: &'static str,
}

pub struct Accounts {
    dir: PathBuf,
    list: Vec<Account>,
    defaults: HashMap<String, String>,
}

pub fn provider_variable(provider: &str) -> Option<&'static str> {
    match provider {
        "codex" => Some("CODEX_HOME"),
        "claude" => Some("CLAUDE_CONFIG_DIR"),
        _ => None,
    }
}

impl Accounts {
    pub fn load(dir: &Path) -> Accounts {
        let _ = std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir);
        let mut accounts = Accounts { dir: dir.to_path_buf(), list: Vec::new(), defaults: HashMap::new() };
        if let Ok(text) = std::fs::read_to_string(dir.join("accounts.json")) {
            if let Ok(value) = serde_json::from_str::<Value>(&text) {
                for entry in value["accounts"].as_array().into_iter().flatten() {
                    if let (Some(id), Some(provider), Some(label)) = (entry["account"].as_str(), entry["provider"].as_str(), entry["label"].as_str()) {
                        accounts.list.push(Account { id: id.into(), provider: provider.into(), label: label.into(), credential_generation: entry["credential_generation"].as_u64().unwrap_or(1), credential_ref: entry["credential_ref"].as_str().map(str::to_string) });
                    }
                }
                for (provider, id) in value["defaults"].as_object().into_iter().flatten() {
                    if let Some(id) = id.as_str() {
                        accounts.defaults.insert(provider.clone(), id.into());
                    }
                }
            }
        }
        accounts
    }

    fn save(&self) -> std::io::Result<()> {
        let value = json!({
            "accounts": self.list.iter().map(|a| json!({"account": a.id, "provider": a.provider, "label": a.label, "credential_generation": a.credential_generation, "credential_ref": a.credential_ref})).collect::<Vec<_>>(),
            "defaults": self.defaults,
        });
        let temporary = self.dir.join(format!(".accounts.{}.tmp", std::process::id()));
        let mut out = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&temporary)?;
        out.write_all(serde_json::to_string_pretty(&value).map_err(std::io::Error::other)?.as_bytes())?;
        out.sync_all()?;
        std::fs::rename(&temporary, self.dir.join("accounts.json"))
    }

    pub fn create(&mut self, provider: &str, label: &str) -> Result<Account, &'static str> {
        provider_variable(provider).ok_or("bad_request")?;
        if self.list.len() >= MAX_ACCOUNTS {
            return Err("busy");
        }
        let id = format!("a{}", crate::core::random_hex(6));
        let homes = self.homes_of(&id, provider).ok_or("bad_request")?;
        for dir in [&self.dir.join(&id), &homes.home, &homes.provider_home] {
            std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir).map_err(|_| "busy")?;
        }
        let account = Account { id: id.clone(), provider: provider.into(), label: label.into(), credential_generation: 1, credential_ref: None };
        self.list.push(account.clone());
        self.defaults.entry(provider.to_string()).or_insert_with(|| id.clone()); // a provider's first account is its default
        if self.save().is_err() {
            self.list.pop();
            return Err("busy");
        }
        Ok(account)
    }

    pub fn get(&self, id: &str) -> Option<&Account> {
        self.list.iter().find(|a| a.id == id)
    }

    pub fn default_for(&self, provider: &str) -> Option<&Account> {
        self.defaults.get(provider).and_then(|id| self.get(id))
    }

    pub fn set_default(&mut self, id: &str) -> Result<(), &'static str> {
        let provider = self.get(id).ok_or("unknown_account")?.provider.clone();
        self.defaults.insert(provider, id.to_string());
        self.save().map_err(|_| "busy")
    }

    pub fn credential_ref(&self, id: &str) -> Option<String> {
        self.get(id).and_then(|a| a.credential_ref.clone())
    }

    /// Attach (or replace) the credential of an account; its credential generation rises.
    pub fn set_credential(&mut self, id: &str, reference: &str) -> Result<(), &'static str> {
        let account = self.list.iter_mut().find(|a| a.id == id).ok_or("unknown_account")?;
        account.credential_ref = Some(reference.to_string());
        account.credential_generation += 1;
        self.save().map_err(|_| "busy")
    }

    pub fn entries(&self) -> Vec<Value> {
        self.list
            .iter()
            .map(|a| json!({"account": a.id, "provider": a.provider, "label": a.label, "default": self.defaults.get(&a.provider) == Some(&a.id), "credential_generation": a.credential_generation, "has_credential": a.credential_ref.is_some()}))
            .collect()
    }

    fn homes_of(&self, id: &str, provider: &str) -> Option<Homes> {
        let variable = provider_variable(provider)?;
        let root = self.dir.join(id);
        Some(Homes { home: root.join("home"), provider_home: root.join(provider), variable })
    }

    pub fn homes(&self, id: &str) -> Option<Homes> {
        let account = self.get(id)?;
        self.homes_of(id, &account.provider)
    }
}
