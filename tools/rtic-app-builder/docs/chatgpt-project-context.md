# RTIC App Builder — ChatGPT project context

This document is a self-contained architectural brief intended for upload to
a ChatGPT Project. It gives ChatGPT enough context to discuss and develop the
architecture without treating historical plans, prototypes, and long-term
goals as if they were all implemented today.

This is a derived summary, not a planning authority. When repository access is
available, use `../RTIC_APP_BUILDER_REFERENCE_IMPLEMENTATION_PLAN.md` for the
canonical roadmap and verify current behavior in code and tests.

## Project purpose

FerroWasp is an RTIC-based flight controller. Its aspirational long-term goal
is broad hardware support, including Betaflight and PX4 boards. Supporting
many boards with independently maintained RTIC applications would make task
synchronization and board maintenance prohibitively expensive.

The RTIC App Builder addresses that problem. Reusable hardware and software
tasks are assembled into generated RTIC source applications from strict BSP
and application manifests. A change to shared logic should then propagate to
every generated board application instead of being copied manually.

The builder is presently a prototype under `tools/rtic-app-builder` in the
FerroWasp monorepo. It is an isolated nested Cargo workspace and is not a
member of the firmware workspace. The monorepo root owns flight-product
priorities, safety decisions, and current board status; those should not be
inferred from builder documents.

## Relationship between the layers

```text
stm32f4xx-hal
    ↓
ferrowasp-stm32f4
    reusable STM32F4 setup for DShot, UART DMA, SPI DMA, ADC, etc.
    ↓
endpoint and functional components
    ↓
RTIC App Builder + board/application manifests
    ↓
generated RTIC source application
    ↓
compiled firmware binary
```

The builder should reuse canonical FerroWasp drivers, tasks, and protocol
crates. It should not become a parallel flight stack or independently rewrite
working FerroWasp implementations.

## Agreed terminology

- **Application** — a generated RTIC source application.
- **Binary** — the compiled firmware image.
- **Task** — an actual RTIC hardware or software task.
- **Component** — a reusable functional unit or implementation, such as SBUS,
  IMU, OSD, battery monitoring, or a UART-DMA driver.
- **Endpoint component** — a reusable hardware-provider implementation, such
  as an STM32F4 UART RX/TX DMA driver.
- **Endpoint instance** — an endpoint component bound to concrete hardware,
  such as USART1, its pins, DMA streams, buffers, and interrupts.
- **Capability** — a typed contract between components and endpoints. It does
  not transfer ownership of the underlying peripheral.

Example:

```text
UART-DMA endpoint component
    instantiated as USART1/PA9/PA10/DMA2 endpoint
        publishes bounded RX chunks -> consumed by MSP DisplayPort
        handles bounded TX requests <- emitted by MSP DisplayPort
        MSP reads OsdTelemetry observations published by explicit state owners
```

The compatibility prototype implements these directions with separate bounded
work, TX, and TX-completion channels. Telemetry is a copied latest-value
snapshot. Target metadata keeps interaction kind, safety classification,
direction, and transport topology explicit.

## Configuration ownership

### Board definition / current BSP manifest

The target `BoardDefinition`, represented by the current prototype's BSP
manifest, describes immutable physical board facts:

- MCU compatibility profile;
- pins and board wiring;
- peripheral instances;
- DMA controller, stream, channel, and interrupt routes;
- timer channels and other physical endpoint resources when applicable.

The BSP should not contain user-selectable protocol assignments or redundant
HAL setup details.

### Application profile / current application manifest

The target `ApplicationProfile`, represented narrowly by the current
application manifest, selects compile-time composition and bounded policy:

- components and endpoint instances included in the binary;
- task priorities;
- queue and buffer capacities;
- logical periods such as blink, debounce, and OSD refresh intervals;
- explicit links between selected resources where static composition requires
  them.

### Persisted platform configuration

Platform configuration is loaded and validated at boot. It assigns generic
compiled endpoints to logical profiles such as MSP DisplayPort, SBUS, CRSF,
GPS, or telemetry. It also owns settings such as motor ordering, motor
direction, and FCU orientation.

UART baud, framing, inversion, and logical protocol belong to this platform
assignment, not the BSP. A named profile implies its validated serial setup.
The active configuration is frozen for the boot: changes made while running
take effect only after reboot, and uncommitted changes are lost.

### MCU/backend contract

The versioned backend derives mechanical HAL details from validated MCU and
endpoint facts, including:

- Rust target, HAL/PAC features, and memory layout;
- GPIO alternate functions;
- GPIO mode, pull, and speed policies owned by an implementation;
- DMA direction implied by typed RX or TX roles;
- clock and interrupt setup;
- the shared monotonic implementation.

The backend must reject unsupported combinations. It must not silently choose
a replacement pin, peripheral, DMA route, priority, or behavior.

## Static ownership and boot-time routing

RTIC hardware ownership remains static. The generated application creates all
peripherals, DMA streams, buffers, and interrupt tasks at compile time. The
boot router connects already-compiled bounded capability handles according to
validated platform configuration; it does not move HAL objects or interrupt
ownership dynamically.

Start with one logical consumer per endpoint. Sharing should be introduced
only when a protocol has an explicit and validated multiplexing model.

Hardware interrupt tasks should remain short. Parsing, encoding, and other
substantial work should be deferred to software tasks through bounded
capabilities.

Artifact mapping:

- `BoardDefinition` is the physical authoring input;
- `ResolvedApplication` is the compile-time canonical graph;
- generated `BoardCapabilities` is the read-only runtime projection of
  compiled endpoints and supported roles;
- persisted `PlatformConfigV1` is a complete assignment candidate;
- `ActivePlatformConfig` is the validated immutable value selected for one
  boot.

## Scheduling model

The STM32F4 backend currently owns one 1 kHz Cortex-M SysTick monotonic. Blink,
button debounce, and OSD refresh derive restricted logical schedules from this
base time source. These ordinary software delays do not reserve TIM2, TIM3, or
TIM4.

Pin-backed timers for motors, PWM, or capture remain distinct physical BSP
resources. A control function that genuinely requires an exclusive hardware
timer may also claim one explicitly.

RTIC software dispatchers are still selected by a renderer special case. The
future allocator must derive the required count from distinct software-task
priorities and select conflict-free interrupts from the board definition's
ordered candidate allowlist after applying backend-reserved/forbidden
constraints.

## Working prototypes

### NUCLEO blinker

- LD2 on PA5;
- B1 on PC13 using EXTI15_10;
- configurable blink and 20 ms debounce periods;
- shared SysTick monotonic scheduling;
- the button enables or disables blinking.

### NUCLEO MSP DisplayPort OSD

- USART1 on PA9/PA10;
- RX DMA2 stream 5 and TX DMA2 stream 7;
- separate static DMA buffers and bounded RX/TX queues;
- hardware tasks own USART1, DMA, buffers, and interrupts;
- RX IDLE and RX-DMA tasks publish to one bounded MPSC work channel;
- divergent OSD and TX-worker tasks are spawned once and await channels;
- TX-DMA completion uses a separate capacity-one SPSC channel;
- the OSD component does not own USART1 and processes outside RTIC locks;
- periodic DisplayPort rendering through the shared monotonic;
- a debounced button toggles demonstration `ARMED`/`DISARMED` display state.

The displayed ARM state is not FerroWasp flight arming logic and cannot enable
motors. The prototype's static MSP serial profile is transitional until the
boot router applies persisted platform configuration.

## Architectural invariants

- Every HAL/PAC peripheral and interrupt has one clear static owner.
- Queues and buffers are statically bounded; payload buffering belongs to
  transports, not RTIC software-task spawn capacity.
- Destructive queues never imply fan-out; topology and delivery semantics are
  explicit.
- Overflow, malformed input, timeout, and hardware-error behavior must be
  explicit.
- Capabilities should not expose concrete HAL/PAC types to functional
  consumers.
- Physical facts, compile-time composition, boot-time configuration, and
  backend-derived mechanics must remain separate.
- Resource and priority conflicts should fail before code generation whenever
  possible.
- Generated names must eventually support multiple instances safely.
- The legacy prototype validates an empty shell and each feature prefix. The
  target pipeline validates complete, semantically valid resolved checkpoints
  and links applicable release/reference applications.
- Externally designated golden FerroWasp applications must not be modified or
  replaced without explicit authorization and fresh applicable validation.
- Experimental replacements should be parallel applications.
- Reuse existing FerroWasp work; do not reinvent working drivers, protocols,
  tasks, or flight logic.
- If an architectural choice is genuinely uncertain, present the trade-off
  and ask before committing to it.
- Broad Betaflight/PX4 board support is an accepted aspirational direction,
  not a reason to reject the architecture as too ambitious.

## Current limitations and planned work

The following are directions, not completed functionality:

- lifting the checked NUCLEO interaction/safety, directed-port, topology, task,
  fault, and backend-recipe contracts into the general component schema;
- splitting the combined UART-DMA/OSD feature bundle into an independently
  valid endpoint provider and software consumer;
- boot-time endpoint routing from persisted platform configuration;
- dynamic application-wide dispatcher allocation and priority/ceiling
  validation;
- instance-safe generation of multiple components of the same kind;
- deterministic dependency closure instead of feature-specific lockfiles;
- replacement of the remaining STM32F401 serial/OSD compatibility adapter
  with canonical FerroWasp crate boundaries; MSP parsing/responding already
  uses `crates/ferrowasp-mspv1` directly;
- canonical semantic composition identity, exact input identity, and separate
  build provenance;
- immutable semantic/input-keyed source artifacts, build-provenance records,
  failed-candidate retention, and reviewed committed reference outputs;
- possible standalone extraction only after stable interfaces and demonstrated
  external demand.

The preferred FerroWasp integration is gradual: preserve the golden
applications designated by the pinned monorepo commit, generate parallel
replacements, reuse existing modules, compare resource and interrupt
ownership, bench-test, and promote only after explicit applicable validation.

## Authoring guidance

When adding SBUS, CRSF, or another protocol, follow the capability, endpoint,
component, composition, and testing guides established by the canonical plan.
Every new unit should state ownership, interaction and safety classes, port roles, RTIC
tasks, physical claims, memory bounds, error/overflow behavior, configuration
ownership, multi-instance behavior, and its automated/hardware tests.

Use the USART1 DMA plus MSP DisplayPort path as the canonical executable
example. Do not present proposed manifest syntax as implemented. API-level
facts should live near code in Rustdoc, and canonical guide examples should be
generated and compiled in CI.

## How to reason about proposed changes

Always label a proposal as one of:

- **implemented** — present and validated in the current repository;
- **transitional** — working but intentionally located on the path to another
  boundary;
- **planned** — agreed direction without complete implementation;
- **aspirational** — long-term goal that should guide but not falsely constrain
  the current MVP.

Prefer the smallest change that advances the target architecture while
preserving working hardware behavior. State assumptions, identify ownership
changes, and call out effects on flight safety, resource conflicts, bounded
memory, and test coverage.

## Source precedence

When documents disagree, use this order:

1. The user's latest explicit architectural decision.
2. Accepted ADRs and protected safety decisions.
3. Checked-in code, strict schemas, tests, and recorded target evidence.
4. `../RTIC_APP_BUILDER_REFERENCE_IMPLEMENTATION_PLAN.md`.
5. `architecture-observations.md`, `stm32f4-backend.md`,
   `betaflight-target-definition-notes.md`, and the authoring handbook for
   their focused topics.
6. The [archived original MVP implementation plan](archive/rtic_feature_assembler_mvp_implementation_plan.md),
   which is a superseded historical reference.

Do not copy current FerroWasp board status into builder documentation. Confirm
it from the monorepo root at a pinned commit when a task depends on current
flight-project state.
