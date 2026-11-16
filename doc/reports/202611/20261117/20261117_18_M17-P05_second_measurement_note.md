# M17-P05 — Measurement Note: the Engine Behind the Fetch Guard

- Status: done; every figure is MEASURED here today unless marked READ (strace) or COMPUTED; nothing is accepted
- Version: 1.0.0
- Branch: `phase/M17-P05-lico-proxy-and-second-measurement` (on the P04 tip `25e70e5`); plan: [M17-P05](../../../plans/202611/20261117/20261117_17_M17-P05_lico_proxy_and_second_measurement_plan.md); before it: [P03 note](20261117_15_M17-P03_first_measurement_note.md)
- The run: the same archive as in P03 and Phase 0 (`v0.2.6`, SHA-256 `eed986e478c5ec5439f531f2d7fb6b0e36a356f1df7a6c44a8e8fd4f2486bd76`; `obscura` `c1400eec...e6c3`, `obscura-worker` `d1025e54...dd72`, both equal to the earlier hashes), fetched again from the project's release page into a new empty directory, extracted without running anything, run only through `lico-sandbox` as the child of `strace`, **with a fence that allows TCP to the proxy's port only** (and to bind the CDP port), a cleared environment, an empty HOME and working directory, on the fixture server of the test (reached only through the guard; its two names, `origin.test` and `other.test`, are resolved by a fake that gives `127.0.0.1`: **no name was looked up for real**). No real site. The archive and the extracted files are deleted and no process is left.

## First screen: does the engine honour the guard, and what the fence needs

**Yes. The engine sends every fetch through `--proxy`, and with the guard as its only way out it needs nothing more than P03's fence minus the fixture port.** The page, its stylesheet, script and image, a 302 redirect, a script `fetch()` and an image from a second host name all arrived as proxied requests (11 connections to the guard in a run of six navigations; every `connect()` in the whole strace of the run went to the guard's port). **The engine made no lookup of its own** (READ: no `AF_INET` datagram socket, no `AF_UNIX`, no `AF_NETLINK`; no open of `/etc/resolv.conf`, `/etc/hosts` or `/etc/nsswitch.conf`), so the fence needs **no DNS and no port 80 or 443**. The P03 gaps stay as they were (UDP, abstract Unix sockets, signals to the owner's other processes): the engine did none of them again.

The fence profile for P06 (what was enough): `read_exec` the engine's directory, `/usr`, `/lib`, `/lib64`; `read` `/etc/ld.so.cache`, `/dev/urandom` and the Korean font directory; `read_write` one scratch directory and `/dev/null`; TCP **connect: the guard's port only**; **bind: the CDP port**; environment: the CDP token, an empty `HOME` and `OBSCURA_PROXY`; the private-network variable is **not** needed. The same five reads were denied as in P03 (`/proc/self/maps`, `/proc/self/cgroup`, `/proc/stat`, `/proc/sys/vm/overcommit_memory`, `/sys/devices/system/cpu/online`) and the engine coped; `/dev/urandom` is the only other file it opened outside the engine's directory and the system libraries.

## Figures

| Question | Result |
|---|---|
| How the proxy is given | **both** `--proxy URL` and the environment variable `OBSCURA_PROXY` work, with the same result. **Credentials in the URL are honoured** (`http://lico:TOKEN@127.0.0.1:PORT`): every request carried `Proxy-Authorization` (the guard saw `authenticated` on all 11 connections), so the per-start token works. **Use the environment variable: an argument puts the token on the command line.** Without credentials in the URL the guard saw `authenticated: false`, as it should |
| What the engine sends | plain `http://` pages as **absolute-form** requests, one proxy connection per request (the guard forces `Connection: close`, so there is no reuse: the page of six requests cost six connections); `https://` as **`CONNECT host:port`** (the guard's event: `Connect`, `origin.test`, the port, authenticated) followed by the TLS handshake as raw bytes (it reached the fixture and failed there because the fixture is not a TLS server: "received corrupt message") |
| Subresources, redirect, script | all through the guard: stylesheet, script and image of the page; the redirect `/redirect` to `/second.html` (two proxied requests, the page ended on the second); a script `fetch()` (124 bytes read); an image from the **other host name** (the guard saw the host `other.test` as a separate connection) |
| The engine's own private-network check | **it stands in front of the guard only for a literal address**: navigating to `http://127.0.0.1:PORT/` was refused by the engine itself (`Access to private/internal IP address 127.0.0.1 is not allowed`) and the guard saw nothing; navigating to a NAME that the guard resolves to loopback was NOT refused by the engine (it does not resolve the name), with its switch off. So the guard's own policy is the real wall for names, and the engine's check is a first wall for literals |
| Loopback not opted in | the guard answered `403` (event `Refused(403)`), **the fixture saw 0 requests**; the engine's `Page.navigate` returned normally and the page stayed blank: **the engine gives CDP no error for a refused or failed fetch** (also for an unknown name: the guard's `502`, a blank page). **Lico must show its own message from the guard's events** (refused, not found), not wait for an error from the engine |
| Opted in | the page loaded as in P03 (title `scripted`, 4 requests); the fixture saw exactly the paths the pages need and nothing else |
| `--port 0` | **not usable under the fence**: the engine tried `bind(127.0.0.1:0)` and the fence denied it (`EACCES`; no port rule can name port 0, and `lico-sandbox` refuses port 0 by design); the engine does not fall back. P03 showed its banner prints the port it listens on, so an unfenced engine would tell it; but Lico chooses a free port, passes it with `--port` and allows exactly it (the race between choosing and binding is a bind error that Lico sees and retries) |
| Engine memory while behind the guard (information only) | 211 MiB after six navigations (as P03: it does not shrink) |
| The guard | 30 tests, 72 of 82 mutants caught, 10 redundant second walls (listed in the plan result); the guard's counters at the end of a run: 11 accepted, 1 refused (the unknown name), 3,067 to 3,480 bytes moved |

## For the lead (P06 and later)

- **Profile:** as above; the host adapter starts the guard with the engine and ends it with the last tab; no private-network variable; credentials in `OBSCURA_PROXY`.
- **A visible reason in the tab:** the guard's `Event` (`Refused(403)` a destination the policy refuses, `Refused(502)` a name that does not resolve or a connect that failed, `Refused(504)` too slow) is what Lico shows ("this address is on this computer or a private network, which this tab does not open", "the address could not be found"), because the engine itself shows a blank page.
- **The `localhost` opt-in:** the guard's `Policy { allow_loopback }` is fixed at its start; per-tab means a second guard (and a second engine) for the opted-in tabs, as mail 147 said.
- **A literal loopback address typed by the person** is refused by the engine before the guard: with the opt-in the person types a name (a hosts-style `localhost` resolved by the guard's resolver to `127.0.0.1`), not the address; or the adapter rewrites the literal to a name the guard maps (a design point for P06).
- **Real name lookups** were never made; the std resolver (a lookup behind a deadline) is thin and is the one part not run here.

## Files kept (the engine's own files are deleted)

`private/scratch/202611/20261117/m17_p05_measure/`: `out.log` (every `MEASURE` line) and `out/` (the strace and the engine's output of each of the five runs).
