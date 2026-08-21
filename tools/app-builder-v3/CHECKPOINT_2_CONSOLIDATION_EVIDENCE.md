# Checkpoint 2 Consolidation Evidence

Date: `2026-08-19`

## Identity and scope

- Branch: `tool/app-builder`
- Reference HEAD: `a994d751cfe0c2a2525b2cb34583e496925bf3b3`
- Worktree: dirty with the migration-plan, regression-fixture, canonical
  builder, and repository-owned input changes listed by `git status --short`;
  no commit was created.
- Toolchain used: `rustc 1.99.0-nightly (77cf889bc 2026-07-12)`,
  `cargo 1.99.0-nightly (59800466c 2026-07-07)`.
- No probe, board, actuator power, flash, embed, or flight command ran.

## Consolidated implementation

The approved isolated host workspace now exists at `tools/app-builder/` with
one typed resolve/lower/render path. The 76 mapped V3 Rust sources were moved
or adapted without deleting either comparison builder. The ownership split is:

- physical Foxeer facts: `boards/foxeer-f405-v2/board.rs`;
- portable task inputs: top-level `tasks/`;
- complete Foxeer composition, sibling `platform_config.rs` and
  `live_config.rs`, app tasks/components, and reconciliation: the Foxeer app;
- STM32F4 host declarations, endpoint definitions, task-authoring aliases,
  task wrappers, and lowering: `tools/app-builder/src/backends/stm32f4/`;
- typed RTIC model, validation, resolution, deterministic renderer, and
  safety report: `tools/app-builder/src/rtic/`.

No source moved into `ferrowasp-stm32f4`: independently compiled runtime
mechanisms remain shared dependencies, while host-compiled task declarations
remain under the backend. The portable task inputs contain no HAL imports.

`tools/app-builder/src/input_catalog.rs` explicitly registers 39 inputs:
1 board, 5 app/configuration/verification inputs, 4 portable tasks, 5 app
tasks, and 24 backend tasks. Validation rejects absent files, duplicate IDs,
and duplicate paths. Tests canonicalize all nine external task `file!()`
locations against their registered repository paths, so source extraction
continues to read the relocated task files rather than the catalog module.

The app-owned `live_config.rs` selects the existing shared `ConfigKey`,
`TuningProfile`, and `StoredConfig` APIs. It adds no schema, key, bound,
default, persistence algorithm, wire format, or actuator capability.

## Safety and deterministic-output result

Resolution still validates the complete graph before rendering. The
reconciliation requires exactly one physical DShot component with
`output_enabled = false` and requires the independent DShot service and ESC
manager task gates to remain disabled. A mutation test enables the physical
gate in the resolved graph and confirms reconciliation fails. The existing
sole-adapter, safety-channel ownership, capacity, task-priority, interrupt,
resource, DMA, and deterministic-ordering tests remain green.

Repeated canonical rendering, the materialized `generate` output, and the
checked fixtures are byte-identical:

| Output | SHA-256 |
|---|---|
| `main.rs` | `1bffafd849c352184e16a1b89998692ab7bc1b2bd95e73e9b6ded931aaec128c` |
| `prelude.rs` | `59fd44d4045e5fa1cb56101fc7d24280ab658d9846ebf333c1c76c191108a297` |
| `platform_config.rs` | `ddeae1e7b24df9bf64c04ed5837471c1ef1086f1bf166d78ab4e8eead9747cbe` |
| `SAFETY_SPINE.md` | `3070863d2fa8bb784b02ce0e5e8ffff920b20e204542b2e10e7de16e9c56ea5c` |

These are also byte-identical to the protected V3 generated files and
Checkpoint 1 hashes. The canonical lockfile hash is
`de6a6aa00945e7b1c39d91f2781d77548f0fa697edd6701efdeb40ae39fe6295`,
identical to V3.

## Verification

All commands were software-only:

- `(cd tools/app-builder && cargo fmt --all --check)` — pass.
- `(cd tools/app-builder && cargo check --locked --target-dir
  /tmp/ferrowasp-cp2-app-builder-target)` — pass.
- `(cd tools/app-builder && cargo test --locked --target-dir
  /tmp/ferrowasp-cp2-app-builder-target)` — 95 unit tests and 3 doctests pass.
- `(cd tools/app-builder && cargo run --locked --target-dir
  /tmp/ferrowasp-cp2-app-builder-target -- check)` — pass.
- `(cd tools/app-builder && cargo run --locked --target-dir
  /tmp/ferrowasp-cp2-app-builder-target -- generate)` — pass; output matches
  all four fixtures and V3 byte for byte.
- `(cd tools/app-builder-v3/generated && cargo check --locked --target
  thumbv7em-none-eabihf --target-dir /tmp/ferrowasp-cp2-generated-target)` —
  pass using the byte-identical generated source.
- `(cd tools/app-builder-v3 && cargo test --workspace --locked --target-dir
  /tmp/ferrowasp-cp2-v3-target)` — 91 unit tests and 3 doctests pass.
- `(cd tools/rtic-app-builder && cargo test --workspace --locked --target-dir
  /tmp/ferrowasp-cp2-rtic-builder-target)` — 61 executed tests pass; the one
  subprocess helper remains intentionally ignored by the harness.
- `python3 -m unittest discover -s tools/tests -p 'test_*.py' -v` — 69 pass.
- `python3 tools/check_rtic_boundaries.py` — pass.
- `python3 tools/check_repository_context.py` — pass after registering and
  routing the intended canonical builder `AGENTS.md`.

The unchanged Checkpoint 1 NUCLEO release-link evidence was not rerun: neither
comparison builder, its manifests, lockfile, nor its generated candidates
changed during consolidation. The V3 Foxeer generated source was checked
fresh because exact relocated-output equivalence was a Checkpoint 2 claim.

Strict Clippy with `-D warnings` reproduces exactly the 11 findings pinned at
Checkpoint 0/1: eight `result_unit_err` authoring stand-ins, two
`drop_non_drop` source-extraction patterns, and one `collapsible_if`. It adds
no input-catalog or relocation warning. Checkpoint 6 owns their resolution.

## Exit assessment

Checkpoint 2 exits successfully. The canonical workspace resolves and renders
the V3 typed model, all protected fixtures pass, task source extraction is
preserved, and Foxeer output remains independently inhibited. Checkpoint 3
still must replace the single hard-coded Foxeer selection with strict,
scriptable selection for Foxeer and both NUCLEO validation applications.
