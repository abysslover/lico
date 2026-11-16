# M17-P03 Plan — `lico-sandbox` and the First Measurement of the Engine

- Status: approved by the lead under the owner's delegation (the owner away), 2026-11-17, answer to question 130 (mail 146), in progress; the order (P03 = the sandbox component and the first measurement, then a stop for a re-plan) and the conditions of the run were given by the lead in mails 143 and 144
- Version: 1.0.0
- Branch: `phase/M17-P03-sandbox-and-first-measurement` (stacked on the P02 tip `23d543c`)
- Milestone: [M17](../../../milestones/202611/20261117/20261117_02_M17_browser_tab_with_obscura_milestone.md); Phase 0: [report](../../../reports/202611/20261117/20261117_12_M17-P01_browser_tab_phase0_report.md)

## Objective

Two things, in this order. (1) `lico-sandbox`, a real component (not a throwaway): it starts a program under a Landlock confinement with a cleared environment, and proves by tests that the confinement denies what it must. (2) Only after those tests pass, the first run of the engine (`obscura` `v0.2.6`, the same archive and SHA-256 as in Phase 0, fetched again under the conditions of mail 143) with my own fixture pages, driven through the real `lico-cdp`, to measure what the documentation cannot say. The figures go into a short measurement note (MEASURED) and into the plan of the next phase. Then STOP with a `done` file; the lead re-plans P04 to P08. If something contradicts the owner's aim, a `question` file instead. No product code changes: the program does not depend on the new component.

## Specification draft: `lico-sandbox 0.1.0` (std only, no dependency; the system calls are declared directly, as `procs.rs` does for `prctl`)

Marks: **KNOWN FROM DOCUMENTATION** (the Landlock kernel interface: `landlock_create_ruleset`, `landlock_add_rule`, `landlock_restrict_self`, the access-right bits of ABI 1 to 4; `PR_SET_NO_NEW_PRIVS`; `close_range`), **MEASURED** (Phase 0: this machine has ABI 4; no namespaces), **PROPOSED**.

Purpose: run a program as a child with (a) the filesystem limited to the listed paths, (b) TCP connect and bind limited to the listed ports, (c) a cleared environment, a given working directory and resource limits, (d) its own session, dying with its parent. **It fails closed**: if Landlock cannot give what was asked, the program is not started.

| Name | Meaning |
|---|---|
| `Confinement { read_exec, read, read_write, connect_ports, bind_ports, env, cwd, limits, min_abi }` | what the child may do; every path is absolute and exists; `read_exec`: read and execute (the program, its libraries); `read`: read only (a data file, a font directory); `read_write`: read, write, create, remove (one scratch directory); a file path is allowed as well as a directory (the rights are cut to what a file can have); `env` is the whole environment of the child; `limits: Limits { address_space, data, cpu_seconds, open_files }` all optional; `min_abi` defaults to 4 |
| `confine(&mut Command, &Confinement) -> Result<(), Error>` | configures a `std::process::Command` (environment cleared and set, working directory, `pre_exec`) and checks everything in the parent: the paths are opened and the ruleset is built BEFORE the fork, so that the child only makes system calls (no allocation after the fork); the caller chooses the standard streams and spawns |
| `abi_version() -> Result<u32, Error>` | the Landlock ABI of this kernel (`Unsupported` when there is none) |
| `plan(&Confinement, abi, &dyn Fn(&Path) -> Option<PathKind>) -> Result<Plan, Error>` | the pure part: the handled rights for the ABI, the rule of each path (the rights allowed for a file or a directory, by ABI), the port rules, every check; no system call |
| `Error { Unsupported { have, need }, Path(&'static str), Port(&'static str), TooMany(&'static str), Env(&'static str), Os(&'static str, errno) }` | `Debug` shows paths and never an environment value (the token travels in the environment) |

What the child does between the fork and the program (all with the system calls only, in this order): `PR_SET_NO_NEW_PRIVS`; the resource limits; the ruleset (`landlock_create_ruleset`, one `landlock_add_rule` per path and per port, `landlock_restrict_self`); `setsid`; `PR_SET_PDEATHSIG` with `SIGKILL`; `close_range(3, all, CLOSE_RANGE_CLOEXEC)` so that no descriptor of the parent survives into the program (the descriptors of the rules are not needed after the restriction and also close on exec).

### The limits (named constants, with their reasons)
| Constant | Value | Reason |
|---|---|---|
| `MIN_ABI` | 4 | ABI 4 is the first with TCP port rules; without them the child could connect anywhere on the machine; a kernel with a lower ABI is refused (`Unsupported`), not run with less |
| `MAX_PATHS` | 64 | the rules of a real confinement are a dozen (system directories, the engine's directory, one scratch directory); more is a mistake or a leak |
| `MAX_PORTS` | 16 | the CDP port and one or two fixture ports are all a tab needs |
| `MAX_ENV` | 32 | an engine needs a token and a few switches; the whole environment of a user is hundreds of entries and is exactly what must not be passed |
| `MAX_ENV_BYTES` | 16 KiB | the same reason, in bytes |

### Invariants
1. **Fail closed**: no Landlock, a lower ABI than `min_abi`, a path that is not absolute or does not exist or has a NUL, a path that is `/` (nothing is confined then), a duplicate port, a port 0, too many paths, ports or environment entries, a variable name that is not `[A-Za-z_][A-Za-z0-9_]*` or a value with a NUL: `Error` and no process.
2. **What the child cannot do** (proved by tests that run a probe as the child, below): read a file outside `read_exec`, `read` and `read_write`; write outside `read_write`; connect to a TCP port that is not in `connect_ports`; bind one that is not in `bind_ports`; see an environment variable that is not in `env`; keep a descriptor of the parent; get privileges back (`NO_NEW_PRIVS`); outlive the parent; share its session.
3. **What it does NOT cover, said plainly (a test pins each, so that a kernel that begins to cover it makes the test fail and the text must be revisited):** UDP (DNS lookups and datagrams), abstract Unix sockets and connections to Unix sockets by path inside the allowed directories, signals to other processes of the same user, reading `/proc/PID/...` of other processes if `/proc` is allowed (so the policy never allows it whole), CPU and the number of processes. The engine can read nothing secret (the confinement keeps it from every file but the allowed ones) so there is nothing for it to send, but the residual risk is real and is the owner's to know.
4. The environment of the child is exactly `env`: nothing is inherited, `PATH` included.
5. No thread is started; the crate reads no file but the ones named in the confinement (to check them) and no environment variable of its own process.

Budget: the default 256 MiB and 30 s.

## Tests first (the sandbox is proved before the engine is ever started)

The probe is the test binary itself, started as the confined child with an argument that selects a probe function (std only; no shell, no Python, no tool of the system besides the dynamic loader and libc the test binary already needs). Pure tests first (the rights of each ABI; the rule of a file against a directory; every refusal of the invariant 1), then the denial tests, each with its control (the same probe allowed passes, so that a denial is not a broken probe): a file outside the allowed directories is DENIED (`EACCES`) and an allowed one is read; a write outside the scratch directory is DENIED and one inside is made; a connection to a port that is not allowed is DENIED (`EACCES`) and to the allowed one (a listener of the test) succeeds; a bind to a port that is not allowed is DENIED; the child sees only the given environment (a canary variable and the whole real environment of the test are absent); a descriptor of the parent is not open in the child; `NO_NEW_PRIVS` is set; the child is in its own session; the child dies when the parent does (the test starts an intermediate parent); `min_abi` above the kernel's is `Unsupported` and no process starts; an address-space limit stops a probe that allocates more; the known gaps (UDP to loopback is NOT stopped; a connection to a Unix socket in an allowed directory is NOT stopped) are asserted as they are. Mutation checks as in M16 and M17-P02, figures reported.

What cannot be red first: the three system calls and the `pre_exec` order are thin; each is proved by the denial test that needs it (a mutant that drops it must make a test fail), not by a test of its own.

## The first measurement (after the tests are green)

**Conditions, unchanged from mails 143 and 144.** The archive `obscura-x86_64-linux.tar.gz` of `v0.2.6` (the plain build; commit `8534b40`; 71,782,128 bytes; SHA-256 `eed986e478c5ec5439f531f2d7fb6b0e36a356f1df7a6c44a8e8fd4f2486bd76`) fetched from the project's own release page over HTTPS into a new empty directory under `private/scratch/` (dated layout), the hash compared with GitHub's digest and with the one of Phase 0 (the code download is announced to the owner in the approval prompt), extracted without running anything but the one program; run only through `lico-sandbox`, under `pri-heavy` with a memory limit, with a cleared environment and an empty HOME and working directory; the CDP token passed in the environment; `--allow-private-network` (or its variable) passed **for the measurement only**, because the fixtures are on `127.0.0.1`; no stealth switch and no stealth variable; no proxy; fixture pages only, served by a small server of my own on a loopback port: static HTML, CSS, an image, one script, a form, Korean text. No real website, not even to look. Afterwards the archive and the extracted files are deleted (hashes and figures kept) and no process is left (checked by exact pid). A `question` file (131) BEFORE running only if something in the archive differs from Phase 0 or the confinement turns out unable to hold (for example the engine needs `/proc` or a loopback port range that cannot be allowed without opening the machine).

**The harness** is an ignored integration test of `lico-sandbox` (`tests/engine_probe.rs`, run with `LICO_ENGINE_DIR=... cargo test -p lico-sandbox --test engine_probe -- --ignored --nocapture`; never part of a normal run; it needs the extracted engine and says so) that depends on `lico-cdp` as a dev-dependency: it writes the confinement, starts the engine, connects with `lico-cdp` and prints the figures as JSON lines for the note.

**What is measured (each MEASURED, or "not observed" and why):**
1. How the engine starts and how the port is chosen (`serve --port N`; whether `--port 0` works; the discovery answer, `/json/version` or the like, and the WebSocket path); the first-line output; the processes it starts (`obscura-worker`) and whether it needs anything the confinement denies (a denial is recorded as it is, with the call that failed, and the confinement is widened only for what the engine needs and never to `/proc` whole, the home directory or a port range).
2. The identity it presents: `navigator.userAgent`, `navigator.webdriver`, the platform, the language and the timezone (through `Runtime.evaluate` on a fixture page), and whether `Emulation.setUserAgentOverride` (or the `--user-agent` option) lets Lico set a plain, honest one.
3. Frames: `Page.startScreencast` (the format it sends, `jpeg` or `png`; the parameters it accepts; bytes per frame at 1200x800 and 640x560; frames per second while a fixture animates and while it is still; whether it sends nothing when the page does not change; whether the acknowledgement is needed to go on) and `Page.captureScreenshot` (formats, bytes, latency); the decoded size and the CPU the engine uses per frame (from its `/proc` figures, read by the harness, which is not confined).
4. Rendering: whether each fixture paints: CSS layout, an image, the form controls, one script that changes the page, Korean text (with a font directory handed in with `--font-dir` and without one), and what is visibly wrong (the frame is saved under `private/scratch/` for the owner and looked at; no pixel test).
5. Input: whether `Input.dispatchMouseEvent` (press, release, move, wheel), `Input.dispatchKeyEvent` (with `text`), `Input.insertText` and any composition command exist and change a fixture (a click on a link, a typed word in the form, scrolling), the page's own reports of the events it got, and the loading and title events.
6. Memory and time: the resident size of the engine and of each worker idle, after one page, while screencasting and after closing the tab (information, not a gate); the start time to the first answer; the page load time of the fixtures.
7. Limits seen: the largest frame (to set `DEFAULT_MAX_MESSAGE` from a figure), the events per second, what the engine does on a navigation to a refused address (a real internet address is NOT tried: the private-network block is checked with a fixture address that the block should refuse, if there is one that does not leave the machine, else it is reported as not observed).

## Steps

- [x] 1. This plan, the specification draft and the question file (130); the lead's approval.
- [ ] 2. `lico-sandbox`: the pure tests red, `plan` and the refusals green; then the denial tests red, the system calls and `confine` green; mutation checks; receipt; commits (`Cargo.lock` with the new member).
- [ ] 3. The harness (ignored test) written and run once against a harmless program of the system to see it work with no engine (the sandboxed probe path), then the download (announced), the check of the hashes, the run, the figures.
- [ ] 4. The measurement note (`doc/reports/...`, short, every figure MEASURED, the saved frames listed), the cleanup (archive and extracted files deleted, no process left), the result in this plan, the `done` file, and STOP for the lead.

## Estimate and the stop rule

`lico-sandbox` about 600 lines of source and 900 of tests (rate between M16's component rate and its hub rate, because system calls are new ground): 3 to 4.5 USD; the harness about 600 lines: 1.5 to 2.5 USD; the run and the debugging of what the confinement denies, and the note: 2 to 3 USD. **Total 6.5 to 10 USD.** If the phase has used about 15 USD and is not near done, I stop at a green commit and write a `question` file with the figures.

## Acceptance Criteria

`lico-sandbox` with a version, `SPEC.md`, contract tests, a receipt and its mutation figures, its denial tests green before the engine is started; the measurement note with every figure marked and the archive deleted; `cargo xtask check` and `verify` pass; the program, its binary and its tests unchanged; nothing installed; the phase stops with a `done` file.

## Addition of the lead (mail 146): what the engine tries, and what the fence does not close

`strace` is installed on this machine (MEASURED: `/usr/bin/strace`; `ptrace_scope` is 1, so a tracer can follow its own children). During the run the engine is started as the child of `strace -f -e trace=network,openat -o FILE` where the strace is itself the confined child (the confinement is inherited by the engine and its worker; `FILE` is in the scratch directory that the confinement may write). Every `socket`, `connect`, `sendto` and every `openat` that failed with `EACCES` or went outside the fixtures and the system libraries is reported: what the engine tried, not only what it was allowed. The measurement note's first screen says in plain words **what the fence does not close**: abstract Unix sockets (the owner's X server listens on one), UDP, signals to other processes of the same user (and `/proc` of other processes, never opened by the policy), with the residual risk.
