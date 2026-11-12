use super::*;

fn provider() -> Child {
    let mut command = Command::new("sleep");
    command.arg("60");
    unsafe {
        command.pre_exec(|| {
            setsid(); // its own group, like a provider
            Ok(())
        });
    }
    let child = command.spawn().expect("sleep");
    note_provider(child.id());
    child
}

#[test]
fn the_stop_returns_and_ends_the_providers_whatever_state_a_session_lock_is_in() {
    let dir = std::env::temp_dir().join(format!("lico_core_stop_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let session = |id: &str| Arc::new(Mutex::new(Session::new(id.to_string(), false, &dir, 100, 100)));
    let (held, poisoned, free) = (session("held"), session("poisoned"), session("free"));
    let p = poisoned.clone();
    let _ = thread::spawn(move || {
        let _guard = p.lock().unwrap();
        panic!("a session lock poisoned on purpose");
    })
    .join();
    assert!(poisoned.is_poisoned());
    let (h, held_now) = (held.clone(), mpsc::channel());
    thread::spawn(move || {
        let _guard = h.lock().unwrap(); // held for long, like the log's fsync under load
        held_now.0.send(()).unwrap();
        thread::sleep(Duration::from_secs(6));
    });
    held_now.1.recv().unwrap();
    let mut providers = [provider(), provider(), provider()]; // none is a session's own pid: only the list outside the locks can reach them
    let from = Instant::now();
    end_providers(&[held, poisoned, free], Duration::from_millis(500));
    assert!(from.elapsed() < Duration::from_secs(3), "the stop took {:?}", from.elapsed());
    for provider in providers.iter_mut() {
        let until = Instant::now() + Duration::from_secs(2);
        while provider.try_wait().unwrap().is_none() {
            assert!(Instant::now() < until, "a provider survived the stop");
            thread::sleep(Duration::from_millis(10));
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn within_returns_by_the_budget_whatever_the_work_does() {
    let from = Instant::now();
    assert!(within(Duration::from_secs(2), || {}), "a quick job finishes inside the budget");
    assert!(from.elapsed() < Duration::from_millis(500));
    let from = Instant::now();
    assert!(!within(Duration::from_millis(300), || thread::sleep(Duration::from_secs(3))), "a slow job is given up");
    assert!(from.elapsed() >= Duration::from_millis(300) && from.elapsed() < Duration::from_millis(1500), "it took {:?}", from.elapsed());
    let from = Instant::now();
    assert!(!within(Duration::from_secs(2), || panic!("the work panicked on purpose")), "a panic is not a finished job");
    assert!(from.elapsed() < Duration::from_millis(500), "a panic returns at once, it took {:?}", from.elapsed());
    assert!(within(Duration::from_secs(2), || {}), "the caller is not poisoned by it");
}

const CHILD: &str = "LICO_STOP_REPLY_CHILD";

/// The stop ends the process it runs in, so this test runs itself again in a child process (the first half) and the child (the second half) asks
/// host_stop for a stop while one session lock is held for 8 s, then prints how long the reply took.
#[test]
fn the_stop_reply_does_not_wait_for_a_session_lock_that_is_held() {
    if std::env::var(CHILD).is_err() {
        let from = Instant::now();
        let done = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "core::tests::the_stop_reply_does_not_wait_for_a_session_lock_that_is_held", "--nocapture", "--test-threads=1"])
            .env(CHILD, "1")
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&done.stdout).to_string();
        let reply = text.lines().find_map(|l| l.split("REPLY ms=").nth(1)).unwrap_or_else(|| panic!("the process ended without a reply: {text}"));
        assert!(reply.ends_with("ok=true"), "the stop was refused: {reply}");
        let reply: u64 = reply.split_whitespace().next().unwrap().parse().unwrap();
        assert!(reply < 2000, "the reply took {reply} ms");
        assert!(from.elapsed() < Duration::from_secs(7), "the process did not end by itself: {:?}", from.elapsed());
        return;
    }
    let dir = std::env::temp_dir().join(format!("lico_core_reply_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = Config {
        login_bin: None, login_args: Vec::new(), login_timeout: 60, log_dir: dir.join("log"), agent: PathBuf::new(), retain: 100, window: 100,
        queue_bytes: 1 << 20, payload: 4096, max_sessions: 8, codex: PathBuf::new(), codex_args: Vec::new(), claude: PathBuf::new(),
        claude_args: Vec::new(), cred_bin: PathBuf::new(), cred_args: Vec::new(), job_bin: PathBuf::new(), cred_dir: dir.join("cred"), workspace_root: dir.join("work"), handoff_dir: dir.join("handoff"),
        accounts_dir: dir.join("accounts"), registry_dir: dir.join("registry"), ssh_bin: PathBuf::new(), ssh_config: None, text: false, realistic_text: false,
    };
    let accounts = crate::accounts::Accounts::load(&cfg.accounts_dir).unwrap_or_else(|_| panic!("accounts"));
    let shared = Arc::new(Shared {
        cfg, sessions: RwLock::new(Vec::new()), views: Mutex::new(HashMap::new()), started: Instant::now(), pending_creates: AtomicUsize::new(0),
        accounts: Mutex::new(accounts), logins: Mutex::new(HashMap::new()), clip: Default::default(),
    });
    let held = Arc::new(Mutex::new(Session::new("held".to_string(), false, &dir, 100, 100)));
    shared.sessions.write().unwrap().push(held.clone());
    let (locked, now) = mpsc::channel();
    thread::spawn(move || {
        let _guard = held.lock().unwrap(); // held for long, like the log's fsync under load
        locked.send(()).unwrap();
        thread::sleep(Duration::from_secs(8));
    });
    now.recv().unwrap();
    let from = Instant::now();
    let reply = host_stop(&shared, true, None);
    println!("REPLY ms={} ok={}", from.elapsed().as_millis(), reply.is_ok());
    thread::sleep(Duration::from_secs(20)); // the stop ends this process long before
}
