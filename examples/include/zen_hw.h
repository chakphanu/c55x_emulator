/*
 * Zen Touch main program: on-chip peripherals and low-level hardware setup,
 * reconstructed. Chip: TMS320VC5509A (working assumption).
 * Register names follow TI SPRS205K.
 *
 * Peripheral registers live in I/O space (ioport); CPU status registers are memory-mapped
 * registers in data space (ST1_55 at word 0x03, ST3_55 at word 0x04).
 */
#ifndef ZEN_HW_H
#define ZEN_HW_H

#define IOREG(addr) (*(volatile ioport unsigned int *)(addr))

/* EMIF (SPRS205K Table 3-23) */
#define EGCR    IOREG(0x0800)
#define EMIRST  IOREG(0x0801)
#define CE0_1   IOREG(0x0803)   /* CEn_1..CEn_3 at 0x0803 + 3 * n */
#define CE1_1   IOREG(0x0806)
#define CE2_1   IOREG(0x0809)
#define CE3_1   IOREG(0x080C)
#define SDC1    IOREG(0x080F)
#define SDPER   IOREG(0x0810)
#define SDINIT  IOREG(0x0812)
#define SDC2    IOREG(0x0813)
#define SDC3    IOREG(0x0814)

/* DMA (Table 3-24): channel n at 0x0C00 + 0x20 * n */
#define DMA_GCR IOREG(0x0E00)
#define DMA_CH_BASE(n) (0x0C00 + 0x20 * (n))

/* Timers (Table 3-27): TIM, PRD, TCR, PRSC */
#define TIM0_PRD  IOREG(0x1001)
#define TIM0_TCR  IOREG(0x1002)
#define TIM0_PRSC IOREG(0x1003)
#define TIM1_TCR  IOREG(0x2402)
#define TCR_TSS     (1u << 4)   /* timer stop */
#define TCR_ARB     (1u << 5)   /* auto reload */
#define TCR_TLB     (1u << 10)  /* load from PRD/TDDR */
#define TCR_IDLEEN  (1u << 15)

/* Clock generator (SPRU317K 3.8) */
#define CLKMD   IOREG(0x1C00)
#define CLKMD_LOCK      (1u << 0)
#define CLKMD_PLLENABLE (1u << 4)

/* McBSP0 / McBSP1 serial port control and pin control */
#define MCBSP0_SPCR2 IOREG(0x2804)
#define MCBSP0_SPCR1 IOREG(0x2805)
#define MCBSP1_SPCR2 IOREG(0x2C04)
#define MCBSP1_SPCR1 IOREG(0x2C05)
#define MCBSP1_PCR   IOREG(0x2C12)

/* GPIO[7:0] (Table 3-31) */
#define IODIR   IOREG(0x3400)
#define IODATA  IOREG(0x3401)

/* Chip identification block; 0x3803 bits 4..1 = silicon revision (Table 3-32), the rest
 * is undocumented. Bits 15..4 of 0x3804 select variant-specific settings. */
#define CHIP_ID(n) IOREG(0x3800 + (n))
#define CHIP_VARIANT() ((CHIP_ID(4) >> 4) & 0xFFF)
#define CHIP_VARIANT_250 0x250
#define CHIP_VARIANT_255 0x255

/* RTC (Table 3-25), BCD time registers */
#define RTCSEC    IOREG(0x1800)
#define RTCSECA   IOREG(0x1801)
#define RTCMIN    IOREG(0x1802)
#define RTCMINA   IOREG(0x1803)
#define RTCHOUR   IOREG(0x1804)
#define RTCHOURA  IOREG(0x1805)
#define RTCDAYW   IOREG(0x1806)
#define RTCDAYM   IOREG(0x1807)
#define RTCMONTH  IOREG(0x1808)
#define RTCYEAR   IOREG(0x1809)
#define RTCPINTR  IOREG(0x180A)
#define RTCINTEN  IOREG(0x180B)
#define RTCINTFL  IOREG(0x180C)
#define RTC_REG_0D IOREG(0x180D)    /* not in SPRS205K Table 3-25 */
#define RTCPINTR_UIP  (1u << 7)     /* update in progress (bit use from the firmware) */
#define RTCINTEN_SET  (1u << 7)     /* halt updates while setting */
#define RTCINTEN_AIE  (1u << 5)     /* alarm interrupt enable */
#define RTCINTFL_AF   (1u << 5)     /* alarm flag */

/* I2C (Table 3-33) */
#define I2COAR  IOREG(0x3C00)
#define I2CIER  IOREG(0x3C01)
#define I2CSTR  IOREG(0x3C02)
#define I2CCLKL IOREG(0x3C03)
#define I2CCLKH IOREG(0x3C04)
#define I2CCNT  IOREG(0x3C05)
#define I2CDRR  IOREG(0x3C06)
#define I2CSAR  IOREG(0x3C07)
#define I2CDXR  IOREG(0x3C08)
#define I2CMDR  IOREG(0x3C09)
#define I2CPSC  IOREG(0x3C0C)
#define I2CSTR_NACK (1u << 1)
#define I2CSTR_ARDY (1u << 2)
#define I2CSTR_RRDY (1u << 3)
#define I2CSTR_XRDY (1u << 4)
#define I2CSTR_BB   (1u << 12)
#define I2CMDR_MST  (1u << 10)
#define I2CMDR_STP  (1u << 11)

/* ADC (SPRS205K: ADCCTL, ADCDATA, ADCCLKDIV, ADCCLKCTL) */
#define ADCCTL    IOREG(0x6800)
#define ADCDATA   IOREG(0x6801)
#define ADCCLKDIV IOREG(0x6802)
#define ADCCLKCTL IOREG(0x6803)
#define ADCCTL_START  (1u << 15)
#define ADCCTL_CHAN(c) ((unsigned int)(c) << 12)
#define ADCDATA_BUSY  (1u << 15)
#define ADCDATA_MASK  0x3FF

/* USB (Table 3-26, 3-37) */
#define USBDPLL    IOREG(0x1E00)
#define USBCTL     IOREG(0x67FC)
#define USBIDLECTL IOREG(0x7000)

/* External Bus Selection Register (Table 3-5) */
#define EBSR    IOREG(0x6C00)
#define EBSR_SR_CMD (1u << 6)   /* SDRAM self-refresh command */
#define EBSR_CKE_EN (1u << 7)
#define EBSR_HOLDA  (1u << 9)
#define EBSR_HOLD   (1u << 10)

/* Idle configuration register (I/O 0x0001, C55x CPU; not in SPRS205K, hypothesis) */
#define ICR     IOREG(0x0001)

/* CPU status registers (memory-mapped) */
#define ST1_55  (*(volatile unsigned int *)0x0003L)
#define ST3_55  (*(volatile unsigned int *)0x0004L)
#define ST1_XF   (1u << 13)
#define ST1_INTM (1u << 11)
#define ST3_MPNMC (1u << 6)

/* The firmware saves ST1 in a fixed word (0x68) before masking interrupts and restores
 * INTM from it afterwards (not reentrant). */
extern volatile unsigned int g_irq_saved_st1;

#define IRQ_SAVE()                          \
    do {                                    \
        g_irq_saved_st1 = ST1_55;           \
        asm(" BSET INTM");                  \
        asm(" RPT #5");                     \
        asm(" NOP");                        \
    } while (0)

#define IRQ_RESTORE()                       \
    do {                                    \
        if (g_irq_saved_st1 & ST1_INTM) {   \
            asm(" BSET INTM");              \
        } else {                            \
            asm(" BCLR INTM");              \
        }                                   \
    } while (0)

#endif /* ZEN_HW_H */
