# App Builder V3

V3 currently provides a small, compilable hardware-definition authoring
surface. Use it to add and type-check HAL-independent physical definitions
before they are consumed by the pending generator migration.

From this directory, run:

```text
cargo check
cargo test
```

The `xtask::hardware_definitions` module is the supported authoring entry
point. The older generator source remains in `xtask/src/`, but is deliberately
excluded from the default build until its components are migrated into V3.

For Rust completion and diagnostics, open `tools/app-builder-v3` in its own
VS Code window. Its local editor configuration loads only the host-side
`xtask` package, rather than the repository's embedded firmware workspace.
