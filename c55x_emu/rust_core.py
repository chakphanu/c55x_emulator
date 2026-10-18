"""Rust CPU core (rust/c55x_core, built by rust/build.sh into build/py); every Cpu
gets one (cpu.py). An instruction form the core cannot compile is an EmulationError when
it is executed.
"""

import sys

from .paths import CORE_DIR as BUILD


def load():
    if str(BUILD) not in sys.path:
        sys.path.insert(0, str(BUILD))
    try:
        import c55x_core
    except ImportError as e:
        raise ImportError(f"Rust core not built ({e}); run rust/build.sh") from e
    return c55x_core


def attach(cpu):
    """-> the core for cpu (which keeps it as cpu.core)."""
    cpu.code.dropped = []
    return load().Core(cpu)
