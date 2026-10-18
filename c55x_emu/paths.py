"""Locations of the data the emulator reads but does not ship, all below ROOT.

ROOT is the repository root: ZEN_ROOT if set, else the current directory. Run the emulator
and its tools from the repository root (`python3 -m c55x_emu ...`). The data (output/,
analysis/, build/) may be missing; tests then skip and loaders raise FileNotFoundError.
ZEN_TI_BIN overrides the TI C55x tool directory (default: where cl55 is found in PATH, else
ROOT/build/ti-cgt-c5500-4.4.1/bin); ZEN_CORE_DIR the Rust core directory.
"""

import os
import shutil
import sys
from pathlib import Path


def _root():
    env = os.environ.get("ZEN_ROOT")
    if env:
        return Path(env).resolve()
    here = Path.cwd().resolve()
    if not (here / "scripts").is_dir():
        sys.exit(f"run from the repository root (no scripts/ in {here}) or set ZEN_ROOT")
    return here


ROOT = _root()
CODE_DIR = ROOT / "output" / "code" / "Jukebox2_code_blocks"
FRESC_DIR = ROOT / "output" / "code" / "FRESC_decrypted_code_blocks"
LISTING_DIR = ROOT / "output" / "analysis" / "listing"
SYMBOLS = ROOT / "analysis" / "symbols.json"
HYBRID_DIR = ROOT / "build" / "hybrid"
UPDATE_DIR = (ROOT / "output" / "extracted" / "ZenTouchP4S_PCFW_LF_2_10_05_extracted")


def _ti_bin():
    if os.environ.get("ZEN_TI_BIN"):
        return Path(os.environ["ZEN_TI_BIN"])
    found = shutil.which("cl55") or shutil.which("dis55")
    if found:
        return Path(found).resolve().parent
    return ROOT / "build" / "ti-cgt-c5500-4.4.1" / "bin"


TI_BIN = _ti_bin()
# the extension built by rust/build.sh (default: build/py next to this package's repo)
CORE_DIR = Path(os.environ.get("ZEN_CORE_DIR")
                or Path(__file__).resolve().parents[1] / "build" / "py")
EXAMPLES_DIR = ROOT / "build" / "examples"      # built by examples/build.sh
