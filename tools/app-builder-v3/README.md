# App Builder V3

App Builder V3 is an isolated, compilable authoring model for future generated
STM32F4 RTIC applications. It currently validates declarations and reusable
task bodies; RTIC source generation is not implemented yet.

The active source is organized by responsibility:

- `xtask/src/hardware_definitions/` contains reusable hardware types,
  STM32F4-specific components, and HAL-specific hardware tasks;
- `xtask/src/rtic/` contains the task, component, and application-composition
  declaration model;
- `xtask/src/tasks/` contains reusable HAL-agnostic task bodies;
- `xtask/src/target/board.rs` declares the selected board hardware;
- `xtask/src/target/app_composition.rs` selects components, task instances,
  resources, constants, priorities, spawns, and interrupt bindings.

From this directory, run:

```text
cargo check --workspace
cargo test --workspace
```

For Rust completion and diagnostics, open `tools/app-builder-v3` in its own
VS Code window. Its local editor configuration loads only the host-side
`xtask` package rather than the repository's embedded firmware workspace.
