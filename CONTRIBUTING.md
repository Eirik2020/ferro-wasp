# Contributing

FerroWasp is open source under the Apache License, Version 2.0.

Unless you clearly state otherwise, any contribution intentionally submitted for
inclusion in FerroWasp is submitted under Apache-2.0, without additional terms
or conditions.

## What Is Welcome

- issues;
- bug reports;
- bench-test reports;
- hardware notes;
- reproducible failure cases;
- safety observations;
- design feedback;
- questions and discussion;
- focused code and documentation patches.

## Contribution Expectations

Keep changes small and reviewable. For firmware changes, describe the hardware
or host-test context used for verification.

Do not submit code owned by an employer, client, school, or other organization
unless that ownership and permission has been explicitly cleared.

## Development Workflow

Install the pinned Rust toolchain from `rust-toolchain.toml` and follow the
prerequisites and target-specific commands in `README.md`. The root workspace
contains reusable crates; each deployable firmware app under `apps/` has an
isolated Cargo graph and must be checked separately.

Before opening a pull request, run the checks relevant to the change. The
normal source-only baseline is:

```text
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
mdbook build mdbook
```

For firmware changes, also run `cargo fmt --check` and
`cargo check --release --locked` from each affected app directory. Do not
assume a probe or target board is available in CI, and do not weaken a target
check merely because hardware is unavailable.

Pull requests should state:

- the requirement or problem being addressed;
- affected board, feature set, pins, timers, DMA routes, and task priorities;
- host checks and firmware checks run;
- any bench or flight hardware used, with propeller/power state;
- expected failure behavior and remaining limitations;
- whether public protocol, configuration, logging, or safety behavior changed.

Never include credentials, private network configuration, raw personal paths,
large captures, or unreviewed hardware logs. Use placeholder configuration and
small sanitized evidence where it materially helps review.

## Safety Boundary

Do not propose or submit changes that allow any path outside the safety and
actuator-output path to command motor hardware.

Telemetry, OSD, configurator, scripts, experimental code, and companion modules
must not directly own arming authority, actuator authority, watchdog authority,
or motor peripherals.

## Third-Party Material

Do not submit copied code, copied protocol implementations, vendor examples,
forum snippets, generated code, or documentation from other projects unless the
source and licence are clearly identified and compatible with Apache-2.0.

When in doubt, open an issue with a link and a short description instead of
pasting code.
