# RTIC App Builder Current Roadmap

Last updated: 2026-07-27

## Status And Authority

This is the bounded current roadmap for the isolated RTIC App Builder under
`tools/rtic-app-builder`. It identifies checked implementation, active
constraints, current gaps, and the next ordered milestones. It is an
implementation plan, not evidence that proposed work exists, is target-tested,
is flight-ready, or is certified.

The complete accepted 2026-07-24 reference baseline is preserved verbatim at
[`docs/archive/RTIC_APP_BUILDER_REFERENCE_IMPLEMENTATION_PLAN_FULL_BASELINE_2026-07-24.md`](docs/archive/RTIC_APP_BUILDER_REFERENCE_IMPLEMENTATION_PLAN_FULL_BASELINE_2026-07-24.md).
Its retained identity is:

- size: `154211` bytes;
- SHA-256:
  `1DB2C24B4595C290589E757D4F0A398944CC20DD9B20BF8AF4CE5DCE2D4C8846`.

Relative links inside that immutable file reflect its original location at the
builder root; resolve them from `tools/rtic-app-builder`.

Use the archive for a specific detailed requirement, schema example, staged
work package, risk analysis, or provenance question. Do not load it as default
builder context or treat its illustrative paths and syntax as implemented.

When sources conflict, use:

1. the user's latest explicit architecture or safety decision;
2. repository-wide FerroWasp safety rules and accepted architecture decisions;
3. checked code, strict manifests/contracts, tests, and target evidence;
4. this current roadmap;
5. focused builder design notes;
6. the archived full baseline and older historical plans.

## Document Routing

Start with `README.md` for current commands and implemented behavior. Then load
only the focused source needed:

| Need | Read |
|---|---|
| Current prototype evidence and vocabulary | `docs/architecture-observations.md` |
| Adopted versus deferred FerroWasp crate reuse | `docs/backend-unification.md` |
| Current STM32F4 backend behavior | `docs/stm32f4-backend.md` |
| OSD/USART1 DMA implementation and provenance | `docs/osd-usart1-dma.md` |
| Board-capability and boot-frozen configuration direction | `docs/betaflight-target-definition-notes.md` |
| Component/endpoint/capability authoring | `docs/authoring/README.md` |
| Hardware procedure | `docs/hardware-smoke-test.md` |
| Full 2026-07-24 specification | the archived baseline linked above |

Current FerroWasp flight state, golden-app behavior, board facts, and target
evidence remain owned by the monorepo root. Pin the commit and inspect those
sources when builder work depends on them.

## Executive Direction

The builder is a deterministic static application-composition compiler. It
will consume explicit physical board facts, an application profile, versioned
component metadata, a selected backend, and build policy. It will produce a
versioned resolved application, readable RTIC Rust, architecture/provenance
reports, and checked or built firmware artifacts.

The builder selects, validates, names, connects, and renders existing Rust
implementations. It does not generate flight algorithms, driver state
machines, protocol parsers, control logic, or safety policy.

The protected migration direction is:

```text
working handwritten validation prototypes
    -> typed endpoint/consumer composition
    -> deterministic resolved application
    -> generated validation applications
    -> output-inhibited parallel F405 candidate
    -> separately reconciled release path
    -> additional backends and targets
```

The current handwritten golden flight app remains authoritative until any
generated replacement is independently reconciled through static, electrical,
bench, and flight evidence. Builder work must not incidentally replace or
broadly refactor it.

## Non-Negotiable Constraints

- Only a safety-owned actuator-output component may command motor hardware.
  The resolved graph must reject duplicate authority or observer/configuration
  paths to actuation.
- Every pin, peripheral, DMA route, interrupt, timer channel, static buffer,
  and hardware state machine has one explicit owner.
- Runtime configuration may choose among compiled logical roles but cannot
  transfer HAL/PAC objects, interrupts, DMA, buffers, timers, or motor
  authority.
- Critical-path queues, buffers, snapshots, and journals require exact
  capacity, overflow behavior, fault propagation, and tests.
- Ambiguous pins, routes, providers, priorities, dispatchers, or capacities
  are errors. There is no hidden fallback.
- Component metadata is declarative. It may reference reviewed Rust
  implementations but cannot inject arbitrary Rust fragments.
- One architecture-aware renderer owns application composition; permanent
  feature-specific source renderers are rejected.
- Generated Rust must be readable, formatted, reproducible where tool outputs
  permit, and traceable to input hashes and builder version.
- Interaction kind, port role, safety class, delivery semantics, and
  transport ownership remain separate typed concepts.
- Build rejection, boot initialization failure, and runtime faults are
  distinct and use explicit diagnostics or typed fault paths.
- A successful generation, compile, link, or hardware smoke test does not
  establish flight safety, electrical timing, or certification.

## Checked Implementation Baseline

The current tree implements an isolated Rust workspace whose only workspace
member is `xtask`. It keeps builder dependencies and STM32F401 HAL selection
out of the FerroWasp firmware workspace.

Checked source-of-truth boundaries are:

- `bsp/nucleo-f401re.toml`: physical NUCLEO-F401RE facts and stable resource
  IDs;
- `applications/`: strict blinky/button and USART1 DMA/MSP DisplayPort
  application manifests;
- `architecture-contracts/`: executable task, transport, timing, fault,
  mechanism, and safety-scope contracts for both applications;
- `feature-library/` and `templates/`: current STM32F4 feature fragments and
  central application templates;
- `xtask/`: parsing, validation, architecture checks, rendering, command
  execution, checkpointing, and CLI behavior;
- `generated/`: disposable generated checkpoints, never a hand-maintained
  source of truth.

Implemented validation applications demonstrate:

- static single ownership of GPIO, USART1, DMA streams, interrupts, and
  buffers;
- a blinky/button application with a shared 1 kHz SysTick monotonic and
  delayed debounce;
- an isolated USART1 RX/TX DMA plus MSP DisplayPort application;
- bounded work, TX, completion, and observation transports;
- short hardware tasks with deferred protocol processing;
- explicit RTIC task forms, priorities, overflow behavior, typed faults,
  boot-spawn failure handling, and versioned backend mechanisms;
- pre-mutation input/contract checks, incremental embedded checks, final
  release linking, retained failed candidates, successful checkpoint
  promotion, and fingerprint-gated resume;
- canonical reuse of `crates/ferrowasp-mspv1` through the narrow transitional
  F401 serial/OSD adapter.

The builder translates the selected `STM32F401` backend profile into target,
HAL/PAC, linker, clock, alternate-function, DMA-direction, and shared-timebase
details. It does not allocate undeclared hardware.

## Current Limitations

The working vertical slices are executable evidence, not the final general
component catalogue. Current gaps include:

- strict application contracts are not yet the general renderer-facing
  component/capability schema;
- the UART-DMA provider and MSP DisplayPort consumer remain bundled in one
  feature instead of independently instantiated components;
- endpoint data does not yet carry the full canonical FerroWasp completion,
  discontinuity/generation, timestamp, and UART-error metadata;
- generic TX remains insufficiently length-aware;
- overflow policy is not yet declared per capability edge;
- application-wide priority/ceiling and shared-lock graph validation is
  incomplete;
- the shared monotonic is implemented but not yet a general typed scheduling
  capability;
- GPIO-bank, dispatcher, init-local, and Cargo dependency special cases still
  need validated metadata;
- dependency closure still uses feature-specific lockfile templates;
- generated names and physical/task metadata are not yet proven for arbitrary
  repeated component instances;
- boot-frozen logical endpoint routing is designed but not implemented;
- the F401 serial/OSD adapter remains intentionally transitional;
- there is no generated, output-inhibited F405 parallel candidate, STM32H7
  backend, general semantic graph diff, or release provenance flow yet.

## Ordered Roadmap

### R1 — Generalize The Executable Composition Model

1. Lift the checked NUCLEO architecture contracts into a provisional
   renderer-facing resolved model without weakening their semantics.
2. Define typed/versioned component, endpoint, port, transport, fault, task,
   resource, backend-recipe, and safety-scope identities.
3. Keep schema examples executable fixtures rather than duplicated prose.
4. Add instance-safe deterministic names, exact physical claims, complete
   task/resource access, initialization order, and canonical hashing.
5. Validate the complete resolved graph before rendering or mutating output.

Exit: both current NUCLEO applications resolve through one explicit model and
retain their architecture-contract and compile/link behavior.

### R2 — Separate Endpoint And Consumer Components

1. Model USART1 RX/TX DMA ownership as a standalone endpoint component.
2. Model MSP DisplayPort as a divergent software consumer with directed
   bounded RX, TX, completion, and observation ports.
3. Add canonical RX metadata, length-aware TX, per-edge overflow/fault
   semantics, and multi-instance-safe endpoint types.
4. Replace renderer and dependency-template special cases with validated
   component requirements and deterministic dependency closure.
5. Preserve a byte-compared blinky fixture and add structural or byte-compared
   OSD output coverage.

Exit: endpoint and consumer compose independently without exposing HAL/PAC
ownership or weakening bounded transport semantics.

### R3 — Complete Builder-Core Evidence

Add deterministic resolved-application reports, resource/interrupt/task maps,
machine-readable diagnostics, input/build provenance, repeat-generation
checks, rollback/fault tests, and semantic graph comparison. Freeze a public
schema only after the NUCLEO renderer contract is proven.

Exit: the NUCLEO builder core is reviewable and reproducible independently of
the later flight-target work.

### R4 — Add Canonical Vertical Slices

Add SBUS and one SPI/DMA IMU through reusable FerroWasp boundaries, with
explicit endpoint ownership, bounded transports, faults, timing, and host plus
generated-application tests. Add boot-frozen logical endpoint routing only
after the static ownership graph and platform configuration boundary are
validated.

### R5 — Parallel F405 Candidate

Pin current monorepo sources and APIs. Add an explicit F405 board definition
and generate a parallel actuator-inhibited candidate. Reconcile resources,
task priorities, scheduling, protocols, logs, and safety behavior against the
current golden app. This milestone requires separate architecture review and
target-test planning; it does not authorize motor output or replacement of the
handwritten app.

### R6 — Mid- And Long-Term Expansion

After the builder core and F405 experiment are reconciled, consider schema
1.0, generalized capability/dependency graphs, observation snapshots,
STM32H7/Pixhawk-class backends, simulation/replay/HIL integration, memory and
resource reports, controlled release provenance, solver-assisted authoring,
independent output verification, qualification strategy, and standalone
extraction. These remain future work, not current commitments.

## Verification Baseline

Run from `tools/rtic-app-builder`:

```text
cargo fmt --all --check
cargo check -p xtask --locked
cargo test -p xtask --locked
cargo check --manifest-path compat/ferrowasp-serial-osd/Cargo.toml --tests --locked
```

For generator changes, regenerate each affected canonical application. The
generator must preserve its input/contract validation, incremental embedded
checks, final release link, failure capture, deterministic promotion, and
resume fingerprint gates.

Hardware commands (`cargo xtask flash` or `embed`) require explicit user
intent and suitable probe hardware. Host checks, generated compilation, and a
development-board smoke test remain distinct evidence tiers.

## Updating This Roadmap

- Keep this current file below its registered byte budget.
- Record live implementation only when supported by checked code/tests.
- Link to focused documents instead of copying their detail.
- Move superseded detail to `docs/archive/` with its date, size, and hash.
- Do not edit the archived 2026-07-24 baseline.
- Update `README.md` and focused design notes when implementation behavior
  changes; this roadmap owns sequencing, not duplicated API detail.
