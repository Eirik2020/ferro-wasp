# Checkpoint 0 Baseline and Decision Brief

Status: baseline complete; path decision deferred pending filesystem approval

Captured: `2026-08-15T23:03:35Z`

This is the detailed evidence artifact for Checkpoint 0 of
`MAINLINE_MIGRATION_IMPLEMENTATION_PLAN.md`. It records the pre-migration
state and presents the architectural choice. It does not authorize a move,
rename, workspace-layout change, output enablement, or deletion.

## Repository identity and preserved work

- Branch: `tool/app-builder`
- Commit: `a994d751cfe0c2a2525b2cb34583e496925bf3b3`
- Tracked canonical-builder tree:
  `8230539a91c57ca841c3c451cc80eb7cc2a4c963`
- Tracked V3 tree: `746805b235fa7c5d84215b52490a5a4d670da671`
- Canonical golden-fixture tree:
  `3965a5530081e7d5424a8fc47af13ed21cc0d549`

Before this artifact was added, the complete worktree delta was three modified
files and two untracked files. All five belong to bootstrapping this migration
plan:

- modified: root `AGENTS.md`, `project_meta/DOCUMENT_REGISTRY.json`, and
  `tools/tests/test_repository_context.py`;
- untracked: this workspace's `AGENTS.md` and
  `MAINLINE_MIGRATION_IMPLEMENTATION_PLAN.md`.

Neither builder's tracked implementation or generated candidate was locally
modified. Cargo used the repository-configured cache outside the repository;
the initial sandboxed checks could not write there, and the unchanged commands
were rerun with filesystem approval. No probe, flash, actuator, or target
hardware command was run.

## Dependency and output identities

| Input or output | SHA-256 |
|---|---|
| Canonical `Cargo.lock` | `718be68d974d7724541cd6513a41205d6a97571da78bb0c4f11a1f6b35d26eeb` |
| Canonical toolchain | `1b8c3a1f2250e7957a51844f4c6b859f2c011437d182913eb2e3fd63e6e611ba` |
| Canonical OSD compatibility lock | `3d91054d731a3b2a03419d5f350aa0c8b420ecd51bd30e8d639b1775d0c06f91` |
| Canonical blinky golden lock | `39fd7c371c39be3f5b8cfab5707edb528c6ecc2201e8e21114031bd9698bd941` |
| V3 `Cargo.lock` | `de6a6aa00945e7b1c39d91f2781d77548f0fa697edd6701efdeb40ae39fe6295` |
| V3 generated `Cargo.lock` | `36c5c5d77b96e70a1f3afaca72b054f1260fa072410b2c6d2444483483b0a9a3` |
| V3 generated `Cargo.toml` | `a4217d3d72d6ef609006e4018ae5a73375e6d9f27ecc9445d54c4ba02ab1801b` |
| V3 generated target config | `20e70b491df107286ac9c07ed5c474f7ab114e4163434d9bae62071e362964c3` |
| V3 generated `main.rs` | `1bffafd849c352184e16a1b89998692ab7bc1b2bd95e73e9b6ded931aaec128c` |
| V3 generated `prelude.rs` | `59fd44d4045e5fa1cb56101fc7d24280ab658d9846ebf333c1c76c191108a297` |
| V3 generated `platform_config.rs` | `ddeae1e7b24df9bf64c04ed5837471c1ef1086f1bf166d78ab4e8eead9747cbe` |
| V3 generated `SAFETY_SPINE.md` | `3070863d2fa8bb784b02ce0e5e8ffff920b20e204542b2e10e7de16e9c56ea5c` |

The canonical builder had no retained `generated/` candidate at capture time;
its checked golden fixtures and test suite are the reproducible baseline.

## Verification results

| Workspace | Command | Result |
|---|---|---|
| Canonical | `cargo fmt --all --check` | pass |
| Canonical | `cargo check -p xtask --locked` | pass |
| Canonical | `cargo test -p xtask --locked` | pass: 61 tests; one subprocess helper intentionally ignored |
| Canonical | `cargo check --manifest-path compat/ferrowasp-serial-osd/Cargo.toml --tests --locked` | pass |
| Canonical | `cargo clippy -p xtask --all-targets --locked -- -D warnings` | pass |
| V3 | `cargo fmt --all --check` | pass |
| V3 | `cargo check --workspace --locked` | pass; dependency future-incompatibility warning retained |
| V3 | `cargo test --workspace --locked --quiet` | pass: 88 unit/integration tests and 3 doctests |
| V3 | `cargo run -p xtask --locked -- check` | pass: selected application resolves and renders |
| Generated V3 | `cargo check --locked --quiet` | pass for `thumbv7em-none-eabihf` |
| V3 | `cargo clippy --workspace --all-targets --locked -- -D warnings` | expected baseline failure: 11 warnings promoted to errors |

The V3 strict-Clippy failures are eight `result_unit_err` diagnostics in
`golden_service_authoring.rs`, two `drop_non_drop` diagnostics in safety/flash
task authoring, and one `collapsible_if` diagnostic in flash task authoring.
They are a later CI-integration gate, not a failure to capture this baseline.

## Capability map

| Capability | Canonical `tools/rtic-app-builder` | `tools/app-builder-v3` | Migration requirement |
|---|---|---|---|
| Workspace/toolchain | Isolated workspace; pinned nightly and Thumb target | Isolated workspace; Rust 1.85 declared but no local toolchain pin | Retain isolation and one reviewed pin |
| Inputs | Strict BSP, application, architecture-contract, feature, template, and backend inputs | Rust-authored board, platform configuration, and one `APP_COMPOSITION` | Feed the typed model from strict versioned inputs |
| Resolved model | Executable task/transport/timing/fault contracts, but feature-specific composition remains | General typed resources, endpoints, components, tasks, state recipes, channels, timing, safety scope, and RTIC graph | Adopt V3's typed graph without weakening canonical contracts |
| Target breadth | NUCLEO-F401RE blinky and OSD validation apps | One Foxeer F405 V2 composition | Preserve both NUCLEO fixtures and add Foxeer explicitly |
| Renderer | Central template plus feature fragments and validated markers | One architecture-aware `syn`/`quote` renderer producing three Rust files and a report | Converge on one architecture-aware renderer |
| Safety treatment | Explicit safety scope; NUCLEO examples cannot gain flight authority | Typed safety channels, sole actuator path, source-pinned reconciliation, and two output-inhibition gates | Preserve authority and regression-test default inhibition |
| CLI | `generate`, `build`, `flash`, `embed`, and `clean`; explicit `--app`; strict `--resume` | Positional `generate` or `check`; one compile-time-selected app | Preserve scriptable canonical commands and add typed-model selection/checking |
| Mutation/recovery | Pre-mutation validation, per-app lock, staged candidates, retained failures, atomic promotion/rollback, fingerprinted resume | Resolve/render first, then write four fixed files in place if changed | Keep the canonical transaction/checkpoint lifecycle |
| Generated validation | Rust parse/format, incremental embedded checks, final locked release link | Host resolve/render checks and a separately invoked generated Thumb check | Integrate generated check and release link into the canonical lifecycle |
| Provenance | Build state, input fingerprints, command diagnostics, and build metadata | Deterministic human safety-spine and pinned semantic comparison; no machine build provenance | Combine both evidence forms |
| CI/repository routing | Named as canonical by instructions and environment tooling, but not directly exercised by current CI | Not named by current CI or environment tooling | Route one canonical builder and add strict CI gates |
| Release authority | No Foxeer release candidate | Generated Foxeer package remains forced unarmed and output-inhibited | Keep handwritten app authoritative until separate cutover gates pass |

## Architectural choices

### Option A — Retain the canonical path and port V3 into it (recommended)

Keep `tools/rtic-app-builder` as the supported isolated workspace. Introduce
V3's typed resolved model, STM32F4 definitions, reusable task/component
authoring, Foxeer declarations, renderer, and reconciliation report inside
that workspace. Adapt its inputs to the canonical strict manifest/catalog and
retain the canonical CLI, staging, fingerprints, diagnostics, and fixtures.
Keep V3 intact as a comparison source until parity is demonstrated, then ask
before archival or removal.

Benefits: smallest routing and environment change, preserves the mature
transaction workflow, keeps the established validation apps, and avoids
teaching two builders the same lifecycle. Cost: a deliberate adapter layer is
needed while the strict manifests are lifted into the richer typed model.

### Option B — Promote the V3 path and port the canonical workflow into it

Make `tools/app-builder-v3` the supported workspace, then move or recreate the
canonical manifests, contracts, CLI, transaction/checkpoint implementation,
fixtures, toolchain, and environment routing there. Retire the old path only
after parity.

Benefits: the richer model keeps its current layout. Cost: broader path,
documentation, cache, CI, and environment churn; higher risk of accidentally
dropping mature checkpoint behavior; and a second relocation after the model
work is already complete.

## Provisional Option A migration map

This map is for review only and becomes accepted evidence only if the user
selects Option A.

| V3 source | Canonical destination/responsibility |
|---|---|
| `xtask/src/rtic/` | Typed resolved-application model, validation, rendering, and reports under canonical `xtask/src/` |
| `xtask/src/hardware_definitions/` | Versioned component/backend recipes selected by strict inputs; no implicit allocation |
| `xtask/src/tasks/` | Reusable component/task authoring selected by reviewed metadata |
| `xtask/src/target/board.rs` and `platform_config.rs` | Strict Foxeer BSP and boot-frozen platform-configuration inputs plus backend lowering |
| `xtask/src/target/app_composition.rs` | Strict Foxeer application/contract inputs and resolved fixture |
| `xtask/src/target/golden_reconciliation.rs` | Pinned semantic reconciliation and deterministic architecture/safety report |
| `xtask/src/generator.rs` | Canonical assembler/render stages, preserving validation, staging, promotion, and failure retention |
| V3 `generated/` | Disposable per-application canonical generated checkpoint; never hand-edited source |
| Canonical `manifest.rs`, `architecture.rs`, `cli.rs`, `state.rs`, `runner.rs`, `syntax.rs`, and `assembler.rs` | Preserved workflow and gradually adapted to consume the typed graph |
| Canonical NUCLEO manifests/contracts/golden tests | Mandatory regression fixtures through the consolidated pipeline |

## Decision required

The user subsequently directed that V3's current filesystem is not final and
that the final internal layout must be worked out first. The path options and
provisional Option A map above remain comparison evidence, but they are not
ready for selection. Review and approve `CHECKPOINT_0_TARGET_LAYOUT.md` before
returning to the canonical-root decision or accepting a file-level map.
