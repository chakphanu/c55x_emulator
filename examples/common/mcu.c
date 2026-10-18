/* MCU access for the minimal programs, same pattern as the original firmware: one I2C
 * message per byte; within multi-byte commands and before a register read the MCU
 * signals readiness with interrupt flag 16 (IFR1 bit 0). That flag is polled with the
 * interrupt disabled in IER1 (as mcu_send / mcu_read_reg do), otherwise it reaches
 * fw_mcu_isr and counts as an event request. */
#include "fw.h"
#include "zen_mcu.h"

#define IRQ_TIMEOUT 30000

static unsigned int irq_off(void)
{
    unsigned int old = FW_IER1 & MCU_IRQ_MASK;

    FW_IER1 &= ~MCU_IRQ_MASK;
    return old;
}

static void irq_restore(unsigned int old)
{
    FW_IER1 |= old;
}

static int wait_irq(void)
{
    unsigned int n = IRQ_TIMEOUT;

    while (!(IFR1 & MCU_IRQ_MASK)) {
        if (--n == 0) {
            return 0;
        }
    }
    IFR1 = MCU_IRQ_MASK;
    return 1;
}

/* Like mcu_send(1, byte): no wait. */
int mcu_cmd(unsigned int byte)
{
    return i2c_write(MCU_I2C_ADDR, byte);
}

/* Like mcu_send(2, byte, arg): wait for the MCU after each byte. */
int mcu_cmd2(unsigned int byte, unsigned int arg)
{
    unsigned int old = irq_off();
    int ok = i2c_write(MCU_I2C_ADDR, byte) && wait_irq()
        && i2c_write(MCU_I2C_ADDR, arg) && wait_irq();

    irq_restore(old);
    return ok;
}

/* Like mcu_read_reg: select, wait, read two bytes (first byte = high byte). */
int mcu_reg(unsigned int reg, unsigned int *value)
{
    unsigned int b[2];
    unsigned int old = irq_off();
    int ok = i2c_write(MCU_I2C_ADDR, reg) && wait_irq() && i2c_read(MCU_I2C_ADDR, b, 2);

    IFR1 = MCU_IRQ_MASK;                /* as mcu_read_reg after the read */
    irq_restore(old);
    if (ok) {
        *value = (b[0] << 8) | b[1];
    }
    return ok;
}

/* Read one event byte after an MCU request (the interrupt acknowledged the flag). */
int mcu_event_read(unsigned int *event)
{
    return i2c_read(MCU_I2C_ADDR, event, 1);
}
