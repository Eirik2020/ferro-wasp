# Project Structure

FerroWasp is one root Cargo workspace of shared crates, plus one isolated Cargo
workspace per firmware application. Each application selects one chip's HAL and
peripheral-access crate, and keeping them apart stops Cargo from unifying two
chips' features in one build. Firmware is composed with
[FerroForge](https://crates.io/crates/ferroforge).

## Shared Crates

Under `crates/`, all members of the root workspace and host-tested there:

| Crate | Owns |
|---|---|
| `ferrowasp-control` | General control maths, safety-critical: the PID, the gyro low-pass and notch, reference frames and rotations, the attitude estimator, gyro-bias and accelerometer-level calibration |
| `ferrowasp-core` | Safety state and the safety master's rules, actuator command types, blackbox records, stored configuration |
| `ferrowasp-drivers` | Device and protocol drivers: IMUs (ICM-42688-P, MPU-6000, MPU-6500), CRSF, BLHeli telemetry, SPI NOR flash, and the `NorStorage` trait |
| `ferrowasp-flight` | Portable flight logic: arming, control, the ESC manager, blackbox and configuration storage, log sync, the command link |
| `ferrowasp-io-core` | Bounded serial and SPI contracts, I/O health, statistics, and time types |
| `ferrowasp-mspv1` | MSPv1 framing and the DJI O4 OSD responder |
| `ferrowasp-mspv2` | MSPv2 framing and the opt-in configurator RPC |
| `ferrowasp-waveform` | DShot packet encoding and compare sequences |
| `rc-pwm` | RC pulse-width commands for servos and auxiliary outputs |
| `ferrowasp-stm32` | STM32 mechanisms that need no chip-family HAL: UART, SPI, ADC, the DShot motor bank, board-description types |
| `ferrowasp-stm32f4` | The STM32F4 backend over `stm32f4xx-hal` |
| `ferrowasp-stm32h7` | The STM32H743 backend over `stm32h7xx-hal` |
| `ferrowasp-stm32-tasks` | The RTIC task definitions every STM32 flight board shares, including the flash manager |

Only `ferrowasp-stm32f4` and `ferrowasp-stm32h7` name a chip HAL, and only
they hold unsafe code: their DMA endpoint declarations and their jump into the
chip's ROM bootloader.

A safety-critical crate declares `[package.metadata.ferrowasp] critical =
true` and never depends on a crate that does not; its external dependencies
are pinned to one version.

## Firmware Applications

Under `firmware/`, each its own Cargo workspace:

| Application | Board |
|---|---|
| `foxeer-f405-v2` | Foxeer F405 V2, STM32F405; the golden flight app |
| `tbs-lucid-h7` | TBS Lucid H7, STM32H743 |
| `stm32f401-bringup` | NUCLEO-F401RE; peripheral bring-up without actuators |

A flight application has three parts:

```text
firmware/<board>/
|-- board/        board data crate: pins, DMA and serial routes, resource claims,
|                 default configuration, USB identity; no HAL, host-tested
|-- src/board/    HAL glue: type aliases and peripheral construction
|-- src/lib.rs    the facade main.rs imports
`-- src/main.rs   the ferroforge::app! declaration: resources, priorities,
                  init wiring, and task selection only
```

Each board data crate is also a member of the root workspace, so
`cargo test --workspace` runs its tests; the application's own workspace
excludes it and depends on it by path. [Board Support](board_support.md)
describes what a board owns.

## Host Tools

| Path | What |
|---|---|
| `tools/ferro-configurator/` | FerroConfigurator, its own Cargo workspace: CLI and GUI for flashing, configuration, motor tests, and log download |
| `tools/*.py` | Bench, RTT, analysis, evidence, and repository checks |
| `tools/rtic-app-builder/` | The earlier app generator, its own workspace, due to be retired |

## Documentation

`mdbook/` is this book. `project_meta/` holds internal maintainer material and
is not published.
