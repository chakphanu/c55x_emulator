#!/bin/sh
# Build the native runner c55x-run (rust/c55x_run, on the core crate rust/c55x_core) and
# place it in build/run. Usage: rust/build.sh
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
export CARGO_TARGET_DIR="$ROOT/build/rust-target"
cargo build --release --manifest-path "$ROOT/rust/Cargo.toml" -p c55x_run
mkdir -p "$ROOT/build/run"
# copy, then rename: running jobs keep the old file (cp over it crashes them)
cp "$CARGO_TARGET_DIR/release/c55x-run" "$ROOT/build/run/c55x-run.new"
mv -f "$ROOT/build/run/c55x-run.new" "$ROOT/build/run/c55x-run"
echo "built $ROOT/build/run/c55x-run"
