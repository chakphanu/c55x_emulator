"""PyQt5 control logic: the Zen Touch buttons, touch pad strip and Lock switch (controls.py);
qt_device.py draws them as the player's front.

Buttons send press, repeated hold events while held and the release. The touch strip:
upper / lower quarter = fast scrolling while pressed, middle = drag to scroll (speed sets
the level), a short click without movement = tap, mouse wheel = one entry per notch.
Keyboard: Up/Down scroll, Enter OK, Backspace/Esc Back, M Menu, R Random, Space
Play/Pause, Left/Right Previous/Next, +/- volume, P Power, L Lock.
"""

import time

from PyQt5.QtCore import QRect, QRectF, Qt, QTimer
from PyQt5.QtGui import QColor, QPainter
from PyQt5.QtWidgets import QWidget

from .controls import (HOLD, HOLD_DELAY_MS, HOLD_REPEAT_MS, LABELS, MAX_LEVEL, PRESS, RELEASE,
                       button_event, lock_event, scroll_event)

KEYS = {Qt.Key_Return: "ok", Qt.Key_Enter: "ok", Qt.Key_Backspace: "back", Qt.Key_Escape: "back",
        Qt.Key_M: "menu", Qt.Key_R: "random", Qt.Key_Space: "play", Qt.Key_Left: "previous",
        Qt.Key_Right: "next", Qt.Key_Plus: "vol_up", Qt.Key_Minus: "vol_down", Qt.Key_P: "power"}
FAST_LEVEL = 6
DRAG_STEP_PX = 14                       # finger travel per scroll event
TAP_MAX_PX, TAP_MAX_S = 4, 0.3


class Holder:
    """Press / repeated hold / release of one button."""

    def __init__(self, name, send, parent):
        self.name, self.send = name, send
        self.timer = QTimer(parent)
        self.timer.timeout.connect(self._hold)
        self.down = False

    def press(self):
        if self.down:
            return
        self.down = True
        self.send(button_event(self.name, PRESS))
        self.timer.start(HOLD_DELAY_MS)

    def _hold(self):
        self.send(button_event(self.name, HOLD))
        self.timer.setInterval(HOLD_REPEAT_MS)

    def release(self):
        if not self.down:
            return
        self.down = False
        self.timer.stop()
        self.send(button_event(self.name, RELEASE))


class TouchStrip(QWidget):
    def __init__(self, send, width=46, height=220):
        super().__init__()
        self.send = send
        self.setFixedSize(width, height)
        self.setToolTip("touch pad: drag in the middle, hold top/bottom for fast scrolling, "
                        "click = tap, mouse wheel = one entry")
        self.repeat = QTimer(self)
        self.repeat.timeout.connect(self._fast)
        self.fast_dir = 0
        self.start = None               # (y, time) of the press in the middle zone
        self.last_y = self.moved = 0
        self.lit = None

    def _zone(self, y):
        q = self.height() // 4
        return -1 if y < q else (1 if y >= self.height() - q else 0)

    def mousePressEvent(self, e):
        y = e.pos().y()
        zone = self._zone(y)
        self.lit = zone
        if zone:
            self.fast_dir = zone
            self._fast()
            self.repeat.start(HOLD_REPEAT_MS)
        else:
            self.start, self.last_y, self.moved = (y, time.monotonic()), y, 0
        self.update()

    def _fast(self):
        self.send(scroll_event(self.fast_dir, FAST_LEVEL))

    def mouseMoveEvent(self, e):
        if self.start is None:
            return
        y = e.pos().y()
        dy = y - self.last_y
        if abs(dy) >= DRAG_STEP_PX:
            now = time.monotonic()
            speed = abs(dy) / max(now - self.start[1], 1e-3)          # px per second
            level = max(1, min(MAX_LEVEL, int(speed / 150)))
            self.send(scroll_event(dy, level))
            self.last_y, self.start = y, (self.start[0], now)
        self.moved = max(self.moved, abs(y - self.start[0]))

    def mouseReleaseEvent(self, e):
        self.repeat.stop()
        if self.start is not None:
            y = e.pos().y()
            self.moved = max(self.moved, abs(y - self.start[0]))
            for _ in range(abs(y - self.last_y) // DRAG_STEP_PX):   # travel not yet sent
                self.send(scroll_event(y - self.last_y, 1))
            if self.moved <= TAP_MAX_PX and time.monotonic() - self.start[1] <= TAP_MAX_S:
                self.send(button_event("tap", PRESS))
                self.send(button_event("tap", RELEASE))
        self.start, self.lit = None, None
        self.update()

    def wheelEvent(self, e):
        notches = e.angleDelta().y() // 120
        for _ in range(abs(notches)):
            self.send(scroll_event(-notches, 1))

    def paintEvent(self, e):
        """The pad surface; the pressed zone (fast up / scroll / fast down) is shaded."""
        p = QPainter(self)
        p.setRenderHint(QPainter.Antialiasing)
        w, h, q = self.width(), self.height(), self.height() // 4
        p.setPen(Qt.NoPen)
        p.setBrush(QColor(0xE4, 0xE6, 0xE8))
        p.drawRoundedRect(QRectF(0, 0, w, h), w / 2, w / 2)
        if self.lit is not None:
            top = {-1: 0, 0: q, 1: h - q}[self.lit]
            p.setClipRect(QRect(0, top, w, h - 2 * q if self.lit == 0 else q))
            p.setBrush(QColor(0xC8, 0xCC, 0xD0))
            p.drawRoundedRect(QRectF(0, 0, w, h), w / 2, w / 2)
            p.setClipping(False)
        p.setPen(QColor(0xB0, 0xB4, 0xB8))
        p.setBrush(Qt.NoBrush)
        p.drawRoundedRect(QRectF(0.5, 0.5, w - 1, h - 1), w / 2, w / 2)


class Controls:
    """The buttons' press / hold / release, the Lock switch and the keyboard shortcuts;
    the widgets showing them (qt_device.py) are bound with bind() and lock."""

    def __init__(self, mcu, parent, on_event=None):
        self.mcu, self.on_event = mcu, on_event
        self.holders = {name: Holder(name, self.send, parent) for name in LABELS if name != "tap"}
        self.lock = None                # checkable button showing the Lock switch

    def bind(self, button, name):
        button.setAutoRepeat(False)
        button.pressed.connect(self.holders[name].press)
        button.released.connect(self.holders[name].release)

    def bind_lock(self, button):
        self.lock = button
        button.setCheckable(True)
        button.toggled.connect(lambda on: self.send(lock_event(on)))

    def send(self, event):
        self.mcu.inject(event)
        if self.on_event:
            self.on_event()

    def key(self, e, down):
        """Keyboard shortcut handling; True if the key was used."""
        if e.isAutoRepeat():
            return e.key() in KEYS or e.key() in (Qt.Key_Up, Qt.Key_Down)
        if e.key() in (Qt.Key_Up, Qt.Key_Down):
            if down:
                self.send(scroll_event(1 if e.key() == Qt.Key_Down else -1, 1))
            return True
        if e.key() == Qt.Key_L and down:
            self.lock.toggle()
            return True
        name = KEYS.get(e.key())
        if name is None:
            return False
        (self.holders[name].press if down else self.holders[name].release)()
        return True
