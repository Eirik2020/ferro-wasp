# App Builder V3 Mainline Filesystem Implementation Plan

**Status:** Approved architecture reference, revision 3 (`2026-08-16`)  
**Approval:** User approved the reviewed filesystem corrections, including the
compile-time input catalog, host/runtime task boundary, isolated builder
workspace, and sibling platform/live configuration files.
**Accepted file map:**
[`CHECKPOINT_0_MIGRATION_MAP.md`](CHECKPOINT_0_MIGRATION_MAP.md) applies these
rules to all 76 current V3 Rust sources.

**Repository:** `Eirik2020/ferro-wasp`  
**Reference branch scanned:** `tool/app-builder`  
**Reference commit:** `a994d751cfe0c2a2525b2cb34583e496925bf3b3` (`expanded app builder v3`)  
**Audience:** Codex / repository maintainers  
**Primary objective:** Move App Builder V3 from an isolated prototype layout into the main FerroWasp repository structure without changing runtime behavior, task semantics, board facts, safety authority, generated RTIC semantics, or target hardware behavior.

---

## 1. Objective

App Builder V3 is becoming a mainline FerroWasp tool, but its current filesystem still reflects prototype isolation:

```text
tools/app-builder-v3/
├── generated/
└── xtask/
    └── src/
        ├── generator.rs
        ├── hardware_definitions/
        ├── rtic/
        ├── target/
        └── tasks/
```

This structure mixes several ownership domains:

1. App Builder compiler/generator infrastructure.
2. App Builder MCU/HAL resolution backends.
3. Reusable software tasks.
4. HAL-specific hardware tasks.
5. Target-side STM32F4 helper/runtime code.
6. Physical board declarations.
7. Complete application compositions.
8. Generated firmware.

The goal of this migration is to separate those domains according to what they *are*, not according to where they happened to be implemented during V3 prototyping.

---

# 2. Architectural ownership rules

These rules are authoritative for this migration.

## 2.1 Reusable software tasks

Reusable **hardware-independent software RTIC tasks** belong at the top level:

```text
tasks/
```

Examples, only when their imports and contracts remain portable:

- safety master logic/task body;
- a heartbeat over an abstract status interface;
- OSD service task;
- USB service task when its task body is portable over an abstract interface;
- actuator safety/command adapter when HAL-independent;
- generic ESC manager when it operates on portable interfaces;
- application-independent storage/service tasks.

Rule:

> If the reusable RTIC task body still conceptually makes sense on STM32F4, STM32H7, NXP RT, or a host/simulator backend after substituting concrete resources, it belongs in top-level `tasks/`.

These are App Builder **inputs**. The App Builder does not own the reusable software task library.

---

## 2.2 HAL-specific task boundary

HAL-specific task code must be classified by **which crate compiles it**, not
only by the hardware semantics of the body it eventually emits.

Target-runtime task mechanisms that compile independently of the host builder
belong in the corresponding HAL crate. For STM32F4:

```text
crates/ferrowasp-stm32f4/src/tasks/
```

Examples:

- USART IDLE interrupt task;
- RX DMA interrupt task;
- TX DMA interrupt task;
- SPI DMA interrupt task;
- EXTI interrupt task;
- TIM update interrupt task;
- DShot DMA completion task;
- hardware service tasks whose resource types or behavior are STM32F4-specific.

Host-side RTIC task declarations, `reusable_task!` wrappers, source-extraction
metadata, and lowering recipes belong with the STM32F4 builder backend:

```text
tools/app-builder/src/backends/stm32f4/tasks/
```

Rule:

> A target-runtime mechanism belongs in the HAL crate. A host-compiled
> declaration that describes or emits that mechanism remains a builder input,
> even when the emitted body is STM32F4-specific.

Do **not** put STM32F4 hardware task declarations in top-level `tasks/`, and do
not make `ferrowasp-stm32f4` depend on App Builder macros or authoring types.

---

## 2.3 Target-side HAL helper/runtime code

Code that actually runs on the STM32F4 target and helps use `stm32f4xx-hal` belongs in:

```text
crates/ferrowasp-stm32f4/
```

Examples already present in the repository include runtime mechanisms around:

- UART DMA;
- SPI DMA;
- DShot;
- timers;
- clocks;
- watchdog;
- serial endpoints;
- board/runtime configuration types.

This code answers:

> “How does firmware actually operate this STM32F4 peripheral?”

Examples:

```rust
configure_uart_dma(...)
start_dma_transfer(...)
clear_dma_flags(...)
configure_exti(...)
initialize_dshot(...)
```

This is **not** App Builder backend code.

---

## 2.4 App Builder HAL backend

Host-side code that understands STM32F4 hardware semantics for generation belongs under the App Builder:

```text
tools/app-builder/src/backends/stm32f4/
```

This code does **not** run on the MCU.

It answers questions such as:

- Is a pin legal for this peripheral?
- Which alternate function applies?
- Which DMA routes are valid?
- Which interrupt corresponds to a selected DMA stream?
- Which timer capabilities exist?
- How does an abstract resolved application lower into STM32F4-specific RTIC resources?
- Which `stm32f4xx-hal` / PAC spelling must be rendered?

Examples:

```text
PA3 + USART2 RX
    -> validate AF mapping

USART2 RX + DMA request
    -> resolve DMA1 Stream5 Channel4

DMA1 Stream5
    -> resolve DMA1_STREAM5 interrupt

TIM4
    -> resolve timer capabilities / interrupt
```

Rule:

> Builder backend code describes and resolves hardware for code generation. HAL crate code operates the hardware at runtime.

Do not merge these layers.

---

## 2.5 Board declarations

Physical board facts belong in a reusable top-level board area:

```text
boards/
```

A board declaration describes immutable or hardware-defined facts such as:

- MCU;
- clock source/frequency;
- physical pins;
- peripheral instances;
- DMA routes;
- timer availability;
- connector identities;
- onboard flash;
- ADC channels;
- DShot output wiring;
- physical sensor installations;
- sensor-to-body orientation;
- memory layout when board-specific.

The current V3 Foxeer board declaration is exactly this kind of information.

A board must **not** own application policy such as:

- RC protocol selection;
- MSP assignment;
- control-loop scheduling policy;
- task priorities;
- safety-channel topology;
- application task graph.

A board may be reused by multiple complete applications.

---

## 2.6 Apps

`apps/` contains **complete App Builder application compositions**, not reusable “flight core” fragments.

App-specific task bodies and component declarations that would be meaningless
without one complete application also remain app-owned, for example:

```text
apps/<app>/tasks/
apps/<app>/components/
```

Only definitions demonstrated to be reusable move to top-level `tasks/` or a
builder backend. App-owned Rust inputs are compiled through
`input_catalog.rs`; they are not added to the embedded app's target module
tree.

An app composition is the complete application graph and includes/selects:

- one board declaration;
- platform configuration;
- components;
- software and hardware task instances;
- task priorities;
- resource bindings;
- safety classification;
- safety channels;
- task-local state declarations;
- monotonic selection;
- scheduling resources;
- init spawns;
- interrupt bindings where appropriate.

The composition is expected to fail App Builder validation / compilation when the selected board cannot satisfy its requirements.

Therefore:

```text
AppComposition + selected Board
    -> complete application
```

There is no additional `builds/` or `targets/` source layer required.

---

## 2.7 Generated firmware

Generated RTIC firmware is an output of an app composition.

Do not create a separate source-owned `builds/` abstraction.

Keep generated output clearly separated from hand-authored application declarations.

The preferred model is:

```text
apps/<app>/generated/
```

This location is accepted only after a focused Cargo test proves that the
existing isolated app, the nested generated package, relative dependencies,
lockfile behavior, and target configuration remain isolated. If that test
fails, use an explicitly app-associated external output path and record why;
do not force nested workspaces for visual uniformity.

Generated files must remain clearly marked as generated and must not be hand edited.

---

## 2.8 Host compilation of repository-owned Rust inputs

Top-level `tasks/`, `boards/`, and app-owned Rust definitions are source
ownership boundaries, not independent Cargo packages during this migration.
The isolated host builder compiles them through one explicit registry:

```text
tools/app-builder/src/input_catalog.rs
```

The initial registry may use checked `#[path = "..."]` module declarations so
the existing `crate::reusable_task!` contract, builder authoring types,
`file!()` paths, and source-span extraction keep working unchanged.

The registry must:

- list every external Rust input explicitly, without glob discovery;
- keep board, app, portable-task, and backend-task registrations distinct;
- prove that `file!()` resolves to the relocated source file;
- fail clearly when a registered input is missing or duplicated;
- be covered by source-discovery and deterministic-rendering tests.

This is a compile-time bridge, not builder ownership of the external files.
Creating separately versioned input crates or replacing the task macro is a
later architectural decision, not part of this filesystem migration.

---

# 3. Desired top-level repository structure

Target direction:

```text
ferro-wasp/
├── Cargo.toml
├── Cargo.lock
├── README.md
│
├── apps/
│   ├── <complete-app>/
│   │   ├── app_composition.rs
│   │   ├── platform_config.rs
│   │   ├── live_config.rs
│   │   ├── components/             # app-specific declarations only
│   │   ├── tasks/                  # app-specific task authoring only
│   │   ├── README.md
│   │   └── generated/
│   │       ├── Cargo.toml
│   │       ├── src/
│   │       │   ├── main.rs
│   │       │   ├── prelude.rs
│   │       │   ├── platform_config.rs
│   │       │   └── live_config.rs
│   │       └── SAFETY_SPINE.md
│   └── ...
│
├── boards/
│   ├── foxeer-f405-v2/
│   │   ├── board.rs
│   │   ├── README.md
│   │   └── ...
│   ├── fcu3/
│   ├── nucleo-f401re/
│   └── ...
│
├── tasks/
│   ├── mod.rs
│   ├── heartbeat.rs
│   ├── safety_master.rs
│   ├── msp_osd.rs
│   └── ...
│
├── crates/
│   ├── ferrowasp-core/
│   ├── ferrowasp-drivers/
│   ├── ferrowasp-tasks/          # see migration note below
│   ├── ferrowasp-stm32f4/
│   │   └── src/
│   │       ├── ...
│   │       └── tasks/
│   │           ├── mod.rs
│   │           └── ...              # target-runtime mechanisms only
│   └── ...
│
├── tools/
│   ├── app-builder/
│   │   ├── Cargo.toml
│   │   ├── README.md
│   │   ├── AGENTS.md
│   │   └── src/
│   │       ├── main.rs
│   │       ├── lib.rs
│   │       ├── generator.rs
│   │       ├── input_catalog.rs
│   │       ├── rtic/
│   │       │   ├── component.rs
│   │       │   ├── composition.rs
│   │       │   ├── platform_config.rs
│   │       │   ├── render.rs
│   │       │   ├── report.rs
│   │       │   ├── resolve.rs
│   │       │   ├── safety_channel.rs
│   │       │   ├── state.rs
│   │       │   ├── task.rs
│   │       │   └── timing.rs
│   │       └── backends/
│   │           └── stm32f4/
│   │               ├── mod.rs
│   │               ├── board_declaration.rs
│   │               ├── dma.rs
│   │               ├── gpio.rs
│   │               ├── pins.rs
│   │               ├── serial.rs
│   │               ├── spi.rs
│   │               ├── timer.rs
│   │               ├── mcu.rs
│   │               ├── interrupts.rs
│   │               ├── validate.rs
│   │               ├── lower.rs
│   │               └── tasks/
│   └── ...
│
├── mdbook/
└── project_meta/
```

This is a direction, not permission to create empty speculative directories. Only create directories required by moved code.

---

# 4. Important note about `ferrowasp-tasks`

The repository currently contains:

```text
crates/ferrowasp-tasks/
```

with reusable firmware behavior.

V3 separately contains reusable App Builder task bodies under:

```text
tools/app-builder-v3/xtask/src/tasks/
```

The desired architectural direction is:

```text
top-level tasks/
```

for reusable App Builder software task definitions.

However, do **not** blindly move all existing `crates/ferrowasp-tasks` code into `tasks/`.

First distinguish:

### A. reusable runtime/domain implementation

Example:

```text
control algorithms
arming state machines
storage managers
protocol-independent actuator logic
OSD logic
ESC management logic
```

These may remain normal reusable Rust crates.

### B. reusable App Builder RTIC task declarations/bodies

Example current V3 files using:

```rust
crate::reusable_task! { ... }
```

These belong in the top-level task library.

The task declaration may call reusable implementation from `ferrowasp-tasks`.

Preferred relationship:

```text
tasks/safety_master.rs
        |
        v
crates/ferrowasp-tasks/... reusable implementation
```

Do not duplicate domain logic into the task-definition layer.

---

# 5. Current V3 mapping

Use the current V3 tree as the migration source.

## 5.1 App Builder core

Current:

```text
tools/app-builder-v3/xtask/src/rtic/
├── component.rs
├── composition.rs
├── platform_config.rs
├── render.rs
├── report.rs
├── resolve.rs
├── safety_channel.rs
├── state.rs
├── task.rs
└── timing.rs
```

Proposed:

```text
tools/app-builder/src/rtic/
```

Initially preserve the current module structure.

Do not rename `rtic` to `model`, split `resolve/`, or perform other cosmetic restructuring in the same migration unless required to remove a dependency cycle.

This migration is primarily about ownership boundaries.

---

## 5.2 Generator

Current:

```text
tools/app-builder-v3/xtask/src/generator.rs
```

Proposed:

```text
tools/app-builder/src/generator.rs
```

Keep generation behavior unchanged.

Preserve:

```text
APP_COMPOSITION
 -> resolve
 -> golden reconciliation
 -> render
 -> report
 -> rustfmt
 -> write generated files
```

except for path/input wiring required by the filesystem migration.

---

## 5.3 App Builder STM32F4 backend

Current V3 has a mixed directory:

```text
tools/app-builder-v3/xtask/src/hardware_definitions/stm32f4/
```

Classify every file before moving it.

### Builder-backend candidates

These are expected to remain host-side App Builder code:

```text
board_declaration.rs
dma_route.rs
gpio.rs
mcu.rs
pins.rs
serial.rs
spi.rs
timer.rs
lower.rs
```

Move/rename as appropriate under:

```text
tools/app-builder/src/backends/stm32f4/
```

Also place App Builder-only validation / resolution tables here.

Do not move runtime `stm32f4xx-hal` helper implementations here.

---

## 5.4 V3 hardware endpoint declarations

Current:

```text
hardware_definitions/stm32f4/hw_endpoint/
├── imu_endpoint.rs
└── serial_endpoint.rs
```

These require deliberate classification.

For each endpoint type, determine whether it is:

1. an App Builder host-side declaration/lowering construct; or
2. target-side endpoint implementation; or
3. both concerns currently mixed in one file.

If host-side declaration only:

```text
tools/app-builder/src/backends/stm32f4/endpoints/
```

If target runtime mechanism:

```text
crates/ferrowasp-stm32f4/src/endpoints/
```

If mixed:

- split only along the host/runtime boundary;
- keep behavior identical;
- preserve names where practical;
- do not redesign endpoint semantics during the move.

---

## 5.5 HAL-specific V3 tasks

Current:

```text
tools/app-builder-v3/xtask/src/hardware_definitions/stm32f4/tasks/
```

Examples include:

```text
button_exti.rs
imu_data_ready.rs
periodic_control_tick.rs
serial_rx_bridge.rs
serial_rx_dma_irq.rs
serial_rx_idle_irq.rs
serial_tx_dma_irq.rs
serial_tx_worker.rs
spi_imu_owner_service.rs
spi_imu_parser.rs
spi_imu_poll.rs
spi_imu_rx_dma_irq.rs
spi_imu_timeout.rs
...
```

Do not bulk-move based solely on current folder.

Classify each task.

### Definite hardware/HAL characteristics

A task or task mechanism is STM32F4-specific when it:

- binds to an STM32F4 interrupt;
- owns STM32F4 DMA;
- owns a concrete STM32F4 peripheral;
- depends on STM32F4 HAL/PAC types;
- exists because of STM32F4 peripheral topology;
- clears/handles concrete STM32F4 hardware state.

After making that determination, classify its compile-time role:

- target-runtime code that does not use builder macros or authoring types may
  move to `crates/ferrowasp-stm32f4/src/tasks/`;
- a host-compiled declaration, `reusable_task!` wrapper, contract, or lowering
  recipe stays under `tools/app-builder/src/backends/stm32f4/tasks/`;
- a mixed file must either be split strictly at the host/runtime boundary or
  stay host-side until such a split can preserve exact behavior.

The current V3 hardware-task files are host-authoring inputs. Do not move them
unchanged into `ferrowasp-stm32f4`, because that would couple the runtime crate
to App Builder macros and source-authoring infrastructure.

### Portable software task characteristics

A task belongs under:

```text
tasks/
```

when it only consumes portable resource interfaces and its behavior is not STM32F4-specific.

Examples such as parser/bridge/worker tasks must be classified based on imports and contracts rather than filename.

---

## 5.6 Reusable V3 software tasks

Current:

```text
tools/app-builder-v3/xtask/src/tasks/
```

Move HAL-independent `reusable_task!` task definitions to:

```text
tasks/
```

Preserve:

- task body;
- task contract;
- local/shared/config/spawn slots;
- tests;
- source discoverability required by V3 rendering.

Important:

V3 currently records task source and uses `syn` plus source spans to locate/rewrite the reusable task body.

The builder registers these relocated modules through
`src/input_catalog.rs`; the top-level directory does not become a Cargo crate
in this migration.

After moving files, update source discovery so:

- `file!()` continues to identify the actual task source;
- task body extraction still finds exactly one matching `reusable_task!`;
- rendering remains deterministic;
- generated task bodies are semantically identical.

Do not replace the span-rewriting mechanism in this filesystem migration.

That is a separate architectural change.

---

# 6. Board migration

Current Foxeer V3 board:

```text
tools/app-builder-v3/xtask/src/target/board.rs
```

It contains real physical board facts including:

- STM32F405 MCU;
- HSE/system clock;
- TIM2/TIM4/TIM5/TIM6 availability;
- ADC1 PC0/PC1 route;
- SPI2 NOR wiring;
- OTG_FS pins/identity;
- physical DShot lanes;
- USART1 ESC telemetry;
- USART2 `serial1`;
- UART4 `serial2`;
- SPI1;
- nested IMU installation;
- sensor-to-body orientation.

Move this declaration to a reusable board package/directory:

```text
boards/foxeer-f405-v2/
```

Preferred initial shape:

```text
boards/foxeer-f405-v2/
├── board.rs
├── mod.rs
└── README.md
```

Do not arbitrarily split the board into many small files during the first migration.

The declaration is currently compact enough to move as a coherent unit.

The board must expose a stable board declaration usable by more than one app.

During this migration the board is compiled through the builder's explicit
`input_catalog.rs` registration. `boards/` is not implicitly made a Cargo
workspace member, and the board declaration must not depend on a complete
application.

---

# 7. Application migration

Current complete V3 application:

```text
tools/app-builder-v3/xtask/src/target/app_composition.rs
```

This is **not** a reusable library fragment.

It currently defines the complete selected application:

- board reference;
- monotonic;
- init-delay timer;
- serial endpoint components;
- SPI IMU endpoint;
- periodic control component;
- DShot actuator;
- service hardware;
- task state;
- shared resources;
- safety channels;
- all task instances;
- priorities;
- config bindings;
- spawn bindings;
- init spawns.

Move it into an app-owned location under:

```text
apps/
```

Do not create an intermediate `builds/` or `targets/` source layer.

The concrete app name should follow the actual firmware identity used in the repository.

If the existing `apps/foxeer-f405-v2` package is being replaced by this V3-generated application, do not silently overwrite it.

First determine the intended mainline app name and migration relationship.

Prefer a migration where the generated V3 app can be compared with the existing golden Foxeer firmware before replacement.

The app-owned Rust inputs are registered explicitly by the host builder. They
must not be added to the embedded app's target compilation merely because they
share its directory.

---

# 8. Platform and live configuration

Current:

```text
tools/app-builder-v3/xtask/src/target/platform_config.rs
```

It currently selects:

```text
serial1 -> RcSbus
serial2 -> MspV1Osd
spi1    -> Imu(installation 1)
```

This is application configuration, not an immutable board fact.

Keep it with the complete app and beside the live configuration:

```text
apps/<app>/platform_config.rs
apps/<app>/live_config.rs
```

Do not move these service assignments into `boards/foxeer-f405-v2`.

`platform_config.rs` owns boot-frozen service placement and hardware-facing
application bindings. `live_config.rs` owns only the app's bounded,
runtime-adjustable configuration surface.

The current V3 tree has no distinct `live_config.rs`. Stage 0 must identify
the existing source and behavior that will populate this boundary. Do not
invent a new configuration schema, default, mutation path, or runtime behavior
as part of the filesystem-only move.

Live configuration must not arm the application, alter safety state, bypass
freshness or health checks, or command actuators. Existing whitelisting,
bounds, disarmed-only write policy, validation, and failure behavior remain
mandatory.

The board owns endpoint availability.

The app's platform and live configuration remain sibling source files with
different responsibilities.

---

# 9. Golden reconciliation

Current:

```text
tools/app-builder-v3/xtask/src/target/golden_reconciliation.rs
```

Do not assume its final location only from its name.

Inspect its dependencies and responsibility.

Classify as one of:

### App-specific semantic reconciliation

If it encodes expected semantics for one complete app:

```text
apps/<app>/verification/
```

or a nearby app-owned module.

### Generic App Builder reconciliation mechanism

If it is reusable across arbitrary apps:

```text
tools/app-builder/src/
```

### Repository evidence fixture

If it is primarily a fixed comparison snapshot / evidence declaration:

place it under an app-specific verification/evidence directory, not builder core.

Do not mix generic validation with Foxeer-specific golden assumptions.

---

# 10. Generated output

Current:

```text
tools/app-builder-v3/generated/
```

contains:

```text
.cargo/config.toml
Cargo.toml
Cargo.lock
src/main.rs
src/prelude.rs
src/platform_config.rs
SAFETY_SPINE.md
```

`live_config.rs` is a required final generated boundary. Because the current
V3 output does not contain that file, its initial content must be traced to
existing authoritative configuration behavior rather than treated as a
pre-migration generated-file match.

This output belongs to the selected app, not to the App Builder implementation.

Preferred direction:

```text
apps/<app>/generated/
```

The final generated package includes sibling `src/platform_config.rs` and
`src/live_config.rs` after the live-configuration source mapping is complete.

The builder should accept an explicit output path or derive it from the selected app.

Generated files must retain deterministic generation.

Do not hand-edit generated files during migration.

---

# 11. No new `target/` / `targets/` source abstraction

Do not create a top-level source folder named:

```text
target/
targets/
```

Cargo already uses:

```text
target/
```

for build artifacts.

More importantly, V3 does not require an additional target abstraction.

The complete `AppComposition` already contains/selects its board.

The model is:

```text
Board declaration
      +
complete AppComposition
      |
      v
App Builder validation/resolution
      |
      v
generated RTIC firmware
```

---

# 12. Dependency direction

Preserve a clean dependency direction.

Conceptually:

```text
crates/ferrowasp-core
crates/ferrowasp-drivers
crates/ferrowasp-*
          ^
          |
top-level reusable software tasks
          ^
          |
complete app composition
          |
          +------> reusable board declaration
          |
          v
App Builder host-side model/resolver
          |
          v
App Builder STM32F4 backend
          |
          v
generated RTIC application
          |
          v
crates/ferrowasp-stm32f4 runtime/HAL implementation
```

Avoid circular ownership.

In particular:

- `ferrowasp-stm32f4` must not depend on the App Builder host tool.
- board declarations must not depend on a complete app.
- reusable software tasks must not depend on a concrete board.
- App Builder core should not depend on Foxeer-specific application semantics except through supplied declarations.
- App Builder STM32F4 backend may understand STM32F4 hardware declaration types.
- apps may select boards and task/component declarations.

The compile graph is explicit even though physical source ownership crosses
directories:

```text
tools/app-builder/src/input_catalog.rs
    -> #[path] tasks/...                       portable authoring inputs
    -> #[path] boards/...                      physical declarations
    -> #[path] apps/<app>/...                  complete app inputs
    -> normal mod backends/stm32f4/tasks/...   HAL authoring inputs

generated firmware
    -> ferrowasp-stm32f4                       target runtime dependency
```

The target runtime dependency never points back to the host builder.

---

# 13. Workspace strategy

The current root Cargo workspace includes reusable crates while deployable firmware apps are intentionally isolated due to incompatible PAC/HAL feature sets.

Do not force all generated firmware packages into the root workspace if doing so unifies incompatible target dependencies.

Preserve the current isolation principle.

The approved mainline App Builder is an isolated host workspace under:

```text
tools/app-builder/
```

Do not add it, the external Rust input directories, or generated firmware to
the root Cargo workspace during this migration.

Do not change target dependency topology solely to make the filesystem aesthetically uniform.

---

# 14. Implementation stages

## Stage 0 — inventory and dependency classification

Before moving code:

1. Enumerate every file under `tools/app-builder-v3/xtask/src`.
2. For every file, record:
   - physical owner/destination;
   - exact crate that compiles it;
   - host-side vs target-side;
   - generic vs STM32F4-specific;
   - board-specific vs reusable;
   - authoring declaration vs emitted target body vs runtime implementation;
   - whether its source is emitted into generated firmware;
   - runtime mechanism dependency;
   - direct dependencies;
   - generated-source path assumptions.
3. Identify the authoritative existing configuration behavior that will map
   into app-owned `live_config.rs`; record explicitly that current V3 has no
   separate file.
4. Produce a migration table in the PR/working notes.

Do not start with broad `git mv` operations until classification is complete.

---

## Stage 1 — mainline App Builder shell

Create:

```text
tools/app-builder/
```

Move only the host-side generic App Builder infrastructure first:

```text
main.rs
lib.rs
generator.rs
input_catalog.rs
rtic/*
```

Keep behavior unchanged.

Add only the explicit external-input registrations required for the current
V3 graph. Do not add glob discovery, runtime Rust loading, or a new input
crate.

Make `cargo check` / `cargo test` for the host builder pass before proceeding.

Remove the extra prototype-only `xtask/` nesting.

The resulting tool should conceptually be:

```text
tools/app-builder/
├── Cargo.toml
└── src/
```

not:

```text
tools/app-builder/xtask/
```

---

## Stage 2 — STM32F4 App Builder backend

Create:

```text
tools/app-builder/src/backends/stm32f4/
```

Move host-only hardware declaration/resolution/lowering logic from:

```text
hardware_definitions/stm32f4/
```

into this backend.

Do not move target runtime mechanisms here.

Keep STM32F4 lowering behavior unchanged.

Verify generated source against the pre-migration V3 output.

---

## Stage 3 — reusable boards

Create:

```text
boards/foxeer-f405-v2/
```

Move the physical board declaration out of V3 `target/`.

Register it through `src/input_catalog.rs` and update imports so the same
declaration can be consumed by more than one app.

Do not embed app-specific service assignments in the board.

Add focused board declaration validation tests.

---

## Stage 4 — reusable software task library

Create the top-level reusable task library:

```text
tasks/
```

Move V3 HAL-independent reusable task definitions there.

Do not duplicate reusable implementation already provided by `crates/ferrowasp-tasks`.

Where appropriate:

```text
task wrapper/body
    -> calls reusable implementation crate
```

Maintain V3 source-extraction compatibility.

Register each moved task explicitly in `src/input_catalog.rs`. Classification
must use imports and contracts: for example, a task that calls an STM32F4 USB
helper is not portable merely because it currently lives under V3 `tasks/`.

Run all task host tests.

---

## Stage 5 — STM32F4 task boundary

Move current host-compiled STM32F4 task declarations, contracts, and lowering
recipes to:

```text
tools/app-builder/src/backends/stm32f4/tasks/
```

Move code to `crates/ferrowasp-stm32f4/src/tasks/` only when Stage 0 proves it
is a target-runtime mechanism that compiles without App Builder macros,
contracts, or authoring contexts.

Avoid duplicate names and implementations. If a current file combines both
roles, leave it host-side for the filesystem migration unless a strict split
can preserve generated source and runtime behavior exactly.

Example:

```text
ferrowasp-stm32f4/src/serial/...
    runtime mechanism/state

tools/app-builder/src/backends/stm32f4/tasks/serial_rx_dma_irq.rs
    host RTIC task declaration/body authoring
```

---

## Stage 6 — complete app composition

Move the V3 complete application composition and app-specific platform
configuration into the appropriate app directory. Add the sibling
`live_config.rs` boundary only by relocating or adapting identified existing
configuration behavior without changing it.

Do not split the application into a reusable “flight core” as part of this migration.

Do not add a `builds/` layer.

The app owns the complete topology.

The app explicitly selects the board declaration.

`app_composition.rs`, `platform_config.rs`, and `live_config.rs` are registered
explicitly by the host builder and remain outside the embedded target's module
tree.

---

## Stage 7 — generated firmware relocation

Relocate generated output from:

```text
tools/app-builder-v3/generated/
```

to the app-owned generated output location.

Update generator paths.

Before adopting `apps/<app>/generated/`, prove the existing isolated app and
the nested generated package can coexist without workspace capture, PAC/HAL
feature unification, lockfile ambiguity, or incorrect relative paths. Use the
documented app-associated fallback if that proof fails.

Verify:

- generated `main.rs`;
- generated `prelude.rs`;
- generated `platform_config.rs`;
- generated `live_config.rs`;
- `SAFETY_SPINE.md`;
- Cargo metadata;
- target config.

Output must remain deterministic.

---

## Stage 8 — remove V3 prototype shell

Only after the mainline paths build and generate correctly, all protected V3
and existing canonical-builder capabilities pass through the consolidated
workflow, and the user approves the destructive cleanup:

remove/archive:

```text
tools/app-builder-v3/
```

Do not remove the old `tools/rtic-app-builder/` in the same change unless explicitly requested.

That older implementation is a separate cleanup decision.

This filesystem plan does not by itself establish mainline capability parity.
The checkpoints in `MAINLINE_MIGRATION_IMPLEMENTATION_PLAN.md` remain
authoritative for preserved CLI, manifests, transactional generation,
provenance, fixtures, CI integration, and any builder removal.

---

# 15. Required semantic invariants

This migration must not alter these behaviors.

## Application semantics

Preserve:

- selected board hardware;
- task set;
- task names;
- priorities;
- interrupt ownership;
- resource ownership;
- local/shared bindings;
- config values;
- spawns;
- init spawns;
- safety classifications;
- state schemas;
- safety-channel producer/consumer topology;
- queue capacities;
- timing/cadence;
- platform service routing;
- live-configuration defaults, bounds, schema behavior, write policy, and
  rejection/failure behavior.

## Safety

Preserve both current actuator inhibition gates.

Do not enable physical output.

Do not change:

- actuator authority;
- safety master behavior;
- pre-arm ordering;
- motor command path;
- DShot output gating;
- telemetry qualification;
- stale-data behavior;
- watchdog behavior.

This is a filesystem/ownership migration, not a functional actuator change.

## Board facts

Do not alter:

- Foxeer pin assignments;
- DMA routes;
- timer assignments;
- sensor orientation;
- SPI wiring;
- ADC channels;
- DShot lane mapping;
- serial peripheral selection;
- clock settings.

A move is not permission to “clean up” hardware mappings.

---

# 16. Generated-source equivalence

Before and after migration, compare generated output.

The preferred acceptance check is:

1. Generate with the original V3 tree.
2. Save generated files as baseline.
3. Generate from the migrated mainline tree.
4. Format both with the same `rustfmt`.
5. Diff:
   - `main.rs`;
   - `prelude.rs`;
   - `platform_config.rs`;
   - `live_config.rs` when comparing against its identified authoritative
     pre-migration source behavior;
   - `SAFETY_SPINE.md`.

Expected result:

```text
no semantic difference
```

Path/header differences are acceptable only where unavoidable and must be explained.

Current V3 has no generated `live_config.rs`, so its acceptance evidence is a
traceable behavior/schema comparison to the authoritative existing
configuration source plus deterministic repeated-generation hashes. Do not
claim a byte-for-byte V3 file match that cannot exist.

Any task/resource/priority/channel/interrupt difference is a migration defect unless explicitly approved.

---

# 17. Verification gates

At each logical stage, run the narrowest relevant checks.

## Root/shared code

```bash
cargo fmt --all --check
cargo check --workspace --locked
cargo test --workspace --locked
```

Run strict Clippy for the changed workspace when the stage reaches mainline
acceptance:

```bash
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Intermediate filesystem stages may record the V3 baseline's known strict
Clippy failures, but final mainline integration must resolve them rather than
waive the warning gate.

## App Builder host tool

From its workspace/package:

```bash
cargo check --locked
cargo test --locked
```

Then:

```bash
cargo run --locked -- check
cargo run --locked -- generate
```

or the equivalent package-qualified command after mainlining.

## Generated firmware

Using the exact generated package:

```bash
cargo check --locked
```

for:

```text
thumbv7em-none-eabihf
```

when the target is installed.

Do not weaken target checks to make migration pass.

## Repository checks

Preserve existing repository checks including:

```bash
python tools/check_rtic_boundaries.py
python tools/check_repository_context.py
```

where applicable.

---

# 18. Tests to add or preserve

At minimum preserve/add coverage for:

### Board declaration

- Foxeer board validates.
- Duplicate resource claims are rejected.
- invalid DMA/pin/peripheral relationships fail resolution.

### Task source extraction

- every external Rust input is explicitly registered once;
- missing or duplicate registrations fail clearly;
- reusable task source can be found after relocation;
- `file!()` identifies the relocated source rather than the registry;
- exactly one matching reusable task body is located;
- source rewrite remains deterministic.

### App composition

- complete composition validates against Foxeer board;
- invalid/missing board resource fails clearly;
- wrong resource type fails;
- duplicate ownership fails;
- invalid interrupt/resource binding fails;
- invalid safety-channel ownership fails;
- `platform_config.rs` and `live_config.rs` are loaded as sibling app inputs;
- live-configuration values preserve their bounds, defaults, write policy,
  and fail-closed validation behavior.

### Backend

- pin parsing;
- AF validation;
- DMA route resolution;
- interrupt derivation;
- timer capability lookup;
- generated HAL/PAC spelling.

### Generation

- output parses as Rust;
- rustfmt is idempotent;
- generated firmware `cargo check` succeeds;
- generated platform and live configuration are deterministic;
- safety spine is deterministic.

---

# 19. Naming guidance

Prefer names that expose architectural ownership.

Good:

```text
boards/
tasks/
crates/ferrowasp-stm32f4/src/tasks/
tools/app-builder/src/backends/stm32f4/
apps/<app>/app_composition.rs
apps/<app>/platform_config.rs
apps/<app>/live_config.rs
```

Avoid ambiguous names such as:

```text
hardware/
common/
helpers/
platform/
target/
misc/
```

unless the contained responsibility is genuinely clear from surrounding context.

Especially avoid using `target` as a source-domain name because Cargo already uses `target/` for build artifacts.

---

# 20. Documentation updates

Update repository guidance in the same checkpoint that activates the new
top-level `boards/`, `tasks/`, or app-authoring boundaries. Do not let agents
operate against stale ownership instructions during an intermediate state.

The current repository instructions state that board facts live under each app's `src/board/`.

That becomes stale if top-level reusable `boards/` is adopted.

Update at least:

```text
AGENTS.md
apps/AGENTS.md
tools/AGENTS.md
project_meta/CODEX_PROJECT_CONTEXT.md
mdbook architecture/developer documentation as applicable
```

New intended boundary:

```text
ferrowasp-core              pure reusable types/safety primitives
ferrowasp-drivers           protocols/devices
ferrowasp-stm32f4           target-side STM32F4 runtime mechanisms
tasks/                      reusable HAL-independent RTIC software tasks
boards/                     reusable physical board declarations
apps/                       complete App Builder compositions
tools/app-builder           host-side model/resolver/renderer
tools/app-builder/backends  host-side MCU/HAL resolution/lowering + task authoring
```

Do not leave old “board facts live under app src/board” instructions active after migration.

---

# 21. Non-goals

Do not use this migration to:

- redesign the task macro;
- convert `reusable_task!` into a proc macro;
- replace source-span rewriting with AST-native generation;
- redesign safety channels;
- redesign task state schemas;
- change task priorities;
- modify the control loop;
- change protocols;
- enable actuator output;
- change Foxeer board wiring;
- generalize immediately to STM32H7;
- split App Builder into many Cargo crates without a demonstrated need;
- rewrite existing reusable runtime crates;
- redesign platform configuration;
- invent or redesign live-configuration semantics;
- rename large portions of the public API solely for aesthetics.

Those can be separate follow-up changes.

---

# 22. Deliverables

The migration is complete when:

1. `tools/app-builder-v3/` is no longer the active implementation.
2. Mainline App Builder lives under:

   ```text
   tools/app-builder/
   ```

3. Reusable HAL-independent software tasks are no longer owned by the App Builder.
4. STM32F4 runtime task mechanisms are owned by:

   ```text
   crates/ferrowasp-stm32f4/
   ```

   Host-compiled STM32F4 task declarations remain under the builder backend
   and do not create a reverse runtime dependency.

5. STM32F4 App Builder resolution/lowering is clearly separate under:

   ```text
   tools/app-builder/src/backends/stm32f4/
   ```

6. Foxeer physical board facts are reusable independently of one app.
7. Complete app composition remains complete, explicitly selects its board,
   and keeps `platform_config.rs` beside `live_config.rs`.
8. No extra `builds/`/`targets/` source layer has been introduced.
9. Generated firmware lives with or is explicitly associated with its app rather than the builder implementation.
10. Pre/post migration generated RTIC semantics match, and generated live
    configuration is traceably equivalent to its identified authoritative
    source behavior.
11. Host-side App Builder tests pass.
12. Generated firmware `cargo check` passes when target tooling is available.
13. Repository documentation reflects the new ownership boundaries.
14. No actuator/safety/runtime behavior changes are introduced.

---

# 23. Suggested final conceptual model

```text
                    ┌─────────────────────────┐
                    │ reusable SW tasks       │
                    │ tasks/                  │
                    └────────────┬────────────┘
                                 │
                                 │
┌────────────────────┐           │
│ reusable board     │           │
│ boards/foxeer...   │           │
└──────────┬─────────┘           │
           │                     │
           ▼                     ▼
      ┌──────────────────────────────┐
      │ complete AppComposition      │
      │ apps/<app>/                  │
      └──────────────┬───────────────┘
                     │
                     ▼
      ┌──────────────────────────────┐
      │ App Builder core             │
      │ tools/app-builder/           │
      └──────────────┬───────────────┘
                     │
                     ▼
      ┌──────────────────────────────┐
      │ STM32F4 builder backend      │
      │ backends/stm32f4/           │
      │ pin/DMA/IRQ/lowering         │
      └──────────────┬───────────────┘
                     │
                     ▼
      ┌──────────────────────────────┐
      │ generated RTIC application  │
      └──────────────┬───────────────┘
                     │
                     ▼
      ┌──────────────────────────────┐
      │ STM32F4 target runtime       │
      │ ferrowasp-stm32f4            │
      │ HAL helpers + task mechanisms│
      └──────────────────────────────┘
```

The key architectural rule is:

> **The App Builder knows how to resolve hardware. The HAL crate knows how to operate hardware. Boards describe what hardware exists. Apps describe the complete firmware. Reusable software tasks live outside all four.**
