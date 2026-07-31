# NUCLEO-F401RE RTIC prototype app

This isolated embedded crate provides two buildable versions of the same
non-actuator RTIC application:

- `handwritten` is the reference used while prototyping builder behavior;
- `generated` is the output slot for builder experiments.

From the parent `tools/app-builder-v2` directory, generate the RTIC shell and
blink task with:

```text
cargo xtask generate
```

The command creates the RTIC task attribute from a Rust `TaskDeclaration`,
combines it with the handwritten task body, and validates the complete Rust
syntax before replacing `generated/src/main.rs`. It does not alter the
handwritten reference.

Both versions use the NUCLEO-F401RE's existing sandbox hardware contract:

- PA5 / LD2 is a push-pull status LED;
- PC13 / B1 is an active-low EXTI input;
- SysTick provides the 1 kHz RTIC monotonic;
- EXTI0 and EXTI1 are software-task dispatchers.

The handwritten reference also includes the B1 demonstration. The generated
clean-slate app contains only the one-second LD2 blink task. This validation app
has no actuator or motor-output authority.

## Build

Run the commands from this directory so Cargo uses the embedded target declared
in `.cargo/config.toml`:

```text
cargo check --locked --bin handwritten
cargo check --locked --bin generated
cargo build --locked --release --bin generated
```

The release build produces `target/thumbv7em-none-eabihf/release/generated`.
Flashing is deliberately not configured in the sandbox; select the exact ELF,
chip, and probe explicitly with your preferred probe-rs workflow.
