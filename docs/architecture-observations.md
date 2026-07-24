# Architecture observations and terminology

This document records the architectural lessons from the working
NUCLEO-F401RE blinker/button and USART1 DMA MSP DisplayPort applications. The
application manifests remain the authoritative record of which examples are
implemented.

This is an evidence and terminology note, not an independent roadmap. The
canonical migration sequence and target schemas are in
`../RTIC_APP_BUILDER_REFERENCE_IMPLEMENTATION_PLAN.md`.

## Terminology

- **Application** — generated RTIC source application. It is the assembled
  source tree before compilation, for example `nucleo-f401re-osd`.
- **Binary** — compiled firmware image produced from an application and ready
  to flash.
- **Task** — an actual RTIC hardware or software task. Interrupt handlers,
  debounce handlers, and deferred OSD processing functions are tasks.
- **Component** — a reusable functional unit such as SBUS, IMU, OSD, battery
  monitoring, or a UART-DMA driver/provider.
- **Endpoint** — a concrete hardware-facing instance, such as USART1 on
  PA9/PA10 with DMA2 streams 5/7, USART6 RX DMA, or ADC1 channel 3.
- **Capability** — a typed semantic contract between components and
  endpoints. Interaction kind, safety class, and port role are independent;
  none transfers ownership of the underlying peripheral.

A UART can appear to be both a component and an endpoint, so the level of
description matters. The reusable UART RX/TX DMA implementation is a
component. Once instantiated with a peripheral, pins, DMA streams, buffers,
and interrupts, it is an endpoint. The current compatibility implementation
is honestly USART1-specific. It exposes separate work, TX, and completion
channel edges instead of a broad bidirectional shared object. A future generic
endpoint facade must make its concrete endpoint types recipe outputs before it
claims multi-UART multiplicity.

Use these terms consistently in manifests and generator diagnostics:

```text
UART-DMA component
    instantiates USART1/PA9/PA10/DMA2 endpoint
        publishes bounded RX chunks -> consumed by OSD
        handles bounded TX requests <- emitted by OSD
    connects to MSP parsing/rendering RTIC tasks
```

## What the prototype validated

The static endpoint plus software-consumer model fits RTIC well:

- USART, DMA streams, buffers, and interrupts have one static owner.
- Hardware interrupt tasks remain short and defer protocol work.
- OSD consumes a bounded serial capability and does not own HAL/PAC objects.
- Queue and buffer memory is statically bounded.
- A second component can update typed OSD state without gaining serial
  hardware ownership.
- Explicit manifest priorities and incremental compile gates expose resource,
  initialization, and scheduling mistakes early.
- BSP physical facts remain separate from application behavior and feature
  composition.

The demonstrated path is:

```text
USART1 RX/IDLE and DMA tasks
    -> bounded MPSC work channel
        -> divergent MSP/DisplayPort task
            -> bounded SPSC TX channel
                -> divergent TX worker
                    <- SPSC completion channel <- USART1 TX DMA task

B1 EXTI task -> monotonic debounce task -> OSD telemetry capability
```

## Main abstraction correction

The ownership boundary inside the prototype is correct, but the current
`msp-displayport-usart1-dma` bundle still packages one UART-DMA provider and
the OSD consumer together. The intended reusable composition is:

```text
uart_dma_endpoint component
    publishes bounded SerialRxChunk
    handles bounded SerialTxChunk requests

msp_displayport component
    consumes bounded SerialRxChunk
    emits bounded SerialTxChunk requests
    reads OsdTelemetry observation

button_arm_demo component
    writes only the display-demo observation in the current prototype
```

The generated UART endpoint and consumer must be separately modeled, but the
new resolver compiles complete, semantically valid graph checkpoints. It does
not require every arbitrary component prefix to compile, and it must not bundle
endpoint and consumer merely to preserve the legacy feature-prefix loop.

## Boot-time routing direction

FerroWasp requires a finite set of BSP-declared physical endpoints to be
compiled statically. At boot, validated immutable platform configuration maps
logical components such as OSD, SBUS, GPS, and telemetry onto compatible
endpoints. Changing the persisted mapping takes effect only after reboot.

```text
BSP-declared static endpoints
          |
validated ActivePlatformConfig
          |
frozen boot router
          |
logical component capabilities
```

The router changes which bounded capability handles are connected. It does
not move interrupts, DMA ownership, buffers, or HAL objects at runtime. Start
with one logical consumer per UART endpoint; introduce sharing only for an
explicitly supported protocol and timing model.

### Serial configuration ownership

UART baud, word length, stop bits, parity, inversion, and direction belong to
the active platform assignment, not the BSP or generated application
manifest. Platform configuration selects a named component/profile such as
`msp_displayport`, `sbus`, or `crsf`; the backend translates that name into a
validated HAL configuration for the selected MCU and endpoint.

The BSP declares only physical endpoint facts:

```toml
[resources.serial.usart1]
peripheral = "USART1"
tx_pin = "PA9"
rx_pin = "PA10"
```

DMA direction is structural as well. A UART RX DMA route is always
peripheral-to-memory and a UART TX DMA route is always memory-to-peripheral,
so the BSP selects controller/stream/channel without repeating `direction`.
The backend emits the correct HAL transfer type from the route's typed role.

Alternate function is also not user configuration. The MCU backend derives
AF7 from the validated USART1/PA9/PA10 mapping. An illustrative future
platform assignment is:

```toml
[serial.usart1]
component = "msp_displayport"
```

This assignment implies the component's required serial profile. The current
MSP DisplayPort implementation uses 115200 baud and 8-N-1. SBUS and CRSF will
have their own backend profiles when implemented. These names express the
intended model, not a finalized serialized platform-config schema.

Boot must load and validate platform configuration before enabling
reassignable UART DMA endpoints. Endpoint existence, pins, DMA routes, and
interrupt ownership remain compiled and static; framing and logical routing
are frozen from `ActivePlatformConfig` for the remainder of the boot.

The current Nucleo OSD prototype still applies the MSP DisplayPort profile
statically because the boot router is not implemented. That is a transitional
limitation, not the target manifest ownership model.

## Capability contracts

The NUCLEO architecture contracts now make the previously prose-only
concurrency rules executable. Future component metadata grows from
collision-only claims into the same typed concepts:

- interaction kind, independent safety classification, and explicit port role;
- type/ABI revision, logical cardinality, transport producer/consumer
  cardinality, delivery semantics, and whether the port is mandatory;
- exact SPSC, MPSC, latest-value, journal, same-task-direct, or service
  transport semantics;
- per-edge transport ownership where every consumer must receive a value;
- exclusive physical claims: pins, peripherals, DMA routes, and interrupts;
- scheduling requirements: task priorities, dispatchers, timer/monotonic use,
  and maximum critical-section expectations.

`required_symbols` is useful for insertion ordering but is not a sufficient
component contract. The generator should diagnose missing or incompatible
capabilities before rendering Rust, and generated names must be safe for more
than one instance of the same component.

## Prototype limitations to resolve

- Carry FerroWasp's RX metadata: completion reason, discontinuity/generation,
  timestamp, and UART error state.
- Make TX DMA transfers length-aware rather than treating a protocol-sized
  padded buffer as the generic serial contract.
- Declare queue overflow policy per capability. Control traffic may reject new
  data, while OSD commonly benefits from replacing the oldest stale frame.
- Add global priority/ceiling validation. Priorities remain explicit, but the
  builder should check the complete shared-resource lock graph.
- Generalize the implemented SysTick monotonic into an explicit typed
  scheduling capability when multiple backend timebases become necessary.
- Move renderer special cases for GPIO banks, dispatchers, init-local storage,
  and Cargo dependencies into validated component requirements.
- Replace feature-specific lockfile templates with deterministic dependency
  closure generation.
- Continue replacing compatibility code with canonical FerroWasp crates when
  the in-tree capability boundaries are behavior-compatible. MSP already uses
  `crates/ferrowasp-mspv1`; the remaining F401 serial/OSD adapter is tracked in
  `backend-unification.md`.
- Keep the button ARM example explicitly display-only. Real OSD telemetry must
  consume latest-value capabilities published by the safety, battery, RC, and
  IMU components; it must never own or emulate flight arming authority.

## Migration implications

Preserve the working OSD application as an integration and hardware regression
reference. The authoring checklist is recorded in `authoring/README.md`. The
canonical reference plan owns implementation ordering. The prototype implies
these migration requirements:

1. Keep the byte-compared blinky golden and add a byte-compared or structural
   OSD migration fixture.
2. Lift the checked NUCLEO transport/task contracts into the provisional
   renderer-facing IR without weakening their semantics.
3. Model USART1 RX/TX DMA ownership as a self-contained endpoint component.
4. Model MSP DisplayPort as a divergent software consumer with directed RX/TX
   ports.
5. Add instance-safe names, exact physical claims, and complete task/resource
   access metadata.
6. Port canonical FerroWasp chunk metadata, overflow behavior, and fault
   reporting before generalizing to more UART instances.
7. Treat boot-frozen routing as a later runtime projection from the resolved
   static graph, not as ownership transfer.

This preserves the successful static RTIC ownership model while creating the
generic endpoint layer needed for configurable multi-board applications.
