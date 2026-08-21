# Checkpoint 0 Proposed Final Builder Filesystem

Status: superseded by [`filesystem_ref.md`](filesystem_ref.md)

Date: `2026-08-16`

The user selected the repository-integrated filesystem in
`filesystem_ref.md` and approved its review corrections on `2026-08-16`.
This earlier builder-internal/catalog-first proposal is retained only as
decision provenance. Do not use it as the migration destination or active
Checkpoint 0 decision detail.

This document defines a path-neutral target filesystem for the consolidated
app builder. `<builder-root>` deliberately does not yet mean either
`tools/rtic-app-builder` or `tools/app-builder-v3`. Choosing the permanent root
before approving the internal layout would allow the current prototype shape
to decide the architecture.

This proposal changes no crate layout, public CLI, manifest format, hardware
assignment, safety state, or actuator gate. It is a design input to Checkpoint
0, not authorization to move files.

## Problems the final layout must remove

The current canonical builder has the stronger execution lifecycle, but its
`feature-library/` combines component metadata with source-injecting Rust
fragments and feature-specific dependency templates. Its typed contracts are
separate from the general composition model it still needs.

V3 has the stronger resolved model, but its current Rust tree mixes several
different ownership boundaries:

- `target/` contains the selected board, application, platform configuration,
  and golden reconciliation in Rust;
- `hardware_definitions/stm32f4/` contains physical types, endpoint
  declarations, backend lowering, service authoring, and hardware tasks;
- `tasks/` contains portable task/component authoring;
- `rtic/` contains the resolved model, resolution, rendering, and reporting;
- `generator.rs` selects one global `APP_COMPOSITION` and writes a fixed
  generated crate in place.

The source module name `target/` is also easily confused with Cargo build
output and broad tooling globs. The final source tree should not retain it.

## Filesystem design rules

1. Authored declarative inputs, builder implementation, checked fixtures,
   disposable outputs, compatibility adapters, and documentation have
   separate roots.
2. Physical board facts live only in BSP inputs. Applications select stable
   BSP resource IDs but do not redeclare pins, DMA, interrupts, or timers.
3. External component metadata is strict and versioned. It can select a
   reviewed implementation recipe by stable ID but cannot contain Rust source
   or arbitrary constructor expressions.
4. The resolved model is MCU-neutral where practical and contains no HAL/PAC
   types. Backend modules own MCU/HAL lowering, initialization recipes, target
   triples, linker facts, and physical endpoint realization.
5. Reusable flight behavior remains in canonical FerroWasp crates. Builder
   component modules provide declarations and bounded RTIC wiring around
   reviewed APIs; they do not become another home for flight algorithms.
6. One resolver produces one validated resolved application before rendering.
   One renderer owns RTIC source composition. Permanent feature-specific Rust
   fragment assembly is not part of the final layout.
7. Checked golden fixtures live under `tests/`; generated working and failed
   candidates live under ignored `generated/`. A generated crate is never a
   second source of truth.
8. Operational lifecycle code is separate from input parsing, semantic
   resolution, backend lowering, and rendering so transaction/recovery logic
   cannot obscure architecture decisions.
9. The builder remains an isolated Cargo workspace. Its generated firmware
   and dependency selection do not enter the FerroWasp root workspace.
10. Migration-control documents are temporary. The implementation plan and
    checkpoint briefs remain until final user review, then follow the plan's
    explicit deletion protocol.

## Proposed final tree

```text
<builder-root>/
├── AGENTS.md
├── README.md
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml
├── .cargo/
│   └── config.toml
├── catalog/
│   ├── bsp/
│   │   ├── nucleo-f401re.toml
│   │   └── foxeer-f405-v2.toml
│   ├── applications/
│   │   ├── nucleo-f401re-blinky.toml
│   │   ├── nucleo-f401re-osd.toml
│   │   └── foxeer-f405-v2.toml
│   ├── architecture-contracts/
│   │   ├── nucleo-f401re-blinky.toml
│   │   ├── nucleo-f401re-osd.toml
│   │   └── foxeer-f405-v2.toml
│   ├── components/
│   │   ├── portable/
│   │   └── stm32f4/
│   ├── build-policies/
│   └── reconciliations/
│       └── foxeer-f405-v2.toml
├── xtask/
│   ├── Cargo.toml
│   ├── src/
│   │   ├── main.rs
│   │   ├── lib.rs
│   │   ├── cli.rs
│   │   ├── input/
│   │   ├── model/
│   │   ├── resolve/
│   │   ├── components/
│   │   │   ├── portable/
│   │   │   └── stm32f4/
│   │   ├── backends/
│   │   │   └── stm32f4/
│   │   ├── render/
│   │   ├── pipeline/
│   │   └── reconcile/
│   └── tests/
├── templates/
│   └── crate/
├── tests/
│   ├── fixtures/
│   │   ├── valid/
│   │   └── invalid/
│   ├── golden/
│   └── expected-reports/
├── compat/
├── docs/
│   ├── authoring/
│   └── archive/
└── generated/
    └── <application>/
        ├── working/
        │   ├── Cargo.toml
        │   ├── Cargo.lock
        │   ├── build.rs
        │   ├── memory.x
        │   ├── src/
        │   ├── reports/
        │   ├── artifacts/
        │   ├── assembler-state.toml
        │   └── build-metadata.toml
        └── failed/
            └── <bounded-attempt-id>/
```

Directory entries appear only when needed. For example, compatibility code is
not required to remain after its callers migrate, and a generated application
may omit `build.rs` only when its selected backend explicitly does not require
one.

## Ownership by directory

### `catalog/`

`catalog/` is the complete human-authored declarative input boundary. Grouping
these files under one root makes input discovery, confinement, canonical
hashing, review, and schema versioning explicit.

- `bsp/` owns immutable physical facts and stable physical resource IDs.
- `applications/` selects a BSP, components, instances, logical bindings,
  priorities, capacities, periods, features, and build policy.
- `architecture-contracts/` declares required task forms, transports, timing,
  safety scope, faults, mechanisms, and invariants independently of renderer
  implementation.
- `components/` declares typed ports, capabilities, ownership, multiplicity,
  configuration, safety classification, failure behavior, and one reviewed
  implementation-recipe ID. It contains no Rust fragments.
- `build-policies/` declares target, profile, allowed features, locked-build
  requirements, and artifact policy without embedding shell commands.
- `reconciliations/` pins an authoritative comparison identity, comparison
  scope, and reviewed deviations. The generic comparison implementation stays
  in Rust.

Stable IDs, not relative file paths, form cross-file references. Paths remain
fingerprinted inputs but are not runtime identities.

### `xtask/src/input/`

Owns strict schema types, version checks, catalog lookup, confined path
resolution, complete-path diagnostics, and deserialization. It does not
allocate resources or construct RTIC source.

### `xtask/src/model/`

Owns the typed unresolved and resolved application vocabulary: IDs,
components, endpoints, ports, tasks, task forms, resources, interrupts,
transports, state recipes, timing, faults, safety classes, platform
configuration, dependency requirements, and provenance identities. It does
not own file I/O, command execution, or HAL/PAC types.

### `xtask/src/resolve/`

Owns deterministic composition passes: component expansion, capability
matching, ownership, physical claims, initialization order, priorities and
ceilings, queue topology, timing, dispatchers, dependency closure, safety
authority, and final graph validation. It produces a complete model or a
bounded diagnostic; it never writes generated output.

### `xtask/src/components/`

Owns reviewed component registries and wiring recipes selected by stable
component IDs. `portable/` covers platform-independent services and glue;
`stm32f4/` contains components whose semantics are specific to that MCU family
but which do not own general backend policy. Component code may reference
canonical FerroWasp APIs and render bounded wiring. It must not duplicate
flight algorithms or accept arbitrary source from catalog metadata.

### `xtask/src/backends/`

Owns target/compiler/HAL selection, MCU profiles, pins, alternate functions,
DMA direction and compatibility, timer/channel realization, interrupt names,
electrical initialization policy, linker memory translation, and backend task
recipes. The backend translates explicit claims; it never silently allocates
hardware.

### `xtask/src/render/`

Owns deterministic rendering of crate metadata, linker/build files, RTIC Rust,
platform configuration, architecture/safety reports, and machine-readable
provenance. `templates/crate/` may contain only stable crate skeleton material;
task and feature-specific `.rs.tpl` fragment assembly is retired.

### `xtask/src/pipeline/`

Owns orchestration after inputs and graph semantics are defined: application
locking, fingerprints, staging, command construction/execution, diagnostics,
failed-candidate retention, resume, final release link, promotion, artifact
copying, and confined clean operations. It consumes the resolver and renderer;
it does not contain component semantics.

### `xtask/src/reconcile/`

Owns generic semantic comparison and reviewed-deviation validation. Foxeer
reference pins and accepted deviations are data in `catalog/reconciliations/`,
not a hard-coded selected-target module.

### `tests/`, `compat/`, and `generated/`

- `tests/fixtures/` holds checked valid and invalid inputs.
- `tests/golden/` holds the minimal tracked expected generated applications.
- `tests/expected-reports/` holds deterministic semantic/report fixtures.
- `xtask/tests/` holds Rust integration tests using those fixtures.
- `compat/` holds narrow buildable adapters with an explicit removal gate.
- `generated/<application>/working/` is the last promoted complete candidate.
- `generated/<application>/failed/` retains bounded, uniquely identified failed
  candidates and diagnostics. Cargo build products stay in the configured
  external target cache; reviewed release artifacts are copied into the
  candidate's `artifacts/` directory.

## Current-to-final mapping

| Current source | Proposed final responsibility |
|---|---|
| Canonical `bsp/` | `catalog/bsp/` |
| Canonical `applications/` | `catalog/applications/` |
| Canonical `architecture-contracts/` | `catalog/architecture-contracts/` |
| Canonical `feature-library/*/feature.toml` | Split into strict `catalog/components/` declarations and application selections |
| Canonical feature `*.rs.tpl` files | Replaced by reviewed component/backend Rust recipes and the single renderer |
| Canonical crate/linker templates | Restricted to `templates/crate/` |
| Canonical `manifest.rs` and validation | `xtask/src/input/` plus semantic passes in `resolve/` |
| Canonical `architecture.rs` | Typed `model/` declarations and `resolve/` validation |
| Canonical `assembler.rs`, `state.rs`, `runner.rs`, and `diagnostics.rs` | `xtask/src/pipeline/` |
| V3 `rtic/` declarations | `xtask/src/model/` |
| V3 `rtic/resolve.rs` | `xtask/src/resolve/` |
| V3 `rtic/render.rs` and `report.rs` | `xtask/src/render/` |
| V3 `hardware_definitions/stm32f4/` | Split by responsibility between `components/stm32f4/` and `backends/stm32f4/` |
| V3 `tasks/` | `components/portable/`, limited to declaration/wiring recipes around canonical behavior |
| V3 `target/board.rs` | Strict `catalog/bsp/foxeer-f405-v2.toml` plus backend lowering |
| V3 `target/app_composition.rs` | Strict application, contract, component, and policy inputs |
| V3 `target/platform_config.rs` | Application bindings plus typed platform-configuration model/lowering |
| V3 `target/golden_reconciliation.rs` | Generic `reconcile/` code plus declarative Foxeer reconciliation input and expected-report fixture |
| V3 `generator.rs` | `pipeline/` orchestration calling input, resolve, render, check, and promotion stages |
| Current generated Foxeer crate | `generated/foxeer-f405-v2/working/` after a successful transactional promotion |

## Deliberate non-decisions

The filesystem proposal does not yet freeze the TOML schema, Rust public API,
exact file-per-component granularity, component IDs, or whether a small
component declaration can share a catalog file. Those are executable-schema
decisions for later checkpoints. It does freeze the ownership boundaries that
those decisions must respect.

It also does not choose the permanent `<builder-root>`. After this internal
layout is approved, the canonical-path options can be reevaluated by measuring
which current workspace reaches this tree with less risk and less temporary
duplication.

## Decisions requested

Please review these three structural choices:

1. Put every human-authored builder input below one `catalog/` root rather
   than keeping `bsp/`, `applications/`, and contracts at top level.
2. Replace source-injecting feature fragments with strict component metadata
   plus reviewed Rust component/backend recipes and one renderer.
3. Use the responsibility-oriented Rust modules `input`, `model`, `resolve`,
   `components`, `backends`, `render`, `pipeline`, and `reconcile`, with no
   selected-target Rust module.

Approval of this layout is required before choosing Option A or Option B and
before producing the accepted file-level migration map.
