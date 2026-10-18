"""Zen Touch controls -> MCU event bytes (mcu.py, decoded by mcu_event, src/drivers/mcu_event.c).

Controls as in the user manual ("A Tour of Your Player"):
front OK, Previous, Play/Pause, Next, Back, Menu, Random and the touch pad; side Power,
Volume +/-; Lock switch. The assignment below was found in the emulator with Jukebox2
2.10.05 (first-boot language menu and the Now Playing screen):

  button      detail  key  evidence
  power       0       17   holding it (repeated hold events, ~3 s) shuts down; the only
                           key mcu_event keeps while locked (17 -> 25)
  ok          1       8    takes the language, opens Selected Music from Now Playing
  play        2       13   starts the decoder (dec_engine_start, codec3_load)
  previous    4       5    list handler treats 3 and 5 alike; 5 resets the queue position
  next        5       3    3 steps the play queue from the current entry (pool chain word 0)
  vol_up      6       7    volume popup, level 17 -> 19 after three presses
  vol_down    7       6    volume popup, level 17 -> 15 after three presses
  back        8       11   Now Playing -> language menu
  menu        11      1    opens the Menu screen
  random      13      27   opens "Random Play All"
  tap         14      8    touch pad tap ("Tap to Select"), same key as OK
Details 3 (key 29) and 9 (key 28) showed no effect and are not mapped. Event class 1 is
the press, 2 a hold (the firmware reacts on the release, class 3; holding Power needs
repeated hold events), 3 the release. Keymap B (classes 4..6) is not a front control.

Touch pad: class 7 = scroll down (key 15), class 9 = scroll up (key 14); the detail is the
speed level 0..8, mapped through g_mcu_curve_table (level 1 moved the language list by one
entry, level 4 by two). Lock switch: event 0xd9 locks (g_mcu_b14 = 1), 0xda unlocks.
Previous/Next are derived from the code paths, not seen on a playing track; the hold
repeat timing of the real MCU is unknown (HOLD_DELAY_MS / HOLD_REPEAT_MS are assumptions).
"""

BUTTONS = {"power": 0, "ok": 1, "play": 2, "previous": 4, "next": 5, "vol_up": 6,
           "vol_down": 7, "back": 8, "menu": 11, "random": 13, "tap": 14}
LABELS = {"power": "Power", "ok": "OK", "play": "Play/Pause", "previous": "Previous",
          "next": "Next", "vol_up": "Volume +", "vol_down": "Volume -", "back": "Back",
          "menu": "Menu", "random": "Random", "tap": "Tap"}
PRESS, HOLD, RELEASE = 0x10, 0x20, 0x30
SCROLL_DOWN, SCROLL_UP = 0x70, 0x90
MAX_LEVEL = 8                           # last entry of a g_mcu_curve_table row
LOCK, UNLOCK = 0xD9, 0xDA
HOLD_DELAY_MS, HOLD_REPEAT_MS = 600, 150


def button_event(name, phase):
    """phase: PRESS, HOLD or RELEASE."""
    if phase not in (PRESS, HOLD, RELEASE):
        raise ValueError(f"phase 0x{phase:02x}")
    return phase | BUTTONS[name]


def scroll_event(steps_down, level):
    """Touch pad movement: positive = down; level 0..MAX_LEVEL (speed)."""
    if not 0 <= level <= MAX_LEVEL:
        raise ValueError(f"touch level {level}")
    return (SCROLL_DOWN if steps_down > 0 else SCROLL_UP) | level


def lock_event(locked):
    return LOCK if locked else UNLOCK
