#!/bin/sh
# Run a prepared firmware in the emulator: scripts/run_firmware.sh WORKDIR [emulator options]
# e.g. scripts/run_firmware.sh firmware-work --seconds 5 --ascii
#      scripts/run_firmware.sh firmware-work --disk zen.img --png lcd.png
# WORKDIR comes from prepare_firmware.sh. Options are those of `python3 -m c55x_emu`;
# --disk FILE uses (or creates) a disk image.
set -eu
W=${1:?usage: scripts/run_firmware.sh WORKDIR [emulator options]}
shift
ROOT=$(cd "$(dirname "$0")/.." && pwd)
[ -d "$W/output/code/Jukebox2_code_blocks" ] || { echo "$W is not prepared (run prepare_firmware.sh)" >&2; exit 1; }
W=$(cd "$W" && pwd)
cd "$ROOT"
ZEN_ROOT=$W exec python3 -m c55x_emu jukebox2 "$@"
