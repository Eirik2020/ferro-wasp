# App Builder Mainline Migration Implementation Plan

Status: active

## Execution status

This is the routine continuation entry point. Read this section and the active
checkpoint section; the full-plan read conditions are defined in `AGENTS.md`.

- Plan-spec revision: `3` (`2026-08-16`). Progress-only edits do not change
  this revision.
- Active checkpoint: **Checkpoint 4 — Reproducible generation lifecycle**.
- Last completed checkpoint: **Checkpoint 3 — Typed inputs, catalog, and CLI**.
- Current completion boundary: **Mainline builder** (Checkpoints 0–6).
- Immediate objective: add interruption-safe staging/promotion, complete
  input fingerprints, retained failures, resume validation, and provenance.
- Active detail to load: this section and `## Checkpoint 4` only.
- Required focused context: `CHECKPOINT_3_INPUT_CLI_EVIDENCE.md` and the
  canonical generator/operations code plus the protected builder's lifecycle,
  state, runner, and diagnostics implementation being adapted.
- Current evidence: Checkpoints 0–3 pin the approved ownership boundaries,
  protected fixtures, canonical model, explicit three-app catalog, scriptable
  commands, locked release links, deterministic hashes, and output inhibition.
- Current blocker: generation still writes the working tree directly and has
  no complete fingerprint, retained failure, resumability, or provenance.

When a checkpoint closes, mark its checklist, replace its pending evidence
with bounded references, read the full plan once for the transition, and then
update this section to the next checkpoint. Keep this section short; do not
copy completed history into it.

## Purpose

This plan migrates the useful App Builder V3 work into one supported,
mainline RTIC app-builder workflow and, as a separately gated outcome, permits
a generated Foxeer F405 V2 application to replace the handwritten golden app.
It is an implementation plan, not evidence that an unchecked item exists,
passes on target, or is safe to operate.

There are two completion boundaries:

1. **Mainline builder:** one canonical, reproducible, input-driven builder is
   exercised by repository CI. Its generated Foxeer candidate remains
   output-inhibited.
2. **Foxeer firmware cutover:** the generated package achieves the required
   software and release parity, receives fresh exact-image target evidence,
   and deliberately replaces the handwritten release artifact.

Reaching the first boundary does not authorize the second. The handwritten
`apps/foxeer-f405-v2` application remains the authoritative flight image until
every applicable cutover checkpoint is complete.

## Starting point

The current V3 candidate already provides a typed component/resource/task
model, deterministic rendering, a source-pinned semantic reconciliation, and
an output-inhibited Foxeer graph. It generates `main.rs`, `prelude.rs`,
`platform_config.rs`, and `SAFETY_SPINE.md` for one compile-time-selected
composition.

V3 does not yet have a distinct authored or generated `live_config.rs`. The
approved filesystem requires it beside `platform_config.rs`, but its content
must be traced to existing authoritative configuration behavior before any
file is introduced.

The existing `tools/rtic-app-builder` remains the repository's canonical
builder location. It already provides strict BSP and application manifests,
architecture contracts, multiple NUCLEO fixtures, explicit application
selection, pre-mutation validation, checked checkpoints, retained failures,
fingerprint-gated resume, and final release linking. Those are migration
requirements, not disposable legacy behavior.

Known starting gaps include:

- V3 supports only the hard-coded `generate` and `check` commands;
- V3 is not part of the repository CI, documentation registry, or standard
  environment checks;
- generated Foxeer output is not the release/configurator artifact;
- the generated control path is forced unarmed and has two independent
  output-inhibition gates;
- intentional scheduling, routing, feature, dependency, and service
  differences remain to be accepted or corrected;
- historical handwritten-image bench and flight results are not evidence for
  a newly generated image.

## Token-efficient checkpoint discipline

- Work in checkpoint order. A later checkpoint may be investigated early, but
  it is not complete until all earlier exit criteria it depends on pass.
- Mark an item `[x]` only after recording reproducible evidence in that
  checkpoint's **Evidence** section. Record commands, relevant output, commit
  or dirty-tree identity, and artifact hashes where applicable.
- Keep each checkpoint's inline evidence at no more than 512 bytes. Store raw
  logs, lengthy diagnostics, reports, and run records in their owned artifact
  locations and link them. Replace `Pending` rather than appending a running
  transcript.
- Do not reload completed checkpoint sections during normal continuation.
  Reopen one only when a declared input changed, evidence is inconsistent, or
  a later exit criterion explicitly depends on its detail.
- Do not rerun an unaffected gate solely to rediscover its result. Reuse its
  recorded evidence only when fingerprinted inputs are unchanged; exact-image,
  target, checkpoint-transition, and final-verification requirements still
  require fresh execution where stated.
- Search for and load only the active heading and explicitly routed source
  sections. Historical archives remain targeted provenance, not default
  context.
- Preserve small, reviewable changes. Do not mix an architectural migration,
  safety-policy change, hardware enablement, and release cutover into one
  opaque patch.
- Strictly reject ambiguous or undeclared pins, peripherals, DMA streams,
  interrupts, timers, priorities, dispatchers, queue capacities, ownership,
  safety classification, and failure behavior. The builder translates
  declarations; it does not allocate hardware implicitly.
- Only the safety kernel and safety-owned actuator-output path may command
  motor hardware. Protocol, configuration, logging, generated glue, and host
  tooling never gain actuator authority.
- Keep both isolated builders out of the root Cargo workspace unless the user
  explicitly approves that architectural change.
- Before target activity, use `project_meta/testing/TEST_CATALOG.json` and the
  applicable procedure. The user operates hardware and must explicitly
  confirm powered or flight activity. Stop conditions override plan progress.

## Checkpoint 0 — Baseline and architectural decision

Goal: establish a reproducible baseline, approve the final filesystem and
canonical destination, and record the file-level migration map before moving
code or changing workspace structure.

- [x] Capture the branch, commit, dirty-tree summary, builder dependency
  locks, generated output hashes, and current test results for both builders.
- [x] Produce a capability map covering manifests, contracts, model/resolver,
  renderer, CLI, staging/checkpoints, provenance, fixtures, target support,
  build commands, and documentation ownership.
- [x] Identify local or untracked work that must be preserved before any move,
  rename, archive, or deletion.
- [x] Define and obtain explicit user approval for the path-neutral final
  filesystem and source-of-truth boundaries.
- [x] Reassess the migration choices against the approved layout and obtain
  explicit user approval for the canonical root and crate/workspace layout.
- [x] Record the accepted decision and a file-level migration map without
  claiming that the migration is implemented.

Exit criteria: the two baselines are reproducible, no user work is at risk,
and the user has approved the final internal filesystem and architectural
destination.

Evidence:

- [`CHECKPOINT_0_BASELINE.md`](CHECKPOINT_0_BASELINE.md) pins identities and
  checks. [`filesystem_ref.md`](filesystem_ref.md) is the approved architecture;
  [`CHECKPOINT_0_MIGRATION_MAP.md`](CHECKPOINT_0_MIGRATION_MAP.md) classifies
  all 76 V3 sources and the `live_config.rs` behavior boundary.

## Checkpoint 1 — Preserve regression behavior

Goal: turn current behavior into fixtures before consolidation changes it.

- [x] Preserve structural or golden-output coverage for V3's resolved Foxeer
  graph, generated Rust, platform configuration, and safety-spine report;
  separately pin the authoritative existing behavior that will become
  `live_config.rs`, since V3 has no pre-migration file to diff.
- [x] Preserve the existing NUCLEO blinky and OSD manifests, contracts,
  generated-output expectations, incremental checks, and release-link gates.
- [x] Add or retain negative tests for duplicate ownership, resource and DMA
  conflicts, interrupt conflicts, invalid priorities, missing capacities,
  invalid safety edges, unknown fields, and nondeterministic ordering.
- [x] Confirm repeated generation produces identical source and semantic
  report hashes from identical inputs.
- [x] Run and record each isolated workspace's focused baseline without
  modifying generated source by hand.

Exit criteria: failures introduced by consolidation can be distinguished from
pre-existing behavior, and all protected fixtures pass from clean generation.

Evidence:

- [`CHECKPOINT_1_REGRESSION_BASELINE.md`](CHECKPOINT_1_REGRESSION_BASELINE.md)
  pins fixtures, hashes, negative coverage, exact software/build results, and
  both NUCLEO release links. V3 retains only its 11 known Checkpoint 0 Clippy
  findings. No hardware ran and Foxeer remains output-inhibited.

## Checkpoint 2 — Consolidate the typed builder core

Goal: make one canonical implementation own resolution and rendering without
losing the established workflow.

- [x] Move or adapt V3's typed hardware, endpoint, component, task, channel,
  application-composition, validation, and RTIC rendering model into the
  approved canonical workspace.
- [x] Preserve one architecture-aware renderer rather than permanent
  feature-specific source generators.
- [x] Preserve strict contracts, deterministic names, explicit physical
  claims, typed/versioned mechanisms, bounded transports, and complete graph
  validation before rendering.
- [x] Keep HAL-specific implementation details out of portable component and
  task declarations.
- [x] Compile repository-owned `tasks/`, `boards/`, and app Rust inputs through
  one explicit `src/input_catalog.rs`; preserve `file!()`/source extraction and
  reject missing or duplicate registrations.
- [x] Keep host-compiled STM32F4 task declarations under the builder backend;
  move only independently compiled runtime mechanisms into
  `ferrowasp-stm32f4`, with no reverse dependency on builder macros or types.
- [x] Keep the generated Foxeer composition output-inhibited throughout this
  checkpoint and add a regression test that rejects accidental enablement.
- [x] Leave the noncanonical implementation available for comparison until
  all protected fixtures pass; do not archive or delete it yet.

Exit criteria: the approved canonical workspace can resolve and render the V3
typed model, while all Checkpoint 1 fixtures and safety-inhibition tests pass.

Evidence:

- [`CHECKPOINT_2_CONSOLIDATION_EVIDENCE.md`](CHECKPOINT_2_CONSOLIDATION_EVIDENCE.md)
  records the isolated canonical core, 39-entry catalog and source-path
  proofs, byte-identical generated hashes, fresh Thumb check, all 95 canonical
  tests, protected-builder regressions, repository gates, and unchanged 11
  deferred Clippy findings. No hardware ran; both output gates remain disabled.

## Checkpoint 3 — Typed inputs, catalog, and CLI

Goal: replace the hard-coded single composition with strict, selectable
builder inputs.

- [x] Preserve strict, versioned declarative manifests for the existing
  validation applications while using the explicit compile-time Rust input
  catalog for repository-owned V3 board, app, and task definitions. Never
  accept arbitrary CLI-provided Rust source.
- [x] Express the Foxeer board facts and application selection through those
  inputs while retaining stable resource and component identities.
- [x] Support explicit catalog selection for the Foxeer candidate and both
  NUCLEO validation applications.
- [x] Support scriptable `generate`, `check`, `build`, and `clean` commands
  with stable nonzero failure exits and an explicit application selector.
- [x] Preserve `flash` and `embed` only as explicit hardware commands. Merely
  implementing or testing their command construction does not authorize
  running them.
- [x] Reject unknown fields, incompatible schema versions, incomplete
  selections, ambiguous providers, and undeclared resource allocation before
  output mutation.
- [x] Cover multiple instances of the same component type and deterministic
  collision-free naming.

Exit criteria: all three applications are selected through explicit inputs
and use the same resolved-model and renderer pipeline without a hard-coded
global `APP_COMPOSITION` selection.

Evidence:

- [`CHECKPOINT_3_INPUT_CLI_EVIDENCE.md`](CHECKPOINT_3_INPUT_CLI_EVIDENCE.md)
  records the explicit three-app catalog, unified resolved/rendered API,
  strict validation, CLI/command tests, 151 passing tests, byte-stable
  fixtures, three locked release links, and unexecuted hardware commands.

## Checkpoint 4 — Reproducible generation lifecycle

Goal: restore and strengthen the operational guarantees required for
mainline automation.

- [ ] Validate all inputs and the complete architecture graph before changing
  the last successful generated checkpoint.
- [ ] Generate into a staging location, retain bounded diagnostics and failed
  candidates, and promote only a completely checked candidate.
- [ ] Fingerprint all declared inputs, relevant source/API identities,
  builder version, lockfile, toolchain, target, features, and build policy.
- [ ] Include authored/generated `platform_config.rs` and `live_config.rs` in
  deterministic fingerprints and drift checks; compare the latter to its
  identified authoritative behavior because V3 has no original file.
- [ ] Permit resume only when the fingerprint matches and the retained working
  checkpoint still passes its validation.
- [ ] Format and parse generated Rust, run the generated embedded
  `cargo check --locked`, and perform a final release link.
- [ ] Emit deterministic human-readable architecture/safety reconciliation
  and machine-readable provenance suitable for CI artifact retention.
- [ ] Add rollback, interrupted-run, stale-resume, invalid-input, failed-build,
  and repeat-generation tests.

Exit criteria: a failed or interrupted run cannot corrupt the last successful
candidate, and an identical clean run reproduces the declared source and
report outputs with attributable build provenance.

Evidence:

- Pending.

## Checkpoint 5 — Cross-application regression and semantic reconciliation

Goal: demonstrate that the result is a reusable app builder and explain every
material Foxeer difference.

- [ ] Generate, check, and release-link both NUCLEO validation applications
  through the consolidated pipeline.
- [ ] Generate, check, and release-link the output-inhibited Foxeer candidate
  through the same pipeline.
- [ ] Validate application-wide resource ownership, priority ceilings, shared
  locks, dispatchers, initialization order, queue topology, capacities,
  overflow policy, fault propagation, and bounded work.
- [ ] Reconcile the generated Foxeer graph against a pinned current
  handwritten source identity rather than a stale copied description.
- [ ] Correct or explicitly accept each difference in timer/monotonic use,
  UART DMA reservation, task priority and decomposition, safety scheduling,
  feature profiles, services, dependencies, and dispatchers.
- [ ] Require no unexplained difference in actuator authority, motor mapping,
  arming/failsafe behavior, command freshness, watchdog behavior, physical
  routes, or failure containment.

Exit criteria: all validation apps pass and the Foxeer report contains only
reviewed, intentional differences while both output gates remain disabled.

Evidence:

- Pending.

## Checkpoint 6 — Mainline builder integration

Goal: make the consolidated host builder the repository-supported path.

- [ ] Pin the supported toolchain and dependency lock behavior for the
  isolated builder workspace.
- [ ] Resolve all builder Clippy warnings and pass the repository-equivalent
  strict warning policy.
- [ ] Add CI gates for formatting, builder checks/tests, strict Clippy,
  deterministic regeneration/no drift, generated Thumb checks, and generated
  release linking.
- [ ] Publish the output-inhibited Foxeer result only as a clearly identified
  non-flight candidate artifact at this stage.
- [ ] Update `tools/AGENTS.md`, repository context/document routing,
  development-environment checks, and narrow builder documentation to name
  the canonical path and commands.
- [ ] Confirm root firmware dependency resolution remains unaffected by the
  isolated builder workspace.
- [ ] Make the patch reviewable and ensure all intended inputs, tests, locks,
  and generated-file policies are tracked. Commit, push, or open a pull
  request only when the user requests that external workflow.
- [ ] Archive or remove the duplicate builder only after capability parity is
  demonstrated and the user approves the destructive cleanup.

Exit criteria: repository CI treats one isolated path as the supported
builder, all host and generated software gates pass, documentation has no
competing canonical route, and the generated Foxeer artifact is still clearly
output-inhibited.

**Mainline-builder boundary:** this checkpoint completes migration of the
builder itself. It does not replace or enable the handwritten flight image.

Evidence:

- Pending.

## Checkpoint 7 — Generated Foxeer package and feature parity

Goal: prepare an output-inhibited generated package that can enter a separate
flight-image review without changing release authority.

- [ ] Match the required package/binary identity, target, linker memory
  configuration, build script, panic/logging configuration, and release
  profile expected by CI and the configurator.
- [ ] Inventory the handwritten release, diagnostic, fault-injection, bench,
  logging, configuration, and protocol features; implement required profiles
  or record explicit reviewed omissions.
- [ ] Preserve bounded USB/configurator compatibility, stored-configuration
  schema behavior, app-owned `live_config.rs`, log identities, and
  disarmed-only write policy without moving configuration authority into the
  board or host builder.
- [ ] Decide explicitly whether destructive flash scratch testing and MSPv2
  RPC are required for cutover; do not make optional parity an unexplained
  omission.
- [ ] Add software fixtures or replay comparisons for safety, RC, IMU,
  control, actuator requests, telemetry qualification, storage/configuration,
  OSD, USB, and fault behavior where host testing is feasible.
- [ ] Produce a release candidate, hashes, feature manifest, architecture
  report, memory/resource report, and residual-difference list while retaining
  both output-inhibition gates.

Exit criteria: the output-inhibited generated package passes the complete
software matrix and has a reviewed, bounded list of target-only evidence still
required.

Evidence:

- Pending.

## Checkpoint 8 — Safety-authority and output-gate review

Goal: define and review the exact generated image that may enter actuator
testing without moving motor authority outside the safety-owned path.

- [ ] Review every generated safety state, arming guard, permit transition,
  command-freshness rule, watchdog/fault input, actuator lease, telemetry
  prerequisite, queue failure, and shutdown path against the current golden
  behavior.
- [ ] Keep the safety master as the sole owner of arming and temporary actuator
  permission; keep control limited to bounded, timestamped requests; keep the
  safety-owned actuator adapter as the sole motor-peripheral authority.
- [ ] Treat removal of forced `armed = false`, safety-state output inhibition,
  and physical-component output inhibition as separate reviewable changes.
- [ ] Add tests proving that no other profile, component, protocol,
  configurator, or failure path can enable either gate or command a motor.
- [ ] Resolve the ESC-only power-cycle recovery defect through a bounded,
  disarmed, ARM-low-gated reinitialization design with normal delay and fresh
  four-ESC qualification, or retain it as an explicit fail-closed cutover
  blocker.
- [ ] Obtain explicit user approval before changing a safety state, controlled
  timer/DMA assignment, or either output gate. A compile or link pass is not
  approval.
- [ ] Freeze and hash the exact user-approved candidate and its configuration
  for target testing; do not silently rebuild or substitute another image.

Exit criteria: the exact candidate has an approved safety review, no actuator
authority leak, explicit failure behavior, and a documented target-test scope.
The checkpoint itself does not authorize flashing, actuator power, or flight.

Evidence:

- Pending.

## Checkpoint 9 — Fresh exact-image target evidence

Goal: establish evidence for the generated candidate rather than reusing
historical handwritten-image results.

- [ ] Add or select the required entries in
  `project_meta/testing/TEST_CATALOG.json` and follow the current Foxeer target
  procedure. Do not invent an ad hoc powered procedure inside this plan.
- [ ] Record commit and dirty-tree identity, ELF/binary hash, features,
  persisted configuration, board/IMU identity, propeller state, actuator-power
  state, equipment, operator, and retained evidence paths.
- [ ] Complete software/static review plus unpowered boot, USB, IMU, ADC, OSD,
  flash, logging, configuration, timing, and fault-observation gates.
- [ ] With explicit user confirmation and the required safe setup, obtain
  electrical DShot evidence for all four lanes, including M4 complementary
  polarity, pulse width/jitter, and TIM1/TIM8 phase behavior.
- [ ] With explicit user confirmation that propellers are removed, complete
  the current powered props-off exact-image procedure, including motor
  identity/direction, correction direction, telemetry qualification,
  arm-high inhibition, disarm, RC loss, stale/fault behavior, and ADC scaling.
- [ ] Exercise and record the reviewed ESC-only power-cycle recovery behavior;
  a latched or automatic unsafe recovery blocks cutover.
- [ ] Run the final preflight and any bounded flight gate only when the user
  explicitly requests and operates it. Preserve exact-image continuity and
  stop on every catalogued stop condition.
- [ ] Update the evidence index/run records without translating an incomplete
  or observational result into a pass.

Exit criteria: every required catalog gate passes on the same exact generated
candidate and configuration, with hashes and evidence retained. Unavailable
hardware or user-operated steps remain open rather than being inferred.

Evidence:

- Pending.

## Checkpoint 10 — Release cutover and rollback

Goal: deliberately replace the handwritten release artifact only after the
same generated image passes all preceding gates.

- [ ] Reconfirm that the candidate hash, features, configuration schema, and
  generated provenance match the exact image used for target evidence.
- [ ] Publish the generated candidate in parallel first and verify artifact
  naming, packaging, configurator discovery, and download behavior without a
  silent fallback to the handwritten ELF.
- [ ] Define and test a bounded rollback to the last accepted handwritten
  artifact before changing the default release route.
- [ ] Update CI release jobs, configurator packaging, documentation, and
  support metadata to select the generated artifact deliberately.
- [ ] Keep the handwritten app and its evidence available as a pinned
  reference until the user separately approves archival or removal.
- [ ] Run the complete software matrix and repository context checks after the
  cutover, and retain the final generated source/report/artifact hashes.
- [ ] Confirm documentation describes an experimental, safety-oriented
  prototype and does not claim certification or airworthiness.

Exit criteria: mainline release and configurator workflows consume the
reviewed generated artifact, rollback is available, all automated gates pass,
and no stale or differently built image is presented as the tested candidate.

Evidence:

- Pending.

## Checkpoint 11 — User review and plan cleanup

Goal: let the user review the complete implementation and retained evidence
before this plan is removed.

- [ ] Re-run the verification appropriate to every changed workspace and the
  repository context checks; record any unavailable hardware or external
  dependency without weakening the result.
- [ ] Present the user with a concise completion report covering implemented
  architecture, changed paths and interfaces, test commands/results, artifact
  identities, target evidence, accepted deviations, residual risks, and
  rollback status.
- [ ] Explicitly ask the user to review the completed implementation. Do not
  delete this plan in the same step and do not interpret silence as approval.
- [ ] After the user confirms the review is complete, explicitly ask for or
  receive authorization to delete this completed plan.
- [ ] Only after that authorization, delete this file, remove its context
  budget from `project_meta/DOCUMENT_REGISTRY.json`, and remove or revise the
  plan pointer in `AGENTS.md` in the same change so no dangling instruction
  remains.

Exit criteria: the user has reviewed the completed implementation and has
explicitly authorized plan deletion. Until then, this checkpoint and the plan
remain open.

Evidence:

- Pending.

## Verification baseline

Choose commands according to the files changed and record exact results in the
active checkpoint. The expected software baseline includes:

```text
(cd tools/rtic-app-builder && cargo fmt --all --check)
(cd tools/rtic-app-builder && cargo check -p xtask --locked)
(cd tools/rtic-app-builder && cargo test -p xtask --locked)
(cd tools/rtic-app-builder && cargo check --manifest-path compat/ferrowasp-serial-osd/Cargo.toml --tests --locked)
(cd tools/app-builder-v3 && cargo fmt --all --check)
(cd tools/app-builder-v3 && cargo check --workspace --locked)
(cd tools/app-builder-v3 && cargo test --workspace --locked)
python -m unittest discover -s tools/tests -p "test_*.py" -v
python tools/check_repository_context.py
```

Also run strict Clippy, deterministic regeneration, generated embedded checks,
and release linking once their checkpoint implements those commands. Hardware
commands are intentionally absent from this software baseline.
