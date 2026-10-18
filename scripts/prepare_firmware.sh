#!/bin/sh
# Extract the Jukebox2 program from an updater EXE you supply and prepare it for the
# emulator: scripts/prepare_firmware.sh UPDATER.exe [WORKDIR]
#
# Steps (all in WORKDIR, default ./firmware-work, which becomes ZEN_ROOT for the run):
#   1. roundtrip_zen_touch.py  updater EXE -> container, decrypted parts (Jukebox2.jrm)
#   2. export_zen_code.py      Jukebox2.jrm -> code blocks at their CPU addresses
#   3. make_listing.py         code blocks -> disassembly listing (dis55)
# Needs the firmware container tools from zentouch-reverse-engineering (ZEN_TOOLS, default
# ../public) and the TI C55x tools (ZEN_TI_BIN). Tested with ZenTouchP4S_PCFW_LF_2_10_05.exe.
# The tools need the three public firmware keys in ZEN_TL_KEY, ZEN_FRESC_KEY and
# ZEN_SIGNING_KEY (Rockbox utils/zenutils/source/zen_crypt/main.cpp, entries "Zen Touch";
# see docs/tools.md in zentouch-reverse-engineering).
# Nothing of the firmware is stored outside WORKDIR; do not publish its contents.
set -eu
UPDATER=${1:?usage: scripts/prepare_firmware.sh UPDATER.exe [WORKDIR]}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
W=$(mkdir -p "${2:-firmware-work}" && cd "${2:-firmware-work}" && pwd)
TOOLS=${ZEN_TOOLS:-$ROOT/../public}
TI=${ZEN_TI_BIN:-$HOME/ti-cgt-c5500-4.4.1/bin}
for v in ZEN_TL_KEY ZEN_FRESC_KEY ZEN_SIGNING_KEY; do
    eval "[ -n \"\${$v:-}\" ]" || { echo "$v is not set (public Rockbox key, see docs/tools.md of the tools repo)" >&2; exit 1; }
done
[ -f "$UPDATER" ] || { echo "no such file: $UPDATER" >&2; exit 1; }
[ -f "$TOOLS/scripts/roundtrip_zen_touch.py" ] || { echo "firmware tools not found in $TOOLS (set ZEN_TOOLS)" >&2; exit 1; }
[ -x "$TI/dis55" ] || { echo "dis55 not found in $TI (set ZEN_TI_BIN)" >&2; exit 1; }
[ ! -e "$W/output" ] || { echo "$W/output exists; use a new WORKDIR" >&2; exit 1; }
export ZEN_ROOT=$W ZEN_TI_BIN=$TI
UPDATER=$(cd "$(dirname "$UPDATER")" && pwd)/$(basename "$UPDATER")
python3 "$TOOLS/scripts/roundtrip_zen_touch.py" "$UPDATER" -o "$W/output/roundtrip"
python3 "$TOOLS/scripts/export_zen_code.py" "$W/output/roundtrip/original/Jukebox2.jrm" -o "$W/output/code/Jukebox2_code_blocks"
python3 "$ROOT/tools/make_listing.py"
echo "ready: scripts/run_firmware.sh $W [emulator options]"
