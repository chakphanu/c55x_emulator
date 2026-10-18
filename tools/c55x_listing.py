"""Validated dis55 (algebraic) listing of all Jukebox2 CODE records in one address space."""

import json
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from c55x_emu.paths import ROOT  # noqa: E402
from roundtrip_c55x_asm import build, byte_line, disassemble, fill_gaps, strip_pad


CODE_DIR = ROOT / "output" / "code" / "Jukebox2_code_blocks"


def load_blocks(code_dir=CODE_DIR):
    """Return [{index, filename, address, data}] in manifest order."""
    manifest = json.loads((code_dir / "manifest.json").read_text())
    blocks = []
    for entry in manifest["blocks"]:
        data = (code_dir / entry["filename"]).read_bytes()
        if len(data) != entry["payload_size_bytes"]:
            raise ValueError(f"{entry['filename']}: size differs from manifest")
        blocks.append({"index": entry["index"], "filename": entry["filename"],
                       "address": entry["address_field"], "data": data})
    return blocks


def disassemble_block(data, address, work):
    """Disassemble one payload; items are [addr, bytes, text|None] plus '}' / '{dropped' markers."""
    wrap = "\n".join(['\t.sect ".text"'] + [byte_line(data[i:i + 16]) for i in range(0, len(data), 16)]) + "\n"
    wrapped, errors = build(wrap, address, work, "wrap")
    if wrapped is None or strip_pad(wrapped, len(data)) != data:
        raise RuntimeError(f"wrapper roundtrip failed at 0x{address:x}: {sorted(errors)[:5]}")
    items, _ = fill_gaps(disassemble("wrap.out", work), address, data)
    return items


def load_listing(code_dir=CODE_DIR):
    """Return blocks with an added 'items' list (see disassemble_block)."""
    blocks = load_blocks(code_dir)
    with tempfile.TemporaryDirectory(prefix="c55x-listing-") as tmp:
        for block in blocks:
            work = Path(tmp) / block["filename"]
            work.mkdir()
            block["items"] = disassemble_block(block["data"], block["address"], work)
    return blocks


def instructions(items):
    """Only real instruction/raw items, without repeat-block markers."""
    return [it for it in items if isinstance(it, list)]
