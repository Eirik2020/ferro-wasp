# FerroConfigurator Instructions

These instructions apply to the isolated host workspace under this directory
in addition to `../AGENTS.md` and the repository root instructions.

## Scope

FerroConfigurator is a bounded host-side companion for FerroWasp. It may
discover devices, read and persist firmware-whitelisted configuration, acquire
and validate logs, convert retained evidence, and flash an explicitly selected
release image. It never owns arming, actuator gating, failsafe, watchdog, or
motor authority.

Keep this workspace isolated from the embedded root Cargo workspace. It has a
separate lockfile and host dependency graph so Windows serial and DFU tooling
cannot disturb firmware dependency resolution.

## Context routing

For configuration work, read the shared firmware configuration schema and the
focused configurator client/tests. For blackbox work, read
`../../../crates/ferrowasp-core/src/blackbox.rs`, the focused Python reference
implementation, and its tests. Do not load flight-app shells, raw logs,
archived project documents, generated build trees, minified documentation
assets, or vendored binary contents unless a specific failure requires them.

The bounded ASCII USB storage/configuration contract remains the initial
transport. Do not enable or expand the MSPv2 configurator path without explicit
approval and target evidence.

## Safety and integrity

- Configuration writes remain disarmed-only and require complete readback
  verification.
- Flashing, log erase, and overwrite are explicit commands with visible
  confirmation and nonzero failure exits.
- Validate board identity, release manifest, hashes, address ranges, page
  CRCs, flight identity, and resume state before claiming success.
- Preserve raw `.fwbb` evidence. Conversion writes a separate output and may
  not make invalid acquisition appear valid.
- Never silently select a stale or wrong-board firmware image.

## Verification

Run from this directory:

```text
cargo fmt --all --check
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Package and hardware checks are additional gates and do not replace these
software tests.
