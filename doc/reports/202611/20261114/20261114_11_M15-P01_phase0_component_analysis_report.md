# M15-P01 Report — Phase 0: Measure and Decide (the Component Foundation)

- Status: written, waiting for the owner's choice of the first component (nothing was moved, deleted or built)
- Version: 1.0.0
- Branch: `phase/M15-P01-phase0-component-analysis` (from `main` at `38a319c`, `lico 0.3.2`)
- Plan: [M15-P01](../../plans/202611/20261114/20261114_11_M15-P01_phase0_component_analysis_plan.md)
- Appendix (the long tables): [M15-P01 appendix](20261114_11_M15-P01_phase0_component_analysis_appendix.md)

Every figure is marked **MEASURED** (a tool run on this machine says so), **COMPUTED** (derived from the source text by a script of mine, method given) or **ASSUMED** (my judgement). The scratch scripts live in `private/phase0/` and are not committed; the P02 graph tool in Rust must reproduce the numbers of section 2 before it is trusted.

## Owner page (one screen)

**What the code is** (COMPUTED): one Rust crate, 63 top-level modules and 170 modules in all, 61,965 lines (the window, `gui`, is 26,497 of them), 671 Rust tests; beside it 117 Python test files with 550 tests, 6,533 lines of Python measurement tools and 3.0 MB of `exp/`.

**What the graph says** (COMPUTED from `use` and `crate::` paths, test-only uses counted apart):
- The modules do not form layers. **48 of the 63 modules are in one single dependency cycle** (402 module-to-module edges, 343 of them inside it, 58 pairs that depend on each other both ways). Cutting it takes at least 58 edges (proved: one edge of every two-way pair) and my best cut is 65 edges carrying 192 use sites.
- The six hubs (`core`, `session`, `folders`, `transfer`, `handoff`, `codex`) are hubs because of a few things they hold: `core::Shared`, `find`, `Reply`, `Config` and `random_hex`; `session::Session`, `State`, `obj`, `now_secs`; `folders`'s text rules; `transfer`'s formatting helpers; `codex`'s text helpers; test fixtures (`transfer::world`, `handoff::Scratch`). Moving those out removes about a third of the cycle edges but leaves 43 modules in the cycle: the rest is genuine mutual calling (`core` starts providers and tools, which call back into `core`). So the hubs cannot be moved; they are dissolved by ports, over many steps, from the bottom.
- **Eight modules depend on nothing** (layer 0): `png`, `mcp`, `dbus`, `quota`, `marks`, `payload`, `creds`, `helper` (and the test-only `testgate`). These are where components can start.

**What the tests cost** (MEASURED, quiet host): the Rust tests, each alone, sum to 171 s (about 33 s of that is process start), run together in 14 s; the Python tests sum to 446 s, 124 s on four workers; the window tests are 154 of the 550 and 292 s of the sum; 68 Python tests peak above the 256 MiB default of the rules (all of them window or host system tests: they need a budget class of their own).

**My recommendation for the first component: `png`** (1,361 lines, 3 files). It depends on nothing; its production surface is one name (`PngEncoder`); the only edge to cut is a test-only helper (`handoff::Scratch`, two test files); it has 21 Rust tests of its own, 11.0 s, peak 61 MiB, which no change to its one consumer (`capture`) should ever re-run. It also has two tests that compare against the external `convert` program, which makes it the right first test of the manifest's "external requirement" field. The runners-up: **`dbus`** (453 lines, one consumer, no Rust test at all today: P03 would write its first contract tests against a private `dbus-daemon`, and the planned Linux computer use would sit on it) and **`mcp`** (438 lines, four consumers, 11 stand-alone tests, owns the tool list).

**What I ask you** (details in section 8): (1) which component first (png / dbus / mcp / other); (2) what of `exp/` and of the retired prototypes to delete; (3) the Python retirement order of section 4; (4) a separate budget class for system tests (I propose 512 MiB and 60 s per test; the measured worst of the proposed system-test list is 390 MiB and 27.7 s for one file).

## 0. Method

- **Graph** (COMPUTED): the module tree is built from `mod x;` declarations of `unified.rs` (the root of the `lico` binary) with `#[path]` and `#[cfg(test)]`; comments and string literals are blanked; `use` trees, `crate::`, `super::`, `self::` paths and `alias::item` names of modules imported by `use` are resolved to the longest known module prefix; `#[cfg(test)]` modules and items and whole files declared under `#[cfg(test)]` count as test uses. Edges are kept per module and collapsed to the top-level module for the graph below (`gui` and its 62 submodules are one node there). Cycles by strongly connected components (Tarjan); layers = longest path from a module that depends on nothing outside its own cycle.
- **Checks of the graph** (MEASURED): `crate::` occurs 982 times in the code (988 in the raw text; the 6 others are in comments and strings); a plain regex finds 422 distinct cross-module pairs and the graph holds all of them plus 14 more (paths by `super::`, aliases, the root's bare names); 63 nodes equal the 62 `mod` lines of `unified.rs` plus the root; the 170 modules are the 171 `.rs` files under `src/host/hostd/src` minus `bin/synth_agent.rs`, which is the root of the other binary and not part of `lico`.
- **Uncertainty** (COMPUTED): 4 glob imports other than `use super::*` and 90 `use super::*` (all in test modules, they resolve to the module itself), 1 file with `macro_rules!`. Edges are module level, not function level: a module that calls a function of another counts once however often, and calls through trait objects or method syntax leave no path in the text. The text-based graph can only miss edges, not invent them (spot checks in section 2).
- **Costs** (MEASURED): Rust: `private/measure/rust_per_test.py`, each of the 671 tests alone in its own process, tree RSS (the process and what it started) sampled every 30 ms, on a host with load average below 5. Python: a cold full run with the ledger (`suite run --cold --ledger`): per-test wall time and tree peak. Both on `0.3.2` with no source difference from `main`.

## 1. Inventory

### 1.1 Rust, top level (COMPUTED by `wc` and the graph; one line each; `lines` include the module's own tests)

| module | what it is (first doc line) | files | lines | layer | fan-in prod/test | fan-out | in big cycle | Rust tests |
|---|---|---:|---:|---:|---|---:|---|---:|
| `gui` | The primary client (formerly candidate A2): winit, softbuffer and cosmic-text (shaping, font fallback). | 63 | 26497 | 2 | 1/0 | 16 | no | 217 |
| `clipboard` | The clipboard reader (plan M02-P04, S04a; XF-04): what the owner of a selection (`CLIPBOARD` or `PRIMARY`) hol | 9 | 2437 | 1 | 14/3 | 6 | yes | 27 |
| `chats` | Full-pagination chat discovery (`session.chats`, zero top-N truncation) and | 6 | 1574 | 1 | 7/0 | 9 | yes | 9 |
| `capture` | `lico capture`: the screen, a window or a region of it as a PNG file, and a smaller copy for the agent (plan M | 6 | 1543 | 1 | 6/0 | 2 | yes | 17 |
| `png` | A PNG writer of our own: 8-bit RGB, no interlace, written row by row to any `Write`. | 3 | 1361 | 0 | 1/2 | 0 | no | 21 |
| `llm` | Finding Local LLM servers on this machine (`M12`): which listening TCP sockets answer like an OpenAI-compatibl | 7 | 1352 | 1 | 5/0 | 6 | yes | 18 |
| `provider_record` | The whole conversation of a Claude Code session, read from the program's own record (S05). | 2 | 1167 | 1 | 11/0 | 4 | yes | 40 |
| `call_room` | Persistent inter-session Call Rooms (`M08-P01`): canonical per-pair rooms (`⇄ A · B`), | 2 | 1120 | 1 | 7/1 | 9 | yes | 2 |
| `create` | The tool `create` (`S04c`, extended in `M09-P02`): an agent asks to open a new `Claude` or `Codex` session tab | 2 | 1117 | 1 | 2/0 | 10 | yes | 30 |
| `calls` | The calls of an agent that act, on the tool connection: `send` (S03b, with the person's approval from S04); `c | 2 | 1076 | 1 | 13/2 | 16 | yes | 19 |
| `core` | Host operations shared by the local IPC and the optional loopback HTTP listener. | 2 | 1051 | 1 | 41/14 | 25 | yes | 3 |
| `computer` | The MCP `computer` tool (`CU-02`..`CU-06`, `SF-01`, `SF-03`, `SF-05`, `SF-06`, `XF-07`). | 3 | 1002 | 1 | 8/0 | 17 | yes | 5 |
| `screentools` | The agent tools `capture`, `clipboard_read` and `clipboard_write` on the tool connection, with the person's ap | 4 | 928 | 1 | 5/0 | 12 | yes | 20 |
| `transfer` | One session's text to another (S03a): `transfer.send`. | 2 | 802 | 1 | 15/12 | 10 | yes | 25 |
| `session` | Sessions, durable sequenced logs, and subscriber queues. | 1 | 801 | 1 | 35/5 | 8 | yes | 0 |
| `mention` | `@AgentName` mention parser, session resolver, and recent-context bundler (`M08-P01`). | 1 | 797 | 1 | 5/0 | 9 | yes | 3 |
| `pending` | What an agent asks for that the person must allow first (S04): the request waits in the host's memory, a card  | 1 | 791 | 1 | 12/2 | 6 | yes | 22 |
| `codex` | The Codex app-server adapter: one provider process per session, JSON lines on its | 1 | 759 | 1 | 17/0 | 14 | yes | 1 |
| `folders` | Sessions in a folder the person picked: the checks of that folder, what a person chose for a session (folder, | 1 | 754 | 1 | 22/1 | 15 | yes | 12 |
| `cli` | The `lico` command line: a client of the host's local socket. | 1 | 740 | 1 | 3/0 | 6 | yes | 1 |
| `http` | The optional loopback HTTP listener: the same operations as the local socket | 1 | 707 | 1 | 2/0 | 15 | yes | 0 |
| `tools` | The tool connection of a Claude session: a small JSON-RPC server on the loopback that the program in the sessi | 1 | 703 | 1 | 7/0 | 14 | yes | 14 |
| `helper` | The host side of the helper process (plan M02-P04, S03; XF-07): `run_job` starts `lico xjob <job> [args]` as a | 2 | 694 | 0 | 2/0 | 0 | no | 16 |
| `ssh_remote` | Agentless SSH remote execution helpers for isolated remote account sign-in (`M06-P02`) | 1 | 692 | 1 | 8/0 | 9 | yes | 1 |
| `ssh` | Agentless SSH host discovery (`~/.ssh/config` + `Include`) and bounded non-interactive probing (`M06-P01`). | 1 | 677 | 1 | 7/0 | 4 | yes | 3 |
| `claude` | The Claude Code adapter: one `claude -p` process per session speaking newline-delimited JSON | 1 | 599 | 1 | 7/0 | 17 | yes | 0 |
| `history` | The tool `history` (S07): an agent asks to read the last records of another session's history, which is the ho | 2 | 595 | 1 | 2/0 | 10 | yes | 20 |
| `lines` | The lines a program writes, read as a stream with a bound on what is kept. | 1 | 556 | 1 | 4/0 | 1 | yes | 13 |
| `fsview` | On-demand working-directory listing and bounded regular-file preview (`M04-P04`). | 1 | 512 | 1 | 3/0 | 4 | yes | 4 |
| `ipc` | Protocol v1 over the owner-only Unix socket: strict requests, one reply per | 1 | 504 | 1 | 8/3 | 22 | yes | 0 |
| `vdisplay` | Per-session private virtual X11 displays (`Xvfb`, plan M02-P05 S02; CU-01, CU-04, MB-03). | 1 | 485 | 1 | 8/0 | 4 | yes | 2 |
| `pasted` | The host side of a pasted picture (plan M02-P04, S05a; requirements XF-05, SF-04). | 3 | 466 | 1 | 6/1 | 5 | yes | 11 |
| `inbox_read` | The tool `read_handoff` (S07): a file the host stored in the caller's own inbox (`<handoff dir>/<caller id>`,  | 2 | 463 | 1 | 2/0 | 4 | yes | 11 |
| `dbus` | A minimal D-Bus client over a Unix socket (EXTERNAL authentication, little-endian messages): | 1 | 453 | 0 | 1/0 | 0 | no | 0 |
| `mcp` | JSON-RPC for the tool connection: the messages a program sends to the host over HTTP and what the host answers | 1 | 438 | 0 | 4/2 | 0 | no | 11 |
| `quota` | Per-account provider rate-limit / weekly quota telemetry (`M04-P03`). | 1 | 429 | 0 | 5/0 | 0 | no | 2 |
| `clipwrite` | The host's part of `lico clipboard write` (plan M02-P04, S04b; XF-04, decision L1 of mail 122). | 2 | 405 | 1 | 3/0 | 2 | yes | 11 |
| `leads` | How a history record begins says who wrote it. A record has no kind field: the window styles `>> ` as text a p | 1 | 345 | 1 | 15/1 | 2 | yes | 13 |
| `memory` | Memory per session. The figure a row shows is what the agent and everything it started hold of their own: `Rss | 1 | 335 | 1 | 10/0 | 2 | yes | 4 |
| `login` | Provider sign-in supervised by the host. The provider's own login command runs in the account's private direct | 1 | 316 | 1 | 2/0 | 3 | yes | 0 |
| `handoff` | Long text one session sends another is kept as a file in the receiver's own folder (S03a): | 1 | 294 | 1 | 7/13 | 2 | yes | 6 |
| `worktree` | Git worktree sessions (M02-P06, S01; WT-01, WT-02, SF-07). | 1 | 291 | 1 | 5/0 | 4 | yes | 2 |
| `tokens` | Per-session provider token telemetry (`M07-P01`). | 1 | 281 | 1 | 4/0 | 1 | yes | 2 |
| `accounts` | Accounts: stable local ids, editable labels, a default per provider, and one private home per | 1 | 266 | 1 | 4/6 | 3 | yes | 1 |
| `native` | The provider's own default home as a visible `Native` choice (`M11-P01`). The default home is where Claude Cod | 1 | 240 | 1 | 4/0 | 5 | yes | 4 |
| `host` | The Lico session host (synthetic phase): owns agent processes, their output and | 1 | 231 | 2 | 1/0 | 12 | no | 0 |
| `marks` | Characters that draw nothing on their own, or only on the character before them: the format characters (Cf), t | 3 | 229 | 0 | 1/1 | 0 | no | 6 |
| `registry` | The session registry: which provider sessions exist, their account binding, provider session id and | 1 | 202 | 1 | 15/0 | 5 | yes | 1 |
| `admin` | Administrator account, session, and capability control operations (`M03-P01`, `M03-P04`): | 1 | 200 | 1 | 2/0 | 9 | yes | 2 |
| `search` | On-demand conversation search across one session or all sessions (M02-P06, S03; SR-01, SR-02). | 1 | 192 | 1 | 2/0 | 5 | yes | 1 |
| `credcmd` | `lico-cred`: the credential command line. Secrets come in on standard input and leave only | 1 | 190 | 2 | 1/0 | 2 | no | 0 |
| `new_session` | `session.create`: a new session of a provider (or the synthetic one) for a person or an agent, on an account,  | 1 | 158 | 1 | 3/0 | 7 | yes | 0 |
| `notify` | Desktop notifications for background sessions (M02-P06, S02; NT-02, SF-02, SF-03). | 1 | 156 | 1 | 3/0 | 4 | yes | 1 |
| `echo` | What the history shows of an input the host accepted: the first lines of the text, folded. | 1 | 139 | 1 | 1/0 | 3 | yes | 6 |
| `keyring` | A Secret Service client (the `org.freedesktop.secrets` interface) for credentials Lico owns. | 1 | 139 | 1 | 1/0 | 1 | no | 0 |
| `ident` | Which program this is, said the same way wherever a person can look: the window's title, Help > About, `lico - | 1 | 126 | 1 | 3/1 | 1 | yes | 5 |
| `payload` | Deterministic synthetic record payloads: a pure function of the sequence number. | 1 | 108 | 0 | 4/2 | 0 | no | 2 |
| `creds` | The credential metadata store: opaque references, masked labels and never a secret value. | 1 | 106 | 0 | 2/0 | 0 | no | 1 |
| `testgate` | Test only: a gate that lets a few heavy kinds of test run at the same time and no more. The unit tests run on  | 1 | 97 | 0 | 0/5 | 0 | no | 1 |
| `(root)` | `lico`: the whole product in one executable. The session host, the command line, the credential | 1 | 92 | 3 | 0/0 | 7 | no | 0 |
| `xjob` | `lico xjob <job> [args...]`: the job side of the helper process (plan M02-P04, S03; XF-07). | 1 | 70 | 2 | 1/0 | 3 | no | 2 |
| `lifecycle` | Closing a session: its provider process ends and the session stays, disconnected, with its log, its folder and | 1 | 62 | 1 | 1/0 | 9 | yes | 0 |
| `broker` | The key broker: how a provider gets its account's key without being given the keyring. | 1 | 53 | 1 | 1/0 | 1 | yes | 0 |

Totals: 63 nodes, 170 modules, 61,965 lines. Other Rust in the repository: `src/host/rust` (3 files, 750 lines) and `src/host/c` (1 file, 600 lines) are the older host variants of the October experiment; `src/candidates` (8 files, 3,593 lines of Rust, C and C++) the retired window prototypes; `src/bin/synth_agent.rs` (89 lines) the test double of an agent, `build.rs` (68 lines) the build commit. The full list of the 170 modules is in the appendix.

### 1.2 `gui` (COMPUTED)

63 modules, 26,497 lines: 5,789 lines are tests (4,251 in test files, the rest inline); 2,881 lines are inside drawing functions (functions that take `buf: &mut [u32]`, a lower bound of the painter); 3,278 lines are in files that name none of `winit`, `softbuffer`, `cosmic_text`, `x11rb`; about 17,800 lines (67 %) are neither tests nor drawing: rows, layout numbers, state, wording, events, the JSON dump. Most files name a painter crate only for `Color` (`cosmic_text`) or the key types (`winit`), which a display-free part replaces by two small types of its own. Largest: `gui::ui` 1,095, `gui` 792, `panel` 785, `sidebar` 780, `sidebar_chats` 768, `editor` 722.

### 1.3 Python and the rest (MEASURED by `wc` and `git ls-files`)

| group | files | lines |
|---|---:|---:|
| `tests/` (117 test files, 3 helper modules, 13 fixture files) | 133 | 23,873 |
| `src/lico_measure` (measurement and test tooling) | 35 | 6,533 |
| `src/lico_protocol` (protocol schemas and reference mappings) | 9 | 1,335 |
| `src/lico_host` | 3 | 248 |
| agent simulators `src/host/adapters/{claude,codex}/simulator.py` and their readmes | 5 | 1,077 |
| `ref_shrink.py` (reference for the capture shrinking, run by a Rust test) | 1 | 36 |
| `exp/` (October experiments, toolkit comparisons) | 75 | 34,238 |

## 2. The import graph

### 2.1 Numbers (COMPUTED)

- 63 nodes, **402 production edges** and 77 test-only edges (436 distinct pairs in all; some pairs have both kinds). 2,245 use sites in all (production and test).
- **One strongly connected component of 48 modules** (with the test edges it is 49): `accounts admin broker call_room calls capture chats claude cli clipboard clipwrite codex computer core create echo folders fsview handoff history http ident inbox_read ipc lifecycle lines llm login memory mention native new_session notify pasted pending provider_record registry screentools search session ssh ssh_remote tokens tools transfer vdisplay worktree`; 343 of the 402 edges lie inside it.
- Layers (longest path from a module that depends on nothing outside its own cycle): **layer 0** (depends on nothing): `creds dbus helper marks mcp payload png quota testgate`; **layer 1**: the cycle of 48 and `keyring` (on `dbus`); **layer 2**: `credcmd`, `gui`, `host`, `xjob` (above the cycle: nothing in the cycle uses them); **layer 3**: the root (`unified`, dispatch only).
- **Smallest cut**: 58 two-way pairs force at least 58 edges (each needs one of its two edges cut; PROVED by counting). The best cut I found has 65 edges (heuristic: 300 random orders, each improved by moving single modules) carrying 192 use sites, 44 of those edges and 102 sites touch a hub. The true minimum is between 58 and 65 (exact search is not possible for 48 nodes by subset search).
- **Hubs among themselves** (exact, 6 nodes): their 19 edges among each other need a cut of **6 edges** (16 use sites): `core->codex 5`, `core->folders 2`, `folders->codex 2`, `session->codex 1`, `session->folders 4`, `transfer->codex 2`, with the order, low to high, `session, core, handoff, folders, transfer, codex`. That says: the providers (`codex`, and by the same pattern `claude`) must sit above the hubs and be reached through a trait the hubs own; `folders` (its text rules and `Settings`) must sit below `core` and `session`.

### 2.2 What the six hubs depend on (COMPUTED; production edges, use sites in brackets)

| hub | depends on | used by | what others take from it (most used) |
|---|---|---|---|
| `core` | 25 modules: `session` (16), `payload` 6, `claude` 5, `codex` 5, `registry` 5, `login` 4, `pending` 4, `mention` 3, `worktree` 3, and 16 more | 41 modules | `find` 69, `Shared` 43, `random_hex` 33, `Reply` 28, `build_id` 9, `input` 8, `approval_answer` 8, `OPS` 7, `Config` 5 |
| `session` | 8: `folders` 4, `registry` 4, `calls` 2, `echo` 2, `claude`, `codex`, `payload`, `pending` | 35 | `obj` 27, `State` 23, `now_secs` 21, `Session` 18, `Origin` 12, `encode` 8 |
| `folders` | 15: `worktree` 6, `core` 5, `registry` 3, `codex` 2, `provider_record` 2, `quota` 2, `session` 2, `tokens` 2, and 7 more | 22 | `valid_path_shape` 19, `shows_nothing` 15, `refused_char` 12, `check_folder` 9, `Settings` 9, `valid_name` 7 |
| `transfer` | 10: `core` 8, `session` 7, `handoff` 4, `calls` 3, `provider_record` 3, `codex` 2, `memory` 2, and 3 more | 15 | `id7` 18, `world` 12 (a test fixture), `session_label` 11, `size_text` 11, `send` 7, `take` 7 |
| `handoff` | 2: `registry` 2, `core` 1 | 7 | `Scratch` 23 (a test helper), `mode_of` 7, `store` 5, `sweep` 3 |
| `codex` | 14: `lines` 7, `tools` 6, `quota` 4, `session` 4, `calls` 3, `core` 3, `tokens` 3, and 7 more | 17 | `clip` 10, `sanitize` 9, `dropped` 8, `note` 3, `send_input` 2 |

Reading: each hub offers its consumers a few small things (a lookup, a handle on the shared state, ids, time, text rules, formatting, test fixtures) and a few real operations; it depends on many modules because it also *drives* them (`core` starts the provider processes and tools, which call back).

### 2.3 What-if experiments (COMPUTED; an edge is dropped when every name used through it is in the moved set)

| experiment | edges | the big cycle | two-way pairs |
|---|---:|---:|---:|
| baseline | 402 | 48 modules | 58 |
| the leaf utilities of the hubs moved to a base (`random_hex`, `obj`, `now_secs`, the text rules, `id7`, `size_text`, `hex`, `unhex`, the `lines` values) | 362 | 46 | 52 |
| a port for the shared state (`core::Shared`, `find`, `Reply`, `Config`, `check_generation`, and the `session` types) | 345 | 46 | 42 |
| that and the other hubs reduced to what they offer as values | 301 | 43 | 32 |

The structure is deep: a base and a state port take the edge count from 402 to 301 and still leave 43 modules in one cycle. The remaining cycles are calls (`core` -> `claude`, `codex`, `pending`, `tools`, `mention`, `call_room` -> back into `core`), which need ports owned by the lower component and implemented by the higher one.

## 3. Test ownership and cost

### 3.1 Rust (MEASURED; 671 tests = 670 run + 1 ignored, equal to `cargo test -- --list`)

The owner of a Rust test is the top-level module of its path (`png::tests::...` belongs to `png`). Each test alone: median 0.05 s (process start), 90th percentile 0.72 s, slowest 11.85 s; the sum 171.1 s; only 2 tests peak above 256 MiB (both `gui::fonts`, 271 and 262 MiB) and none runs over 30 s.

| module | tests | sum of single-test times (s) | slowest (s) | highest tree peak (MiB) |
|---|---:|---:|---:|---:|
| `gui` | 217 | 53.0 | 6.72 | 272 |
| `capture` | 17 | 21.6 | 11.85 | 151 |
| `create` | 30 | 16.0 | 1.10 | 15 |
| `pending` | 22 | 11.1 | 1.05 | 17 |
| `png` | 21 | 11.0 | 3.90 | 61 |
| `calls` | 19 | 9.0 | 4.52 | 15 |
| `history` | 20 | 7.9 | 1.08 | 15 |
| `screentools` | 20 | 6.0 | 1.09 | 15 |

(All 40 modules with tests are in the appendix.) The modules with the most tests are not the ones at the bottom: `provider_record` 40, `create` 30, `clipboard` 27, `transfer` 25, `pending` 22, `gui` 217.

### 3.2 Python (MEASURED; 550 tests in 117 files)

Time is the sum of per-test wall times of a cold run (446 s; 124 s on four workers); the peak is the tree of the test (pytest, the host, the window, `Xvfb`, whatever it started). Every file is mapped to the module or modules it exercises in the appendix (READ from the file's docstring, its name and the operations it uses; the testmap's `areas` are only process kinds). The summary by what is exercised (a file counts for every module it names, so the columns overlap):

| exercised | Python tests | time (s) | note |
|---|---:|---:|---|
| `gui` | 154 | 292.4 | 57 files; the window is a process of its own: every one of these tests starts `Xvfb`, a host and a window |
| `host` (host-side system tests) | 53 | 89.6 | 12 files |
| `tools` / `mcp` | 95 / 36 | 24.5 / 19.2 | the tool connection: `tool_connection`, `tool_send`, `tool_create`, `tool_history`, `agent_tools` |
| `helper` | 51 | 11.3 | `clipboard_read`, `computer_use`, `xjob` |
| `lines` | 25 | 11.1 | |
| `accounts` | 12 | 6.5 | |
| `creds` | 10 | 7.6 | |
| `dbus` / `keyring` | 7 | 3.4 | the credential tests through a private keyring (`credential_store`, `accounts`, `account_lifecycle`) |
| `marks` | 19 | 2.5 | `mark_table` 10, `forged_leads` 9 |
| `png` | 27 | 3.0 | `capture_cli` 15, `picture_input` 12 |
| `quota` | 1 | 0.9 | `quota_display` |
| `ident` | 7 | 1.0 | |

68 of the 550 Python tests peak above 256 MiB (the window tests, 140 to 1,094 MiB: `tool_send` is the highest at 1,094 MiB, the host plus a provider double and many processes); none runs over 30 s (slowest 11.5 s). **Consequence**: the default budget of the rules (256 MiB, 30 s) fits every component test and no system test; the system tests need a class of their own (section 7).

### 3.3 Stand-alone share of the candidates (COMPUTED from the test edges; MEASURED costs)

A Rust test is *stand-alone* when its test module has no use of another module (the graph's test edges). `marks`, `mcp`, `quota`, `payload`, `creds`, `ident`: all their tests are stand-alone. `png`: 19 of 21 (two test files use `handoff::Scratch`, a temp-directory helper). `helper`: its 16 tests start `Xvfb` and the real binary (not stand-alone in the sense of the rules, they are system tests of a small kind). `dbus` and `keyring`: no Rust test at all.

## 4. Python: what to do with each group

### 4.1 Groups

| group | what it does | who runs it | needed by the product at run time | Rust replacement | order, effort (ASSUMED) |
|---|---|---|---|---|---|
| `tests/` (117 files) | integration tests of the executable: start the real binary, a private host, `Xvfb`, fake providers, read the window through a JSON dump, `xdotool`, `xprop`, `xclip` | `lico_measure.suite`, the commit hook | no | component contract tests (stand-alone modules), a short Rust list of system tests (7.1) | continuous, with each component; the window tests last (L) |
| `src/lico_measure` (35 files) | the rigs (`rig`, `p02`, `captures`, `ime`, `keyring_rig`, `xpixel`, `fakewm`), the runner (`suite`, `testmap`, `receipts`, `ledger`, `ledger_report`, `procscope`), the October evidence generators (`experiments`, `p02`..`p05`, `bundle*`, `hostcmp`, `hostile`, `legacy`, `decision`, `report`) | tests and the suite | no | `lico-xtask` (runner, receipts, ledger, process scopes: M, P02) and a Rust test kit (rigs: L) | tools first (P02), rigs when the first window test moves |
| `src/lico_protocol` (9 files, 1,335 lines) | protocol v1 schemas, the validators and the reference mappings of the provider adapters | tests (`protocol_conformance`, `reference_mapping`, `model_names`, `llm`, `control_parity`) | no (the host has its own Rust checks; the Python is the second implementation that the tests compare with) | a `protocol` component whose Rust contract tests hold the schemas as data | with the `ipc`/`host` components (M) |
| `src/lico_host` (3 files) | an older host test driver | a few tests | no | retire with the rig port | S |
| agent simulators (`adapters/claude|codex/simulator.py`, 1,077 lines) | stand-ins for the two providers' programs | `claude_adapter`, `codex_adapter`, `recovery`, and every test that starts a session | no | extend `synth_agent.rs` (89 lines, exists) to speak both providers' dialects | before the provider components (M) |
| `ref_shrink.py` | the Python reference for the capture shrinking | the Rust test `capture::tests` (it needs `python3` at test time) | no | a Rust reference implementation or a stored fixture | S, with `capture`/`png` |
| `python3 -c` scripts in `ssh_remote.rs` and `llm/remote_script.rs` | what runs on the REMOTE ssh host (the folder listing, the endpoint scan) | the product, on the remote machine | **yes, on the remote host** (this machine runs no Python for it) | none before a `lico` host exists on the ssh host (the owner's later milestone, installed with approval); until then they stay, with their tests | later (not M15) |
| `exp/` | the October experiments (toolkit comparisons, canaries, listener memory) | nobody (`tests/testmap.toml` has an inert rule for it) | no | none | delete (4.3), question to the owner |

### 4.2 `lico_measure`, file by file (verdict: **K** keep as Rust, **R** replace by Rust test code, **X** retire) — see the appendix for the functions

The runner and the receipts (`suite`, `testmap`, `receipts`, `ledger`, `ledger_report`, `procscope`: 1,263 lines) are **K**, to become `lico-xtask`; `receipts.py` and the `--receipts` option of `suite.py` are already a file-level form of the rules' receipt (a hash of the test file and what it imports, kept per file), the predecessor of `RECEIPT.json`. The rigs used by every host and window test (`rig.py`, the `ProtoConn`, `start`, `read_log`, `cli` part of `p02.py`, `captures.py`, `ime.py`, `keyring_rig.py`, `xpixel.py`, `fakewm.py`) are **K** as a Rust test kit. The October evidence generators (`experiments.py`, the scenario half of `p02.py`, `p03.py`, `p04.py`, `p05.py`, `bundle*.py`, `__main__.py`, `hostcmp.py`, `hostile.py`, `legacy.py`, `decision.py`, `report.py`, `stats.py`, `workload.py`, `providers.py`, `real_turn.py`, `latency.py`) are **X** once the tests that import them are replaced or retired (each of `decision`, `report`, `latency`, `hostile` has one test of its own); `notices.py` (third-party notices of the executable) is **K** as part of the release tooling.

### 4.3 `exp/` (proposal, a question to the owner)

3.0 MB, 75 files, 34,238 lines, nobody runs them; history keeps them. Eleven documents link to `exp/` and three places outside `doc/` cite the record-layout evidence (`src/host/adapters/README.md`, `tests/fixtures/claude_record/README.md`, `tests/README.md`). Proposal: delete `exp/toolkit_*` (the toolkit comparison that ended in the hand-drawn client), `exp/202610/*` canaries and listener measurements, and keep `exp/202610/20261027/00003_record_layout_2_1_311/` (the evidence of the Claude record fixtures) as a single directory with its readme; replace the links in the eleven documents by a note "removed in M15, see git history at `38a319c`". Same question for `src/candidates` (3,593 lines), `src/host/rust` and `src/host/c` (1,350 lines) and their harness (`hostcmp.py`, `legacy.py`, the October half of `experiments.py`).

## 5. Candidates for the first component

All have no dependency on another module in production (layer 0) except where noted. Costs MEASURED, edges COMPUTED, the Python tests READ.

| | `png` | `dbus` | `mcp` | `marks` | `quota` | `payload` | `creds` / `keyring` |
|---|---|---|---|---|---|---|---|
| lines (files) | 1,361 (3) | 453 (1) | 438 (1) | 229 (3) | 429 (1) | 108 (1) | 106 (1) / 139 (1) |
| production dependencies | none | none | none | none | none | none | none / `dbus` |
| used by | `capture` (prod), tests of `capture`, `clipboard` | `keyring` | `tools` 6, `screentools` 3, `computer` 2, `admin` 1 | `leads` 2 | `claude` 4, `codex` 4, `folders` 2, `accounts`, `core` | `core` 6, `calls`, `session`, `ssh_remote` | `credcmd`, `core` / `credcmd` 17 |
| de-facto surface (names others use) | `PngEncoder` (prod); test support: `read_png`, `both_readers`, `outside`, `noise`, `text_like` | `Conn`, `Value`, `Error`, `byte_array`, `wipe` (5) | `Answer`, `TOOLS`, `Block`, `failure`, `WRITE_TOOL`, `Outcome`, `handle_with` (7) | `covers`, `opens`, `RANGES` (3) | `session_key`, `observe_*`, `usage_json`, `forget` (6) | `hex`, `unhex` (2) | `find list add remove random_ref` (5) / `Store Error wipe_secret` |
| edges to cut | test-only: `handoff::Scratch` (2 test files) | none | none | none | none | none | none |
| own Rust tests (time, peak) | 21 (11.0 s, 61 MiB) | **0** | 11 (0.6 s) | 6 (0.5 s) | 2 (0.1 s) | 2 (0.1 s) | 1 / 0 |
| stand-alone share | 19 of 21 (90 %) | n/a | 11 of 11 | 6 of 6 | 2 of 2 | 2 of 2 | 1 of 1 |
| Python tests that exercise it (count, time) | 27 (3.0 s) | 7 (3.4 s, through the keyring) | 36 (19.2 s) | 19 (2.5 s) | 1 (0.9 s) | none | 10 (7.6 s) |
| external requirement | `convert` (ImageMagick), 2 tests | `dbus-daemon` for a test | none | none | none | none | keyring daemon for a test |

Also checked and put aside: `helper` (694 lines, 2 consumers; its tests start `Xvfb` and the real binary: system tests), `ident` (126 lines, but `ident` and `core` depend on each other: `build_id`), `lines` (556 lines, 25 Python tests; one edge to `transfer::size_text`, a text helper that belongs in a base), `tokens` (281; depends on `registry`).

**Recommendation: `png`.** It has the clearest payoff of the layer 0 modules and the cleanest surface: one production name, no production dependency, one test-only edge that is a plain move (a temporary-directory helper into the component's own test support); its 21 stand-alone tests take 11.0 s of the 171 s of Rust test time (6.4 %) and are exactly the kind that a receipt makes unnecessary to re-run: a typical change to `capture` should need none of them and none of their 61 MiB. Its two ImageMagick comparisons make the manifest declare an external requirement honestly. Its 27 Python tests (`capture_cli`, `picture_input`) show the retirement rule on real data. Cost: it is the biggest of the candidates in lines and has the most tests to pin, so P03 is the largest of the layer 0 jobs (ASSUMED: two to three days of work).

**Second: `dbus`**, if the owner wants the Linux computer-use path early. It is small and clean, and its 5 names are all the keyring needs; but it has no Rust test, so P03 would write its first tests (contract tests against a private `dbus-daemon`, which is installed on this machine: EXTERNAL authentication, `Hello`, `ListNames`, `GetNameOwner`; no keyring daemon needed; ASSUMED to be enough, not tried) and has nothing to pin or retire; and what AT-SPI on the accessibility bus would need (signals, match rules, a connection that waits for messages) is not in the client today (it has `call` only), so it would grow by minor versions. **Third: `mcp`**: real stand-alone tests and four consumers, and it owns `TOOLS`, the list whose size budget (5,120 bytes) a version rule would protect; its Python tests (36, 19.2 s) belong to the tools more than to it.

## 6. The build order, `gui`, and the hubs

### 6.1 The next components (PROPOSED; a stop point after each, the owner can stop at any)

| # | component | why here | stop point: what must be true |
|---|---|---|---|
| 1 | `png` (P03 of M15) | section 5 | tools proven on a real component; one receipt; a typical consumer change measured |
| 2 | `dbus`, then `keyring` on it | the credential path; the Linux computer-use base; new tests | the private-bus test pattern works; `credential_store` Python tests reviewed for retirement |
| 3 | `mcp` | the tool list under a version rule | the tool-list byte budget becomes a contract test of the component |
| 4 | `marks` (and `payload`, `quota` as small ones) | stand-alone; `mark_table.py` (10 tests) is a clean retirement case for the mutation-check rule | one Python test file retired with its mutation check shown |
| 5 | `base`: the leaf values the hubs hold (`random_hex`, `obj`, `now_secs`, `hex`, `unhex`, `id7`, `size_text`, `session_label`, the folder text rules `valid_*`, `shows_nothing`, `refused_char`, the `lines` values) | removes about 40 edges from the cycle; the first component that changes the hubs' dependencies | the graph tool shows the cycle smaller; no behaviour change (the whole suite once, as a release check) |
| 6 | `wire`/`state` ports: a trait for the session directory and the host state (`find`, `Shared`, `Reply`, `Config`) owned below `core`, implemented by `core` | the biggest single cut (345 edges, two-way pairs 58 to 42) | consumers (`calls`, `tools`, `pending`, `transfer`, `history`, ...) build without `core` |
| 7 | the provider port: a `Provider` trait below the hubs; `claude` and `codex` above them implement it | exactly the exact cut among the six hubs (`core`/`session`/`folders`/`transfer` -> `codex`) | `core` no longer names `codex` or `claude` |
| 8 | the display-free part of `gui` (6.2), in several components | the window is 43 % of the code and 154 of the 550 Python tests | the first window test replaced by a model test without a display |
| 9 | the file and folder family (`fsview`, `folders` rest, `worktree`, `ssh`, `ssh_remote`) | after `base` and the ports | |
| 10 | `chats`, `native`, `llm` (the newest code, cleanest tests) | the found-server work will sit on `llm` | |

### 6.2 Splitting `gui` (COMPUTED numbers above, design PROPOSED)

`gui-model` (display-free): rows and their wording, sidebar state and paging, layout numbers, the pointer and keyboard state machines, the dialogs' state (the new `llm_rows.rs` and `pathbar.rs` show the pattern: pure functions and a small state, tested without a display), the JSON dump; it uses two small types of its own, `Rgb` and `Key`, instead of `cosmic_text::Color` and `winit` keys. `gui-paint` (thin): `Controls::text/fill`, fonts (`cosmic-text`), the window and the buffer (`winit`, `softbuffer`), the X11 clipboard owner. The shell `gui` wires them. Steps with stop points: (a) the 3,278 lines already free of painter crates move first (`scroll`, `files_host`, `files_render`, `admin_ui`, `llm_rows`, `tool_fold`); (b) `Rgb` and `Key` replace the crate types at the boundary; (c) the `*_render.rs`/`*_nav.rs` pairs (`sidebar_render`/`sidebar_nav`) show where state and drawing already part: split along those lines.

### 6.3 Dissolving each hub (PROPOSED; ports named by the lower owner)

- `core`: `Shared` (sessions, views, accounts, logins, clipboard state, calls) becomes the trait `HostState` (find a session, list, a handle) owned by the state component; the operations (`input`, `approval_answer`, `status`, `OPS`) stay in `core`, which implements the trait and is the only module that names every provider and tool.
- `session`: the record (`Session`, `State`, `Origin`, the log) is separated from its behaviours; the four edges to `folders`/`registry` and the edges to `calls`, `echo`, `pending` become ports (`Settings` reader, `ApprovalSink`, `EchoSink`).
- `folders`: the text rules go to `base` (they are leaf functions); `Settings` and `check_folder` stay; the six edges to `worktree` go with the file and folder family.
- `transfer`: formatting helpers to `base`; the test fixture `world` to a test-support crate (it explains 12 of its 15 consumers' test edges); `send`/`take` stay behind the session port.
- `handoff`: `Scratch` and `mode_of` to a file-utility leaf; `store` and `sweep` stay.
- `codex`: `clip`, `sanitize`, `dropped`, `strip` (text helpers used by `claude` too) to a provider-text leaf; the rest becomes an implementation of the provider port.

## 7. The machinery (PROPOSED design; P02 builds it)

**Layout.** A workspace at the repository root: `members = ["src/host/hostd", "components/*", "tools/xtask"]`; `Cargo.lock` moves to the root and stays committed; `build-release.sh` keeps its `--locked`, builds the one `lico` and remaps the root path (today `$here=/lico`) so that two clones still build the same bytes; the existing crate stays a member and shrinks as components leave it. One executable stays.

**Manifest** (in each component's `Cargo.toml`, `[package.metadata.lico]`; the check refuses a component without it): `purpose` (one sentence), `spec = "SPEC.md"`, `depends = [{ name, version }]` (the only components it may use; downward only), `external = [ "convert", "dbus-daemon" ]` (programs its tests need), `budget = { mib = 256, seconds = 30 }`, `layer` (computed, checked), `contract_tests = [..]`.

**`SPEC.md` skeleton** (the table of the `refactor` method): purpose; public API (each name, SEEN IN CODE or PROPOSED); invariants and errors; dependencies and direction; versioning rule (major: the contract breaks, minor: it grows, patch: nothing in the contract changes); the contract tests, one per public name and invariant; the pattern used (port, facade, adapter, strategy); the external requirements.

**`RECEIPT.json`** (next to the component, in the diff): `{ "component", "version", "hash", "inputs": { "sources": [path, sha256 ...], "tests": [path, sha256 ...], "dependencies": [ { name, version, receipt_hash } ], "externals": [ name, version string ], "lock": digest of the `Cargo.lock` entries of the crates it uses, "toolchain": `rustc -vV` and the target }, "result": { "passed", "failed", "ignored", "count" }, "cost": { "wall_s", "peak_mib", "budget": { "mib", "seconds" } }, "tool": version of the xtask, "date" }`. **The hash covers**, in this order, the sorted (path, SHA-256) pairs of every file under the component's directory except `RECEIPT.json` and build output, the receipt hashes of the components it depends on (so a change below invalidates everything above it, and a version number can never hide a change), the `Cargo.lock` entries of the external crates it uses, the `rustc -vV` text and the target triple. The date and the cost are not in the hash. A receipt is valid when a recomputation gives the same hash and its result is all passed; otherwise the tool says which input changed.

**The isolated run.** `cargo test --no-run -p <component> --target-dir private/target/<component>` first (the compile is measured apart: it uses up to about 1 GiB and is bounded by the build job count, as today), then the test binary run alone with the tree RSS of the process and its children sampled from `/proc` every 100 ms (the method of `ledger.py`/`procscope.py`, ported) and the wall time, against the manifest budget; over budget is a failure of the run, reported with the test that peaked. The default is 256 MiB and 30 s (MEASURED: every Rust test fits, two `gui::fonts` tests are at 262 and 271 MiB and belong to the `gui` family, which will say so in its manifest).

**System tests.** A separate budget class, owned by the application, written in Rust against the real binary (a test kit with the rigs of 4.1). PROPOSED budget: 512 MiB and 60 s per test (MEASURED worst of the candidate list below: 390 MiB for `tool_connection`, 27.7 s for `host_replacement` as a file). Candidate list (14 files, 74 tests, 97 s of summed time, MEASURED): `single_executable`, `client_integration`, `host_replacement`, `first_view`, `chats_end_to_end`, `native_end_to_end`, `llm_end_to_end_m13`, `m14_end_to_end`, `credential_store`, `computer_use`, `clipboard_write`, `tool_connection`, `recovery`, `stale_host`; the subset that stays (ASSUMED: about 25 tests, one flow each) is chosen with the owner at P03/P04 by the mutation checks, and the two-clone acceptance stays.

## 8. What I ask the owner, and honest notes

1. **The first component**: `png` (recommended), `dbus` (strategic), `mcp`, or another of section 5.
2. **What of `exp/` to delete** (4.3), and whether `src/candidates`, `src/host/rust`, `src/host/c` and their harness go too.
3. **The Python retirement order** (4.1): tools into `lico-xtask` first (P02), then component tests as components appear, rigs and simulators when the first window test moves; the remote `python3` scripts stay until a `lico` host exists on the ssh host.
4. **A budget class for system tests** (512 MiB and 60 s per test, section 7).

Honest notes:
- The graph is read from text, not from the compiler; it is checked against a plain regex (section 0) and cannot show calls made by method syntax or trait objects; the P02 tool will be validated against these numbers.
- The minimum cut of the big cycle is bracketed (58 to 65), not exact.
- The Python-to-module mapping is my reading of 117 files; where a test names several modules the table says so and the per-module sums overlap.
- Time and memory of single Rust tests include process start (median 0.05 s); the sum (171 s) is therefore not what a whole run costs (14 s).
- Effort figures are my estimates (ASSUMED). Nothing in the repository was changed except this report, its appendix, the plan notes and the index.
