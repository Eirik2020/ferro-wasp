# RTIC App Builder Reference Implementation Plan

## Near-Term, Mid-Term, and Long-Term Implementation Baseline

**Project:** FerroWasp / FerroPilot  
**Primary subsystem:** RTIC App Builder  
**Intended reader:** Codex and human maintainers  
**Document type:** Reference implementation plan  
**Baseline date:** 24 July 2026  
**Classification:** Internal project planning source; review before public publication  
**Status:** Proposed implementation baseline derived from the current FerroWasp/FerroPilot master source  
**Primary implementation horizon:** Immediate work through approximately 24 months, followed by long-term product maturation

> This plan is an implementation specification, not evidence that any described feature is already implemented, flight-ready, certified, or assurance-approved. Current repository code, tests, accepted architecture decisions, and target evidence remain authoritative.

---

## 1. Executive implementation decision

The RTIC App Builder shall be implemented as a **deterministic static application-composition compiler**.

It shall consume:

1. a physical `BoardDefinition`;
2. an `ApplicationProfile`;
3. a versioned component catalogue;
4. a selected platform backend;
5. an explicit toolchain/build policy;

and produce:

1. a versioned `ResolvedApplication`;
2. ordinary, readable RTIC Rust source;
3. machine-readable architecture reports;
4. a checked and optionally built firmware application;
5. deterministic provenance suitable for comparison and later assurance work.

The builder shall not contain flight algorithms, driver state machines, protocol parsers, control logic, or safety policy implementations. Those remain normal Rust crates and reusable components. The builder only selects, validates, names, connects, and renders them.

The implementation shall follow one protected migration path:

```text
working handwritten prototypes
    -> clean endpoint/consumer vertical slice
    -> deterministic ResolvedApplication
    -> generated validation applications
    -> generated parallel F405 flight application
    -> reconciled generated release path
    -> multiple backends and targets
```

The existing flight-tested FCU3 application remains the golden runtime reference until a generated application has been statically, electrically, bench, and flight reconciled. Codex must not replace or broadly refactor the golden application as an incidental part of builder work.

---

## 2. Non-negotiable implementation constraints

### 2.1 Safety and authority

The following rule is architectural, not optional:

> Outer layers may request actuation. Only the safety-owned actuator-output path may command motor hardware.

The builder must therefore reject any resolved graph where:

- more than one component owns motor-output peripherals;
- more than one component provides physical actuation authority;
- a non-approved component receives an `Authority<T>` capability;
- an observer can provide, mutate, or directly invoke actuation;
- experimental components claim motor peripherals, safety authority, or critical hardware interrupts;
- a configuration or maintenance component can bypass the safety-owned actuator path.

### 2.2 Static ownership

Every resolved peripheral, pin, DMA stream/channel/request, interrupt, timer channel, static buffer, and hardware state machine shall have one explicit owner.

Runtime configuration may select among precompiled logical roles. It may not transfer:

- HAL/PAC objects;
- DMA ownership;
- interrupt ownership;
- timer ownership;
- static buffers;
- motor-output ownership.

### 2.3 Bounded behavior

All generated critical-path storage shall be statically bounded. Every queue, buffer, snapshot, or journal requires:

- exact capacity;
- sizing rationale or source;
- overflow behavior;
- error/fault propagation;
- test coverage.

The builder shall not silently invent capacities.

### 2.4 Readable generated source

Generated Rust is a reviewable product and shall:

- be formatted with `rustfmt`;
- contain stable generated identifiers;
- avoid opaque macro-generated component composition beyond RTIC’s required macros;
- identify its input hashes and builder version;
- contain no hand-maintained behavioral code;
- be reproducible byte-for-byte after normalization where tool outputs permit;
- be committed for reference applications and controlled releases.

### 2.5 No hidden fallback

The builder shall never silently replace:

- a pin;
- a DMA route;
- an interrupt;
- a timer;
- a driver;
- a component provider;
- a priority;
- a dispatcher;
- a capacity.

Ambiguity or conflict is an error. Future solver-assisted authoring may propose alternatives, but the committed verbose resolution remains explicit.

### 2.6 One central renderer

The builder shall use one architecture-aware RTIC renderer. Permanently reject designs such as:

```text
render_sbus(...)
render_mpu6500(...)
render_osd(...)
render_dshot(...)
```

Components contribute typed metadata and references to existing Rust implementation items. They do not render arbitrary source.

---

## 3. Scope of the reference implementation

### 3.1 Near-term supported component subset

The first useful reference implementation shall support:

- one STM32F4 board family;
- NUCLEO-F401RE validation targets;
- Foxeer F405 V2 or equivalent explicit F405 target;
- one shared monotonic;
- hardware tasks;
- software tasks;
- local resources;
- shared resources;
- static buffers;
- UART RX/TX DMA endpoint ownership;
- bounded RX/TX queues;
- MSP DisplayPort consumer;
- periodic OSD/heartbeat work;
- SBUS decoder;
- one SPI/DMA IMU endpoint and service;
- exact scheduling classes resolved to numeric priorities;
- deterministic imports, resources, task names, initialization order, and wiring;
- `cargo check` after generation;
- architecture reports.

The first vertical slice is deliberately UART-DMA endpoint plus MSP consumer because a prototype already exists and it exercises hardware ownership, capabilities, queues, periodic work, and compilation without motor authority.

### 3.2 Mid-term supported subset

The mid-term implementation shall add:

- complete typed capability classes;
- deterministic dependency closure;
- repeated component instances;
- application-wide priority and dispatcher allocation;
- boot-frozen endpoint routing;
- observation snapshots;
- component health metadata;
- generated parallel F405 flight application;
- STM32H7 backend and board targets;
- semantic graph diff;
- resource, memory, task, interrupt, capability, and provenance reports;
- integration with host simulation, replay, and hardware-in-the-loop testing;
- generated release composition for selected applications.

### 3.3 Long-term scope

Long-term work may add:

- solver-assisted compact board definitions;
- multiple HAL implementations below one platform contract;
- memory-domain and DMAMUX solving for STM32H7;
- reusable builder extraction into a standalone project;
- controlled FerroPilot assurance overlays;
- qualified or independently verified builder subsets where economically justified;
- additional deterministic-control application classes if serious external FOSS use appears.

### 3.4 Explicit non-goals

The reference implementation shall not:

- generate flight algorithms;
- create a manifest programming language;
- permit arbitrary Rust snippets in manifests;
- replace Cargo, rustc, RTIC, or the HAL;
- dynamically schedule runtime components;
- dynamically remap hardware ownership;
- automatically infer safety policy;
- qualify RTIC, rustc, a HAL, or generated firmware;
- prove schedulability;
- prove hardware timing;
- replace target tests;
- replace the current flight application before reconciliation;
- support every STM32F4 peripheral in the first release;
- solve general pin/DMA/timer allocation in the near term.

---

## 4. Required repository shape

The exact existing paths may differ. Codex shall map these logical responsibilities onto the current repository rather than performing an unnecessary top-level restructure.

```text
ferrowasp/
├── Cargo.toml
├── crates/
│   ├── ferrowasp-core/
│   ├── ferrowasp-actuator/
│   ├── ferrowasp-io-core/
│   ├── ferrowasp-drivers/
│   ├── ferrowasp-stm32f4/
│   ├── ferrowasp-stm32h7/
│   ├── ferrowasp-bsp/
│   ├── ferrowasp-tasks/
│   ├── ferrowasp-protocol/
│   └── ferrowasp-sim/
├── tools/
│   └── rtic-app-builder/
│       ├── Cargo.toml
│       ├── crates/
│       │   ├── rtic-app-model/
│       │   ├── rtic-app-schema/
│       │   ├── rtic-app-catalogue/
│       │   ├── rtic-app-resolver/
│       │   ├── rtic-app-validator/
│       │   ├── rtic-app-render/
│       │   ├── rtic-app-report/
│       │   └── rtic-app-cli/
│       ├── schemas/
│       ├── examples/
│       └── tests/
├── boards/
├── application_profiles/
├── component_catalogue/
├── apps/
│   ├── handwritten/
│   ├── generated-validation/
│   └── generated-targets/
├── generated/
│   ├── work/
│   ├── committed/
│   └── failed/
├── tests/
│   ├── host/
│   ├── target/
│   ├── hil/
│   └── evidence/
└── project_docs/
```

### 4.1 Crate dependency direction

```text
rtic-app-model
    no dependency on schema, rendering, Cargo, HAL, or FerroWasp implementation crates

rtic-app-schema
    -> rtic-app-model

rtic-app-catalogue
    -> rtic-app-model

rtic-app-resolver
    -> model + catalogue

rtic-app-validator
    -> model

rtic-app-render
    -> model

rtic-app-report
    -> model

rtic-app-cli
    -> schema + catalogue + resolver + validator + render + report
```

Rules:

- `rtic-app-model` must remain portable and serialization-friendly.
- Rendering must consume only a valid `ResolvedApplication`.
- Schema parsing must not perform resolution.
- Resolution must not write files.
- Validation must be callable independently.
- CLI orchestration may invoke Cargo and filesystem operations.
- Platform implementation crates must not depend on the builder.
- Functional FerroWasp crates must not depend on the builder.
- Generated applications depend on normal FerroWasp crates and selected HAL/backend crates.

---

## 5. Core domain model

All external inputs and generated artifacts require an explicit `schema_version`. Stable IDs shall be strings with restricted syntax:

```text
[a-z][a-z0-9]*(?:[-_][a-z0-9]+)*
```

Use namespaced IDs where collision risk exists:

```text
component:uart-dma-endpoint
instance:uart1
capability:observe/gyro-state
resource:dma2-stream2
task:uart1-rx
```

### 5.1 Source identity and diagnostics

Every parsed item should retain source provenance.

```rust
pub struct SourceRef {
    pub path: Utf8PathBuf,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub severity: Severity,
    pub summary: String,
    pub detail: Option<String>,
    pub primary: Option<SourceRef>,
    pub related: Vec<RelatedDiagnostic>,
    pub help: Option<String>,
}
```

Do not expose internal panics as user diagnostics. Expected invalid input returns structured diagnostics. Internal invariant violations may panic only in tests; production CLI converts them into a builder-internal failure with a preserved error chain.

### 5.2 Board definition

`BoardDefinition` records physical facts and supported concrete endpoint resources. It must not contain flight algorithms, tuning, or user-selected protocol roles.

```rust
pub struct BoardDefinition {
    pub schema_version: SchemaVersion,
    pub id: BoardId,
    pub family: PlatformFamilyId,
    pub backend: BackendId,
    pub mcu: McuId,
    pub revision: Option<String>,
    pub clocks: ClockDefinition,
    pub memory_regions: Vec<MemoryRegion>,
    pub pins: BTreeMap<PinId, PinDefinition>,
    pub peripherals: BTreeMap<PeripheralId, PeripheralDefinition>,
    pub dma_routes: BTreeMap<DmaRouteId, DmaRouteDefinition>,
    pub interrupts: BTreeMap<InterruptId, InterruptDefinition>,
    pub timers: BTreeMap<TimerResourceId, TimerDefinition>,
    pub fitted_devices: BTreeMap<DeviceId, FittedDevice>,
    pub endpoint_slots: BTreeMap<EndpointSlotId, EndpointSlot>,
    pub reserved_resources: BTreeSet<PhysicalResourceId>,
    pub metadata: BoardMetadata,
}
```

Important distinction:

- `endpoint_slots` describe hardware that is compiled and may be bound to a logical endpoint instance.
- Application profiles select endpoint/component instances.
- Platform configuration may later select a boot-frozen protocol role among already compiled compatible roles.
- Runtime configuration never changes physical ownership.

### 5.3 Application profile

```rust
pub struct ApplicationProfile {
    pub schema_version: SchemaVersion,
    pub id: ApplicationId,
    pub board: BoardId,
    pub backend: Option<BackendId>,
    pub components: Vec<ComponentInstanceRequest>,
    pub explicit_connections: Vec<ConnectionRequest>,
    pub scheduling_classes: BTreeMap<SchedulingClassId, Priority>,
    pub capacities: BTreeMap<CapacityId, usize>,
    pub policies: ApplicationPolicies,
    pub feature_flags: BTreeSet<FeatureId>,
    pub build_profile: BuildProfileId,
}
```

Application profile owns:

- selected components;
- instance IDs;
- scheduling policy;
- capacities;
- capability connections;
- compile-time feature composition;
- policy restrictions.

It does not own board physical facts.

### 5.4 Component definition

A component definition is catalogue metadata for an existing implementation.

```rust
pub struct ComponentDefinition {
    pub schema_version: SchemaVersion,
    pub id: ComponentTypeId,
    pub version: ComponentVersion,
    pub maturity: ComponentMaturity,
    pub implementation: ImplementationReference,
    pub multiplicity: MultiplicityPolicy,
    pub provides: Vec<ProvidedCapabilityDefinition>,
    pub requires: Vec<RequiredCapabilityDefinition>,
    pub tasks: Vec<TaskTemplate>,
    pub resources: Vec<ResourceTemplate>,
    pub physical_claims: Vec<PhysicalClaimTemplate>,
    pub capacities: Vec<CapacityRequirement>,
    pub initialization: InitializationTemplate,
    pub configuration: Vec<ConfigurationBinding>,
    pub failure_contract: FailureContract,
    pub tests: TestReferences,
}
```

`ImplementationReference` shall refer to typed Rust items by path:

```rust
pub struct ImplementationReference {
    pub crate_name: String,
    pub module_path: String,
    pub init_fn: Option<String>,
    pub task_entrypoints: BTreeMap<TaskTemplateId, String>,
    pub resource_types: BTreeMap<ResourceTemplateId, String>,
    pub cargo_features: BTreeSet<String>,
}
```

This is a reference, not arbitrary source.

### 5.5 Capability classes

```rust
pub enum CapabilityClass {
    Authority,
    Critical,
    Request,
    Observe,
    ObserveEvent,
    Service,
}
```

A capability key shall include:

- class;
- semantic type ID;
- version;
- optional instance/domain qualifier;
- cardinality rules.

```rust
pub struct CapabilityKey {
    pub class: CapabilityClass,
    pub type_id: CapabilityTypeId,
    pub version: CapabilityVersion,
    pub domain: Option<String>,
}
```

Connection rules:

| Class | Providers | Consumers | Fan-out | Missing connection |
|---|---:|---:|---|---|
| `Authority<T>` | exactly 1 unless profile explicitly excludes capability | normally exactly 1 authorized consumer | prohibited by default | error |
| `Critical<T>` | exactly 1 unless aggregation component is explicit | one or more explicit consumers | no automatic fan-out | error for required input |
| `Request<T>` | one service owner | one or more requesters | explicit | error if required |
| `Observe<T>` | exactly 1 writer | zero or more readers | allowed | allowed |
| `ObserveEvent<T>` | exactly 1 journal owner or explicit aggregator | zero or more readers | allowed and bounded | allowed |
| `Service<T>` | exactly 1 selected provider | one or more clients | explicit or uniquely inferred | error if required |

The resolver must never infer an authority connection solely because types match. Authority edges must be explicitly declared or introduced by a narrowly defined core policy that is visible in the resolved graph.

### 5.6 Component instances

```rust
pub struct ComponentInstanceRequest {
    pub id: ComponentInstanceId,
    pub component: ComponentTypeId,
    pub version_req: VersionReq,
    pub bind: BTreeMap<BindingKey, BindingValue>,
    pub scheduling_class: Option<SchedulingClassId>,
    pub configuration: BTreeMap<ConfigKey, ConfigValue>,
    pub enabled: bool,
}
```

Every generated symbol derives from the instance ID, not only from the component type. This is required for repeated UARTs, IMUs, observers, and protocol decoders.

### 5.7 Tasks

```rust
pub enum TaskKind {
    Init,
    Idle,
    Hardware { interrupt: InterruptId },
    Software,
}

pub struct TaskTemplate {
    pub id: TaskTemplateId,
    pub kind: TaskKindTemplate,
    pub entrypoint: RustPath,
    pub scheduling: SchedulingRequirement,
    pub local_resources: Vec<ResourceTemplateId>,
    pub shared_resources: Vec<ResourceAccessTemplate>,
    pub spawn_inputs: Vec<SpawnInput>,
    pub capacity: Option<CapacityRequirement>,
    pub period: Option<DurationRequirement>,
}
```

The resolved task contains exact:

- generated name;
- task kind;
- hardware interrupt if applicable;
- numeric priority;
- dispatcher if software task;
- local/shared resource names;
- capacity;
- period;
- spawn edges;
- source component instance.

### 5.8 Resources

Resources are separated into:

- physical resources;
- generated static storage;
- RTIC local resources;
- RTIC shared resources;
- immutable constants;
- initialization-only values.

```rust
pub enum ResourceKind {
    Physical(PhysicalResourceId),
    Local,
    Shared,
    StaticBuffer,
    StaticQueue,
    Snapshot,
    Constant,
}
```

A resolved physical resource has exactly one owner. Shared logical resources may have several task accesses, but exactly one generated storage definition.

### 5.9 Scheduling and dispatchers

Profiles use named scheduling classes. The resolver records numeric priorities.

```rust
pub struct SchedulingRequirement {
    pub class: SchedulingClassId,
    pub relation: Vec<PriorityRelation>,
    pub must_be_hardware: bool,
}
```

Example classes:

```text
actuator
sensor
control
serial
health
observer
background
```

Dispatcher allocation requires:

1. inventory hardware interrupt claims;
2. inventory backend-reserved and forbidden interrupts;
3. find distinct software-task priorities;
4. allocate one dispatcher interrupt per required software priority;
5. preserve deterministic assignments;
6. report allocation;
7. reject conflicts.

The first implementation may require dispatchers to be explicitly listed in the board definition. A general allocator can be added after the first vertical slice.

### 5.10 Resolved application

`ResolvedApplication` is the central stable intermediate representation.

```rust
pub struct ResolvedApplication {
    pub schema_version: SchemaVersion,
    pub builder_version: String,
    pub id: ResolvedApplicationId,
    pub input_identity: InputIdentity,
    pub board: ResolvedBoardIdentity,
    pub backend: ResolvedBackendIdentity,
    pub toolchain: ToolchainIdentity,
    pub components: Vec<ResolvedComponentInstance>,
    pub capabilities: Vec<ResolvedCapability>,
    pub connections: Vec<ResolvedConnection>,
    pub tasks: Vec<ResolvedTask>,
    pub resources: Vec<ResolvedResource>,
    pub physical_claims: Vec<ResolvedPhysicalClaim>,
    pub dispatchers: Vec<ResolvedDispatcher>,
    pub initialization_order: Vec<InitializationStep>,
    pub cargo: ResolvedCargoPlan,
    pub generated_names: GeneratedNameTable,
    pub warnings: Vec<Diagnostic>,
}
```

Requirements:

- canonical ordering;
- no maps serialized in nondeterministic order;
- stable schema;
- self-contained enough for rendering and reporting;
- source references retained where practical;
- hash computed from canonical serialized representation;
- builder version and input hashes recorded;
- no target-runtime values that are only known after execution.

---

## 6. Input schemas and examples

### 6.1 Board definition example

```toml
schema_version = "0.1"

[board]
id = "reference-f405"
family = "stm32f4"
backend = "ferrowasp-stm32f4"
mcu = "STM32F405RGT6"
revision = "v1"

[clock]
source = "hse"
hse_hz = 8_000_000
sysclk_hz = 168_000_000

[interrupts]
reserved = ["OTG_FS", "OTG_HS"]
dispatchers = ["EXTI2", "EXTI3", "EXTI4", "TIM6_DAC"]

[endpoint_slots.uart1]
kind = "uart-dma"
peripheral = "USART1"
rx = "PA10"
tx = "PA9"
rx_interrupt = "USART1"
dma_rx = "DMA2_STREAM2_CHANNEL4"
dma_tx = "DMA2_STREAM7_CHANNEL4"
rx_buffer_bytes = 256
tx_buffer_bytes = 256

[endpoint_slots.spi1_imu]
kind = "spi-dma-sample"
peripheral = "SPI1"
sck = "PB3"
miso = "PB4"
mosi = "PB5"
cs = "PA4"
data_ready = "PB0"
data_ready_interrupt = "EXTI0"
dma_rx = "DMA2_STREAM0_CHANNEL3"
dma_tx = "DMA2_STREAM3_CHANNEL3"

[fitted_devices.imu1]
driver = "mpu6500"
endpoint_slot = "spi1_imu"
board_rotation = "identity"

[output_slots.motor1]
kind = "waveform"
timer = "TIM1_CH1"
pin = "PA8"
```

Validation rules include:

- referenced pins/peripherals/routes exist;
- physical resources are unique unless explicitly shareable;
- timer and DMA routes are supported by backend metadata;
- dispatcher interrupts are not reserved or claimed by hardware tasks;
- capacities are within backend limits;
- fitted device bindings are compatible with endpoint slot type.

### 6.2 Application profile example

```toml
schema_version = "0.1"

[application]
id = "nucleo-msp-reference"
board = "nucleo-f401re"
build_profile = "dev"

[scheduling_classes]
serial = 8
observer = 3
background = 1

[capacities]
uart_rx_bytes = 256
uart_tx_bytes = 256
msp_frames = 8

[[components]]
id = "uart1"
component = "uart-dma-endpoint"
version = "0.1"
scheduling_class = "serial"

[components.bind]
endpoint_slot = "uart1"
rx_capacity = "uart_rx_bytes"
tx_capacity = "uart_tx_bytes"

[[components]]
id = "displayport"
component = "msp-displayport"
version = "0.1"
scheduling_class = "observer"

[components.bind]
rx_endpoint = "uart1"
tx_endpoint = "uart1"
frame_capacity = "msp_frames"

[[connections]]
from = "uart1.provides.critical:serial-rx-bytes"
to = "displayport.requires.critical:serial-rx-bytes"

[[connections]]
from = "displayport.provides.request:serial-tx-frame"
to = "uart1.requires.request:serial-tx-frame"
```

### 6.3 Component definition example: UART-DMA endpoint

```toml
schema_version = "0.1"

[component]
id = "uart-dma-endpoint"
version = "0.1.0"
maturity = "experimental"
multiplicity = "many"

[implementation]
crate = "ferrowasp-stm32f4"
module = "serial::uart_dma"
init_fn = "init_endpoint"

[implementation.task_entrypoints]
rx_irq = "on_rx_interrupt"
tx_irq = "on_tx_interrupt"
tx_service = "service_tx"

[[provides]]
id = "rx_bytes"
class = "critical"
type = "serial-rx-bytes"
version = "1"

[[requires]]
id = "tx_request"
class = "request"
type = "serial-tx-frame"
version = "1"
cardinality = "zero-or-more"

[[tasks]]
id = "rx_irq"
kind = "hardware"
interrupt_from_binding = "endpoint_slot.rx_interrupt"
scheduling_class = "serial"

[[tasks]]
id = "tx_service"
kind = "software"
scheduling_class = "serial"
capacity_from_binding = "tx_queue_capacity"

[[resources]]
id = "endpoint"
kind = "local"
rust_type = "ferrowasp_stm32f4::serial::UartDmaEndpoint"

[[resources]]
id = "rx_buffer"
kind = "static-buffer"
size_from_binding = "rx_capacity"

[[resources]]
id = "tx_queue"
kind = "static-queue"
size_from_binding = "tx_capacity"

[[physical_claims]]
binding = "endpoint_slot.peripheral"
exclusive = true

[[physical_claims]]
binding = "endpoint_slot.dma_rx"
exclusive = true

[[physical_claims]]
binding = "endpoint_slot.dma_tx"
exclusive = true

[[physical_claims]]
binding = "endpoint_slot.rx"
exclusive = true

[[physical_claims]]
binding = "endpoint_slot.tx"
exclusive = true
```

### 6.4 Component definition example: MSP DisplayPort consumer

```toml
schema_version = "0.1"

[component]
id = "msp-displayport"
version = "0.1.0"
maturity = "experimental"
multiplicity = "many"

[implementation]
crate = "ferrowasp-tasks"
module = "osd::msp_displayport"
init_fn = "init"

[implementation.task_entrypoints]
consume_rx = "consume_rx"
periodic = "periodic"
submit_tx = "submit_tx"

[[requires]]
id = "rx_bytes"
class = "critical"
type = "serial-rx-bytes"
version = "1"
cardinality = "exactly-one"

[[provides]]
id = "tx_frame"
class = "request"
type = "serial-tx-frame"
version = "1"

[[tasks]]
id = "consume_rx"
kind = "software"
scheduling_class = "observer"
capacity = 8

[[tasks]]
id = "periodic"
kind = "software"
scheduling_class = "observer"
period_us = 100_000
capacity = 1
```

---

## 7. Resolution pipeline

The CLI must expose each stage independently for testing and diagnosis.

```text
load
    -> parse
    -> schema validate
    -> normalize
    -> catalogue load
    -> instantiate candidates
    -> resolve dependencies
    -> resolve capability connections
    -> allocate generated names
    -> resolve physical bindings
    -> resolve scheduling and dispatchers
    -> calculate initialization order
    -> validate resolved graph
    -> canonicalize
    -> write ResolvedApplication
    -> render source
    -> format
    -> cargo check/build
    -> report
    -> transactional commit
```

### 7.1 Stage 1: load and parse

Responsibilities:

- read UTF-8 files;
- reject duplicate input paths;
- parse TOML;
- attach source locations where parser support permits;
- reject unknown top-level schema versions;
- preserve unknown fields only if an explicit forward-compatibility policy permits them; default is rejection.

Tests:

- valid minimal input;
- invalid UTF-8;
- malformed TOML;
- unsupported schema;
- unknown field;
- duplicate table or ID;
- missing required field.

### 7.2 Stage 2: normalize

Normalization shall:

- canonicalize IDs;
- expand defaults that are genuinely policy defaults;
- resolve relative paths against a defined root;
- sort order-insensitive inputs;
- normalize durations and sizes to canonical units;
- reject aliases that would produce ambiguous identity.

Normalization shall not select providers or physical resources.

### 7.3 Stage 3: catalogue loading

Catalogue loading shall:

- discover component definitions from explicit roots;
- validate every component independently;
- reject duplicate component ID/version combinations;
- create an immutable indexed catalogue;
- record catalogue file hashes;
- support exact versions first;
- add semantic version ranges only after exact-version operation is stable.

Near-term policy: application profiles should pin exact component versions.

### 7.4 Stage 4: candidate instantiation

For every requested component:

- validate component exists;
- validate requested version;
- validate instance ID uniqueness;
- apply bindings;
- expand task/resource/capability templates;
- retain source link to request and definition;
- reject missing required binding;
- reject unknown binding;
- reject incompatible board/backend requirement.

### 7.5 Stage 5: dependency closure

Near-term closure should be explicit and conservative.

Supported mechanisms:

1. profile explicitly lists all components; or
2. a component declares a required implementation dependency with exactly one catalogue provider.

Do not initially support broad “find any provider for this type” behavior. Ambiguous providers are an error.

Future deterministic closure may:

- select a provider specified by policy;
- select the only compatible provider;
- add required adapter components;
- preserve full resolution reasoning in the report.

### 7.6 Stage 6: capability connection resolution

Resolution order:

1. validate explicit connections;
2. resolve uniquely inferable non-authority service/observe connections if policy permits;
3. require explicit authority edges;
4. require explicit critical fan-out;
5. validate class/type/version compatibility;
6. validate cardinality;
7. validate prohibited direction or policy;
8. emit unconnected required-capability errors;
9. emit unused optional-capability warnings.

Every resolved connection records why it exists:

```rust
pub enum ResolutionReason {
    ExplicitProfileConnection,
    UniqueProviderInference,
    RequiredImplementationDependency,
    CorePolicy { policy_id: String },
}
```

### 7.7 Stage 7: generated-name allocation

Generated names use:

```text
<instance_id>__<template_id>
```

Examples:

```text
uart1__rx_irq
uart1__endpoint
displayport__periodic
imu1__sample_ready
```

Rules:

- sanitize to valid Rust identifiers;
- reject two source IDs that normalize to the same Rust identifier;
- use stable suffixes only when unavoidable;
- record all mappings;
- never depend on input file iteration order;
- reserve RTIC and Rust keywords.

### 7.8 Stage 8: physical resource resolution

The resolver shall expand bindings into physical claims and validate:

- one exclusive owner;
- permitted sharing for explicitly shareable resources only;
- pin alternate-function compatibility;
- peripheral/pin compatibility;
- DMA compatibility;
- timer-channel compatibility;
- interrupt conflicts;
- backend-reserved resources;
- board-reserved resources;
- memory-region compatibility;
- static-buffer placement constraints where known.

Near-term implementation may rely on explicit board-declared valid endpoint slots rather than a full MCU resource database. This greatly reduces solver complexity and prevents false inference.

### 7.9 Stage 9: scheduling resolution

The resolver shall:

- resolve every task’s scheduling class;
- look up numeric priority;
- validate required relative ordering;
- distinguish hardware and software priorities;
- group software tasks by numeric priority;
- allocate one dispatcher per used software priority;
- reject collision with claimed/reserved interrupts;
- record priority ceilings inputs for later reporting;
- reject missing class or out-of-range priority.

Near-term dispatcher policy:

- board definition supplies an ordered list of allowed dispatcher interrupts;
- resolver sorts distinct software priorities descending;
- allocates dispatchers deterministically in list order;
- one dispatcher per distinct software priority;
- emits explicit report.

### 7.10 Stage 10: initialization order

Initialization order is derived from:

- physical endpoint construction before consumers;
- provider before consumer where initialization requires a handle;
- explicit initialization dependencies;
- no dependency cycle.

Use a stable topological sort. Cycles produce a diagnostic containing the cycle path.

The builder must distinguish:

- construction dependency;
- runtime capability edge;
- spawn edge;
- observation edge.

They are not interchangeable.

### 7.11 Stage 11: resolved validation

Run all architecture validators against the complete graph:

- ID uniqueness;
- exact physical ownership;
- authority graph;
- capability completeness;
- task/resource consistency;
- scheduling relations;
- dispatcher validity;
- initialization acyclicity;
- capacity completeness;
- no unsupported component/backend pair;
- experimental policy restrictions;
- generated name uniqueness;
- Cargo dependency completeness;
- one monotonic policy;
- one RTIC app root;
- one actuator owner if motor output is included.

Validation should return all independent diagnostics in one run where safe, rather than stopping at the first error.

### 7.12 Stage 12: canonicalization and hashing

Canonicalization shall:

- sort lists by stable ID where ordering has no semantic meaning;
- preserve explicit semantic order where required;
- serialize using a fixed format;
- exclude volatile timestamps from identity hash;
- include builder semantic version, schema versions, catalogue hashes, board hash, profile hash, and selected toolchain identity.

Suggested identity:

```text
resolved_application_id =
    <application-id>-<board-id>-<first-12-hex-of-sha256>
```

### 7.13 Stage 13: rendering

Renderer inputs:

- valid `ResolvedApplication`;
- render options containing output paths and generated-header policy.

Renderer outputs:

```text
generated/<id>/
├── Cargo.toml
├── build.rs                 # only if required and deterministic
├── memory.x                 # only when target policy requires generated memory file
├── src/
│   ├── main.rs
│   ├── generated/
│   │   ├── mod.rs
│   │   ├── app.rs
│   │   ├── init.rs
│   │   ├── tasks.rs
│   │   ├── resources.rs
│   │   ├── capabilities.rs
│   │   └── provenance.rs
│   └── user_hooks.rs        # preferably absent; no arbitrary behavioral hook
├── resolved_application.json
└── reports/
```

For the first implementation, a single generated `src/main.rs` is acceptable if it is clearer and easier to compare. Split files only when the renderer and tests remain simpler.

### 7.14 Stage 14: formatting and compiler invocation

- invoke `rustfmt` using pinned toolchain;
- invoke Cargo with JSON message format;
- preserve full rustc rendered diagnostics;
- add only concise builder context:
  - application ID;
  - checkpoint;
  - generated directory;
  - command;
- do not relabel rustc errors as component defects without proof.

### 7.15 Stage 15: transactional commit

Use a working directory:

```text
generated/work/<application-id>/<run-id>/
```

Process:

1. render into a clean run directory;
2. format;
3. write IR and reports;
4. run `cargo check`;
5. optionally run release build and configured tests;
6. on success, atomically replace/update `generated/committed/<resolved-id>/`;
7. update a stable pointer file for the application;
8. preserve failed candidate under `generated/failed/` only when requested or in CI artifacts;
9. never partially overwrite the last valid committed output.

No implementation shall “incrementally edit” the committed generated application in place.

---

## 8. Command-line interface

The builder should be accessible through the existing project `xtask` interface where practical.

Recommended commands:

```text
cargo xtask app validate \
  --board boards/nucleo-f401re.toml \
  --profile application_profiles/nucleo-msp.toml

cargo xtask app resolve \
  --board ... \
  --profile ... \
  --out generated/work/...

cargo xtask app generate \
  --board ... \
  --profile ... \
  --check

cargo xtask app build \
  --board ... \
  --profile ... \
  --release

cargo xtask app diff \
  --old generated/committed/<old>/resolved_application.json \
  --new generated/work/<new>/resolved_application.json

cargo xtask app graph \
  --resolved ... \
  --format mermaid

cargo xtask app explain \
  --resolved ... \
  --component uart1

cargo xtask app catalogue validate \
  component_catalogue/

cargo xtask app clean --failed
```

### 8.1 Exit codes

| Code | Meaning |
|---:|---|
| 0 | success |
| 2 | input/schema diagnostic |
| 3 | resolution/validation diagnostic |
| 4 | rendering failure |
| 5 | formatter failure |
| 6 | Cargo/rustc failure |
| 7 | target test failure |
| 8 | deterministic-output drift |
| 10 | internal builder failure |

### 8.2 Machine-readable output

All commands should support:

```text
--diagnostic-format human
--diagnostic-format json
```

JSON diagnostics enable the configurator, CI, and future editor integrations.

### 8.3 Explainability

`app explain` should answer:

- why a component exists;
- why a capability edge exists;
- which physical resources it owns;
- which tasks it contributes;
- which priority and dispatcher it uses;
- which input source selected it;
- which Cargo features it requires.

This avoids making the builder an opaque solver.

---

## 9. Generated application structure

The generated app should be intentionally boring.

Illustrative RTIC skeleton:

```rust
#![no_std]
#![no_main]

use rtic::app;

mod generated_provenance {
    pub const BUILDER_VERSION: &str = "...";
    pub const RESOLVED_APPLICATION_ID: &str = "...";
    pub const INPUT_HASH: &str = "...";
}

#[app(
    device = stm32f4xx_hal::pac,
    dispatchers = [EXTI2, EXTI3],
    peripherals = true
)]
mod app {
    #[shared]
    struct Shared {
        displayport__state: ferrowasp_tasks::osd::DisplayPortState,
    }

    #[local]
    struct Local {
        uart1__endpoint: ferrowasp_stm32f4::serial::UartDmaEndpoint,
        uart1__rx_buffer: &'static mut [u8; 256],
    }

    #[init(local = [
        uart1__rx_storage: [u8; 256] = [0; 256],
        uart1__tx_storage: [u8; 256] = [0; 256],
    ])]
    fn init(ctx: init::Context) -> (Shared, Local) {
        // Deterministically ordered generated construction.
        // Calls normal implementation-crate constructors.
        todo!()
    }

    #[task(
        binds = USART1,
        priority = 8,
        local = [uart1__endpoint],
        shared = [displayport__state]
    )]
    fn uart1__rx_irq(ctx: uart1__rx_irq::Context) {
        ferrowasp_stm32f4::serial::uart_dma::on_rx_interrupt(...);
    }

    #[task(priority = 3, capacity = 1, shared = [displayport__state])]
    async fn displayport__periodic(ctx: displayport__periodic::Context) {
        ferrowasp_tasks::osd::msp_displayport::periodic(...);
    }
}
```

The implementation crate functions remain responsible for behavior. The renderer supplies types, names, bindings, resource placement, priorities, and wiring.

---

## 10. Diagnostics catalogue

Use stable diagnostic codes. Initial set:

### Schema and identity

```text
APP001 unsupported schema version
APP002 duplicate ID
APP003 invalid ID syntax
APP004 unknown field
APP005 missing required field
APP006 invalid value/range
```

### Catalogue and component

```text
CMP001 unknown component
CMP002 unsupported component version
CMP003 duplicate component version
CMP004 missing binding
CMP005 unknown binding
CMP006 incompatible board/backend
CMP007 invalid component metadata
```

### Capability graph

```text
CAP001 missing required capability
CAP002 incompatible capability type/version
CAP003 ambiguous provider
CAP004 prohibited authority inference
CAP005 invalid cardinality
CAP006 prohibited capability edge
CAP007 unused optional capability
```

### Resource ownership

```text
RES001 physical resource conflict
RES002 reserved resource claimed
RES003 incompatible pin/peripheral binding
RES004 invalid DMA route
RES005 invalid timer binding
RES006 interrupt conflict
RES007 static capacity missing
RES008 capacity exceeds backend limit
```

### Scheduling

```text
SCH001 missing scheduling class
SCH002 invalid priority
SCH003 required priority relation violated
SCH004 insufficient dispatchers
SCH005 dispatcher conflicts with hardware task
SCH006 initialization dependency cycle
```

### Safety policy

```text
SAF001 multiple actuator owners
SAF002 multiple actuation-authority providers
SAF003 unauthorized authority consumer
SAF004 observer connected to authority path
SAF005 experimental component claims protected resource
SAF006 motor output present without safety-state input
SAF007 motor output present without freshness policy
```

### Rendering/build

```text
GEN001 generated-name collision
GEN002 unsupported render construct
GEN003 nondeterministic output detected
BLD001 rustfmt failure
BLD002 cargo check failure
BLD003 release build failure
BLD004 generated output drift
```

Diagnostics shall include source file/line where possible and related claims for conflicts.

---

# Part I — Near-Term Reference Implementation

## 11. Near-term objective

**Time horizon:** immediate work through approximately six months.

The near-term objective is not “generate the entire flight controller.” It is:

> Establish one trustworthy composition pipeline that can resolve, render, check, inspect, and reproduce a small RTIC application, then extend the exact same model to a generated parallel F405 application without replacing the golden flight application.

The near-term programme is divided into two sub-phases:

- **Immediate foundation:** approximately 0–8 weeks;
- **Near-term expansion:** approximately 2–6 months.

---

## 12. Immediate foundation: 0–8 weeks

### N0 — Repository reconnaissance and protected baseline

**Objective:** Map the plan onto the actual repository before changing architecture.

**Codex actions:**

1. Read:
   - root `Cargo.toml`;
   - existing `xtask`;
   - current builder prototypes;
   - NUCLEO LED/button app;
   - NUCLEO MSP DisplayPort app;
   - UART-DMA endpoint code;
   - OSD consumer code;
   - current generated code/checkpoint logic;
   - `PROJECT_CONTEXT.md`;
   - `CODEX_ACTIVE_WORK.md`;
   - active ADRs;
   - target support documents.
2. Produce `tools/rtic-app-builder/IMPLEMENTATION_INVENTORY.md` containing:
   - existing paths;
   - reusable code;
   - transitional code;
   - duplicate responsibilities;
   - current tests;
   - missing tests;
   - current command entrypoints;
   - safety-sensitive files not to modify.
3. Record the exact golden application and target configuration.
4. Add no behavior changes in this task.

**Acceptance criteria:**

- inventory references exact files and symbols;
- current prototype commands are documented and reproduced;
- current tests/checks are run;
- protected flight files are listed;
- no runtime behavior changed.

**Non-goals:**

- restructure the workspace;
- rename every crate;
- refactor the golden app;
- add general abstractions.

---

### N1 — Architecture decision records and vocabulary freeze

**Objective:** Freeze stable terminology before public APIs are created.

Create or update ADRs for:

1. `BoardDefinition` versus backend/BSP terminology;
2. `ApplicationProfile`;
3. versioned `ResolvedApplication`;
4. capability classes;
5. endpoint versus functional consumer;
6. one central renderer;
7. transactional generation;
8. explicit verbose board targets;
9. golden handwritten app preservation;
10. public builder/private assurance boundary.

**Acceptance criteria:**

- each ADR states context, decision, alternatives, consequences, migration, and status;
- terms match code and schema names;
- no two documents use conflicting names without a migration note.

---

### N2 — Builder workspace and model crate

**Objective:** Establish dependency boundaries before implementing features.

**Create or map:**

```text
rtic-app-model
rtic-app-schema
rtic-app-catalogue
rtic-app-resolver
rtic-app-validator
rtic-app-render
rtic-app-report
rtic-app-cli
```

A reduced initial crate count is acceptable:

```text
rtic-app-core   # model + resolver + validator
rtic-app-cli    # schema + orchestration + render/report modules
```

provided modules retain the future boundaries and do not create cyclic dependencies.

**Minimum model types:**

- IDs/newtypes;
- `SourceRef`;
- diagnostics;
- `BoardDefinition`;
- `ApplicationProfile`;
- `ComponentDefinition`;
- capability definitions;
- task/resource templates;
- `ResolvedApplication`;
- canonical serialization.

**Implementation rules:**

- use `BTreeMap`/`BTreeSet` for deterministic ordering;
- derive `serde` traits where appropriate;
- use `camino::Utf8PathBuf` or a consistent path type;
- use `thiserror` only for internal error chains, not as the user diagnostic model;
- avoid HAL/RTIC dependencies in the model crate;
- deny unsafe code in builder crates;
- add `#![forbid(unsafe_code)]` where dependencies permit.

**Tests:**

- ID validation;
- canonical ordering;
- serialization round trip;
- stable hash fixture;
- duplicate ID diagnostics;
- no nondeterministic map output.

**Acceptance criteria:**

```text
cargo test -p rtic-app-model
cargo test -p rtic-app-schema
```

pass on Linux and Windows-compatible path fixtures.

---

### N3 — Schema v0.1

**Objective:** Parse explicit board, application, and component definitions.

**Deliverables:**

```text
schemas/board-definition-v0.1.schema.json
schemas/application-profile-v0.1.schema.json
schemas/component-definition-v0.1.schema.json
```

The implementation may use Rust validation as authoritative and generate JSON Schema for tooling, or validate using both. Avoid maintaining two inconsistent rule sets.

**Schema v0.1 restrictions:**

- exact versions;
- explicit endpoint slots;
- explicit dispatcher list;
- explicit capacities;
- explicit connections;
- no compact solver inputs;
- no conditional expressions;
- no arbitrary code;
- no inheritance;
- no runtime routing;
- no implicit component discovery.

**Fixtures:**

```text
tests/fixtures/valid/minimal-led/
tests/fixtures/valid/nucleo-msp/
tests/fixtures/invalid/duplicate-id/
tests/fixtures/invalid/resource-conflict/
tests/fixtures/invalid/missing-capability/
tests/fixtures/invalid/authority-fanout/
```

**Acceptance criteria:**

- all valid fixtures parse and normalize;
- each invalid fixture produces the expected diagnostic code;
- unknown fields are rejected;
- schema version is mandatory;
- diagnostics identify the source path.

---

### N4 — First component catalogue

**Objective:** Represent the existing NUCLEO MSP prototype without embedding behavior in the builder.

Initial catalogue:

```text
component_catalogue/
├── platform/
│   ├── monotonic-systick.toml
│   └── gpio-led.toml
├── endpoints/
│   └── uart-dma-endpoint.toml
└── functions/
    ├── button-edge.toml
    └── msp-displayport.toml
```

**Required decomposition:**

```text
UART-DMA endpoint
    owns USART, DMA, pins, buffers, interrupts
    provides bounded RX capability
    consumes bounded TX requests

MSP DisplayPort consumer
    owns parser/renderer state only
    receives RX capability
    submits TX requests
    contributes periodic software task
```

**Codex must not:**

- copy UART state machine logic into builder code;
- merge OSD and UART because it is convenient;
- expose HAL types to the MSP functional component;
- introduce generic messaging for the entire flight stack.

**Acceptance criteria:**

- the catalogue validates independently;
- component definitions refer to existing Rust paths;
- missing Rust paths are detected during a catalogue smoke build or generated app check;
- repeated UART instance names produce distinct generated symbols in unit tests.

---

### N5 — Deterministic resolver v0.1

**Objective:** Produce a complete `ResolvedApplication` for LED/button and MSP vertical slices.

**Required resolver functions:**

```rust
pub fn resolve_application(
    board: &BoardDefinition,
    profile: &ApplicationProfile,
    catalogue: &ComponentCatalogue,
    toolchain: &ToolchainPolicy,
) -> Result<ResolvedApplication, DiagnosticSet>;
```

Internal passes:

```text
validate_inputs
instantiate_components
resolve_explicit_connections
allocate_names
resolve_physical_claims
resolve_scheduling
allocate_dispatchers
order_initialization
validate_architecture
canonicalize
```

Each pass should be unit-testable and either:

- mutate a private builder state with documented invariants; or
- return a typed next-stage structure.

Avoid a long untyped `serde_json::Value` transformation pipeline.

**Acceptance criteria:**

- same inputs produce identical canonical IR and hash across repeated runs;
- input file order does not affect output;
- component catalogue traversal order does not affect output;
- resource conflicts report both owners;
- required capability omissions report provider/consumer context;
- initialization cycles report a readable cycle;
- no generated source is required to test the resolver.

---

### N6 — Central RTIC renderer v0.1

**Objective:** Render the two validation applications from the same IR.

**First rendering targets:**

1. NUCLEO LED/button;
2. NUCLEO UART-DMA/MSP DisplayPort.

**Renderer responsibilities:**

- imports;
- RTIC `#[app]` declaration;
- dispatcher list;
- `Shared`/`Local`;
- init local static storage;
- deterministic initialization statements;
- hardware tasks;
- software tasks;
- periodic spawn/schedule setup;
- component entrypoint calls;
- generated provenance;
- Cargo dependencies/features.

**Renderer non-responsibilities:**

- protocol parsing;
- LED state behavior;
- DMA state machine;
- OSD formatting;
- control logic;
- safety logic.

**Testing approach:**

- structured unit tests for emitted sections;
- golden-file snapshots for complete generated applications;
- compile tests are authoritative over textual snapshots;
- snapshots must be easy to update intentionally;
- forbid brittle tests that assert whitespace only.

**Acceptance criteria:**

```text
cargo xtask app generate --profile nucleo-led --check
cargo xtask app generate --profile nucleo-msp --check
```

both succeed from a clean checkout.

---

### N7 — Native compiler diagnostics and transactional checking

**Objective:** Make generated-code failures useful to Codex and humans.

**Implementation:**

- invoke Cargo with `--message-format=json-diagnostic-rendered-ansi`;
- stream rendered rustc diagnostics;
- capture command, status, and generated path;
- classify as `BLD002` without rewriting the rustc message;
- preserve failed work tree under an opt-in debug flag or CI artifact;
- never update committed generated output after a failed check;
- add a deterministic “last-known-good” pointer file.

**Checkpoint policy:**

Near-term generation may check after major complete checkpoints:

1. base RTIC shell;
2. resources and init;
3. hardware endpoint;
4. functional consumer;
5. complete application.

Do not create semantically invalid partial Rust solely to compile after every line. Check complete architectural checkpoints.

**Acceptance criteria:**

- an intentionally invalid Rust path surfaces the original rustc diagnostic;
- the last valid committed output remains unchanged;
- failed candidate path is printed;
- CI can upload failed source as an artifact;
- builder does not incorrectly blame the last component added.

---

### N8 — Reports v0.1

**Objective:** Make generated architecture inspectable before flight relevance.

Generate:

```text
reports/
├── summary.md
├── components.csv
├── tasks.csv
├── resources.csv
├── physical_claims.csv
├── capabilities.csv
├── connections.csv
├── dispatchers.csv
├── initialization_order.csv
└── architecture.mmd
```

`summary.md` should include:

- builder version;
- input hashes;
- resolved application ID;
- board/backend;
- components and versions;
- task/priority table;
- hardware ownership table;
- capability graph summary;
- warnings;
- build result.

**Acceptance criteria:**

- report order is deterministic;
- every generated task/resource maps to a component instance;
- every physical resource has one owner;
- every connection includes resolution reason;
- Mermaid graph renders without manual editing.

---

## 13. Near-term expansion: 2–6 months

### N9 — Explicit F405 board target

**Objective:** Encode one real F405 target as a verbose physical definition.

Preferred public target: Foxeer F405 V2.  
Internal evidence target: FCU3 where required.

**Work:**

- exact MCU/package;
- clocks;
- memory;
- pins;
- interrupts;
- UART endpoint slots;
- SPI/IMU endpoint slot;
- actuator output slots;
- DMA routes;
- timer channels;
- fitted devices;
- reserved resources;
- debug/USB resources.

**Required evidence note:**

The board definition is only a configuration artifact until its physical claims are checked against schematic, MCU reference data, HAL support, and target behavior.

**Acceptance criteria:**

- every declared physical route has a source reference;
- target compiles with selected backend;
- board definition lints cleanly;
- no resource is silently inferred;
- target support maturity is stated separately from feature completeness.

---

### N10 — SBUS vertical slice

**Objective:** Add a safety-relevant but bounded input chain without actuation.

Components:

```text
uart-dma-endpoint instance
    -> sbus-decoder
        -> qualified-rc-intent snapshot/critical output
```

Builder metadata shall express:

- bounded frame storage;
- decoder task;
- frame freshness;
- frame-lost/failsafe output;
- no authority;
- explicit scheduling relation relative to observer tasks.

**Host tests:**

- valid frames;
- malformed frames;
- frame loss;
- queue overflow;
- stale intent;
- repeated component instance naming.

**Target tests:**

- UART settings;
- DMA/IDLE/HT/TC behavior as applicable;
- bounded queue behavior;
- timestamp/freshness;
- no actuator ownership.

---

### N11 — SPI/DMA IMU vertical slice and first platform contract

**Objective:** Validate the endpoint/consumer boundary against a second hardware class.

Components:

```text
spi-dma-sample-endpoint
    -> imu-device-driver/service
        -> calibrated timestamped sample
            -> observation snapshot or test consumer
```

Define a versioned platform contract covering:

- request/start semantics;
- transfer-complete event;
- timestamp semantics;
- buffer ownership;
- cancellation/abort;
- bus error;
- timeout;
- overrun;
- recovery;
- data validity;
- initialization state.

The builder records ownership and wiring. Backend code implements the contract.

**Acceptance criteria:**

- host contract tests exist;
- target endpoint tests exist;
- no HAL-specific transfer type escapes into portable IMU service;
- generated app compiles with one IMU instance;
- initialization order is deterministic;
- two IMU instances either work or fail with an explicit unsupported-multiplicity diagnostic.

---

### N12 — Observation snapshot v0.1

**Objective:** Support read-only observer fan-out without introducing general pub/sub.

Near-term implementation may use RTIC shared state with:

- one writer;
- immutable copy/borrow semantics;
- generation/timestamp;
- bounded critical section;
- zero or more readers.

Builder rules:

- exactly one `Observe<T>` provider;
- automatic fan-out allowed only for `Observe<T>`;
- observers cannot provide `Authority<T>`;
- observer scheduling classes must not outrank protected critical classes without explicit policy;
- observer absence does not invalidate critical producer.

**Acceptance criteria:**

- OSD reads from a snapshot rather than canonical mutable critical state where practical;
- observer overload/failure test does not block the producer beyond measured accepted bounds;
- architecture report marks observation-plane edges distinctly.

---

### N13 — Generated parallel F405 candidate

**Objective:** Generate a meaningful static subset of the handwritten flight application.

Minimum candidate:

```text
one RC endpoint/decoder
one IMU endpoint/service
one periodic/sample-triggered control placeholder or existing portable control component
one safety-state input or test safety master
one actuator request path
one actuator owner using a bench-safe backend or output-inhibited configuration
one observation/logging consumer
```

Safety restrictions:

- generation is parallel;
- no replacement of golden app;
- motor output disabled by default;
- embedded default configuration remains non-flyable;
- props-off only until explicit bench gates pass;
- do not alter motor map, signs, priorities, or arming behavior without dedicated review.

Comparison report:

```text
handwritten vs generated
- component list
- peripheral ownership
- DMA ownership
- interrupts
- tasks
- priorities
- dispatchers
- static memory
- queue capacities
- initialization order
- feature flags
- binary size
- target observations
```

**Exit gate:**

- generated candidate reproduces intended static ownership;
- all compile and host tests pass;
- props-off target validation passes for declared scope;
- differences are explained;
- no unexplained alternate actuator path exists;
- golden app remains available and reproducible.

---

### N14 — CI baseline

Add CI jobs:

```text
builder-format
builder-clippy
builder-tests
schema-fixtures
catalogue-validation
determinism-linux
determinism-windows
generate-nucleo-led
generate-nucleo-msp
check-generated-drift
check-f405-candidate
```

Rules:

- lockfile committed;
- toolchain pinned;
- generated reference apps regenerated from clean directories;
- CI fails on unexplained drift;
- generated source is compiled;
- reports are uploaded for failed candidate jobs;
- no target hardware result is fabricated in CI.

---

## 14. Near-term exit criteria

The near-term reference implementation is complete only when:

1. one canonical model resolves LED/button and UART-DMA/MSP applications;
2. generated source is readable and compiler-checked;
3. the resolver is deterministic;
4. diagnostics are source-oriented and actionable;
5. endpoint and consumer behavior remains outside the builder;
6. physical ownership conflicts are rejected;
7. capability cardinality and authority policies are enforced;
8. one explicit F405 board target exists;
9. SBUS and one SPI/IMU vertical slice are represented;
10. a generated parallel F405 candidate exists;
11. the golden handwritten app has not been implicitly replaced;
12. architecture reports are generated;
13. CI detects generated drift;
14. all limitations are documented.

---

# Part II — Mid-Term Reference Implementation

## 15. Mid-term objective

**Time horizon:** approximately 6–18 months.

The mid-term objective is:

> Make the builder the normal static composition path for selected release applications, while proving that the same component graph can support F405, STM32H7, host simulation, and bounded verification without leaking backend-specific semantics into the flight core.

---

## 16. Mid-term work packages

### M1 — `ResolvedApplication` schema v1.0

Promote the IR to `1.0` only after:

- at least two generated validation apps;
- one generated F405 candidate;
- repeated component instances;
- one hardware endpoint from UART and SPI classes;
- exact priority/dispatcher resolution;
- capability graph reporting;
- migration tests from prior schema.

Add:

- schema migration library;
- explicit deprecation policy;
- compatibility matrix;
- canonical JSON fixture;
- semantic equality independent of source formatting;
- stable hash policy.

Do not freeze the schema based on design alone.

---

### M2 — General typed capability graph

Complete capability support for:

```text
Authority<T>
Critical<T>
Request<T>
Observe<T>
ObserveEvent<T>
Service<T>
```

Add policy validation:

- authority reachability;
- protected component classes;
- prohibited observer-to-critical paths;
- permitted adapters;
- explicit aggregation components;
- version compatibility;
- capability-domain isolation.

Add graph queries:

```text
providers_of(type)
consumers_of(type)
authority_path_to(resource)
critical_predecessors(component)
observers_of(snapshot)
unconnected_requirements()
```

These queries shall support reports, tests, and future assurance overlays.

---

### M3 — Deterministic dependency closure

Add a constrained provider-selection mechanism.

Rules:

1. exact explicit provider wins;
2. profile policy may name a preferred provider;
3. one compatible provider may be inferred;
4. multiple compatible providers are an error;
5. authority providers are never inferred;
6. resolution reason is recorded;
7. selected version is exact in the resolved IR;
8. no network access during resolution;
9. catalogue and lock state are input identities.

Support adapter components only when represented explicitly in the catalogue.

---

### M4 — Application-wide scheduling model

Implement:

- named scheduling classes;
- exact numeric priorities;
- task-level relative-order constraints;
- software dispatcher allocation;
- hardware interrupt inventory;
- reserved/forbidden interrupt policy;
- task capacity validation;
- periodic release metadata;
- shared-resource access report;
- preliminary priority-ceiling analysis inputs.

The builder does not claim WCET or schedulability. It can report:

- declared deadlines;
- periods;
- priorities;
- blocking relationships;
- capacities;
- measured evidence links supplied by external tooling.

Add diagnostics for:

- conflicting relative-order constraints;
- missing dispatcher;
- hardware/software priority confusion;
- task capacity less than declared maximum concurrent spawn count where statically derivable.

---

### M5 — Boot-frozen endpoint routing

Purpose: permit installation-specific protocol assignment without moving physical ownership.

Model:

```text
compiled endpoint instance
    owns hardware permanently

compiled compatible protocol consumers
    are present in application

platform configuration
    selects one allowed logical route in maintenance mode

boot validation
    freezes route before normal operation
```

Builder responsibilities:

- declare allowed routing matrix;
- validate protocol/endpoint compatibility;
- generate route tables and configuration schema fragments;
- ensure all route variants preserve the same endpoint ownership;
- ensure safety-relevant route changes require maintenance mode and reboot;
- ensure invalid route configuration inhibits arming.

Do not implement runtime hot-swapping.

---

### M6 — Observation plane v1.0

Move from small RTIC shared snapshots toward a verified single-writer/multi-reader abstraction where justified.

Required semantics:

- one writer;
- immutable readers;
- timestamp/generation;
- bounded publication;
- no backpressure;
- stale detection;
- reader missed-update semantics;
- no authority.

Add `ObserveEvent<T>` only for real event requirements:

- safety-state transition;
- failsafe reason;
- watchdog;
- queue overflow transition;
- configuration transition.

Event journals require explicit capacity and drop policy.

---

### M7 — Generated release application path

For selected applications:

```text
clean input checkout
    -> resolve
    -> canonical IR
    -> render
    -> rustfmt
    -> cargo check
    -> tests
    -> release build
    -> ELF/report inspection
    -> generated drift check
    -> artifact identity
```

The released binary shall be built from the clean generated output, not from hand-edited source.

Retire a handwritten release app only after:

- static graph comparison;
- host tests;
- target boot;
- props-off complete-drone bench tests;
- timing/waveform evidence;
- fault cases;
- controlled flight reconciliation where applicable;
- explicit migration ADR.

Keep handwritten prototypes for experiments where appropriate.

---

### M8 — STM32H7 backend and Pixhawk-class target

Add STM32H7 without changing portable component semantics.

Builder/model additions:

- memory regions/domains;
- DMA-accessible memory constraints;
- cacheability;
- DMAMUX requests;
- interrupt inventory;
- multiple buses and clocks;
- linker-section placement requests;
- explicit static buffer placement;
- backend capability/version requirements.

Default target: Pixhawk 6C.  
Fallbacks: Pixhawk 6C Mini, CubePilot with partner support, or F405 schedule-protection target.

Acceptance:

- same portable component catalogue above platform contract;
- H7-specific semantics terminate in backend;
- resolved report shows memory placement and DMA compatibility;
- independent ELF/map inspection verifies placement;
- no H7 feature is silently emulated.

---

### M9 — Multi-backend platform conformance experiment

Use one representative IMU SPI/DMA pipeline to compare:

- `stm32f4xx-hal`;
- `embassy-stm32` used below RTIC without an Embassy executor in critical paths;
- future backend if justified.

Keep identical above the platform contract:

- generated component graph;
- task names and priorities;
- request/event types;
- error taxonomy;
- buffer ownership;
- portable driver/service;
- tests.

Compare:

- initialization complexity;
- unsafe inventory;
- binary size;
- static memory;
- timing/jitter;
- abort/recovery;
- API stability;
- maintainability;
- conformance failures.

The builder selects a backend by explicit profile/board compatibility. It does not translate backend semantics at runtime.

---

### M10 — Simulation, replay, and hardware-in-the-loop integration

The builder should export enough IR to instantiate equivalent host shells, but the first simulator must not be blocked on full generation.

Mid-term integration:

- map portable component instances to host implementations;
- use virtual time;
- produce common trace schema;
- inject RC loss, stale IMU, stale request, queue saturation, watchdog, authority revocation;
- compare integer/logical state transitions;
- retain explicit non-equivalence limits;
- keep target timing and waveform tests separate.

Add report fields linking:

- generated component instance;
- host test;
- target test;
- trace ID;
- requirement ID;
- evidence location.

---

### M11 — Semantic graph diff and change-impact input

`cargo xtask app diff` shall distinguish:

```text
identity-only change
component version change
capability edge change
physical resource change
task change
priority change
dispatcher change
capacity change
initialization-order change
Cargo feature/dependency change
board/backend/toolchain change
```

Output:

- human Markdown;
- machine JSON;
- risk tags, not release-approval decisions.

Examples:

```text
HIGH: actuator owner component version changed
HIGH: motor output timer mapping changed
HIGH: safety-state capability provider changed
MEDIUM: serial task priority changed
MEDIUM: queue capacity reduced
LOW: observer component added
INFO: source file formatting changed without semantic IR change
```

FerroWasp may publish technical graph diffs. Proprietary release-approval scoring remains outside the public builder.

---

### M12 — Memory and resource reports

Generate:

- declared static buffers;
- queue capacities;
- estimated static sizes where types are known;
- actual ELF section sizes;
- stack policy inputs;
- memory-region placement;
- DMA accessibility;
- interrupt table;
- timer/DMA ownership;
- Cargo dependency list;
- SBOM input.

Independent post-build inspection must read the ELF/linker map rather than trusting only renderer metadata.

---

### M13 — Component authoring SDK and linting

Provide a contributor workflow:

```text
cargo xtask component new uart-protocol
cargo xtask component lint path/to/component.toml
cargo xtask component test path/to/component.toml
cargo xtask component example path/to/component.toml
```

Authoring guide requires:

- responsibilities;
- implementation Rust paths;
- provides/requires;
- ownership;
- tasks;
- capacities;
- errors;
- timeout/freshness;
- configuration;
- multi-instance behavior;
- host tests;
- target tests;
- maturity.

Do not introduce a procedural macro DSL unless it removes verified boilerplate without hiding architecture. TOML plus Rust paths remains the baseline.

---

### M14 — Configuration schema integration

Builder emits canonical schema fragments for:

- platform configuration;
- live configuration;
- component-owned fields;
- constraints;
- units;
- enums;
- dependencies;
- cross-field rules where representable.

Rules:

- firmware independently validates;
- invalid configuration inhibits arming;
- embedded defaults remain non-flyable;
- platform changes require maintenance mode and reboot;
- live tuning requires disarmed state;
- logs include configuration hashes.

The builder does not become the runtime configurator.

---

### M15 — Controlled release provenance

Add a release manifest containing:

- source commit;
- branch/tag;
- builder version;
- schema versions;
- board hash;
- application profile hash;
- catalogue hash;
- resolved application hash;
- toolchain;
- target triple;
- Cargo lock hash;
- features;
- generated source hash;
- ELF hash;
- binary hash;
- configuration schema IDs;
- test/evidence references.

Public FerroWasp uses this for reproducibility. Proprietary FerroPilot may add signatures, approval, access control, and controlled evidence links.

---

## 17. Mid-term exit criteria

Mid-term is complete when:

1. selected release applications are generated from canonical inputs;
2. multiple board applications share one component model;
3. `ResolvedApplication` is versioned and stable enough for downstream tools;
4. exact tasks, priorities, dispatchers, resources, capabilities, and capacities are reported;
5. generated F405 application is reconciled for its declared scope;
6. an STM32H7 candidate uses the same portable component graph;
7. one platform contract has two backend implementations or a completed comparison;
8. host and target tests exercise the same portable logic;
9. critical fault transitions are reproducible;
10. graph diff identifies safety-relevant composition changes;
11. configuration schemas and provenance are integrated;
12. generated release artifacts require no hand edits;
13. builder limitations remain explicit.

---

# Part III — Long-Term Reference Implementation

## 18. Long-term objective

**Time horizon:** approximately 18 months and beyond.

The long-term objective is:

> Mature the RTIC App Builder into a reusable, transparent composition and evidence foundation shared by FerroWasp and FerroPilot, while retaining static ownership, readable source, public functional completeness, and a strict separation between open technical composition and proprietary assurance governance.

---

## 19. Long-term work packages

### L1 — Compact board authoring and resource solver

Add only after verbose targets are proven.

Pipeline:

```text
compact board authoring definition
    -> target-family resource database
    -> constraint solver
    -> verbose resolved board definition
    -> human review
    -> commit
    -> normal application resolution
```

Solver domains may include:

- pin alternate functions;
- DMA routes;
- timers/channels;
- dispatcher interrupts;
- H7 DMAMUX requests;
- memory domains;
- DMA-accessible regions;
- cacheability;
- optional device placement.

Requirements:

- deterministic;
- explains selected and rejected alternatives;
- supports constraints and preferences separately;
- never hides final mapping;
- never modifies committed resolution silently;
- solver output is reviewable and diffable;
- safety-critical mappings require explicit acceptance.

The builder shall continue accepting verbose board definitions permanently.

---

### L2 — Multi-target and multi-HAL catalogue

Support:

- STM32F4;
- STM32H7;
- host simulator;
- selected future MCU families;
- multiple HAL implementations where justified.

Every backend advertises:

- platform-contract versions;
- supported endpoint kinds;
- resource constraints;
- interrupt rules;
- memory rules;
- unsafe-code boundary;
- conformance evidence references.

Portable functional components remain backend-independent.

---

### L3 — Reusable assurance-facing IR

Keep public `ResolvedApplication` technical and complete. Add a private FerroPilot overlay keyed by stable public IDs.

Public IR may include:

- technical architecture;
- provenance;
- requirements/test IDs if public;
- component maturity;
- resource and timing declarations.

Private overlay may include:

- controlled approval state;
- proprietary qualification status;
- customer configuration;
- evidence completeness;
- release authorization;
- internal scoring;
- known-problem disposition;
- restricted requirements.

The private system must not silently change public semantics. Stricter policy must be expressed through explicit profile inputs and visible generated source.

---

### L4 — Independent output verification

Reduce common-mode failure between resolver, renderer, and reports.

Independent checks:

- parse generated Rust with `syn`;
- inspect RTIC app structure;
- compare generated task/resource declarations against IR;
- inspect Cargo metadata;
- inspect ELF symbols/sections;
- inspect linker map;
- inspect vector table;
- inspect binary identity;
- target waveform/timing tests;
- compare separate graph extraction against resolved IR.

Do not treat reports generated from the same in-memory structures as independent evidence.

---

### L5 — Builder qualification strategy

Only qualify or formally control builder functions where it materially reduces downstream work.

Candidate stable subsets:

- schema validator;
- deterministic resolver;
- authority/resource conflict validators;
- renderer;
- provenance generator;
- graph diff;
- independent output checker.

Before qualification:

- scope must be narrow;
- requirements stable;
- version frozen;
- tests and problem reports mature;
- independent verification exists;
- economic/customer need is real.

Do not attempt to qualify a rapidly changing UI or general solver prematurely.

---

### L6 — Standalone extraction boundary

The builder may move to its own repository only when at least one trigger exists:

- stable public schema and IR;
- independent release cadence;
- external non-FerroWasp users;
- multiple application domains;
- maintenance burden from monorepo coupling;
- separate governance need.

Extraction prerequisites:

- no dependency from builder core to FerroWasp-private types;
- component catalogue interface versioned;
- renderer backend interface explicit;
- test fixtures portable;
- licensing and contribution model clear;
- migration requires no architectural rewrite.

Until then, keep it in the monorepo.

---

### L7 — Additional deterministic-control applications

Potential future users:

- ESC controllers;
- servo controllers;
- robot controllers;
- flight-termination systems;
- static industrial controllers.

Do not generalize FerroWasp prematurely. A serious FOSS project with real requirements should drive extraction/generalization. One-off closed-source derivatives are insufficient justification.

---

### L8 — Long-term developer tooling

Potential integrations:

- FerroConfigurator board/application visualizer;
- Board Builder frontend;
- FerroDebugger generated target awareness;
- simulator scenario generator;
- editor diagnostics;
- architecture browser;
- evidence index;
- SBOM and dependency dashboards;
- controlled release dashboard.

All tools consume the same schemas and IR. None become an alternate composition authority.

---

## 20. Long-term exit criteria

The long-term platform is mature when:

- multiple target families conform to versioned platform contracts;
- verbose mappings remain available and authoritative;
- solver-assisted authoring produces reviewable explicit outputs;
- core component evidence is reusable across approved targets;
- generated source and final binaries are independently checked;
- public FerroWasp and private FerroPilot consume the same functional IR;
- the builder can be extracted without redesign;
- commercial differentiation comes from controlled evidence, support, and responsibility rather than hiding basic functional composition.

---

# Part IV — Verification and Test Strategy

## 21. Test pyramid

### 21.1 Pure unit tests

Cover:

- ID parsing;
- schema normalization;
- catalogue indexing;
- capability compatibility;
- cardinality;
- name allocation;
- resource ownership;
- scheduling constraints;
- dispatcher allocation;
- topological sorting;
- canonicalization;
- hashing;
- graph diff.

### 21.2 Fixture tests

Each diagnostic requires at least one fixture. Prefer small, focused fixtures.

Example:

```text
invalid/
├── cap-ambiguous-provider/
├── cap-authority-inferred/
├── res-dma-conflict/
├── res-pin-conflict/
├── sch-no-dispatcher/
├── sch-cycle/
├── saf-two-actuator-owners/
└── gen-name-collision/
```

Tests assert diagnostic code and key related IDs, not full unstable wording.

### 21.3 Golden IR tests

Store canonical `resolved_application.json` for:

- minimal LED;
- button/LED;
- NUCLEO MSP;
- SBUS input;
- SPI IMU;
- F405 parallel candidate.

Regeneration must be intentional and reviewed.

### 21.4 Generated source compile tests

Compile every reference app. Compilation proves type/build consistency only.

### 21.5 Host component tests

Test actual behavior in implementation crates:

- protocol parser;
- control logic;
- safety state;
- capacity/overflow;
- freshness;
- configuration validation.

The builder test suite shall not duplicate those algorithms.

### 21.6 Target tests

Target tests verify:

- endpoint initialization;
- interrupt ownership;
- DMA transitions;
- buffer behavior;
- peripheral timing;
- safe-state output;
- reset behavior;
- waveform behavior;
- target-specific error recovery.

### 21.7 Integration tests

Complete chains:

```text
UART bytes -> decoder -> state
IMU event -> service -> timestamped sample
intent + sample -> control request
request + safety -> actuator owner
snapshot -> OSD/logger
```

### 21.8 Fault injection

Required cases where applicable:

- malformed input;
- missing capability;
- queue saturation;
- stale sample;
- stale actuation request;
- DMA error;
- endpoint timeout;
- observer overload;
- invalid platform configuration;
- watchdog;
- authority revocation;
- reset.

### 21.9 Determinism tests

Run generation repeatedly:

- same process;
- new process;
- shuffled input file discovery;
- Linux;
- Windows;
- clean checkout.

Compare:

- canonical IR hash;
- generated source after newline normalization;
- reports;
- Cargo manifest;
- provenance.

Toolchain-dependent binary reproducibility is a separate objective and shall not be assumed.

---

## 22. Continuous integration and release gates

### Pull-request gates

Required for builder changes:

- format;
- clippy;
- unit tests;
- schema fixtures;
- catalogue lint;
- deterministic generation;
- generated reference app checks;
- generated drift check;
- no new unsafe code;
- documentation update when schema/API changes.

### Protected-path review

Changes affecting these require explicit review labels or code owners:

```text
authority validators
actuator component metadata
motor-output board bindings
safety-state capabilities
priority allocation
dispatcher allocation
platform configuration schema
generated provenance
golden flight app
```

### Release gates

A builder release requires:

- changelog;
- schema compatibility statement;
- migration tool/tests where needed;
- deterministic fixture set;
- generated reference applications;
- known limitations;
- signed tag if project policy supports it;
- no unexplained generated drift.

---

# Part V — Migration from Current Prototypes

## 23. Migration principles

1. Preserve working prototypes.
2. Extract metadata before extracting behavior.
3. Separate endpoint ownership from functional consumption.
4. Generate parallel applications before replacing handwritten applications.
5. Compare structure before behavior.
6. Compare behavior before flight.
7. Use narrow PRs.
8. Do not combine builder architecture work with unrelated motor, control, or sensor changes.
9. Record every durable decision in ADRs.
10. Keep Codex active-work notes current.

---

## 24. Proposed migration sequence

### Migration A — Existing LED/button generator

- identify current generation logic;
- model component/task/resource metadata;
- render through central IR;
- keep old output as fixture;
- compare generated source;
- remove old specialized renderer after parity.

### Migration B — Existing MSP/USART generator

- split endpoint and consumer;
- define capabilities;
- model DMA buffers/queues;
- model periodic task and monotonic;
- resolve explicit connections;
- render and compile;
- retire renderer-specific dispatcher logic.

### Migration C — SBUS

- reuse UART endpoint;
- add decoder component;
- test multiple UART consumer incompatibility;
- model freshness and failsafe outputs;
- generate input-only F405 app.

### Migration D — IMU

- define SPI/DMA platform contract;
- add endpoint and service components;
- add timestamped sample capability;
- generate bench app;
- validate target.

### Migration E — Parallel flight chain

- extract stable portable control/safety components;
- represent exact resources/tasks;
- generate output-inhibited app;
- compare against golden app;
- progress through controlled bench gates.

---

# Part VI — Codex Execution Protocol

## 25. Mandatory task preamble for Codex

Every significant Codex task shall begin by writing or updating a task record with:

```text
Task ID:
Objective:
Current behavior and evidence:
Required behavior:
Safety constraints:
Protected files/boundaries:
Likely files:
Planned changes:
Acceptance criteria:
Required commands/tests:
Documentation/evidence updates:
Explicit non-goals:
```

Codex shall not infer permission to modify motor mapping, gyro signs, arming, authority, priorities, or safety behavior from a builder-related task.

---

## 26. Codex implementation rules

### Before editing

- inspect current files and tests;
- search for duplicate implementation;
- identify current public APIs;
- identify protected safety boundaries;
- reproduce current command;
- record baseline test result.

### During editing

- make the smallest coherent change;
- preserve deterministic iteration;
- avoid stringly typed internal state where an enum/newtype is practical;
- add tests with each pass;
- do not add silent defaults;
- do not swallow errors;
- do not rewrite rustc diagnostics;
- do not use unsafe code in builder crates;
- do not add arbitrary source-generation hooks;
- keep generated code readable.

### After editing

- run focused tests;
- run workspace tests where practical;
- regenerate reference apps;
- check drift;
- inspect generated source;
- update task record;
- update ADR/schema docs if semantics changed;
- state untested target behavior explicitly.

---

## 27. Recommended first 15 pull requests

### PR-001 — Inventory and baseline

No behavior changes. Documents current builder and protected flight paths.

### PR-002 — Builder model and diagnostics

Adds IDs, diagnostics, source references, and canonical serialization.

### PR-003 — Schema v0.1 and fixtures

Parses board/profile/component definitions with focused invalid fixtures.

### PR-004 — Catalogue loader

Loads exact component versions deterministically.

### PR-005 — Instance expansion and generated names

Adds component instances, template expansion, and collision tests.

### PR-006 — Capability graph v0.1

Supports explicit connections and cardinality; authority inference prohibited.

### PR-007 — Physical claim validator

Rejects pin/DMA/peripheral/interrupt conflicts using explicit endpoint slots.

### PR-008 — Scheduling and dispatcher resolver

Uses profile classes and board-provided dispatcher list.

### PR-009 — `ResolvedApplication` output

Produces canonical JSON and stable hash.

### PR-010 — Central LED/button renderer

Generates and checks the simplest application.

### PR-011 — UART-DMA endpoint catalogue

Represents hardware endpoint independently.

### PR-012 — MSP consumer catalogue and generated app

Generates NUCLEO MSP app and forwards rustc diagnostics.

### PR-013 — Transactional generation

Adds work/committed/failed directories and last-known-good behavior.

### PR-014 — Reports and Mermaid graph

Adds deterministic architecture reports.

### PR-015 — Generated drift CI

Regenerates from clean checkout and rejects unexplained differences.

After PR-015, pause and review architecture before adding F405 flight components.

---

## 28. Task decomposition after first architecture review

Recommended next sequence:

```text
PR-016 explicit F405 board definition
PR-017 SBUS component and host tests
PR-018 generated SBUS bench app
PR-019 SPI/DMA contract model
PR-020 IMU endpoint/service catalogue
PR-021 generated IMU bench app
PR-022 observation snapshot metadata
PR-023 OSD migration to observation input
PR-024 portable safety/authority component metadata
PR-025 output-inhibited generated F405 integration app
PR-026 handwritten/generated architecture comparison report
PR-027 target props-off evidence integration
```

Do not schedule flight replacement as a normal PR. It requires a separate gated migration programme.

---

# Part VII — Acceptance Commands

## 29. Expected command set

The exact package names may differ after repository mapping.

```bash
# Builder unit and fixture tests
cargo test -p rtic-app-model
cargo test -p rtic-app-schema
cargo test -p rtic-app-resolver
cargo test -p rtic-app-validator
cargo test -p rtic-app-render
cargo test -p rtic-app-report

# Catalogue checks
cargo xtask app catalogue validate component_catalogue/

# Validation applications
cargo xtask app generate \
  --board boards/nucleo-f401re.toml \
  --profile application_profiles/nucleo-led.toml \
  --check

cargo xtask app generate \
  --board boards/nucleo-f401re.toml \
  --profile application_profiles/nucleo-msp.toml \
  --check

# Determinism
cargo xtask app generate ... --out /tmp/run-a
cargo xtask app generate ... --out /tmp/run-b
diff -ru /tmp/run-a /tmp/run-b

# F405 candidate
cargo xtask app generate \
  --board boards/foxeer-f405-v2.toml \
  --profile application_profiles/f405-generated-candidate.toml \
  --check

# Semantic graph difference
cargo xtask app diff \
  --old generated/committed/<old>/resolved_application.json \
  --new generated/work/<new>/resolved_application.json

# Workspace checks
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Where all-features compilation is not valid for mutually exclusive embedded targets, replace it with an explicit supported-feature matrix rather than weakening CI silently.

---

# Part VIII — Risks and Controls

## 30. Principal risks

| Risk | Failure mode | Control |
|---|---|---|
| Builder becomes a second flight stack | Behavior duplicated in metadata/renderers | Rust implementation crates own behavior; one central renderer |
| Premature generality | Months spent on solver/DSL | Explicit endpoint slots and exact manifests first |
| Opaque resolution | Users cannot explain generated app | `ResolvedApplication`, reports, `explain`, resolution reasons |
| Nondeterminism | Drift and unverifiable releases | Ordered collections, canonicalization, cross-platform tests |
| Hidden fallback | Wrong hardware or priority selected | Ambiguity is error; verbose mapping committed |
| Common-mode reporting error | Reports repeat resolver mistake | Independent generated-source and ELF checks |
| Golden app regression | Builder refactor breaks flight baseline | Protected path, parallel app, dedicated migration gate |
| Authority leak | Non-safety component reaches motors | Typed capability policy and graph validation |
| Over-crediting compilation | Build success treated as target proof | Explicit evidence levels and target gates |
| Schema churn | Downstream tools break | Versioning, migration tests, delayed 1.0 |
| Crate fragmentation | Excess boilerplate and slow work | Start with logical modules if needed; preserve boundaries |
| HAL leakage | Portable components become target-specific | Platform contract and endpoint boundary |
| Manifest DSL creep | Hidden algorithms and source fragments | Strict declarative schema; reject arbitrary code |
| Too many target combinations | Verification becomes unbounded | Narrow reference matrices and maturity labels |

---

# Part IX — Decisions Still Requiring Explicit ADRs

## 31. Deferred implementation choices

These choices should not block the first vertical slice, but must be decided before their affected phase:

1. Exact builder crate count versus modules inside fewer crates.
2. TOML parser/source-span library.
3. JSON Schema generation approach.
4. Stable hash serialization format.
5. Whether generated apps are workspace members or isolated Cargo projects.
6. Monotonic representation in component metadata.
7. Exact RTIC 2 periodic scheduling pattern used by generated software tasks.
8. How implementation Rust paths are compile-validated before full generation.
9. Whether Cargo dependency versions come from catalogue, workspace dependencies, or a controlled policy file.
10. Exact snapshot implementation.
11. Boot-frozen router location and configuration schema.
12. STM32H7 memory-placement syntax.
13. Public schema compatibility guarantees before `1.0`.
14. Conditions for committing generated sources for every app versus reference/release apps only.
15. Conditions for standalone builder extraction.

Default choices in this plan are conservative and can be superseded only by an accepted ADR.

---

# Part X — Definition of Done

## 32. Builder core

Done when:

- deterministic;
- versioned;
- documented;
- no unsafe code;
- no hidden fallback;
- all expected invalid inputs produce structured diagnostics;
- unit and fixture tests cover resolution rules;
- output is independently inspectable.

## 33. Component definition

Done when:

- stable ID/version;
- implementation paths;
- typed capabilities;
- exact tasks/resources;
- physical claims;
- capacities;
- scheduling requirements;
- failure/overflow/timeout behavior;
- multi-instance behavior;
- host and target test references;
- maturity stated.

## 34. Board definition

Done when:

- exact physical resources;
- source references;
- reserved resources;
- explicit endpoint slots;
- backend compatibility;
- validation passes;
- target evidence scope stated;
- no user tuning or runtime authority policy embedded.

## 35. Resolved application

Done when:

- complete;
- canonical;
- hashed;
- exact tasks/resources/capabilities/connections;
- exact priorities/dispatchers;
- exact physical ownership;
- initialization order;
- toolchain/Cargo plan;
- no unresolved ambiguity;
- architecture validators pass.

## 36. Generated application

Done when:

- readable;
- formatted;
- deterministic;
- compiler-checked;
- no hand edits;
- provenance embedded;
- reports generated;
- target validation appropriate to claim completed.

## 37. Generated flight release path

Done when:

- generated from clean canonical inputs;
- reconciled against golden implementation;
- host/target/HIL tests completed for scope;
- timing/waveform evidence exists;
- configuration identity recorded;
- ELF/binary independently inspected;
- known limitations documented;
- explicit release approval exists.

---

# Part XI — Final Ordered Roadmap

## 38. Near term: 0–6 months

```text
1. Inventory current prototypes and freeze protected baseline.
2. Adopt ADRs and vocabulary.
3. Implement deterministic model and diagnostics.
4. Implement schema v0.1.
5. Implement exact component catalogue.
6. Implement resolver and validators.
7. Implement central renderer.
8. Generate LED/button app.
9. Separate UART-DMA endpoint from MSP consumer.
10. Generate/check NUCLEO MSP app.
11. Add transactional generation and rustc forwarding.
12. Add architecture reports and drift CI.
13. Add explicit F405 board target.
14. Add SBUS vertical slice.
15. Add SPI/DMA IMU vertical slice and first platform contract.
16. Add read-only observation snapshot.
17. Generate output-inhibited parallel F405 candidate.
18. Compare generated and handwritten structures.
```

Primary exit artifact:

> **RTIC App Builder Vertical Slice v1 plus Generated Parallel F405 Candidate**

---

## 39. Mid term: 6–18 months

```text
1. Stabilize ResolvedApplication v1.0.
2. Complete typed capability graph.
3. Add deterministic dependency closure.
4. Add application-wide scheduling and dispatcher allocation.
5. Add boot-frozen endpoint routing.
6. Stabilize observation plane and bounded event journals.
7. Move selected releases to clean generated composition.
8. Integrate configuration schemas and provenance.
9. Add semantic graph diff and independent output checks.
10. Integrate host simulation, replay, fault injection, and HIL.
11. Implement STM32H7 backend and Pixhawk-class target.
12. Compare multiple HAL implementations under one platform contract.
13. Produce memory/resource/ELF reports.
14. Add component authoring SDK and contributor guides.
```

Primary exit artifact:

> **Generated Multi-Target Release Pipeline v1**

---

## 40. Long term: 18+ months

```text
1. Add compact authoring and deterministic resource solver.
2. Expand target-family and backend catalogue.
3. Maintain reusable public ResolvedApplication semantics.
4. Add private FerroPilot assurance overlays without semantic divergence.
5. Independently verify generated source and final ELF/binary.
6. Qualify stable builder subsets only where commercially justified.
7. Extract builder only after stable interfaces and external demand.
8. Support other deterministic-control projects only from real shared requirements.
9. Integrate configurator, debugger, simulator, evidence, and board tooling around one IR.
```

Primary exit artifact:

> **Reusable FerroWasp/FerroPilot Static Application and Evidence Platform**

---

## 41. Final implementation rule

When Codex faces a choice between a broad elegant abstraction and a narrow explicit implementation that advances the first verified vertical slice, choose the narrow explicit implementation unless an accepted ADR says otherwise.

The reference implementation succeeds by making the following visible and reproducible:

```text
what components exist
what each instance owns
what each instance provides and requires
how capabilities are connected
what tasks execute
what priorities and dispatchers are used
what capacities are allocated
what physical resources are claimed
what source was generated
what compiler checked
what target evidence exists
```

It fails if those facts are hidden behind a manifest language, opaque solver, component-specific renderer, runtime ownership transfer, or undocumented fallback.

---

## Appendix A — Minimal canonical `ResolvedApplication` example

```json
{
  "schema_version": "0.1",
  "builder_version": "0.1.0",
  "id": "nucleo-msp-reference-nucleo-f401re-a1b2c3d4e5f6",
  "board": {
    "id": "nucleo-f401re",
    "backend": "ferrowasp-stm32f4"
  },
  "components": [
    {
      "id": "displayport",
      "component": "msp-displayport",
      "version": "0.1.0"
    },
    {
      "id": "uart1",
      "component": "uart-dma-endpoint",
      "version": "0.1.0"
    }
  ],
  "connections": [
    {
      "from": "uart1.rx_bytes",
      "to": "displayport.rx_bytes",
      "class": "critical",
      "reason": "explicit-profile-connection"
    },
    {
      "from": "displayport.tx_frame",
      "to": "uart1.tx_request",
      "class": "request",
      "reason": "explicit-profile-connection"
    }
  ],
  "tasks": [
    {
      "name": "uart1__rx_irq",
      "kind": "hardware",
      "interrupt": "USART1",
      "priority": 8
    },
    {
      "name": "displayport__periodic",
      "kind": "software",
      "dispatcher": "EXTI2",
      "priority": 3,
      "capacity": 1
    }
  ],
  "physical_claims": [
    {
      "resource": "USART1",
      "owner": "uart1"
    },
    {
      "resource": "DMA2_STREAM2_CHANNEL4",
      "owner": "uart1"
    },
    {
      "resource": "DMA2_STREAM7_CHANNEL4",
      "owner": "uart1"
    }
  ]
}
```

---

## Appendix B — Architecture report outline

```markdown
# Resolved Application Summary

## Identity
- Application:
- Board:
- Backend:
- Builder:
- Resolved hash:
- Source commit:
- Toolchain:

## Components
| Instance | Type | Version | Maturity |

## Capabilities
| Provider | Capability | Consumer | Class | Reason |

## Tasks
| Task | Kind | Interrupt/Dispatcher | Priority | Capacity | Component |

## Resources
| Resource | Kind | Owner | Accessors | Capacity |

## Physical ownership
| Peripheral/pin/DMA/timer/interrupt | Owner |

## Initialization order
1.
2.
3.

## Policies
- Authority providers:
- Actuator owner:
- Experimental restrictions:
- Observer restrictions:

## Warnings and limitations

## Build result
- Command:
- Status:
- ELF:
- Binary:
```

---

## Appendix C — Codex task example

```markdown
# Task APP-N6-002: Generate NUCLEO MSP vertical slice

## Objective
Generate and compiler-check the NUCLEO USART1 DMA plus MSP DisplayPort application through the central ResolvedApplication renderer.

## Current behavior and evidence
The repository contains a working prototype with UART DMA ownership, static RX/TX storage, bounded queues, a separate OSD consumer, and periodic software work. Record exact file paths and the current successful command before editing.

## Required behavior
The same application shall be expressed through:
- one explicit board definition;
- one application profile;
- two component definitions;
- one deterministic ResolvedApplication;
- one central renderer.

## Safety constraints
- No motor-output code.
- No authority capabilities.
- No change to the golden flight application.
- No HAL types in the MSP functional component interface.
- No arbitrary source snippets in manifests.

## Acceptance criteria
- Generated source is readable and rustfmt-clean.
- Cargo check succeeds from a clean directory.
- Repeated generation is identical.
- UART physical resources have exactly one owner.
- Capability edges are explicit.
- Failed Cargo check does not replace last-known-good output.
- Reports list tasks, resources, connections, and dispatchers.

## Required tests
- valid generation fixture;
- missing RX capability;
- duplicate DMA claim;
- generated-name collision;
- insufficient dispatcher;
- invalid Rust path preserving rustc diagnostic.

## Documentation
Update implementation inventory, active-work record, component authoring notes, and generated app README.

## Non-goals
- General pin solver.
- F405 support.
- SBUS.
- Runtime routing.
- OSD feature expansion.
```

---

## Appendix D — Review checklist before merging a builder PR

```text
[ ] Scope matches one task.
[ ] Current behavior was reproduced before changes.
[ ] Protected flight files were not modified, or modification was explicitly authorized.
[ ] No behavioral implementation moved into the builder.
[ ] No arbitrary Rust source was added to manifests.
[ ] Deterministic data structures are used.
[ ] New invalid cases have diagnostic fixtures.
[ ] Generated output was inspected, not only compiled.
[ ] Last-known-good output survives failure.
[ ] rustc diagnostics remain intact.
[ ] Schema/IR changes are versioned and documented.
[ ] Reference apps regenerate cleanly.
[ ] Generated drift is understood.
[ ] Safety/authority validators still pass.
[ ] Untested target behavior is stated.
[ ] Active-work and ADR documents are updated where needed.
```

---

## Source basis

This plan is derived from the canonical `FERROWASP_FERROPILOT_MASTER_SOURCE(1).md`, especially its current baseline, target architecture, RTIC App Builder mission and exclusions, roadmap, immediate execution sequence, definitions of done, repository model, and Codex task-handoff rules. The source remains authoritative where this implementation plan is silent. Repository code, tests, live evidence, and newer explicit project decisions take precedence where they conflict.
