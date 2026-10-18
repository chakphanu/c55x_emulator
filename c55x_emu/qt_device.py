"""The player's front as in the user manual ("Your Player at a Glance"): dark bumper
around a silver face, the LCD (blue EL backlight, manual appendix A), the touch pad in the middle with the round OK button on top, Back /
Menu / Random left of it, Previous / Play-Pause / Next right of it; the side buttons
(Power, Volume +/-) on the left edge and the Lock switch on the top edge.

The widgets are real controls (qt_controls.Controls / TouchStrip); this module only lays
them out and paints the case. All sizes scale with the LCD scale.
"""

from PyQt5.QtCore import QRectF, Qt
from PyQt5.QtGui import QColor, QLinearGradient, QPainter, QPen
from PyQt5.QtWidgets import QLabel, QPushButton, QWidget

from .lcd import HEIGHT, WIDTH
from .controls import LABELS
from .qt_controls import TouchStrip

# LCD colors: blue EL backlight with dark pixels, a dark glass when the display is off
LCD_BACK, LCD_PIXEL, LCD_OFF = QColor(0x5A, 0x9A, 0xE6), QColor(0x0C, 0x16, 0x3C), QColor(0x2A, 0x30, 0x3A)

KEY_STYLE = """
QPushButton {{ background: qlineargradient(x1:0, y1:0, x2:0, y2:1, stop:0 #f4f5f6, stop:1 #d6d9dc);
              border: 1px solid #9ea4aa; border-radius: {r}px; color: #4a5058;
              font: bold {pt}pt "DejaVu Sans"; }}
QPushButton:pressed {{ background: #bfc4c9; }}
QPushButton:checked {{ background: #e0a040; color: #302000; }}
"""


class DeviceView(QWidget):
    def __init__(self, controls, scale=3):
        super().__init__()
        u = scale / 3                       # layout unit: the sizes below are for scale 3
        self.u = u
        px = lambda v: round(v * u)
        self.lcd_w, self.lcd_h = WIDTH * scale, HEIGHT * scale
        self.side = px(12)                  # room left of the case for the side buttons
        self.bumper, self.pad = px(26), px(40)
        face_w = self.lcd_w + 2 * self.pad
        face_h = px(64) + self.lcd_h + px(44) + px(330) + px(40)
        self.case = QRectF(self.side, px(10), face_w + 2 * self.bumper, face_h + 2 * self.bumper)
        self.face = self.case.adjusted(self.bumper, self.bumper, -self.bumper, -self.bumper)
        self.setFixedSize(round(self.case.right()) + px(4), round(self.case.bottom()) + px(4))

        lcd_x, lcd_y = round(self.face.left()) + self.pad, round(self.face.top()) + px(64)
        self.screen = QLabel(self)
        self.screen.setGeometry(lcd_x, lcd_y, self.lcd_w, self.lcd_h)

        cx = round(self.face.center().x())
        top = lcd_y + self.lcd_h + px(44)
        pill_w, ok_d = px(86), px(72)
        self.pill = QRectF(cx - pill_w / 2, top, pill_w, px(330))
        ok = QPushButton("OK", self)
        ok.setGeometry(cx - ok_d // 2, top + px(7), ok_d, ok_d)
        ok.setStyleSheet(KEY_STYLE.format(r=ok_d // 2, pt=max(7, px(12))))
        controls.bind(ok, "ok")
        strip_w = px(62)
        strip = TouchStrip(controls.send, strip_w, round(self.pill.height()) - ok_d - px(24))
        strip.setParent(self)
        strip.move(cx - strip_w // 2, top + ok_d + px(14))

        key_w, key_h = px(104), px(56)
        rows = [top + px(40) + i * px(92) for i in range(3)]
        left_x = round(self.face.left()) + px(18)
        right_x = round(self.face.right()) - px(18) - key_w
        for (name, label), y in zip((("back", "\u2190 BACK"), ("menu", "MENU"), ("random", "RANDOM")), rows):
            self._key(controls, name, label, left_x, y, key_w, key_h)
        for (name, label), y in zip((("previous", "|\u25c0\u25c0"), ("play", "\u25b6 / \u275a\u275a"),
                                     ("next", "\u25b6\u25b6|")), rows):
            self._key(controls, name, label, right_x, y, key_w, key_h)

        tab_h = px(46)                      # side buttons stick out of the left edge
        for name, label, y in (("power", "\u23fb", 110), ("vol_up", "+", 190), ("vol_down", "\u2212", 246)):
            self._key(controls, name, label, 0, round(self.case.top()) + px(y), self.side + px(2), tab_h,
                      radius=px(4), pt=max(5, px(8)))
        lock = QPushButton("LOCK", self)
        lock.setGeometry(round(self.case.right()) - px(130), round(self.case.top()) - px(8), px(70), px(16))
        lock.setStyleSheet(KEY_STYLE.format(r=px(6), pt=max(5, px(7))))
        lock.setToolTip("Lock switch (L)")
        controls.bind_lock(lock)

    def _key(self, controls, name, label, x, y, w, h, radius=None, pt=None):
        b = QPushButton(label, self)
        b.setGeometry(x, y, w, h)
        b.setStyleSheet(KEY_STYLE.format(r=radius if radius is not None else round(10 * self.u),
                                         pt=pt or max(6, round(10 * self.u))))
        b.setToolTip(LABELS[name])
        controls.bind(b, name)
        return b

    def paintEvent(self, e):
        p = QPainter(self)
        p.setRenderHint(QPainter.Antialiasing)
        u = self.u
        g = QLinearGradient(self.case.topLeft(), self.case.topRight())
        g.setColorAt(0, QColor(0x3A, 0x3D, 0x42))
        g.setColorAt(0.5, QColor(0x5C, 0x60, 0x66))
        g.setColorAt(1, QColor(0x34, 0x37, 0x3C))
        p.setPen(QPen(QColor(0x22, 0x24, 0x28), 1.5))
        p.setBrush(g)
        p.drawRoundedRect(self.case, 70 * u, 70 * u)
        g = QLinearGradient(self.face.topLeft(), self.face.bottomRight())
        g.setColorAt(0, QColor(0xF6, 0xF7, 0xF8))
        g.setColorAt(1, QColor(0xCE, 0xD2, 0xD6))
        p.setPen(QPen(QColor(0x9A, 0xA0, 0xA6), 1))
        p.setBrush(g)
        p.drawRoundedRect(self.face, 46 * u, 46 * u)
        bezel = QRectF(self.screen.geometry()).adjusted(-10 * u, -10 * u, 10 * u, 10 * u)
        p.setPen(QPen(QColor(0x80, 0x86, 0x8C), 1))
        p.setBrush(QColor(0x1C, 0x1F, 0x24))
        p.drawRoundedRect(bezel, 8 * u, 8 * u)
        p.setPen(QPen(QColor(0xA6, 0xAC, 0xB2), 1.5))
        p.setBrush(QColor(0xEC, 0xEE, 0xF0))
        p.drawRoundedRect(self.pill, self.pill.width() / 2, self.pill.width() / 2)
