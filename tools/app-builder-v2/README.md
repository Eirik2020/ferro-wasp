# RTIC task-composition sandbox

This isolated sandbox generates NUCLEO-F401RE and FerroWasp FCU3 RTIC
applications from three inputs per target:

1. a Rust `BoardDeclaration` describing the MCU, clock, monotonic, and physical
   resources;
2. an `AppDeclaration` selecting standalone tasks, component instances, and
   init spawns;
3. reusable task modules that colocate capability declarations and handwritten
   functions.

The Nucleo example declares a priority-1 `blink_led` task with `led3` as a
local resource. Its handwritten body waits on the SysTick monotonic and toggles
the NUCLEO LD2 LED on PA5. Each iteration spawns a one-shot `report_blink`
task, which logs the wrapping `u32` blink count through defmt RTT.
The board's active-low B1 button on PC13 is configured as a falling-edge EXTI
input. Its synchronous interrupt task toggles the shared `blink_enabled`
software resource; the STM32 backend derives the `EXTI15_10` binding from
PC13. The async blink task remains scheduled but holds LD2 low while disabled.

## Generate

From this directory:

```text
cargo xtask generate
```

The generator validates all targets before writing either one. For each target
it creates RTIC task attributes, combines them with the handwritten bodies,
parses the complete Rust, and writes:

```text
targets/nucleo_f401re/src/generated_app.rs
targets/nucleo_f401re/src/prelude.rs
targets/ferrowasp_fcu3/src/generated_app.rs
targets/ferrowasp_fcu3/src/prelude.rs
```

The generated prelude contains the runtime-link imports and only the capability
and backend reexports required by the resolved application.
`generated_app.rs` imports that surface with one scoped
`use crate::prelude::*;`, preventing its import list from growing with each new
component.

Each reusable task is one module under [`tasks/`](tasks). For example,
[`tasks/blink.rs`](tasks/blink.rs) contains both its portable capability
contract and ordinary Rust function:

```rust
app_task! {
    pub const BLINK: TaskDefinition = TaskDefinition::asynchronous("blink")
        .with_parameters(&[duration("toggle_interval")])
        .with_local(&[digital_output("led")])
        .with_shared(&[boolean("enabled")]);

    async fn blink(mut cx: blink::Context) {
        Mono::delay(cx.config.toggle_interval).await;
        // Remaining handwritten implementation.
    }
}
```

[`tasks/mod.rs`](tasks/mod.rs) is the explicit reusable-task registry. A
definition owns the body ID, sync/async kind, arguments, logical resources,
portable capabilities, and typed compile-time parameter requirements. It does
not own target hardware names, concrete parameter values, or scheduling.

One app declaration is
[`targets/nucleo_f401re/src/app_composition.rs`](targets/nucleo_f401re/src/app_composition.rs).
The target composition creates concrete instances from those definitions:

```rust
pub const BLINK_LED: TaskDeclaration = tasks::BLINK
    .spawned_as("blink_led")
    .priority(1)
    .with_parameters(&[
        parameter("toggle_interval").duration(MillisDurationU32::millis(1_000)),
    ])
    .with_local(&[resource("led").to_hw("led3")])
    .with_shared(&[resource("enabled").to_sw("blink_enabled")]);
```

`APP.tasks` controls all rendered instances; `APP.init.spawns` controls only the
instances started from RTIC `init`. `APP.software_resources.shared` and
`APP.software_resources.local` declare non-hardware RTIC state. Task bodies use
logical resource fields that declarations bind explicitly to hardware or
software IDs.

Task parameters are compile-time configuration, not RTIC resources or spawn
arguments. Reusable bodies access them through `cx.config.<parameter>`. The
composition binds each parameter with a typed value such as
`fugit::MillisDurationU32`; generation turns the binding into a function-local
constant and rewrites the body access. Definitions and declarations must have
exactly matching parameter IDs and kinds, and zero durations are rejected.
The initial generic parameter vocabulary deliberately contains only
`Duration`; additional kinds should be added only when unrelated tasks share
the same need.

The body uses `cx.local.led` and `cx.shared.enabled`; generation rewrites only
those parsed field accesses to `cx.local.led3` and
`cx.shared.blink_enabled`. The resolver checks each `to_hw` target against the
board, each `to_sw` target against the corresponding application software
resource section, and each binding against the definition's required
capability. The current capability set covers digital output, interrupt input,
Boolean state, an RC input snapshot, and the bounded serial consumer used by
the UART component.

Components expand before ordinary resource resolution. A component definition
owns reusable task and resource slots, while a declaration supplies an
instance ID, configuration, and target bindings. The Nucleo composition uses
one configurable serial-port instance:

```rust
pub const UART2: ComponentDeclaration = ComponentDeclaration {
    id: "uart2",
    definition: &SERIAL_PORT_COMPONENT,
    configuration: ComponentConfiguration::SerialPort(UART2_ASSIGNMENT),
    bindings: &[resource("endpoint").to_hw("uart2_endpoint")],
};
```

Expansion prefixes private and exposed artifacts with `uart2_`, validates the
endpoint capability, and feeds the resulting ordinary tasks and resources to
the existing resolver. Generated component-owned resource declarations,
post-init resource values, and RTIC tasks are enclosed in balanced page-width
component comments so their ownership remains visible in the generated app.
Inside RTIC `init`, shared clock and peripheral setup is grouped as system
initialization. Standalone hardware setup is grouped directly by task and
component hardware setup directly by component. Each init spawn has one plain
ownership comment instead of a start/end guard pair.

An empty application is declared as `pub const APP: AppDeclaration =
AppDeclaration::EMPTY;`. Rust struct literals require every field explicitly,
so `AppDeclaration { init: InitDeclaration {} }` cannot omit `spawns` or
`tasks`, and a non-empty struct literal must also include `software_resources`.

One board declaration is
[`targets/nucleo_f401re/src/board.rs`](targets/nucleo_f401re/src/board.rs). It
contains only HAL-independent physical data such as resource IDs and numeric
`PinId { port, pin }` coordinates. Port numbering is normalized at this
boundary; the STM32 backend maps port `0` to GPIOA, port `1` to GPIOB, and so
on. Digital outputs currently use push-pull drive, receive their initial level
explicitly, and default to no internal pull. Resolution uses those declarations
without depending on an MCU HAL. The Nucleo button declaration explicitly
selects pull-up and falling-edge operation. Interrupt declarations reference a
logical local input binding instead of naming a raw interrupt vector:

```rust
tasks::BUTTON_EXTI.interrupt_as("button_exti", "button")
    .with_local(&[resource("button").to_hw("user_button")]);
```

Serial endpoints use `SerialPortId::new(number)`. The neutral declaration does
not distinguish UART from USART: the audited MCU route catalog maps the port
number, RX pin, and DMA coordinates to the concrete PAC peripheral and IRQ.
USART-only capabilities such as synchronous clocks and flow control are not
modelled until an application uses them.

The STM32F4 backend maps each declared numeric pin once, validates it against
the selected STM32F401RE or STM32F405RG LQFP64 package catalog, and
algorithmically renders the generic HAL pin type, GPIO port split, pin
accessor, pull, initial state, interrupt edge, and EXTI binding. For example,
changing a valid digital output from
`PinId::new(0, 5)` (PA5) to `PinId::new(1, 4)` (PB4) produces
`Pin<'B', 4, Output<PushPull>>`, `GPIOB.split(...)`, and `gpiob.pb4` without a
PB4-specific renderer branch. It splits only ports used by resolved resources,
so unused board hardware is not initialized.

Package validation proves only that a pin is bonded on the selected MCU
package. Connector routing, debugger conflicts, and external electrical
constraints remain explicit responsibilities of the board declaration author.
Task code imports its digital capability through `ferrowasp-io-core`; the
existing RTIC and SysTick-monotonic facade remains in `ferrowasp-stm32f4` for
this prototype.

The first button prototype deliberately has no debounce yet. A physical press
can therefore produce more than one EXTI edge; a later task can add a bounded
monotonic debounce policy without changing the board declaration.

## Verify

```text
cargo check --workspace --locked
cargo test -p xtask --locked
cargo xtask generate
cd targets/nucleo_f401re
cargo check --locked
cargo build --locked --release
cd ../ferrowasp_fcu3
cargo check --locked
cargo build --locked --release
```

The `task-check` workspace member builds a generated, host-only RTIC-shaped
context around every included reusable task. `cargo check --workspace`
therefore checks the original task files for logical fields, resource
operations, typed configuration fields, shared locking, monotonic calls, defmt
formatting, and task spawn signatures. It does not execute tasks or simulate
RTIC scheduling. The embedded target check remains authoritative for RTIC macro
expansion and HAL integration.

The app is a non-actuator validation prototype. It has no motor-output
authority.
