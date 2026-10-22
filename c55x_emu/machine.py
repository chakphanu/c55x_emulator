"""A Zen Touch board: bus, CPU, LCD, I2C, MCU, timers and RTC running a loader.Program."""

import hashlib

from .ata import Ata
from .bus import MEM_BYTES, Bus
from .diskimage import synthetic_disk
from .clockgen import USBDPLL, ClockGen
from .codec import ADDR as CODEC_ADDR
from .codec import Codec
from .cpu import Cpu
from .decoder import MAX_CACHED_INSNS, CodeCache
from .i2c import I2c
from .idlectl import IdleControl
from .lcd import Lcd
from .loader import load_coff
from .mcu import ADDR as MCU_ADDR
from .mcu import Mcu
from .nor import Nor
from .norimage import synthetic_nor
from .rtc import Rtc
from .timer import TIMERS, Timer

MCU_IRQ = 16
# All program sections together cannot exceed the 24-bit program address space.
MAX_IMAGE_BYTES = MEM_BYTES


class Machine:
    def __init__(self, program, mcu_regs=None, rtc_now=None, disk=None, nor_image=None):
        """disk: disk.Disk (default: synthetic in-memory disk, diskimage.py);
        nor_image: NOR flash words (default: synthetic, norimage.py)."""
        """program: loader.Program or a path to a COFF executable."""
        if not hasattr(program, "sections"):
            program = load_coff(program)
        self.program = program
        self.image_info = self._image_info(program)
        if self.image_info["total_bytes"] > MAX_IMAGE_BYTES:
            raise ValueError(f"image {program.name}: {self.image_info['total_bytes']} bytes "
                             f"exceeds the program address space ({MAX_IMAGE_BYTES})")
        self.bus = Bus()
        for addr, data in program.sections:
            self.bus.load(addr, data)
        self.lcd = Lcd(self.bus)
        self.code = CodeCache(self.bus, program.insns)
        self.cpu = Cpu(self.bus, self.code, program.symbols)
        self.cpu.pc = program.entry
        self.cpu.bios_idle = program.bios_idle
        self.cpu.poll_loops = program.poll_loops
        self.idle_ctl = self.cpu.idle_ctl = IdleControl(self.bus)
        self.i2c = I2c(self.bus)
        self.mcu = Mcu(lambda: self.cpu.raise_interrupt(MCU_IRQ), mcu_regs)
        self.i2c.attach(MCU_ADDR, self.mcu)
        self.codec = Codec()
        self.i2c.attach(CODEC_ADDR, self.codec)
        self.timers = [Timer(self.bus, self.cpu, base, irq) for base, irq in TIMERS]
        self.time_base = (0.0, 0)          # (seconds, cycle) at the last clock change
        self.clock = ClockGen(self.bus, on_change=self._fold_time)
        self.rtc = Rtc(self.bus, self.cpu, self.clock.mhz, rtc_now)
        self.usb_pll = ClockGen(self.bus, USBDPLL)
        self.disk = disk or synthetic_disk()
        self.ata = Ata(self.cpu, self.disk)
        self.cpu.audio_clock = lambda: (self.codec.clock(), self.clock.mhz())
        self.audio_out = None              # e.g. wav.WavRecorder; samples are dropped without
        self.nor = Nor(self.bus, nor_image if nor_image is not None else synthetic_nor())
        self.hle = {}                      # function address -> handler(machine), see run()

    def _image_info(self, program):
        """Report the loaded firmware image: name, total bytes, section count,
        sha256 of the concatenated section data, and per-section summaries."""
        h = hashlib.sha256()
        total = 0
        sections = []
        for addr, data in program.sections:
            h.update(data)
            total += len(data)
            sections.append({"address": addr, "size": len(data),
                             "sha256_16": hashlib.sha256(data).hexdigest()[:16]})
        return {"name": program.name, "total_bytes": total,
                "section_count": len(program.sections),
                "sha256": h.hexdigest(),
                "sections": sections}

    def resource_report(self):
        """The loaded image plus live sizes of the growable emulator structures."""
        return dict(self.image_info, code_cache_insns=len(self.code.insns),
                    code_cache_max=MAX_CACHED_INSNS)

    def run(self, max_steps, stop=frozenset(), until_seconds=None):
        """-> True when the CPU reached an idle loop (cpu.run; stop: pcs to halt at;
        until_seconds: emulated time limit, converted with the current CPU clock).
        Functions in self.hle are replaced by Python: on entry the handler runs (it reads
        the arguments, sets the result and returns with cpu.core.ret()); max_steps then
        applies to each stretch between two handler calls."""
        until = None
        if until_seconds is not None:
            until = self.cpu.cycles + max(0, round((until_seconds - self.seconds()) * self.clock.mhz() * 1e6))
        stops = set(stop) | self.hle.keys()
        while True:
            idle = self.cpu.run(max_steps, stops, until)
            pc = self.cpu.pc
            if idle or pc in stop or pc not in self.hle:
                break
            self.hle[pc](self)
            if until is not None and self.cpu.cycles >= until:
                break
        audio = self.cpu.core.take_audio()
        if audio and self.audio_out is not None:
            self.audio_out.feed(audio)
        return idle

    def seconds(self):
        """Emulated time: cycles converted with the CPU clock in effect (clockgen.py)."""
        secs, cycle = self.time_base
        return secs + (self.cpu.cycles - cycle) / (self.clock.mhz() * 1e6)

    def _fold_time(self):
        self.time_base = (self.seconds(), self.cpu.cycles)

    def where(self):
        return self.program.where(self.cpu.pc)
