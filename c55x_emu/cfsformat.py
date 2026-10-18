"""Format the cfs partition of a disk with the original Jukebox2 code.

The cfs on-disk format is not reconstructed (open question R17), so an empty cfs is made
by the firmware itself: Jukebox2 boots in the emulator until its boot task enters the
cfs mount wrapper (0x008126d0, which calls the mount 0x008125f4), and at that entry a
call to func_00812d95 (open device, format, mount; result flag at data word 0x424032,
0 = ok) is injected. 0x008126d0 is a function entry, so the return address of its own
call is already on the stack; one padding word keeps the stack parity the compiled code
relies on (dbl locals at even SP addresses). When the injected call returns, the run
stops; the registers do not matter any more.
"""

from .registers import EmulationError

MOUNT_WRAPPER, FORMAT, FORMAT_FLAG = 0x8126D0, 0x812D95, 0x424032
MAX_SECONDS = 60                        # emulated time budget (the format needs about 1 s)
SLICE = 1_000_000


def format_cfs(disk, max_seconds=MAX_SECONDS):
    """Run the cfs format on disk (diskimage.synthetic_disk layout); returns the number
    of sectors written. Raises EmulationError if the format is not reached or fails."""
    from .loader import load_jukebox2
    from .machine import Machine

    m = Machine(load_jukebox2(), disk=disk)
    cpu = m.cpu
    before = disk.writes
    injected = False
    stop = frozenset((MOUNT_WRAPPER,))
    while m.seconds() < max_seconds:
        if cpu.pc == MOUNT_WRAPPER:
            if injected:
                flag = m.bus.read(FORMAT_FLAG)
                if flag:
                    raise EmulationError(f"cfs format failed, flag 0x{flag:04x}")
                return disk.writes - before
            injected = True
            cpu.xsp -= 1                        # padding word, see the module comment
            cpu.pc = cpu.call(FORMAT, MOUNT_WRAPPER)
        if m.run(SLICE, stop):
            raise EmulationError(f"cfs format: CPU idle with nothing to wait for ({m.where()})")
    where = "in the format" if injected else "before the cfs mount"
    raise EmulationError(f"cfs format: not finished after {max_seconds} s {where} ({m.where()})")
