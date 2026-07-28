# Roadmap

This roadmap covers the current FerroWasp technology demonstrator. It is not a
certification, production, or airworthiness plan.

## Standing Rule

```text
Outer layers may request actuation.
Only the safety / actuator-output path may command motor hardware.
```

Every board port, telemetry path, experiment, and development tool must
preserve that boundary.

## Current Baseline: FerroWasp FCU3

The FCU3 prototype now provides:

- SBUS input over owned USART2 RX DMA;
- MPU6500 sampling over the bounded SPI1 DMA transport;
- FCU3 800 Hz IMU polling or Foxeer PC4/EXTI4 data-ready sampling and a 400 Hz
  rate-control/mixer update;
- standard, safety-owned four-lane DShot600 at 500 frame sets per second;
- PA10 / USART1 RX BLHeli legacy telemetry managed outside actuator authority;
- idle arming qualification using fresh eRPM evidence from all four ESCs;
- DJI O4 MSP DisplayPort OSD and `defmt`/BB2 logging;
- shared RC PWM infrastructure retained for servo and auxiliary outputs.

The recorded props-off gates cover motor identity and direction, mixed-command
signs, motion-opposing correction, telemetry, qualified arming, injected
stalled-motor behavior, RC loss, arm-high recovery inhibition, explicit
disarm, and restart behavior. Logic-analyzer timing and measured stop latency
remain open.

The operator reports that the first controlled outdoor flight on the standard
DShot image went well, allowed strong manoeuvres, and did not reproduce the
earlier yawing. Pitch authority felt low. The next tuning experiment should
change pitch P in small increments while leaving the other axes unchanged and
checking blackbox data for tracking error, oscillation, motor saturation, and
thermal effects. This is prototype characterization, not a validated tune.

## Phase 1: Prepare the Source Repository for Publication

Immediate work:

- publish only a sanitized history after rotating any credential that appeared
  in earlier commits;
- exclude old raw capture branches and large private bench artifacts;
- make all application builds, formatting checks, and documentation builds
  pass from a clean checkout;
- align the default branch name with CI and documentation deployment triggers;
- update public status, security-reporting, setup, licence, and attribution
  information;
- pin safety-relevant build inputs closely enough to reproduce a known source
  state;
- run a final secret and large-object scan against the exact public history.

Source publication does not imply that a firmware binary is flight-qualified.

## Phase 2: Foxeer F405 V2 Target Validation

The Foxeer app already has ROM-DFU, USB enumeration, status, ICM42688-P
runtime evidence, and staged onboard-flash support. Flight arming remains
compile-time inhibited; the flash path is implemented but lacks target evidence.

The next hardware sequence is:

1. Re-establish a clean build and boot checkpoint on the intended Foxeer board.
2. Validate the fitted IMU identity, body-axis mapping, sample freshness, and
   startup behavior.
3. Identify the SPI2 NOR, pass the isolated scratch-sector self-test, and
   validate persistent config/log recovery plus control-loop timing.
4. Calibrate ADC voltage/current scaling.
5. Measure all four DShot outputs, including TIM1_CH3N polarity on M4.
6. With propellers removed, verify logical motor order, rotation direction,
   stick response, and motion-opposing correction.
7. Review the evidence before removing the board-specific arming inhibit.

FCU3 timer, DMA, motor-map, and DShot assumptions must not be copied to Foxeer
without target evidence.

## Phase 3: Reproducible WSL/Docker Development Environment

The repository now carries a documented WSL 2/Docker reference environment
that can:

- build and test reusable crates;
- check each isolated embedded application and supported feature set;
- run formatting, strict Clippy, and mdBook validation;
- pin the Rust/tool versions used by CI;
- keep probe/USB passthrough optional so ordinary source checks do not require
  hardware access.

The image is checked only when its environment sources change. Continue
keeping it aligned with CI and the pinned nested toolchains. It complements
native flashing workflows; it does not hide target assumptions or claim
hardware validation from a cross-compile.

## Phase 4: Safety and Evidence Hardening

- add an independent actuator deadline for complete loss of future control-loop
  commands;
- make IMU initialization, gyro-bias calibration, and freshness explicit
  pre-arm prerequisites; the current first post-arm stale-IMU check requests
  disarm, but a brief armed transition remains possible;
- complete setpoint and ADC/OSD freshness policies;
- capture DShot pulse timing, jitter, polarity margin, and TIM1/TIM8 phase with
  suitable instrumentation;
- measure RC-loss and explicit-disarm stop latency;
- expand fault-injection and duration evidence;
- keep a small, sanitized evidence bundle tied to source and firmware hashes.

## Phase 5: Protocols and Developer Tools

- retain SBUS as the proven baseline while adding CRSF/ELRS;
- keep the BLHeli legacy UART manager non-authoritative and bounded: missing
  evidence may block DShot pre-arm qualification, while post-arm telemetry loss
  remains observational and does not itself disarm;
- consider bidirectional DShot telemetry separately from the current UART path;
- grow USB/configuration interfaces as validated request paths with no direct
  arming or motor authority;
- add persistent parameters only after validation and rollback policy exist.

## Longer-Term Work

- thinner RTIC app shells and more reusable task logic;
- generated board/resource policy where it improves auditability;
- STM32H7 and Pixhawk-class exploration;
- formalized hazard, requirement, timing, and traceability evidence.

Fixed-wing, VTOL, autonomy, payload control, and broad feature parity with
larger flight stacks are outside the current demonstrator scope.
