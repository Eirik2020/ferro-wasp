# FerroWasp Codex Project Context

This document is compact project memory for coding agents working in the
`ferro-wasp` repository.

For cross-repo boundaries, also read:

- `project_docs/CROSS_REPO_SYNC.md`

Before changing current bench/debug workflows, motor-output behavior, logging
tools, or test plans, also read:

- `project_docs/CODEX_ACTIVE_WORK.md`

## Repository Role

This repository is the source of truth for the current FerroWasp prototype /
technology demonstrator.

It owns:

- embedded firmware code and build configuration;
- current STM32F405 board assumptions;
- motor map, gyro signs, RC mapping, gains, loop rates, and active features;
- actuator-output, arming, failsafe, watchdog, and safety implementation docs;
- BB2 logging format and firmware-side tooling contracts;
- bench and flight evidence.

Non-firmware planning belongs in the sibling `..\ferro-planning` repository
unless it directly affects current firmware work.

## Current Phase

FerroWasp is in rapid prototyping.

The FCU3 STM32F405-class prototype now boots, senses, arms through
telemetry-qualified DShot idle, controls, logs, and has completed a successful
controlled outdoor flight while preserving the actuator-authority boundary.
The immediate goal is to publish a sanitized, accurate experimental source
baseline, then target-verify the Foxeer F405 V2 port.

This repo should be presented as a working prototype and research platform, not
as a certified, airworthy, production-ready, or validated flight stack.

## Current Baseline

The active bench target is an STM32F405-class RTIC firmware prototype with:

- `no_std`, `no_main` Rust firmware;
- RTIC 2 scheduling;
- `stm32f4xx-hal`;
- SBUS RC input over USART2 RX DMA;
- MPU6500 IMU over SPI1 on FCU3, plus runtime-selected MPU6500 or ICM42688-P
  over the same bounded DMA transport on Foxeer;
- FCU3 800 Hz IMU polling, Foxeer PC4/EXTI4 data-ready sampling, and 400 Hz
  control/output update;
- prototype complementary roll/pitch estimate;
- rate controller and Quad-X mixer;
- safety-gated four-lane DShot600 ESC output by default, with an explicit
  four-channel PWM fallback;
- PA10 / USART1 legacy BLHeli telemetry in the default DShot image only, with
  bounded ESC-manager association and actuator-owned idle-eRPM qualification;
- DJI O4 MSPv1 OSD over UART4;
- ADC DMA observation for voltage/current/temperature;
- `defmt`/RTT logging and compact BB2 control-loop frames;
- optional Foxeer USB CDC status plus staged onboard SPI-NOR log/config access;
- capped FCU3 DShot600 bench modes separate from the default mixed-control
  DShot output.

FerroWasp FCU3 is the validated flight baseline. Foxeer F405 V2 has a separate
RTIC app and BSP with the same supported flight-service subset, ICM42688-P/EXTI
sampling, default DShot600/eRPM-qualified arming, and an explicit four-channel
RC PWM fallback. Its normal flight profile and final normal-mixer/OSD
props-off handoff passed, but its first prop-on departure exposed positive
pitch feedback and attempted a forward flip. A physical-body/controller pitch
compatibility correction is implemented and passed new unpowered-orientation
and powered normal-mixer props-off checks. The clean logged image is programmed
and boot-verified; the controlled hop remains. Its base `usb_serial` path
reports bounded status only.
Flash-enabled variants
accept a bounded, whitelisted storage/config command set while disarmed; USB
cannot request arming, alter safety state, or command actuators.

The detailed support matrix lives in:

- `mdbook/src/current_support.md`

The current bench/flight handoff lives in:

- `project_docs/CODEX_ACTIVE_WORK.md`

## Core Safety Boundary

```text
Outer layers may request actuation.
Only the safety / actuator-output path may command motor hardware.
```

Current meaning:

- RC input parses pilot intent.
- Safety logic decides arming/permission state.
- Control logic computes requested motor outputs.
- Actuator output owns or directly drives motor peripherals.
- Telemetry, USB, configurators, OSD, debugger tooling, and experiments must
  never directly command motors or grant actuator authority.

## Current Priority Order

Verify against `CODEX_ACTIVE_WORK.md` before acting, but the standing priority
order is:

1. Prepare a clean, sanitized, internally consistent public source baseline;
   rotate the historical probe credential before changing visibility.
2. Consolidate the current working tree onto the intended public default branch
   and make all supported target/CI checks pass.
3. Repeat the corrected Foxeer controlled-field first hop. Both props-off gates
   and the clean-image programming/boot check passed; the pre-fix image remains
   withdrawn from flight use.
4. Establish a reproducible WSL/Docker development environment after Foxeer
   target bring-up.
5. Return to FCU3 flight characterization with the isolated pitch P
   `0.25 -> 0.30` experiment and a BB2 capture where practical.
6. Validate the standard DShot backend independently with a logic analyzer;
   the successful experimental flight does not establish pulse width, jitter,
   polarity margin, or cross-timer phase.
7. Add an independent actuator deadline watchdog for completely absent future
   commands.
8. Strengthen stale/invalid IMU, RC-link quality, and ADC/OSD freshness policy.
9. Add CRSF/ELRS only after the current SBUS/F405 baseline remains stable.
10. Extract reusable modules only when behavior is covered by tests and docs.

## Controlled Configuration

Do not casually change:

- motor order or `MOTOR_OUTPUT_MAP`;
- board orientation;
- gyro axis/sign mapping;
- RC channel mapping or command signs;
- throttle scaling;
- active gains;
- PWM timing;
- loop rate;
- BB2 frame format;
- arming, failsafe, watchdog, or actuator-gating behavior.

Changes in those areas require an explicit task, evidence, documentation, and a
small patch.

## Current Known Gaps

- the standard FCU3 DShot600 path has powered props-off and controlled-flight
  evidence, but logic-analyzer pulse width, jitter, M4 polarity margin, and
  TIM1/TIM8 phase measurements remain open;
- telemetry-qualified arming is target-validated: the actuator owner applies
  idle under a temporary permit and requires three fresh in-range eRPM samples
  from every ESC before the safety master may arm;
- active motor commands use the bounded SPSC `MotorCmd` path with actuator-owned
  freshness validation; stale-command injection and normal runtime have target
  evidence;
- independent detection of a completely absent future motor command is not yet
  implemented;
- SBUS RC loss, immediate motor stop, arm-high recovery inhibition, and fresh
  low-to-high rearm are target-validated, but broader link-quality policy is
  still needed;
- IMU initialization, gyro-bias calibration, and freshness are pre-arm
  prerequisites in both flight apps; negative target fault injection remains
  to be captured;
- ADC/OSD freshness is incomplete;
- motor identity, Betaflight logical mapping, CW/CCW rotation, mixed-command
  direction, reset/reconnect arm-high inhibition, BLHeli legacy eRPM telemetry,
  and the negative idle-RPM arming fault are target-validated on FCU3;
- the latest flight removed the previously observed yawing by pilot report,
  while pitch authority felt low; the next isolated tuning candidate is pitch
  P `0.30`, with mixer saturation and directional/CG effects still to be
  distinguished;
- CRSF/ELRS is not implemented;
- estimator and tune are prototype-level;
- typed FCU3, Foxeer F405 V2, and NUCLEO-F401RE manifests exist, but no final
  board generator exists;
- deployable FCU3, Foxeer, and F401 firmware packages are isolated under
  `apps/`; this prevents incompatible PAC features from being unified and
  keeps board-specific RTIC resource contracts separate;
- no formal evidence package or traceability matrix exists.

## Architecture Direction

Keep the current implementation moving toward these boundaries without forcing
a broad refactor before the prototype is stable:

```text
ferrowasp-core    pure types, units, actuator commands, safety states
ferrowasp-mcu     chip-family peripheral support
ferrowasp-drivers IMU, RC, ESC, telemetry, flash, sensor drivers
ferrowasp-bsp     board pin maps, connected devices, DMA/timer assignments
ferrowasp-tasks   reusable task logic
ferrowasp-apps    thin RTIC app shells
ferrowasp-gen     optional manifest/generator layer
```

HAL-specific types should not leak into core logic. The RTIC app shell owns
scheduling, priorities, resources, and task wiring.

The root Cargo workspace contains reusable crates. Firmware commands must run
from the selected isolated package:

```text
apps/stm32f405-flight  STM32F405 flight RTIC contract; FCU3 selected by default
apps/stm32f401-bringup STM32F401 RTIC bring-up contract; Nucleo selected by default
apps/foxeer-f405-v2    STM32F405 Foxeer RTIC contract; default DShot flight candidate
```

## Evidence and Documentation

For safety-relevant changes, record:

- reason or requirement;
- firmware commit and branch;
- hardware configuration;
- enabled features;
- active safety-relevant configuration;
- tests run;
- logs/evidence location;
- known limitations.

Relevant docs:

- `project_docs/PUBLICATION_CHECKLIST.md`
- `project_docs/CODEX_ACTIVE_WORK.md`
- `mdbook/src/current_support.md`
- `project_docs/BENCH_TEST_PLAN.md`
- `mdbook/src/roadmap.md`
- `project_docs/CROSS_REPO_SYNC.md`

## Assurance Language

Allowed:

- safety-oriented;
- certification-aligned;
- evidence-friendly;
- designed for traceability;
- prototype;
- research platform.

Do not claim:

- certified;
- airworthy;
- SIL-rated or SIL-certified;
- DAL-rated;
- DO-178C compliant;
- production safe;
- mission-qualified.

## Long-Term Planning

For non-firmware planning, use:

```text
..\ferro-planning
```

This repo may link to those plans, but should not duplicate them or become the
source of truth for them.
