# NUCLEO-F401RE RTIC prototype app

This self-contained embedded target crate builds the generated, non-actuator
RTIC application in `src/generated_app.rs` and its selected imports in
`src/prelude.rs`. Its handwritten inputs are `src/board.rs`,
`src/app_composition.rs`, and the unified reusable task modules under the
builder's `tasks/` directory.

From `tools/app-builder-v2`, generate the RTIC shell and tasks with:

```text
cargo xtask generate
```

The command creates RTIC task attributes from concrete `TaskDeclaration`
instances, combines them with their reusable `TaskDefinition` bodies, and
validates the complete Rust syntax before replacing this target's generated
`src/generated_app.rs` and `src/prelude.rs`.

The target uses the NUCLEO-F401RE's existing sandbox hardware contract:

- PA5 / LD2 is a push-pull status LED;
- PC13 / B1 is an active-low EXTI input;
- SysTick provides the 1 kHz RTIC monotonic;
- EXTI0 and EXTI1 are software-task dispatchers;
- EXTI15_10 is bound to the B1 hardware task;
- USART2 RX on PA3 uses DMA1 Stream 5 Channel 4 for raw or SBUS input;
- DMA1_STREAM5 and USART2 service full-buffer and IDLE receive events.

The generated app keeps the blink task scheduled and uses B1 to toggle its
shared enable flag. While disabled, the task drives LD2 low. The initial EXTI
prototype is not debounced, so one physical press can occasionally cause more
than one toggle. This validation app has no actuator or motor-output authority.

`src/app_composition.rs` configures the endpoint with `SerialProtocol::Raw`
for 115200-baud 8N1 input. A separate COMPORT functional component consumes
the endpoint's raw RX interface and prints complete lines as `COMPORT: ...`
through the defmt RTT terminal. CR, LF, and CRLF terminate messages; invalid
UTF-8 is logged as bytes, and lines longer than 64 bytes are discarded with
one warning. The endpoint itself does not parse or echo bytes.

An SBUS command-input composition may instead select `SerialProtocol::Sbus`
and bind a command-input component to the same raw endpoint interface.
Standard SBUS is electrically inverted, but the STM32 UART configuration used
here is not: PA3 must receive an already uninverted, 3.3-volt-compatible
signal, such as through a suitable inverter or a receiver's uninverted output.

Do not connect a USB-to-UART adapter and an SBUS receiver to PA3 at the same
time. The adapter must use 3.3-volt logic and share ground with the board. PA3
is also connected to the Nucleo ST-LINK virtual COM route by default, so check
the board solder-bridge configuration before attaching external hardware. A
successful build does not establish that the electrical path or DMA reception
has been validated on hardware.

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
