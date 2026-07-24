# RTIC App Builder Reference Implementation Plan

## Near-Term, Mid-Term, and Long-Term Implementation Baseline

**Project:** FerroWasp / FerroPilot  
**Primary subsystem:** RTIC App Builder  
**Intended reader:** Codex and human maintainers  
**Document type:** Reference implementation plan  
**Baseline date:** 24 July 2026  
**Classification:** Internal project planning source; review before public publication  
**Status:** Accepted working baseline; public schema remains provisional pending full NUCLEO renderer-contract proof
**Primary implementation horizon:** Immediate work through approximately 24 months, followed by long-term product maturation

> This plan is an implementation specification, not evidence that any described feature is already implemented, flight-ready, certified, or assurance-approved. Current repository code, tests, accepted architecture decisions, and target evidence remain authoritative.

### Status, scope, and precedence

This document is the canonical forward implementation plan for the RTIC App
Builder. It supersedes the roadmap and post-MVP expansion sequence in
`docs/archive/rtic_feature_assembler_mvp_implementation_plan.md`, which
remains a historical record of the first NUCLEO prototype.

Use this precedence when sources disagree:

1. the user's latest explicit architectural or safety decision;
2. accepted ADRs and protected FerroWasp safety decisions;
3. checked-in code, strict input schemas, tests, and recorded target evidence;
4. this reference implementation plan;
5. focused architecture notes linked from this plan;
6. historical plans and dated implementation observations.

The FerroWasp monorepo contains this RTIC App Builder prototype at
`tools/rtic-app-builder`, alongside the protected applications and canonical
FerroWasp crates. The builder remains a nested Cargo workspace and is not a
member of the firmware workspace. The NUCLEO adapter uses the canonical
`crates/ferrowasp-mspv1` protocol crate, while its STM32F401 serial/OSD
compatibility layer remains transitional. Co-location alone does not authorize
replacing that layer or changing flight code. Work that depends on golden
applications or canonical component APIs must record the monorepo commit and
exact paths before implementation begins. An example Rust path in this
document is not evidence that the item currently exists. The current adopted
and deferred crate boundary is recorded in `docs/backend-unification.md`.

Document map:

- `README.md` — current implemented commands and repository orientation;
- `docs/architecture-observations.md` — evidence and lessons from the current
  blinky and UART-DMA/MSP prototypes;
- `docs/stm32f4-backend.md` — current narrow backend contract;
- `docs/osd-usart1-dma.md` — current OSD prototype and pinned provenance;
- `docs/betaflight-target-definition-notes.md` — focused board-capability and
  boot-frozen platform-configuration model;
- `docs/authoring/README.md` — current authoring checklist and handbook entry;
- `docs/chatgpt-project-context.md` — derived, compact upload summary;
- `docs/archive/rtic_feature_assembler_mvp_implementation_plan.md` —
  superseded historical MVP plan.

### Adopted review corrections

The implementation baseline includes these corrections:

- define a constrained renderer-facing invocation and initialization contract
  before freezing schema v0.1;
- keep catalogue files declarative and prohibit arbitrary Rust fragments;
- separate interaction kind, port role, and safety classification;
- assign destructive transports per edge and prohibit implicit queue fan-out;
- use precise SPSC/MPSC/latest-value/journal/direct/service transports;
- model RTIC 2 hardware, divergent consumer, periodic, and delayed one-shot
  task forms without spawn-queue payload assumptions;
- delegate wake-up correctness to tested channel adapters;
- separate build rejection from boot initialization and runtime failures;
- use typed fault catalogues and typed/versioned backend recipe signatures;
- separate physical board facts from backend selection/scheduling policy;
- permit overlapping unselected endpoint alternatives while rejecting
  conflicts in the resolved application;
- keep parsing/rendering outside long RTIC shared locks;
- apply actuator-owner rules according to application safety scope;
- use exact component versions in the near term;
- separate semantic composition identity, exact input identity, and build
  provenance;
- reuse the prototype's tested framed hashing, command-running, locking,
  failure capture, and successful Windows-safe promotion paths while adding
  complete fingerprints, immutable commits, and rollback fault tests;
- make schema examples executable fixtures rather than independently maintained
  pseudo-specifications;
- gate F405 work on pinned access to canonical FerroWasp sources and APIs;
- deliver the NUCLEO builder core as a milestone independent of the later F405
  candidate.

The current NUCLEO implementation is executable evidence for these corrections,
not the final general schema. Strict contracts in `architecture-contracts/`
are validated before rendering. The blinky and USART1 DMA/MSP DisplayPort
applications compile and link with explicit RTIC 2 task forms, bounded
channels, latest-value telemetry snapshots, typed runtime faults, explicit
boot-spawn failure handling, and versioned backend-mechanism declarations.

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
    -> generated parallel, output-inhibited F405 candidate
    -> reconciled generated release path
    -> multiple backends and targets
```

The application designated as golden by the pinned FerroWasp source remains
the runtime reference until a generated application has been statically,
electrically, bench, and flight reconciled. The source material used to draft
this plan named FCU3, but every implementation task must verify the current
designation and commit rather than treating that status as live repository
fact. Builder work must not replace or broadly refactor the verified golden
application incidentally.

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
- experimental components claim resources protected by the selected profile's
  explicit `ProtectedResourcePolicy`;
- a configuration or maintenance component can bypass the safety-owned actuator path.

Maturity alone does not make an interrupt or peripheral protected. The
profile/backend policy explicitly lists protected motor outputs, authority
ports, interrupts, and peripherals. Actuator-owner cardinality is enforced
according to application safety scope, not imposed on validation targets.

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

- complete typed interaction, safety, role, and transport contracts;
- deterministic dependency closure;
- repeated component instances;
- application-wide priority and dispatcher allocation;
- boot-frozen endpoint routing;
- observation snapshots;
- component health metadata;
- generated parallel, output-inhibited F405 candidate;
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

All external inputs and generated artifacts require an explicit
`schema_version`. Leaf IDs use:

```text
[a-z][a-z0-9]*(?:[-_][a-z0-9]+)*
```

Qualified references use a namespace and one or more `/`-separated leaf IDs:

```text
<leaf-id> ":" <leaf-id> ("/" <leaf-id>)*
```

Use qualified references where collision risk exists:

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

Serialized source paths shall be repository-relative logical paths using `/`
separators. Absolute paths, host prefixes, temporary-directory names, and
platform-native separators must not enter canonical artifacts. Source
locations are diagnostic metadata and are excluded from semantic composition
identity; exact labeled input bytes are covered separately by `InputIdentity`.

Do not expose internal panics as user diagnostics. Expected invalid input returns structured diagnostics. Internal invariant violations may panic only in tests; production CLI converts them into a builder-internal failure with a preserved error chain.

### 5.2 Board definition

`BoardDefinition` records physical facts and supported concrete endpoint resources. It must not contain flight algorithms, tuning, or user-selected protocol roles.

```rust
pub struct BoardDefinition {
    pub schema_version: SchemaVersion,
    pub id: BoardId,
    pub family: PlatformFamilyId,
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
    pub unavailable_interrupts: BTreeSet<InterruptId>,
    pub metadata: BoardMetadata,
}

pub struct BoardBackendTarget {
    pub board: BoardId,
    pub backend: BackendId,
    pub endpoint_realizations: BTreeMap<EndpointSlotId, BackendEndpointRealization>,
    pub dispatcher_preference: Vec<InterruptId>,
}
```

Important distinction:

- `endpoint_slots` describe hardware that is compiled and may be bound to a logical endpoint instance.
- Application profiles select endpoint/component instances.
- Platform configuration may later select a boot-frozen protocol role among already compiled compatible roles.
- Runtime configuration never changes physical ownership.
- Board validation checks each endpoint slot internally. Alternative slots may
  overlap pins, peripherals, DMA routes, timer channels, or interrupts because
  they are not simultaneously selected.
- Resolved-application validation rejects conflicts among the selected
  exclusive claims.
- The board records physical interrupt facts. The selected
  `BoardBackendTarget` records deterministic dispatcher preference and
  backend-specific endpoint compatibility.

### 5.3 Application profile

```rust
pub struct ApplicationProfile {
    pub schema_version: SchemaVersion,
    pub id: ApplicationId,
    pub board: BoardId,
    pub backend_target: BoardBackendTargetId,
    pub safety_scope: ApplicationSafetyScope,
    pub components: Vec<ComponentInstanceRequest>,
    pub explicit_connections: Vec<ConnectionRequest>,
    pub scheduling_classes: BTreeMap<SchedulingClassId, Priority>,
    pub capacities: BTreeMap<CapacityId, CapacitySelection>,
    pub timebase: Option<TimebaseRequest>,
    pub policies: ApplicationPolicies,
    pub feature_flags: BTreeSet<FeatureId>,
    pub build_profile: BuildProfileId,
}

pub enum ApplicationSafetyScope {
    Validation,
    Bench,
    Flight,
    Simulator,
}

pub struct CapacitySelection {
    pub value: usize,
    pub rationale: String,
    pub source: Option<SourceRef>,
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

Validation and bench profiles may have no actuator owner. Flight profiles
require exactly one approved actuator owner and authority path, plus declared
freshness and safe-state inputs. The two NUCLEO references are explicitly
`Validation`.

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
    pub capability_ports: Vec<CapabilityPortDefinition>,
    pub tasks: Vec<TaskTemplate>,
    pub resources: Vec<ResourceTemplate>,
    pub physical_claims: Vec<PhysicalClaimTemplate>,
    pub capacities: Vec<CapacityRequirement>,
    pub initialization: InitializationTemplate,
    pub bindings: Vec<BindingDefinition>,
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
    pub constructors: BTreeMap<ConstructorId, RustPath>,
    pub task_entrypoints: BTreeMap<TaskEntrypointId, RustPath>,
    pub cargo_features: BTreeSet<String>,
}
```

`RustPath` is a validated Rust item path, not arbitrary source. Resource
templates use structured, validated `RustTypeTemplate` values. Relative item paths
are resolved against `crate_name` and `module_path` during normalization, so
the resolved application contains only complete, unambiguous paths.

Component definitions also declare every structural binding accepted by an
instance:

```rust
pub struct BindingDefinition {
    pub id: BindingKey,
    pub value_kind: BindingValueKind,
    pub required: bool,
    pub constraints: Vec<BindingConstraint>,
}

pub enum BindingValueKind {
    EndpointSlot { kind: EndpointKindId },
    Capacity { unit: CapacityUnit },
    PhysicalResource { kind: PhysicalResourceKind },
    ComponentInstance,
    Constant { value_type: ScalarType },
}
```

`bind` selects compile-time structure, ownership, or sizing. `configuration`
supplies typed component behavior settings. They are separate namespaces;
unknown, duplicate, missing-required, or wrong-kind entries are errors before
candidate instantiation.

### 5.5 Capability interaction, safety, and transport

Interaction semantics and safety classification are independent dimensions.
A stream may be non-critical OSD traffic or safety-critical RC input; a
request may also be safety-critical. The canonical IR therefore records both:

```rust
pub enum InteractionKind {
    Authority,
    Stream,
    Request,
    Snapshot,
    EventJournal,
    Service,
}

pub enum SafetyClass {
    SafetyCritical,
    SafetyRelated,
    NonCritical,
    ObservationOnly,
}
```

Direction and responsibility are represented separately on each port:

```rust
pub enum CapabilityPortRole {
    GrantAuthority,
    ReceiveAuthority,
    PublishStream,
    ConsumeStream,
    EmitRequest,
    HandleRequest,
    PublishObservation,
    ReadObservation,
    AppendObservationEvent,
    ReadObservationEvent,
    OfferService,
    UseService,
}

pub struct CapabilityPortDefinition {
    pub id: CapabilityPortId,
    pub key: CapabilityKey,
    pub type_arguments: BTreeMap<TypeParameterId, ValueTemplateRef>,
    pub role: CapabilityPortRole,
    pub cardinality: Cardinality,
    pub required: bool,
    pub allowed_transports: BTreeSet<TransportKind>,
}

pub enum TransportKind {
    SpscQueue,
    MpscQueue,
    LatestValue,
    EventJournal,
    SameTaskDirect,
    SynchronousService,
}
```

A capability key shall include:

- interaction kind;
- semantic type ID;
- version;
- optional instance/domain qualifier;

```rust
pub struct CapabilityKey {
    pub interaction: InteractionKind,
    pub type_id: CapabilityTypeId,
    pub version: CapabilityVersion,
    pub domain: Option<String>,
}
```

Semantic capability IDs resolve through a versioned catalogue entry before
task or resource resolution:

```rust
pub struct CapabilityTypeDefinition {
    pub type_id: CapabilityTypeId,
    pub version: CapabilityVersion,
    pub payload: RustTypeTemplate,
    pub parameters: Vec<TypeParameterDefinition>,
    pub allowed_transports: BTreeSet<TransportKind>,
}
```

`RustTypeTemplate` is a validated path plus typed generic/const parameters,
not a format string. Every resolved capability records the fully instantiated
payload type and adapter type. Every `ResolvedConnection` also records its
safety class and an exact transport instance:

```rust
pub struct ResolvedTransport {
    pub id: TransportId,
    pub kind: TransportKind,
    pub producers: Vec<CapabilityPortId>,
    pub consumers: Vec<CapabilityPortId>,
    pub capacity: Option<NonZeroUsize>,
    pub overflow: Option<OverflowPolicy>,
    pub wakeup: WakeupSemantics,
    pub freshness: FreshnessSemantics,
    pub blocking: BlockingSemantics,
    pub fault: Option<FaultId>,
}

pub struct ResolvedConnection {
    pub source: CapabilityPortId,
    pub destination: CapabilityPortId,
    pub interaction: InteractionKind,
    pub safety: SafetyClass,
    pub transport: TransportId,
}
```

Transport topology is explicit. One SPSC queue has exactly one producer and
one consumer. One MPSC queue has one consumer. Destructive queues never imply
fan-out: multiple consumers require one transport per connection, an explicit
multicast component, a latest-value snapshot, or a journal with independent
reader cursors. `SameTaskDirect` is legal only when both endpoints execute in
the same task invocation and require no RTIC lock or blocking operation.

Each transport definition states producer and consumer cardinality, ownership,
capacity, overflow, wake-up, freshness, blocking, execution-context
restrictions, and fault propagation. Queue capacity belongs to the transport,
not an RTIC software-task spawn queue.

The resolver must never infer an authority connection solely because types
match. Authority edges must be explicitly declared or introduced by a narrowly
defined core policy that is visible in the resolved graph.

For the implemented NUCLEO OSD slice:

- USART1 IDLE and RX-DMA interrupt tasks are two producers of one MPSC work
  channel consumed by the divergent OSD task;
- the OSD task is the sole producer and the divergent TX worker is the sole
  consumer of an SPSC TX channel;
- the TX-DMA interrupt task and TX worker use a separate SPSC completion
  channel;
- telemetry is a latest-value snapshot, copied under a short RTIC lock before
  parsing or rendering;
- every channel has an explicit bounded capacity, overflow policy, wake-up
  semantics, and typed fault.

Schema fields may use concise names such as `emits`, `handles`, `publishes`, or
`reads`, but they must deserialize into the explicit port roles above.
Generic `provides`/`requires` wording must not be used for `Request<T>` or
`Authority<T>` because it obscures responsibility.

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
    pub invocation: TaskInvocationTemplate,
    pub scheduling: SchedulingRequirement,
    pub local_resources: Vec<ResourceTemplateId>,
    pub shared_resources: Vec<ResourceAccessTemplate>,
    pub capability_bindings: Vec<TaskCapabilityBinding>,
    pub lock_groups: Vec<LockGroupTemplate>,
    pub spawn_inputs: Vec<SpawnInput>,
    pub execution: TaskExecutionTemplate,
}

pub enum TaskExecutionTemplate {
    HardwareRunToCompletion,
    AsyncOneShot,
    AsyncDivergentConsumer { receive_port: CapabilityPortId },
    AsyncPeriodic { schedule: PeriodicSchedule },
    AsyncDelayedOneShot { delay: DurationRequirement },
}

pub struct PeriodicSchedule {
    pub period: DurationRequirement,
    pub phase: Option<DurationRequirement>,
    pub mode: PeriodicMode,
    pub missed_release: MissedReleasePolicy,
}

pub enum PeriodicMode {
    FixedRate,
    FixedDelay,
}

pub enum MissedReleasePolicy {
    SkipToNext,
    RunOnceAndRebase,
    RecordFaultAndContinue,
    RecordFaultAndStop,
}

pub struct TaskCapabilityBinding {
    pub port: CapabilityPortId,
    pub transport: TransportId,
    pub borrow_scope: BorrowScope,
}

pub enum BorrowScope {
    Invocation,
    PreInvokeCopy,
    PostInvokeCommit,
}

pub struct LockGroupTemplate {
    pub id: LockGroupId,
    pub members: Vec<LockMemberTemplate>,
}

pub enum LockMemberTemplate {
    SharedResource(ResourceTemplateId),
    CapabilityPort(CapabilityPortId),
}
```

Hardware tasks run to completion. Divergent consumers are spawned exactly
once during initialization and await their bounded transport in a loop;
producers send data through the transport rather than respawning an active
software task. Periodic tasks define fixed-rate versus fixed-delay timing,
initial phase, and missed-release behavior. Delayed one-shot tasks model
debounce and similar work without borrowing RTIC 1 spawn-queue semantics.
Lock acquisition, argument borrowing, fixed outcome handling, and spawning are
generated from typed metadata:

```rust
pub struct TaskInvocationTemplate {
    pub entrypoint: TaskEntrypointId,
    pub arguments: Vec<InvocationArgumentTemplate>,
    pub outcome_actions: Vec<OutcomeActionTemplate>,
}

pub enum InvocationArgumentTemplate {
    LocalResource(ResourceTemplateId),
    SharedResource(ResourceTemplateId),
    CapabilityPort(CapabilityPortId),
    SpawnInput(SpawnInputId),
    Configuration(ConfigKey),
    Constant(ConstantId),
}

pub enum OutcomeActionTemplate {
    SpawnOn {
        outcome: OutcomeVariantId,
        task: TaskTemplateId,
    },
    RecordFaultOn {
        outcome: OutcomeVariantId,
        fault: FaultId,
    },
    SendPortOn {
        outcome: OutcomeVariantId,
        port: CapabilityPortId,
    },
    Ignore {
        outcome: OutcomeVariantId,
        rationale: String,
    },
}

pub struct ResolvedTaskInvocation {
    pub function: RustPath,
    pub arguments: Vec<ResolvedInvocationArgument>,
    pub lock_groups: Vec<ResolvedLockGroup>,
    pub outcome_actions: Vec<ResolvedOutcomeAction>,
}
```

Entrypoints return ordinary typed values or component-defined outcome enums.
The renderer may match a declared outcome and perform one of the bounded
actions above. Catalogue metadata cannot contain expressions, statements,
closure bodies, or arbitrary Rust.

The external schema serializes invocation arguments as tagged records in
their call order. A task's declared local/shared resources, capability
bindings, lock groups, and spawn inputs must exactly cover the references used
by its invocation; unused or undeclared references are schema errors.

The renderer must not synthesize a queue-plus-pending wake protocol.
Wake-up correctness belongs to a pinned, tested channel or adapter
implementation. The NUCLEO reference uses divergent `rtic-sync` channel
receivers; hardware tasks use nonblocking `try_send`, and channel closure or
overflow is converted to a typed runtime fault. This removes the
enqueue/drain/pending-clear lost-wakeup race from generated code.

Each lock group is ordered and rendered as one RTIC tuple lock. Every mutable
shared-resource or capability-adapter argument belongs to exactly one group
unless its validated type supports independent access. Invocation-wide locks
are permitted only for facade methods whose bounded critical-section duration
is part of their contract. Prefer `PreInvokeCopy` and `PostInvokeCommit`;
parsing, protocol handling, and rendering must occur outside RTIC shared
locks. A `read` declaration is not assumed lock-free: the resolved access is
classified as immutable, RTIC-shared mutable, copied snapshot, or a verified
lock-free primitive.

The resolved task contains exact:

- generated name;
- task kind;
- hardware interrupt if applicable;
- numeric priority;
- dispatcher if software task;
- local/shared resource names;
- period;
- spawn edges;
- resolved implementation entrypoint and ordered arguments;
- exact local/shared/capability access and lock grouping;
- bounded outcome-to-spawn/fault handling;
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
pub struct ResourceTemplate {
    pub id: ResourceTemplateId,
    pub storage: ResourceStorageKind,
    pub placement: ResourcePlacement,
}

pub enum ResourceStorageKind {
    Physical(PhysicalResourceId),
    TypedValue {
        rust_type: RustTypeTemplate,
    },
    StaticBuffer {
        element_type: RustTypeTemplate,
        length: SizeTemplate,
        count: SizeTemplate,
    },
    StaticQueue {
        item_type: RustTypeTemplate,
        capacity: CapacityRequirement,
        overflow: OverflowContract,
    },
    Snapshot {
        value_type: RustTypeTemplate,
    },
    Constant {
        value_type: RustTypeTemplate,
        value: ConstantTemplate,
    },
}

pub enum ResourcePlacement {
    RticLocal { owner: TaskTemplateId },
    RticShared,
    Immutable,
    InitializationOnly,
}
```

The external TOML flattens these tagged variants into fields such as
`storage`, `rust_type`, `element_rust_type`, `size_from_binding`,
`item_rust_type`, typed generic/const-argument bindings,
`capacity_from_binding`, and `owner_task`; normalization must populate the
structured variants above and reject fields that do not belong to the
selected variant.

Storage shape and RTIC access placement are independent. For example, a
bounded static queue used by several tasks has `StaticQueue` storage and
`RticShared` placement; it is not forced to choose one concept as its
`kind`. A resolved physical resource has exactly one owner. Shared logical
resources may have several task accesses, but exactly one generated storage
definition.

Every resource definition must state:

- its generated storage class;
- its owner component instance;
- all task readers/writers;
- whether access is local, immutable, or RTIC-shared;
- exact capacity where applicable;
- capacity source and sizing rationale;
- overflow policy and fault/health propagation;
- initialization producer and consumers.

The UART-DMA vertical slice therefore models RX-DMA state as shared by the
USART-IDLE and RX-DMA interrupt tasks, TX-DMA state as shared by the TX-DMA
interrupt and TX-service tasks, and queue handles according to their actual
task access. It must not describe one broad endpoint as task-local when
several tasks access it.

### 5.8.1 Constrained renderer-facing construction contract

`ResolvedApplication` must contain enough structure to render without
consulting component definitions again and without recognizing component IDs.
Component definitions express initialization through the same closed
vocabulary, but refer to catalogue-local constructor IDs and unresolved
bindings:

```rust
pub struct InitializationTemplate {
    pub operations: Vec<InitializationOperationTemplate>,
}

pub enum InitializationOperationTemplate {
    BackendPrepare {
        recipe: BackendRecipeId,
        inputs: Vec<ValueTemplateRef>,
        outputs: Vec<ResourceTemplateId>,
    },
    CallConstructor {
        constructor: ConstructorId,
        arguments: Vec<ValueTemplateRef>,
        outputs: Vec<ResourceTemplateId>,
    },
    ConstructResource {
        resource: ResourceTemplateId,
        constructor: ConstructorKind,
    },
    SpawnTask {
        task: TaskTemplateId,
        inputs: Vec<ValueTemplateRef>,
    },
}
```

Normalization validates every referenced constructor against
`ImplementationReference`, resolves bindings and generated resource names,
and produces a deliberately small, versioned operation set:

Every intermediate token passed between template operations is declared as a
resource with `InitializationOnly` placement. Template outputs therefore use
`ResourceTemplateId`; resolution maps them to general `ResolvedValueId`
values without introducing hidden backend temporaries.

```rust
pub enum ResolvedInitOperation {
    BackendPrepare {
        recipe: BackendRecipeId,
        version: BackendRecipeVersion,
        inputs: Vec<ResolvedValueRef>,
        outputs: Vec<ResolvedValueId>,
    },
    CallConstructor {
        function: RustPath,
        arguments: Vec<ResolvedValueRef>,
        outputs: Vec<ResolvedValueId>,
        return_contract: ConstructorReturnContract,
        failure: Option<BootInitializationFailure>,
    },
    ConstructValue {
        rust_type: ResolvedRustType,
        constructor: ConstructorKind,
        output: ResolvedValueId,
    },
    SpawnTask {
        task: ResolvedTaskId,
        inputs: Vec<ResolvedValueRef>,
    },
}
```

Backend recipes have typed, versioned signatures and are restricted to
backend mechanisms:

```rust
pub struct BackendRecipeDefinition {
    pub id: BackendRecipeId,
    pub version: BackendRecipeVersion,
    pub mechanism: BackendMechanism,
    pub inputs: Vec<TypedRecipeSlot>,
    pub outputs: Vec<TypedRecipeSlot>,
    pub physical_claims: Vec<PhysicalClaimKind>,
}
```

Clock preparation, GPIO-bank splitting, physical UART/DMA construction,
monotonic construction, DMA-accessible storage, and interrupt/peripheral
configuration are backend mechanisms. Generic queues, snapshots, journals,
fault storage, and constants use closed backend-independent operations. A
recipe ID is never an untyped escape hatch.

Static resolution/generation/compilation failures and boot failures are
different phases:

```rust
pub enum BuildFailurePolicy {
    RejectResolution,
    RejectGeneration,
    RejectCompilation,
}

pub enum BootInitializationFailurePolicy {
    EnterMaintenanceMode,
    InhibitArming,
    RecordFaultAndDegrade,
    Reset,
    Panic,
}
```

Every fallible constructor records its exact return shape, error conversion,
fault sink, partial-initialization behavior, continuation policy, and arming
effect. Infallible initialization spawns in the NUCLEO validation apps use an
explicit panic/halt if the declared once-at-boot invariant is violated;
runtime queue, DMA, and delayed-spawn failures are recorded through typed fault
state instead.

Rules:

- `BackendRecipeId` and `BackendRecipeVersion` select a typed signature
  implemented and tested by the selected platform backend. The resolver
  validates all input/output slots and physical claims before rendering. It is
  not a Rust snippet.
- Adding a recipe requires a backend contract change, renderer support,
  focused tests, and a schema/compatibility decision.
- `CallConstructor` calls a validated normal Rust item using only resolved
  arguments and records all returned values. Its `function` path must come
  from the component's validated constructor map, and its return/failure
  contract must be complete.
- `ConstructorKind` is a closed set for mechanically constructible values,
  such as `Default` or a zeroed fixed-size array where the resolved type makes
  that operation valid. Named associated or free constructors use
  `CallConstructor`. Neither form can contain source text.
- `ResolvedRustType` contains the fully instantiated path, generic/const
  arguments, array lengths/counts, and storage shape required to emit a
  concrete type; it contains no unresolved binding or format string.
- Task wrappers are rendered from `ResolvedTask`, its
  `ResolvedTaskInvocation`, resolved resource/capability accesses, lock groups,
  and resolved outcome actions. No template IDs remain at render time.
- Imports and Cargo dependencies come from the resolved Cargo plan.
- Component definitions and application profiles never carry arbitrary
  imports, item bodies, statements, or template fragments.

Before schema v0.1 is frozen, a renderer-contract ADR must demonstrate that
this operation set can represent the complete current blinky and NUCLEO OSD
prototypes. The latter includes the arm-button/debounce tasks, shared
`OsdTelemetryState`, and telemetry input in addition to the extracted
UART-DMA/MSP core shown in section 6. If the operation set cannot represent
that topology, extend the closed structural model or improve the
implementation-crate facade; do not add an escape hatch for arbitrary Rust.

### 5.8.2 Fault model

Fault IDs resolve through a typed catalogue; renderer actions never operate on
an unvalidated string:

```rust
pub struct FaultDefinition {
    pub id: FaultId,
    pub severity: FaultSeverity,
    pub latching: bool,
    pub arming_effect: ArmingEffect,
    pub payload_type: Option<ResolvedRustType>,
}

pub struct ResolvedFaultSink {
    pub owner: ComponentInstanceId,
    pub storage: ResolvedResourceId,
    pub saturation: FaultSaturationPolicy,
}
```

The catalogue declares ownership, severity, transient/latching semantics,
saturation behavior, and arming effect. The NUCLEO applications are
`Validation` scope, so their faults have no actuator authority or arming
effect. The OSD reference implements typed counters for RX/TX DMA errors,
channel overflow/closure, TX completion failures, and debounce spawn failure.
The blinky reference records debounce spawn failure in bounded shared state.

### 5.9 Scheduling and dispatchers

Profiles use named scheduling classes. The resolver records numeric priorities.

```rust
pub struct TimebaseRequest {
    pub tick_hz: NonZeroU32,
}

pub struct ResolvedTimebase {
    pub backend_recipe: BackendRecipeId,
    pub generated_name: GeneratedSymbol,
    pub tick_hz: NonZeroU32,
    pub clock_hz: NonZeroU32,
    pub cargo: ResolvedCargoContribution,
}

pub struct SchedulingRequirement {
    pub class: SchedulingClassId,
    pub relation: Vec<PriorityRelation>,
    pub must_be_hardware: bool,
}
```

The backend timebase recipe is a closed renderer/backend contract. For the
NUCLEO slice it accounts for the `systick_monotonic!` declaration, imports and
Cargo contribution, and the `Mono::start` initialization step. The resolver
adds the corresponding `BackendPrepare` operation before component
initialization. Periodic execution is invalid without exactly one compatible
resolved timebase.

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
    pub semantic_identity: SemanticIdentity,
    pub board: ResolvedBoardIdentity,
    pub backend: ResolvedBackendIdentity,
    pub build_policy: ResolvedBuildPolicy,
    pub timebase: Option<ResolvedTimebase>,
    pub components: Vec<ResolvedComponentInstance>,
    pub capabilities: Vec<ResolvedCapability>,
    pub connections: Vec<ResolvedConnection>,
    pub tasks: Vec<ResolvedTask>,
    pub resources: Vec<ResolvedResource>,
    pub physical_claims: Vec<ResolvedPhysicalClaim>,
    pub dispatchers: Vec<ResolvedDispatcher>,
    pub initialization_order: Vec<InitializationStep>,
    pub initialization_operations: Vec<ResolvedInitOperation>,
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
- semantic hash computed from the canonical resolved semantics;
- exact labeled input hashes recorded separately;
- builder version and input hashes recorded;
- no target-runtime values that are only known after execution.

`BuildProvenance` is emitted by the CLI after formatting/checking/building. It
records the semantic identity, exact input identity, actual toolchain identity,
implementation-source/content identities, sanitized build environment, host,
target, command lines and environment overrides, Cargo lock identity, and
produced artifacts. It is not embedded in the semantic hash and is not an
input to rendering.

### 5.11 Relationship to runtime platform configuration

The focused platform-configuration note uses four artifacts that are
complementary, not competing:

| Artifact | Lifecycle | Purpose |
|---|---|---|
| `BoardDefinition` | builder input | Authoritative physical board facts and explicit endpoint slots |
| `ResolvedApplication` | compile-time IR | Exact component, task, resource, ownership, connection, and build composition |
| `BoardCapabilities` | generated read-only runtime projection | Stable target/capability identity and the finite set of compiled endpoints and roles |
| `PlatformConfigV1` | persisted candidate | Complete user-selected assignment among compiled compatible roles |
| `ActivePlatformConfig` | one boot | Validated, immutable routing/configuration selected before endpoints are enabled |

`BoardCapabilities` is generated from `ResolvedApplication`; it is not another
board-authoring schema. `PlatformConfigV1` cannot add code or transfer hardware
ownership. Boot validation produces `ActivePlatformConfig`, which remains
frozen until reset. Exact platform-config schema and recovery policy remain
subject to their focused ADRs and do not block the first static NUCLEO slice.

---

## 6. Input schemas and examples

The Rust types in section 5 describe the normalized semantic model; the TOML
below is the external serialization. The schema uses these explicit mappings:

| TOML form | Normalized field |
|---|---|
| `[[connections]]` | `ApplicationProfile.explicit_connections` |
| `execution` plus schedule/receive fields | closed `TaskExecutionTemplate` with exact channel or timing semantics |
| flattened resource shape/placement fields | tagged `ResourceStorageKind` and `ResourcePlacement` |
| relative implementation item path | complete validated `RustPath` |

Only `SourceRef` spans, explicitly optional scalar values, and collections
marked `default_empty` by schema may be omitted in authoring input.
`default_empty` is limited to semantically neutral collections such as no
local resources, no spawn inputs, no constructor entries, or no Cargo
features; normalization always serializes them explicitly in canonical
artifacts. Application safety policies, component failure contracts,
transport overflow behavior, required bindings, and initialization producers
never receive silent defaults.

Until the provisional public schema exists, the only normative NUCLEO
concurrency examples are the checked files in `architecture-contracts/` plus
their rendered applications and tests. The longer TOML listings below are
non-normative schema design sketches. At schema implementation time they must
be generated from or byte-checked against fixture files; fields that cannot be
represented by the checked transport/task contracts are rejected rather than
grandfathered from these sketches.

### 6.1 Board definition example

```toml
schema_version = "0.1"
reserved_resources = []
unavailable_interrupts = []
timers = {}
fitted_devices = {}
metadata = {}

[board]
id = "nucleo-f401re"
family = "stm32f4"
mcu = "STM32F401"
revision = "re"

[clock]
source = "hsi"
source_hz = 16_000_000
sysclk_hz = 84_000_000

[memory_regions.flash]
kind = "flash"
origin = 0x08000000
length_bytes = 524288

[memory_regions.ram]
kind = "ram"
origin = 0x20000000
length_bytes = 98304

[pins.pa9]
package_pin = "PA9"
capabilities = ["usart1-tx-af7"]

[pins.pa10]
package_pin = "PA10"
capabilities = ["usart1-rx-af7"]

[peripherals.usart1]
kind = "uart"
hardware = "USART1"

[peripherals.dma2]
kind = "dma-controller"
hardware = "DMA2"

[interrupts.usart1]
vector = "USART1"

[interrupts.dma2_stream5]
vector = "DMA2_STREAM5"

[interrupts.dma2_stream7]
vector = "DMA2_STREAM7"

[interrupts.exti0]
vector = "EXTI0"

[interrupts.exti1]
vector = "EXTI1"

[interrupts.exti2]
vector = "EXTI2"

[interrupts.exti3]
vector = "EXTI3"

[dma_routes.usart1_rx]
controller = "dma2"
stream = 5
channel = 4
interrupt = "dma2_stream5"

[dma_routes.usart1_tx]
controller = "dma2"
stream = 7
channel = 4
interrupt = "dma2_stream7"

[endpoint_slots.uart1]
kind = "uart-dma"
peripheral = "usart1"
rx_pin = "pa10"
tx_pin = "pa9"
rx_interrupt = "usart1"
dma_rx = "usart1_rx"
dma_tx = "usart1_tx"

[backend_target]
board = "nucleo-f401re"
backend = "ferrowasp-stm32f4"
dispatcher_preference = ["exti0", "exti1", "exti2", "exti3"]
```

Validation rules include:

- referenced pins/peripherals/routes exist;
- each endpoint alternative is internally valid; unselected alternatives may
  overlap physical resources;
- selected resolved endpoint claims are unique unless explicitly shareable;
- timer and DMA routes are supported by backend metadata;
- RX/TX direction is derived from the typed `dma_rx`/`dma_tx` endpoint role
  and is not repeated as an independently editable board fact;
- dispatcher interrupts are not reserved or claimed by hardware tasks;
- capacities are within backend limits;
- fitted device bindings are compatible with endpoint slot type.

This example intentionally matches the implemented NUCLEO route: USART1 RX is
DMA2 stream 5 channel 4, not the valid-but-different STM32F405 stream 2 route.
F405 examples must live in separate fixtures and identify an F405 board.

### 6.2 Application profile example

```toml
schema_version = "0.1"
feature_flags = []

[policies]
allow_authority_inference = false
allow_experimental = true
warnings_as_errors = true

[application]
id = "nucleo-msp-core"
board = "nucleo-f401re"
backend_target = "nucleo-f401re/ferrowasp-stm32f4"
safety_scope = "validation"
build_profile = "dev"

[timebase]
tick_hz = 1_000

[scheduling_classes]
serial-hardware = 4
serial-service = 3
observer = 2
background = 1

[capacities.serial_chunk_bytes]
value = 70
rationale = "Current MSPv1 compatibility transport maximum"

[capacities.uart_rx_buffers]
value = 2
rationale = "Active plus spare RX DMA buffer"

[capacities.uart_rx_queue]
value = 4
rationale = "Current prototype configuration; traffic bound not yet justified"

[capacities.uart_tx_buffers]
value = 1
rationale = "One active TX DMA transfer"

[capacities.uart_tx_queue]
value = 16
rationale = "Current prototype configuration; traffic bound not yet justified"

[capacities.tx_completion_queue]
value = 1
rationale = "One completion for the single active TX DMA transfer"

[[components]]
id = "uart1"
component = "usart1-dma-endpoint"
version = "0.1.0"
enabled = true

[components.bind]
endpoint_slot = "uart1"
rx_buffer_bytes = "serial_chunk_bytes"
rx_buffer_count = "uart_rx_buffers"
rx_queue_capacity = "uart_rx_queue"
tx_buffer_bytes = "serial_chunk_bytes"
tx_buffer_count = "uart_tx_buffers"
tx_queue_capacity = "uart_tx_queue"

[components.configuration]
serial_profile = "msp_displayport"

[[components]]
id = "displayport"
component = "msp-displayport"
version = "0.1.0"
enabled = true

[components.bind]
frame_bytes = "serial_chunk_bytes"

[components.configuration]
refresh_period_us = 100_000

[[connections]]
from = "uart1.rx_chunks"
to = "displayport.rx_chunks"

[[connections]]
from = "displayport.tx_frames"
to = "uart1.tx_frames"
```

This is the extracted two-component serial/MSP core, not the complete current
`applications/nucleo-f401re-osd.toml` composition. The full migration parity
fixture also models `button_arm_toggle`, debounce/interrupt work, the
display-state writer, shared `OsdTelemetryState`, and the DisplayPort
component's telemetry read port. Omitting those from this focused schema
example must not be reported as full current-app parity.

For compile-smoke purposes, the core component explicitly constructs a fixed
default `OsdTelemetryState`; it does not pretend that this is a connected
observation source. The full-parity catalogue version replaces that fixture
resource with a required `ReadObservation` port connected to the demo writer,
and later flight-relevant profiles connect authoritative observation
publishers.

The explicit `serial_profile = "msp_displayport"` is likewise a transitional
static selection for this development fixture. Once boot-frozen routing is
implemented, `ActivePlatformConfig` supplies that validated named profile;
neither the board definition nor a hidden backend default chooses it.

### 6.3 Component definition example: UART-DMA endpoint

The fixture catalogue first defines the payload shared by the two component
files. MSP behavior already comes from the canonical protocol crate, but the
serial payload path remains in the provenance-pinned F401 adapter until a
compatible canonical facade replaces it:

```toml
schema_version = "0.1"

[[capability_types]]
id = "serial-rx-chunk"
version = "1"
rust_type_path = "ferrowasp_serial_osd_compat::SerialChunk"
allowed_transports = ["spsc-queue", "mpsc-queue"]

[[capability_types.parameters]]
id = "frame_bytes"
kind = "const-usize"

[[capability_types]]
id = "serial-tx-chunk"
version = "1"
rust_type_path = "ferrowasp_serial_osd_compat::SerialChunk"
allowed_transports = ["spsc-queue"]

[[capability_types.parameters]]
id = "frame_bytes"
kind = "const-usize"
```

The checked-in compatibility crate now separates concrete USART1 RX/TX DMA
mechanisms from OSD work processing, typed faults, and transport ownership.
The complete generated NUCLEO application compiler-checks those real APIs.
The future catalogue-shaped `builder_facade` entrypoints named below do not
yet exist; add thin behavior-preserving wrappers before these provisional
component definitions enter the compile-smoke-tested catalogue. Schema-only
fixtures may parse earlier, but must be labelled non-compilable until that
entry condition is met.

```toml
schema_version = "0.1"
capacities = []

[component]
id = "usart1-dma-endpoint"
version = "0.1.0"
maturity = "experimental"
multiplicity = "one"

[implementation]
crate = "ferrowasp-serial-osd-compat"
module = "builder_facade::uart_dma"

[implementation.task_entrypoints]
rx_idle = "on_idle_interrupt"
rx_dma = "on_rx_dma_interrupt"
tx_dma = "on_tx_dma_interrupt"
tx_service = "service_tx"

[[bindings]]
id = "endpoint_slot"
kind = "endpoint-slot"
endpoint_kind = "uart-dma"
required = true

[[bindings]]
id = "rx_buffer_bytes"
kind = "capacity"
unit = "bytes"
required = true

[[bindings]]
id = "rx_buffer_count"
kind = "capacity"
unit = "items"
required = true

[[bindings]]
id = "rx_queue_capacity"
kind = "capacity"
unit = "items"
required = true

[[bindings]]
id = "tx_buffer_bytes"
kind = "capacity"
unit = "bytes"
required = true

[[bindings]]
id = "tx_buffer_count"
kind = "capacity"
unit = "items"
required = true

[[bindings]]
id = "tx_queue_capacity"
kind = "capacity"
unit = "items"
required = true

[[configuration]]
id = "serial_profile"
value_type = "enum"
allowed = ["msp_displayport"]
required = true

[[capability_ports]]
id = "rx_chunks"
interaction = "stream"
safety = "non-critical"
role = "publish-stream"
type = "serial-rx-chunk"
version = "1"
type_arguments = { frame_bytes = "rx_buffer_bytes" }
cardinality = "one-or-more"
required = true
allowed_transports = ["mpsc-queue"]

[[capability_ports]]
id = "tx_frames"
interaction = "request"
safety = "non-critical"
role = "handle-request"
type = "serial-tx-chunk"
version = "1"
type_arguments = { frame_bytes = "tx_buffer_bytes" }
cardinality = "zero-or-more"
required = false
allowed_transports = ["spsc-queue"]

[[tasks]]
id = "rx_idle"
kind = "hardware"
execution = "hardware-run-to-completion"
interrupt_from_binding = "endpoint_slot.rx_interrupt"
scheduling_class = "serial-hardware"
shared_resources = [
  { id = "rx_dma_state", access = "exclusive" },
]
capability_bindings = [
  { port = "rx_chunks", transport = "connection:rx_chunks", borrow_scope = "post-invoke-commit" },
]
lock_groups = [
  { id = "rx_endpoint", members = ["shared:rx_dma_state"] },
]

[tasks.invocation]
entrypoint = "rx_idle"
arguments = [
  { kind = "shared-resource", id = "rx_dma_state" },
]
outcome_actions = [
  { kind = "send-port-on", outcome = "chunk", port = "rx_chunks" },
]

[[tasks]]
id = "rx_dma"
kind = "hardware"
execution = "hardware-run-to-completion"
interrupt_from_binding = "endpoint_slot.dma_rx.interrupt"
scheduling_class = "serial-hardware"
shared_resources = [
  { id = "rx_dma_state", access = "exclusive" },
]
capability_bindings = [
  { port = "rx_chunks", transport = "connection:rx_chunks", borrow_scope = "post-invoke-commit" },
]
lock_groups = [
  { id = "rx_endpoint", members = ["shared:rx_dma_state"] },
]

[tasks.invocation]
entrypoint = "rx_dma"
arguments = [
  { kind = "shared-resource", id = "rx_dma_state" },
]
outcome_actions = [
  { kind = "send-port-on", outcome = "chunk", port = "rx_chunks" },
]

[[tasks]]
id = "tx_dma"
kind = "hardware"
execution = "hardware-run-to-completion"
interrupt_from_binding = "endpoint_slot.dma_tx.interrupt"
scheduling_class = "serial-hardware"
shared_resources = [
  { id = "tx_dma_state", access = "exclusive" },
]
capability_bindings = []
lock_groups = [
  { id = "tx_endpoint", members = ["shared:tx_dma_state"] },
]

[tasks.invocation]
entrypoint = "tx_dma"
arguments = [
  { kind = "shared-resource", id = "tx_dma_state" },
]
outcome_actions = []

[[tasks]]
id = "tx_service"
kind = "software"
execution = "async-divergent-consumer"
scheduling_class = "serial-service"
shared_resources = [
  { id = "tx_dma_state", access = "exclusive" },
]
capability_bindings = [
  { port = "tx_frames", transport = "connection:tx_frames", borrow_scope = "pre-invoke-copy" },
]
lock_groups = [
  { id = "tx_endpoint", members = ["shared:tx_dma_state"] },
]

[tasks.invocation]
entrypoint = "tx_service"
arguments = [
  { kind = "shared-resource", id = "tx_dma_state" },
  { kind = "capability-port", id = "tx_frames" },
]
outcome_actions = []

[[resources]]
id = "rx_dma_state"
storage = "typed-value"
placement = "rtic-shared"
rust_type = "ferrowasp_serial_osd_compat::Usart1RxDma"
rust_type_const_arguments_from_bindings = ["rx_buffer_bytes"]

[[resources]]
id = "tx_dma_state"
storage = "typed-value"
placement = "rtic-shared"
rust_type = "ferrowasp_serial_osd_compat::Usart1TxDma"
rust_type_const_arguments_from_bindings = ["tx_buffer_bytes"]

[[resources]]
id = "rx_buffers"
storage = "static-buffer"
placement = "initialization-only"
element_rust_type = "u8"
size_from_binding = "rx_buffer_bytes"
count_from_binding = "rx_buffer_count"

[[resources]]
id = "tx_buffers"
storage = "static-buffer"
placement = "initialization-only"
element_rust_type = "u8"
size_from_binding = "tx_buffer_bytes"
count_from_binding = "tx_buffer_count"

[[resources]]
id = "rx_queue"
storage = "static-queue"
placement = "rtic-shared"
item_rust_type = "ferrowasp_serial_osd_compat::SerialChunk"
item_const_arguments_from_bindings = ["rx_buffer_bytes"]
capacity_from_binding = "rx_queue_capacity"
overflow = "reject-new-and-record-fault"
overflow_fault = "serial-rx-overflow"

[[resources]]
id = "tx_queue"
storage = "static-queue"
placement = "rtic-shared"
item_rust_type = "ferrowasp_serial_osd_compat::SerialChunk"
item_const_arguments_from_bindings = ["tx_buffer_bytes"]
capacity_from_binding = "tx_queue_capacity"
overflow = "reject-new-and-record-fault"
overflow_fault = "serial-tx-overflow"

[[initialization.operations]]
kind = "construct-resource"
resource = "rx_buffers"
constructor = "zeroed-array"

[[initialization.operations]]
kind = "construct-resource"
resource = "tx_buffers"
constructor = "zeroed-array"

[[initialization.operations]]
kind = "backend-prepare"
recipe = "stm32f4/usart1-dma-endpoint-v1"
inputs = [
  "endpoint_slot",
  "rx_buffers",
  "tx_buffers",
  "configuration.serial_profile",
]
outputs = ["rx_dma_state", "tx_dma_state"]

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
binding = "endpoint_slot.rx_pin"
exclusive = true

[[physical_claims]]
binding = "endpoint_slot.tx_pin"
exclusive = true

[failure_contract]
build_failure = "reject-resolution"
boot_initialization_failure = "panic"
unhandled_task_error = "record-component-fault"

[tests]
compile_fixtures = ["nucleo-msp-core"]
host_targets = []
```

### 6.4 Component definition example: MSP DisplayPort consumer

```toml
schema_version = "0.1"
capacities = []
physical_claims = []

[component]
id = "msp-displayport"
version = "0.1.0"
maturity = "experimental"
multiplicity = "many"

[implementation]
crate = "ferrowasp-serial-osd-compat"
module = "builder_facade::msp_displayport"

[implementation.constructors]
state = "ferrowasp_serial_osd_compat::OsdComponent::new"
telemetry = "ferrowasp_serial_osd_compat::OsdTelemetryState::default"

[implementation.task_entrypoints]
consume_rx = "consume_rx"
periodic = "periodic"

[[bindings]]
id = "frame_bytes"
kind = "capacity"
unit = "bytes"
required = true

[[configuration]]
id = "refresh_period_us"
value_type = "duration-us"
required = true

[[capability_ports]]
id = "rx_chunks"
interaction = "stream"
safety = "non-critical"
role = "consume-stream"
type = "serial-rx-chunk"
version = "1"
type_arguments = { frame_bytes = "frame_bytes" }
cardinality = "exactly-one"
required = true

[[capability_ports]]
id = "tx_frames"
interaction = "request"
safety = "non-critical"
role = "emit-request"
type = "serial-tx-chunk"
version = "1"
type_arguments = { frame_bytes = "frame_bytes" }
cardinality = "exactly-one"
required = true

[[tasks]]
id = "consume_rx"
kind = "software"
execution = "async-divergent-consumer"
scheduling_class = "observer"
local_resources = ["consume_output"]
shared_resources = [
  { id = "state", access = "exclusive" },
  { id = "telemetry", access = "read" },
]
capability_bindings = [
  { port = "rx_chunks", transport = "connection:rx_chunks", borrow_scope = "pre-invoke-copy" },
  { port = "tx_frames", transport = "connection:tx_frames", borrow_scope = "post-invoke-commit" },
]
lock_groups = [
  { id = "telemetry_snapshot", members = ["shared:telemetry"] },
]

[tasks.invocation]
entrypoint = "consume_rx"
arguments = [
  { kind = "capability-port", id = "rx_chunks" },
  { kind = "shared-resource", id = "state" },
  { kind = "shared-resource", id = "telemetry" },
  { kind = "local-resource", id = "consume_output" },
  { kind = "capability-port", id = "tx_frames" },
]
outcome_actions = [
  { kind = "send-port-on", outcome = "tx-frame", port = "tx_frames" },
]

[[tasks]]
id = "periodic"
kind = "software"
execution = "async-periodic"
scheduling_class = "serial-service"
period_us_from_configuration = "refresh_period_us"
periodic_mode = "fixed-delay"
missed_release = "skip-to-next"
local_resources = ["periodic_output"]
shared_resources = [
  { id = "state", access = "exclusive" },
  { id = "telemetry", access = "read" },
]
capability_bindings = [
  { port = "tx_frames", transport = "connection:tx_frames", borrow_scope = "post-invoke-commit" },
]
lock_groups = [
  { id = "telemetry_snapshot", members = ["shared:telemetry"] },
]

[tasks.invocation]
entrypoint = "periodic"
arguments = [
  { kind = "shared-resource", id = "state" },
  { kind = "shared-resource", id = "telemetry" },
  { kind = "local-resource", id = "periodic_output" },
  { kind = "capability-port", id = "tx_frames" },
]
outcome_actions = [
  { kind = "send-port-on", outcome = "tx-frame", port = "tx_frames" },
]

[[resources]]
id = "state"
storage = "typed-value"
placement = "rtic-shared"
rust_type = "ferrowasp_serial_osd_compat::OsdComponent"

[[resources]]
id = "telemetry"
storage = "typed-value"
placement = "rtic-shared"
rust_type = "ferrowasp_serial_osd_compat::OsdTelemetryState"

[[resources]]
id = "consume_output"
storage = "static-buffer"
placement = "rtic-local"
owner_task = "consume_rx"
element_rust_type = "u8"
size_from_binding = "frame_bytes"
count = 1

[[resources]]
id = "periodic_output"
storage = "static-buffer"
placement = "rtic-local"
owner_task = "periodic"
element_rust_type = "u8"
size_from_binding = "frame_bytes"
count = 1

[[initialization.operations]]
kind = "call-constructor"
constructor = "state"
outputs = ["state"]

[[initialization.operations]]
kind = "call-constructor"
constructor = "telemetry"
outputs = ["telemetry"]

[[initialization.operations]]
kind = "construct-resource"
resource = "consume_output"
constructor = "zeroed-array"

[[initialization.operations]]
kind = "construct-resource"
resource = "periodic_output"
constructor = "zeroed-array"

[[initialization.operations]]
kind = "spawn-task"
task = "periodic"
inputs = []

[failure_contract]
build_failure = "reject-resolution"
boot_initialization_failure = "panic"
unhandled_task_error = "record-component-fault"

[tests]
compile_fixtures = ["nucleo-msp-core"]
host_targets = []
```

The checked fixture may use the remaining compatibility adapter until
canonical FerroWasp facades exist, but catalogue maturity and provenance must
say so. Changing from compatibility to canonical paths is an explicit
catalogue and fixture change.

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

Near-term v0.1 does not add components. The profile explicitly lists every
component instance, and this stage validates that the listed set satisfies all
explicit port, implementation, and backend requirements.

Do not initially support broad “find any provider for this type” behavior. Ambiguous providers are an error.

The constrained component-dependency metadata and deterministic selection
mechanism are introduced only in M3. That future closure may:

- select a provider specified by policy;
- select the only compatible provider;
- add required adapter components;
- preserve full resolution reasoning in the report.

### 7.6 Stage 6: capability connection resolution

Resolution order:

1. validate explicit connections;
2. resolve uniquely inferable non-authority service/snapshot connections if policy permits;
3. require explicit authority edges;
4. require explicit per-edge delivery or an explicit multicast/snapshot/journal
   mechanism for fan-out;
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
- normalize logical source labels and paths to repository-relative `/` form;
- exclude source spans, absolute paths, host identity, timestamps, commands,
  and produced-artifact paths from semantic composition identity;
- include builder semantic version, schema versions, resolved board/backend
  semantics, selected components, connections, resources, scheduling, Cargo
  plan, and normalized build policy in semantic composition identity;
- record hashes of exact labeled board, profile, catalogue, policy, and
  lockfile input bytes as a separate input identity;
- record the actual toolchain, host, target, commands, and artifact hashes in
  post-resolution build provenance.

The composition hash is computed from a versioned
`SemanticApplicationIdentityInput` projection, not by blindly hashing the
entire serialized `ResolvedApplication`. The projection excludes its own
derived ID, diagnostic source spans, warnings, exact input-byte hashes, and
build provenance. Tests must enumerate every included and excluded field so a
new IR field cannot silently change or escape identity policy.

Suggested identity:

```text
resolved_application_id =
    <application-id>-<board-id>-<first-12-hex-of-composition-sha256>

input_set_id =
    sha256(canonical labels + exact input bytes)

build_input_id =
    sha256(input set id + implementation source identities + builder/renderer/
           backend/template identities + lock/toolchain policy + sanitized
           build environment)

build_provenance_id =
    sha256(composition id + build input id + actual commands/toolchain/host +
           artifact hashes)
```

The same resolved semantics must produce the same composition hash on Linux
and Windows. Build provenance is expected to differ when actual toolchains,
hosts, or produced artifacts differ. The 12-hex composition suffix is
human-facing only. Canonical metadata retains the full digest, storage keys
use full digests, and any existing-key reuse verifies exact identity and
bytes.

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

Rendered source and build records are immutable, separately keyed artifacts:

```text
generated/committed/source/<application-id>/<full-composition-sha256>/<input-set-id>/
generated/committed/build/<build-provenance-id>/
generated/failed/<application-id>/<run-id>/
```

Process:

1. create a same-filesystem candidate as a sibling of the eventual source
   destination, preserving path depth for relative Cargo dependencies;
2. render into the clean candidate;
3. format;
4. write IR and reports;
5. run `cargo check` and optionally run release build/configured tests;
6. promote the candidate to its immutable full-digest source destination.
   Refuse to overwrite an existing identity unless all identity metadata and
   bytes match, in which case reuse it;
7. validate the exact promoted path before publishing it. Generated Cargo
   dependencies must be depth-independent, or the final-path check is
   mandatory; a candidate-only check is not sufficient;
8. write the immutable build record/artifacts under the full
   `build_provenance_id`;
9. atomically update a small stable pointer file containing the full source
   and build IDs only after both artifacts are valid;
10. preserve failed candidate and command diagnostics under
   `generated/failed/`; an explicit retention/cleanup command may prune old
   failures without touching the last-known-good output;
11. never partially overwrite the last valid committed output.

No implementation shall “incrementally edit” the committed generated application in place.

The current prototype does not yet provide this immutable source/build store
or end-to-end last-known-good guarantee. It uses a mutable `working`
generation, sibling `.candidate-*` directories, and per-feature failures; it
can promote prefixes before the final release build succeeds. Reuse its
framed hashing, locking, diagnostic capture, and same-filesystem promotion
primitives, but treat the state layout and pointer guarantee above as new
work.

---

## 8. Command-line interface

The builder should be accessible through the existing project `xtask` interface where practical.

Recommended commands:

```text
cargo xtask app validate \
  --board boards/nucleo-f401re.toml \
  --profile application_profiles/nucleo-msp-core.toml

cargo xtask app resolve \
  --board ... \
  --profile ... \
  --out <scratch>/resolved_application.json

cargo xtask app generate \
  --board ... \
  --profile ... \
  --check

cargo xtask app build \
  --board ... \
  --profile ... \
  --release

cargo xtask app diff \
  --old generated/committed/source/<app>/<composition>/<input-set>/resolved_application.json \
  --new <scratch>/resolved_application.json

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

The exact source skeleton is maintained as generated, formatted, compiled
fixtures rather than duplicated as pseudo-Rust in this plan. The first
normative fixtures are:

```text
tests/fixtures/valid/nucleo-led/
tests/fixtures/valid/nucleo-msp-core/
tests/golden-ir/nucleo-led/resolved_application.json
tests/golden-ir/nucleo-msp-core/resolved_application.json
tests/golden-source/nucleo-led/
tests/golden-source/nucleo-msp-core/
```

The NUCLEO MSP-core fixture must demonstrate:

- generated provenance constants for builder version, composition ID, and
  exact input-set ID;
- the selected PAC path and deterministic dispatcher list;
- exact `Shared`, `Local`, and init-local static storage;
- two RX buffers and one TX buffer for the current endpoint contract;
- distinct USART-IDLE, RX-DMA, TX-DMA, TX-service, MSP-consumer, and periodic
  task wrappers;
- resource access matching the resolved graph;
- normal implementation-crate calls with no generated protocol or DMA state
  machine;
- no `todo!`, ellipses, placeholder markers, or hand-maintained behavioral
  hooks.

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

> Establish one trustworthy composition pipeline that can resolve, render,
> check, inspect, and reproduce a small RTIC application, then extend the same
> model to a generated, output-inhibited parallel F405 candidate without
> replacing the golden flight application.

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
   - `README.md`;
   - `docs/chatgpt-project-context.md`;
   - this reference plan;
   - any ADRs or active-work records that actually exist at task time;
   - target support documents;
   - the pinned FerroWasp monorepo commit and root-level project guidance when
     the task depends on flight or canonical component sources.
2. Produce `docs/implementation-inventory.md` containing:
   - existing paths;
   - reusable code;
   - transitional code;
   - duplicate responsibilities;
   - current tests;
   - missing tests;
   - configured capacities, their evidence/rationale, and any unverified
     prototype-only values;
   - current command entrypoints;
   - safety-sensitive files not to modify.
3. Record the exact golden application and target configuration.
4. Record which protected/golden paths are in the monorepo and which required
   sources, if any, still live externally; do not invent local paths for
   external sources.
5. Add no behavior changes in this task.

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
4. capability interaction, safety, role, and transport dimensions;
5. endpoint versus functional consumer;
6. one central renderer;
7. transactional generation;
8. explicit verbose board targets;
9. golden handwritten app preservation;
10. public builder/private assurance boundary;
11. constrained renderer-facing initialization, task-invocation, lock-group,
    timebase, and periodic-execution contract;
12. semantic identity versus exact input identity versus build provenance;
13. FerroWasp source pinning and compatibility-facade migration.

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

### N2A — Transport and RTIC 2 execution contract

**Objective:** Prove the concurrency and lifecycle model before exposing a
stable public schema.

**Deliverables:**

- independent interaction and safety classifications;
- SPSC, MPSC, latest-value, journal, same-task-direct, and synchronous-service
  transport definitions;
- edge-level transport ownership and explicit fan-out rules;
- RTIC 2 hardware, divergent consumer, periodic, and delayed one-shot task
  forms;
- fixed-rate/fixed-delay and missed-release policies;
- typed fault catalogue and boot/runtime failure separation;
- typed/versioned backend recipe signatures;
- bounded-lock facade rules;
- host validation tests and complete NUCLEO architecture contracts.

**Exit gate:**

- no renderer-generated queue/pending wake protocol;
- no destructive queue fan-out;
- no software-task spawn capacity used as data buffering;
- complete blinky and NUCLEO OSD applications render and link;
- the OSD hardware tasks, divergent consumers, channel topology, fault paths,
  and short-lock behavior are represented without arbitrary Rust escape
  hatches.

The current prototype evidence for this milestone is checked under
`architecture-contracts/` and enforced before rendering. It proves the two
NUCLEO vertical slices only; it is not the future public component schema.

---

### N3 — Provisional schema and executable fixtures

**Objective:** Parse explicit board, application, and component definitions
after N2A, while keeping the schema provisional until renderer-contract proof
and deterministic cross-platform fixtures pass.

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
- no implicit component discovery;
- exact port roles rather than ambiguous request/authority
  `provides`/`requires` labels;
- only closed structural renderer operations and validated Rust paths.

**Fixtures:**

```text
tests/fixtures/valid/minimal-led/
tests/fixtures/valid/nucleo-msp-core/
tests/fixtures/invalid/duplicate-id/
tests/fixtures/invalid/resource-conflict/
tests/fixtures/invalid/missing-capability/
tests/fixtures/invalid/authority-fanout/
```

**Acceptance criteria:**

- all valid fixtures parse and normalize;
- the TOML examples in section 6 are sourced from or byte-checked against the
  valid fixture files;
- each invalid fixture produces the expected diagnostic code;
- unknown fields are rejected;
- schema version is mandatory;
- diagnostics identify the source path.

Only after the complete blinky and OSD fixture/IR/source triplets render,
compile, and compare deterministically on supported platforms may this schema
be labelled the schema v0.1 freeze candidate.

---

### N4 — First component catalogue

**Objective:** Represent the existing NUCLEO MSP prototype without embedding behavior in the builder.

**Entry condition:** Every catalogue entry names an existing implementation
item. Move the current LED/button behavior behind small normal Rust
entrypoints before cataloguing it. Record a pinned FerroWasp source identity
for external items and confirm that required endpoint/consumer facade items
exist. Until then, the checked UART/MSP slice may reference the
provenance-pinned F401 compatibility adapter, with experimental maturity and
an explicit replacement note. MSP parsing/responding must continue to use the
canonical `crates/ferrowasp-mspv1` crate. Do not write catalogue entries that
claim nonexistent canonical Rust paths.

Initial catalogue:

```text
component_catalogue/
├── platform/
│   └── gpio-led.toml
├── endpoints/
│   └── uart-dma-endpoint.toml
└── functions/
    ├── button-edge.toml
    └── msp-displayport.toml
```

The first-class `TimebaseRequest` is resolved by the selected backend and is
not also represented as a component catalogue entry.

Land entries incrementally only after their referenced implementation items
exist: LED/button first, then UART/MSP after the compatibility or canonical
facades are added. The loader may validate schema-only draft fixtures earlier,
but it must not report their Rust paths as compile-smoke-validated.

**Required decomposition:**

```text
UART-DMA endpoint
    owns USART, DMA, pins, buffers, interrupts
    publishes bounded RX chunks
    handles bounded TX requests

MSP DisplayPort consumer
    owns parser/renderer state only
    consumes bounded RX chunks
    emits bounded TX requests
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
- compatibility paths are provenance-pinned and cannot be mistaken for
  canonical FerroWasp APIs;
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
resolve_initialization_operations
resolve_task_invocations
resolve_cargo_plan
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
- required capability omissions report source/destination port roles and
  connection context;
- initialization cycles report a readable cycle;
- complete blinky and NUCLEO MSP-core IRs contain no unresolved renderer
  choices;
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
- fixed task-wrapper invocation and outcome handling from resolved structural
  metadata;
- backend preparation recipes selected by resolved backend recipe IDs.

**Renderer non-responsibilities:**

- protocol parsing;
- LED state behavior;
- DMA state machine;
- OSD formatting;
- control logic;
- safety logic.
- component-ID matching;
- loading component definitions after resolution;
- arbitrary Rust fragments from catalogue or application inputs.

**Testing approach:**

- structured unit tests for emitted sections;
- golden-file snapshots for complete generated applications;
- compile tests are authoritative over textual snapshots;
- snapshots must be easy to update intentionally;
- forbid brittle tests that assert whitespace only.
- add burst/overflow tests or recorded sizing analysis before promoting the
  prototype RX/TX queue depths from configured values to justified bounds.

**Acceptance criteria:**

```text
cargo xtask app generate --profile nucleo-led --check
cargo xtask app generate --profile nucleo-msp-core --check
```

both succeed from a clean checkout.

The renderer must also pass a negative architecture test showing that adding a
new component instance expressible with existing resolved operations does not
require a new `match`/`if` branch on its component ID.

---

### N7 — Native compiler diagnostics and transactional checking

**Objective:** Make generated-code failures useful to Codex and humans.

**Implementation:**

- extract or adapt the existing command runner, framed hashing, application
  lock, diagnostic capture, failed-candidate retention, and Windows-safe
  promotion primitives instead of reimplementing them;
- retain the current fingerprint mechanism, but expand its dependency set:
  the prototype does not currently hash all path-dependency implementation
  sources. Include pinned commits or deterministic content-tree hashes for
  implementation crates plus builder, renderer, backend, template, manifest,
  lockfile, and toolchain-policy identities;
- run build tools with a documented allowlisted environment, or record every
  inherited build-affecting variable as part of `build_input_id`; command
  provenance includes both explicit overrides and retained inherited values;
- invoke Cargo with `--message-format=json-diagnostic-rendered-ansi`;
- stream rendered rustc diagnostics;
- capture command, status, and generated path;
- classify as `BLD002` without rewriting the rustc message;
- preserve the failed work tree and command diagnostics locally, and upload
  them as CI artifacts according to retention policy;
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

The legacy assembler may continue checking feature prefixes while it remains
available. New resolved-application generation checks only complete,
semantically valid checkpoints; migration must not force endpoint and consumer
components into a bundle merely to preserve the old prefix loop.

**Acceptance criteria:**

- an intentionally invalid Rust path surfaces the original rustc diagnostic;
- the last valid committed output remains unchanged;
- failed candidate path is printed;
- CI can upload failed source as an artifact;
- builder does not incorrectly blame the last component added.
- existing promotion/resume tests from the prototype remain green on Windows
  and Unix CI;
- new fault-injection tests cover failure of the second rename/restoration,
  cleanup failure, immutable-key collision, exact-byte reuse, final-path
  validation, and preservation of the last-known-good pointer.

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

- exactly one `Snapshot` writer;
- snapshot fan-out uses latest-value semantics rather than destructive queues;
- observation-only components cannot grant `Authority`;
- observer scheduling classes must not outrank protected critical classes without explicit policy;
- observer absence does not invalidate critical producer.

**Acceptance criteria:**

- OSD reads from a snapshot rather than canonical mutable critical state where practical;
- observer overload/failure test does not block the producer beyond measured accepted bounds;
- architecture report marks observation-plane edges distinctly.

---

### N13 — Generated parallel F405 candidate

**Objective:** Generate a meaningful static subset of the handwritten flight application.

**Entry gate:** The task record names the authoritative FerroWasp monorepo
commit, golden application, board revision, canonical component APIs, and
available target evidence. If those sources cannot be inspected, N13 remains
dependency-blocked; completion of the NUCLEO builder-core milestone is not
blocked.

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
generate-nucleo-msp-core
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

Near-term delivery has two independently reviewable milestones.

### Milestone A — NUCLEO Builder Core v0.1

Complete when:

1. one canonical model resolves LED/button and UART-DMA/MSP applications;
2. the renderer contract represents both applications without arbitrary Rust
   fragments or component-ID branches;
3. generated source is readable and compiler-checked;
4. the resolver and semantic composition hash are deterministic across the
   supported Linux/Windows matrix;
5. diagnostics are source-oriented and actionable;
6. endpoint and consumer behavior remains outside the builder;
7. physical ownership conflicts are rejected;
8. capability port roles, cardinality, and synthetic authority-policy fixtures
   are enforced;
9. architecture reports are generated;
10. CI detects generated drift;
11. current hashing/locking/promotion/failure-capture primitives and tests are
    retained, and the new immutable source/build store, final-path validation,
    rollback fault tests, and last-known-good pointer contract are complete;
12. all limitations and compatibility dependencies are documented.

Milestone A does not claim F405 support, flight equivalence, or target evidence.

### Milestone B — Parallel F405 Candidate

Complete when:

1. authoritative FerroWasp sources and the golden application are pinned and
   inspectable;
2. one explicit F405 board target exists;
3. SBUS and one SPI/IMU vertical slice are represented;
4. a generated output-inhibited parallel F405 candidate exists;
5. its structure is compared with the pinned golden application;
6. the golden handwritten app has not been implicitly replaced;
7. target evidence and untested behavior are stated accurately.

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

Complete capability support across the independent interaction kinds
`Authority`, `Stream`, `Request`, `Snapshot`, `EventJournal`, and `Service`,
with a separate safety classification on every resolved connection.

Add policy validation:

- authority reachability;
- protected component classes;
- prohibited observation-to-authority paths;
- permitted adapters;
- explicit aggregation components;
- version compatibility;
- capability-domain isolation.

Add graph queries:

```text
ports_with_role(type, role)
connection_sources_of(type)
connection_destinations_of(type)
authority_path_to(resource)
critical_predecessors(component)
observers_of(snapshot)
unconnected_required_ports()
```

These queries shall support reports, tests, and future assurance overlays.

---

### M3 — Deterministic dependency closure

Add a constrained implementation-dependency and service-offer selection
mechanism. Request handlers and other directed port roles follow their own
cardinality rules rather than generic provider terminology.

Rules:

1. an exact explicit implementation/service selection wins;
2. profile policy may name a preferred implementation or service offer;
3. one compatible non-authority service offer may be inferred;
4. multiple compatible offers are an error;
5. authority grant/receive connections are never inferred;
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
- transport capacity and task-form validation;
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
- a data-producing edge that attempts to use software-task spawn as buffering.

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

Add `EventJournal` only for real event requirements:

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
- interaction kind, safety class, explicit port role, topology, and
  compatibility;
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
- NUCLEO MSP core;
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

- semantic composition hash;
- exact input-set hash when fixture bytes are identical;
- generated source after newline normalization;
- semantic reports after defined path/newline normalization;
- Cargo manifest;
- the platform-independent fields of build provenance.

Actual toolchain/host fields and the build-provenance hash may differ across
platforms. Toolchain-dependent binary reproducibility is a separate objective
and shall not be assumed.

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

- verify, freeze, and reuse the existing byte-compared
  `tests/golden/nucleo-f401re-blinky/` migration fixture;
- identify current generation logic;
- move LED/button behavior behind normal implementation entrypoints;
- model component/task/resource metadata;
- render through central IR;
- compare generated source;
- remove old specialized renderer after parity.

### Migration B — Existing MSP/USART generator

- commit the current generated source and task/resource inventory as a
  migration fixture;
- pin the compatibility or canonical FerroWasp implementation source;
- split endpoint and consumer;
- define directed RX critical-data and TX request port roles;
- model DMA buffers/queues;
- model periodic task and monotonic;
- model exact resource access, task invocations, outcome actions, and backend
  initialization operations;
- resolve explicit connections;
- render and compile a complete valid graph checkpoint;
- add a separate full-parity fixture for `nucleo-f401re-osd` covering
  `button_arm_toggle`, debounce/interrupt work, shared telemetry state, and
  the DisplayPort telemetry read port;
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

Every significant implementation task shall begin by writing or updating one
task record in the repository's chosen issue/PR system or, for local work,
under `docs/work/`. Do not introduce a second global active-work file when the
same record already exists elsewhere. The record contains:

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

No behavior changes. Documents current builder, external dependencies, and
protected flight paths; verifies the existing blinky golden and adds the
missing OSD structural/golden baseline.

### PR-002 — Architecture contracts

Accepts vocabulary, renderer-operation, task-invocation,
identity/provenance, and external-source ADRs.

### PR-003 — Reusable orchestration infrastructure

Extracts/adapts the current runner, hashing, lock, checkpoint, failure
preservation, and candidate-promotion code without changing legacy behavior.

### PR-004 — Builder model and diagnostics

Adds IDs, source references, diagnostics, resolved structural operations, and
canonical semantic serialization.

### PR-005 — Schema v0.1 and executable fixtures

Parses board/profile/component definitions; documentation examples are checked
against focused valid and invalid fixtures.

### PR-006 — LED/button facade and catalogue loader

Moves the existing LED/button behavior behind normal Rust entrypoints, loads
exact component versions, and smoke-validates the first real catalogue paths.
UART/MSP entries remain schema-only until PR-012 supplies their facades.

### PR-007 — Instance expansion and generated names

Adds repeated component instances, template expansion, and collision tests.

### PR-008 — Capability graph v0.1

Supports explicit directed port connections and cardinality; authority
inference is prohibited.

### PR-009 — Physical claim validator

Rejects pin/DMA/peripheral/interrupt conflicts using explicit endpoint slots.

### PR-010 — Scheduling, initialization, and resolved output

Resolves priorities, dispatchers, initialization operations, task invocations,
Cargo plan, canonical `ResolvedApplication`, and semantic/input identities.

### PR-011 — Central renderer and blinky

Generates and checks the simplest application from resolved IR only, reusing
the facade and catalogue entry established in PR-006.

### PR-012 — UART/MSP implementation facades

Pins canonical or compatibility sources and exposes separately usable
UART-DMA endpoint and MSP consumer entrypoints.

### PR-013 — UART/MSP catalogue and generated app

Represents the endpoint and consumer independently, generates the extracted
NUCLEO MSP core, forwards rustc diagnostics, and uses the adapted
transactional pipeline.

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
  --profile application_profiles/nucleo-msp-core.toml \
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
  --old generated/committed/source/<app>/<composition>/<input-set>/resolved_application.json \
  --new <scratch>/resolved_application.json

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
6. How implementation Rust paths are compile-validated before full generation.
7. Whether Cargo dependency versions come from catalogue, workspace dependencies, or a controlled policy file.
8. Exact snapshot implementation.
9. Boot-frozen router location and configuration schema.
10. STM32H7 memory-placement syntax.
11. Public schema compatibility guarantees before `1.0`.
12. Conditions for committing generated sources for every app versus reference/release apps only.
13. Conditions for standalone builder extraction.

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
- builder/composition/input identity constants embedded, with actual
  toolchain/command/artifact provenance stored in the immutable build sidecar;
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
2. Verify/freeze the existing blinky golden and add an OSD structural/golden baseline.
3. Adopt vocabulary, renderer-contract, identity/provenance, and external-source ADRs.
4. Extract/reuse transactional generation and command-running infrastructure.
5. Implement deterministic model and structured diagnostics.
6. Implement schema v0.1 from executable fixtures.
7. Implement exact component catalogue and validated port roles.
8. Implement resolver, validators, initialization operations, and task invocations.
9. Implement the central renderer.
10. Generate/check the LED/button app.
11. Establish pinned UART-DMA endpoint and MSP consumer implementation facades.
12. Generate/check the NUCLEO MSP core.
13. Add architecture reports and drift CI.
14. Complete NUCLEO Builder Core v0.1.
15. After the external-source entry gate, add an explicit F405 board target.
16. Add SBUS and SPI/DMA IMU vertical slices.
17. Add a read-only observation snapshot.
18. Generate an output-inhibited parallel F405 candidate.
19. Compare generated and handwritten structures.
```

Independent exit artifacts:

> **NUCLEO Builder Core v0.1**
>
> **Parallel F405 Candidate**, only after its external-source and target gates

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
what each capability port publishes, consumes, emits, handles, grants, or receives
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

## Appendix A — Canonical `ResolvedApplication` fixture

The normative example shall be the complete generated fixture at:

```text
tests/golden-ir/nucleo-msp-core/resolved_application.json
```

PR-010 creates it from the checked section 6 inputs. This plan intentionally
does not carry an independently edited abridged JSON object, because an
abridged object easily appears schema-valid while omitting required identity,
resource-access, initialization, Cargo, or provenance fields.

The fixture review must confirm at least:

- exact component versions `0.1.0`;
- directed `uart1.rx_chunks -> displayport.rx_chunks` and
  `displayport.tx_frames -> uart1.tx_frames` connections;
- USART1 plus DMA2 stream 5 channel 4 RX and stream 7 channel 4 TX ownership;
- two RX buffers and one TX buffer;
- all hardware and software tasks, priorities, dispatchers, capacities,
  resource access, and implementation invocations;
- semantic composition and exact input-set identities;
- repository-relative normalized source references;
- no actual host/toolchain identity inside semantic composition identity.

---

## Appendix B — Architecture report outline

```markdown
# Resolved Application Summary

## Identity
- Application:
- Board:
- Backend:
- Builder:
- Semantic composition ID:
- Exact input-set ID:
- External source commit(s):
- Build provenance ID:
- Actual toolchain/host:

## Components
| Instance | Type | Version | Maturity |

## Capabilities
| Source port/role | Capability | Destination port/role | Class | Reason |

## Tasks
| Task | Kind | Interrupt/Dispatcher | Priority | Capacity | Component |

## Resources
| Resource | Storage | Placement | Owner | Accessors | Capacity |

## Physical ownership
| Peripheral/pin/DMA/timer/interrupt | Owner |

## Initialization order
1.
2.
3.

## Policies
- Authority grant/receive ports:
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
# Task APP-N6-002: Generate NUCLEO MSP-core vertical slice

## Objective
Generate and compiler-check the extracted NUCLEO USART1 DMA plus MSP
DisplayPort core through the central `ResolvedApplication` renderer.

## Current behavior and evidence
The repository contains a working prototype with UART DMA ownership, static RX/TX storage, bounded queues, a separate OSD consumer, and periodic software work. Record exact file paths and the current successful command before editing.

## Required behavior
The extracted two-component core shall be expressed through:
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
Update the implementation inventory, the issue/PR or local work record if one
is used, component authoring notes, and the generated app README.

## Non-goals
- Full parity with the current button/telemetry OSD composition.
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

This plan was initially derived from an external FerroWasp/FerroPilot planning
source supplied during architecture work. That planning source is not checked
in with a stable path and commit, so it is not a silent dependency or an
authority for implementation tasks. This checked-in plan is self-contained for
RTIC App Builder planning.

When work depends on current FerroWasp code, safety policy, golden
applications, board status, or target evidence, the task must pin and inspect
the monorepo commit and exact source paths. External FerroPilot sources must be
pinned separately when applicable. Repository code, tests, accepted ADRs, live
evidence, and newer explicit project decisions take precedence over this plan
where they conflict.
