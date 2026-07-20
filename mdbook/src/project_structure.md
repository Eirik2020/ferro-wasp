# Project Structure

The repository is currently a rapid prototype, not yet the final multi-crate layout.

Current important paths:

```text
.
|-- apps/
|   |-- stm32f405-flight/        # F405 flight RTIC shell; FCU3 selected by default
|   |-- stm32f401-bringup/       # F401 RTIC LED/USART multi-target bring-up app
|   `-- foxeer-f405-v2/          # Separate Foxeer F405 V2 RTIC flight/bring-up app
|-- crates/
|   |-- ferrowasp-bsp/           # FCU3, Foxeer, and Nucleo board contracts
|   |-- ferrowasp-core/          # Safety, signals, actuator command helpers
|   |-- ferrowasp-drivers/       # IMU and BLHeli legacy telemetry drivers
|   |-- ferrowasp-io-core/       # Portable bounded serial/SPI contracts
|   |-- ferrowasp-mspv1/         # MSPv1 parser/serializer and OSD responder support
|   |-- ferrowasp-stm32f4/       # STM32F4 UART/SPI/ADC/PWM/DShot mechanisms
|   |-- ferrowasp-tasks/         # Control, OSD, and ESC-manager task logic
|   |-- ferrowasp-waveform/      # DShot packet and encoding helpers
|   |-- ferrowasp-pid/           # no_std PID/rate-control primitive crate
|   `-- rc-pwm/                  # Local PWM controller crate
|-- mdbook/
|   `-- src/                     # This documentation book
|-- CONTRIBUTING.md              # Current contribution boundary
|-- DISCLAIMER.md                # Experimental flight-control disclaimer
|-- LICENSE.md                   # Apache License, Version 2.0
|-- NOTICE.md                    # Copyright, naming, and status notice
|-- THIRD_PARTY_NOTICES.md        # Licences for vendored documentation assets
`-- SECURITY.md                  # Security reporting expectations
```

## Current Reality

Most active flight-firmware wiring still lives in
`apps/stm32f405-flight/src/main.rs`. That is acceptable for the current
bring-up phase because it keeps hardware iteration fast while giving each
deployable image an independent Cargo/PAC graph.

The code already contains early signs of the future shape:

- reusable safety, signal, and actuator conversion types in `crates/ferrowasp-core/`
- board-specific pin, DMA, serial/SPI, timer, IRQ, profile, storage-shape, and
  construction policy in `crates/ferrowasp-bsp/`
- isolated FCU3 flight, Foxeer flight/bring-up, and NUCLEO-F401RE bring-up
  apps with independent Cargo and RTIC resource contracts
- reusable STM32F4 UART/SPI/ADC/static PWM and DShot mechanisms under
  `crates/ferrowasp-stm32f4/`
- software-driver and control helpers, including the BLHeli parser and bounded
  ESC manager, under `crates/ferrowasp-drivers/` and `crates/ferrowasp-tasks/`
- local support crates for `rc-pwm`, `ferrowasp-pid`, `ferrowasp-mspv1`, and `ferrowasp-waveform`
- active MSPv1 / DJI O4 OSD support under `crates/ferrowasp-mspv1/` and `crates/ferrowasp-tasks/`
- host-testable pure logic for safety, RC mapping, filters, PID, mixer, DShot,
  BLHeli telemetry/qualification, MSPv1, MPU6500, and ICM42688-P helpers

## Target Direction

The long-term layout is expected to move toward:

```text
ferrowasp-core     reusable types, units, safety state, queues
ferrowasp-mcu      chip-family MCU support
ferrowasp-drivers  IMU, RC, ESC, telemetry, flash, sensor drivers
ferrowasp-bsp      board pin maps, clocks, DMA/timer assignments
ferrowasp-tasks    reusable task logic
ferrowasp-apps     thin RTIC app shells
ferrowasp-gen      optional source generation for task/resource wiring
manifest/          board, task, resource, and policy descriptions
```

An app may serve multiple boards only when their BSPs satisfy the same
compile-time RTIC resource contract. A different MCU interrupt model or
firmware role gets another thin app package.

The migration should remain gradual. First make FerroWasp FCU3 work well, then
pull stable logic into cleaner modules.
