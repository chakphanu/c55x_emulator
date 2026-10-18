"""Instruction-level TMS320C55x emulator for the Zen Touch (prototype).

Instructions are decoded by TI dis55 (algebraic syntax) and the text is compiled into
Python closures. Only the CPU modes of compiled C code are supported (CPL=1, SXMD=1,
M40=0, SATD=0, FRCT=0, C54CM=0); anything unknown raises instead of guessing.
"""
