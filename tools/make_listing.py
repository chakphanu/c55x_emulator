#!/usr/bin/env python3
"""Disassemble the code blocks of a Jukebox2 image (from export_zen_code.py) into the
listing files the emulator reads: <out>/block_NN_AAAAAAAA.asm, one line per instruction
(address, bytes, text). Needs the TI tools (dis55, asm55, lnk55; ZEN_TI_BIN).

Usage: make_listing.py [--code-dir DIR] [--out DIR]
Defaults: ZEN_ROOT/output/code/Jukebox2_code_blocks -> ZEN_ROOT/output/analysis/listing.
"""
import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from c55x_emu.paths import CODE_DIR, LISTING_DIR  # noqa: E402
from c55x_listing import load_listing  # noqa: E402


def listing_text(items):
    lines = []
    for it in items:
        if not isinstance(it, list):
            lines.append("\t\t\t}" if it == "}" else "")
            continue
        addr, raw, text = it
        body = text if text is not None else ".byte " + ",".join(f"0x{b:02x}" for b in raw)
        lines.append(f"{addr:08x}  {raw.hex():<14} {body}")
    return "\n".join(lines) + "\n"


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--code-dir", type=Path, default=CODE_DIR)
    ap.add_argument("--out", type=Path, default=LISTING_DIR)
    args = ap.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    for block in load_listing(args.code_dir):
        path = args.out / (Path(block["filename"]).stem + ".asm")
        path.write_text(listing_text(block["items"]), errors="surrogateescape")
        print(f"wrote {path}")


if __name__ == "__main__":
    main()
