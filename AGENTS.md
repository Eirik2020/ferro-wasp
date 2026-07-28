# AGENTS.md

## Project identity and routing

FerroWasp is a small, deterministic, safety-oriented Rust/RTIC flight-control
framework. Prefer auditability, timing determinism, fault containment, and
evidence generation over broad feature parity. The project is in rapid
prototyping, but the major safety boundaries remain mandatory. Do not treat it
as a PX4, ArduPilot, or Betaflight clone.

This file applies repository-wide. Before scoped work, also read:

- `apps/AGENTS.md` for firmware applications;
- `apps/foxeer-f405-v2/AGENTS.md` for the Foxeer app;
- `tools/AGENTS.md` for host tooling;
- `tools/ferro-configurator/AGENTS.md` for the isolated USB configurator;
- `tools/rtic-app-builder/AGENTS.md` for the isolated builder workspace;
- `project_docs/AGENTS.md` for project documentation.

Nested instructions add local rules and never weaken this file. Load only the
documents routed by the applicable instructions:

- architectural change: `project_docs/CODEX_PROJECT_CONTEXT.md`;
- live bench, motor, logging, or test-plan work:
  `project_docs/CODEX_ACTIVE_WORK.md`;
- test selection or execution: `project_docs/testing/README.md`;
- cross-repository movement: `project_docs/CROSS_REPO_SYNC.md`.

## Non-negotiable safety rules

- Only the safety kernel and safety-owned actuator-output path may command
  motor peripherals. Outer layers may request actuation but never own the
  hardware.
- Experimental, configurator, marketplace, and profile logic must not bypass
  arming, failsafe, actuator gating, watchdog, health, or command-freshness
  checks.
- Unsafe Rust must be minimized, isolated, documented, and reviewed.
- Do not claim SIL, DAL, DO-178C, airworthiness, or certification. Use
  “certification-aligned” or “evidence-friendly” unless a release is actually
  certified.
- Treat stale sensor/setpoint paths, unbounded work, blocking real-time work,
  runtime panics, and actuator-authority leaks as high-priority defects.

## Architecture boundary

Keep reusable types and safety state in `ferrowasp-core`; MCU support in
`ferrowasp-mcu`; protocols and devices in `ferrowasp-drivers`; reusable task
logic in `ferrowasp-tasks`; and RTIC wiring in thin app shells. Keep each
board's physical facts in that isolated app's `src/board/` support module.
Reusable STM32F4 mechanisms and configuration types belong in
`ferrowasp-stm32f4`, not in board support. Optional generation belongs in
`ferrowasp-gen`, manifests, or the isolated builder.

Keep HAL-specific types out of core logic. Put reusable behavior in shared
crates rather than duplicating it between boards.

Treat each app's src/board directory as hardware data and narrow adaptation,
not as a general implementation layer. If code remains useful after
substituting pins, peripherals, routes, or configuration values, move it to
the narrowest shared crate before adding another board copy.

## Engineering rules

- Prefer `no_std`, allocation-free firmware paths and fixed-size bounded
  queues.
- Avoid panics in runtime control/safety paths and abstractions that obscure
  latency or worst-case work.
- Use typed units for time, rates, voltages, and actuator commands where
  practical.
- Preserve existing names and make small, reviewable patches. Do not perform
  broad refactors unless asked.
- Prefer explicit safety state machines over clever abstractions.
- Infer as little as possible when dependencies, commands, or hardware facts
  are missing; report the assumption.

Ask before changing pin maps, DMA streams, timer assignments, safety states,
crate layout, or public protocol formats. Explain architectural changes before
implementing them.

## Verification

Run checks proportional to the change:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo check --workspace`
- `cargo test --workspace`

For embedded targets, prefer `cargo check` unless the target setup is known.
Do not assume a probe, MCU, runner, or actuator power is available. Report
missing target tooling rather than rewriting unrelated code.

For safety-relevant changes, provide a clear reason, failure behavior,
unit/SIL/HIL coverage where feasible, confirmation that motor authority did not
move, and any unsafe-code or timing implications.
