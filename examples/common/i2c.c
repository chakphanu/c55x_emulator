/* Polling I2C master for the minimal programs: register sequence of the original firmware
 * (start/stop codes, status bits), plus timeouts so a silent slave cannot hang. */
#include "fw.h"

#define MDR_RUN       0x5020    /* out of reset (iic_hw_init) */
#define MDR_START_TX  0x76A0    /* start, master transmitter, repeat mode */
#define MDR_STOP      0x5EA0    /* stop, master transmitter, repeat mode */
#define MDR_START_RX  0x7C20    /* start and stop, master receiver, count mode */
#define PSC_200MHZ    15        /* iic_hw_init: mhz / 12 - 1 for 200 MHz; a slower CPU clock
                                 * only makes the bus slower */
#define CLK_KEY0      55        /* g_iic_speed_table entry for g_iic_speed 0 */
#define OWN_ADDR      5
#define STR_CLEAR     0x301E
#define STR_TX_READY  (I2CSTR_ARDY | I2CSTR_XRDY)
#define TIMEOUT       30000

void i2c_init(void)
{
    I2CMDR = 0;
    I2CIER = 0;
    I2CPSC = PSC_200MHZ;
    I2CCNT = 0;
    I2CMDR = MDR_RUN;
    I2CCLKL = CLK_KEY0;
    I2CCLKH = CLK_KEY0;
    I2COAR = OWN_ADDR;
    I2CSTR = STR_CLEAR;
}

static int wait_idle(void)
{
    unsigned int n = TIMEOUT;

    while (I2CSTR & I2CSTR_BB) {
        if (--n == 0) {
            return 0;
        }
    }
    n = TIMEOUT;
    while (I2CMDR & I2CMDR_MST) {
        if (--n == 0) {
            return 0;
        }
    }
    return 1;
}

static void stop(void)
{
    I2CMDR = MDR_STOP;
    wait_idle();
    I2CSTR = 0x201E;
}

int i2c_write(unsigned int addr, unsigned int byte)
{
    unsigned int st, n = TIMEOUT;

    if (!wait_idle()) {
        return 0;
    }
    I2CSAR = addr;
    I2CMDR = MDR_START_TX;
    I2CDXR = byte;
    do {
        st = I2CSTR;
        if (--n == 0) {
            stop();
            return 0;
        }
    } while ((st & STR_TX_READY) != STR_TX_READY && !(st & I2CSTR_NACK));
    if (st & I2CSTR_NACK) {
        stop();
        return 0;
    }
    I2CMDR = MDR_STOP;
    return 1;
}

/* n bytes in one message (e.g. a codec register word, high byte first) */
int i2c_write_bytes(unsigned int addr, const unsigned int *bytes, int n)
{
    unsigned int st, t;
    int i;

    if (!wait_idle()) {
        return 0;
    }
    I2CSAR = addr;
    I2CMDR = MDR_START_TX;
    for (i = 0; i < n; i++) {
        I2CDXR = bytes[i];
        t = TIMEOUT;
        do {
            st = I2CSTR;
            if (--t == 0 || (st & I2CSTR_NACK)) {
                stop();
                return 0;
            }
        } while (!(st & I2CSTR_XRDY));
    }
    I2CMDR = MDR_STOP;
    return 1;
}

int i2c_read(unsigned int addr, unsigned int *bytes, int count)
{
    unsigned int n;
    int i;

    if (!wait_idle()) {
        return 0;
    }
    I2CSAR = addr;
    I2CCNT = count;
    I2CMDR = MDR_START_RX;
    for (i = 0; i < count; i++) {
        n = TIMEOUT;
        while (!(I2CSTR & I2CSTR_RRDY)) {
            if ((I2CSTR & I2CSTR_NACK) || --n == 0) {
                stop();
                return 0;
            }
        }
        bytes[i] = I2CDRR & 0xFF;
    }
    return 1;
}
