# Temporary serial/OSD compatibility adapter

This isolated adapter ports the ownership pattern from FerroWasp's
`ferrowasp-stm32f4/src/uart_dma.rs`, `ferrowasp-io-core/src/serial`, and
`ferrowasp-tasks/src/osd.rs` from an external source snapshot at commit
`bc26276c5b34e20f96f603d95585893b91784d05`.

It deliberately does not modify or participate in the FerroWasp root
applications or workspace. The USART1 specialization is for the
NUCLEO-F401RE builder experiment only. Replace this crate with canonical
FerroWasp crates when their stable public endpoint boundary is available.

The periodic heartbeat and 15-step overlay sequence are ported from
`ferrowasp-tasks/src/osd.rs`; periodic scheduling is supplied by the generated
application's backend-owned 1 kHz SysTick monotonic.
