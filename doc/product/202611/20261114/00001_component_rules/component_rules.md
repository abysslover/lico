# Lico component rules: versioned components, explicit contracts, tests that stay inside their component

- Status: draft, awaiting the owner's approval (nothing in it is binding until the owner approves it)
- Version: 0.1.0
- Project: Lico
- Written: 2026-11-14
- Origin: the owner's instruction of 2026-11-14: stop re-running every test for every change; manage the code as components with versions, explicit dependencies, a contract and a spec each; do not re-run a module's own tests when the module has not changed; keep the number of tests and the memory of a run under control; write this down before the dependencies grow further. The method is the one the owner already uses for the personal backend (`refactor`, decision D35): divide and conquer, interface specification first, release and pin.

Every figure below is marked MEASURED (a report or a tool run says so), COMPUTED (derived here from the source with a rough script, to be redone in Phase 0) or PROPOSED (a rule or a number for the owner to accept or change).

## 1. Why now

- One crate, 64 top-level modules, about 61,500 lines of Rust under `src/host/hostd/src` (COMPUTED), plus about 33,900 lines of Python (187 files). About 667 Rust unit tests (`#[test]`, COMPUTED) and 539 Python tests (MEASURED, M13 reports). Every phase ends with the whole cold suite (about 115 s) and that run has grown with the code.
- The cold suite peaked at 1,998.7 MiB of the 2,048 MiB limit in M12 (MEASURED). M13-P01 brought it to 1,422..1,521 MiB by making single tests lighter, not by changing the structure (MEASURED); the next features push it up again.
- The modules are tangled (COMPUTED, rough count of `crate::<module>` uses; one module's uses of another's name, not yet a proper import graph): `core` is used by 41 modules and itself uses 25 (a hub in cycles), `session` is used by 35, `folders` by 22, `transfer` by 20, `handoff` by 18, `codex` by 17. `gui` is 26,097 lines (42% of the code) and uses 19 modules. A change in a hub can break anything, so every phase must test everything. That cost grows with every feature.

## 2. The rules (PROPOSED)

1. **A component is the unit of change, of version and of test.** It has: a name; one sentence of purpose (what it owns and what it does not); a version `major.minor.patch`; a public API; a list of the components it depends on, each with the version it was built against; a contract (the spec of its public API, invariants and errors); its own tests; and a test receipt (rule 5). Everything that is not in the public API is private to the component.
2. **Dependencies are explicit, downward only, with no cycles.** A component names what it uses in its manifest and may use nothing else. Where a lower component needs something from a higher one, the lower one owns a port (a trait it defines) and the higher one implements it. A dependency on `core` as a hub is not allowed in the target state: `core` is dissolved into components and ports.
3. **Versions mean something.** Major: the contract breaks. Minor: the contract grows and nothing existing changes. Patch: nothing in the contract changes. A user of a component depends on a version; a new version is taken on purpose by changing that dependency, never by accident. A move and a behaviour change never share a commit.
4. **A component is tested thoroughly while it is built; its users do not test it again.** The component's own tests (unit tests and contract tests, one contract test per public name and per invariant) run when the component or anything it depends on has changed, and only then. A user runs its own tests, which are about its own use of the contract, not about what lies below. When a user takes a new version of a dependency, the user's tests run, not the dependency's.
5. **A test receipt makes "not re-run" checkable.** A receipt is a small record kept with the component: a hash over the component's source and test files, the versions of its dependencies and the toolchain; the result; the count of tests; the time; the peak memory; the date. If the hash computed now equals the receipt's, the tests are not run again and the receipt is cited in the report. If it differs, they run and the receipt is renewed. A reviewer can recompute the hash; a receipt that does not match is not accepted. (Where the receipts live, in the repository next to the component or in a build cache, is an open question in section 6.)
6. **A run has a budget; the phase gate is the sum of what changed.** Each component states the most memory and time its own tests may need (PROPOSED default: 256 MiB and 30 s; a component that needs more says so in its manifest and the owner sees it). A phase's verification is: the components that changed, the components that depend on them (their own tests only), and the few system tests that name them. The whole cold suite is no longer a per-phase gate; it runs at release (rule 8). The 2,048 MiB and 180 s limits of the whole stay as limits of the release run.
7. **System tests are few, named and owned by the application.** Behaviour belongs in component tests wherever it can be tested without a display (for the GUI: layout, rows, wording, state transitions as plain functions; a thin painter on top). The tests that start the real binary or a real window are a short named list, each saying which components it covers. They are written in Rust so that they can run on Windows and macOS later; tests that depend on `Xvfb` and `xdotool` are marked Linux-only.
8. **Release acceptance stays whole.** Before a version is installed, the two-clone byte-for-byte build (`suite acceptance` today), all receipts matching, the system-test list, the idle RSS and hidden-growth measurements run once. That is the only whole-suite run.
9. **One executable stays.** Components are crates of one workspace, linked into the one `lico` binary. No dynamic plug-in, no second executable, no change of the idle RSS rule (`< 50 MiB`) or of the one-file, hand-drawn-client principles.
10. **The tools are Rust.** The manifest check, the dependency-graph check (no cycle, only downward edges, only listed dependencies), the hash and receipt tool and the budget measurement are written in Rust inside the workspace, so that the process needs no Python and runs on every platform.
11. **New code follows the rules from the day the owner approves this.** A new feature is a new component (or a new version of one) with its spec written first. A new feature adds no new upward edge into an existing hub; if it needs one, a port is added to the lower component first, as its own change with its own version. The owner's gates G1 to G4, the commit guard and "numbers only as measured" are unchanged.

## 3. What a component looks like (PROPOSED shape)

A workspace crate `components/<name>/` (or the existing names) with `Cargo.toml` (name, version, dependencies with versions), a `SPEC.md` (the table of the `refactor` method: purpose, public API, invariants and errors, dependencies and direction, versioning rule, the list of contract tests, the pattern used), `src/`, `tests/`, and a `RECEIPT.json` (rule 5). A `lico-xtask` crate holds the Rust tools of rule 10. Final names are for the first component's plan to settle.

## 4. How a component is cut (the method of the `refactor` skill, in order)

1. Phase 0, once, read-only: a measured inventory and import graph (function-level and test-only uses counted apart, cycles by strongly connected components, layers from the bottom, fan-in and fan-out), the readers of the repository itself, at least five bottom-up candidates for the first component, and the order of the next six to ten with stop points. The owner picks the first component from that report; nothing moves before.
2. One component per phase: specification first (shown to the owner at G2), pin the present with characterization tests, refactor in small green commits with the smallest fitting pattern (port, facade, adapter, strategy), detach it as its own crate, release and pin, then the independent review and the owner's gates.
3. A cycle that cannot be cut without a behaviour change stops the phase and goes to the owner with three options and a recommendation. The owner can stop after any component.

## 5. First candidates (COMPUTED, rough; Phase 0 redoes this properly)

Bottom of the tree, no dependency on another module of the crate, size in lines, number of modules that use it: `dbus` 452 / 1, `mcp` 437 / 5, `quota` 428 / 5, `payload` 107 / 6, `marks` 226 / 1. One dependency: `png` 1,358 lines (uses `handoff` only), `keyring` (uses `dbus`), `ident`, `lines`, `tokens`, `helper`. The recommendation to be checked in Phase 0: start with `dbus` (smallest, no dependencies, and the planned Linux computer-use work would sit on it), then `payload` and `quota`. The hubs (`core`, `session`, `folders`, `transfer`, `handoff`, `codex`) come last and are dissolved by ports, not moved whole; `gui` is split into a display-free part (rows, layout, wording, state) and a thin painter.

## 6. What the owner decides

1. Whether these rules are accepted as they are or changed (this document is a draft).
2. The default budget of a component run (256 MiB, 30 s are proposals).
3. Where receipts live: next to the component in the repository, so a reviewer sees them in the diff (recommended), or in a local build cache.
4. Whether the first component is `dbus` or another candidate, after the Phase 0 report.
5. The order against the other planned work (the small 0.3.2 release, using a found Local LLM server as a session, a lico host on an ssh host, computer use on Linux, then Windows and macOS, the removal of Python from the product, the measurement tools and the integration tests). The Python removal and this document go together: each component's Python tests are replaced by its Rust contract tests and then retired, and the measurement tools become the Rust tools of rule 10. `exp/` is cleaned up; `lico_chat.py` is the next milestone after that.
