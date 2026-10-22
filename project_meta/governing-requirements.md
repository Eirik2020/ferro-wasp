# Governing Requirements

FerroWasp is a deterministic, safety-oriented Rust/RTIC flight controller for
small multirotors, built on FerroForge, and a prototype research platform. It
puts auditability, timing determinism, fault containment, and evidence before
feature count.

Agreed direction, updated 2026-10-22. This document is limited to two pages
maximum.

**Authority.** `AGENTS.md` in the repository root defines the authority order and
how conflicts are resolved. These requirements override every document below
this one. If one proves impossible or impractical, call a review with the user
rather than editing around it. Implementation status does not change them.

1. **W1 - Motor authority.** Only the safety master and its actuator-output path
   command motor hardware. Flight modes, FerroConfigurator, host tools, logging,
   profiles, and experiments may only request actuation, and the firmware
   accepts or refuses. Nothing bypasses arming, failsafe, actuator gating,
   watchdog, health, or command-freshness checks. A disarmed bench request, such
   as the motor test, runs under a short firmware-held lease that lapses when
   renewals stop.
2. **W2 - Fail closed.** Stale sensor or setpoint data, unbounded or blocking
   real-time work, runtime panics, and leaks of actuator authority are
   high-priority defects. A fault leaves the motors stopped and the system
   disarmed, never in a guessed state.
3. **W3 - FerroForge.** FerroForge is FerroWasp's main framework, and the two are
   developed together. Each firmware is a `ferroforge::app!`, and reusable tasks
   are `#[ferroforge::task]` definitions in shared crates. When FerroWasp needs
   something FerroForge lacks, it is built in FerroForge rather than worked
   around in FerroWasp. FerroWasp builds against FerroForge as released on
   crates.io; a `path` dependency or `[patch]` changes the flight image.
4. **W4 - Layering.** Shared logic is free of HAL types. A board's physical facts
   are host-tested data, and an app shell is RTIC wiring. Code still useful after
   substituting hardware facts moves to the narrowest shared crate, and each new
   type, task, or failure response has one owning layer. Code is
   safety-critical or not, and safety-critical code never depends on
   non-critical code. Firmware is `no_std`,
   allocation-free, and uses bounded queues. Unsafe code lives only at
   chip-backend peripheral and DMA boundaries, and is documented. The crate map
   is `mdbook/src/project_structure.md`.
5. **W5 - Golden app.** `foxeer-f405-v2` is the behavioural reference. A feature
   lands there first or in the same change. Where another board leads, the
   change records why, and remaining work records bringing Foxeer level. Any
   other departure is explicit, with a rationale and board-specific
   verification. Golden status never licenses copying Foxeer's hardware facts.
6. **W6 - Controlled configuration.** Pin maps, DMA streams, timer assignments,
   IMU orientation and signs, default motor order, RC mapping and command signs,
   throttle scaling, default gains, loop rate, DShot timing, arming, failsafe,
   watchdog, and gating behaviour, the stored-configuration schema, the blackbox
   format, and public wire and CLI formats change only with a stated reason and
   the revalidation that [the test documentation](testing/README.md) assigns to
   what changed.
7. **W7 - Evidence.** A compile does not establish target behaviour, a target
   boot does not establish actuator behaviour, and props-off operation does not
   establish flight readiness; each claim needs evidence at its own level.
   Evidence verifies the requirements in `project_meta/requirements/`, each
   derived from W1-W7 or from another requirement. A record stands while the
   code and configuration it covered are unchanged, and status is computed
   from evidence, never recorded by hand.

Implementation details belong in the public book, `mdbook/`, what is left to
build in [remaining work](remaining-work.md), and decisions in the
[decision index](ARCHITECTURE_DECISIONS.md). Keep this page concise when
updating it.
