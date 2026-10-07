# UART

UART support uses bounded DMA-backed paths for RC input, OSD traffic, and the
flight image's legacy ESC telemetry.

## Current Implementation

The reusable STM32F4 UART DMA mechanism lives in
`crates/ferrowasp-stm32f4/src/uart_dma.rs`. Each board's pin conversion,
storage shape, and device construction live in its app's `src/board/`.

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
| `EscTelemetry` | 115200 baud, 8N1, RX DMA | Standard flight-board BLHeli legacy telemetry path on USART1 |
| `Mavlink` | 57600 baud, RX DMA | Future telemetry/config subset |

## Port Binding

Pins, DMA streams and interrupts are fixed per port at compile time, in each
board's `src/board/serial.rs` route table. A port is named by the UART's
number on the chip, as the pilot sees it on the board: USART3 is `uart3`
whatever it serves. Saved configuration holds a function for each of
`uart1` to `uart8`. Which function a port serves (RC input, MSP OSD, ESC
telemetry) is chosen at boot from that table:

- Init reads the saved bindings once, checks each against the port's
  capabilities (`resolve_bindings` in `ferrowasp-io-core`), starts every bound
  port with its function's line settings, and moves the port's stream into the
  function's task. A port with nothing bound is never enabled.
- A binding the port cannot carry, or a function bound to two ports, is
  dropped and logged; the rest still apply. With no RC port the RC link never
  becomes valid, so the craft cannot arm.
- Until a pilot saves bindings the board's defaults apply. Over USB,
  `serial` shows them and `serial <port> <function>` stages a change, which
  takes effect after `config save` and a reboot. Changes are refused while
  armed.
- Port transports are role-free and chip-neutral
  (`crates/ferrowasp-stm32f4/src/uart_port.rs`): they record faults on the
  stream, and the function task decides what a fault means. Each family's
  backend starts its ports (`init_f405_uart_ports`, `init_h743_uart_ports`),
  and `crates/ferrowasp-stm32f4-tasks/src/uart_port.rs` defines the receive
  and transmit tasks of every logical port for a board to bind. All UART
  transports run at priority 11, since any port may serve any function; the
  function tasks keep their own priorities.

Each function speaks one protocol today, so the protocol follows from the
function. A function gains a protocol setting when it gets a second protocol.

## Default Bindings

On the F405 boards, USART2 is routed to RC input in SBUS mode:

```text
USART2 RX DMA/IDLE IRQ -> owned RxChunk -> persistent async SBUS parser
    -> RC validity + rates/throttle/arm
```

The current prototype uses PA2/PA3 for USART2 TX/RX.

UART4 is routed to the DJI O4 MSP OSD path:

```text
UART4 RX DMA/IDLE IRQ -> owned RxChunk -> osd_refresh task -> MSP parser/responder
OSD frame queue -> UART4 TX DMA -> DJI O4 air unit
```

The current prototype uses PA0/PA1 for UART4 TX/RX.

USART1 is routed to the combined BLHeli legacy ESC telemetry wire. On Foxeer,
UART4 can carry ESC telemetry instead:

```text
PA10 USART1 RX DMA -> bounded chunks -> ESC manager parser/association
    -> timestamped per-motor observations
```

PA9/USART1 TX is not configured.

On the TBS Lucid H7, USART6 (`uart6`) carries SBUS, inverted in the UART;
USART3 (`uart3`) carries MSP DisplayPort with TX DMA; and UART8 (`uart8`)
carries ESC telemetry on PE0. In the standard flight image, the manager sends typed telemetry requests through
a bounded queue to the DShot actuator service; it never writes motor hardware
itself. A CRC-valid response seen before the matching frame-start
acknowledgement remains quarantined until that exact sequence/output
acknowledgement arrives.

## Design Intent

The UART layer should only move bytes and frame buffers. Protocol parsers should interpret those bytes, and safety-critical authority should remain elsewhere.

For example:

- SBUS may update pilot setpoints and arm-switch intent.
- MSP or MAVLink may request configuration changes.
- No UART protocol should directly write motor output or actuator permission.

See [UART RX](./uart_rx.md) for the buffer ownership model.
