# UART

UART support uses bounded DMA-backed paths for RC input, OSD traffic, and the
default FCU3 DShot image's legacy ESC telemetry.

## Current Implementation

The reusable STM32F4 UART DMA mechanism lives in
`crates/ferrowasp-stm32f4/src/uart_dma.rs`. FerroWasp FCU3 pin conversion,
storage shape, and device construction live in
`crates/ferrowasp-bsp/src/stm32f4/ferrowasp_fcu3/`.

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
| `EscTelemetry` | 115200 baud, 8N1, RX DMA | Active FCU3 BLHeli legacy telemetry path on USART1 in the default DShot image; inactive in the PWM fallback |
| `Mavlink` | 57600 baud, RX DMA | Future telemetry/config subset |

## Active Use

USART2 is currently routed to RC input in SBUS mode:

```text
USART2 RX DMA/IDLE IRQ -> owned RxChunk -> persistent async SBUS parser
    -> RC validity + rates/throttle/arm
```

The current prototype uses PA2/PA3 for USART2 TX/RX.

UART4 is currently routed to the DJI O4 MSP OSD path:

```text
UART4 RX DMA -> filled buffer queue -> osd_refresh task -> MSP parser/responder
OSD frame queue -> UART4 TX DMA -> DJI O4 air unit
```

The current prototype uses PA0/PA1 for UART4 TX/RX.

In the default DShot image, USART1 is routed to the combined BLHeli legacy ESC
telemetry wire:

```text
PA10 USART1 RX DMA -> bounded chunks -> ESC manager parser/association
    -> timestamped per-motor observations
```

PA9/USART1 TX is not configured. The explicit PWM fallback does not run the ESC
manager. In the DShot image, the manager sends typed telemetry requests through
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
