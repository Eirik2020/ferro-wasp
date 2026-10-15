# FerroWasp

FerroWasp is a safety-focused Rust/RTIC flight-controller firmware project for
multicopter UAVs, composed with the [FerroForge](https://crates.io/crates/ferroforge)
framework. It favors explicit resource ownership, deterministic
scheduling, bounded interfaces, and a small codebase that can be inspected as
a whole.

The Foxeer F405 V2 is the golden flight target. The TBS Lucid H7 carries the
same feature set on an STM32H743, so far on the bench only, and NUCLEO-F401RE is
a non-actuating STM32F4 development target. See
[Current Support](current_support.md) for the board and feature matrix.

## Where to Begin

- To install and operate a packaged Foxeer image, use the
  [User Guide](user/getting_started.md).
- To build, test, or contribute from source, use the
  [Developer Setup](developer_getting_started.md).
- To understand the current implementation, browse the flight-controller,
  interface, and development chapters in the sidebar.

## Runtime Shape

```mermaid
flowchart LR
    rc[SBUS or CRSF RC<br/>UART DMA]
    imu[ICM42688-P / MPU-6000 / MPU6500<br/>SPI1 DMA]
    safety[Safety Master]
    control[2 kHz<br/>Control Loop]
    escmgr[ESC Manager<br/>legacy telemetry]
    actuator[Actuator Output<br/>DShot600]
    motors[ESCs / Motors]
    debug[RTT / blackbox / USB]

    rc --> safety
    rc --> control
    imu --> control
    safety --> actuator
    safety --> control
    control --> actuator
    actuator --> motors
    escmgr -->|bounded request| actuator
    actuator -->|started-frame ack| escmgr
    motors -->|UART telemetry| escmgr
    control --> debug
    safety --> debug
```

FerroWasp is open source under the Apache License, Version 2.0. The firmware is
experimental and is not a stable or production flight stack. See
[Publication and Licence](publication_status.md) for the complete status and
disclaimer links.
