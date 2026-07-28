<p align="center">
  <img src="project_meta/FerroWasp_logo_transparent.png" alt="FerroWasp logo" width="760">
</p>

FerroWasp is a safety-focused Rust/RTIC flight-controller firmware project for
multicopter UAVs. It builds a small, inspectable flight stack around explicit
resource ownership, deterministic scheduling, and bounded hardware interfaces.

The firmware is experimental and intended for development, research, and
careful prototype testing. It is not a stable or production flight stack.

## Current State

Foxeer F405 V2 is the golden flight target and the reference for supported
runtime behavior. The matrix is intentionally short; the complete matrix and
board-specific limitations live in the
[Current Support](mdbook/src/current_support.md) chapter.

| Capability | Foxeer F405 V2 |
|---|---|
| Status | Golden flight app; boots, arms, controls, logs, and has completed controlled prototype flights |
| Platform | STM32F405, allocation-free embedded Rust, RTIC 2 |
| RC and sensing | SBUS, MPU6500 or ICM42688-P, data-ready IMU sampling, voltage/current ADC |
| Flight control | 400 Hz rate controller, filtering, gyro calibration, Quad X mixer |
| Motors | Four-lane DShot600 with guarded arming and fresh-command containment |
| ESC feedback | BLHeli legacy UART telemetry with four-motor eRPM qualification |
| Pilot interfaces | DJI O4 MSP DisplayPort OSD and USB CDC |
| Configuration and logging | Persistent tuning plus onboard SPI-NOR blackbox access through FerroConfigurator |

FerroWasp FCU3 remains a supported secondary STM32F405 flight target.
NUCLEO-F401RE is a non-actuating development target. Additional boards can be
added to the matrix as their support becomes meaningful.

## Getting Started

### Users

Use the [User Guide](mdbook/src/user/getting_started.md) to install the packaged
FerroConfigurator, flash a Foxeer F405 V2 over USB, manage configuration, and
download flight logs. The packaged workflow does not require a Rust toolchain
or source checkout.

### Developers

Use the [Developer Setup](mdbook/src/developer_getting_started.md) guide for the
pinned WSL2/Docker environment, native setup, workspace checks, embedded app
builds, and documentation workflow.

## Documentation

The mdBook is the canonical home for user and developer documentation. Its
maintained [Summary](mdbook/src/SUMMARY.md) is the documentation index; the
same book is published through GitHub Pages.

The root README stays intentionally brief. Internal agent context, retained
evidence records, and machine-readable test metadata are not user entry points
and remain outside the public documentation flow.

## Configuration and Tools

[FerroConfigurator](mdbook/src/user/ferro_configurator.md) is the primary
Foxeer companion application. It provides guided USB flashing, safe parameter
management, selective blackbox downloads, and FWBB-to-ULog conversion. See the
mdBook for usage and development details.

## Support

Issues, hardware observations, and focused design feedback are welcome through
[GitHub Issues](https://github.com/Eirik2020/ferro-wasp/issues). Report
security-sensitive findings privately as described in
[SECURITY.md](.github/SECURITY.md).

## Contributing

Contributions are welcome. See the [contribution guide](mdbook/src/contributing.md)
and the
[Developer Setup](mdbook/src/developer_getting_started.md) guide.

## Licence

FerroWasp is licensed under the Apache License, Version 2.0. See
[LICENSE](LICENSE), [NOTICE](NOTICE), and
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
