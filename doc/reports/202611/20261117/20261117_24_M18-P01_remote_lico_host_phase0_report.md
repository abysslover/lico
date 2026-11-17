# M18-P01 Report — Phase 0 for the Remote Lico Host

- Status: implemented (a report only: no code, no connection, no install); waits for the lead and the owner
- Version: 1.0.0
- Branch: `phase/M18-P01-remote-host-phase0` (from the M17-P09 tip `35f01bb`)
- Plan: [M18-P01](../../../plans/202611/20261117/20261117_23_M18-P01_remote_lico_host_phase0_plan.md); milestone: [M18](../../../milestones/202611/20261117/20261117_03_M18_remote_lico_host_milestone.md)
- Labels: **READ** (read in this repository's code today), **MEASURED** (a figure from M16 or M17 or a command run here, read-only), **COMPUTED** (derived from measured figures), **ASSUMED** (not checked; said so). No host name, personal name or path of the owner's machines appears here; hosts are written `HOST`.

## Owner page (one screen)

**Recommendation: build it, with the local host as the only peer of the window and one long-lived `ssh -T HOST <path>/lico host --stdio` link per approved host, in seven phases (about 38 USD, range 30 to 46), after your answer to question 1 (which host first, and who installs).**

1. **What you would see.** Sessions on an ssh host would run under a `lico` process ON that machine and appear in the window like local ones (same list, tabs, cards, approvals), with the host's label. The remote host's sessions keep running when the window closes, as local ones do. File browsing, chats and the look for model servers on that machine would no longer need `python3` there (READ: six remote `python3 -c` scripts today), and would use one open connection instead of a new `ssh` for every click (READ: `ControlMaster=no`, `ControlPath=none`, a new connection per operation).
2. **What gets worse.** A machine must have the `lico` copy installed first, by you approving THAT host; a copy per remote architecture must be built; a remote process exists while there are sessions. Nothing is installed without your approval of the host, and Lico shows the exact steps first.
3. **The transport (recommended): the existing ssh connection, no new listening port anywhere.** `ssh` runs the remote `lico host --stdio` and the protocol (the same JSON lines as today's socket) goes over its standard input and output. A forwarded Unix socket or an HTTP tunnel would each add a listener or a secret on the remote disk; section 3 compares them.
4. **The remote is not trusted.** Everything that comes from a remote host is data: size-capped, depth-capped, cleaned like text from outside, allowed only for the operations and sessions the window asked for. A remote host cannot make the local one run anything, write anything or open anything.
5. **Nothing secret is copied to a remote host by Lico.** Each Claude or Codex account signs in on that machine as it does today; the local keyring stays the only place Lico keeps a secret. A Local LLM session on a remote host talks to loopback ON that host and, in the first version, only to a server that wants no key (section 5).
6. **One decision of yours about the build.** The binary here needs glibc `2.39` (READ: the symbol versions in the release binary), so it will not start on an older machine. Two ways: a static `musl` build of the same crate, or a `headless` flavour of it for hosts (no window code). I recommend one `headless` static build for remotes, about half the size (ASSUMED, to be measured in P03); it is a second flavour of the same crate, not a second program, but the one-executable rule is yours to read.
7. **First small slice (no ssh at all):** P02 proves the link against a local process that pretends to be a remote host (a fake) over pipes: handshake, version check, list, subscribe, caps. About 3.5 USD, then a stop.
8. **Questions for you** (section 8, each with my recommendation): which host first and who installs; the headless flavour; transcripts on the remote disk; keyed model servers; what to do with the six `python3` scripts and when; a remote browser tab (not now).
9. **Cost of this phase:** about 1.5 USD (counter 88.5 to about 90); estimate 3 to 4, stop rule 5.5.

## 1. Goal (item 1)

Sessions on an ssh host (Claude, Codex, and the Local LLM session, which on a remote host means a model server on that host's own loopback) run under a `lico host` process on that machine, speaking the protocol of `protocol_v1.md`, and the window shows them as it shows local ones. Which of today's remote features move:

| Feature today (READ) | Moves to the remote host? | The person's gain or loss |
|---|---|---|
| a Claude or Codex session on a host (one `ssh` per session carries the provider's JSON over its pipes) | yes: the remote `lico host` starts the provider locally on that machine and keeps the session and its log there | the session survives a dropped link (today it ends with the ssh pipe); a reconnect replays from the remote log |
| login of a remote account (`claude auth login`, `codex login` run over `ssh` in the account's own home) | yes, as a remote host operation; the window shows the same URL or code | no change for the person |
| listing and reading folders, the git line of a folder, checking a folder (three `python3 -c` scripts and a `sh` command, a new `ssh` each, 4 s limit) | yes: plain host operations, no `python3` on the remote | faster (one connection), works without `python3`, same caps |
| the chat list of a host (two `python3 -c` scripts in `chats/seed.rs`) | yes | the same list from the remote host's own code |
| the look for model servers on a host (`llm/remote.rs`, one `python3 -c`) | yes: the remote host runs the same look it runs locally | no `python3` needed |
| Local LLM sessions | on the host's loopback only; a server key cannot be used in the first version (section 5) | a remote model server without a key works; one with a key does not yet |
| the browser tab | **not** in this milestone | the engine, its hash, its fence and its network are per machine; a remote browser is a later decision |
| the window itself, the keyring, the accounts' secrets | stay local | nothing moves |

## 2. Today (item 2, READ)

- **How a host is found and probed.** `ssh.rs` reads the owner's ssh configuration (with `Include`, size and depth caps) for host aliases only, validates an alias (`^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$`), and runs every command as `ssh -T -o BatchMode=yes -o ControlMaster=no -o ControlPath=none -o ConnectTimeout=N -- HOST ...` (`ssh_base_command`); the probe asks the remote for `$HOME` and whether `claude` and `codex` exist.
- **What runs on the remote.** A Claude session: `ssh_remote::remote_claude_command` builds one `ssh` that runs `claude -p --input-format stream-json ...` inside `~/.local/state/lico/accounts/ACCOUNT/{home,claude}` (mode `0700`, the account's own home) in the session's folder; a Codex session likewise with `codex app-server`. The local host keeps the pipe, the log and the registry; the remote keeps only the provider process and the account homes. A session without a folder gets `~/.local/state/lico/workspaces/SESSION`.
- **The short remote commands.** `run_ssh_bounded` runs `sh -c ...` over `BatchMode` ssh in its own process group, kills the group at the limit (4 s for most), bounds the output, and maps exit code 255 to `ssh_unreachable`. Three of them are `python3 -c` scripts: `folder_list`, `folder_read` (preview, capped) and `check_remote_folder`; `folder_git` is plain `git` through `sh`. `chats/seed.rs` has two `python3 -c` scripts for the chat list and `llm/remote.rs` one for the model-server look (it stops with `LICO_ERR=no_python3` if `python3` is missing).
- **What the window asks for.** `folder.list`, `folder.read`, `folder.git`, `session.chats`, `llm.discover`, `session.create` and `account.*` with an `ssh_host`; the window shows the result as it shows local ones.
- **What becomes unnecessary** with a remote host: the six `python3 -c` scripts and the `python3` requirement, the per-operation `ssh`, `REMOTE_PATH_EXPORT` and `REMOTE_PATH_SETUP` shell prologues, and `remote_claude_command` and `remote_codex_command` as ssh-pipe builders (the remote host starts the provider itself). The owner's earlier decision stands: they stay until a lico host exists on the ssh host and has been shown to work.

## 3. Transport options (item 3)

| Option | New listener on the remote | Secret on the remote disk | Agent forwarding | Owner-only | What it needs | Verdict |
|---|---|---|---|---|---|---|
| (a) **`ssh -T HOST <path>/lico host --stdio`**: the protocol over the standard streams of one ssh connection the local host opens and keeps | none | none | none (`-a`, `ClearAllForwardings=yes`, `ForwardX11=no`, `BatchMode=yes`) | the connection is the owner's own ssh login; no socket or token exists | a `--stdio` role in `lico host` (the socket server's reader and writer on stdin and stdout: the connection code is the same); the client side | **recommended** |
| (b) ssh forwarding of an owner-only Unix socket (`-L local:remote`) | a Unix socket in a `0700` folder on the remote, for as long as the host runs | none, but the socket's folder must be owner-only | none | by mode `0700` of the folder and of the socket | stale-socket handling (`StreamLocalBindUnlink`), two sockets to manage | acceptable; more moving parts than (a), and a listening socket on the remote |
| (c) an HTTP loopback tunnel to the remote `--http-port` | a TCP port on the remote's loopback, visible to every user of that machine | the bearer token is a file on the remote (today's `--http-token-file`) | none | by the token only | a token file, a port | not recommended: a listener and a secret on the remote disk |

**What the window trusts from a remote: nothing.** (READ for the local rules, ASSUMED for the new ones) text from a remote host is cleaned like any text from outside (the `clean` of M17 and the transcript's own cleaning); every line has a size cap (the local host already caps a request line), JSON depth and key counts are capped, ids and paths are validated by the same shape rules the host applies to its own clients (`valid_str`, `valid_path_shape`), and only replies to requests the local host sent and events of sessions it subscribed to are accepted; an operation that the local host has not asked for (a `clipboard.write`, a `picture.slot`, a `view.attach`, a `subscribe` coming from the remote side) is dropped and counted. A remote cannot reach the local keyring, the local state folder, the clipboard or the window: it speaks to the local host only, and the local host forwards to the window what it would show of a local session.

**One more decision (the shape).** Two shapes are possible: (A) the window talks to several hosts itself, or (B) **the window talks only to the local host, which keeps one link per approved remote host and mirrors that host's sessions as sessions of its own**. I recommend (B): the window, the HTTP door and the CLI do not change, there is one place that approves hosts, one place that caps and cleans, and the remote sessions appear in the local registry as a kind `remote` with the host's label (a hub touch through the driver port of M16, as `local` was, and the operations port of M17).

## 4. Installing the remote host (item 4)

The owner approves EACH host, by name, and Lico never installs on its own. The approval is stored in an owner-only file of the local state folder with the SHA-256 of the binary it was approved for; another binary needs a new approval.

1. **Read-only probe (after the approval of the host, before anything is copied).** `uname -m` and the C library of the remote (`ldd --version`), and whether `~/.local/share/lico` exists: three commands, shown first. The result decides which build to offer: x86_64 or aarch64, glibc or musl.
2. **The build.** MEASURED here: the release binary is `10,505,744` bytes, links `libc`, `libm` and `libgcc_s` dynamically and needs the glibc symbol version `2.39` (READ with `strings`); the window code reaches the display libraries by `dlopen`, so a headless run does not need them. A machine with an older glibc cannot run it. Recommended: a **`headless` flavour** (the window crates behind a cargo feature, `winit`, `softbuffer`, `cosmic-text`, `x11rb` not built) **built static for `musl`**: about half the size (ASSUMED: P03 measures it), no libc dependency, and the same version and protocol. The one-executable rule is about what the person installs on their own machine; the remote copy is the same crate in a second flavour: your reading of the rule is question 3 of section 8.
3. **How it gets there.** `lico remote install HOST` prints the plan and waits for the person's yes: the local file and its SHA-256, the remote folder, the three commands that will run (`mkdir -m 0700`, a copy over the same ssh connection with `cat > file` and `chmod 0700`, the SHA-256 check on the remote), and the command that starts it. The copy lives at `~/.local/share/lico/remote/<first 16 hex of the SHA-256>/lico` (mode `0700`, not on `PATH`); **no `.bashrc`, no shell profile, no `PATH` change, no systemd unit**; Lico runs it by its absolute path. The first time, the owner may prefer to type the three commands by hand: the plan is printable for that.
4. **Version rule.** The remote host's first line is a hello with the protocol number and the build id. A different protocol number is refused with a plain message ("the copy on HOST is for protocol N, this window speaks M: update it?") and the update is the same approved install; the same build id needs nothing. A link never runs a remote copy whose SHA-256 differs from the approved one.
5. **Budgets.** The local side counts against Lico's budgets (50 MiB idle, components, down-only dependencies). The remote copy's idle RSS is ASSUMED near the local host's `8.3 MiB` (MEASURED locally) and is measured in P03 against a local process; a remote host process ends when its last session ends and the link is closed.
6. **Removal.** `lico remote remove HOST` prints and, on a yes, runs `rm -r` of that one folder; the account homes and workspaces under `~/.local/state/lico` are NOT removed (they hold the person's provider sign-ins and files) and the message says so; the approval is dropped locally.

## 5. Credentials (item 5)

- **Nothing secret is copied by Lico to a remote host.** The binary is public code; the approval file is local; no token, key or login is sent. READ: today's remote accounts already sign in on the remote machine inside `~/.local/state/lico/accounts/ACCOUNT/`, and the local host stores only an account label and a host binding; that stays.
- **Claude and Codex accounts on a remote host** sign in on that host as now (`claude auth login`, `codex login`), run by the remote `lico host` and shown by the window through the local host; their tokens stay in the remote account home and are never read by Lico.
- **The local keyring stays the only place Lico keeps a secret.**
- **Local LLM sessions on a remote host** use a model server on loopback ON that host. A server key lives in the local keyring only (LLM-05), so **a remote Local LLM session can use a server that wants no key and, in the first version, cannot use one that does.** Sending a key to a remote host's memory for one session would be a decision of the owner (question 4).
- **Transcripts of a remote session are kept on the remote disk** (its log and the Local LLM record): they hold what the person typed and the answers, never a key. READ: today the provider's own history already lives there; the owner may want to know.

## 6. Components, phases, cost (item 6)

**Components** (rules 1.0.1: a version, a SPEC, contract tests, a receipt, down-only dependencies, a budget of 256 MiB and 30 s unless named):

| Component | What | Depends on |
|---|---|---|
| `lico-link` | the framing of the protocol over a byte stream: lines with a size cap, JSON depth and key caps, the hello and the version rule, request ids, a bounded queue | nothing |
| `lico-link-fake` | a scripted fake remote host (a local process or an in-process pair speaking `lico-link`): sessions, events, failures, a version that differs | `lico-link` |
| `lico-remote` | the client side: the handshake, the allow-list of operations and events, cleaning and caps of everything from the remote, the mirror of a remote host's sessions as data | `lico-link` |
| `lico-install` | pure: the install plan (steps, commands, SHA-256 checks, the folder, the removal) and the probe's reading | nothing |

The adapter `remote/` in the host and the `--stdio` role are program code behind the driver port and the operations port, not components.

**Phases** (tests first; the stop points are the lead's):

| Phase | What | Estimate (USD) | Stop |
|---|---|---|---|
| P02 | `lico-link`, `lico-link-fake` and the first slice: the handshake, the version message, a list, a subscribe and the caps proved against the fake over pipes (no ssh) | 3 to 4 (about 1,400 lines at 1.2 to 1.4 per 1,000, plus mutation checks) | `question` file for the plan, then stop |
| P03 | `lico host --stdio` and the `headless` static build (measured: size, idle RSS); `lico-remote`; the host adapter `remote/` against a REAL local `lico host --stdio` child process on this machine (still no ssh) | 8 to 10 (hub and adapter work at 6.7 per 1,000: about 1,300 lines, plus about 600 of components) | stop: plan and `question` file first (hubs, security surface) |
| P04 | `lico-install`, the approval file, `lico remote install|remove|list`, the probe, the plan printed; the install is proved against a fake remote shell, never a real host | 4 to 5 | no stop after P03's approval |
| P05 | the remote features move: folders, git, chats, the model-server look on the remote host; the six `python3` scripts are NOT deleted yet (they stay until the owner has seen a real host work) | 6 to 8 | stop: the OWNER's first real host (question 1) |
| P06 | sessions on a remote host: Claude and Codex started by the remote host, login through it, a dropped link and a reconnect; Local LLM on the remote loopback (keyless) | 7 to 9 | stop for the owner's check |
| P07 | the six scripts retired (if the owner says so), verification and the release `0.6.0` as M16-P08 and M17-P09 | 4 to 6 | `done`, stop |

**Total about 38 USD (range 30 to 46)**, COMPUTED from the rates measured in M16 and M17 (pure components `1.2` to `1.4` per 1,000 lines, hub and adapter work `6.7`) and from the overruns of M17 (about `1.8` times its Phase 0 estimate): the lower figure is the rates as measured, the upper one adds the overrun I have seen. The ledger stood at `216.66` of `235` when the lead wrote mail 154: **the whole plan does not fit under 235** (`216.66 + 30 = 246`); only P02 and part of P03 do. That is the lead's and the owner's to weigh (the limit above 235 is the owner's).

## 7. What is not known (and how each would be found)

- The size and idle RSS of the `headless` static build: ASSUMED; measured in P03 by building it (no remote needed).
- Whether `ssh -T` with `BatchMode` keeps one connection healthy for hours through the owner's networks and what a reconnect costs: not measurable without a real host; the first real host (question 1) answers it.
- The behaviour of the providers' own CLIs when started by a remote `lico host` instead of by an ssh pipe (their environment, their home): READ from `remote_claude_command` which environment is set today (`HOME` and `CLAUDE_CONFIG_DIR` in the account folder, `LANG`, `NO_PROXY`); the remote host would set the same; verified only in P06 against a real host.
- The ssh configuration of the owner is never read into a tracked file; only the shape (`Host` aliases, `Include`) is used by the code that exists.

## 8. Open questions for the owner (each with a recommendation)

1. **Which host or hosts first, and who installs?** Recommendation: ONE host of your choice that you can reach non-interactively with a key, and that you install on by hand the first time from the plan Lico prints (three commands), so that the first install is yours and Lico only checks it. Name it to the lead; it is not written in any file.
2. **The shape: the window talks only to the local host (B)?** Recommendation: yes (section 3).
3. **A `headless` static build for remotes, as a second flavour of the same crate?** Recommendation: yes; the other way is one fat binary with a glibc floor of `2.39`, which fails on older machines.
4. **A model server with a key on a remote host?** Recommendation: not in the first version (keyless only); revisit after the first host.
5. **Transcripts and logs of remote sessions on the remote disk?** Recommendation: yes, owner-only (`0700`/`0600`), as the providers' own histories already are there.
6. **The six `python3 -c` scripts?** Recommendation: keep them until you have used a real host for a while, then retire them in P07 (the owner's earlier decision).
7. **A remote browser tab?** Recommendation: not in M18.
8. **The budget.** The whole plan is about 38 USD and does not fit under the ledger's 235 from where it stands. Recommendation: approve P02 and P03 only for now (about 12 to 14), then decide with the figures.
