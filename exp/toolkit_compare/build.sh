#!/bin/sh
# Build one toolkit experiment: `build.sh NAME` with NAME in egui, iced, slint, or `host` for the session host and the
# reference client. Output goes under $LICO_BUILD_ROOT (default: private/build of this checkout); crates come from
# $CARGO_HOME (default: private/cargo_home of this checkout). Add LOCKED=0 for the first build of a project (creates Cargo.lock).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
name=${1:?usage: build.sh egui|iced|slint|host}
export CARGO_HOME=${CARGO_HOME:-$repo/private/cargo_home}
root=${LICO_BUILD_ROOT:-$repo/private/build}
export CARGO_INCREMENTAL=0
locked=--locked
[ "${LOCKED:-1}" = 0 ] && locked=
case "$name" in
host)
  export CARGO_TARGET_DIR=$root/host_hostd
  cargo build --release $locked --manifest-path "$repo/src/host/hostd/Cargo.toml" --bin lico --bin lico-synth-agent
  ln -f "$root/host_hostd/release/lico" "$root/host_hostd/release/lico-host"
  ln -f "$root/host_hostd/release/lico" "$root/host_hostd/release/lico-candidate-a2"
  ;;
*)
  export CARGO_TARGET_DIR=$root/toolkit_$name
  cargo build --release $locked --manifest-path "$repo/exp/toolkit_$name/Cargo.toml"
  ;;
esac
