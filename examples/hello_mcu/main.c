/*
 * Hello-MCU: shows the MCU registers 0..2, the incoming MCU event bytes and a tick count.
 * Start-up like mcu_init (the original firmware): status register 1, register 2, then
 * the commands 6/9, 33 and 36/3, with INT1 (MCU request) already enabled as in mcu_init,
 * so the ready signal after a single command reaches the ISR instead of staying latched.
 * Then timer 0. The ISRs only count (irq.c), the main loop reads one event byte per
 * request. Event 0x00
 * (the answer when nothing is pending, ignored by mcu_event) is not listed.
 * Key codes come from g_mcu_keymap_9b / g_mcu_keymap_ab (0x423e9b, 0x423eab);
 * their meaning is open question M4.
 */
#include "fw.h"

#define HISTORY (PAGES - 4)         /* event lines on pages 4..12 */
#define TICK_PRD  62499             /* timer 0: 16 * 62500 = 1,000,000 CPU clocks per tick */
#define TICK_TDDR 15

static const unsigned int keymap_9b[16] = { 17, 8, 13, 29, 5, 3, 7, 6, 11, 28, 12, 1, 12, 27, 8, 12 };
static const unsigned int keymap_ab[16] = { 13, 4, 5, 3, 7, 6, 16, 22, 18, 26, 12, 12, 12, 12, 12, 12 };
static unsigned int events[HISTORY];
static char text[TEXT_COLS + 1];

static char *put(char *out, const char *s)
{
    while (*s != 0) {
        *out++ = *s++;
    }
    return out;
}

static void show_registers(void)
{
    unsigned int st1, st2, bat;
    char *p = text;

    if (mcu_reg(1, &st1) && mcu_reg(2, &st2) && mcu_reg(0, &bat)) {
        p = hex_put(put(p, "ST1 "), st1, 4);
        p = hex_put(put(p, " ST2 "), st2, 4);
        p = hex_put(put(p, " BAT "), bat, 4);
    } else {
        p = put(p, "MCU: NO ANSWER");
    }
    *p = 0;
    lcd_text(1, text);
}

static void event_line(int page, unsigned int ev)
{
    unsigned int cls = (ev >> 4) & 15;
    char *p = hex_put(put(text, "EV "), ev, 2);

    if (cls >= 1 && cls <= 3) {
        p = hex_put(put(p, "  KEY "), keymap_9b[ev & 15], 2);
        p = put(p, cls == 1 ? " T0" : cls == 2 ? " T1" : " T2");
    } else if (cls >= 4 && cls <= 6) {
        p = hex_put(put(p, "  KEY "), keymap_ab[ev & 15], 2);
        p = put(p, cls == 4 ? " B4" : cls == 5 ? " B5" : " B6");
    } else if (cls >= 7 && cls <= 10) {
        p = put(p, "  LEVEL");
    } else if (cls >= 11 && cls <= 13) {
        p = put(p, "  STATUS");
    }
    *p = 0;
    lcd_text(page, text);
}

static void show_events(unsigned int count)
{
    int i;
    char *p = hex_put(put(text, "EVENTS "), count, 4);

    *p = 0;
    lcd_text(3, text);
    for (i = 0; i < HISTORY && i < (int)count; i++) {
        event_line(4 + i, events[i]);
    }
}

static void show_ticks(unsigned int ticks, unsigned int requests)
{
    char *p = hex_put(put(text, "TICKS "), ticks, 4);

    p = hex_put(put(p, "  MCU IRQ "), requests, 4);
    *p = 0;
    lcd_text(2, text);
}

void main(void)
{
    int page, i;
    unsigned int ev, count = 0, ticks = 0, requests = 0;

    emif_ce1_slow();
    lcd_init();
    for (page = 0; page < PAGES; page++) {
        lcd_text(page, "");
    }
    lcd_text(0, "ZEN TOUCH MCU TEST");
    lcd_on();
    irq_init();
    i2c_init();
    irq_enable(0, FW_INT1);
    show_registers();
    mcu_cmd2(6, 9);
    mcu_cmd(33);
    mcu_cmd2(36, 3);
    show_events(0);
    show_ticks(0, 0);
    timer0_start(TICK_PRD, TICK_TDDR);
    irq_enable(FW_TINT0, 0);
    for (;;) {
        if (g_ticks != ticks) {
            ticks = g_ticks;
            show_ticks(ticks, requests);
        }
        if (g_mcu_requests == requests) {
            continue;
        }
        requests = g_mcu_requests;
        if (!mcu_event_read(&ev) || ev == 0) {
            continue;
        }
        for (i = HISTORY - 1; i > 0; i--) {
            events[i] = events[i - 1];
        }
        events[0] = ev;
        show_events(++count);
        if ((ev >> 4) == 12 || (ev >> 4) == 13) {
            show_registers();       /* status change: show the new register values */
        }
    }
}
