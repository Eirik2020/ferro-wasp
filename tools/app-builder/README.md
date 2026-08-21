# FerroWasp App Builder

This is the isolated consolidation workspace for FerroWasp's typed RTIC app
builder. It compiles repository-owned board, app, component, and task inputs
through `src/input_catalog.rs`; those inputs remain owned by their repository
directories and are not modules of the handwritten embedded applications.

During the migration, `../app-builder-v3` and `../rtic-app-builder` remain
available as protected comparison implementations. The active checkpoint and
cleanup gates are owned by
[`../app-builder-v3/MAINLINE_MIGRATION_IMPLEMENTATION_PLAN.md`](../app-builder-v3/MAINLINE_MIGRATION_IMPLEMENTATION_PLAN.md).

The selected Foxeer composition is still output-inhibited. A generated build
is software evidence only and does not authorize flashing or motor operation.

Run the current host checks from this directory:

```text
cargo fmt --all --check
cargo check --locked
cargo test --locked
cargo run --locked -- check --app foxeer-f405-v2
cargo run --locked -- generate --app nucleo-f401re-blinky
cargo run --locked -- build --app nucleo-f401re-osd
```

The exact catalog IDs are `foxeer-f405-v2`, `nucleo-f401re-blinky`, and
`nucleo-f401re-osd`. `clean`, `flash`, and `embed` also require `--app`.
`flash` and `embed` are explicit hardware operations; generation or a
successful build does not authorize running either command.
