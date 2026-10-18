/*
 * Hello-LCD: minimal stand-alone program for the Zen Touch (TMS320VC5509A).
 * Only CE1 timing and the LCD controller are set up (open question D4); then a test
 * picture (border, text, checker band) is drawn and the CPU idles in a loop.
 */
#include "fw.h"

static unsigned int line[LCD_WIDTH];

static void build_page(int page)
{
    int x;
    unsigned int edge = 0;

    if (page == 0) {
        edge = 0x01;
    } else if (page == PAGES - 1) {
        edge = 1u << ((LCD_HEIGHT - 1) & 7);
    }
    for (x = 0; x < LCD_WIDTH; x++) {
        line[x] = edge;
    }
    if (page == 5) {
        text_render(line, (LCD_WIDTH - 6 * 15) / 2, "HELLO ZEN TOUCH");
    } else if (page == 9) {
        for (x = 8; x < LCD_WIDTH - 8; x++) {
            line[x] = (x & 1) ? 0xAA : 0x55;
        }
    }
    line[0] = 0xFF;
    line[LCD_WIDTH - 1] = 0xFF;
}

void main(void)
{
    int page;

    emif_ce1_slow();
    lcd_init();
    for (page = 0; page < PAGES; page++) {
        build_page(page);
        lcd_page(page, line);
    }
    lcd_on();
    for (;;) {
    }
}
