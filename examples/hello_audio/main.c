/*
 * Hello-Audio: a 1 kHz tone (10 periods in 441 frames) through the audio path of the
 * Zen Touch: TLV320AIC23 over I2C (register values as aic23_init_regs in
 * the original firmware, but output stage on and DAC unmuted), McBSP0 as in
 * mcbsp_config(0, 1, 1, 2) (16-bit words, two per frame, clocks from the codec as master)
 * and DMA channel 0 on the McBSP0 transmit event, auto-initialized over the table.
 * Left channel = tone, right channel = inverted tone. The main loop only waits with
 * interrupts globally enabled (no interrupt sources enabled).
 */
#include "fw.h"

#define AIC23_ADDR 0x1A
#define AIC23(reg, v) (((reg) << 9) | (v))
#define MCBSP0(r) (*(volatile ioport unsigned int *)(0x2800 + (r)))
#define DMA0(r) (*(volatile ioport unsigned int *)(0x0C00 + (r)))
#define SPCR2 4
#define SPCR1 5
#define XCR2 8
#define XCR1 9
#define PCR 0x12
#define DXR1_PORT 0x2803
#define CSDP 0
#define CCR 1
#define CICR 2
#define CSSA_L 4
#define CSSA_U 5
#define CDSA_L 6
#define CDSA_U 7
#define CEN 8
#define CFN 9

#define FRAMES 441
#define AMPLITUDE 8000
#define SIN_W 1136              /* AMPLITUDE * sin(2 pi * 10 / 441) */
#define COS2_W_Q14 32436        /* 2 cos(2 pi * 10 / 441) in Q14 */

static int samples[2 * FRAMES];

static void aic23(unsigned int reg, unsigned int value)
{
    unsigned int word = AIC23(reg, value);
    unsigned int bytes[2];

    bytes[0] = word >> 8;
    bytes[1] = word & 0xFF;
    i2c_write_bytes(AIC23_ADDR, bytes, 2);
}

static void make_tone(void)
{
    long prev = 0, cur = SIN_W, next;
    int i;

    for (i = 0; i < FRAMES; i++) {
        samples[2 * i] = (int)prev;
        samples[2 * i + 1] = (int)-prev;
        next = (((long)(int)COS2_W_Q14 * (int)cur) >> 14) - prev;  /* 16 x 16 bit product */
        prev = cur;
        cur = next;
    }
}

static void tone_codec_init(void)
{
    aic23(15, 0x000);           /* reset */
    aic23(6, 0x067);            /* power: everything but the inputs on (output stage too) */
    aic23(4, 0x012);            /* analog path: DAC selected */
    aic23(5, 0x000);            /* digital path: DAC not muted */
    aic23(7, 0x043);            /* interface: master, DSP format, 16 bit */
    aic23(8, 0x023);            /* USB clock mode, 44.1 kHz (44.118 kHz) */
    aic23(2, 0x179);            /* headphone volume both channels, 0 dB */
    aic23(9, 0x001);            /* interface active */
}

static void tone_mcbsp_init(void)
{
    MCBSP0(SPCR1) &= 0xFFFE;
    MCBSP0(SPCR2) &= 0xFF3E;
    MCBSP0(SPCR1) = 0x20;
    MCBSP0(SPCR2) = 0x120;
    MCBSP0(XCR1) = 0x140;       /* one phase, 2 words per frame, 16 bit */
    MCBSP0(XCR2) = 0;
    MCBSP0(PCR) = 0x4003;       /* external bit clock and frame sync (codec is master) */
    MCBSP0(SPCR2) |= 0x40;      /* GRST */
    MCBSP0(SPCR2) |= 0x80;      /* FRST */
}

static void tone_dma_init(void)
{
    unsigned long src = (unsigned long)samples << 1;            /* byte address */
    unsigned long dst = (unsigned long)DXR1_PORT << 1;
    unsigned int port = src < 0x10000UL ? 1 : 0;                /* DARAM : SARAM */

    DMA0(CSDP) = 1 | (port << 2) | (3 << 9);                    /* 16 bit, memory -> peripheral */
    DMA0(CICR) = 0x20;                                          /* block interrupt flag */
    DMA0(CSSA_L) = (unsigned int)src;
    DMA0(CSSA_U) = (unsigned int)(src >> 16);
    DMA0(CDSA_L) = (unsigned int)dst;
    DMA0(CDSA_U) = (unsigned int)(dst >> 16);
    DMA0(CEN) = 2;
    DMA0(CFN) = FRAMES;
    DMA0(CCR) = 0x02 | 0x100 | 0x200 | 0x1000;                  /* XEVT0, auto-init, repeat, src ++ */
    DMA0(CCR) |= 0x80;                                          /* enable */
}

void main(void)
{
    emif_ce1_slow();
    lcd_init();
    lcd_on();
    lcd_text(1, "HELLO AUDIO");
    lcd_text(3, "1 KHZ TONE  L + / R -");
    make_tone();
    i2c_init();
    tone_codec_init();
    tone_mcbsp_init();
    tone_dma_init();
    MCBSP0(SPCR2) |= 1;         /* XRST: transmitter on */
    irq_enable(0, 0);
    for (;;) {
    }
}
