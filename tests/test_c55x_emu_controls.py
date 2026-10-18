"""C55x emulator: Zen Touch controls -> MCU event bytes (controls.py), and on Jukebox2:
OK, Back, Menu, the touch pad in the first-boot language menu and Power off / on."""

import unittest

from c55x_emu.controls import (BUTTONS, HOLD, PRESS, RELEASE, button_event, lock_event,
                               scroll_event)
from c55x_emu.diskimage import UPDATE_DIR
from c55x_emu.loader import CODE_DIR
from c55x_emu.mcu import key_code

KEYS = {"power": 17, "ok": 8, "play": 13, "previous": 5, "next": 3, "vol_up": 7, "vol_down": 6,
        "back": 11, "menu": 1, "random": 27, "tap": 8}


class ControlTests(unittest.TestCase):
    def test_buttons_hit_their_key_codes(self):
        for name, key in KEYS.items():
            for phase in (PRESS, HOLD, RELEASE):
                self.assertEqual(key_code(button_event(name, phase)), key, name)
        self.assertEqual(set(KEYS), set(BUTTONS))

    def test_touch_and_lock_events(self):
        self.assertEqual((scroll_event(3, 1), scroll_event(-1, 8)), (0x71, 0x98))
        self.assertEqual((lock_event(True), lock_event(False)), (0xD9, 0xDA))
        with self.assertRaises(ValueError):
            scroll_event(1, 9)


@unittest.skipUnless((CODE_DIR / "manifest.json").exists() and (UPDATE_DIR / "manifest.json").exists(),
                     "needs the Jukebox2 code blocks and the extracted update")
class LanguageMenuTests(unittest.TestCase):
    """About 15 s: formats a synthetic disk, boots to the language menu."""

    @classmethod
    def setUpClass(cls):
        from c55x_emu.cfsformat import format_cfs
        from c55x_emu.diskimage import MINIFS_SECTORS, synthetic_disk
        from c55x_emu.loader import load_jukebox2
        from c55x_emu.machine import Machine
        disk = synthetic_disk(MINIFS_SECTORS + 200_000)
        format_cfs(disk)
        cls.m = Machine(load_jukebox2(), disk=disk)
        while cls.m.seconds() < 21.5:
            cls.m.run(200_000)

    def wait(self, seconds):
        end = self.m.seconds() + seconds
        while self.m.seconds() < end:
            if self.m.run(100_000, until_seconds=end) and self.m.idle_ctl.clock_stopped():
                return                              # powered down: time stands still

    def press(self, *events):
        for e in events:
            self.m.mcu.inject(e)
            self.wait(0.3)
        return [list(r) for r in self.m.lcd.rows()]

    def test_navigation(self):
        language = self.press()
        down = self.press(scroll_event(1, 1))
        self.assertNotEqual(down, language)
        self.assertEqual(self.press(scroll_event(-1, 1)), language)
        ok = button_event("ok", PRESS), button_event("ok", RELEASE)
        now_playing = self.press(*ok)
        self.assertNotEqual(now_playing, language)
        back = button_event("back", PRESS), button_event("back", RELEASE)
        self.assertEqual(self.press(*back), language)
        self.press(*ok)
        menu = self.press(button_event("menu", PRESS), button_event("menu", RELEASE))
        self.assertNotIn(menu, (language, now_playing))

    def test_power_off_and_on(self):
        hold = [button_event("power", PRESS)] + [button_event("power", HOLD)] * 25
        self.press(*hold, button_event("power", RELEASE))
        self.wait(3)
        self.assertTrue(self.m.cpu.in_idle and self.m.idle_ctl.clock_stopped())
        self.assertFalse(self.m.lcd.on)
        self.press(button_event("power", PRESS), button_event("power", RELEASE))
        self.wait(3)
        self.assertTrue(self.m.lcd.on)
        self.assertFalse(self.m.cpu.in_idle)


if __name__ == "__main__":
    unittest.main()
