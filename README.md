# c55x_emulator

Emulator for the TI TMS320C55x DSP of the Zen Touch player (Jukebox2): a Python machine
model (`c55x_emu/`) on a Rust CPU core (`rust/c55x_core`), with models for the ATA disk,
NOR flash, LCD, audio codec, MCU and USB/MTP. It ships no firmware and no vendor code:
you supply your own updater EXE.

## Requirements

- Python 3 (PyQt5 only for the GUI), a Rust toolchain (cargo)
- TI C55x code generation tools 4.4.1 (`cl55`, `asm55`, `lnk55`, `dis55`), which are not
  redistributable. Set `ZEN_TI_BIN` to their `bin` directory (default
  `~/ti-cgt-c5500-4.4.1/bin`).

## Build

    rust/build.sh                  # builds build/py/c55x_core.so
    python3 -m unittest discover -s tests -t tests

## Run the examples

`examples/` has three small programs that run on the bare emulated hardware:
`hello_lcd` (text on the LCD), `hello_mcu` (MCU protocol and key events) and
`hello_audio` (a tone through the codec).

    scripts/run_example.sh hello_lcd --ascii
    scripts/run_example.sh hello_mcu --event 0x11 --ascii
    scripts/run_example.sh hello_audio --seconds 2 --wav tone.wav

The script builds the example (`examples/build.sh NAME`, output in `build/examples/`) and
starts `python3 -m c55x_emu` with your options. `--help` lists them all (`--png FILE`,
`--steps N`, `--seconds S`, `--trace N`, `--disk FILE`, `--mcu-reg R=V`, `--wav FILE`).

## Run the original firmware from your own updater

Needs the firmware container tools from
[zentouch-reverse-engineering](https://github.com/debauer/zentouch-reverse-engineering)
(clone it next to this repo as `../public`, or set `ZEN_TOOLS`).

The updater EXE is available from
https://github.com/brianpipa/CreativeZen/tree/main/CreativeZenTouch.

    scripts/prepare_firmware.sh ZenTouchP4S_PCFW_LF_2_10_05.exe firmware-work
    scripts/run_firmware.sh firmware-work --seconds 5 --png lcd.png

`prepare_firmware.sh` extracts the updater, decrypts the Jukebox2 program, writes its code
blocks at their CPU addresses and disassembles them with `dis55` (listing for the
emulator). Everything lands in the work directory, which `run_firmware.sh` uses as
`ZEN_ROOT`. Only version 2.10.05 (`ZenTouchP4S_PCFW_LF_2_10_05.exe`) is tested; other
versions have other addresses and need changes in `c55x_emu/loader.py`. Keep the work
directory private: it contains the firmware.

`--disk FILE` attaches a disk image (a missing file is created as a synthetic one).
Without symbols from a reverse engineering project, reports show addresses instead of
function names.

## Environment variables

| Variable | Meaning |
| --- | --- |
| `ZEN_ROOT` | working tree with `output/`, `analysis/`, `build/` (see `c55x_emu/paths.py`) |
| `ZEN_TI_BIN` | TI C55x tools directory |
| `ZEN_CORE_DIR` | directory with `c55x_core.so` (default `build/py`) |
| `ZEN_TOOLS` | checkout of zentouch-reverse-engineering (for `prepare_firmware.sh`) |

## Layout

- `c55x_emu/`: emulator package, `python3 -m c55x_emu IMAGE` is the headless entry
- `rust/`: CPU core and peripherals (Python extension)
- `examples/`: example programs and their minimal hardware headers
- `tools/`: `make_listing.py` (code blocks to listing), `roundtrip_c55x_asm.py`
  (disassembly that reassembles to identical bytes), `c55x_listing.py`
- `scripts/`: the shell scripts above
- `tests/`: unit tests
