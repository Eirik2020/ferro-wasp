# Governing Requirements

FerroWasp is a deterministic, safety-oriented Rust/RTIC flight controller for
small multirotors, and a prototype research platform. It prefers auditability,
timing determinism, fault containment, and evidence over feature parity. It is
not a PX4, ArduPilot, or Betaflight clone.

Agreed direction, updated 2026-10-13. This document is limited to two pages
maximum.

**Authority.** `AGENTS.md` in the repository root defines the authority order and
how conflicts are resolved. These requirements override every document below
this one. If one proves impossible or impractical, call a review with the user
rather than editing around it. Implementation status does not change them.

1. **W1 - Motor authority.** Only the safety kernel and its actuator-output path
   command motor hardware. Flight modes, the configurator, host tools, logging,
   profiles, and experiments may request actuation; none owns a motor
   peripheral or bypasses arming, failsafe, actuator gating, watchdog, health, or
   command-freshness checks. Logging and disarmed configuration change data and
   permitted tuning values only.
2. **W2 - Fail closed.** Stale sensor or setpoint paths, unbounded or blocking
   real-time work, runtime panics, and leaks of actuator authority are
   high-priority defects. Unexpected motor, arming, telemetry, storage, or
   RC-loss behaviour stops the work and fails closed. Unsafe Rust is minimal,
   isolated, documented, and reviewed.
3. **W3 - Controlled configuration.** These change only with the user's approval,
   a stated reason, and revalidation at the level they affect: pin maps, DMA
   streams, timer assignments, IMU orientation and gyro signs, motor order and
   mapping, RC mapping and command signs, throttle scaling, gains, loop rate,
   DShot timing, arming, failsafe, watchdog, and gating behaviour, the stored
   configuration schema, the blackbox format, public wire and CLI formats, and
   crate layout. An orientation, motor-map, or command-sign change repeats the
   props-off axis, sign, motor identity, direction, and stick and tilt checks
   before flight.
4. **W4 - Layering.** Reusable types and safety state live in `ferrowasp-core`;
   protocols and devices in `ferrowasp-drivers`; portable flight logic in
   `ferrowasp-flight`; HAL-neutral STM32 mechanisms in `ferrowasp-stm32`; each
   family's HAL in `ferrowasp-stm32f4` or `ferrowasp-stm32h7`; shared RTIC task
   bodies in `ferrowasp-stm32-tasks`. An app shell is RTIC wiring. A board's
   physical facts live in its host-tested `firmware/<board>/board` data crate,
   and `src/board/` is HAL glue. Code still useful after substituting hardware
   facts moves to the narrowest shared crate. Every new type, resource, task,
   configuration field, and failure response has a named owning layer; reuse
   existing drivers, protocols, and safety state machines rather than building
   parallel ones. Firmware paths are `no_std`, allocation-free, and use bounded
   queues.
5. **W5 - Golden app.** `foxeer-f405-v2` is the behavioural reference. A feature
   lands there first or in the same change; another board departs from it only
   explicitly, with a rationale and board-specific verification. Silent drift is
   a defect. Golden status never licenses copying Foxeer hardware facts.
6. **W6 - The user operates the hardware.** The user performs every bench,
   motor, and flight procedure. An agent uses a probe or a powered board only at
   the user's request, and never assumes one is attached. Propellers stay off
   until the reviewed preflight prerequisites are complete.
7. **W7 - Evidence.** A compile does not establish target behaviour, a target boot
   does not establish actuator behaviour, and props-off operation does not
   establish flight readiness; each claim needs evidence at its own level.
   Evidence is tied to component IDs, and a record stands while its components
   are unchanged. Never claim a gate passed from a build. `logs/` is the only
   copy of every flight log and flown binary; never delete or rewrite it.
8. **W8 - Assurance language.** Never claim SIL, DAL, DO-178C, airworthiness,
   production safety, or certification. Say "safety-oriented",
   "certification-aligned", or "evidence-friendly". Label a claim implemented,
   transitional, planned, or aspirational, and separate what was measured from
   what was inferred.
9. **W9 - Repository boundary.** This repository is the only authority for
   FerroWasp's firmware, host tools, evidence, and plans; FerroConfigurator,
   inside it, is its host tool. FerroForge comes from crates.io: a `path` dependency or `[patch]` on a
   local checkout changes the flight image.

Implementation details belong in the public book, `mdbook/`, what is left to
build in [remaining work](remaining-work.md), and decisions in the
[decision index](ARCHITECTURE_DECISIONS.md). Keep this page concise when
updating it.
