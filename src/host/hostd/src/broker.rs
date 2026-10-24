//! The key broker: how a provider gets its account's key without being given the keyring.
//! The host listens on a private (abstract) socket for one session and answers each connection of the
//! same user by running the credential command with the keyring's bus, which the provider never
//! receives. What a provider's process tree can reach is therefore that one account's key (which the
//! provider needs anyway) and nothing else of the keyring. The socket ends with the session's process.

use std::io::Write;
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::{SocketAddr, UnixListener};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

pub fn serve(cred_bin: PathBuf, cred_args: Vec<String>, cred_dir: PathBuf, name: String, reference: String) -> std::io::Result<Arc<AtomicBool>> {
    let listener = UnixListener::bind_addr(&SocketAddr::from_abstract_name(name.as_bytes())?)?;
    listener.set_nonblocking(true)?;
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    thread::spawn(move || {
        while !flag.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((mut stream, _)) => {
                    if !crate::ipc::peer_is_owner(&stream) {
                        continue; // another user's connection gets nothing
                    }
                    let _ = stream.set_nonblocking(false);
                    let mut command = Command::new(&cred_bin);
                    command.args(&cred_args).arg("--data-dir").arg(&cred_dir).args(["helper", "--ref", &reference]).env_clear().stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
                    for key in ["PATH", "DBUS_SESSION_BUS_ADDRESS", "XDG_RUNTIME_DIR"] {
                        if let Some(value) = std::env::var_os(key) {
                            command.env(key, value);
                        }
                    }
                    if let Ok(mut output) = command.output() {
                        if output.status.success() {
                            let _ = stream.write_all(&output.stdout);
                        }
                        for byte in output.stdout.iter_mut() {
                            unsafe { std::ptr::write_volatile(byte, 0) }; // the key is not left in this buffer
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(50)),
                Err(_) => break,
            }
        }
    });
    Ok(stop)
}
