# TBS Lucid H7 Flight App

This isolated RTIC 2 application targets the TBS Lucid H7 (STM32H743). It
carries the Foxeer F405 V2 feature set on the Lucid's own peripherals, through
the `ferrowasp-stm32h7` backend, and owns its own Cargo graph, linker
configuration, and binary.

It has not run on hardware. Flight arming is disabled in `src/board/profiles.rs`
until the IMU orientation, motor order, and ADC scale are verified on the board.
Pins, differences from the Foxeer, and current limitations are in
[Current Support](../../mdbook/src/current_support.md#tbs-lucid-h7-unverified).

Build:

```text
cargo build --release --locked --target thumbv7em-none-eabihf
```
