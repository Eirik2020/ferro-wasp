# RTIC task-composition sandbox

This isolated sandbox generates one NUCLEO-F401RE RTIC application from three
inputs:

1. a Rust `BoardDeclaration` describing the MCU, clock, monotonic, and physical
   resources;
2. an `AppDeclaration` selecting included tasks and init spawns;
3. handwritten, unannotated task functions.

The initial example declares a priority-1 `blink_led` task with `led3` as a
local resource. Its handwritten body waits on the SysTick monotonic and toggles
the NUCLEO LD2 LED on PA5. Each iteration spawns a one-shot `report_blink`
task, which logs the wrapping `u32` blink count through defmt RTT.
The board's active-low B1 button on PC13 is configured as a falling-edge EXTI
input. Its synchronous `EXTI15_10` task toggles the shared `blink_enabled`
software resource; the async blink task remains scheduled but holds LD2 low
while disabled.

## Generate

From this directory:

```text
cargo xtask generate
```

The generator validates the declaration and handwritten function, creates the
RTIC `#[task(...)]` attribute, inserts the combined task into the app template,
parses the complete result, and writes:

```text
targets/nucleo_f401re/src/main.rs
```

The handwritten body is
[`tasks/task-bodies/blink_led.rs`](tasks/task-bodies/blink_led.rs). It is source
input and is never compiled as a separate Rust module. The Rust declaration is
[`tasks/task-declarations/blink_led.rs`](tasks/task-declarations/blink_led.rs),
which the central
[`tasks/task-declarations/mod.rs`](tasks/task-declarations/mod.rs) registry
reexports for app declarations. Apps can import that registry with `*` and
then explicitly select task constants in `APP.tasks` and `APP.init.spawns`.

The app declaration is
[`targets/nucleo_f401re/src/app_composition.rs`](targets/nucleo_f401re/src/app_composition.rs).
`APP.tasks` controls all rendered tasks; `APP.init.spawns` controls only the
tasks started from RTIC `init`. `APP.software_resources.shared` and
`APP.software_resources.local` declare non-hardware RTIC state. Tasks refer to
that state by ID in the same resource lists they use for board hardware, and
the resolution phase checks that the selected section matches the task use.

An empty application is declared as `pub const APP: AppDeclaration =
AppDeclaration::EMPTY;`. Rust struct literals require every field explicitly,
so `AppDeclaration { init: InitDeclaration {} }` cannot omit `spawns` or
`tasks`, and a non-empty struct literal must also include `software_resources`.

The board declaration is
[`targets/nucleo_f401re/src/board.rs`](targets/nucleo_f401re/src/board.rs). It
contains only HAL-independent physical data such as typed resource IDs and
physical-pin names. Digital outputs default to push-pull, initial-low, and
active-high.
Resolution uses those declarations without depending on an MCU
HAL. EXTI inputs default to active-low, pull-up, and falling-edge operation.
The STM32F4 backend validates the selected target and maps the resolved PA5 and
PC13 resources to `ferrowasp-stm32f4` types and initialization. Task code imports
its digital capability through `ferrowasp-io-core`; the existing RTIC and
SysTick-monotonic facade remains in `ferrowasp-stm32f4` for this prototype.

The first button prototype deliberately has no debounce yet. A physical press
can therefore produce more than one EXTI edge; a later task can add a bounded
monotonic debounce policy without changing the board declaration.

## Verify

```text
cargo test -p xtask --locked
cd targets/nucleo_f401re
cargo check --locked
cargo build --locked --release
```

The app is a non-actuator validation prototype. It has no motor-output
authority.
