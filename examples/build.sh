#!/bin/sh
# Build an example program: examples/build.sh hello_lcd|hello_audio|hello_mcu
# -> build/examples/<name>/<name>.out (+ .map, .dis)
# Needs the TI C55x code generation tools 4.4.1 (cl55, asm55, lnk55, dis55); ZEN_TI_BIN is
# their bin directory (default: where cl55 is found in PATH).
set -eu
NAME=${1:?usage: examples/build.sh <hello_lcd|hello_audio|hello_mcu>}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
TI=${ZEN_TI_BIN:-$(dirname "$(command -v cl55 || echo "$ROOT/build/ti-cgt-c5500-4.4.1/bin/cl55")")}
EX=$ROOT/examples
OUT=$ROOT/build/examples/$NAME
[ -x "$TI/cl55" ] || { echo "cl55 not found in $TI (set ZEN_TI_BIN)" >&2; exit 1; }
[ -d "$EX/$NAME" ] || { echo "no example $NAME" >&2; exit 1; }
mkdir -p "$OUT"
cd "$OUT"
OBJS=""
for SRC in "$EX/$NAME"/*.c "$EX"/common/*.c; do
    OBJ=$(basename "$SRC" .c).obj
    "$TI/cl55" -q --memory_model=large -O2 -vcpu:2.2 --symdebug:none \
        --include_path="$EX/common" --include_path="$EX/include" --include_path="$TI/../include" \
        -c "$SRC" --output_file="$OBJ"
    OBJS="$OBJS $OBJ"
done
for SRC in "$EX"/common/*.asm; do
    OBJ=$(basename "$SRC" .asm).obj
    "$TI/asm55" -q --memory_model=large "$SRC" "$OBJ" > "$OBJ.log" 2>&1 || { cat "$OBJ.log"; exit 1; }
    OBJS="$OBJS $OBJ"
done
"$TI/lnk55" -q $OBJS "$EX/common/fw.cmd" -o "$NAME.out" -m "$NAME.map"
"$TI/dis55" -q -r -g "$NAME.out" > "$NAME.dis"
echo "built $OUT/$NAME.out"
