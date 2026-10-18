"""Locations of the data the emulator reads but does not ship.

ZEN_ROOT names the working tree with output/, analysis/ and build/ (the private
reconstruction repo). Without it the first parent of the current directory that holds
analysis/symbols.json is used; if there is none, ZEN_ROOT is the current directory and
every file below is simply missing (tests skip, loaders raise FileNotFoundError).
ZEN_TI_BIN overrides the TI C55x tool directory (default ~/ti-cgt-c5500-4.4.1/bin, else
ZEN_ROOT/build/ti-cgt-c5500-4.4.1/bin); ZEN_CORE_DIR the Rust core directory.
"""

import os
from pathlib import Path


def _root():
    env = os.environ.get("ZEN_ROOT")
    if env:
        return Path(env).resolve()
    here = Path.cwd().resolve()
    for d in (here, *here.parents):
        if (d / "analysis" / "symbols.json").exists():
            return d
    return here


ROOT = _root()
CODE_DIR = ROOT / "output" / "code" / "Jukebox2_code_blocks"
FRESC_DIR = ROOT / "output" / "code" / "FRESC_decrypted_code_blocks"
LISTING_DIR = ROOT / "output" / "analysis" / "listing"
SYMBOLS = ROOT / "analysis" / "symbols.json"
HYBRID_DIR = ROOT / "build" / "hybrid"
UPDATE_DIR = (ROOT / "output" / "extracted" / "ZenTouchP4S_PCFW_LF_2_10_05_extracted")
_HOME_TI = Path.home() / "ti-cgt-c5500-4.4.1" / "bin"
TI_BIN = Path(os.environ.get("ZEN_TI_BIN")
              or (_HOME_TI if _HOME_TI.exists() else ROOT / "build" / "ti-cgt-c5500-4.4.1" / "bin"))
# the extension built by rust/build.sh (default: build/py next to this package's repo)
CORE_DIR = Path(os.environ.get("ZEN_CORE_DIR")
                or Path(__file__).resolve().parents[1] / "build" / "py")
