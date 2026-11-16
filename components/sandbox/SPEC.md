# lico-sandbox 0.1.0 — specification

Written with the P03 plan (M17-P03); approved by the lead under the owner's delegation (the owner away), mails 144 to 146. Marks: **KNOWN FROM DOCUMENTATION** (the Landlock kernel interface: `landlock_create_ruleset`, `landlock_add_rule`, `landlock_restrict_self`, the access-right bits of ABI 1 to 5, `PR_SET_NO_NEW_PRIVS`, `close_range`), **MEASURED** (the denials, by the tests below, on the kernel of this machine, Landlock ABI 4).

## Purpose

Run a program as a child with (a) the filesystem limited to the listed paths, (b) TCP connect and bind limited to the listed ports, (c) a cleared environment, a given working directory and optional resource limits, (d) its own session, dying with its parent, no descriptor of the parent, no new privileges. **It fails closed**: if the kernel cannot give what was asked, the program is not started. Std only; the system calls are declared directly (as `procs.rs` of the program does for `prctl`).

## API (`tests/contract.rs` pins the names)

| Name | Meaning |
|---|---|
| `Confinement { read_exec, read, read_write, connect_ports, bind_ports, env, cwd, limits, min_abi }`, `Default` | what the child may do. Every path is absolute, clean (no `.` or `..`), exists, is not `/` and is listed once; a file is allowed as well as a directory (the rights are cut to what a file can have). `read_exec`: read and execute (the program, its libraries); `read`: read only; `read_write`: read, write, create, remove and rename but **not** execute (one scratch directory). `env` is the WHOLE environment of the child. `cwd` must exist. `min_abi` defaults to `MIN_ABI` |
| `Limits { address_space, data, cpu_seconds, open_files }`, all `Option<u64>` | `RLIMIT_AS`, `RLIMIT_DATA`, `RLIMIT_CPU`, `RLIMIT_NOFILE`; `address_space` may not suit a program that reserves virtual memory (an engine does: a measurement says) |
| `confine(&mut Command, &Confinement) -> Result<(), Error>` | checks everything and builds the ruleset in the parent, then configures the command: environment cleared and set, working directory, and the child's system calls between fork and program; the program must be an absolute path under a `read_exec` path; the caller chooses the standard streams and spawns |
| `abi_version() -> Result<u32, Error>` | the Landlock ABI of this kernel; `Unsupported { have: 0 }` when there is none |
| `plan(&Confinement, abi, &dyn Fn(&Path) -> Option<PathKind>) -> Result<Plan, Error>`, `Plan { handled_fs, handled_net, paths, ports }`, `PathRule`, `PortRule`, `PathKind`, `handled_fs(abi)`, `handled_net(abi)` | the pure part: every check and every rule for an ABI, with no system call |
| `Error { Unsupported { have, need }, Path, Port, TooMany, Env, Os(call, errno) }` | `Debug` of a `Confinement` and of an `Error` shows the names of the environment variables and never a value (the token travels there) |

What the child does between the fork and the program, in this order, with system calls only and no allocation: `PR_SET_NO_NEW_PRIVS`; the resource limits; `landlock_restrict_self`; `setsid`; `PR_SET_PDEATHSIG` with `SIGKILL` (the signal comes when the thread that spawned the child ends: a caller that spawns from a thread that ends early must spawn from a long-lived one, as `procs.rs` does); `close_range(3, all, CLOSE_RANGE_CLOEXEC)`.

## The limits (named constants, with their reasons)

| Constant | Value | Reason |
|---|---|---|
| `MIN_ABI` | 4 | the first Landlock ABI with TCP port rules; without them the child could connect anywhere on the machine, so a lower ABI is refused (`Unsupported`), not run with less |
| `MAX_PATHS` | 64 | a real confinement has a dozen rules (system directories, the program's directory, one scratch directory); more is a mistake or a leak |
| `MAX_PORTS` | 16 | the CDP port and one or two ports of fixtures are all a browser tab needs |
| `MAX_ENV` | 32 | a program like an engine needs a token and a few switches; a user's environment is hundreds of entries and is exactly what must not be passed |
| `MAX_ENV_BYTES` | 16 KiB | the same reason, in bytes |

## Invariants

1. **Fail closed**: no Landlock, a lower ABI than `min_abi`, a port rule on an ABI below 4, a path that is relative, has `.`/`..` or a NUL, is `/`, does not exist or is listed twice, too many paths, ports or environment entries, a port 0 or twice, a variable name that is not `[A-Za-z_][A-Za-z0-9_]*` or given twice, a value with a NUL, a `cwd` that is not an existing directory, a program that is not an absolute path under `read_exec`: an `Error` and no process.
2. **What the child cannot do** (each proved by a test that runs a probe as the child, with its control: the same probe allowed succeeds): read a file outside `read_exec`, `read` and `read_write` (`EACCES`); write or make a directory outside `read_write`, or inside a `read_exec` directory; execute anything from `read_write`; connect to a TCP port that is not in `connect_ports` (none listed: no TCP at all); bind one that is not in `bind_ports` (port 0 included); see an environment variable that is not in `env`; keep a descriptor of the parent; get privileges back (`NO_NEW_PRIVS` is 1); stay in the parent's session; outlive its parent; open more files than `open_files` or allocate more than `address_space`.
3. **What it does NOT close, said plainly. A test pins each one, so that a kernel that begins to cover it makes the test fail and this text must be revisited:** UDP (datagrams to loopback and DNS lookups are not stopped); abstract Unix sockets (the owner's X server listens on one, and so may other services; clearing the environment hides `DISPLAY`, it does not stop a program that guesses); Unix sockets by path inside an allowed directory; signals to other processes of the same user (`kill(pid, 0)` succeeds); reading `/proc/PID` of other processes whenever `/proc` is allowed (a policy never allows `/proc` whole: the environment and the command line of another process are readable by the same user); CPU use and the number of processes. A confined program that can read nothing secret has nothing to send, but a program that is hostile may use these doors to reach what is not a file: that residual risk is the owner's to know.
4. The environment of the child is exactly `env`: nothing is inherited, `PATH` included.
5. No thread is started; the crate reads no file except to check the paths of a confinement, and no environment variable of its own process.

## Versioning and budget

`0.1.0` while it has met one program (the engine of M17-P03); semantic after. Budget: the default 256 MiB and 30 s.
