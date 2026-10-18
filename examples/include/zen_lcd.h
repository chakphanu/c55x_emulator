/* Hardware definitions of the LCD controller for the examples (data word addresses). */
#ifndef ZEN_LCD_H
#define ZEN_LCD_H

/* LCD controller access (data word addresses). Address bit 0 selects command (0) or
 * display data (1), like the controller's A0 pin. Every write is followed by an 8-cycle
 * delay and a dummy read of the second register. */
#define LCD_CMD_REG    (*(volatile unsigned int *)0x281000L)
#define LCD_DATA_REG   (*(volatile unsigned int *)0x281001L)
#define LCD_STATUS_REG (*(volatile unsigned int *)0x283800L)

#define LCD_WIDTH  160  /* columns (lcd_cfg_set_defaults, lcd_update_rect) */
#define LCD_HEIGHT 104  /* rows; the controller addresses 8-row pages */

#define LCD_WRITE(value)           \
    do {                           \
        LCD_CMD_REG = (value);     \
        asm(" RPT #7");            \
        asm(" NOP");               \
        (void)LCD_STATUS_REG;      \
    } while (0)

#define LCD_WRITE_DATA(value)      \
    do {                           \
        LCD_DATA_REG = (value);    \
        asm(" RPT #7");            \
        asm(" NOP");               \
        (void)LCD_STATUS_REG;      \
    } while (0)

/* Controller commands. All bytes used by the firmware except 0x66 match the Sitronix
 * ST7571 instruction table (datasheet v1.0, 2008-01-29, section 8). The ST7571 has only
 * 128 segments while lcd_cfg_set_defaults reports a width of 160, so the actual chip is
 * probably a compatible relative; it is not identified. */
#define LCD_CMD_DISPLAY_OFF        0xAE        /* | 1: display on */
#define LCD_CMD_PARTIAL_DUTY       0x48        /* 2 bytes: duty ratio follows */
#define LCD_CMD_ADC_REVERSE        0xA1        /* segment direction reversed */
#define LCD_CMD_SHL_REVERSE        0xC8        /* common direction reversed */
#define LCD_CMD_OSC_ON             0xAB        /* start built-in oscillator */
#define LCD_CMD_REGULATOR(r)       (0x20 | (r)) /* regulator resistor ratio 0..7 */
#define LCD_CMD_CONTRAST           0x81        /* 2 bytes: electronic volume 0..63 follows */
#define LCD_CMD_BIAS(b)            (0x50 | (b)) /* LCD bias select 0..7 */
#define LCD_CMD_POWER(vc_vr_vf)    (0x28 | (vc_vr_vf)) /* booster/regulator/follower on bits */
#define LCD_CMD_POWER_SAVE         0xA9        /* enter power save (sleep) mode */
#define LCD_CMD_RELEASE_POWER_SAVE 0xE1
#define LCD_CMD_UNKNOWN_66         0x66        /* not defined in the ST7571 table */
#define LCD_CMD_PAGE(p)            (0xB0 | (p)) /* page (8-row band) address */
/* Column address as on ST7565-style controllers (8-bit column); the ST7571 table instead
 * uses Y7..Y1 in these two commands. */
#define LCD_CMD_COLUMN_LOW(x)      ((x) & 0x0F)
#define LCD_CMD_COLUMN_HIGH(x)     (0x10 | ((x) >> 4))

#endif /* ZEN_LCD_H */
