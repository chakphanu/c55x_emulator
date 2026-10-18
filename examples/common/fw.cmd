/* Memory layout of the minimal programs (byte addresses). Everything on chip: code and constants in
 * SARAM (like Jukebox2 block 4 at 0x20000), variables and stacks in DARAM. */
-e _fw_reset
MEMORY
{
    VECT (RX)   : origin = 0x00000100, length = 0x00000200  /* 0x100 vectors + linker padding */
    DARAM (RW)  : origin = 0x00002000, length = 0x00006000
    SARAM (RWX) : origin = 0x00020000, length = 0x00010000
}
SECTIONS
{
    .vectors  : {} > VECT
    .text     : {} > SARAM
    .const    : {} > SARAM
    .bss      : {} > DARAM
    .stack    : {} > DARAM
    .sysstack : {} > DARAM
}
