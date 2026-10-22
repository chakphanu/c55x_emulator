"""Memory and I/O space of a TMS320VC5509A.

Program space is byte addressed, data space word addressed (byte = 2 * word, big endian),
both over one 16 MB array. Data words 0x00..0x5f are the CPU memory-mapped registers and
go to the CPU. Peripherals hook word ranges of data space (EMIF devices) or ports of the
I/O space; unhooked ports behave as plain latches and are counted in `unmodeled`.
"""

import collections
import hashlib

MEM_BYTES = 1 << 24
MMR_END = 0x60
MAX_LOAD_BYTES = 1 << 22  # 4 MiB: a single firmware section cannot exceed this


class Bus:
    def __init__(self):
        self.mem = bytearray(MEM_BYTES)
        self.data_hooks = []            # (lo, hi, read(word), write(word, value))
        self.ports = {}                 # port -> (read(port), write(port, value))
        self.latches = {}
        self.unmodeled = collections.Counter()
        self.mmr = None                 # set by the CPU: (read(addr), write(addr, value))
        self.code_pages = {}            # set by decoder.CodeCache: pages holding decoded code
        self.invalidate_code = None
        self.load_log = []              # (byte_addr, size, sha256) per load() call

    def load(self, byte_addr, data):
        if byte_addr + len(data) > MEM_BYTES:
            raise ValueError(f"section at 0x{byte_addr:x} exceeds the address space")
        if len(data) > MAX_LOAD_BYTES:
            raise ValueError(f"section at 0x{byte_addr:x}: {len(data)} bytes "
                             f"exceeds the per-load maximum {MAX_LOAD_BYTES}")
        self.mem[byte_addr:byte_addr + len(data)] = data
        self.load_log.append((byte_addr, len(data),
                               hashlib.sha256(data).hexdigest()[:16]))

    def hook_data(self, lo, hi, read, write):
        self.data_hooks.append((lo, hi, read, write))

    def hook_port(self, port, read, write):
        self.ports[port] = (read, write)

    def _hook(self, word):
        for lo, hi, read, write in self.data_hooks:
            if lo <= word < hi:
                return read, write
        return None

    def read(self, word):
        word &= 0x7FFFFF
        if word < MMR_END:
            return self.mmr[0](word)
        h = self._hook(word)
        if h:
            return h[0](word) & 0xFFFF
        return (self.mem[2 * word] << 8) | self.mem[2 * word + 1]

    def write(self, word, value):
        word &= 0x7FFFFF
        value &= 0xFFFF
        if word < MMR_END:
            self.mmr[1](word, value)
            return
        h = self._hook(word)
        if h:
            h[1](word, value)
            return
        if (word >> 7) in self.code_pages:          # byte page = 2 * word >> 8
            self.invalidate_code(2 * word)
        self.mem[2 * word] = value >> 8
        self.mem[2 * word + 1] = value & 0xFF

    def read32(self, word):
        """Long word: high half at the even address (C55x Lmem)."""
        word &= ~1
        return (self.read(word) << 16) | self.read(word + 1)

    def write32(self, word, value):
        word &= ~1
        self.write(word, value >> 16)
        self.write(word + 1, value)

    def port_read(self, port):
        port &= 0xFFFF
        if port in self.ports:
            return self.ports[port][0](port) & 0xFFFF
        self.unmodeled[port] += 1
        return self.latches.get(port, 0)

    def port_write(self, port, value):
        port &= 0xFFFF
        value &= 0xFFFF
        if port in self.ports:
            self.ports[port][1](port, value)
            return
        self.unmodeled[port] += 1
        self.latches[port] = value
