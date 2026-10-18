/* Interrupt handling of the minimal programs: counting ISRs (vectors in vectors.asm),
 * timer 0 setup (SPRU595C), enabling. */
#include "fw.h"

volatile unsigned int g_ticks;
volatile unsigned int g_mcu_requests;

interrupt void fw_timer0_isr(void)
{
    g_ticks++;
}

interrupt void fw_mcu_isr(void)
{
    g_mcu_requests++;
}

void irq_init(void)
{
    asm(" BSET INTM");
    FW_IER0 = 0;
    FW_IER1 = 0;
    FW_IFR0 = 0xFFFF;
    FW_IFR1 = 0xFFFF;
    g_ticks = 0;
    g_mcu_requests = 0;
}

/* Period (tddr + 1) * (prd + 1) CPU clocks, auto reload. */
void timer0_start(unsigned int prd, unsigned int tddr)
{
    TIM0_TCR = TCR_TSS | TCR_TLB;
    TIM0_PRD = prd;
    TIM0_PRSC = tddr & 0xF;
    TIM0_TCR = TCR_TSS | TCR_TLB;           /* load TIM/PSC from PRD/TDDR */
    TIM0_TCR = TCR_ARB;                     /* TLB = 0, TSS = 0: run */
}

void irq_enable(unsigned int ier0, unsigned int ier1)
{
    FW_IER0 |= ier0;
    FW_IER1 |= ier1;
    asm(" BCLR INTM");
}
