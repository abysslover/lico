# Host implementations (M01-P02 S01)

Three implementations of the protocol v1 core, built to compare size and resident
memory before the host language is decided. They accept the same arguments as the
reference oracle in `src/lico_host` (`--socket`, `--sessions`, `--producing`,
`--rate`, `--history`, `--retain`) and must pass `tests/test_protocol_conformance.py`.
Record payloads are a deterministic function of the sequence number.

| Directory | Build |
|---|---|
| `rust` (standard library only) | `cargo build --release --locked` |
| `rust` with serde_json 1.0.154 (pinned in `Cargo.lock`) | `cargo build --release --locked --features serde` |
| `c` (POSIX threads) | `gcc -Os -s -o lico-host-c main.c -lpthread` |

The Rust crates the serde build uses for Linux are permissively licensed (MIT or
Apache-2.0 or Unlicense). The tests need the candidates built first.

## The owned host (`hostd`)

`hostd` is the Rust host that follows from the language decision: it owns synthetic
agent processes (`lico-synth-agent`, started with a parent-death signal so none
outlives the host), keeps a bounded window of recent records in memory and writes
every record to an owner-only segment log, serves protocol v1 and bounds every
subscriber queue so a slow view is told to resynchronise and never stalls the host.
Build: `cargo build --release --locked` in `src/host/hostd` (serde_json 1.0.154,
pinned in its `Cargo.lock`). Log retention keeps at least `--retain` records,
trimmed in whole segments.
