# RTIC App Builder Instructions

These instructions apply with `../AGENTS.md` and the repository root rules.

## Workspace boundary and context

This directory is an intentionally isolated Cargo workspace. Run its Cargo and
`xtask` commands from `tools/rtic-app-builder`; do not add it, its generated
applications, or its dependency selection to the FerroWasp root workspace
without explicit architectural approval.

Read `README.md` first, then only the design note relevant to the change. Use
the bounded `RTIC_APP_BUILDER_REFERENCE_IMPLEMENTATION_PLAN.md` for current
sequencing. The full 2026-07-24 baseline under `docs/archive/` is provenance,
not default context; open only the section needed for a specific historical or
detailed requirement.

Current FerroWasp flight status, golden-app behavior, board evidence, and
safety policy remain owned by the monorepo root. Inspect those sources at the
current commit when builder work depends on them; do not duplicate their live
state here.

## Sources of truth

- `bsp/` owns declared physical hardware facts and stable resource IDs.
- `applications/` selects BSP resources and application behavior.
- `architecture-contracts/` owns executable task, transport, timing, fault,
  mechanism, and safety-scope requirements.
- `feature-library/`, `templates/`, and `xtask/` own generation behavior.
- `compat/` contains narrow, explicitly transitional adapters.
- `generated/` is disposable output. Do not hand-edit generated Rust or treat
  it as a second source of truth.

Correct manifests, contracts, templates, feature fragments, or backend code,
then regenerate. Resume only when the builder's fingerprint and checkpoint
rules accept unchanged inputs.

## Generator rules

- Translate declared values; do not allocate pins, peripherals, interrupts,
  DMA streams, timers, priorities, or queue topology on behalf of the author.
- Parse manifests strictly and reject unknown, conflicting, incomplete, or
  unsupported declarations.
- Validate all inputs and the applicable architecture contract before mutating
  generated output.
- Preserve deterministic ordering, bounded transports, explicit capacities,
  typed/versioned mechanisms, and reproducible diagnostics.
- Keep task forms, ownership, interaction classification, safety
  classification, failure behavior, and timing semantics explicit.
- A generated compile or link pass does not establish target behavior or
  flight safety.

The NUCLEO examples are validation applications. They do not gain FerroWasp
flight arming or motor authority.

## Verification

Run focused tests, then the isolated workspace baseline:

```text
cargo fmt --all --check
cargo check -p xtask --locked
cargo test -p xtask --locked
cargo check --manifest-path compat/ferrowasp-serial-osd/Cargo.toml --tests --locked
```

For generator changes, regenerate the affected checked example and retain the
incremental check plus final release-link result. Hardware commands such as
`cargo xtask flash` or `embed` require explicit user intent and suitable probe
hardware; do not substitute them for software-only validation.
