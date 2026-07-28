# Project Structure

The repository is currently a rapid prototype, not yet the final multi-crate layout.

Current important paths:

```text
.
|-- apps/
|   |-- stm32f405-flight/        # FCU3 board support and thin F405 RTIC shell
|   |-- stm32f401-bringup/       # Nucleo board support and minimal F401 RTIC shell
|   `-- foxeer-f405-v2/          # Golden Foxeer board support and RTIC flight app
|       |-- src/board/           # Immutable board facts and typed construction
|       |-- src/lib.rs           # Internal app-support facade
|       `-- src/main.rs          # RTIC declarations and wiring only
|-- crates/
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
|-- .github/
|   |-- CONTRIBUTING.md          # GitHub pointer to the canonical guide
|   `-- SECURITY.md              # Private security-reporting policy
|-- LICENSE                      # Apache License, Version 2.0
|-- NOTICE                       # Copyright, naming, and status notice
`-- THIRD_PARTY_NOTICES.md       # Licences for vendored assets
```

## Current Reality

Each deployable board has an independent Cargo/PAC graph. Its `src/main.rs`
contains the RTIC resource and task declarations, locking, scheduling, and
concrete initialization wiring. Board declarations and constructors live in
the same package's support library; reusable behavior remains in shared crates.
Foxeer is the golden flight app.

The code already contains early signs of the future shape:

- reusable safety, signal, and actuator conversion types in `crates/ferrowasp-core/`
- board-specific pin, DMA, serial/SPI, timer, IRQ, profile, storage-shape, and
  construction policy under each app's `src/board/`
- isolated FCU3 flight, Foxeer flight/bring-up, and NUCLEO-F401RE bring-up
  apps with independent Cargo and RTIC resource contracts
- reusable STM32F4 UART/SPI/ADC, servo/auxiliary PWM, and DShot mechanisms under
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
ferrowasp-core       reusable types, units, safety state, queues
ferrowasp-mcu        family-neutral MCU contracts
ferrowasp-drivers    IMU, RC, ESC, telemetry, flash, sensor drivers
ferrowasp-stm32f4    reusable STM32F4 mechanisms and config types
ferrowasp-tasks      reusable task logic
app src/board        board pin maps, clocks, DMA/timer assignments
app src/lib.rs       board composition and internal support facade
app src/main.rs      thin RTIC shell
ferrowasp-gen        optional source generation for task/resource wiring
manifest/            optional board, task, resource, and policy descriptions
```

Boards use isolated app packages so their PAC features and RTIC resource
contracts cannot be accidentally unified.

Reusable behavior moves outward only when its ownership and bounded execution
are explicit; custom macros are not used to hide runtime work.
