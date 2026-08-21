# Checkpoint 3 Input and CLI Evidence

Date: `2026-08-19`

## Identity and scope

- Branch: `tool/app-builder`
- Reference HEAD: `a994d751cfe0c2a2525b2cb34583e496925bf3b3`
- Worktree: dirty with the migration implementation listed by
  `git status --short`; no commit was created.
- Toolchain: `rustc 1.99.0-nightly (77cf889bc 2026-07-12)`,
  `cargo 1.99.0-nightly (59800466c 2026-07-07)`.
- No probe, board, actuator power, flash, embed, or flight command ran.

## Selection and resolution

`tools/app-builder/src/input_catalog.rs` contains the only selectable
application catalog. Its exact IDs are `foxeer-f405-v2`,
`nucleo-f401re-blinky`, and `nucleo-f401re-osd`. Foxeer selects the approved
repository-owned Rust composition through the explicit compile-time input
catalog. The NUCLEO entries select fixed builder-relative schema-6 application
and BSP manifests; the CLI exposes no manifest, BSP, or Rust-source path.

All entries pass through `application::resolve` into one
`ResolvedApplication`, then through `application::render` into one
`RenderedApplication`. The graph retains a typed V3 variant and a strict
NUCLEO compatibility-ingestion variant while typed recipes are expanded. The
compatibility variant preserves the versioned manifests, architecture
contracts, feature claims, and golden renderer inside the canonical builder;
neither comparison builder is called or modified.

Catalog validation rejects duplicate IDs, missing files, absolute paths, and
parent traversal. Resolution occurs completely before `generator::generate`
computes or mutates the output path. Tests cover unknown and nested manifest
fields, missing required fields, incompatible schema versions, incomplete
feature selections, ambiguous bindings, resource/provider collisions,
undeclared resources, and unsafe output paths.

Foxeer board facts remain in `boards/foxeer-f405-v2/board.rs`, app selection
and configuration remain under `apps/foxeer-f405-v2/`, and reusable task and
backend definitions keep the Checkpoint 2 ownership split. A focused test
proves that the two instances of the serial endpoint component retain stable
`serial1`/`serial2` identities and have no resource-ID collision.

## Command surface

Every operation requires `--app` (with visible `--application` alias):
`check`, `generate`, `build`, `clean`, `flash`, and `embed`. Unknown catalog
IDs fail with exit 1; Clap usage/selection failures use exit 2. `clean` first
requires an exact catalog ID and removes only that app's confined generated
root; temporary-directory tests prove neighboring output survives.

Build, flash, and embed use shell-free `CommandSpec` values and an injectable
runner. Tests assert exact target, lock, output directory, chip, SWD, verify,
reset, and ELF arguments. Only the three locked release-build commands were
executed. Hardware command construction was tested without executing either
hardware command.

## Determinism and safety result

The copied NUCLEO blinky golden crate is byte-identical after canonical
resolution/rendering. All three applications render identically on repeated
runs. Foxeer's protected sources and report remain unchanged:

| Output | SHA-256 |
|---|---|
| `main.rs` | `1bffafd849c352184e16a1b89998692ab7bc1b2bd95e73e9b6ded931aaec128c` |
| `prelude.rs` | `59fd44d4045e5fa1cb56101fc7d24280ab658d9846ebf333c1c76c191108a297` |
| `platform_config.rs` | `ddeae1e7b24df9bf64c04ed5837471c1ef1086f1bf166d78ab4e8eead9747cbe` |
| `SAFETY_SPINE.md` | `3070863d2fa8bb784b02ce0e5e8ffff920b20e204542b2e10e7de16e9c56ea5c` |

The Foxeer report still states that both selected output gates are false,
the physical component records `output enabled = false`, and control is
forced disarmed. The handwritten Foxeer app remains authoritative.

## Verification

All commands were software-only:

- `(cd tools/app-builder && cargo fmt --all --check)` — pass.
- `(cd tools/app-builder && cargo check --locked --offline --target-dir
  /tmp/ferrowasp-app-builder-cp3-target)` — pass.
- `(cd tools/app-builder && cargo test --locked --offline --target-dir
  /tmp/ferrowasp-app-builder-cp3-target)` — 151 unit tests and 3 doctests pass.
- `cargo run ... -- check --app <id>` and `generate --app <id>` for all three
  IDs — pass; repeated generation retains the hashes above.
- `cargo run ... -- build --app <id>` for all three IDs — locked release links
  pass for `thumbv7em-none-eabihf`.
- Unknown selection through the built CLI — exit 1 with the complete expected
  ID list and no output mutation.

The resulting software-only ELF hashes were:

| Application | SHA-256 |
|---|---|
| Foxeer | `d837705777a365418c0ffa6c8a92be9e1e7fee25c914f77801d45c6ff11d611a` |
| NUCLEO blinky | `cf1f39c0ce5ca44c874e9a09d9df83ae0ff50f45e0702a82a266ab285f607e60` |
| NUCLEO OSD | `e2ae4082b919990023f36c10a8776c9ef3cd59ef7f66a16ac3e49368000819e9` |

These hashes are build evidence only, not target or cutover evidence.

## Exit assessment

Checkpoint 3 exits successfully. The canonical builder selects all three
applications by stable catalog ID, validates before mutation, provides the
required explicit scriptable commands, and does not expose arbitrary Rust
input. Flash/embed remain explicit and unexecuted. Checkpoint 4 still owns
staging/promotion, fingerprints, retained failures, resumability, provenance,
and interruption-safe lifecycle behavior.
