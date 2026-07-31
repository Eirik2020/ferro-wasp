# RTIC task-composition sandbox

This isolated sandbox generates one NUCLEO-F401RE RTIC application from three
inputs:

1. a Rust `BoardDeclaration` describing the MCU, clock, monotonic, and physical
   resources;
2. an `AppDeclaration` selecting included tasks and init spawns;
3. a handwritten, unannotated async task function.

The initial example declares a priority-1 `blink_led` task with `led2` as a
local resource. Its handwritten body waits on the SysTick monotonic and toggles
the NUCLEO LD2 LED on PA5 every second.

## Generate

From this directory:

```text
cargo xtask generate
```

The generator validates the declaration and handwritten function, creates the
RTIC `#[task(...)]` attribute, inserts the combined task into the app template,
parses the complete result, and writes:

```text
app/generated/src/main.rs
```

The handwritten body is
[`tasks/task-bodies/blink_led.rs`](tasks/task-bodies/blink_led.rs). It is source
input and is never compiled as a separate Rust module. The Rust declaration is
[`tasks/task-declarations/blink_led.rs`](tasks/task-declarations/blink_led.rs),
which `xtask` imports and assembles.

The app declaration is [`apps/nucleo_f401re.rs`](apps/nucleo_f401re.rs).
`APP.tasks` controls all rendered tasks; `APP.init.spawns` controls only the
tasks started from RTIC `init`.

An empty application is declared as `pub const APP: AppDeclaration =
AppDeclaration::EMPTY;`. Rust struct literals require every field explicitly,
so `AppDeclaration { init: InitDeclaration {} }` cannot omit `spawns` or
`tasks`.

The board declaration is
[`boards/nucleo_f401re.rs`](boards/nucleo_f401re.rs). The generator uses its
HSI clock declaration through `ferrowasp-stm32f4::clocks::freeze_hsi` and
renders the SysTick monotonic plus LD2 initialization from its hardware entry.
The generated app imports its RTIC, HAL, and SysTick-monotonic surface only
through the narrow `ferrowasp-stm32f4::rtic` facade; `ferrowasp-core` remains
hardware-agnostic.

## Verify

```text
cargo test -p xtask --locked
cd app
cargo check --locked --bin generated
cargo build --locked --release --bin generated
```

The app is a non-actuator validation prototype. It has no motor-output
authority.
