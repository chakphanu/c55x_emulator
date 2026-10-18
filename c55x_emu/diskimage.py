"""Synthetic Zen Touch disk: partition table (sector 0) with an empty minifs and cfs.

Partition table, as read by the partition code of Jukebox2 (mount 0x00826da9, table
init 0x00826e45, create 0x00826eb7, lookup func_00826fbc). 16-bit words; a long is high
word first; on disk a word is little endian (ATA data order):
  words 0-1   magic 0x4d424c4b ("MBLK")
  words 2-3   sector size, 512 (compared with the drive's sector size)
  words 4-5   next free sector (1 + sizes of the created partitions)
  words 8+12i entry i (2 entries): sectors (long), start (long), name (one character per
              word, up to 7, then 0)
Partition contents:
  minifs  sectors 1 .. 40416 (MINIFS_PART_BYTES 0x13bc000, src/include/zen_minifs.h);
          all zero is an empty file system (minifs_format writes zeros, minifs_mount
          requires the first long to be 0). With an extracted official update
          (scripts/extract_zen_firmware.py) it gets the files the update writes:
          jukebox2.jrm (the CODE/TL chunk with its 8-byte header, as update_apply_ciff
          stores it) and the DATA 'H' files (jukebox2.jrs, unicjkl.nft, splash.jbm,
          devicon.ico, devlogo.png), in update order (minifs_image.py).
  cfs     rest of the disk, left zero (format not reconstructed, open question R17);
          --format-cfs formats it with the original Jukebox2 code (cfsformat.py)
The partition sizes are assumptions: the main program never creates partitions.

python3 -m c55x_emu.diskimage OUT.img [--sectors N] [--format-cfs] writes a sparse image.
"""

import argparse
import json
from pathlib import Path

from . import minifs_image
from .disk import SECTOR, ZEN_TOUCH_20GB, Disk
from .paths import UPDATE_DIR

MAGIC = 0x4D424C4B
TL_TAG = "a9544c20"
MINIFS_SECTORS = 0x13BC000 // SECTOR
ENTRY_WORDS, FIRST_ENTRY = 12, 8


def partition_table(total_sectors, partitions):
    """partitions: [(name, sectors or None = rest)] -> sector 0 bytes."""
    words = [0] * (SECTOR // 2)

    def put_long(i, v):
        words[i], words[i + 1] = (v >> 16) & 0xFFFF, v & 0xFFFF

    put_long(0, MAGIC)
    put_long(2, SECTOR)
    nxt = 1
    for n, (name, size) in enumerate(partitions):
        if len(name) > 7:
            raise ValueError(f"partition name '{name}' longer than 7 characters")
        size = size if size is not None else total_sectors - nxt
        base = FIRST_ENTRY + ENTRY_WORDS * n
        put_long(base, size)
        put_long(base + 2, nxt)
        for i, ch in enumerate(name):
            words[base + 4 + i] = ord(ch)
        nxt += size
    if nxt > total_sectors:
        raise ValueError("partitions exceed the disk")
    put_long(4, nxt)
    return bytes(b for w in words for b in (w & 0xFF, w >> 8))


def update_files(update_dir=UPDATE_DIR):
    """[(minifs name, bytes)] the official update writes to the disk, in update order."""
    update_dir = Path(update_dir)
    manifest = json.loads((update_dir / "manifest.json").read_text())
    ciff = (update_dir / "firmware.ciff").read_bytes()
    files = []
    for e in manifest["entries"]:
        if e["tag_hex"] == TL_TAG:
            start = e["archive_offset"]
            files.append(("jukebox2.jrm", ciff[start:start + 8 + e["size"]]))
        elif e["filename"].startswith("H"):
            files.append((e["filename"][1:], (update_dir / e["filename"]).read_bytes()))
    return files


def synthetic_disk(sectors=ZEN_TOUCH_20GB, path=None, files=None):
    """files: [(name, bytes)] for minifs; None = the official update if extracted."""
    if files is None:
        files = update_files() if (UPDATE_DIR / "manifest.json").exists() else []
    disk = Disk(sectors, path)
    disk.write(0, partition_table(sectors, [("minifs", MINIFS_SECTORS), ("cfs", None)]))
    minifs_image.write(disk, 1, files, MINIFS_SECTORS * SECTOR)
    disk.writes = 0
    disk.files = [name for name, _ in files]
    return disk


def open_disk(path):
    """An existing image file, or a new synthetic one (unformatted cfs) at path."""
    return Disk(path=path) if Path(path).exists() else synthetic_disk(path=path)


def main():
    ap = argparse.ArgumentParser(description="write a synthetic Zen Touch disk image (sparse)")
    ap.add_argument("out")
    ap.add_argument("--sectors", type=int, default=ZEN_TOUCH_20GB)
    ap.add_argument("--format-cfs", action="store_true",
                    help="format the cfs by running the Jukebox2 format code (slow)")
    args = ap.parse_args()
    disk = synthetic_disk(args.sectors, args.out)
    cfs = "empty"
    if args.format_cfs:
        from .cfsformat import format_cfs      # imports the machine, which imports us
        cfs = f"formatted, {format_cfs(disk)} sectors written"
    disk.flush()
    print(f"wrote {args.out}: {args.sectors} sectors, minifs 1..{MINIFS_SECTORS} "
          f"({', '.join(disk.files) or 'empty'}), cfs after it ({cfs})")


if __name__ == "__main__":
    main()
