"""Human readable machine status for the front ends (qt_ui.py)."""

# Interrupt numbers of the VC5509A (SPRS205K table 3-40) as the board uses them
IRQ_NAMES = {3: "hard disk", 4: "timer 0 (1 ms tick)", 9: "DMA 1", 12: "McBSP2 receive",
             14: "DMA 4", 15: "DMA 5 (disk data)", 16: "MCU", 18: "DMA 0", 19: "RTC",
             20: "DMA 2", 21: "DMA 3", 22: "timer 1"}
STATES = {"running": "running", "idle loop": "idle, waiting for an interrupt"}


def duration(seconds):
    m, s = divmod(seconds, 60)
    return f"{int(m)} min {s:04.1f} s" if m else f"{s:.2f} s"


def count(n):
    for unit, size in (("G", 1e9), ("M", 1e6), ("k", 1e3)):
        if n >= size:
            return f"{n / size:.1f} {unit}"
    return str(n)


def describe(m, state):
    """-> list of lines: time, CPU state, interrupts, peripherals."""
    cpu = m.cpu
    if cpu.in_idle:
        state = ("powered down (idle with the clock generator stopped; MCU or RTC can wake it)"
                 if m.idle_ctl.clock_stopped() else "idle instruction, waiting for an interrupt")
    lines = [f"Emulated time: {duration(m.seconds())}   (CPU clock {m.clock.mhz():g} MHz, "
             f"{count(cpu.cycles)} cycles)",
             f"State: {STATES.get(state, state)}",
             f"Code: {m.where()}  (pc 0x{cpu.pc:06x})"]
    taken = sorted(cpu.interrupts_taken.items())
    if taken:
        lines.append("Interrupts: " + ", ".join(f"{IRQ_NAMES.get(n, f'#{n}')} {c}x" for n, c in taken))
    else:
        lines.append("Interrupts: none yet")
    lines.append(f"Disk: {m.disk.reads} sectors read, {m.disk.writes} written")
    return lines
