"""Copy files onto a Jukebox2 disk image the way a PC does: over (emulated) USB/MTP.

python3 -m c55x_emu.addmusic zen_disk.img ../mp3/*.mp3

Boots Jukebox2 on the image, plugs in the USB cable, lets the firmware's own MTP
responder receive each file (SendObjectInfo + SendObject, usbhle.py), unplugs and gives
the firmware time to finish. The image is changed in place; keep a copy. Only the file
name is sent as metadata (the firmware stores it as the title).
"""

import argparse
import sys
from pathlib import Path

from .disk import Disk
from .loader import load_program
from .machine import Machine
from .png import write_png
from .registers import EmulationError
from .usbhle import FORMAT_MP3, RESPONSE_OK, MtpHost, usb_cable

FORMATS = {".mp3": FORMAT_MP3}
STEP = 1.0                              # emulated seconds per check


def run_until(m, done, seconds, what):
    """Run in STEP slices until done() or seconds of emulated time have passed."""
    end = m.seconds() + seconds
    while not done():
        if m.seconds() >= end:
            raise EmulationError(f"{what}: not finished after {seconds} s (pc in {m.where()})")
        start = m.seconds()
        idle = m.run(10**15, until_seconds=min(end, start + STEP))
        if idle and m.seconds() == start and not done():
            raise EmulationError(f"{what}: CPU idle with nothing scheduled (pc in {m.where()})")


def add_files(m, files, timeout):
    """files: [Path] -> [(name, handle)]. Raises unless every response is OK."""
    for f in files:
        if f.suffix.lower() not in FORMATS:
            raise ValueError(f"{f}: only {', '.join(FORMATS)} supported")
    host = MtpHost([(f.name, f.read_bytes()) for f in files])
    host.install(m)
    usb_cable(m, True)
    run_until(m, host.done, timeout, "MTP session")
    usb_cable(m, False)
    bad = [(hex(c), tid) for c, tid, _ in host.responses() if c != RESPONSE_OK]
    if bad:
        raise EmulationError(f"MTP responses not OK (code, transaction): {bad}")
    if host.rx:
        raise EmulationError(f"{len(host.rx)} bytes not taken by the device")
    # transaction ids: 0 OpenSession, then SendObjectInfo (params storage, parent, handle)
    infos = [r for r in host.responses() if r[1] % 2 == 1 and r[1] < host.expected - 1]
    return [(f.name, params[2]) for f, (_, _, params) in zip(files, infos)]


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("disk", help="disk image (changed in place)")
    ap.add_argument("files", nargs="+", type=Path)
    ap.add_argument("--boot-seconds", type=float, default=15.0, help="emulated time before plugging in")
    ap.add_argument("--timeout", type=float, default=600.0, help="emulated seconds for the transfer")
    ap.add_argument("--settle-seconds", type=float, default=15.0,
                    help="emulated time after unplugging (database update)")
    ap.add_argument("--png", help="write the final LCD contents to this PNG file")
    args = ap.parse_args()
    if not Path(args.disk).exists():
        sys.exit(f"{args.disk}: not found (create one with python3 -m c55x_emu.diskimage --format-cfs)")
    disk = Disk(path=args.disk)
    m = Machine(load_program("jukebox2"), disk=disk)
    try:
        m.run(10**15, until_seconds=args.boot_seconds)
        added = add_files(m, args.files, args.timeout)
        m.run(10**15, until_seconds=m.seconds() + args.settle_seconds)
    except EmulationError as e:
        sys.exit(f"stopped at {m.seconds():.1f} s in {m.where()}: {e} (image left as is)")
    finally:
        disk.flush()
    for name, handle in added:
        print(f"added {name} (object handle {handle})")
    print(f"{m.seconds():.1f} s emulated, {disk.writes} sectors written")
    if args.png:
        write_png(args.png, m.lcd.rows(), scale=3)
        print(f"wrote {args.png}")


if __name__ == "__main__":
    main()
