# FerroWasp — ChatGPT Project Context

This is a self-contained architectural brief intended for upload to a ChatGPT
Project. It supplies durable project context, safety boundaries, terminology,
and reasoning rules without requiring ChatGPT to infer the architecture from
individual source files.

FerroWasp changes rapidly. This document deliberately avoids becoming the
authority for live bench or flight status. When current state matters, verify
it against `project_meta/CODEX_ACTIVE_WORK.md`,
`mdbook/src/current_support.md`, the selected app, and the current board support.

## Project identity

FerroWasp is an experimental Rust/RTIC flight-control firmware framework for
multicopter UAVs. Its goals are a small, understandable, deterministic, and
safety-oriented stack whose scheduling, resource ownership, failure behavior,
and evidence are inspectable.

The project is in rapid prototyping. It is a working research platform and
technology demonstrator, not certified, airworthy, production-ready, or safe
for operational use.

Broad support for Betaflight- and PX4-class boards is an accepted aspirational
direction. FerroWasp is not intended to clone PX4, ArduPilot, or Betaflight.
Their hardware definitions and established practices may be used as input
without inheriting their complete software architecture.

## Repository responsibility

The `ferro-wasp` repository is authoritative for:

- embedded firmware and build configuration;
- active board assumptions and typed board contracts;
- motor mapping, orientation, RC mapping, gains, rates, and enabled features;
- arming, failsafe, watchdog, and actuator-output behavior;
- firmware-side telemetry, logging, and configuration contracts;
- bench and flight evidence tied to firmware behavior.

Sibling repositories have different responsibilities:

- `ferro-debugger` owns Raspberry Pi probe, flashing, RTT streaming, and
  remote-debug workflow implementation;
- `ferro-planning` owns non-firmware plans and cross-repository work packages;
- `rtic-app-builder` is the standalone prototype for assembling reusable RTIC
  tasks/components into board applications;
- `ferro-configurator` owns the user-facing configuration workflow.

Planning summaries do not override the current FerroWasp implementation.
Avoid duplicating rapidly changing status across repositories.

## Non-negotiable safety boundary

```text
Outer layers may request actuation.
Only the safety-owned actuator-output path may command motor hardware.
```

This means:

- RC input publishes pilot intent but grants no motor authority;
- estimation and control calculate requested behavior but do not own motor
  peripherals;
- the safety master determines whether actuation is permitted;
- the actuator owner is the sole path that applies validated commands to
  DShot or PWM hardware;
- OSD, telemetry, USB, configuration, storage, logging, debugger tools, and
  experimental code cannot arm, grant authority, or command motors directly;
- missing, stale, malformed, or faulted inputs must not create authority;
- failsafe, disarm, lease expiry, watchdog, and backend-fault paths select a
  safe output through the same authority boundary.

Experimental and bench modes must preserve this boundary. A compile-time
feature gate may restrict output further, but it must not bypass arming,
permission checks, bounded command transport, or actuator ownership.

## Architecture

The current repository is organized around these practical layers:

```text
ferrowasp-core
    portable safety states, frames, signals, actuator commands, and units

ferrowasp-actuator
    actuator authority, command validation, mapping, state, and faults

ferrowasp-io-core
    portable bounded serial, SPI, waveform, time, health, and routing contracts

ferrowasp-drivers
    IMU, ESC telemetry, flash, and other device/protocol drivers

ferrowasp-stm32f4
    STM32F4 clocks, UART/SPI/ADC DMA, timers, PWM, DShot, and memory mechanisms

app src/board and src/lib.rs
    board pins, clocks, peripherals, DMA/timer routes, orientation, profiles,
    storage shape, construction policy, and internal support facade

ferrowasp-tasks
    reusable control, OSD, ESC-manager, storage, and service task logic

protocol/helper crates
    ferrowasp-mspv1, ferrowasp-mspv2, ferrowasp-waveform, ferrowasp-pid, rc-pwm

apps
    isolated RTIC application shells and their independent PAC/build graphs
```

HAL-specific types should not leak into portable core or functional logic.
RTIC app shells currently own static scheduling, priorities, shared/local
resources, interrupt bindings, and final task wiring. Stable logic should move
into reusable crates gradually and only with tests that preserve behavior.

## Firmware applications

Deployable applications are isolated from the root workspace because
different PAC features and RTIC resource contracts must not be unified:

```text
apps/stm32f405-flight
    FerroWasp FCU3 secondary flight application with retained target evidence

apps/foxeer-f405-v2
    Foxeer F405 V2 golden flight application and runtime baseline

apps/stm32f401-bringup
    NUCLEO-F401RE bring-up application with no actuator outputs
```

Treat `apps/foxeer-f405-v2` as the golden reference for established runtime
behavior and safety policy. Before implementing a feature in a secondary app,
inspect the corresponding Foxeer behavior and identify the invariants that must
remain synchronized. Hardware differences must come from the target board support, not
from copying FCU3 pin, DMA, timer, orientation, or sensor assumptions.

Do not alter either flight-tested application during unrelated architectural
experiments. Create a parallel app or feature-gated replacement, then perform
explicit static, bench, and flight reconciliation before promotion.

## Runtime flow

The intended high-level data and authority flow is:

```text
RC hardware endpoint
    -> bounded protocol parsing
        -> qualified pilot intent

IMU hardware endpoint
    -> bounded sample transport
        -> calibrated body-frame state

pilot intent + body state
    -> estimator/controller/mixer
        -> bounded requested motor command

safety state + fresh command + healthy actuator backend
    -> sole actuator owner
        -> DShot/PWM motor hardware

latest system state
    -> OSD / logging / telemetry / configuration observers
```

Hardware interrupt work should be short and bounded. Parsing, encoding,
storage, and other substantial work should be deferred. Firmware-critical
paths should remain allocation-free and should use fixed-capacity queues or
latest-value state with explicit freshness semantics.

## Components, endpoints, and capabilities

Use the following terminology when discussing the future app-builder boundary:

- **Task** — an actual RTIC hardware or software task.
- **Component** — a reusable functional unit or implementation, such as OSD,
  SBUS, an IMU service, or a UART-DMA driver.
- **Endpoint component** — a reusable hardware-provider implementation.
- **Endpoint instance** — an endpoint component bound to a concrete
  peripheral, pins, DMA routes, buffers, and interrupts.
- **Capability** — a typed, bounded contract provided by one component or
  endpoint and consumed by another without transferring peripheral ownership.

For example:

```text
UART-DMA endpoint instance
    provides SerialRxTx
        consumed by MSP DisplayPort component
```

The current repository already contains reusable bounded I/O contracts and
hardware mechanisms, but a final general component/capability manifest and
board-application generator are not implemented here yet.

## Board support, application, platform, and backend ownership

These concerns must remain separate:

### App-local board support

Each isolated app owns immutable physical facts under `src/board/`: board pins,
connected devices, peripheral instances, clock constraints, DMA and timer
routes, interrupts, memory/storage shape, electrical properties, and
sensor-to-board orientation. `src/lib.rs` exposes the internal support facade.
Reusable mechanisms do not remain board-local.

### Application

The RTIC application owns compile-time composition: selected components,
static task wiring, priorities, bounded capacities, and the concrete resource
contract compiled into a binary.

### Persisted platform configuration

The longer-term platform configuration assigns generic compiled endpoints to
user-selected profiles at boot. Examples include OSD, SBUS, CRSF, GPS, motor
ordering, motor direction, and FCU orientation. UART baud/framing/inversion
belong to the selected protocol profile rather than immutable board support.

The active platform configuration is validated and frozen for the boot.
Changes take effect only after persistence and reboot; runtime routing does not
move interrupts, DMA streams, buffers, or HAL ownership.

### MCU/backend implementation

The MCU layer derives mechanical HAL facts such as alternate functions, typed
DMA direction, peripheral setup, and validated clock behavior. Unsupported
hardware combinations must fail explicitly rather than receive guessed
fallbacks.

This configuration split is architectural direction. Do not describe the full
boot router or generated-app integration as implemented until the relevant
code and target evidence exist.

## Frames, orientation, and controlled configuration

The shared physical body frame is forward/right/down:

- +X forward;
- +Y right;
- +Z down;
- angular rates follow the right-hand rule.

Board profiles express sensor-frame to board-frame and board-frame to
drone-body transformations explicitly. Historical controller-sign
compatibility must remain visible at the controller boundary and must not be
hidden by falsifying a measured physical orientation.

Do not casually change:

- logical or physical motor order;
- motor direction;
- board or FCU orientation;
- gyro/accelerometer axis mapping or signs;
- RC channel mapping, rate signs, or arm-switch behavior;
- throttle scaling, active gains, filtering, or output limits;
- loop rates, timer cadence, RTIC priorities, or deadline assumptions;
- PWM/DShot timing, polarity, DMA routes, or synchronization;
- BB2/logging formats;
- arming, failsafe, watchdog, freshness, or actuator-gating policy.

Such a change requires an explicit task, a small reviewable patch, updated
documentation, and target evidence appropriate to the risk. Orientation,
motor-map, or command-sign changes require props-off axis/sign, motor identity,
motor direction, and stick/tilt response revalidation before flight.

## Current implementation snapshot

At the time this upload brief was created, the repository contains:

- `no_std`, `no_main` RTIC 2 STM32F4 applications;
- an FCU3 flight-tested prototype baseline;
- a separate Foxeer F405 V2 app and board support under active target verification;
- SBUS input over bounded UART DMA;
- MPU6500 and ICM42688-P support through bounded SPI paths;
- prototype estimation, rate control, Quad-X mixing, and typed frame rotation;
- safety-gated four-lane DShot600 as the standard ESC protocol;
- bounded legacy ESC telemetry and arming-qualification support;
- DJI MSPv1 DisplayPort OSD;
- ADC observation, RTT/defmt diagnostics, and BB2 logging;
- feature-gated Foxeer USB/configuration and onboard SPI-NOR work;
- host-testable protocol, mapping, control, waveform, safety, and driver logic.

This is a context snapshot, not a flight authorization. For the latest board
status, withdrawn images, required props-off gates, hashes, and next field
step, use `project_meta/CODEX_ACTIVE_WORK.md` and
`mdbook/src/current_support.md`.

## Known architectural and assurance gaps

Verify the live gap list before acting. Standing themes include:

- additional actuator deadline and absent-command supervision;
- broader IMU, RC-link, ADC, and OSD freshness/fault policy;
- logic-analyzer evidence for DShot timing, jitter, polarity margin, and
  cross-timer phase;
- prototype-level estimator and tuning maturity;
- CRSF/ELRS and wider board coverage;
- continued extraction of reusable task logic from large RTIC shells;
- typed capability composition and generated application wiring;
- SIL/HIL, negative fault injection, timing reports, and traceability;
- replacement of transitional tooling or compatibility boundaries only after
  canonical interfaces are stable.

Do not treat a successful flight as proof of electrical timing, complete
failure coverage, production readiness, or general board compatibility.

## Evidence and assurance language

Safety-relevant work should record:

- reason or requirement;
- commit, branch, app, board, and enabled features;
- active safety-relevant configuration;
- exact commands and tests;
- target hardware and wiring state;
- log/evidence location;
- observed result and remaining limitations.

Acceptable language includes “prototype,” “research platform,”
“safety-oriented,” “certification-aligned,” and “evidence-friendly.” Do not
claim certified, airworthy, SIL/DAL-rated, DO-178C compliant, production safe,
or mission-qualified.

Propellers must be removed for motor order, direction, waveform, arming, and
actuator bench tests unless an explicitly reviewed field-test plan authorizes
otherwise. Unexpected hardware behavior means stop, disarm/de-energize, and
inspect rather than iterating blindly.

## How ChatGPT should approach architectural work

1. Identify whether the request is explanation, planning, implementation, or
   safety-relevant behavior change.
2. Label claims as **implemented**, **transitional**, **planned**, or
   **aspirational**.
3. Verify volatile status in the active handoff and selected app/board-support before
   reasoning from it.
4. State which layer owns every new type, resource, task, configuration field,
   and failure response.
5. Preserve the sole actuator-authority boundary and bounded-memory model.
6. Compare secondary-board runtime behavior against the Foxeer golden app while
   preserving genuine board differences.
7. Reuse existing FerroWasp crates and proven paths; do not create parallel
   drivers, protocols, safety state machines, or control stacks without a
   concrete reason.
8. Avoid broad refactors during active flight testing. Prefer parallel,
   feature-gated, or otherwise isolated prototypes.
9. Ask before choosing unknown pins, DMA streams, timer routes, orientation,
   motor maps, public protocol formats, or safety policy.
10. Explain safety, timing, memory, resource, migration, and evidence effects
    of a proposal—not only its type-level elegance.

## Verification expectations

Choose checks proportionate to the change:

- host unit tests for portable logic;
- formatting and strict Clippy where supported;
- root workspace tests/checks for reusable crates;
- checks and release builds from each affected isolated app directory;
- feature-conflict and compile-time policy tests;
- rendered documentation checks;
- bench/HIL tests for peripheral ownership, timing, RC, IMU, and actuators;
- fault injection for stale/missing data, transport faults, watchdogs, and
  actuator leases;
- retained logs and measurements for flight or waveform claims.

A compile pass does not establish target correctness. A target boot does not
establish actuator correctness. Props-off operation does not establish flight
readiness. Each claim needs evidence at the appropriate level.

## Source precedence

When information conflicts, use this order:

1. The user's latest explicit decision and safety instruction.
2. The current selected app and board support, shared crates, feature configuration, and
   tests.
3. `project_meta/CODEX_ACTIVE_WORK.md` and
   `mdbook/src/current_support.md` for live state and evidence.
4. Accepted, non-superseded entries in
   `project_meta/ARCHITECTURE_DECISIONS.md`.
5. `project_meta/CODEX_PROJECT_CONTEXT.md`, the mdBook architecture pages,
   and this upload brief for durable context.
6. Historical implementation plans and chronological checkpoint sections.
7. Sibling-repository planning summaries.

An older checkpoint can be technically valuable without describing current
defaults. Check dates, feature gates, and whether a later ADR supersedes it.

## Essential companion documents

When repository access is available, consult:

- `project_meta/CODEX_ACTIVE_WORK.md` — live bench/flight handoff;
- `mdbook/src/current_support.md` — current target and feature matrix;
- `project_meta/ARCHITECTURE_DECISIONS.md` — durable decisions and
  supersession;
- `project_meta/testing/README.md` — test selection and verification gates;
- `project_meta/CROSS_REPO_SYNC.md` — repository authority boundaries;
- `AGENTS.md` — contributor and coding-agent safety rules;
- the README and selected app README for supported commands.

If only this file is uploaded, treat any precise current target status as
something to confirm with the user before proposing a flight, bench, pin-map,
orientation, motor-output, or safety-policy change.
