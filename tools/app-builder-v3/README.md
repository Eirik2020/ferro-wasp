# App Builder V3

App Builder V3 is an isolated, compilable authoring model and renderer for
STM32F4 RTIC applications. It validates declarations and reusable task bodies,
expands hardware endpoints, resolves ownership and interrupts, and renders the
selected RTIC application.

The active source is organized by responsibility:

- `xtask/src/hardware_definitions/` contains reusable hardware types,
  STM32F4-specific hardware endpoints, and HAL-specific hardware tasks;
- `xtask/src/rtic/` contains the task, component, and application-composition
  declaration model;
- `xtask/src/tasks/` contains reusable HAL-agnostic task bodies;
- `xtask/src/target/board.rs` declares the selected board hardware;
- `xtask/src/target/app_composition.rs` selects components, task instances,
  resources, constants, priorities, spawns, and interrupt bindings.
- `generated/src/main.rs` is the generated RTIC application and must not be
  edited directly.

From this directory, run:

```text
cargo check --workspace
cargo test --workspace
cargo run -p xtask -- check
cargo run -p xtask -- generate
```

The generated firmware package is isolated from the host-side workspace. With
the `thumbv7em-none-eabihf` target installed, check it using:

```text
(cd generated && cargo check)
```

For Rust completion and diagnostics, open `tools/app-builder-v3` in its own
VS Code window. Its local editor configuration loads only the host-side
`xtask` package rather than the repository's embedded firmware workspace.
