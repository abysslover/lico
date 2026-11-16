//! The contract tests of `lico-proxy` (SPEC.md). No real name lookup and no real network: the resolver and the connector are fakes, and every origin is a socket of the test's
//! own on loopback (the "public" addresses of the tests are never connected to: the fake connector sends them to the origin).

use lico_proxy::*;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const SEC: Duration = Duration::from_secs(2);
const PUBLIC_A: IpAddr = IpAddr::V4(Ipv4Addr::new(93, 184, 216, 34));
const LOOP: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));

// ---- fakes -----------------------------------------------------------------------------------------------------------------------------------------

struct Origin {
    addr: SocketAddr,
    conns: Arc<AtomicUsize>,
    requests: Arc<Mutex<Vec<Vec<u8>>>>,
}

/// `reply`: what the origin answers after it has read a request (head and, by Content-Length, body); `None`: an echo server for tunnels.
fn origin(reply: Option<Vec<u8>>) -> Origin {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (conns, requests) = (Arc::new(AtomicUsize::new(0)), Arc::new(Mutex::new(Vec::new())));
    let (c2, r2) = (conns.clone(), requests.clone());
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { break };
            c2.fetch_add(1, Ordering::SeqCst);
            let (reply, r3) = (reply.clone(), r2.clone());
            thread::spawn(move || {
                let _ = s.set_read_timeout(Some(Duration::from_secs(3)));
                match reply {
                    None => {
                        let mut buf = [0u8; 4096];
                        while let Ok(n) = s.read(&mut buf) {
                            if n == 0 || s.write_all(&buf[..n]).is_err() {
                                break;
                            }
                        }
                    }
                    Some(reply) => {
                        let mut data = Vec::new();
                        let mut buf = [0u8; 4096];
                        loop {
                            if let Some(at) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                                let head = String::from_utf8_lossy(&data[..at]).to_lowercase();
                                let cl = head.lines().find_map(|l| l.strip_prefix("content-length:")).and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(0);
                                if data.len() >= at + 4 + cl {
                                    break;
                                }
                            }
                            match s.read(&mut buf) {
                                Ok(0) | Err(_) => break,
                                Ok(n) => data.extend_from_slice(&buf[..n]),
                            }
                        }
                        r3.lock().unwrap().push(data);
                        let _ = s.write_all(&reply);
                    }
                }
            });
        }
    });
    Origin { addr, conns, requests }
}

fn ok_reply() -> Vec<u8> {
    b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: keep-alive\r\nKeep-Alive: timeout=5\r\nProxy-Connection: keep-alive\r\n\r\nhello".to_vec()
}

#[derive(Default)]
struct FakeResolver {
    answers: Mutex<HashMap<String, Vec<Vec<IpAddr>>>>,
    calls: Mutex<Vec<String>>,
    delay: Mutex<Duration>,
}

impl FakeResolver {
    fn with(self, host: &str, per_call: Vec<Vec<IpAddr>>) -> FakeResolver {
        self.answers.lock().unwrap().insert(host.to_string(), per_call);
        self
    }
    fn count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

impl Resolver for FakeResolver {
    fn resolve(&self, host: &str, _timeout: Duration) -> std::io::Result<Vec<IpAddr>> {
        let delay = *self.delay.lock().unwrap();
        thread::sleep(delay);
        let n = self.calls.lock().unwrap().iter().filter(|h| *h == host).count();
        self.calls.lock().unwrap().push(host.to_string());
        let map = self.answers.lock().unwrap();
        let list = map.get(host).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no such name"))?;
        Ok(list[n.min(list.len() - 1)].clone())
    }
}

struct FakeConnector {
    to: SocketAddr,
    seen: Mutex<Vec<(IpAddr, u16)>>,
    fail: bool,
}

impl Connector for FakeConnector {
    fn connect(&self, ip: IpAddr, port: u16, _timeout: Duration) -> std::io::Result<TcpStream> {
        self.seen.lock().unwrap().push((ip, port));
        if self.fail {
            return Err(std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused"));
        }
        TcpStream::connect(self.to)
    }
}

struct Rig {
    proxy: Proxy,
    resolver: Arc<FakeResolver>,
    connector: Arc<FakeConnector>,
    origin: Origin,
}

fn config() -> Config {
    Config { limits: Limits { connect: SEC, resolve: SEC, idle: Duration::from_secs(5), total: Duration::from_secs(10), max_bytes: 1 << 20, max_body: 1 << 16 }, ..Config::default() }
}

fn rig(cfg: Config, reply: Option<Vec<u8>>) -> Rig {
    let o = origin(reply);
    let resolver = Arc::new(
        FakeResolver::default()
            .with("public.test", vec![vec![PUBLIC_A]])
            .with("local.test", vec![vec![LOOP]])
            .with("private.test", vec![vec![IpAddr::V4(Ipv4Addr::new(10, 0, 0, 5))]])
            .with("metadata.test", vec![vec![IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254))]])
            .with("mixed.test", vec![vec![PUBLIC_A, IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))]])
            .with("mapped.test", vec![vec!["::ffff:127.0.0.1".parse().unwrap()]])
            .with("mapped10.test", vec![vec!["::ffff:10.0.0.1".parse().unwrap()]])
            .with("rebind.test", vec![vec![PUBLIC_A], vec![LOOP]]),
    );
    let connector = Arc::new(FakeConnector { to: o.addr, seen: Mutex::new(Vec::new()), fail: false });
    let proxy = Proxy::start(cfg, resolver.clone(), connector.clone()).unwrap();
    Rig { proxy, resolver, connector, origin: o }
}

fn talk(addr: SocketAddr, bytes: &[u8]) -> Vec<u8> {
    let mut s = TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(4))).unwrap();
    let _ = s.write_all(bytes);
    let mut out = Vec::new();
    let _ = s.read_to_end(&mut out);
    out
}

fn status(resp: &[u8]) -> u16 {
    String::from_utf8_lossy(resp).split(' ').nth(1).and_then(|s| s.parse().ok()).unwrap_or(0)
}

fn get(host: &str, extra: &str) -> Vec<u8> {
    format!("GET http://{host}/path?x=1 HTTP/1.1\r\nHost: {host}\r\nAccept: */*\r\n{extra}\r\n").into_bytes()
}

fn basic(token: &str) -> String {
    basic_as("lico", token)
}

fn basic_as(user: &str, token: &str) -> String {
    let data = format!("{user}:{token}");
    let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in data.as_bytes().chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(alphabet[(n >> 18) as usize & 63] as char);
        out.push(alphabet[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { alphabet[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { alphabet[n as usize & 63] as char } else { '=' });
    }
    out
}

fn loopback_ok() -> Config {
    Config { policy: Policy { allow_loopback: true }, ..config() }
}

// ---- constants, the table, the host rules --------------------------------------------------------------------------------------------------------

#[test]
fn the_limits_are_named_constants_with_the_values_the_spec_gives() {
    assert_eq!((DEFAULT_MAX_CONNECTIONS, MAX_HEAD, MAX_HEADERS, MAX_BODY, MAX_HOST), (16, 16 * 1024, 100, 16 << 20, 253));
    assert_eq!((DEFAULT_CONNECT, DEFAULT_RESOLVE, DEFAULT_IDLE, DEFAULT_TOTAL, DEFAULT_MAX_BYTES), (Duration::from_secs(10), Duration::from_secs(5), Duration::from_secs(60), Duration::from_secs(300), 256 << 20));
    assert_eq!(PUBLIC_PORTS, [80, 443, 8080, 8443]);
    let c = Config::default();
    assert_eq!((c.max_connections, c.limits.max_body, c.policy.allow_loopback, c.token.is_none()), (DEFAULT_MAX_CONNECTIONS, MAX_BODY, false, true));
}

#[test]
fn every_range_of_the_address_table_has_its_class_and_only_public_is_allowed() {
    use Class::*;
    let v4 = |s: &str| IpAddr::V4(s.parse().unwrap());
    let v6 = |s: &str| IpAddr::V6(s.parse::<Ipv6Addr>().unwrap());
    let rows: Vec<(IpAddr, Class)> = vec![
        (v4("0.0.0.0"), Unspecified), (v4("0.1.2.3"), Unspecified), (v4("10.0.0.1"), Private), (v4("10.255.255.255"), Private), (v4("100.64.0.1"), Shared), (v4("100.127.255.255"), Shared),
        (v4("100.128.0.1"), Public), (v4("127.0.0.1"), Loopback), (v4("127.255.255.254"), Loopback), (v4("169.254.169.254"), LinkLocal), (v4("169.254.0.1"), LinkLocal),
        (v4("172.15.255.255"), Public), (v4("172.16.0.1"), Private), (v4("172.31.255.255"), Private), (v4("172.32.0.1"), Public), (v4("192.0.0.8"), Reserved), (v4("192.0.2.1"), Documentation),
        (v4("192.88.99.1"), Reserved), (v4("192.168.0.1"), Private), (v4("192.169.0.1"), Public), (v4("198.18.0.1"), Benchmark), (v4("198.19.255.255"), Benchmark), (v4("198.20.0.1"), Public),
        (v4("198.51.100.7"), Documentation), (v4("203.0.113.9"), Documentation), (v4("224.0.0.1"), Multicast), (v4("239.255.255.255"), Multicast), (v4("240.0.0.1"), Reserved),
        (v4("255.255.255.254"), Reserved), (v4("255.255.255.255"), Broadcast), (v4("93.184.216.34"), Public), (v4("8.8.8.8"), Public), (v4("1.1.1.1"), Public),
        (v6("::"), Unspecified), (v6("::1"), Loopback), (v6("::ffff:127.0.0.1"), Loopback), (v6("::ffff:10.1.2.3"), Private), (v6("::ffff:169.254.169.254"), LinkLocal), (v6("::ffff:8.8.8.8"), Public),
        (v6("64:ff9b::7f00:1"), Loopback), (v6("64:ff9b::a00:1"), Private), (v6("64:ff9b::808:808"), Public), (v6("64:ff9b:1::1"), Reserved), (v6("2002:7f00:1::1"), Loopback),
        (v6("2002:c0a8:101::1"), Private), (v6("2002:808:808::1"), Public), (v6("::7f00:1"), Reserved), (v6("::2"), Reserved), (v6("fc00::1"), Private), (v6("fdff::1"), Private),
        (v6("fec0::1"), Private), (v6("fe80::1"), LinkLocal), (v6("febf::1"), LinkLocal), (v6("ff02::1"), Multicast), (v6("2001::1"), Reserved), (v6("2001:db8::1"), Documentation),
        (v6("2001:2::1"), Reserved), (v6("100::1"), Reserved), (v6("2606:4700::1111"), Public), (v6("2a00:1450::1"), Public), (v6("4000::1"), Reserved), (v6("1::1"), Reserved),
    ];
    for (ip, want) in rows {
        assert_eq!(classify(ip), want, "{ip}");
        let default = Policy::default();
        let opted = Policy { allow_loopback: true };
        assert_eq!(default.allows(want), want == Public, "{ip}: by default only public is allowed");
        assert_eq!(opted.allows(want), want == Public || want == Loopback, "{ip}: the opt-in adds loopback and nothing else");
    }
}

#[test]
fn a_host_is_checked_before_any_lookup() {
    assert_eq!(check_host("Example.COM"), Ok(Host::Name("example.com".into())));
    assert_eq!(check_host("example.com."), Ok(Host::Name("example.com".into())), "one trailing dot is the same name");
    assert_eq!(check_host("a_b.example"), Ok(Host::Name("a_b.example".into())));
    assert_eq!(check_host("1a.example"), Ok(Host::Name("1a.example".into())), "a name with a digit label is still a name");
    assert_eq!(check_host("0xdeadbeef.com"), Ok(Host::Name("0xdeadbeef.com".into())));
    assert_eq!(check_host("localhost"), Ok(Host::Name("localhost".into())));
    assert_eq!(check_host("1.2.3.4"), Ok(Host::Ip(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)))));
    assert_eq!(check_host("2001:db8::1"), Ok(Host::Ip("2001:db8::1".parse().unwrap())));
    assert_eq!(check_host("::ffff:1.2.3.4"), Ok(Host::Ip("::ffff:1.2.3.4".parse().unwrap())));
    for tricky in ["127.1", "2130706433", "0x7f.0.0.1", "0177.0.0.1", "1.2.3", "1.2.3.4.5", "1.2.3.256", "0x7f000001", "010.1.1.1.", "1", "0"] {
        assert_eq!(check_host(tricky), Err(HostError::Numeric), "{tricky}: the system lookup would read it as an address");
    }
    assert_eq!(check_host("한글.test"), Err(HostError::NonAscii));
    assert_eq!(check_host(&"a".repeat(254)), Err(HostError::TooLong));
    assert!(check_host(&format!("{}.com", "a".repeat(60))).is_ok());
    assert_eq!(check_host(""), Err(HostError::Empty));
    for bad in ["a b", "a/b", "a\\b", "a@b", "a:b", "a\u{7}b", "-.", "a..b", ".a", "a,b", "a;b", "a<b"] {
        assert_eq!(check_host(bad), Err(HostError::Invalid), "{bad:?}");
    }
}

#[test]
fn a_deadline_returns_the_answer_in_time_and_gives_up_on_a_slow_one() {
    assert_eq!(with_deadline(Duration::from_secs(2), || 7), Some(7));
    let t = Instant::now();
    assert_eq!(with_deadline(Duration::from_millis(100), || {
        thread::sleep(Duration::from_millis(1500));
        7
    }), None);
    assert!(t.elapsed() < Duration::from_secs(1));
    assert_eq!(StdResolver.resolve("127.0.0.1", SEC).unwrap(), vec![LOOP], "an address literal needs no lookup");
}

// ---- the listener, CONNECT and the policy ----------------------------------------------------------------------------------------------------------

#[test]
fn it_listens_on_loopback_with_an_ephemeral_port() {
    let r = rig(config(), None);
    assert!(r.proxy.addr().ip().is_loopback() && r.proxy.addr().port() != 0);
}

#[test]
fn a_connect_to_a_name_that_resolves_to_a_public_address_is_a_tunnel_of_raw_bytes() {
    let r = rig(config(), None);
    let mut s = TcpStream::connect(r.proxy.addr()).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    s.write_all(b"CONNECT public.test:443 HTTP/1.1\r\nHost: public.test:443\r\n\r\n").unwrap();
    let mut head = vec![0u8; 39];
    s.read_exact(&mut head).unwrap();
    assert_eq!(&head[..], b"HTTP/1.1 200 Connection Established\r\n\r\n");
    s.write_all(b"\x16\x03\x01 raw bytes that are not HTTP").unwrap();
    let mut echoed = vec![0u8; 31];
    s.read_exact(&mut echoed).unwrap();
    assert_eq!(&echoed[..], b"\x16\x03\x01 raw bytes that are not HTTP");
    assert_eq!(r.resolver.count(), 1, "resolved once");
    assert_eq!(*r.connector.seen.lock().unwrap(), vec![(PUBLIC_A, 443)], "the connection went to the address that was checked");
}

#[test]
fn what_is_not_public_is_refused_with_a_403_and_nothing_reaches_the_origin() {
    for host in ["local.test:443", "private.test:443", "metadata.test:80", "mixed.test:443", "mapped.test:443", "mapped10.test:443", "127.0.0.1:443", "[::1]:443", "10.1.2.3:443", "[::ffff:127.0.0.1]:443", "169.254.169.254:80", "192.168.1.1:80", "0.0.0.0:80"] {
        let r = rig(config(), None);
        let resp = talk(r.proxy.addr(), format!("CONNECT {host} HTTP/1.1\r\nHost: {host}\r\n\r\n").as_bytes());
        assert_eq!(status(&resp), 403, "{host}");
        assert_eq!(r.origin.conns.load(Ordering::SeqCst), 0, "{host}");
        assert!(r.connector.seen.lock().unwrap().is_empty(), "{host}: not even a connect was tried");
    }
}

#[test]
fn the_opt_in_opens_loopback_and_nothing_else() {
    let r = rig(loopback_ok(), None);
    for (host, allowed) in [("local.test:3000", true), ("127.0.0.1:3000", true), ("[::1]:3000", true), ("mapped.test:3000", true), ("private.test:3000", false), ("metadata.test:3000", false), ("mapped10.test:3000", false), ("mixed.test:443", false), ("192.168.1.1:3000", false), ("[fe80::1]:3000", false)] {
        let mut s = TcpStream::connect(r.proxy.addr()).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        s.write_all(format!("CONNECT {host} HTTP/1.1\r\n\r\n").as_bytes()).unwrap();
        let mut buf = [0u8; 64];
        let n = s.read(&mut buf).unwrap();
        assert_eq!(status(&buf[..n]), if allowed { 200 } else { 403 }, "{host}");
    }
}

#[test]
fn public_destinations_have_four_ports_and_a_loopback_destination_any() {
    let r = rig(loopback_ok(), None);
    for (host, want) in [("public.test:80", 200), ("public.test:443", 200), ("public.test:8080", 200), ("public.test:8443", 200), ("public.test:25", 403), ("public.test:22", 403), ("public.test:3000", 403), ("public.test:65535", 403), ("local.test:25", 200)] {
        let mut s = TcpStream::connect(r.proxy.addr()).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        s.write_all(format!("CONNECT {host} HTTP/1.1\r\n\r\n").as_bytes()).unwrap();
        let mut buf = [0u8; 64];
        let n = s.read(&mut buf).unwrap();
        assert_eq!(status(&buf[..n]), want, "{host}");
    }
    let resp = talk(r.proxy.addr(), b"GET http://public.test:25/ HTTP/1.1\r\nHost: public.test:25\r\n\r\n");
    assert_eq!(status(&resp), 403, "plain http obeys the same ports");
}

#[test]
fn a_name_is_resolved_once_and_a_second_answer_is_never_seen() {
    let r = rig(config(), Some(ok_reply()));
    let resp = talk(r.proxy.addr(), &get("rebind.test", ""));
    assert_eq!(status(&resp), 200);
    assert_eq!(r.resolver.count(), 1, "one lookup for the connection");
    assert_eq!(*r.connector.seen.lock().unwrap(), vec![(PUBLIC_A, 80)]);
    let resp = talk(r.proxy.addr(), &get("rebind.test", ""));
    assert_eq!(status(&resp), 403, "the next connection looks again, and the answer is now loopback");
    assert_eq!(r.connector.seen.lock().unwrap().len(), 1);
}

#[test]
fn numeric_forms_and_odd_hosts_are_refused_before_the_resolver_is_asked() {
    let r = rig(config(), Some(ok_reply()));
    for host in ["127.1", "2130706433", "0x7f.0.0.1", "0177.0.0.1", "1.2.3", "한글.test"] {
        let resp = talk(r.proxy.addr(), format!("GET http://{host}/ HTTP/1.1\r\nHost: {host}\r\n\r\n").as_bytes());
        assert_eq!(status(&resp), 400, "{host}");
    }
    assert_eq!(r.resolver.count(), 0);
    assert_eq!(r.origin.conns.load(Ordering::SeqCst), 0);
}

#[test]
fn a_name_that_does_not_resolve_or_cannot_be_reached_is_a_502() {
    let r = rig(config(), Some(ok_reply()));
    assert_eq!(status(&talk(r.proxy.addr(), &get("nowhere.test", ""))), 502);
    let o = origin(Some(ok_reply()));
    let resolver = Arc::new(FakeResolver::default().with("public.test", vec![vec![PUBLIC_A]]));
    let connector = Arc::new(FakeConnector { to: o.addr, seen: Mutex::new(Vec::new()), fail: true });
    let p = Proxy::start(config(), resolver, connector).unwrap();
    assert_eq!(status(&talk(p.addr(), &get("public.test", ""))), 502);
}

#[test]
fn a_slow_resolver_is_given_up_on_at_the_deadline() {
    let mut cfg = config();
    cfg.limits.resolve = Duration::from_millis(200);
    let r = rig(cfg, Some(ok_reply()));
    *r.resolver.delay.lock().unwrap() = Duration::from_millis(1500);
    let t = Instant::now();
    assert_eq!(status(&talk(r.proxy.addr(), &get("public.test", ""))), 504);
    assert!(t.elapsed() < Duration::from_millis(1200), "{:?}", t.elapsed());
}

// ---- plain http through the proxy -----------------------------------------------------------------------------------------------------------------

#[test]
fn a_get_reaches_the_origin_in_origin_form_with_its_own_host_and_no_hop_by_hop_headers() {
    let r = rig(config(), Some(ok_reply()));
    let req = get("public.test:8080", "User-Agent: test/1\r\nConnection: keep-alive\r\nProxy-Connection: keep-alive\r\nProxy-Authorization: Basic eDp5\r\nKeep-Alive: 300\r\nTE: trailers\r\nUpgrade: websocket\r\nExpect: 100-continue\r\nTrailer: x\r\n");
    let resp = talk(r.proxy.addr(), &req);
    assert_eq!(status(&resp), 200);
    let seen = String::from_utf8(r.origin.requests.lock().unwrap()[0].clone()).unwrap();
    assert_eq!(seen, "GET /path?x=1 HTTP/1.1\r\nHost: public.test:8080\r\nAccept: */*\r\nUser-Agent: test/1\r\nConnection: close\r\n\r\n");
    let text = String::from_utf8_lossy(&resp).to_string();
    assert!(text.ends_with("\r\n\r\nhello"), "{text}");
    let head = text.to_lowercase();
    assert!(head.contains("connection: close") && !head.contains("keep-alive") && !head.contains("proxy-connection"), "no keep-alive survives in the answer: {text}");
    assert!(head.contains("content-length: 5"));
}

#[test]
fn the_host_header_of_the_client_does_not_choose_the_destination() {
    let r = rig(config(), Some(ok_reply()));
    let resp = talk(r.proxy.addr(), b"GET http://public.test/ HTTP/1.1\r\nHost: private.test\r\n\r\n");
    assert_eq!(status(&resp), 200);
    let seen = String::from_utf8(r.origin.requests.lock().unwrap()[0].clone()).unwrap();
    assert!(seen.starts_with("GET / HTTP/1.1\r\nHost: public.test\r\n"), "{seen}");
    assert_eq!(r.resolver.calls.lock().unwrap().as_slice(), ["public.test"]);
}

#[test]
fn a_post_body_is_sent_whole_by_its_content_length_and_a_body_over_the_cap_is_refused() {
    let r = rig(config(), Some(ok_reply()));
    let body = "name=value&x=".to_string() + &"y".repeat(5000);
    let req = format!("POST http://public.test/form HTTP/1.1\r\nHost: public.test\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n\r\n{body}", body.len());
    let mut s = TcpStream::connect(r.proxy.addr()).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(4))).unwrap();
    let (head, tail) = req.as_bytes().split_at(90); // the head and the body arrive in pieces
    s.write_all(head).unwrap();
    thread::sleep(Duration::from_millis(100));
    s.write_all(&tail[..2000]).unwrap();
    thread::sleep(Duration::from_millis(100));
    s.write_all(&tail[2000..]).unwrap();
    let mut resp = Vec::new();
    s.read_to_end(&mut resp).unwrap();
    assert_eq!(status(&resp), 200);
    let seen = r.origin.requests.lock().unwrap()[0].clone();
    assert!(seen.ends_with(body.as_bytes()), "the body arrived whole");
    assert!(String::from_utf8_lossy(&seen).contains(&format!("Content-Length: {}", body.len())));
    let big = format!("POST http://public.test/f HTTP/1.1\r\nHost: public.test\r\nContent-Length: {}\r\n\r\n", (1 << 16) + 1);
    let before = r.origin.conns.load(Ordering::SeqCst);
    assert_eq!(status(&talk(r.proxy.addr(), big.as_bytes())), 413);
    assert_eq!(r.origin.conns.load(Ordering::SeqCst), before, "nothing was sent for a body over the cap");
}

#[test]
fn everything_that_could_smuggle_a_request_is_refused_and_the_origin_sees_nothing() {
    let cases: Vec<(&str, String, u16)> = vec![
        ("transfer-encoding", "POST http://public.test/ HTTP/1.1\r\nHost: public.test\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n".into(), 501),
        ("transfer-encoding and content-length", "POST http://public.test/ HTTP/1.1\r\nHost: public.test\r\nTransfer-Encoding: chunked\r\nContent-Length: 4\r\n\r\n0\r\n\r\n".into(), 400),
        ("two different content-lengths", "POST http://public.test/ HTTP/1.1\r\nHost: public.test\r\nContent-Length: 4\r\nContent-Length: 5\r\n\r\nabcd".into(), 400),
        ("two equal content-lengths", "POST http://public.test/ HTTP/1.1\r\nHost: public.test\r\nContent-Length: 4\r\nContent-Length: 4\r\n\r\nabcd".into(), 400),
        ("content-length not a number", "POST http://public.test/ HTTP/1.1\r\nHost: public.test\r\nContent-Length: abc\r\n\r\n".into(), 400),
        ("content-length with a sign", "POST http://public.test/ HTTP/1.1\r\nHost: public.test\r\nContent-Length: +4\r\n\r\nabcd".into(), 400),
        ("content-length negative", "POST http://public.test/ HTTP/1.1\r\nHost: public.test\r\nContent-Length: -1\r\n\r\n".into(), 400),
        ("a folded header", "GET http://public.test/ HTTP/1.1\r\nHost: public.test\r\nX-A: one\r\n two\r\n\r\n".into(), 400),
        ("a NUL in a header", "GET http://public.test/ HTTP/1.1\r\nHost: public.test\r\nX-A: a\u{0}b\r\n\r\n".into(), 400),
        ("a bare line feed", "GET http://public.test/ HTTP/1.1\nHost: public.test\n\n".into(), 400),
        ("a header without a colon", "GET http://public.test/ HTTP/1.1\r\nHost: public.test\r\nnonsense\r\n\r\n".into(), 400),
        ("a header name with a space", "GET http://public.test/ HTTP/1.1\r\nHost: public.test\r\nX A: b\r\n\r\n".into(), 400),
        ("a user name in the address", "GET http://user:pw@public.test/ HTTP/1.1\r\nHost: public.test\r\n\r\n".into(), 400),
        ("a user name only", "GET http://user@public.test/ HTTP/1.1\r\nHost: public.test\r\n\r\n".into(), 400),
        ("https in absolute form", "GET https://public.test/ HTTP/1.1\r\nHost: public.test\r\n\r\n".into(), 400),
        ("origin form", "GET /path HTTP/1.1\r\nHost: public.test\r\n\r\n".into(), 400),
        ("asterisk form", "OPTIONS * HTTP/1.1\r\nHost: public.test\r\n\r\n".into(), 400),
        ("a version that is not 1.x", "GET http://public.test/ HTTP/2.0\r\nHost: public.test\r\n\r\n".into(), 400),
        ("no version", "GET http://public.test/\r\nHost: public.test\r\n\r\n".into(), 400),
        ("a lower-case method", "get http://public.test/ HTTP/1.1\r\nHost: public.test\r\n\r\n".into(), 400),
        ("a method that is not allowed", "TRACE http://public.test/ HTTP/1.1\r\nHost: public.test\r\n\r\n".into(), 405),
        ("a spaced target", "GET http://public.test/a b HTTP/1.1\r\nHost: public.test\r\n\r\n".into(), 400),
        ("a CONNECT with a path", "CONNECT public.test:443/x HTTP/1.1\r\n\r\n".into(), 400),
        ("a CONNECT without a port", "CONNECT public.test HTTP/1.1\r\n\r\n".into(), 400),
        ("a CONNECT with a bad port", "CONNECT public.test:0 HTTP/1.1\r\n\r\n".into(), 400),
        ("a header block over the cap", format!("GET http://public.test/ HTTP/1.1\r\nHost: public.test\r\nX-Big: {}\r\n\r\n", "a".repeat(MAX_HEAD)), 431),
        ("more headers than the cap", format!("GET http://public.test/ HTTP/1.1\r\nHost: public.test\r\n{}\r\n", (0..=MAX_HEADERS).map(|i| format!("X-{i}: v\r\n")).collect::<String>()), 431),
        ("a request line that never ends", "GET http://public.test/".to_string() + &"a".repeat(MAX_HEAD), 431),
    ];
    for (name, bytes, want) in cases {
        let r = rig(config(), Some(ok_reply()));
        let resp = talk(r.proxy.addr(), bytes.as_bytes());
        assert_eq!(status(&resp), want, "{name}: {}", String::from_utf8_lossy(&resp[..resp.len().min(40)]));
        assert_eq!(r.origin.conns.load(Ordering::SeqCst), 0, "{name}: the origin was never contacted");
        assert!(r.connector.seen.lock().unwrap().is_empty(), "{name}");
        assert!(String::from_utf8_lossy(&resp).to_lowercase().contains("connection: close"), "{name}: the refusal closes");
    }
}

#[test]
fn a_second_request_in_the_same_bytes_closes_the_connection_before_anything_is_sent() {
    for second in ["GET http://public.test/second HTTP/1.1\r\nHost: public.test\r\n\r\n", "GET http://private.test/second HTTP/1.1\r\nHost: private.test\r\n\r\n", "x"] {
        let r = rig(config(), Some(ok_reply()));
        let mut bytes = get("public.test", "");
        bytes.extend(second.as_bytes());
        let resp = talk(r.proxy.addr(), &bytes);
        assert_eq!(status(&resp), 400, "{second}");
        assert_eq!(r.origin.conns.load(Ordering::SeqCst), 0, "{second}: not even the first request was forwarded");
    }
    // bytes after a body are a second request too
    let r = rig(config(), Some(ok_reply()));
    let resp = talk(r.proxy.addr(), b"POST http://public.test/ HTTP/1.1\r\nHost: public.test\r\nContent-Length: 3\r\n\r\nabcGET http://public.test/x HTTP/1.1\r\n\r\n");
    assert_eq!(status(&resp), 400);
    assert_eq!(r.origin.conns.load(Ordering::SeqCst), 0);
}

#[test]
fn after_its_answer_the_connection_is_over_and_a_second_request_has_nowhere_to_go() {
    let r = rig(config(), Some(ok_reply()));
    let mut s = TcpStream::connect(r.proxy.addr()).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(4))).unwrap();
    s.write_all(&get("public.test", "")).unwrap();
    let mut first = Vec::new();
    s.read_to_end(&mut first).unwrap();
    assert_eq!(status(&first), 200, "the answer, then the end of the stream");
    let _ = s.write_all(&get("private.test", ""));
    let mut more = Vec::new();
    let _ = s.read_to_end(&mut more);
    assert!(more.is_empty(), "nothing more is ever answered on this connection");
    assert_eq!(r.origin.conns.load(Ordering::SeqCst), 1);
}

#[test]
fn an_answer_that_is_not_http_or_too_big_or_ambiguous_is_a_502_and_one_that_is_chunked_goes_through() {
    for (name, reply) in [
        ("garbage", b"this is not http".to_vec()),
        ("nothing", Vec::new()),
        ("a head over the cap", format!("HTTP/1.1 200 OK\r\nX-Big: {}\r\n\r\n", "a".repeat(MAX_HEAD)).into_bytes()),
        ("transfer-encoding with content-length", b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Length: 5\r\n\r\n5\r\nhello\r\n0\r\n\r\n".to_vec()),
        ("a head that never ends", b"HTTP/1.1 200 OK\r\nX-A: b\r\n".to_vec()),
    ] {
        let r = rig(config(), Some(reply));
        assert_eq!(status(&talk(r.proxy.addr(), &get("public.test", ""))), 502, "{name}");
    }
    let chunked = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: keep-alive\r\n\r\n5\r\nhello\r\n0\r\n\r\n".to_vec();
    let r = rig(config(), Some(chunked));
    let resp = talk(r.proxy.addr(), &get("public.test", ""));
    let text = String::from_utf8_lossy(&resp).to_string();
    assert!(status(&resp) == 200 && text.ends_with("\r\n\r\n5\r\nhello\r\n0\r\n\r\n") && text.to_lowercase().contains("connection: close") && !text.to_lowercase().contains("keep-alive"), "{text}");
}

// ---- the token -------------------------------------------------------------------------------------------------------------------------------------

#[test]
fn a_required_token_is_checked_in_constant_form_and_shown_nowhere() {
    let cfg = Config { token: Some("s3cret-token-value".into()), ..config() };
    assert!(!format!("{cfg:?}").contains("s3cret-token-value"));
    let r = rig(cfg, Some(ok_reply()));
    let bad_user = "Proxy-Authorization: Basic ".to_string() + &basic_as("xyz", "s3cret-token-value");
    for (name, extra) in [("none", String::new()), ("wrong", format!("Proxy-Authorization: Basic {}\r\n", basic("wrong-token"))), ("bearer", "Proxy-Authorization: Bearer s3cret-token-value\r\n".into()), ("bad user", bad_user + "\r\n"), ("empty", "Proxy-Authorization:\r\n".into())] {
        let resp = talk(r.proxy.addr(), &get("public.test", &extra));
        assert_eq!(status(&resp), 407, "{name}");
        assert!(String::from_utf8_lossy(&resp).to_lowercase().contains("proxy-authenticate: basic"), "{name}");
        assert!(!String::from_utf8_lossy(&resp).contains("s3cret-token-value"));
    }
    assert_eq!(r.origin.conns.load(Ordering::SeqCst), 0);
    let ok = format!("Proxy-Authorization: Basic {}\r\n", basic("s3cret-token-value"));
    assert_eq!(status(&talk(r.proxy.addr(), &get("public.test", &ok))), 200);
    let seen = String::from_utf8_lossy(&r.origin.requests.lock().unwrap()[0]).to_lowercase();
    assert!(!seen.contains("proxy-authorization") && !seen.contains("s3cret"), "the credential never reaches the origin: {seen}");
    let mut s = TcpStream::connect(r.proxy.addr()).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    s.write_all(format!("CONNECT public.test:443 HTTP/1.1\r\n{ok}\r\n").as_bytes()).unwrap();
    let mut buf = [0u8; 64];
    let n = s.read(&mut buf).unwrap();
    assert_eq!(status(&buf[..n]), 200, "CONNECT takes the token too");
    let resp = talk(r.proxy.addr(), b"CONNECT public.test:443 HTTP/1.1\r\n\r\n");
    assert_eq!(status(&resp), 407);
}

// ---- limits, events, stop --------------------------------------------------------------------------------------------------------------------------

#[test]
fn the_connection_after_the_last_free_slot_gets_a_503_and_a_freed_slot_is_used_again() {
    let mut cfg = config();
    cfg.max_connections = 2;
    let r = rig(cfg, Some(ok_reply()));
    let a = TcpStream::connect(r.proxy.addr()).unwrap();
    let b = TcpStream::connect(r.proxy.addr()).unwrap();
    thread::sleep(Duration::from_millis(200));
    assert_eq!(r.proxy.stats().active, 2);
    let resp = talk(r.proxy.addr(), &get("public.test", ""));
    assert_eq!(status(&resp), 503);
    assert_eq!(r.origin.conns.load(Ordering::SeqCst), 0);
    drop(a);
    thread::sleep(Duration::from_millis(300));
    assert_eq!(status(&talk(r.proxy.addr(), &get("public.test", ""))), 200);
    drop(b);
}

#[test]
fn a_connection_that_says_nothing_or_goes_on_too_long_or_moves_too_many_bytes_is_closed() {
    let mut cfg = config();
    cfg.limits.idle = Duration::from_millis(300);
    let r = rig(cfg, None);
    let t = Instant::now();
    let resp = talk(r.proxy.addr(), b"");
    assert!(resp.is_empty() || status(&resp) == 408, "a connection that sends nothing is ended");
    assert!(t.elapsed() < Duration::from_secs(2));
    // a half request that stalls
    let resp = talk(r.proxy.addr(), b"GET http://public.test/ HTTP/1.1\r\nHost: pub");
    assert!(resp.is_empty() || status(&resp) == 408);
    // a tunnel that goes on past the total time
    let mut cfg = config();
    cfg.limits.total = Duration::from_millis(600);
    let r = rig(cfg, None);
    let mut s = TcpStream::connect(r.proxy.addr()).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(4))).unwrap();
    s.write_all(b"CONNECT public.test:443 HTTP/1.1\r\n\r\n").unwrap();
    let mut buf = [0u8; 256];
    s.read(&mut buf).unwrap();
    let t = Instant::now();
    let mut closed = false;
    while t.elapsed() < Duration::from_secs(4) {
        if s.write_all(b"x").is_err() {
            closed = true;
            break;
        }
        match s.read(&mut buf) {
            Ok(0) | Err(_) if t.elapsed() > Duration::from_millis(300) => {
                closed = true;
                break;
            }
            _ => {}
        }
        thread::sleep(Duration::from_millis(50));
    }
    assert!(closed && t.elapsed() < Duration::from_secs(3), "the total time ended it: {:?}", t.elapsed());
    // the byte cap
    let mut cfg = config();
    cfg.limits.max_bytes = 20_000;
    let r = rig(cfg, None);
    let mut s = TcpStream::connect(r.proxy.addr()).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    s.write_all(b"CONNECT public.test:443 HTTP/1.1\r\n\r\n").unwrap();
    let mut head = [0u8; 39];
    s.read_exact(&mut head).unwrap();
    let chunk = vec![7u8; 4096];
    let mut sent = 0;
    for _ in 0..200 {
        if s.write_all(&chunk).is_err() {
            break;
        }
        sent += chunk.len();
        thread::sleep(Duration::from_millis(2));
    }
    let mut got = 0usize;
    let mut buf = vec![0u8; 8192];
    while let Ok(n) = s.read(&mut buf) {
        if n == 0 {
            break;
        }
        got += n;
    }
    assert!(got <= 20_000 + 4096 && sent < 200 * 4096, "the tunnel was cut near its cap (both directions count): sent {sent}, got back {got}");
}

#[test]
fn what_is_observed_is_the_host_and_the_port_and_never_the_path_or_the_query() {
    let events: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
    let e2 = events.clone();
    let cfg = Config { observe: Some(Arc::new(move |e: &Event| e2.lock().unwrap().push(e.clone()))), token: Some("tok".into()), ..config() };
    let r = rig(cfg, Some(ok_reply()));
    let auth = format!("Proxy-Authorization: Basic {}\r\n", basic("tok"));
    let line = format!("GET http://public.test/secret/path?token=zzz HTTP/1.1\r\nHost: public.test\r\n{auth}\r\n");
    talk(r.proxy.addr(), line.as_bytes());
    talk(r.proxy.addr(), format!("CONNECT public.test:443 HTTP/1.1\r\n{auth}\r\n").as_bytes());
    talk(r.proxy.addr(), b"GET http://public.test/x HTTP/1.1\r\n\r\n");
    talk(r.proxy.addr(), format!("GET http://private.test/ HTTP/1.1\r\n{auth}\r\n").as_bytes());
    thread::sleep(Duration::from_millis(300));
    let seen = events.lock().unwrap().clone();
    let kinds: Vec<(EventKind, &str, u16, bool)> = seen.iter().map(|e| (e.kind.clone(), e.host.as_str(), e.port, e.authenticated)).collect();
    assert!(kinds.contains(&(EventKind::Http, "public.test", 80, true)), "{kinds:?}");
    assert!(kinds.contains(&(EventKind::Connect, "public.test", 443, true)), "{kinds:?}");
    assert!(kinds.iter().any(|k| k.0 == EventKind::Refused(407) && !k.3), "{kinds:?}");
    assert!(kinds.iter().any(|k| k.0 == EventKind::Refused(403) && k.1 == "private.test"), "{kinds:?}");
    let shown = format!("{seen:?}");
    assert!(!shown.contains("secret") && !shown.contains("zzz") && !shown.contains("tok\""), "{shown}");
    let s = r.proxy.stats();
    assert!(s.accepted >= 4 && s.refused >= 2, "{s:?}");
}

#[test]
fn stop_ends_every_connection_and_the_listener_and_returns_quickly() {
    let r = rig(config(), None);
    let addr = r.proxy.addr();
    let mut tunnel = TcpStream::connect(addr).unwrap();
    tunnel.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    tunnel.write_all(b"CONNECT public.test:443 HTTP/1.1\r\n\r\n").unwrap();
    let mut head = [0u8; 39];
    tunnel.read_exact(&mut head).unwrap();
    let mut idle = TcpStream::connect(addr).unwrap();
    idle.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    thread::sleep(Duration::from_millis(200));
    let t = Instant::now();
    r.proxy.stop();
    assert!(t.elapsed() < Duration::from_secs(2), "{:?}", t.elapsed());
    let mut buf = [0u8; 8];
    assert!(matches!(tunnel.read(&mut buf), Ok(0) | Err(_)));
    assert!(matches!(idle.read(&mut buf), Ok(0) | Err(_)));
    assert!(TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_err(), "nothing listens any more");
}
