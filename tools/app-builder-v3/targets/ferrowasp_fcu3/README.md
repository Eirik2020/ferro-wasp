# FerroWasp FCU3 RTIC prototype app

This self-contained embedded target builds the generated, non-actuator RTIC
prototype in `src/generated_app.rs`. Its handwritten inputs are `src/board.rs`,
`src/app_composition.rs`, and the reusable task modules in the builder's
`tasks/` directory.

From `tools/app-builder-v2`, regenerate both supported targets with:

```text
cargo xtask generate
```

The FCU3 hardware declaration follows the frozen STM32F405RGT6/LQFP64 routing:

- PB1 drives the green debug LED;
- USART2 RX on PA3 uses DMA1 Stream 5 Channel 4 for SBUS RC input;
- UART4 RX on PA1 uses DMA1 Stream 2 Channel 4 for raw COMPORT input;
- HSI provides the 168 MHz system and SysTick monotonic clocks.

The checked-in composition runs both serial components simultaneously. Valid
SBUS packets update the shared RC snapshot printed once per second through
`defmt`. CR-, LF-, or CRLF-terminated 115200-baud 8-N-1 lines received on PA1
are printed as `COMPORT: ...` through the same RTT terminal. COMPORT is
receive-only: PA0 and DMA1 Stream 4 are not claimed and input is not echoed.

Connect a USB-to-UART adapter's TX to FCU3 PA1 using 3.3-volt logic and a common
ground. Do not let the adapter and an attached MSP/DJI device drive PA1 at the
same time. SBUS remains on the board's receiver input and must not share PA3
with another transmitter.

## Build

From this directory:

```text
cargo check --locked
cargo build --locked --release
```

The release ELF is
`target/thumbv7em-none-eabihf/release/generated`. A successful build verifies
types, RTIC expansion, and linking; it does not prove electrical routing or DMA
reception on hardware. This prototype has no motor-output authority.
