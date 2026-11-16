//! The fence profile of the engine (decided by the lead under the owner's delegation, mail 149, from the second measurement) as data: what it may read, run, write, connect
//! to and bind, its whole environment and its limits. Nothing here starts anything; `lico_sandbox` does, from this data.

use lico_sandbox::{Confinement, Limits};
use std::path::{Path, PathBuf};

/// The system directories the engine runs from; those that do not exist on this machine are left out.
pub const SYSTEM_DIRS: [&str; 3] = ["/usr", "/lib", "/lib64"];
/// Files the engine reads: the loader's cache and the random source.
pub const READ_FILES: [&str; 2] = ["/etc/ld.so.cache", "/dev/urandom"];
/// The one device it writes.
pub const NULL_DEVICE: &str = "/dev/null";
/// `RLIMIT_DATA` of the engine: the private memory it may map, not the address space (an engine reserves address space it never uses). PROPOSED 1 GiB; the real-engine probe
/// says whether it binds.
pub const DATA_LIMIT: u64 = 1 << 30;
/// Open files of the engine.
pub const OPEN_FILES: u64 = 1024;

pub struct Params<'a> {
    pub engine_dir: &'a Path,
    pub font_dir: Option<&'a Path>,
    pub scratch: &'a Path,
    pub cdp_port: u16,
    pub guard_port: u16,
    /// The bearer token the engine requires on its CDP socket.
    pub cdp_token: &'a str,
    /// `http://lico:TOKEN@127.0.0.1:PORT`, the guard with the per-start credentials.
    pub proxy_url: &'a str,
}

pub fn existing_system_dirs() -> Vec<PathBuf> {
    SYSTEM_DIRS.iter().map(PathBuf::from).filter(|p| p.is_dir()).collect()
}

/// The confinement: read and run the engine's directory and the system directories; read the loader cache, the random source and the font directory; write one scratch
/// directory and `/dev/null`; TCP to the guard's port ONLY; bind the CDP port; the environment is the token, an empty `HOME` (the scratch directory) and the proxy; the working
/// directory is the scratch directory. There is no private-network variable and no port 80 or 443.
pub fn confinement(p: &Params<'_>, system_dirs: &[PathBuf]) -> Confinement {
    let mut read_exec = vec![p.engine_dir.to_path_buf()];
    read_exec.extend(system_dirs.iter().cloned());
    let mut read: Vec<PathBuf> = READ_FILES.iter().map(PathBuf::from).collect();
    if let Some(f) = p.font_dir {
        read.push(f.to_path_buf());
    }
    Confinement {
        read_exec,
        read,
        read_write: vec![p.scratch.to_path_buf(), PathBuf::from(NULL_DEVICE)],
        connect_ports: vec![p.guard_port],
        bind_ports: vec![p.cdp_port],
        env: vec![("OBSCURA_CDP_TOKEN".into(), p.cdp_token.into()), ("HOME".into(), p.scratch.to_string_lossy().into_owned()), ("OBSCURA_PROXY".into(), p.proxy_url.into())],
        cwd: p.scratch.to_path_buf(),
        limits: Limits { address_space: None, data: Some(DATA_LIMIT), cpu_seconds: None, open_files: Some(OPEN_FILES) },
        ..Confinement::default()
    }
}

/// The engine's arguments: `serve --port N` and the font directory. The token and the proxy go by the environment, never by an argument (a command line is readable by others).
pub fn args(p: &Params<'_>) -> Vec<String> {
    let mut a = vec!["serve".to_string(), "--port".to_string(), p.cdp_port.to_string()];
    if let Some(f) = p.font_dir {
        a.push("--font-dir".into());
        a.push(f.to_string_lossy().into_owned());
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params<'a>(font: Option<&'a Path>) -> Params<'a> {
        Params { engine_dir: Path::new("/home/u/.local/share/lico/browser/v0.2.6"), font_dir: font, scratch: Path::new("/state/browser/scratch-1"), cdp_port: 41000, guard_port: 42000, cdp_token: "CDPTOKEN", proxy_url: "http://lico:GUARDTOKEN@127.0.0.1:42000" }
    }

    fn system() -> Vec<PathBuf> {
        SYSTEM_DIRS.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn the_profile_is_exactly_the_one_decided_and_nothing_else() {
        let font = PathBuf::from("/usr/share/fonts/truetype/nanum");
        let c = confinement(&params(Some(&font)), &system());
        assert_eq!(c.read_exec, ["/home/u/.local/share/lico/browser/v0.2.6", "/usr", "/lib", "/lib64"].map(PathBuf::from).to_vec());
        assert_eq!(c.read, ["/etc/ld.so.cache", "/dev/urandom", "/usr/share/fonts/truetype/nanum"].map(PathBuf::from).to_vec());
        assert_eq!(c.read_write, ["/state/browser/scratch-1", "/dev/null"].map(PathBuf::from).to_vec());
        assert_eq!((c.connect_ports.clone(), c.bind_ports.clone()), (vec![42000], vec![41000]), "TCP to the guard only; bind the CDP port only");
        assert!(!c.connect_ports.iter().chain(&c.bind_ports).any(|p| matches!(p, 80 | 443 | 53)), "no 80, 443 or DNS");
        assert_eq!(c.cwd, PathBuf::from("/state/browser/scratch-1"));
        let names: Vec<&str> = c.env.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(names, ["OBSCURA_CDP_TOKEN", "HOME", "OBSCURA_PROXY"], "the whole environment");
        assert!(!c.env.iter().any(|(k, _)| k.contains("PRIVATE") || k == "PATH" || k == "DISPLAY"), "no private-network variable and nothing from the host");
        assert_eq!(c.env[1].1, "/state/browser/scratch-1", "HOME is the empty scratch directory");
        assert_eq!(c.limits, Limits { address_space: None, data: Some(1 << 30), cpu_seconds: None, open_files: Some(1024) });
    }

    #[test]
    fn without_a_font_directory_nothing_of_fonts_is_readable_and_no_argument_is_added() {
        let c = confinement(&params(None), &system());
        assert_eq!(c.read.len(), 2);
        assert_eq!(args(&params(None)), ["serve", "--port", "41000"]);
        assert_eq!(args(&params(Some(Path::new("/f")))), ["serve", "--port", "41000", "--font-dir", "/f"]);
    }

    #[test]
    fn no_secret_is_on_the_command_line_and_the_plan_is_accepted_by_the_sandbox() {
        let a = args(&params(Some(Path::new("/f")))).join(" ");
        assert!(!a.contains("TOKEN") && !a.contains("lico:") && !a.contains("42000"), "{a}");
        // the same data, over directories that exist, passes the sandbox's own checks (a plan, no process)
        let dir = std::env::temp_dir().join(format!("lico_browser_profile_{}", crate::core::random_hex(6)));
        std::fs::create_dir_all(dir.join("engine")).unwrap();
        std::fs::create_dir_all(dir.join("scratch")).unwrap();
        let p = Params { engine_dir: &dir.join("engine"), font_dir: None, scratch: &dir.join("scratch"), ..params(None) };
        let c = confinement(&p, &existing_system_dirs());
        assert!(lico_sandbox::plan(&c, 4, &|q: &Path| Some(if q.is_dir() { lico_sandbox::PathKind::Dir } else { lico_sandbox::PathKind::File })).is_ok());
    }
}
