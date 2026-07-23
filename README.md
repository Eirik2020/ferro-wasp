<p align="center">
  <img src="FerroWasp_logo_transparent.png" alt="FerroWasp logo" width="760">
</p>

FerroWasp is an experimental Rust/RTIC flight-control firmware prototype for
multicopter UAVs.

It is focused on a small, understandable STM32 flight-control stack where RC
input, IMU sampling, control loops, motor mixing, arming, failsafe behavior,
and actuator authority are easy to inspect.

FerroWasp is not a PX4, ArduPilot, or Betaflight clone. It is not certified,
airworthy, production-ready, or suitable for operational use. The current goal
is fast bench learning and cautious prototype flight testing while preserving
one central safety boundary:

```text
Outer layers may request actuation.
Only the safety / actuator-output path may command motor hardware.
```

## Project Status

FerroWasp is in rapid prototyping.

| Area | Current state |
|---|---|
| Primary target | FerroWasp FCU3, STM32F405-class hardware |
| Secondary targets | Foxeer F405 V2 bring-up, NUCLEO-F401RE RTIC heartbeat |
| Runtime | `no_std`, `no_main`, RTIC 2 |
| RC input | SBUS over USART2 RX DMA |
| IMU | MPU6500 on FCU3; runtime-selected MPU6500/ICM42688-P on Foxeer |
| Control | FCU3 800 Hz IMU polling or Foxeer PC4/EXTI4 data-ready sampling, 400 Hz control update, prototype rate loop |
| Motor output | Safety-gated four-lane DShot600 by default on FCU3 and Foxeer; explicit RC PWM fallbacks |
| ESC telemetry | Default DShot images: BLHeli/KISS legacy UART telemetry on PA10/USART1 RX; eRPM validated on all four ESCs on both boards |
| Logging/debug | `defmt`/RTT, compact BB2 frames, and opt-in Foxeer SPI-NOR blackbox/config storage with USB CLI or native MSPv2 RPC |
| OSD/telemetry | DJI O4 MSPv1 OSD on UART4 and Foxeer USB CDC status/storage/configurator access |

More detail lives in the current support matrix:
[mdbook/src/current_support.md](mdbook/src/current_support.md).

On 2026-07-20, the standard FCU3 DShot600 image completed a controlled
experimental outdoor flight. The pilot reported strong maneuvering performance
and no recurrence of the prior yawing behavior. Pitch authority appeared low;
incremental pitch-P testing is the next tuning follow-up. DShot electrical
waveform timing and phase measurements remain open, and this flight is
prototype evidence rather than an airworthiness or production-safety claim.

The Foxeer F405 V2 first-hop attempt on 2026-07-22 exposed positive pitch
feedback and attempted a forward flip. The blackbox-backed polarity correction
has passed both the unpowered frame-sign check and powered normal-mixer
props-off opposition check. The pre-fix image is withdrawn from flight use; a
clean logged image is now programmed and boot-verified, leaving the
conservative controlled hop as the remaining field step.

## Features

Current firmware capabilities:

- Embedded Rust flight firmware using RTIC 2 scheduling
- Safety-owned actuator path for arming, idle, armed output, and disarm
- SBUS RC parsing with link qualification, timeout invalidation, and rearm latch
- SPI IMU sampling through bounded DMA transport
- Standard drone body frame using forward/right/down axes, with board profiles
  rotating IMU sensor axes into the board frame and then into the drone frame
- Gyro filtering, startup gyro-bias calibration, and prototype complementary
  roll/pitch estimate
- PID/feedforward rate controller with quad-X motor mixing
- Betaflight Quad X logical motor numbering:
  motor 1 rear-right, motor 2 front-right, motor 3 rear-left, motor 4
  front-left
- Board-specific logical-to-physical motor output maps
- Four-lane synchronized DShot600 as the standard FCU3 motor protocol
- Explicit four-channel 400 Hz RC PWM fallback build
- Opt-in, capped FCU3 DShot600 bench backend for equal-motor,
  logical-motor, and fixed unequal-vector validation
- Normal PID/mixer output through the fresh, leased, safety-owned DShot path
- Dedicated bounded ESC-manager queues that request telemetry through the
  actuator owner rather than gaining direct motor authority
- Legacy UART ESC telemetry decoding on PA10/USART1 RX in the default DShot
  image, with fresh eRPM used by the actuator-owned idle-spin arming
  qualification; inactive in the PWM fallback
- ADC DMA observation for voltage, current, and internal temperature
- DJI O4 MSPv1 DisplayPort OSD output
- Compact BB2 control-loop logging over `defmt-rtt`
- Opt-in Foxeer onboard SPI-NOR logging and dual-slot persistent tuning storage
- Python RTT logger, analyzer, and IMU live-view tools
- Isolated firmware app packages for FCU3, Foxeer F405 V2, and F401 bring-up

## Getting Started

Prerequisites for a fresh checkout:

- Git and [rustup](https://rustup.rs/). The repository pins its Rust nightly,
  Clippy, rustfmt, and the active Cortex-M target in `rust-toolchain.toml`.
- Python 3 for the repository's host-side logging and analysis tools.
- `probe-rs` for FCU3 SWD flashing and RTT sessions.
- STM32CubeProgrammer for Foxeer F405 V2 ROM-DFU flashing.
- mdBook `0.5.2` and mdbook-mermaid `0.17.0` to build the public book.

After cloning, install/confirm the pinned toolchain and run the hardware-free
workspace checks:

```powershell
rustup show active-toolchain
cargo test --workspace --locked
cargo check --workspace --locked --target thumbv7em-none-eabihf
```

Install the documentation tools when needed:

```powershell
cargo install mdbook --version 0.5.2 --locked
cargo install mdbook-mermaid --version 0.17.0 --locked
mdbook build mdbook
```

The default branch is `main`. CI and documentation deployment are configured
against it, and repository rules require pull requests, passing checks, linear
history, and protection from deletion and force-pushes.

## Documentation

The mdBook is the intended public documentation surface:

- Overview: [mdbook/src/chapter_1.md](mdbook/src/chapter_1.md)
- Current support matrix: [mdbook/src/current_support.md](mdbook/src/current_support.md)
- DShot notes: [mdbook/src/dshot.md](mdbook/src/dshot.md)
- Roadmap: [mdbook/src/roadmap.md](mdbook/src/roadmap.md)
- RTT/debug tools: [mdbook/src/rtt_debug_tools.md](mdbook/src/rtt_debug_tools.md)
- Target verification checklist: [TARGET_VERIFICATION.md](TARGET_VERIFICATION.md)
- Publication checklist:
  [project_docs/PUBLICATION_CHECKLIST.md](project_docs/PUBLICATION_CHECKLIST.md)

## Firmware Apps

The repository root is a workspace for reusable crates. Deployable firmware
images are isolated so incompatible STM32 PAC features cannot be unified:

```text
apps/stm32f405-flight  RTIC 2 flight app; FerroWasp FCU3 by default
apps/stm32f401-bringup Minimal F401 RTIC LED/USART bring-up app
apps/foxeer-f405-v2    RTIC 2 Foxeer flight app; default DShot flight candidate
```

Run firmware commands from the selected app directory.

For FCU3:

```powershell
cd apps/stm32f405-flight
cargo build --locked
```

For Foxeer F405 V2:

```powershell
cd apps/foxeer-f405-v2
.\flash-dfu.ps1 -BuildOnly
```

With an SWD retrofit connected, `cargo run --release --locked` uses `probe-rs`
to program and run the Foxeer target. ROM-DFU recovery remains available
through `flash-dfu.ps1`; add `-UsbDebug` there for the opt-in read-only USB CDC
diagnostics.

## Configuration And Tools

FerroWasp has an opt-in, feature-gated native MSPv2 configurator endpoint on
the Foxeer USB CDC port. It reports `FWSP`, exposes only the existing
whitelisted tuning object and bounded onboard-blackbox reads, and retains the
flash manager's disarmed-only write policy. The current stable repository-local
tools and the ASCII storage endpoint remain available when the MSPv2 gate is
not selected.

Current bring-up and debug workflows are repository-local:

- `tools/README.md` for host and remote debug tooling
- `project_docs/FLIGHT_TEST_QUICK_COMMANDS.md` for current FCU3 bench/field
  commands
- `tools/blackbox_analyzer.py` for compact BB2 log analysis
- `tools/ferrowasp_storage.py` for Foxeer onboard logs and whitelisted settings
- `tools/imu_live_view.py` for live IMU/control observation
- `apps/foxeer-f405-v2/README.md` for the `mspv2_configurator` build contract

Telemetry and configuration interfaces are intentionally limited at this stage.
USB, OSD, logging, and analyzer paths must not gain motor authority or change
safety state.

## Motor Numbering And Safety

Shared flight logic uses Betaflight Quad X logical motor numbering:

| Logical motor | Corner |
|---:|---|
| 1 | Rear-right |
| 2 | Front-right |
| 3 | Rear-left |
| 4 | Front-left |

Each board profile maps those logical motors to physical output pads. Re-test
motor order, motor direction, and stick/tilt response with propellers removed
before any flight on every actuator-capable board, including FCU3 after mapping
changes, Foxeer before its first flight, and any future
actuator-capable target. The F401 bring-up board declares no actuator outputs.

Important rule for contributors: do not add a path that can command motors
outside the safety/actuator-output path.

Arming now requires a supported IMU that has produced data, completed startup
gyro-bias calibration, and remains fresh. The same health guard is rechecked
during actuator preparation. This is implemented in both FCU3 and Foxeer app
shells; negative target fault-injection evidence remains a follow-up.

## Roadmap

Near-term FCU work:

- complete the remaining visibility-dependent launch checks and publish the
  prepared sanitized `main` baseline
- increment pitch P cautiously while checking commanded-rate tracking and
  mixer headroom
- pass the Foxeer corrective props-off pitch-opposition gate and repeat the
  controlled-field first hop with a new retained image
- establish a reproducible WSL/Docker development environment after Foxeer
  bring-up
- preserve the target-validated arming, disarm, RC-loss, and actuator-gating
  behavior
- target-validate stale motor-command rejection and add an independent actuator
  deadline watchdog
- target-validate the healthy/calibrated/fresh IMU pre-arm prerequisite and
  complete ADC/OSD freshness handling
- capture more short flight and characterization logs
- validate DShot timing and synchronization with a logic analyzer
- add CRSF/ELRS after the SBUS/F405 path is stable

Longer-term direction:

- cleaner board profiles and generated task/resource policy checks
- STM32H7 reference target
- BMI088 support and deeper ICM42688-P validation
- target-validate and harden the new Foxeer persistent blackbox/config path
- SIL/HIL tests, fault-injection tests, timing reports, and traceability

## Support

Issues, bug reports, bench-test notes, hardware observations, and design
feedback are welcome.

Report security-sensitive findings through the private process in
[SECURITY.md](SECURITY.md), not through a detailed public issue.

For current implementation state, start with:

- [project_docs/CODEX_PROJECT_CONTEXT.md](project_docs/CODEX_PROJECT_CONTEXT.md)
- [project_docs/CODEX_ACTIVE_WORK.md](project_docs/CODEX_ACTIVE_WORK.md)
- [project_docs/BENCH_TEST_PLAN.md](project_docs/BENCH_TEST_PLAN.md)

## Developers

Before changing setup, inspect the root workspace and the selected isolated app
package. Useful commands:

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets
cargo check --workspace
cargo test --workspace
```

Firmware commands should usually run from one of the app directories under
`apps/`. Prefer `cargo check` for embedded targets unless the target setup is
known. Do not assume a probe, ESC power, or target board is available in CI.

## Contributing

Code, documentation, test notes, and careful issue reports are welcome under the
Apache-2.0 contribution terms described in [CONTRIBUTING.md](CONTRIBUTING.md).

For safety-relevant changes, include the reason, test evidence where feasible,
and any remaining bench or target-validation gaps. Do not claim SIL, DAL,
DO-178C, airworthiness, or production safety for this prototype.

## Hardware

FerroWasp FCU3 is the current flight-tested baseline. Foxeer F405 V2 is a WIP
flight candidate with default DShot600, target-verified motor/RC/IMU/eRPM and
blackbox paths, and runtime IMU pre-arm checks. Its first-hop pitch-polarity
failure is corrected and the repeated props-off opposition gate passed; a clean
logged image is programmed, and a conservative hop remains before flight
validation.
NUCLEO-F401RE is a dev-board scheduler and USART-heartbeat target with no
actuator outputs.

If hardware behaves unexpectedly, stop testing and inspect the board, wiring,
ESCs, and motors. Propellers must be removed for motor-order, motor-direction,
waveform, arming, and actuator bench tests.

## Releases

There is no stable FerroWasp release yet. Treat this repository as prototype
source and evidence, not as a released flight stack.

## Open Source

FerroWasp is licensed under the Apache License, Version 2.0. See
[LICENSE.md](LICENSE.md).

See also [DISCLAIMER.md](DISCLAIMER.md), [NOTICE.md](NOTICE.md),
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md), and
[CONTRIBUTING.md](CONTRIBUTING.md).
