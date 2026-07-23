# Component, endpoint, and capability authoring

This directory is the planned authoring handbook for extending the app
builder. It defines the documentation structure and the common review
checklist before additional protocols such as SBUS or CRSF are added.

The working USART1 DMA plus MSP DisplayPort application is the canonical
vertical example. New guides and abstractions should be verified against that
implementation rather than built around hypothetical code.

## Terminology at the implementation boundary

- **Component** — a reusable implementation or functional unit, such as OSD,
  SBUS, battery monitoring, or a UART-DMA driver.
- **Endpoint component** — a reusable hardware-provider implementation, such
  as the STM32F4 UART RX/TX DMA driver.
- **Endpoint instance** — an endpoint component bound to concrete hardware,
  such as USART1, PA9/PA10, DMA2 streams 5/7, buffers, and interrupts.
- **Capability** — the typed contract an endpoint or component provides to
  another component. A capability does not transfer ownership of the backing
  peripheral.

The existing OSD example demonstrates the intended chain:

```text
UART-DMA endpoint component
    instantiated as USART1/PA9/PA10/DMA2 endpoint
        provides SerialRxTx capability
            consumed by MSP DisplayPort component
                provides OsdTelemetry capability
                    consumed by button ARM-demo component
```

## Planned handbook

The handbook should be developed in dependency order:

1. `capabilities.md` — define typed contracts, ownership, boundedness, error
   and overflow semantics, timing, configuration, multiplicity, and
   versioning. Capability APIs must not expose HAL or PAC types.
2. `endpoints.md` — define peripheral, pin, DMA, interrupt, buffer, and
   initialization ownership. The USART1 DMA provider is the first worked
   example.
3. `components.md` — define software state and RTIC task ownership, required
   and provided capabilities, instance-safe naming, and hardware-independent
   logic. MSP DisplayPort is the first worked example.
4. `composition.md` — describe capability matching, insertion ordering,
   exclusive resource claims, priorities, shared logical scheduling, and the
   BSP/application/platform/backend configuration boundaries.
5. `testing.md` — define host tests, manifest validation, rendered-Rust
   parsing, incremental embedded checks, release linking, overflow tests, and
   hardware smoke tests.

## Common authoring checklist

Every component, endpoint, and capability guide must answer:

- What does this unit own?
- What capabilities does it provide and require?
- Which RTIC hardware and software tasks does it introduce?
- Which pins, peripherals, DMA routes, interrupts, and dispatcher capacity
  does it claim?
- What memory is statically allocated, and how are its capacities selected?
- What happens on queue overflow, malformed input, timeout, or hardware error?
- Which configuration belongs to the BSP, application, persisted platform
  configuration, or backend?
- What initialization and shutdown/safe-state behavior is required?
- Can multiple instances be generated without symbol or resource collisions?
- Which automated and hardware tests establish that it works?

## Configuration ownership reminder

- The **BSP** declares immutable physical facts: pins, peripheral instances,
  DMA routes, and other board wiring.
- The **application** selects components and owns compile-time behavior such
  as queue capacities, task priorities, and logical periods.
- The persisted **platform configuration** assigns configurable peripherals
  to profiles such as `msp_displayport`, `sbus`, or `crsf` at boot. The
  assignment is frozen until reboot.
- The **backend** translates validated MCU/profile facts into HAL types and
  setup, including alternate functions, DMA direction, electrical policy, and
  the shared monotonic implementation.

## Current implementation versus planned contracts

Today, feature metadata records symbols, required symbols, resources,
interrupts, and insertion ordering. `SerialRxTx` demonstrates a real typed
Rust boundary, but the generator does not yet express general typed
`provides` and `requires` capability metadata.

The planned guides must distinguish executable behavior from proposed schema.
Until typed metadata is implemented, examples should identify capability
relationships in prose and Rust types without presenting speculative manifest
syntax as supported configuration.

## Keeping the handbook current

Authoritative API details should live in Rustdoc beside capability types. The
handbook explains ownership, design choices, and the end-to-end workflow.
Manifest and generated-code snippets should come from checked examples where
possible.

CI should generate and compile every canonical handbook example. A feature is
not considered documented when its example no longer passes manifest
validation, embedded `cargo check --locked`, and release linking.

The capability guide should be completed first, followed by the USART1 DMA
endpoint and MSP DisplayPort component guides. Those three documents establish
the pattern that SBUS and CRSF can then test and refine.
