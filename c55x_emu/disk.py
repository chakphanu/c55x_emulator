"""Hard disk contents for the ATA model: 512-byte sectors, sparse.

Without a file the disk lives in memory (unwritten sectors read as zeros). With a path,
sectors are read from and written to that file; a missing file is created as a sparse
file of the full capacity, so only written sectors take space.
"""

import os
from pathlib import Path

SECTOR = 512
ZEN_TOUCH_20GB = 39_070_080             # sectors of a typical 20 GB 1.8" drive (assumption)


class Disk:
    def __init__(self, sectors=ZEN_TOUCH_20GB, path=None):
        self.sectors = sectors
        self.path = Path(path) if path else None
        self.mem = {}
        self.file = None
        if self.path:
            if not self.path.exists():
                with open(self.path, "wb") as f:
                    f.truncate(sectors * SECTOR)
            size = self.path.stat().st_size
            if size != sectors * SECTOR:
                raise ValueError(f"{self.path}: {size} bytes, expected {sectors * SECTOR}")
            self.file = open(self.path, "r+b")
        self.reads = self.writes = 0

    def read(self, lba):
        self._check(lba)
        self.reads += 1
        if self.file:
            self.file.seek(lba * SECTOR)
            return self.file.read(SECTOR)
        return self.mem.get(lba, bytes(SECTOR))

    def write(self, lba, data):
        self._check(lba)
        if len(data) != SECTOR:
            raise ValueError("sector write of wrong size")
        self.writes += 1
        if self.file:
            self.file.seek(lba * SECTOR)
            self.file.write(data)
        else:
            self.mem[lba] = bytes(data)

    def _check(self, lba):
        if not 0 <= lba < self.sectors:
            raise IndexError(f"LBA {lba} outside the disk ({self.sectors} sectors)")

    def flush(self):
        if self.file:
            self.file.flush()
            os.fsync(self.file.fileno())
