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
| `ferrowasp-sensor-traits` | The sensor classes, safety-critical: one trait each for the IMU, the barometer and GNSS, their samples in integer units, the stamped envelope and sequence every class shares, the common error kinds, and the freshness judgement |
| `ferrowasp-sensors` | Adapters from self-contained `embedded-hal` sensor drivers to the sensor classes, owning their units, plausibility limits, deadlines and freshness: the DPS310 barometer, over `uf-dps3xx` |
| `ferrowasp-storage` | The SPI NOR flash driver and the `NorStorage` trait, the storage layout and boot slot selection, and configuration saving |
| `ferrowasp-blackbox` | The onboard flight log: records from the observer's stream of logged control ticks, page assembly, the log writer, the sync ledger, and log maintenance |
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
| `ferrowasp-osd` | The MSP OSD the DJI goggles show, encoded from the craft's published state, with a warning when it stops updating |
| `ferrowasp-output` | Motor output, safety-critical: DShot packet and waveform encoding, the command lease and special commands, and bidirectional reply decoding |
| `rc-pwm` | RC pulse-width commands for servos and auxiliary outputs |
| `ferrowasp-stm32-critical` | The safety-critical STM32 mechanisms that need no chip-family HAL: the UART and SPI DMA state machines, the DShot motor bank, timing, routing and board-description types, reset, and the DMA buffer memory |
| `ferrowasp-stm32` | The non-critical STM32 mechanisms: ADC sampling, servo PWM, the status LED, USB serial; and re-exports of the critical crate |
| `ferrowasp-ferrohal-critical` | The backend's safety-critical half over FerroHAL: DShot on its waveform generator, SPI and UART, timing and reset. The UART and SPI adapters exist only until the transports move to FerroHAL's own model |
| `ferrowasp-ferrohal` | The backend's non-critical half: ADC, the SD card, USB serial, the device ID; and re-exports of the critical half |
| `ferrowasp-stm32-app` | What every STM32 flight application shares above its board: the application facade, the task groups and the queues between tasks; and, until the hardware message contracts, the actuator output, DShot service, ESC telemetry and RC input tasks |

Only the STM32F4 and STM32H7 backend crates name a chip HAL, and only their
safety-critical halves hold unsafe code: their DMA endpoint declarations and
their jump into the chip's ROM bootloader.

Task definitions live in their subsystem's crate, behind a target-only
`tasks` feature, so the logic beside them stays host-tested: the safety
master in `ferrowasp-safety`, the control loop in `ferrowasp-flight-control`,
the observer in `ferrowasp-border`, the SPI1 IMU and UART transports in
`ferrowasp-stm32-critical`, the command line's in `ferrowasp-cli`, the OSD's
and RC telemetry in `ferrowasp-osd`, and the ADC and status LEDs in
`ferrowasp-stm32`.

A safety-critical crate declares `[package.metadata.ferrowasp] critical =
true` and never depends on a crate that does not; its external dependencies
are pinned to one version.

## Device Drivers

Under `drivers/`, also members of the root workspace and host-tested there.
A device driver is a standalone crate over `embedded-hal`, `embedded-io` or
`embedded-hal-bus` that depends on no FerroWasp crate and carries no project
name, so it is usable outside FerroWasp. FerroWasp implements its own traits
for a driver in the subsystem crate that uses it. The same safety-critical
marking and pinning rules apply.

| Crate | Owns |
|---|---|
| `crsf-proto` | The CRSF receiver protocol, safety-critical: the bounded streaming frame parser, the battery, GPS, vario, barometric altitude, attitude and flight-mode telemetry encoders, and a frame reader and telemetry writer over `embedded-io`. No firmware image links it; RC input decodes CRSF through `ferrowasp-crsf` |
| `ubx` | The u-blox UBX protocol for an M10 GNSS receiver: framing with resynchronisation inside dropped frames, configuration through `CFG-VALSET` and `CFG-VALGET` with their acknowledgements, `NAV-PVT` decoding that exposes a position only for a valid 3D fix, and transports over `embedded-io` serial and `embedded-hal` I2C |

## Firmware Applications

Under `firmware/`, each its own Cargo workspace:

| Application | Board |
|---|---|
| `foxeer-f405-v2` | Foxeer F405 V2, STM32F405; the golden flight app |
| `tbs-lucid-h7` | TBS Lucid H7, STM32H743 |

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
