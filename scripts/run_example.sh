#!/bin/sh
# Build an example and run it in the emulator: scripts/run_example.sh NAME [emulator options]
# e.g. scripts/run_example.sh hello_lcd --ascii
#      scripts/run_example.sh hello_mcu --event 0x11 --ascii
#      scripts/run_example.sh hello_audio --seconds 2 --wav out.wav
# Options are those of `python3 -m c55x_emu` (see --help). Needs ZEN_TI_BIN and the Rust
# core (rust/build.sh).
set -eu
NAME=${1:?usage: scripts/run_example.sh <hello_lcd|hello_audio|hello_mcu> [emulator options]}
shift
ROOT=$(cd "$(dirname "$0")/.." && pwd)
"$ROOT/examples/build.sh" "$NAME"
cd "$ROOT"
exec python3 -m c55x_emu "build/examples/$NAME/$NAME.out" "$@"
