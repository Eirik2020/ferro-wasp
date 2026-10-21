# UART

UART support uses bounded DMA-backed paths for RC input, OSD traffic, and the
flight image's legacy ESC telemetry.

## Current Implementation

The reusable STM32F4 UART DMA mechanism lives in
`crates/ferrowasp-stm32f4/src/uart_dma.rs`. Each board's UART routes are data
in its board crate (`board/src/serial.rs`); peripheral construction is in the
app's `src/board/`.

It provides:

- mode-specific UART configuration
- RX DMA setup
- UART4 TX DMA support for the OSD path
- four fixed receive buffers
- free and filled `heapless` queues
- split IRQ-side and parser-side ownership
- IDLE interrupt handling
- transfer-complete handling

Current UART modes:

| Mode | Baud/config | Intended use |
|---|---|---|
| `Sbus` | 100000 baud, even parity, 2 stop bits, RX DMA | Active RC input path |
| `Msp` | 115200 baud, TX/RX DMA | Active DJI O4 OSD path on UART4 |
| `EscTelemetry` | 115200 baud, 8N1, RX DMA | BLHeli legacy ESC telemetry |
| `Crsf` | 420000 baud, 8N1, not inverted, TX/RX DMA | RC input with battery telemetry when `rc_protocol` is `crsf` |
| `Cli` | 115200 baud, 8N1, TX/RX DMA | The text command line, for a configurator over a cable or Bluetooth (`configurator`) |
| `Mavlink` | 57600 baud, RX DMA | Future telemetry/config subset |

## Port Binding

Pins, DMA streams and interrupts are fixed per port at compile time, in each
board's data crate route table (`board/src/serial.rs`). A port is named by the UART's
number on the chip, as the pilot sees it on the board: USART3 is `uart3`
whatever it serves. Saved configuration holds a function for each of
`uart1` to `uart8`. Which function a port serves (RC input, MSP OSD, ESC
telemetry, configurator) is chosen at boot from that table:

- Init reads the saved bindings once, checks each against the port's
  capabilities (`resolve_bindings` in `ferrowasp-serial`), starts every bound
  port with its function's line settings, and moves the port's stream into the
  function's task. A port with nothing bound is never enabled.
- A binding the port cannot carry, or a function bound to two ports, is
  dropped and logged; the rest still apply. With no RC port the RC link never
  becomes valid, so the craft cannot arm.
- Until a pilot saves bindings the board's defaults apply. Over USB,
  `serial` shows them and `serial <port> <function>` stages a change, which
  takes effect after `config save` and a reboot. Changes are refused while
  armed. FerroConfigurator's `config ports` and `config bind` do the same and
  verify the saved table by readback.
- Port transports are role-free and chip-neutral
  (`crates/ferrowasp-stm32/src/uart_port.rs`): they record faults on the
  stream, and the function task decides what a fault means. Each family's
  backend starts one port at a time (`start_rx_tx_port`, `start_rx_port`);
  a board's `init_uart_ports`, in its `src/board/`, chooses which ports, pins,
  and DMA streams it has, and
  `crates/ferrowasp-stm32-critical/src/tasks/uart_port.rs` defines the
  receive and transmit tasks of every logical port for a board to bind. All UART
  transports run at priority 11, since any port may serve any function; the
  function tasks keep their own priorities.

Each function speaks one protocol today, so the protocol follows from the
function. A function gains a protocol setting when it gets a second protocol.

## Default Bindings

On Foxeer, USART2 is routed to RC input in SBUS mode:

```text
USART2 RX DMA/IDLE IRQ -> owned RxChunk -> persistent async SBUS parser
    -> RC validity + rates/throttle/arm
```

Foxeer uses PA2/PA3 for USART2 TX/RX.

UART4 is routed to the DJI O4 MSP OSD path:

```text
UART4 RX DMA/IDLE IRQ -> owned RxChunk -> osd_refresh task -> MSP parser/responder
OSD frame queue -> UART4 TX DMA -> DJI O4 air unit
```

Foxeer uses PA0/PA1 for UART4 TX/RX.

USART1 is routed to the combined BLHeli legacy ESC telemetry wire. On Foxeer,
UART4 can carry ESC telemetry instead:

```text
PA10 USART1 RX DMA -> bounded chunks -> ESC manager parser/association
    -> timestamped per-motor observations
```

PA9/USART1 TX is not configured.

On Foxeer, USART3 (`uart3`, PC10/PC11, the R3/T3 pads) carries the
configurator command line for log sync over a Bluetooth serial module:

```text
USART3 RX DMA/IDLE IRQ -> owned RxChunk -> configurator_link -> command queue
    -> flash manager -> response queue -> configurator_link -> USART3 TX DMA
```

USB and the configurator port each have their own command and response queue
into the flash manager, which answers every command on the link it came from
(`crates/ferrowasp-cli/src/command_link.rs`). Motor commands and the
unconditional log erase stay on USB.

On the TBS Lucid H7, USART1 (`uart1`, PA10, receive only) takes RC input from
the digital VTX connector's SBUS pin, inverted in the UART; USART3 (`uart3`)
carries MSP DisplayPort with TX DMA; UART8 (`uart8`, PE0) carries ESC
telemetry; and USART6 (`uart6`) is spare. Every H743 UART inverts in hardware,
so any port can take SBUS or ESC telemetry; `uart3` and `uart6` have the
transmit DMA that MSP and CRSF need. The Lucid has no configurator port yet.

The ESC manager never writes motor hardware; how it pairs a telemetry reply
with the motor that was asked is in [DShot](./dshot.md#esc-telemetry).

## Design Intent

The UART layer should only move bytes and frame buffers. Protocol parsers should interpret those bytes, and safety-critical authority should remain elsewhere.

For example:

- SBUS may update pilot setpoints and arm-switch intent.
- MSP or MAVLink may request configuration changes.
- No UART protocol should directly write motor output or actuator permission.

See [UART RX](./uart_rx.md) for the buffer ownership model.
