# Temporary serial/OSD compatibility adapter

This isolated adapter ports the ownership pattern from FerroWasp's
`ferrowasp-stm32f4/src/uart_dma.rs`, `ferrowasp-io-core/src/serial`, and
`ferrowasp-tasks/src/osd.rs` at local commit
`bc26276c5b34e20f96f603d95585893b91784d05`.

It deliberately does not modify or participate in the flight-tested FerroWasp
applications. The USART1 specialization is for the NUCLEO-F401RE builder
experiment only. Replace this crate with canonical FerroWasp crates when their
stable public endpoint boundary is available.

The periodic heartbeat and 15-step overlay sequence are ported from
`ferrowasp-tasks/src/osd.rs`; TIM4 scheduling remains builder-owned.
