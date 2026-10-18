/* LCD output for the minimal programs: init as lcd_hw_init (the original firmware). */
#include "fw.h"

#define CE1_SLOW 0x0FFF             /* setup 15, strobe 63, hold 3 cycles (maximum) */
#define MTYPE_ASYNC16 0x1000
#define CONTRAST_DEFAULT 50         /* cinit value of g_lcd_contrast */

static unsigned int line[LCD_WIDTH];

void emif_ce1_slow(void)
{
    IOREG(0x0806) = MTYPE_ASYNC16 | CE1_SLOW;
    IOREG(0x0807) = CE1_SLOW;
    IOREG(0x0808) = 0;
}

void lcd_init(void)
{
    LCD_WRITE(LCD_CMD_PARTIAL_DUTY);
    LCD_WRITE(105);
    LCD_WRITE(LCD_CMD_ADC_REVERSE);
    LCD_WRITE(LCD_CMD_SHL_REVERSE);
    LCD_WRITE(LCD_CMD_OSC_ON);
    LCD_WRITE(LCD_CMD_UNKNOWN_66);
    LCD_WRITE(LCD_CMD_REGULATOR(6));
    LCD_WRITE(LCD_CMD_CONTRAST);
    LCD_WRITE(CONTRAST_DEFAULT * 20 / 100 + 24);
    LCD_WRITE(LCD_CMD_BIAS(7));
    LCD_WRITE(LCD_CMD_POWER(7));
}

void lcd_on(void)
{
    LCD_WRITE(LCD_CMD_DISPLAY_OFF | 1);
}

void lcd_page(int page, const unsigned int *columns)
{
    int x;

    LCD_WRITE(LCD_CMD_PAGE(page));
    LCD_WRITE(LCD_CMD_COLUMN_LOW(0));
    LCD_WRITE(LCD_CMD_COLUMN_HIGH(0));
    for (x = 0; x < LCD_WIDTH; x++) {
        LCD_WRITE_DATA(columns[x]);
    }
}

void lcd_text(int page, const char *s)
{
    int x;

    for (x = 0; x < LCD_WIDTH; x++) {
        line[x] = 0;
    }
    text_render(line, 2, s);
    lcd_page(page, line);
}
