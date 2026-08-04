# Foxeer F405 V2 generated-target prototype

This self-contained embedded crate is the actuator-inhibited starting point
for generating the golden Foxeer F405 V2 RTIC application. It is not the
golden flight image and must not replace `apps/foxeer-f405-v2` until the parity
and safety gates in the builder roadmap are complete.

The initial declaration intentionally exercises only this audited subset of
the authoritative Foxeer board contract:

- STM32F405RGT6/LQFP64;
- 8 MHz HSE and 168 MHz system clock;
- a USB-valid PLL48 clock plan, although USB is not initialized yet;
- receive-only SBUS on USART2 RX/PA3 with DMA1 Stream 5 Channel 4;
- one RC heartbeat over `defmt`/RTT.

UART4, USB, IMU, ADC, storage, timers, ESC telemetry, DShot, and every motor
resource remain unclaimed and uninitialized. This target therefore has no
actuator authority.

The generator checks this subset against the authoritative board facts under
`apps/foxeer-f405-v2/src/board` before writing generated source.

From `tools/app-builder-v2`, regenerate all targets with:

```text
cargo xtask generate
```

From this directory, verify the embedded crate with:

```text
cargo check --locked
cargo build --locked --release
```

A successful build establishes Rust, RTIC, and linker compatibility only. It
does not establish reception on hardware or any bench, preflight, or flight
gate.
