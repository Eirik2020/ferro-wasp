# TBS Lucid H7 Bring-Up Facts

The board facts that `board-bring-up.md` needs for the TBS Lucid H7. Pins and
the differences from the Foxeer are in
`mdbook/src/current_support.md`, "TBS Lucid H7 (unverified)". Nothing here
has run on the board. Arming stays disabled at compile time until the flags
below are set.

## Lucid Build and Flash

Catalog ID: `BUILD-LUCID-001`.

From `firmware/tbs-lucid-h7`:

```text
cargo build --release --locked --target thumbv7em-none-eabihf
```

Flash over SWD with probe-rs chip `STM32H743VITx`. The image links at
`0x08000000`, so it replaces any bootloader already on the board. Record the
commit, the dirty-tree identity, the ELF SHA-256, and the exact features.

## Lucid Board-Specific Checks

Catalog ID: `BRINGUP-LUCID-001`. Run it with `BRINGUP-COMMON-001` and
`BRINGUP-COMMON-003`. Each item below says which common step it supplies.

- **Loop rate (common step 1).** `CONTROL_LOOP_RATE_HZ` is 2000. The core runs at
  400 MHz from the 8 MHz HSE.
- **IMU identity and orientation (common steps 2 and 3).** The board may carry
  an ICM42688-P, an MPU6500 or an MPU-6000. The axis profile in
  `src/board/profiles.rs` is derived from Betaflight's `CW90_DEG_FLIP`. That
  derivation is a candidate, not a fact, because ArduPilot's hwdef disagrees
  with it. ArduPilot also mounts an MPU-6000 differently (`ROLL_180`) from the
  ICM42688-P (`YAW_270`), so an MPU-6000 board will likely fail the
  orientation step with the shared profile and need its own. Pass each fitted
  kind separately, then add it to `imu_kind_flight_verified`.
- **RC (common step 4).** SBUS is on the `uart6` default (USART6 RX, PC7). The
  UART inverts it (RXINV), so no external inverter is needed. A board with
  `config save` state can rebind RC to `uart3` or `uart8`; record which one.
- **Motors (common steps 1, 4 and 9 of the powered gate).** M1-M4 are PB0, PB1
  (TIM3) and PA0, PA1 (TIM5). No output uses a complementary channel, so
  powered step 2 does not apply. The H7 timer/DMA backend is new, so this
  board runs the once-per-backend scope check. Software starts TIM3 and TIM5
  one after the other. The resulting frame offset is not measured or gated,
  because each ESC decodes its own frame.
- **ESC telemetry (powered step 4).** The ESC telemetry wire goes to UART8 RX
  (PE0).
- **Battery voltage (powered step 8).** PC0 with an 11.0 divider, against the
  nominal 3.3 V reference. Only after a pass may the user set
  `documented_baseline_verified`. The current-sensor reading (PC1, Betaflight
  scale 250) is recorded only.
- **Fault injection (`BRINGUP-COMMON-002` and `-004`).** Lucid is the first
  `ferrowasp-stm32h7` board, so it runs `bench_spi_timeout_recovery`.

Recorded, not gated:

- **MSP DisplayPort** on USART3 (PD8 TX, PD9 RX). Require the OSD to draw.
- **USB CDC** on PA11/PA12. The configurator must connect, which is
  the prerequisite for most of the above anyway.
- **microSD storage.** A card with a partition table must be refused and left
  untouched, and RTT must say why. Zero its first block (for example
  `dd if=/dev/zero of=/dev/sdX bs=512 count=1`) and boot. Require "SD storage
  ... supported true", then a blackbox download and a config commit and reload
  over USB. After FerroWasp claims the card, a PC can no longer read it.

Stop on: any stop condition of the common gate being run; a fitted IMU that
reads WHO_AM_I as no supported kind.

Required evidence: the fitted IMU kind or kinds, the serial bindings in use,
and the common gates' evidence.
