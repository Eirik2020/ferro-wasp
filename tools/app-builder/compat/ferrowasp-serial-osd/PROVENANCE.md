# Serial/OSD compatibility adapter

This isolated adapter ports the ownership pattern from FerroWasp's
`ferrowasp-stm32f4/src/uart_dma.rs`, `ferrowasp-io-core/src/serial`, and
`ferrowasp-tasks/src/osd.rs` from an external source snapshot at commit
`bc26276c5b34e20f96f603d95585893b91784d05`.

The adapter now imports MSP parsing and response behavior directly from the
canonical monorepo crate at `crates/ferrowasp-mspv1`; no builder-local protocol
copy remains. It deliberately does not modify or participate in the
FerroWasp root applications or workspace. The USART1 specialization is for
the NUCLEO-F401RE builder experiment only. Replace the remaining adapter only
when the canonical shared crates expose an STM32F401-compatible endpoint
boundary and that change can be validated without altering handwritten apps.

The periodic heartbeat and 15-step overlay sequence are ported from
`ferrowasp-tasks/src/osd.rs`; periodic scheduling is supplied by the generated
application's backend-owned 1 kHz SysTick monotonic.
