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
