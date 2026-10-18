"""PyQt5 panel for the MCU model: inject raw event bytes (front controls: qt_controls.py),
edit registers, show the traffic."""

from PyQt5.QtWidgets import (QComboBox, QGridLayout, QGroupBox, QHBoxLayout, QLabel, QLineEdit,
                             QPlainTextEdit, QPushButton, QVBoxLayout, QWidget)

from .controls import BUTTONS, LABELS
from .mcu import CLASSES, key_code

NAMES = {detail: LABELS[name] for name, detail in BUTTONS.items()}


def parse_byte(text):
    v = int(text, 16)
    if not 0 <= v <= 0xFF:
        raise ValueError(text)
    return v


class McuPanel(QWidget):
    def __init__(self, mcu):
        super().__init__()
        self.mcu = mcu
        self.shown_log = 0
        box = QVBoxLayout(self)
        box.addWidget(self._event_group())
        box.addWidget(self._register_group())
        self.log = QPlainTextEdit()
        self.log.setReadOnly(True)
        self.log.setMaximumBlockCount(500)
        self.log.setMinimumHeight(180)
        box.addWidget(QLabel("MCU traffic"))
        box.addWidget(self.log)

    def _event_group(self):
        group = QGroupBox("Event (class << 4 | detail)")
        grid = QGridLayout(group)
        self.cls = QComboBox()
        for c, text in CLASSES.items():
            self.cls.addItem(f"{c:2d}  {text}", c)
        self.cls.currentIndexChanged.connect(self._relabel)
        grid.addWidget(self.cls, 0, 0, 1, 4)
        self.detail_buttons = []
        for n in range(16):
            b = QPushButton()
            b.clicked.connect(lambda _, n=n: self.send((self.cls.currentData() << 4) | n))
            grid.addWidget(b, 1 + n // 4, n % 4)
            self.detail_buttons.append(b)
        raw = QHBoxLayout()
        self.raw = QLineEdit()
        self.raw.setPlaceholderText("raw byte, hex (e.g. 11)")
        self.raw.returnPressed.connect(self._send_raw)
        send = QPushButton("Send")
        send.clicked.connect(self._send_raw)
        raw.addWidget(self.raw)
        raw.addWidget(send)
        grid.addLayout(raw, 5, 0, 1, 4)
        self._relabel()
        return group

    def _register_group(self):
        group = QGroupBox("Registers (hex, read by 'select + 2-byte read')")
        grid = QGridLayout(group)
        for r in sorted(self.mcu.regs):
            edit = QLineEdit(f"{self.mcu.regs[r]:04x}")
            edit.editingFinished.connect(lambda r=r, e=edit: self._set_reg(r, e))
            grid.addWidget(QLabel(f"reg {r}"), r // 2, (r % 2) * 2)
            grid.addWidget(edit, r // 2, (r % 2) * 2 + 1)
        return group

    def _relabel(self):
        cls = self.cls.currentData()
        for n, b in enumerate(self.detail_buttons):
            ev = (cls << 4) | n
            key = key_code(ev)
            name = NAMES.get(n) if 1 <= cls <= 3 else None
            b.setText(f"{ev:02x}" + (f"  key {key}" if key is not None else "")
                      + (f" {name}" if name else ""))

    def _set_reg(self, reg, edit):
        try:
            v = int(edit.text(), 16) & 0xFFFF
        except ValueError:
            edit.setText(f"{self.mcu.regs[reg]:04x}")
            return
        self.mcu.regs[reg] = v
        edit.setText(f"{v:04x}")

    def _send_raw(self):
        try:
            self.send(parse_byte(self.raw.text().strip()))
            self.raw.clear()
        except ValueError:
            self.raw.selectAll()

    def send(self, event):
        self.mcu.inject(event)
        self.refresh()

    def refresh(self):
        """Append new log lines (the MCU log is a bounded deque: show what is new)."""
        entries = list(self.mcu.log)
        total = self.mcu.log_count
        new = entries[max(0, len(entries) - (total - self.shown_log)):]
        self.shown_log = total
        for line in new:
            self.log.appendPlainText(line)
