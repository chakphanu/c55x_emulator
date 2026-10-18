"""Build a minifs partition (src/include/zen_minifs.h) holding given files.

Byte offsets from the partition start: 0x1000 map-slot bitmap (long), 0x2000 block maps
(6152 bytes: long w0, 3072 cluster words ending with 0xffff, long count), 0x4b000
directory bitmap (long), 0x4c000 directory (48 entries of 48 bytes: 16 name characters,
one per word, word 15 = 0; longs w10 = 1, size, dir_index, map_slot), 0x4d000 cluster
bitmap (longs, bit c & 31 of long c >> 5), 0x4e000 data in 4096-byte clusters. The first
long of the partition is 0 (valid). Files get directory index = map slot = their
position and consecutive clusters, as file_open("w") + file_write would allocate them
on an empty file system.

The firmware sees memory words; a file is stored big endian per word (byte_get: even
index = high byte). Words reach the disk through the ATA data register, low byte first
(ata.py), so every byte pair is swapped on disk.
"""

import struct

from .disk import SECTOR

SLOTS, MAPS, MAP_BYTES = 0x1000, 0x2000, 6152
DIRBITS, DIR, ENTRY_BYTES = 0x4B000, 0x4C000, 48
BITMAP, DATA, CLUSTER = 0x4D000, 0x4E000, 4096
MAX_FILES_MARKABLE = 32                 # slots 32..47 cannot be marked (32-bit bitmaps)
MAX_CLUSTERS_PER_FILE = 3072
BITMAP_CLUSTERS = 640 * 8


def swap_pairs(data):
    if len(data) % 2:
        data += b"\0"
    out = bytearray(len(data))
    out[0::2], out[1::2] = data[1::2], data[0::2]
    return bytes(out)


def entry(name, size, index):
    if len(name) > 15:
        raise ValueError(f"minifs name '{name}' longer than 15 characters")
    words = [ord(c) for c in name] + [0] * (16 - len(name))
    return struct.pack(">16H4L", *words, 1, size, index, index)


def block_map(clusters):
    if len(clusters) >= MAX_CLUSTERS_PER_FILE:
        raise ValueError("file larger than one block map")
    words = clusters + [0xFFFF] + [0] * (MAX_CLUSTERS_PER_FILE - len(clusters) - 1)
    return struct.pack(">L3072HL", 0, *words, len(clusters))


def build(files, partition_bytes):
    """files: [(name, bytes)] -> {partition byte offset: memory-order bytes}; the metadata
    area (0 .. DATA) is one chunk, every file one cluster-aligned chunk."""
    if len(files) > MAX_FILES_MARKABLE:
        raise ValueError("more files than markable directory slots")
    meta = bytearray(DATA)
    chunks, used, next_cluster = {}, 0, 0
    bitmap = [0] * (BITMAP_CLUSTERS // 32)
    for i, (name, data) in enumerate(files):
        n = (len(data) + CLUSTER - 1) // CLUSTER
        clusters = list(range(next_cluster, next_cluster + n))
        next_cluster += n
        if DATA + next_cluster * CLUSTER > partition_bytes or next_cluster > BITMAP_CLUSTERS:
            raise ValueError("files do not fit into the minifs partition")
        for c in clusters:
            bitmap[c >> 5] |= 1 << (c & 31)
        if clusters:
            chunks[DATA + clusters[0] * CLUSTER] = data
        meta[MAPS + i * MAP_BYTES:MAPS + (i + 1) * MAP_BYTES] = block_map(clusters)
        meta[DIR + i * ENTRY_BYTES:DIR + (i + 1) * ENTRY_BYTES] = entry(name, len(data), i)
        used |= 1 << i
    meta[SLOTS:SLOTS + 4] = struct.pack(">L", used)
    meta[DIRBITS:DIRBITS + 4] = struct.pack(">L", used)
    meta[BITMAP:BITMAP + 640] = struct.pack(">160L", *bitmap)
    chunks[0] = bytes(meta)
    return chunks


def write(disk, start_lba, files, partition_bytes):
    """Write the partition through disk.write (sector granularity; all-zero metadata
    sectors are skipped, the disk reads them as zeros anyway)."""
    count = 0
    for off, data in sorted(build(files, partition_bytes).items()):
        raw = swap_pairs(data)
        if off % SECTOR:
            raise ValueError("unaligned minifs chunk")
        for i in range(0, len(raw), SECTOR):
            piece = raw[i:i + SECTOR].ljust(SECTOR, b"\0")
            if any(piece) or off != 0:
                disk.write(start_lba + (off + i) // SECTOR, piece)
                count += 1
    return count
