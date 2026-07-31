# NUCLEO-F401RE RTIC prototype app

This self-contained embedded target crate builds the generated, non-actuator
RTIC application in `src/main.rs`. Its handwritten inputs are `src/board.rs`,
`src/app_composition.rs`, and the shared task declarations and bodies under
the builder's `tasks/` directory.

From `tools/app-builder-v2`, generate the RTIC shell and tasks with:

```text
cargo xtask generate
```

The command creates the RTIC task attribute from a Rust `TaskDeclaration`,
combines it with the handwritten task body, and validates the complete Rust
syntax before replacing this target's `src/main.rs`.

The target uses the NUCLEO-F401RE's existing sandbox hardware contract:

- PA5 / LD2 is a push-pull status LED;
- PC13 / B1 is an active-low EXTI input;
- SysTick provides the 1 kHz RTIC monotonic;
- EXTI0 is the software-task dispatcher;
- EXTI15_10 is bound to the B1 hardware task.

The generated app keeps the blink task scheduled and uses B1 to toggle its
shared enable flag. While disabled, the task drives LD2 low. The initial EXTI
prototype is not debounced, so one physical press can occasionally cause more
than one toggle. This validation app has no actuator or motor-output authority.

## Build

Run the commands from this directory so Cargo uses the embedded target declared
in `.cargo/config.toml`:

```text
cargo check --locked
cargo build --locked --release
```

The release build produces `target/thumbv7em-none-eabihf/release/generated`.
Flashing is deliberately not configured in the sandbox; select the exact ELF,
chip, and probe explicitly with your preferred probe-rs workflow.
