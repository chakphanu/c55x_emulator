/*
 * Shared helpers of the minimal stand-alone programs under fw/: CE1 timing, LCD text output,
 * I2C master (polling, same register sequence as the original firmware) and MCU access
 * (same message pattern as the original firmware).
 */
#ifndef FW_H
#define FW_H

#include "zen_hw.h"
#include "zen_lcd.h"

#define PAGES ((LCD_HEIGHT + 7) / 8)
#define TEXT_COLS (LCD_WIDTH / 6)

/* CPU interrupt registers (memory-mapped, SPRU371F) and the interrupts used here
 * (SPRS205K table 3-40): TINT0 = 4 (IFR0 bit 4), INT1 = 16 (IFR1 bit 0, MCU request). */
#define FW_IER0 (*(volatile unsigned int *)0x0000L)
#define FW_IFR0 (*(volatile unsigned int *)0x0001L)
#define FW_IER1 (*(volatile unsigned int *)0x0045L)
#define FW_IFR1 (*(volatile unsigned int *)0x0046L)
#define FW_TINT0 (1u << 4)
#define FW_INT1  (1u << 0)

/* irq.c: interrupt service routines count events, the main loop handles them */
extern volatile unsigned int g_ticks;          /* timer 0 interrupts */
extern volatile unsigned int g_mcu_requests;   /* MCU interrupt requests (INT1) */
void irq_init(void);                            /* counters 0, all interrupts off */
void timer0_start(unsigned int prd, unsigned int tddr);
void irq_enable(unsigned int ier0, unsigned int ier1);

/* lcd.c */
void emif_ce1_slow(void);
void lcd_init(void);
void lcd_on(void);
void lcd_page(int page, const unsigned int *columns);      /* LCD_WIDTH column bytes */
void lcd_text(int page, const char *s);                     /* one text line per page */

/* font.c: 5x7 glyphs for ' ' .. 'Z' */
int text_render(unsigned int *columns, int x, const char *s);
char *hex_put(char *out, unsigned int value, int digits);

/* i2c.c: 1 on success */
void i2c_init(void);
int i2c_write(unsigned int addr, unsigned int byte);
int i2c_write_bytes(unsigned int addr, const unsigned int *bytes, int n);
int i2c_read(unsigned int addr, unsigned int *bytes, int count);

/* mcu.c */
int mcu_cmd(unsigned int byte);
int mcu_cmd2(unsigned int byte, unsigned int arg);
int mcu_reg(unsigned int reg, unsigned int *value);
int mcu_event_read(unsigned int *event);

#endif /* FW_H */
