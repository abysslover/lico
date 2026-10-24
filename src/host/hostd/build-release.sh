#!/bin/sh
# Reproducible build of the single executable: `build-release.sh OUTPUT_DIR` leaves `lico` and `lico.sha256` there.
# Run from a checkout; the crates come from the lock file (set CARGO_NET_OFFLINE=true when they are already cached).
# Source and cache paths are rewritten, so the file does not depend on, or contain, where it was built.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
out=${1:?usage: build-release.sh OUTPUT_DIR}
mkdir -p "$out"
out=$(cd "$out" && pwd)
cargo_home=${CARGO_HOME:-$HOME/.cargo}
sysroot=$(rustc --print sysroot)
export CARGO_INCREMENTAL=0
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$out/target}
export RUSTFLAGS="--remap-path-prefix=$here=/lico --remap-path-prefix=$cargo_home=/cargo --remap-path-prefix=$sysroot=/rust -C link-arg=-Wl,--build-id=none"
cargo build --release --locked --manifest-path "$here/Cargo.toml" --bin lico
cp "$CARGO_TARGET_DIR/release/lico" "$out/lico"
(cd "$out" && sha256sum lico > lico.sha256)
cat "$out/lico.sha256"
