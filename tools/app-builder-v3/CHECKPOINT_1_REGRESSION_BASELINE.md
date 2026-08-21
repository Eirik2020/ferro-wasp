# Checkpoint 1 Regression Baseline

Status: complete

Captured: `2026-08-19`

Branch: `tool/app-builder`

Commit: `a994d751cfe0c2a2525b2cb34583e496925bf3b3`

This artifact pins the behavior that must survive consolidation into the
approved `tools/app-builder/` workspace. Generated files were produced only by
their builders. No probe, flash, powered-actuator, or flight command ran.

## Added regression fixtures

- `generator::tests::selected_target_matches_checked_golden_outputs_deterministically`
  renders twice and byte-compares the V3 RTIC application, prelude, platform
  configuration, and semantic safety-spine report with the checked outputs.
- `tests/golden/foxeer-f405-v2-live-config.txt` pins the existing 21-key public
  configuration contract, bounds, Foxeer defaults, stored schema version and
  lengths, log divisor, initial request sequence, and legacy migration.
- `rtic::composition::tests::duplicate_interrupt_binding_is_rejected` pins the
  pre-render duplicate-interrupt failure.

The existing suites retain the other required negative boundaries: duplicate
local/safety ownership, hardware IDs, pins, timers, DMA streams, resolved
claims, and manifest feature order; invalid priorities; zero capacities;
missing, shared, noncritical, or type-incompatible safety edges; strict unknown
manifest/state fields; and deterministic ordering/rendering/hashing.

## Fixture identities

| Fixture | SHA-256 |
|---|---|
| V3 generated `src/main.rs` | `1bffafd849c352184e16a1b89998692ab7bc1b2bd95e73e9b6ded931aaec128c` |
| V3 generated `src/prelude.rs` | `59fd44d4045e5fa1cb56101fc7d24280ab658d9846ebf333c1c76c191108a297` |
| V3 generated `src/platform_config.rs` | `ddeae1e7b24df9bf64c04ed5837471c1ef1086f1bf166d78ab4e8eead9747cbe` |
| V3 generated `SAFETY_SPINE.md` | `3070863d2fa8bb784b02ce0e5e8ffff920b20e204542b2e10e7de16e9c56ea5c` |
| Foxeer live-configuration snapshot | `06366a21e6b2cd2a79284843266933bdfe3e3dfeaddb6673f8d4cd1f81ad2ea1` |
| NUCLEO blinky application manifest | `8325128ae7ffc951056041f4244bbb2c51fd8fea511971ab80acbf57b457f64b` |
| NUCLEO OSD application manifest | `5addea1c8306544898a241e7d0e4214ef192f84442a5fee3fec978944bff813b` |
| NUCLEO blinky architecture contract | `f286788ec92463c71c968d4099e7383e7ae65ae303d6f12cae1cb40df5334fbd` |
| NUCLEO OSD architecture contract | `d102c06e38f856168d85525ba070c7693874ee391a458af5d36ae73f331303b6` |
| NUCLEO blinky golden tree, fixed-order aggregate | `99a44676ceb1d103b43ea14e2f5b63fd7efba55eae5a40c9ed5befbb07387aa5` |

Two consecutive V3 `generate` runs produced the four identical hashes above.
The exact-byte regression test independently renders twice on every suite run.

## Verification

| Workspace | Command or gate | Result |
|---|---|---|
| Both builders | `cargo fmt --all --check` | pass |
| Canonical | `cargo check -p xtask --locked` | pass |
| Canonical | `cargo test -p xtask --locked` | pass: 61 executed, one subprocess helper ignored |
| Canonical OSD adapter | `cargo check --manifest-path compat/ferrowasp-serial-osd/Cargo.toml --tests --locked` | pass |
| Canonical | strict Clippy for all `xtask` targets | pass |
| V3 | `cargo check --workspace --locked` | pass |
| V3 | `cargo test --workspace --locked` | pass: 91 unit tests and 3 doctests |
| V3 | `cargo run -p xtask --locked -- check` | pass |
| V3 | two generated-source/report runs and SHA-256 comparison | pass, identical |
| V3 generated Foxeer | locked Thumb `cargo check` | pass |
| Canonical blinky | clean non-resume generation, empty plus two feature checks, release link | pass |
| Canonical OSD | clean non-resume generation, empty plus two feature checks, release link | pass after fetching the absent locked `rtic-sync` crate |
| Repository tooling | 69 Python unit tests | pass |
| Repository context | `python tools/check_repository_context.py` | pass |

The canonical release-linked validation binaries were
`b6125f6eb935a5036ad10a59a8c427886458c4f858731a4514cc25158262f769`
for blinky and
`7edec6bfa40b9fbad8446181f76ed448b45dd35174a348f658f1d3ccadbe37f5`
for OSD. They are disposable embedded-build artifacts, not target evidence.

Strict V3 Clippy still reports exactly the 11 Checkpoint 0 baseline findings:
eight `result_unit_err`, two `drop_non_drop`, and one `collapsible_if`. None is
in the new regression code; Checkpoint 6 owns eliminating this known debt.

## Safety boundary

This checkpoint added host-side assertions only. Motor authority, safety
states, task priorities, physical assignments, output gates, runtime timing,
and unsafe-code use did not move or change. The generated Foxeer graph remains
independently inhibited by the safety state and physical component gates.
