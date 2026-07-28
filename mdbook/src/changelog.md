# Changelog

This page records notable FerroWasp changes without implying that a firmware
artifact is stable, airworthy, or production-ready.

## Unreleased

No user-facing changes have been recorded after the `v0.1.0` release
candidate.

## 0.1.0 - 2026-07-28

First public experimental pre-release.

### Added

- Isolated RTIC applications for FerroWasp FCU3, Foxeer F405 V2, and
  NUCLEO-F401RE.
- Allocation-free STM32F4 sensor, serial, DMA, DShot600, ESC-telemetry, USB,
  configuration-storage, and blackbox support.
- The isolated FerroConfigurator desktop workspace for Foxeer flashing,
  configuration, blackbox download, and ULog conversion.
- Bounded host-side repository, evidence, and RTIC-boundary checks.

### Changed

- Foxeer F405 V2 is now the golden flight application.
- RTIC `main.rs` files are thin wiring shells; reusable mechanisms live in
  shared crates, while physical board facts remain in each app's `src/board/`.
- DShot600 is the standard flight ESC protocol. RC PWM remains available for
  servo and auxiliary-output use.
- Foxeer USB, onboard flash storage, blackbox logging, and ESC telemetry are
  standard board services rather than optional app-local implementations.

### Fixed

- Strengthened SBUS startup/freshness handling, guarded DShot arming and stop
  behavior, synchronized TIM1/TIM8 output, and bounded ESC-telemetry
  association.
- Corrected FCU3 motor mapping, output polarity, scheduling cadence, and
  roll/pitch control direction issues found during prototype validation.

### Removed

- Removed the standalone `ferrowasp-bsp` crate. Board facts now live with each
  isolated app, while reusable STM32F4 behavior lives in
  `ferrowasp-stm32f4`.

## Development History

- 2025-12: initial repository and SBUS parsing.
- 2026-02 to 2026-04: STM32F4 bring-up, DMA transports, sensors, actuator
  experiments, and control-loop iteration.
- 2026-05 to 2026-06: mixer, arming, OSD, safety documentation, and reusable
  control foundations.
- 2026-07: isolated board apps, bounded STM32F4 mechanisms, DShot600 and ESC
  telemetry promotion, onboard logging/configuration, and the first controlled
  prototype-flight reports.

Current support and limitations belong in [Current Support](current_support.md);
planned work belongs in the [Roadmap](roadmap.md); test evidence remains in the
internal evidence system.
