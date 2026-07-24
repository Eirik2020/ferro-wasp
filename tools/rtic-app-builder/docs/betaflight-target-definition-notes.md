# Betaflight-inspired target and platform configuration model

Last reviewed: 2026-07-24

Status: Focused accepted direction for board capabilities and boot-frozen
platform configuration. This is not a description of functionality that exists
today and does not own implementation sequencing. The canonical builder roadmap
is `../RTIC_APP_BUILDER_REFERENCE_IMPLEMENTATION_PLAN.md`.

## Purpose and project boundary

Live FerroWasp target status, bench evidence, and current priorities remain
owned by the `ferro-wasp` repository. This document records only the durable
builder/configuration boundaries so it does not become a second, stale support
matrix. Any implementation task that depends on current external status must
pin and inspect the relevant FerroWasp/FerroConfigurator repository commit.

## Sources and implementations reviewed

The design is inspired by the separation used in Betaflight:

- [Betaflight target configuration repository](https://github.com/betaflight/config)
- [Betaflight target configuration guide](https://betaflight.com/docs/development/manufacturer/creating-configuration)
- [Betaflight serial configuration](https://betaflight.com/docs/wiki/guides/current/Serial)
- [Betaflight serial configuration types](https://github.com/betaflight/betaflight/blob/master/src/main/io/serial.h)
- [Betaflight serial initialization and claiming](https://github.com/betaflight/betaflight/blob/master/src/main/io/serial.c)
- [Betaflight generic UART driver](https://github.com/betaflight/betaflight/blob/master/src/main/drivers/serial_uart.c)

The assessment also covered implementations observed during the architecture
review in:

- this repository's manifest, backend, feature fragments, validation, and RTIC
  application template;
- `ferro-wasp`, particularly its BSPs, generic I/O crates, Foxeer RTIC app,
  configuration storage, MSPv2 RPC, actuator mapping, and orientation types;
- `ferro-configurator`, particularly its configuration model, staging,
  persistence, readback, and future RPC reference design.

The implementation observations below are a dated 2026-07-23 snapshot, not an
API contract. The exact external FerroWasp/FerroConfigurator revisions were
not recorded; all such facts must be re-verified against pinned commits before
use. The architecture sections are the intended long-lived part of this
document.

## Architectural decision

Use distinct authoring, compile-time, runtime-platform, and tuning artifacts:

| Artifact/layer | Owns | Must not own |
| --- | --- | --- |
| `BoardDefinition` (current prototype: BSP manifest) | MCU, pins, physical wiring, valid peripheral/DMA/interrupt routes, electrical constraints, and explicit endpoint slots | Component selection, task topology, per-aircraft assignments, or tuning |
| `ApplicationProfile` | Selected component instances, capacities, scheduling policy, and explicit capability-port connections | New physical routes, runtime assignment, or behavioral implementation |
| `ResolvedApplication` / generated board application | Exact buffers, static RTIC tasks, ownership, initialization, dispatchers, Cargo plan, and generated `BoardCapabilities` projection | User tuning or runtime ownership transfer |
| Platform configuration | Assignment of compiled endpoints to functions, motor layout and direction metadata, and board-to-vehicle orientation | Pin mux, DMA/IRQ selection, task topology, or safety authority |
| Tuning configuration | PID gains, filters, rates, logging options, and other explicitly live/disarmed settings | Peripheral topology or actuator ownership |

The `BoardDefinition` describes physical possibilities. The
`ApplicationProfile` selects a compile-time composition. The resulting
`ResolvedApplication` records exactly what the firmware contains, and its
generated `BoardCapabilities` projection describes the finite runtime choices.
Platform configuration selects among those compiled capabilities at boot. It
cannot create a new peripheral route, change an RTIC interrupt binding, or
select code that was not compiled into the board application.

Builder inputs, resolved composition, and persisted platform configuration are
therefore different artifacts:

- the builder consumes board facts and an application profile to generate one
  deterministic resolved graph and RTIC application;
- the FCU stores a platform configuration for one physical installation;
- FerroConfigurator edits that stored configuration without generating another
  firmware binary.

## MCU target naming

Use Betaflight-style family identifiers in BSP manifests, for example:

```toml
[bsp]
id = "nucleo-f401re"
mcu = "STM32F401"
```

The identifier selects a versioned builder compatibility profile. That profile
owns the Rust compilation target, HAL crate and feature, PAC path, and linker
memory layout, so BSPs do not repeat those coupled implementation details.

A family identifier does not imply support for every package or density sold
under that STM32 family. It means the board conforms to the builder's documented
profile. If future boards expose a useful distinction that cannot be expressed
safely by a family profile, the registry may add a more specific identifier or
an explicit variant. Do not add exact ordering codes preemptively.

## Betaflight model and RTIC adaptation

Betaflight target files use compact physical declarations such as `PA5`, UART
pins, timer mappings, and DMA options. Runtime serial configuration separately
assigns functions such as GPS, receiver, MSP, or telemetry to compiled ports.
At boot a feature finds the port assigned to its function and claims it.

FerroWasp should preserve that separation, but RTIC requires a different
dispatch mechanism:

```text
Generated BSP capability
  fixed UART peripheral, pins, DMA streams and IRQ tasks
                         |
                         v
  bounded per-endpoint RX/TX queues or service handles
                         |
                         v
Frozen boot-time assignment table
                         |
                         v
  OSD, receiver, GPS, telemetry, or another feature task
```

Interrupt handlers, DMA owners, priorities, buffers, and queue capacities remain
static. Boot configuration changes which software consumer receives an
endpoint's data, which endpoint accepts a component's output, and which named
serial profile configures that endpoint.

Unused endpoints should remain in a safe dormant state and should not start DMA
until claimed.

UART framing and electrical mode are platform configuration. A boot assignment
such as `msp_displayport`, `sbus`, or `crsf` selects the backend-defined baud,
word length, stop bits, parity, direction, and inversion policy. The BSP lists
only the physical UART, usable pins, and DMA routes. The MCU backend derives
pin alternate functions and rejects a profile that the endpoint cannot
support. The generated application compiles the available endpoint and
component implementations but does not own the active UART assignment.

DMA route direction is implied by the typed UART role: RX means
peripheral-to-memory and TX means memory-to-peripheral. It is not a BSP field.

## Board capabilities and feature composition

The builder should generate a read-only `BoardCapabilities` description with a
stable target identifier and capability/ABI revision. Each endpoint has a
stable ID and its complete physical contract.

Component ports distinguish ownership, semantics, and direction:

- a hardware endpoint component exclusively owns its HAL object, IRQs, DMA
  routes, buffers, and queues;
- for UART, it **publishes** bounded RX chunks and **handles** bounded TX
  requests;
- a functional component **consumes** compatible RX chunks or **emits** TX
  requests without claiming the underlying hardware;
- the generated application checks class, role, type/version, cardinality, and
  capacity compatibility before generating static task and resource instances;
- the boot router connects the instances selected by platform configuration.

For example, OSD owns parsing and encoding software tasks, consumes serial RX
chunks, and emits serial TX requests. It does not instantiate or own a
particular UART, UART RX DMA task, UART TX DMA task, or UART peripheral task.

Central board initialization must split clocks, GPIO banks, DMA controllers,
and PAC peripherals once. Feature bundles must not independently freeze clocks
or split the same peripheral containers.

## Timer resource model

Betaflight's `TIMER_PIN_MAP` describes pin-backed timer channels: physical pin,
one valid MCU timer mapping, and a DMA option. The MCU target code derives the
timer peripheral, channel, alternate function, and DMA route. That remains the
right inspiration for motors, PWM, capture, and other timer I/O.

The builder treats ordinary software timing differently. The STM32F4 backend
owns one verbose base timebase internally: Cortex-M SysTick, 1 kHz, monotonic
mode. Blink, button debounce, and OSD refresh derive restricted logical
schedules from it. Their manifests expose only behavioral periods and task
priorities; they cannot select the counter, clock, event, or interrupt.
Consequently the NUCLEO BSP no longer reserves TIM2, TIM3, or TIM4 for those
jobs.

```text
SysTick physical endpoint
    -> monotonic scheduling capability
        -> blink periodic schedule
        -> button one-shot debounce deadline
        -> OSD periodic refresh schedule
```

This does not collapse pin-backed timers into the software scheduler. A future
Betaflight-style timer-channel declaration should remain an explicit BSP fact,
using a compact pin plus mapping/DMA selection validated by the MCU profile. A
control loop that genuinely needs an exclusive hardware timer may also declare
one, but convenience delays should not consume scarce TIM peripherals.

## Platform configuration lifecycle

Platform configuration is a complete, versioned object. Peripheral assignments
and vehicle geometry must not be staged or committed one key at a time because
their validity depends on cross-field and resource constraints.

The lifecycle is:

| State | Meaning |
| --- | --- |
| Active | Immutable configuration selected and validated for the current boot |
| Staged | Complete candidate held in RAM; lost if it is not committed |
| Persisted pending | Atomically stored candidate that survives reset but does not affect the current run |
| Last known good | Previously activated configuration available for boot recovery |

Committing a platform configuration must report `reboot_required = true`. It
must not change any active peripheral routing, motor mapping, direction, or
orientation. Reboot loads and validates the pending object before activation.

This terminology is intentional: a committed change is not lost if the FCU is
left running without a reboot; it remains pending. An uncommitted staged change
may be lost. Existing tuning settings may retain their separate, explicitly
disarmed live-application behavior.

## Boot sequence and recovery

The required two-phase boot is:

1. Initialize minimal clocks, safety state, configuration storage, and a fixed
   recovery/configuration interface.
2. Read the newest complete platform configuration from atomic storage.
3. Validate its checksum, schema, target ID, capability/ABI revision, resource
   assignments, feature dependencies, and vehicle geometry.
4. Select the pending configuration, last-known-good configuration, a safe
   board default, or a motor-inhibited recovery state according to explicit
   policy.
5. Freeze an `ActivePlatformConfig` for the entire run.
6. Initialize or enable the BSP-declared endpoints and connect their bounded
   queues or service handles to selected feature tasks.
7. Start normal software tasks.
8. Permit arming only after platform, sensor, and existing safety checks pass.

Configuration storage and at least one recovery/configuration transport are
bootstrap resources. They cannot themselves be reassignable through the
configuration required to initialize them. For example, an SPI flash device
that stores `PlatformConfig` must have a fixed BSP-owned SPI/CS route, or the
configuration must use another fixed storage backend.

All supported boards need a platform-config storage provider. Its physical
backend may differ by BSP, such as internal flash or fixed external flash,
without changing the platform-config service contract.

## Peripheral assignment models

The Betaflight port-function model maps directly to UART, but the same exclusive
claim abstraction is not sufficient for every bus:

| Peripheral | Platform assignment unit | Important constraint |
| --- | --- | --- |
| UART | Physical port endpoint to one function initially | Validate RX/TX, inversion, DMA, baud/framing, and supported protocol capabilities |
| SPI | Bus plus chip-select endpoint to a device/driver | Multiple devices may share a bus; the bus needs one owner/arbiter and per-device mode/frequency policy |
| I2C | Bus plus address/device endpoint to a driver | The bus is shared; validate address conflicts and bus capabilities |
| ADC | Compiled physical channel/sample slot to a semantic measurement | Preserve board electrical facts and validate scaling/calibration requirements |

UART sharing should not be generalized from a bitmask. Start with one function
per port. Add only explicitly supported sharing modes with clear ownership,
direction, and timing rules.

Only endpoints declared by the BSP should be generated. The goal is not to
initialize every peripheral present on the MCU, but every usable endpoint the
board application promises to support.

## Vehicle installation configuration

### Motor order

Represent motor order with one typed, validated permutation. Reject duplicate,
missing, or out-of-range physical outputs. The immutable active mapping is
consumed inside the safety/actuator-output boundary; configurator and feature
tasks never gain direct motor authority.

### Motor direction

The unpinned 2026-07-23 FerroWasp snapshot did not show a complete
motor-direction representation; re-verify that status before implementation.
For this target model, initially treat direction as validated
installation/mixer metadata. Do not imply that changing this value programs an
ESC. Reversing an ESC through DShot or another maintenance protocol would be a
separate, disarmed operation with its own validation and confirmation flow.

### FCU orientation

Keep sensor-to-board rotation in the BSP because it is a board assembly fact.
Store board-to-vehicle orientation in platform configuration because it changes
with installation. Keep any controller-coordinate compatibility transform in
core control logic.

Persist an enumerated set of valid proper rotations, or validate a more general
representation rigorously. Do not accept duplicate axes, reflections, or
arbitrary unvalidated sign maps as an FCU orientation.

Changes to motor order, motor direction, or FCU orientation should invalidate
the relevant commissioning/verification state and keep arming inhibited until
the configured policy has been satisfied.

## Validation ownership

FerroConfigurator should prevalidate files for quick feedback, but firmware is
authoritative because only the generated application knows its exact compiled
capabilities.

Firmware validation should include at least:

- schema and target/capability compatibility;
- endpoint existence and supported protocol/electrical modes;
- duplicate and conflicting assignments;
- DMA, IRQ, buffer, queue, timer, chip-select, and ADC-channel constraints;
- required feature dependencies;
- preservation of a recovery/configuration path;
- motor permutation and direction validity;
- FCU orientation validity;
- any commissioning state invalidated by the change.

Validation failure must never partially activate a configuration. It must leave
motors inhibited and expose a clear diagnostic through the fixed recovery
interface.

## Current implementation fit

### RTIC app builder

The MVP already has strict unknown-field rejection, deterministic rendering,
incremental compile gates, feature fragment contracts, and fail-closed symbol,
resource, pin, peripheral, and interrupt collision checks. These are valuable
and should be retained.

It currently has two generated NUCLEO-F401RE applications:

- `nucleo-f401re-blinky` composes LED blinking and a debounced button toggle;
- `nucleo-f401re-osd` owns USART1 PA9/PA10, separate DMA2 RX/TX streams,
  bounded queues and buffers, and MSP DisplayPort processing;
- a shared-monotonic task periodically sends the heartbeat/text/draw sequence, and the separate
  debounced B1 component toggles a display-only ARM state;
- the BSP declares physical serial and DMA routes while the application
  declares buffer sizes, queue capacities, priorities, and behavior;
- incremental checks compile the empty application and every feature prefix;
- compatibility copies let the builder reuse FerroWasp protocol and ownership
  work without changing the external source repository.

The prototype also exposed the next required abstractions:

- the UART-DMA provider and MSP DisplayPort consumer are internally separated
  but still packaged in one feature bundle;
- exclusive resource claims and `required_symbols` do not yet express typed
  interaction/safety classifications and directed port roles;
- the serial queues do not yet carry canonical FerroWasp discontinuity,
  completion, timestamp, generation, and UART-error metadata;
- TX is protocol-buffer-sized rather than a generic length-aware DMA service;
- GPIO/init/dependency selection still contains feature-name special cases in
  the renderer;
- boot-time logical component-to-endpoint routing is documented but not yet
  generated.

These are expected MVP limitations. The separate input manifests establish the
board/application boundary for the example. Typed directed ports and bounded
adapters should be added before extracting many FerroWasp tasks, otherwise
feature-owned endpoint construction will be costly to unwind. The terminology,
prototype evidence, and migration implications are maintained in
`architecture-observations.md`.

### FerroWasp snapshot requiring re-verification

The unpinned external FerroWasp tree observed on 2026-07-23 contained much of
the required foundation:

- thin board apps and explicit BSP resource/DMA declarations;
- generic, bounded UART and SPI ownership primitives;
- serial profiles and an initial UART consumer-routing scaffold;
- separate sensor-to-board and board-to-vehicle orientation fields;
- two-slot, sequenced, checksum-protected configuration persistence on Foxeer;
- whole-object MSPv2 stage/commit messages with CRCs and a
  `reboot_required` result field.

The applications observed in that snapshot constructed feature-specific UART
endpoints and bound USART2 to receiver input and UART4 to OSD at
initialization. Persisted configuration then contained tuning/logging values,
was loaded after normal peripheral initialization, and was applied while
disarmed. SPI ownership embedded one chip-select, ADC used a fixed scan, and
I2C was not implemented. If re-verification confirms those constraints,
platform configuration requires a deliberate two-phase boot refactor rather
than a schema-only change.

In that snapshot, persistent configuration was not available through one
common provider on all boards. Foxeer's external-flash implementation was
useful prior art, while the application then described as golden FCU3 did not
expose the same storage service. Re-verify both status and designation.

### FerroConfigurator snapshot requiring re-verification

The unpinned configurator snapshot observed on 2026-07-23 contained only the
then-current tuning and logging fields. Its ASCII workflow staged values
individually, saved them, and immediately read them back. That behavior was
suitable for the observed tuning contract but not for platform topology.

The configurator reference design and FerroWasp's in-progress MSPv2 work already
contain the useful direction: device/board identity, capability reporting,
whole-object staging, expected staged CRC, atomic commit, active/persisted CRCs,
and a reboot-required result. Platform configuration should build on that
contract as a separate versioned type and add reconnect/post-boot activation
verification.

## Integration dependencies

The reference implementation plan owns ordering. Work packages that implement
this focused model must preserve these dependencies:

- generate `BoardCapabilities` as a read-only projection of a valid
  `ResolvedApplication`, not as another board-authoring input;
- define `PlatformConfigV1`, `ActivePlatformConfig`, stable endpoint IDs, and
  their compatibility/versioning rules before enabling runtime assignment;
- establish typed interaction/safety classifications and directed port roles before routing
  functional consumers;
- validate configuration, resource conflicts, recovery, and vehicle geometry
  as complete objects;
- implement atomic persistence and the two-phase boot/arming gate in
  FerroWasp before claiming boot-frozen routing;
- make FerroConfigurator verify commit, reboot/reconnect, and the activated
  configuration;
- address shared SPI/I2C bus arbitration with bus-owner models rather than
  copying UART exclusivity.

## Open design decisions

The direction above is agreed, but these details should be decided before their
implementation:

- exact serialized schema and migration policy for `PlatformConfigV1`;
- target ID versus generated capability-hash format;
- internal-flash and external-flash provider policy per board;
- exact last-known-good promotion and rollback rules;
- how commissioning evidence is cleared and re-established after vehicle
  geometry changes;
- which, if any, serial sharing modes are worth supporting;
- the SPI bus arbitration and per-device descriptor model.

This file is the focused note for the Betaflight-inspired target and platform
configuration direction. The reference implementation plan links here and owns
cross-topic sequencing and milestones.
