# Roadmap

This roadmap records product and engineering direction. Current board support
belongs in [Current Support](current_support.md); dated test results and active
work logs do not belong here.

## Flight Robustness

- add an independent actuator deadline for complete loss of future control-loop
  commands;
- expand IMU freshness, calibration, RC-loss, and actuator fault injection;
- measure DShot timing, cross-timer phase, jitter, and physical stop latency;
- finish bounded I-term behavior, estimator validation, and systematic flight
  tuning;
- extend blackbox records with self-describing configuration, per-motor eRPM,
  accelerometer, saturation, and crash-analysis fields.

## Foxeer User Experience

- keep USB, DShot, ESC telemetry, blackbox, and persistent configuration as
  standard Foxeer capabilities;
- improve guided FerroConfigurator workflows while preserving firmware-owned
  validation;
- make release images and configuration migrations reproducible and easy to
  verify;
- calibrate voltage and current sensing for supported airframes.

## Protocols and Interfaces

- add CRSF/ELRS while retaining SBUS;
- evaluate bidirectional DShot telemetry separately from the current legacy
  UART telemetry path;
- mature configuration and telemetry protocols only after their bounds,
  recovery behavior, and compatibility policy are explicit;
- keep RC-PWM available for servos and auxiliary devices rather than ESCs.

## Platform Architecture

- continue thinning RTIC apps so they contain resource declarations, task
  bindings, scheduling, and board composition rather than reusable logic;
- move reusable STM32F4 initialization and mechanisms into
  `ferrowasp-stm32f4` without hiding runtime work behind custom macros;
- keep physical pins, DMA routes, timer assignments, sensor orientation, and
  other board facts in each app support module;
- add boards only when their explicit hardware contract and verification path
  are maintainable;
- explore STM32H7 and Pixhawk-class targets after the STM32F4 architecture is
  stable.

## Engineering Assurance

- keep the development container, CI, and pinned toolchains reproducible;
- grow host tests, bounded target procedures, and compact evidence generation;
- make timing, unsafe-code, fault-containment, and actuator-authority changes
  straightforward to audit;
- maintain the mdBook as the single public user and developer documentation
  set.
