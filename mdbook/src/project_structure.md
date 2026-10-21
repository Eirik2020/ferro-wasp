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
| `ferrowasp-border` | The border roles, safety-critical, running below the flight path and above everything else: the observer, which publishes the craft's state on a read-only ledger |
| `ferrowasp-boundary-contracts` | What the safety-critical side exchanges with the rest, safety-critical: the messages tasks send the observer and the ledger's topics |
| `ferrowasp-config` | Stored configuration, safety-critical: the configuration keys and their ranges, the configuration slot page, and the stored schema with its migration |
| `ferrowasp-control` | General control maths, safety-critical: the PID, the gyro low-pass and notch, reference frames and rotations, the attitude estimator, gyro-bias and accelerometer-level calibration |
| `ferrowasp-crsf`, `ferrowasp-blheli-telemetry` | The CRSF receiver protocol and BLHeli ESC telemetry decoding, safety-critical, one crate each |
| `ferrowasp-icm42688p`, `ferrowasp-mpu6000`, `ferrowasp-mpu6500` | The IMU drivers over `embedded-hal`, safety-critical, one crate per sensor |
| `ferrowasp-storage` | The SPI NOR flash driver and the `NorStorage` trait, the storage layout and boot slot selection, and configuration saving |
| `ferrowasp-blackbox` | The onboard flight log: the record queue and page assembly, the log writer, the sync ledger, and log maintenance |
| `ferrowasp-cli` | The command line USB and the UART carry: its commands and link, the storage, log-sync and debug commands, and the opt-in MSPv2 configurator RPC |
| `ferrowasp-flight-control` | How FerroWasp flies, safety-critical: the rate controller and its tuning, stick shaping, the mixer and motor map, dynamic idle, the RPM filter's wiring |
| `ferrowasp-feedback` | Actuator feedback, safety-critical: the ESC telemetry manager and the idle qualification arming waits for |
| `ferrowasp-imu` | The IMU group, safety-critical: the SPI request mailbox and async device the IMU is read through |
| `ferrowasp-input` | Command Input, safety-critical: the RC receiver that turns SBUS or CRSF into one frame shape, and the channel map |
| `ferrowasp-io-contracts` | The message types hardware exchanges with the safety-critical core, safety-critical: serial chunks with their continuity metadata, serial faults, timestamps and deadlines, and the queue every link between tasks is |
| `ferrowasp-protocol` | The formats shared with host tools: the onboard flight log's records and pages, and the compact rate record |
| `ferrowasp-safety` | The safety master's rules, safety-critical: arming and its guard, the RC link and its fault responses, the motor command boundary, bench motor requests, and the single-writer signals tasks share |
| `ferrowasp-serial` | The serial transport every port shares, safety-critical: port bindings, line settings and routing, and the bounded reader and writer |
| `ferrowasp-mspv1` | MSPv1 framing and the DJI O4 OSD responder |
| `ferrowasp-mspv2` | MSPv2 framing and the opt-in configurator RPC |
| `ferrowasp-osd` | The MSP OSD the DJI goggles show, encoded from snapshots of the craft's state |
| `ferrowasp-output` | Motor output, safety-critical: DShot packet and waveform encoding, the command lease and special commands, and bidirectional reply decoding |
| `rc-pwm` | RC pulse-width commands for servos and auxiliary outputs |
| `ferrowasp-stm32-critical` | The safety-critical STM32 mechanisms that need no chip-family HAL: the UART and SPI DMA state machines, the DShot motor bank, timing, routing and board-description types, reset, and the DMA buffer memory |
| `ferrowasp-stm32` | The non-critical STM32 mechanisms: ADC sampling, servo PWM, the status LED, USB serial; and re-exports of the critical crate |
| `ferrowasp-stm32f4-critical` | The STM32F4 backend's safety-critical half over `stm32f4xx-hal`: DShot, SPI and UART DMA, clocks and timing, routing, the ROM bootloader entry |
| `ferrowasp-stm32f4` | The STM32F4 backend's non-critical half: ADC, USB serial, the device ID; and re-exports of the critical half |
| `ferrowasp-stm32h7-critical` | The STM32H743 backend's safety-critical half over `stm32h7xx-hal`: DShot and its DMA endpoints, SPI and UART DMA, clocks and timers, the ROM bootloader entry |
| `ferrowasp-stm32h7` | The STM32H743 backend's non-critical half: ADC, the SD card, USB serial, the device ID; and re-exports of the critical half |
| `ferrowasp-stm32-tasks` | The RTIC task definitions every STM32 flight board shares, including the flash manager |

Only the STM32F4 and STM32H7 backend crates name a chip HAL, and only their
safety-critical halves hold unsafe code: their DMA endpoint declarations and
their jump into the chip's ROM bootloader.

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
