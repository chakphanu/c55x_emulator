"""Locations used by the transitional Python binding: ROOT is the repository root
(ZEN_ROOT if set, else the current directory). ZEN_TI_BIN overrides the TI C55x tool
directory (default: where cl55 is found in PATH, else ROOT/build/ti-cgt-c5500-4.4.1/bin);
ZEN_CORE_DIR the Rust core directory.
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
