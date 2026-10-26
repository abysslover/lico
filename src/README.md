# Source

The product is one executable, `lico`, written in Rust: the session host, the command line, the credential command and the window are
subcommands of it. Its crate is `host/hostd` (build and command line: [host/README.md](host/README.md)); the provider adapters
(`claude`, `codex`) are in `host/adapters`. `lico_measure` holds the measurement, test-runner and inspection helpers (see
[../tests/README.md](../tests/README.md)) and `lico_host` the reference host that the Rust host is checked against. `candidates` and
the host implementations in `host/rust` and `host/c` are the alternatives that M01-P01 and M01-P02 measured; they are not the current
build. No private repository is a build or runtime dependency.
