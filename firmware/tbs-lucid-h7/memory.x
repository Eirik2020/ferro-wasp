MEMORY
{
  /* NOTE K = KiBi = 1024 bytes */
  FLASH (rx) : ORIGIN = 0x08000000, LENGTH = 2048K
  /* AXI SRAM. Every DMA buffer is a static here: DMA1 and DMA2 cannot
     reach the DTCM at 0x20000000. */
  RAM (rwx) : ORIGIN = 0x24000000, LENGTH = 512K
}

/* This is where the call stack will be allocated. */
/* The stack is of the full descending type. */
/* NOTE Do NOT modify `_stack_start` unless you know what you are doing */
_stack_start = ORIGIN(RAM) + LENGTH(RAM);
