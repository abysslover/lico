//! The fence profile of the engine (decided by the lead under the owner's delegation, mail 149, from the second measurement; for Moli by the M20 plan, from the trial) as data: what
//! it may read, run, write, connect to and bind, its whole environment and its limits, and the few files the host writes for it. Nothing here starts anything; `lico_sandbox` does, from this data.

use lico_sandbox::{Confinement, Limits};
use std::path::{Path, PathBuf};

/// The system directories the engine runs from; those that do not exist on this machine are left out.
pub const SYSTEM_DIRS: [&str; 3] = ["/usr", "/lib", "/lib64"];
/// Files the engine reads: the loader's cache and the random source.
pub const READ_FILES: [&str; 2] = ["/etc/ld.so.cache", "/dev/urandom"];
/// The one device it writes.
pub const NULL_DEVICE: &str = "/dev/null";
/// The system's bundle of certificate authorities, in the order they are tried: the first one that is a file is read by the engine and named to it (`--ca-cert`). The engine's own default
/// (`/usr/lib/ssl/cert.pem`) is a link into `/etc` that the fence does not let it follow (MEASURED, the Moli trial).
pub const CA_BUNDLES: [&str; 4] = ["/etc/ssl/certs/ca-certificates.crt", "/etc/pki/tls/certs/ca-bundle.crt", "/etc/ssl/ca-bundle.pem", "/etc/ssl/cert.pem"];
/// The least time between two frames of the page, in milliseconds. The engine sends a frame only when the page has changed (MEASURED, the Moli trial); its default is 1000.
pub const SCREENCAST_INTERVAL_MS: u32 = 100;
/// `RLIMIT_DATA` of the engine: the private memory it may map, not the address space (an engine reserves address space it never uses). Obscura (MEASURED, P06 probe): at 1 GiB the
/// engine cannot start (V8 fails to reserve its code range); at 2 and 3 GiB it starts and a page that allocates without end stops it; at 4 GiB the same; with no limit that
/// page runs on. Moli is a V8 engine too and starts under the same value (MEASURED, the Moli trial), so the value stays at the first one that is not flaky.
pub const DATA_LIMIT: u64 = 4 << 30;
/// Open files of the engine.
pub const OPEN_FILES: u64 = 1024;

pub struct Params<'a> {
    pub engine_dir: &'a Path,
    pub font_dir: Option<&'a Path>,
    pub scratch: &'a Path,
    pub cdp_port: u16,
    pub guard_port: u16,
    /// `http://lico:TOKEN@127.0.0.1:PORT`, the guard with the per-start credentials.
    pub proxy_url: &'a str,
    /// The system's bundle of certificate authorities (`find_ca_bundle`); without one the engine cannot check a secure site, and the tab says what it can.
    pub ca_bundle: Option<&'a Path>,
}

pub fn existing_system_dirs() -> Vec<PathBuf> {
    SYSTEM_DIRS.iter().map(PathBuf::from).filter(|p| p.is_dir()).collect()
}

/// The first of `CA_BUNDLES` that `is_file` says is a file (a test gives its own question).
pub fn find_ca_bundle(is_file: &dyn Fn(&Path) -> bool) -> Option<PathBuf> {
    CA_BUNDLES.iter().map(PathBuf::from).find(|p| is_file(p))
}

/// The file the host writes for the engine's font library: the system fonts, the Korean font directory when there is one, and a cache directory the engine may write. The engine has no
/// option for a font directory; its library reads this file (`FONTCONFIG_FILE`) instead of the system's own configuration, which the fence does not let it read.
pub fn fontconfig_file(font_dir: Option<&Path>, cache_dir: &Path) -> String {
    let mut dirs = String::from("<dir>/usr/share/fonts</dir>");
    if let Some(d) = font_dir {
        dirs.push_str(&format!("<dir>{}</dir>", xml_text(&d.to_string_lossy())));
    }
    format!("<?xml version=\"1.0\"?><fontconfig>{dirs}<cachedir>{}</cachedir></fontconfig>\n", xml_text(&cache_dir.to_string_lossy()))
}

fn xml_text(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Where the host writes the font file and where the font library caches, both in the engine's scratch directory.
pub fn fontconfig_path(scratch: &Path) -> PathBuf {
    scratch.join("fonts.conf")
}

pub fn font_cache_dir(scratch: &Path) -> PathBuf {
    scratch.join("fc-cache")
}

/// The confinement: read and run the engine's directory and the system directories; read the loader cache, the random source, the certificate bundle and the font directory; write one
/// scratch directory and `/dev/null`; TCP to the guard's port ONLY; bind the CDP port; the environment is the engine's own home and temporary directory (the scratch directory), its
/// font file and the proxy; the working directory is the scratch directory. There is no port 80 or 443.
pub fn confinement(p: &Params<'_>, system_dirs: &[PathBuf]) -> Confinement {
    let mut read_exec = vec![p.engine_dir.to_path_buf()];
    read_exec.extend(system_dirs.iter().cloned());
    let mut read: Vec<PathBuf> = READ_FILES.iter().map(PathBuf::from).collect();
    if let Some(ca) = p.ca_bundle {
        read.push(ca.to_path_buf());
    }
    if let Some(f) = p.font_dir {
        read.push(f.to_path_buf());
    }
    let scratch = p.scratch.to_string_lossy().into_owned();
    let mut env: Vec<(String, String)> = vec![("HOME".into(), scratch.clone()), ("TMPDIR".into(), scratch), ("FONTCONFIG_FILE".into(), fontconfig_path(p.scratch).to_string_lossy().into_owned())];
    // the engine's library reads one of these spellings; all go to the guard, and no variable lets any address go around it
    for name in ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy", "ALL_PROXY"] {
        env.push((name.into(), p.proxy_url.into()));
    }
    Confinement {
        read_exec,
        read,
        read_write: vec![p.scratch.to_path_buf(), PathBuf::from(NULL_DEVICE)],
        connect_ports: vec![p.guard_port],
        bind_ports: vec![p.cdp_port],
        env,
        cwd: p.scratch.to_path_buf(),
        limits: Limits { address_space: None, data: Some(DATA_LIMIT), cpu_seconds: None, open_files: Some(OPEN_FILES) },
        ..Confinement::default()
    }
}

/// The engine's arguments: `serve` on the loopback address and the CDP port, with the layout and the resources (without them there are no pictures), the frame interval, and the certificate bundle
/// when there is one. The proxy goes by the environment, never by an argument (a command line is readable by others).
pub fn args(p: &Params<'_>) -> Vec<String> {
    let mut a: Vec<String> = ["serve", "--host", "127.0.0.1", "--port"].map(String::from).to_vec();
    a.push(p.cdp_port.to_string());
    a.extend(["--layout", "--resource", "--screencast-interval-ms"].map(String::from));
    a.push(SCREENCAST_INTERVAL_MS.to_string());
    if let Some(ca) = p.ca_bundle {
        a.push("--ca-cert".into());
        a.push(ca.to_string_lossy().into_owned());
    }
    a.extend(["--log-level", "warn"].map(String::from));
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params<'a>(font: Option<&'a Path>, ca: Option<&'a Path>) -> Params<'a> {
        Params { engine_dir: Path::new("/home/u/.local/share/lico/browser/moli-v1.1.25"), font_dir: font, scratch: Path::new("/state/browser/scratch-1"), cdp_port: 41000, guard_port: 42000, proxy_url: "http://lico:GUARDTOKEN@127.0.0.1:42000", ca_bundle: ca }
    }

    fn system() -> Vec<PathBuf> {
        SYSTEM_DIRS.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn the_profile_is_exactly_the_one_decided_and_nothing_else() {
        let font = PathBuf::from("/usr/share/fonts/truetype/nanum");
        let ca = PathBuf::from("/etc/ssl/certs/ca-certificates.crt");
        let c = confinement(&params(Some(&font), Some(&ca)), &system());
        assert_eq!(c.read_exec, ["/home/u/.local/share/lico/browser/moli-v1.1.25", "/usr", "/lib", "/lib64"].map(PathBuf::from).to_vec());
        assert_eq!(c.read, ["/etc/ld.so.cache", "/dev/urandom", "/etc/ssl/certs/ca-certificates.crt", "/usr/share/fonts/truetype/nanum"].map(PathBuf::from).to_vec());
        assert_eq!(c.read_write, ["/state/browser/scratch-1", "/dev/null"].map(PathBuf::from).to_vec());
        assert_eq!((c.connect_ports.clone(), c.bind_ports.clone()), (vec![42000], vec![41000]), "TCP to the guard only; bind the CDP port only");
        assert!(!c.connect_ports.iter().chain(&c.bind_ports).any(|p| matches!(p, 80 | 443 | 53)), "no 80, 443 or DNS");
        assert_eq!(c.cwd, PathBuf::from("/state/browser/scratch-1"));
        let names: Vec<&str> = c.env.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(names, ["HOME", "TMPDIR", "FONTCONFIG_FILE", "HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy", "ALL_PROXY"], "the whole environment");
        assert!(!c.env.iter().any(|(k, _)| k == "PATH" || k == "DISPLAY" || k.contains("PRIVATE") || k.contains("TOKEN") || k == "NO_PROXY"), "nothing from the host, no token, no way around the proxy");
        let get = |k: &str| c.env.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str()).unwrap();
        assert_eq!((get("HOME"), get("TMPDIR")), ("/state/browser/scratch-1", "/state/browser/scratch-1"), "HOME and the temporary directory are the empty scratch directory");
        assert_eq!(get("FONTCONFIG_FILE"), "/state/browser/scratch-1/fonts.conf");
        for k in ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy", "ALL_PROXY"] {
            assert_eq!(get(k), "http://lico:GUARDTOKEN@127.0.0.1:42000", "{k}");
        }
        assert_eq!(c.limits, Limits { address_space: None, data: Some(4 << 30), cpu_seconds: None, open_files: Some(1024) });
    }

    #[test]
    fn without_a_font_directory_or_a_bundle_nothing_more_is_readable() {
        let c = confinement(&params(None, None), &system());
        assert_eq!(c.read, ["/etc/ld.so.cache", "/dev/urandom"].map(PathBuf::from).to_vec());
        let bare = args(&params(None, None));
        assert!(!bare.iter().any(|a| a == "--ca-cert"), "{bare:?}");
    }

    #[test]
    fn the_arguments_are_the_serve_line_the_trial_proved_and_carry_no_secret() {
        let ca = PathBuf::from("/etc/ssl/certs/ca-certificates.crt");
        let a = args(&params(None, Some(&ca)));
        assert_eq!(a, ["serve", "--host", "127.0.0.1", "--port", "41000", "--layout", "--resource", "--screencast-interval-ms", "100", "--ca-cert", "/etc/ssl/certs/ca-certificates.crt", "--log-level", "warn"]);
        assert_eq!(args(&params(None, None)), ["serve", "--host", "127.0.0.1", "--port", "41000", "--layout", "--resource", "--screencast-interval-ms", "100", "--log-level", "warn"]);
        let joined = a.join(" ");
        assert!(!joined.contains("TOKEN") && !joined.contains("lico:") && !joined.contains("42000") && !joined.contains("stealth") && !joined.contains("font-dir"), "{joined}");
    }

    #[test]
    fn the_first_certificate_bundle_that_is_a_file_wins_and_none_is_none() {
        let only = |name: &'static str| move |p: &Path| p == Path::new(name);
        assert_eq!(find_ca_bundle(&only("/etc/pki/tls/certs/ca-bundle.crt")), Some(PathBuf::from("/etc/pki/tls/certs/ca-bundle.crt")));
        assert_eq!(find_ca_bundle(&|_: &Path| true), Some(PathBuf::from("/etc/ssl/certs/ca-certificates.crt")), "the order of the list decides");
        assert_eq!(find_ca_bundle(&|_: &Path| false), None);
        assert!(CA_BUNDLES.iter().all(|p| p.starts_with("/etc/ssl/") || p.starts_with("/etc/pki/")), "system places only");
    }

    #[test]
    fn the_font_file_names_the_system_fonts_the_korean_directory_and_a_cache_in_the_scratch_directory() {
        let cache = font_cache_dir(Path::new("/s"));
        assert_eq!(cache, PathBuf::from("/s/fc-cache"));
        assert_eq!(fontconfig_path(Path::new("/s")), PathBuf::from("/s/fonts.conf"));
        let with = fontconfig_file(Some(Path::new("/usr/share/fonts/truetype/nanum")), &cache);
        assert_eq!(with, "<?xml version=\"1.0\"?><fontconfig><dir>/usr/share/fonts</dir><dir>/usr/share/fonts/truetype/nanum</dir><cachedir>/s/fc-cache</cachedir></fontconfig>\n");
        let without = fontconfig_file(None, &cache);
        assert_eq!(without, "<?xml version=\"1.0\"?><fontconfig><dir>/usr/share/fonts</dir><cachedir>/s/fc-cache</cachedir></fontconfig>\n");
        let odd = fontconfig_file(Some(Path::new("/a&b/<c>")), Path::new("/x&y"));
        assert!(odd.contains("<dir>/a&amp;b/&lt;c&gt;</dir>") && odd.contains("<cachedir>/x&amp;y</cachedir>"), "{odd}");
    }

    #[test]
    fn the_plan_is_accepted_by_the_sandbox() {
        // the same data, over directories that exist, passes the sandbox's own checks (a plan, no process)
        let dir = std::env::temp_dir().join(format!("lico_browser_profile_{}", crate::core::random_hex(6)));
        std::fs::create_dir_all(dir.join("engine")).unwrap();
        std::fs::create_dir_all(dir.join("scratch")).unwrap();
        let p = Params { engine_dir: &dir.join("engine"), font_dir: None, scratch: &dir.join("scratch"), ..params(None, None) };
        let c = confinement(&p, &existing_system_dirs());
        assert!(lico_sandbox::plan(&c, 4, &|q: &Path| Some(if q.is_dir() { lico_sandbox::PathKind::Dir } else { lico_sandbox::PathKind::File })).is_ok());
    }
}
