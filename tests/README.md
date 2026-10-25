# Tests

The suite is `tests/test_*.py` (pytest) and the Rust unit tests (`cargo test --bin lico`). Tests use synthetic fixtures and
meaningful failure conditions: host and window lifetime, snapshot and delta continuity, account isolation, an unavailable keyring,
secret canaries in every sink. Unit, integration and acceptance evidence stay distinct: the **acceptance** group (a clean clone
builds the same executable twice) has its own command and its own triggers; the daily run leaves it out.

## Commands

Run them in the heavy lane (`pri-heavy --label lico --cap 5G --wait 3600 -- …`); `PYTHONPATH=src:.` and `LICO_BUILD_ROOT` are set by the runner.

| | |
|---|---|
| `python -m lico_measure.suite run` | build once (not when the sources hash to the one the binary was built from), then every test file on up to 3 workers |
| `… run --changed [BASE]` | only what `git diff` needs (staged, unstaged, new, and committed since BASE) |
| `… run --receipts` | skip a file whose receipt is current: it prints `skipped (receipt)`; a file that failed last time always runs |
| `… run --cold` | ignore receipts (the cold run is the whole suite with none skipped) |
| `… select --changed [BASE]` | print what a change needs, run nothing |
| `… acceptance` | the clean-clone build; due when a trigger of `testmap.toml` changes, and before a merge or a release |
| `pytest -p lico_measure.ledger --ledger FILE tests` | the ledger: per test wall time and peak memory of the whole process tree |

`budget.toml`: 15 s per test unless an entry gives more with a written reason; the cold run fails over 180 s and the memory over 2 GiB
(all workers together); the summary names every test over its budget. `testmap.toml`: what each test file starts and where each
Rust module runs; a changed file nobody declared selects everything. `size_allowlist.toml`: no `.rs` or `.py` file under `src/` or
`tests/` over 800 lines, except the listed ones, which may not grow.

## Before you add a test

1. **Does a smaller test already cover it?** A unit test in the module, or a test of the host alone, comes before a window test.
   If a stronger test covers a weaker one, delete the weaker one and say which covers it.
2. **Which module does it map to?** Name the Rust modules and Python sources whose change should run it. A test file starts in
   `testmap.toml` under `[areas]` (`gui`, `host`, `cred`, `cli`, `synth`, or none); without that line it runs for every Rust change.
3. **Which class is it, and what does it cost?** Class from the ledger (`python -m lico_measure.ledger_report LEDGER.json`):
   L0 unit (no process), L1 protocol (a host, no display), L2 a window client without a display, L3 end to end on an X server,
   A acceptance (a build or a clone). A new L3 test needs a reason; the ledger gives its seconds and peak MiB. Over 15 s needs an
   entry in `budget.toml`.
4. **Can it share a fixture?** One X server, one host or one input method per module (`captures.ProductWindow`,
   `ime.CompositionRig`) instead of one per test, without making a test depend on the one before it. Wait for a condition
   (the frame counter, a line in the log, a state in the status), never for a fixed sleep.
5. **Is it a main-path test or an exhaustive check?** The suite holds what a change must keep true on the main path. An exhaustive
   differential (millions of lines, every codepoint) stays outside the suite, in `exp/` or a measurement script, and the suite
   keeps a sample of it.

Each test has its own socket, run directory and display (`rig.private_dir()`, `Xvfb` on `-displayfd`); the parallel runner gives each
worker its own client home (`LICO_WORKER`). A test that needs something global (a fixed port, a fixed path under `private/`, a
process found by name) cannot run beside others: make it unique per test.
