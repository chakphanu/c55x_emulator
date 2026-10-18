/* MCU access constants for the examples. */
#ifndef ZEN_MCU_H
#define ZEN_MCU_H

#define MCU_I2C_ADDR 7
#define IFR1 (*(volatile unsigned int *)0x0046L)    /* interrupt flags 16..23 (MMR) */
#define MCU_IRQ_MASK 1                              /* IER1/IFR1 bit 0 = interrupt 16 */

#endif /* ZEN_MCU_H */
