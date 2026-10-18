#!/bin/sh
# Build the Rust CPU core (rust/c55x_core) and place the Python extension in build/py,
# where c55x_emu/rust_core.py imports it from. Usage: rust/build.sh
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
export CARGO_TARGET_DIR="$ROOT/build/rust-target"
cargo build --release --manifest-path "$ROOT/rust/c55x_core/Cargo.toml"
mkdir -p "$ROOT/build/py"
# copy, then rename: running emulators keep the old file (cp over it crashes them)
cp "$CARGO_TARGET_DIR/release/libc55x_core.so" "$ROOT/build/py/c55x_core.so.new"
mv -f "$ROOT/build/py/c55x_core.so.new" "$ROOT/build/py/c55x_core.so"
echo "built $ROOT/build/py/c55x_core.so"
