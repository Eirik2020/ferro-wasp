# AGENTS.md

## Project identity

This repository is for **FerroWasp**: a Rust/RTIC flight-control framework for multicopter UAVs.

Primary goal:
- Build a small, deterministic, safety-oriented flight-control platform.
- Target Betaflight-class STM32 FCUs first, with a longer-term path toward PX4/Pixhawk-class hardware.
- Prefer auditability, timing determinism, fault containment, and evidence generation over broad feature parity.

Current phase:
- The project is in rapid prototyping now.
- Move quickly for bench learning and hardware bring-up, while preserving the major safety boundaries.
- More rigid coding, safety, documentation, and evidence requirements should be introduced as the prototype matures.

Do **not** treat this as a PX4, ArduPilot, or Betaflight clone.

Read `project_docs/CODEX_PROJECT_CONTEXT.md` before making architectural changes.
Read `project_docs/CODEX_ACTIVE_WORK.md` before changing current bench/debug
workflows, motor-output behavior, logging tools, or test plans.
Read `project_docs/CROSS_REPO_SYNC.md` before moving planning, debugger, or
cross-repo material between repositories.

## Non-negotiable safety rules

- Only the **safety kernel / actuator-output path** may command motor outputs.
- Outer layers may request actuation, but they must never own motor peripherals directly.
- Experimental/labs logic must never bypass arming, failsafe, actuator gating, watchdog, or health checks.
- Marketplace/profile/configurator logic may only change allowed tuning/config parameters, never safety authority.
- Unsafe Rust must be minimized, isolated, documented, and reviewed.
- Do not claim SIL, DAL, DO-178C, or airworthiness certification. The correct claim is “certification-aligned” or “evidence-friendly” unless a specific release is actually certified.

## Architecture direction

Use a layered model:

1. `ferrowasp-core`  
   Reusable types, units, actuator commands, safety states, errors, fixed-size queues.

2. `ferrowasp-mcu`  
   Chip-family support and low-level MCU peripherals. Keep chip-family HAL details here.

3. `ferrowasp-drivers`  
   IMU, RC, ESC, telemetry, flash, sensors, and protocol drivers.

4. `ferrowasp-bsp`  
   Board-specific pin mapping, connected devices, clock tree, DMA/timer assignments.

5. `ferrowasp-tasks`  
   Reusable task logic. Avoid RTIC attributes here where practical.

6. `ferrowasp-apps`  
   Thin RTIC app shells. This is where `#[rtic::app]`, `#[task]`, `#[shared]`, `#[local]`, `#[init]`, and `#[idle]` live.

7. `ferrowasp-gen` and `manifest/`  
   Optional source-generation layer for RTIC task wiring, board config, resources, and policy checks.

## RTIC rules

- The RTIC app shell owns scheduling, priorities, resources, and task wiring.
- Task implementation should live outside the app shell and be called from generated/thin RTIC task stubs.
- Prefer static ownership and bounded queues.
- Use RTIC priorities deliberately; do not add shared resources casually.
- Safety-related tasks must have clear priority rationale and timing assumptions.
- If adding a new task, update the task/resource/policy manifest or equivalent documentation.

## FCU3 golden app and drift prevention

Treat `apps/stm32f405-flight`, using the FerroWasp FCU3 BSP, as the **golden
flight app** for established runtime behavior, safety policy, task sequencing,
and supported feature integration.

- Before implementing any feature in a new or secondary flight app, inspect
  the corresponding FCU3 implementation and identify the behavior and safety
  invariants that must be preserved.
- Implement reusable protocol, driver, safety, and task logic in the shared
  crates where practical. Board app shells should contain only the RTIC wiring
  and hardware-specific adaptation that genuinely differs.
- Do not copy an older FCU3 app-shell branch and assume it is current. Compare
  arming preparation, disarm/failsafe handling, actuator leases, fault paths,
  task priorities, logging identities, feature gates, and completion ordering
  against the current golden app.
- Before every bench test of a new or secondary app, cross-check its exact
  enabled feature set against FCU3 again. Confirm that no stale fallback,
  legacy protocol path, or obsolete safety sequence is selected by the build.
- Where the secondary board must differ because of pins, timers, DMA routes,
  sensors, orientation, electrical behavior, or unavailable hardware, document
  the deviation explicitly and cover it with board-specific tests or target
  evidence. Golden-app status does not authorize copying FCU3 hardware details
  onto another board.
- For a feature that does not yet exist in FCU3, add the reusable logic and
  integrate it into FCU3 first or in the same change when practical. If that is
  not practical, document why the secondary app leads and record the required
  FCU3 reconciliation work.
- Any intentional departure from FCU3 runtime or safety behavior requires an
  explicit rationale, review, and verification note. Silent behavioral drift
  is a defect.

## Current firmware assumptions

Current/prototyped stack:
- Embedded Rust with RTIC.
- STM32F405-class hardware is currently important for development.
- Longer-term reference target may be STM32H7-class.
- Tooling: `probe-rs`, `defmt`, `defmt-rtt`, `panic-probe`, `stm32f4xx-hal`, `rtic`, `heapless`.
- Communication paths include RC input, USB/serial telemetry/config, MSP/MAVLink subsets, and possible custom lightweight protocols.
- Motor output path includes DShot first, PWM fallback.
- RC input preference: CRSF/ELRS first; SBUS support exists/has been explored.
- IMU path should prioritize SPI IMUs such as ICM-42688-P; BMI088 is relevant as a safety/vibration option.

## Current task/path concepts

Important runtime paths:
- RC input → parsed channels/rates/throttle/arm switch.
- Safety master → arming state, fault state, arm permit, disarm decisions.
- Control loop → estimator/rate loop/attitude loop/mixer.
- Actuator output → validates permission and emits DShot/PWM.
- Telemetry/config → low-priority reporting and parameter updates.
- Fault manager/watchdog → forces safe output on invalid state.

Known safety state concepts:
- `ArmingState`
- `FaultFlags`
- `HealthState`
- `ActuatorPermission`
- validated motor outputs
- stale command detection
- RC-loss handling
- watchdog timeout handling
- actuator inhibit behavior

## Coding conventions

- Prefer `no_std`, deterministic allocation-free code in embedded crates.
- Prefer fixed-size `heapless` queues/buffers.
- Avoid dynamic allocation in firmware-critical paths.
- Avoid panics in runtime control/safety paths.
- Do not hide timing behavior behind abstractions that make WCET/latency unclear.
- Use typed units for time, rates, voltages, and actuator commands where practical.
- Keep HAL-specific types out of core logic.
- Do not rename modules broadly unless asked; preserve existing naming conventions.
- Make small, reviewable commits/patches.

## Standard commands

Run these when relevant:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets`
- `cargo check --workspace`
- `cargo test --workspace`

For embedded targets:
- Prefer `cargo check` over `cargo build` unless the target setup is known.
- Do not assume hardware runners, probes, or target MCUs are available in CI.
- If a target-specific command fails because of missing tooling, report the missing tool instead of rewriting unrelated code.

## Repository setup

Before changing setup, inspect:

- `README.md`
- `Cargo.toml`
- `Cargo.lock`
- `rust-toolchain.toml`
- `.cargo/config.toml`
- `project_docs/`
- board manifests or hardware configuration files, if present

If dependencies or commands are missing, infer as little as possible and document the assumption.

## Codex behavior

- Prefer small, reviewable patches.
- Explain architectural changes before implementing them.
- Do not invent missing hardware details.
- Ask before changing pin maps, DMA streams, timer assignments, safety states, crate layout, or public protocol formats.
- Do not perform broad refactors unless explicitly asked.
- Preserve existing naming conventions unless the task is specifically about renaming or cleanup.
- When uncertain, add a note in the response rather than silently guessing.
- For safety-relevant code, prefer explicit state machines over clever abstractions.

## Testing and verification expectations

For firmware changes, consider:
- Host unit tests for pure logic.
- SIL simulation for estimator/control/mixer behavior.
- HIL or bench tests for IMU, RC, actuator output, and timing.
- Fault-injection tests for RC loss, IMU dropout, stale commands, watchdog timeout, brownout/reset behavior, and actuator-gate faults.
- Timing measurements for interrupt/task latency and control-loop jitter.

Definition of done for safety-relevant changes:
- Clear requirement or reason.
- Unit/SIL/HIL test where feasible.
- No direct motor authority leak.
- Failure behavior documented.
- Unsafe code documented if used.
- Build/lint/test commands documented in the PR or commit notes.

## Review guidelines

Flag these as high priority:
- Any path that can command motors without going through the safety gate.
- Any experimental feature touching actuator resources, watchdog resources, or arming state directly.
- Any unbounded queue, allocation, blocking operation, or panic in a hard real-time path.
- Any unsafe code without a safety comment.
- Any stale sensor/setpoint path that can still drive actuators.
- Any licensing, certification, or safety claim that overstates the current status.
