# Changelog

All notable changes to this repository are tracked here.

FerroWasp has no tagged releases yet. This changelog describes the current
experimental source tree; it is not a declaration that a firmware artifact is
airworthy or production-ready.

## [Unreleased] - Multi-Target STM32 Prototype

### Added

- Isolated RTIC 2 applications for FerroWasp FCU3, Foxeer F405 V2, and
  NUCLEO-F401RE, with typed board-support profiles and independent Cargo graphs.
- Allocation-free MPU6500 and ICM42688-P drivers over the bounded asynchronous
  SPI DMA transport.
- Owned, bounded UART RX/TX paths for SBUS and DJI O4 MSP DisplayPort traffic.
- Default four-lane DShot600 output on FerroWasp FCU3, plus capped diagnostic
  images and an explicit four-channel RC PWM fallback.
- A synchronized TIM1/TIM8 DShot bank with per-lane DMA completion accounting,
  a command lease, an in-flight deadline, and whole-bank fault containment.
- BLHeli legacy ESC telemetry on PA10 / USART1 RX using DMA, a fixed-storage
  ten-byte parser, and CRC-8 validation.
- A low-priority ESC manager that rotates telemetry requests and associates
  responses through bounded request and acknowledgement queues. The
  safety-owned actuator service remains the only consumer that may request a
  telemetry bit in an emitted DShot frame.
- Telemetry-qualified DShot arming. The actuator owner requires three fresh
  idle eRPM observations from each ESC in the configured 3,000-10,000 eRPM
  window before the safety master can declare the system armed.
- Compact `BB2` rate-control logging and host-side analysis tools.
- Optional Foxeer USB CDC status snapshots plus staged onboard SPI-NOR
  discovery, dual-slot whitelisted configuration storage, CRC-protected flight
  logging, download/analyzer tooling, and a reserved-sector write self-test.
- FerroConfigurator as an isolated in-repository Windows workspace, with a
  manifest- and SHA-256-verified ready Foxeer release image, ROM-DFU flashing,
  complete disarmed-only parameter access, flight-selective/resumable blackbox
  downloads, and native minimal ULog conversion.
- An opt-in Foxeer MSPv2 configurator endpoint with bounded incremental
  framing, `FWSP` identification, versioned postcard RPC, whole-config
  stage/commit/reset, and disarmed-only CRC-protected blackbox chunks. It is
  gated separately from normal USB/status images and has no safety or actuator
  authority.
- Foxeer PC4/EXTI4 IMU data-ready sampling with bounded deferred SPI DMA
  requests and rejected-trigger diagnostics.

### Changed

- Set the Foxeer fresh-storage tuning baseline to roll/pitch/yaw P
  `2.5 / 2.5 / 2.0`, with all I and D gains zero. Existing valid persisted
  configuration continues to override these initialization defaults.
- Moved reusable logic into `ferrowasp-core`, `ferrowasp-drivers`,
  `ferrowasp-io-core`, `ferrowasp-stm32f4`, `ferrowasp-bsp`,
  `ferrowasp-tasks`, and related support crates while keeping concrete RTIC
  ownership in the isolated application shells.
- Routed active and diagnostic motor vectors through the bounded SPSC
  `MotorCmd` queue. Actuator output drains to the latest command and rejects
  stale, missing, non-finite, or invalid data.

### Fixed

- Kept the Foxeer USB log-summary response within its 64-byte bounded frame at
  maximum counter widths. Host tools also recover the unterminated legacy
  response when a periodic `FWDBG1` record supplied its accidental newline.
- Kept interactive FerroConfigurator console text ASCII-only so the Windows
  flash wizard does not display UTF-8 punctuation as mojibake.
- Added SBUS startup qualification, 100 ms link expiry, immediate invalidation
  on transport/parser/failsafe faults, and an arm-low recovery interlock.
- Replaced the FCU3 DShot PWM-style pre-arm delay with a guarded 100 ms stop
  dwell followed by actuator-owned idle eRPM qualification.
- Established Betaflight Quad X logical motor numbering and the measured FCU3
  logical-to-physical map `[3, 4, 2, 1]`.

### Fixed

- Corrected the TIM1_CH3N polarity used by FCU3 motor output 4.
- Corrected the DShot service cadence from approximately 333 Hz to the intended
  500 Hz by using absolute RTIC monotonic deadlines.
- Closed the arming-idle abort path so a revoked permit, RC loss, arm-switch
  drop, or high throttle forces stop before reporting failure.
- Corrected FCU3 RC roll/pitch mapping and pitch correction direction.
- Removed obsolete PID and sequence diagnostics from the flight OSD.
- Made ESC telemetry identity explicitly physical-output based and report the
  corresponding logical motor, avoiding ambiguity with the FCU3 output map.
- Latched the legacy telemetry manager off after an acknowledgement or response
  timeout so a late identity-free frame cannot be assigned to a later output.

### Target Evidence

- FCU3 DShot600 passed synchronized unpowered runtime, powered props-off idle
  and throttle, unequal-vector, logical-motor identity, motor-direction,
  explicit-disarm, RC-loss, arm-high recovery-inhibit, and restart-interlock
  checks with zero reported DShot backend faults in the retained runs.
- PA10 BLHeli legacy telemetry passed 3,950 request/acknowledgement/response
  cycles without reported timeout, CRC, mismatch, unsolicited-frame, or
  discarded-byte errors. eRPM tracked stop, idle, and throttle; the other ESC
  telemetry fields remain unsupported or unvalidated on the installed ESCs.
- Positive DShot arming qualification passed on all four motors. An injected
  zero-eRPM physical-output-1 fault (logical M4/front-left) correctly aborted
  after the bounded qualification interval, selected four stop values, and
  never declared the system armed.
- Props-off blackbox captures confirmed roll, pitch, and yaw stick mixing and
  motion-opposing corrections through the standard DShot image.
- The operator reports that the first outdoor flight on the standard DShot path
  went well, supported strong manoeuvres, and no longer exhibited the previous
  yawing. Pitch authority felt lower than desired; pitch-P adjustment remains a
  measured, incremental tuning task. This flight report is operator evidence,
  not an instrumented timing or airworthiness result.
- NUCLEO-F401RE passed its RTIC heartbeat smoke test. Foxeer F405 V2 passed ROM
  DFU programming, USB enumeration, ICM42688-P detection, and a five-minute
  read-only USB status soak; its actuator path remains inhibited pending
  board-specific validation.

### Known Prototype Gaps

- Logic-analyzer evidence for DShot pulse timing, jitter, polarity margin, and
  TIM1/TIM8 phase alignment is still open.
- Physical stop latency is operator-observed but not instrumented.
- Independent detection of total future control-loop command loss is not yet
  implemented; the current freshness check runs when actuator output is woken.
- IMU freshness/calibration is not yet an explicit arming prerequisite. A stale
  IMU requests disarm on the first armed control tick, so a dead IMU can still
  briefly reach the armed state; this must be closed before calling the arming
  policy complete. Setpoint and ADC/OSD freshness policies also remain
  incomplete.
- The estimator and controller tune remain prototype-level. The recent flight
  result does not make the firmware validated, airworthy, or production-ready.
- CRSF/ELRS, persistent parameters, a complete telemetry/config protocol,
  formal traceability, and a release evidence package remain future work.

## Historical Development Summary

- 2025-12: Initial repository and early SBUS parsing support.
- 2026-02: STM32F405 bring-up, USART DMA, SBUS DMA, debug LED, and early
  DShot/PWM experiments.
- 2026-03: MPU6500 SPI DMA, ADC DMA, RC PWM, and throttle-path cleanup.
- 2026-04: IMU/control-loop iteration.
- 2026-05: Mixer, arming logic, MSP/OSD, and initial safety documentation.
- 2026-06: Workspace and reusable PID foundations.
- 2026-07: Explicit BSPs and isolated applications, bounded DMA transports,
  DShot600 promotion, BLHeli legacy telemetry, telemetry-qualified arming, and
  the first reported DShot flight.
