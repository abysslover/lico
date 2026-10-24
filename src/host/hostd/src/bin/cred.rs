//! `lico-cred`: the credential command line. Secrets come in on standard input and leave only
//! through `helper`, to the caller that asked for one reference; they are never an argument.
//!
//!   lico-cred --data-dir DIR status
//!   lico-cred --data-dir DIR add --provider P --account A --kind K --label MASKED   (secret on stdin)
//!   lico-cred --data-dir DIR list
//!   lico-cred --data-dir DIR remove --ref R
//!   lico-cred --data-dir DIR helper --ref R                                           (secret on stdout)
//!
//! Exit codes: 0 done, 2 usage, 3 the keyring is missing or locked (nothing was stored anywhere
//! else), 4 no such credential, 5 the Secret Service failed.

#[path = "../creds.rs"]
mod creds;
#[path = "../dbus.rs"]
mod dbus;
#[path = "../keyring.rs"]
mod keyring;

use serde_json::json;
use std::io::{IsTerminal, Read, Write};
use std::path::PathBuf;

fn usage(why: &str) -> ! {
    eprintln!("lico-cred: {why}\nusage: lico-cred --data-dir DIR (status | list | add --provider P --account A --kind K --label MASKED | remove --ref R | helper --ref R)");
    std::process::exit(2);
}

fn plain(text: &str) -> bool {
    !text.is_empty() && text.len() <= 64 && text.bytes().all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b))
}

fn fail(error: &keyring::Error) -> ! {
    eprintln!("lico-cred: {}", error.action());
    std::process::exit(error.exit_code());
}

extern "C" {
    fn prctl(option: i32, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> i32;
}

fn main() {
    unsafe { prctl(4, 0, 0, 0, 0) }; // PR_SET_DUMPABLE 0: no core dump or ptrace read of a secret in memory
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 3 || args[0] != "--data-dir" {
        usage("--data-dir DIR and a command are required");
    }
    let dir = PathBuf::from(&args[1]);
    let command = args[2].as_str();
    let mut options = std::collections::HashMap::new();
    let mut rest = args[3..].iter();
    while let Some(flag) = rest.next() {
        match (flag.as_str(), rest.next()) {
            ("--provider" | "--account" | "--kind" | "--label" | "--ref", Some(value)) => {
                options.insert(flag.clone(), value.clone());
            }
            _ => usage("unknown or incomplete option"), // for example --secret: a secret is never an argument
        }
    }
    let option = |name: &str| options.get(name).map(String::as_str).unwrap_or_else(|| usage(&format!("{name} is required")));
    let allowed: &[&str] = match command {
        "status" | "list" => &[],
        "add" => &["--provider", "--account", "--kind", "--label"],
        "remove" | "helper" => &["--ref"],
        _ => usage("unknown command"),
    };
    if options.keys().any(|k| !allowed.contains(&k.as_str())) {
        usage("an option does not belong to this command");
    }
    match command {
        "status" => match keyring::Store::open() {
            Ok(_) => println!("{}", json!({"keyring": "ready"})),
            Err(error) => {
                let state = match error {
                    keyring::Error::NoBus | keyring::Error::NoService => "unavailable",
                    keyring::Error::NoDefault => "no_default_keyring",
                    keyring::Error::Locked => "locked",
                    _ => "failed",
                };
                println!("{}", json!({"keyring": state, "action": error.action()}));
                std::process::exit(error.exit_code());
            }
        },
        "list" => match creds::list(&dir) {
            Ok(entries) => println!("{}", serde_json::to_string(&entries).unwrap()),
            Err(e) => {
                eprintln!("lico-cred: cannot read the metadata: {e}");
                std::process::exit(5);
            }
        },
        "add" => {
            let (provider, account, kind, label) = (option("--provider"), option("--account"), option("--kind"), option("--label"));
            if !plain(provider) || !plain(account) || !plain(kind) || label.is_empty() || label.chars().count() > 40 || label.chars().any(char::is_control) {
                usage("provider, account and kind are 1 to 64 letters, digits or . _ : -; the masked label is 1 to 40 printable characters");
            }
            let mut store = keyring::Store::open().unwrap_or_else(|e| fail(&e)); // fail closed before anything is read
            if std::io::stdin().is_terminal() {
                usage("the secret is read from standard input: pipe it in");
            }
            let mut secret = Vec::new();
            let _ = std::io::stdin().lock().take(8193).read_to_end(&mut secret);
            if secret.len() > 8192 {
                keyring::wipe_secret(&mut secret);
                usage("the secret is longer than 8192 bytes");
            }
            if secret.last() == Some(&b'\n') {
                secret.pop();
            }
            if secret.is_empty() {
                usage("no secret on standard input");
            }
            if String::from_utf8_lossy(label.as_bytes()).contains(&*String::from_utf8_lossy(&secret)) {
                keyring::wipe_secret(&mut secret);
                usage("the masked label must not contain the secret");
            }
            let reference = creds::random_ref().unwrap_or_else(|_| {
                eprintln!("lico-cred: no random source");
                std::process::exit(5)
            });
            let attrs = [("application", "lico"), ("provider", provider), ("account", account), ("kind", kind), ("ref", reference.as_str())];
            let stored = store.set(&attrs, &format!("Lico {provider} {label}"), &secret);
            keyring::wipe_secret(&mut secret);
            if let Err(error) = stored {
                fail(&error);
            }
            if let Err(e) = creds::add(&dir, &reference, provider, account, kind, label) {
                let _ = store.delete(&[("application", "lico"), ("ref", reference.as_str())]); // no secret without its metadata
                eprintln!("lico-cred: cannot write the metadata: {e}");
                std::process::exit(5);
            }
            println!("{}", json!({"ref": reference}));
        }
        "remove" => {
            let reference = option("--ref");
            let mut store = keyring::Store::open().unwrap_or_else(|e| fail(&e));
            match store.delete(&[("application", "lico"), ("ref", reference)]) {
                Ok(()) | Err(keyring::Error::NotFound) => {}
                Err(error) => fail(&error),
            }
            if let Err(e) = creds::remove(&dir, reference) {
                eprintln!("lico-cred: cannot write the metadata: {e}");
                std::process::exit(5);
            }
        }
        "helper" => {
            let reference = option("--ref");
            if std::io::stdout().is_terminal() {
                usage("the secret goes to standard output for a program that asked for it, not to a terminal");
            }
            if !matches!(creds::find(&dir, reference), Ok(Some(_))) {
                eprintln!("lico-cred: no such credential");
                std::process::exit(4);
            }
            let mut store = keyring::Store::open().unwrap_or_else(|e| fail(&e));
            match store.get(&[("application", "lico"), ("ref", reference)]) {
                Ok(mut secret) => {
                    let mut out = std::io::stdout().lock();
                    let _ = out.write_all(&secret);
                    let _ = out.write_all(b"\n");
                    let _ = out.flush();
                    keyring::wipe_secret(&mut secret);
                }
                Err(error) => fail(&error),
            }
        }
        _ => unreachable!(),
    }
}
