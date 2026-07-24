# Experimental USART1 DMA OSD application

`nucleo-f401re-osd` is an isolated builder application for exercising the
static serial endpoint plus software consumer architecture. It does not edit,
link into, or replace a FerroWasp flight application. Live FerroWasp target
status and evidence must be checked from the monorepo root at a pinned commit.
Terminology and the resulting architectural refactor are defined in
`architecture-observations.md`.

## Ownership boundary

The generated RTIC hardware layer owns USART1, DMA2 streams 5 and 7, all three
interrupt handlers, and fixed transfer buffers. RX idle and RX-DMA events are
two producers of one bounded MPSC work channel. A once-started divergent OSD
task is its sole consumer. The OSD task produces bounded TX chunks for a
once-started divergent TX worker, and the TX-DMA interrupt reports completion
through a separate capacity-one channel. Hardware producers never block;
overflow rejects the new item and records a typed fault.

In the agreed terminology, the reusable UART-DMA provider is a component and
this USART1/pin/DMA instantiation is an endpoint. The compatibility types are
deliberately USART1-specific and do not claim multi-instance or multi-UART
support. Directed channels expose RX-publish/consume, TX-emit/handle, and TX
completion edges independently.

The OSD software component owns only MSP parser/responder state and its output
buffer. It consumes `OsdWork`, a copied `OsdTelemetrySnapshot`, and a bounded
TX callback; it has no USART, pin, DMA, PAC, or interrupt type in its public
boundary. Telemetry is copied under a short RTIC lock, while parsing,
rendering, and TX enqueueing happen outside shared locks.

The separate `button_arm_toggle` consumer uses B1 on PC13, EXTI15_10, and a
20 ms logical debounce delay from the shared monotonic to toggle the shared demonstration telemetry state. It does not
represent or call FerroWasp's safety/arming state machine. A change queues an
updated `ARMED`/`DISARMED` row and `DRAW_SCREEN` on the next 100 ms refresh.

```text
USART1 IDLE ----\
                 +-> bounded MPSC work channel -> divergent MSP/OSD task
RX DMA IRQ -----/                                |
                                                  v
                                      bounded SPSC TX channel
                                                  |
                                                  v
                                      divergent TX DMA worker
                                                  ^
                                                  |
                                      SPSC completion channel
                                                  ^
                                                  |
                                            TX DMA IRQ
```

The BSP keeps physical endpoint facts explicit, while the application keeps
buffer and scheduling policy explicit:

- USART1 TX PA9 and RX PA10; the backend derives AF7
- RX DMA2 stream 5 channel 4 and TX DMA2 stream 7 channel 4
- two 70-byte RX DMA buffers and one 70-byte TX DMA buffer
- RX queue capacity 4 and TX queue capacity 16
- fixed-delay OSD refresh task every 100 ms; refresh is the third MPSC work
  producer
- USART/RX-DMA/TX-DMA priorities 4, TX worker priority 3, OSD priority 2
- EXTI0, EXTI1, and EXTI2 as RTIC software-task dispatchers
- B1/PC13 with EXTI15_10, a 20 ms logical debounce delay, and priority 5

The prototype statically applies the backend's `msp_displayport` profile
(115200 baud, 8-N-1). In the target architecture, persisted platform
configuration selects `msp_displayport`, `sbus`, `crsf`, or another supported
component at boot. That selection supplies baud/framing/direction/inversion and
is frozen until reboot; it is not stored in the BSP or application manifest.

The current prototype packages endpoint and consumer in one builder bundle
because the legacy generator compiles every feature prefix. This is a
transitional constraint, not a target architecture rule. The resolved-graph
pipeline will model endpoint and consumer separately and compile complete,
semantically valid checkpoints.

## Failure and safety contract

This is a `Validation` application with no actuator owner. Its typed fault
catalogue covers RX/TX DMA failures, work/TX/completion channel overflow or
closure, and debounce spawn failure. Faults are bounded counters with stable
IDs, severity, transient semantics, and no arming effect. Once-at-boot spawns
for divergent tasks are explicit panic/halt invariants; runtime failures are
recorded rather than silently ignored. The checked contract is
`architecture-contracts/nucleo-f401re-osd.toml`.

## FerroWasp provenance and replacement

This implementation reuses design and protocol code from an external
FerroWasp source snapshot at commit
`bc26276c5b34e20f96f603d95585893b91784d05`:

- `apps/foxeer-f405-v2/src/main.rs` for RTIC task wiring and ownership
- `crates/ferrowasp-stm32f4/src/uart_dma.rs` for DMA transfer handling
- `crates/ferrowasp-io-core/src/serial/` for bounded serial endpoint semantics
- `crates/ferrowasp-tasks/src/osd.rs` for the OSD consumer boundary
- `crates/ferrowasp-mspv1` for MSP parsing and responses

The Foxeer board uses UART4 PA0/PA1 with DMA1 streams 2/4 for DJI OSD. This
application deliberately specializes the same pattern to the requested
NUCLEO USART1 PA9/PA10 mapping; it does not claim that this is the Foxeer
hardware route.

The adapter also ports Foxeer's periodic refresh behavior. Each scheduled refresh
queues a heartbeat plus one entry from the same 15-step clear, text, status,
and draw sequence. A complete overlay is committed about 1.5 seconds after
startup and repeats continuously; MSP request responses continue to share the
same bounded TX path.

`compat/ferrowasp-mspv1` is an unchanged protocol copy. The standalone
`compat/ferrowasp-serial-osd` crate is the narrow STM32F401 adapter. Both have
provenance files and are temporary: when FerroWasp exposes a stable reusable
crate boundary, point the generated application at those canonical crates and
delete both compatibility directories. Do not develop a parallel protocol or
flight stack here.

## Commands and hardware scope

```powershell
cargo xtask generate --app nucleo-f401re-osd
cargo xtask build --app nucleo-f401re-osd
cargo xtask flash --app nucleo-f401re-osd
cargo xtask embed --app nucleo-f401re-osd
```

For bench testing, connect PA9 to the VTX RX input, PA10 to the VTX TX output,
and share ground only after confirming compatible logic levels. Keep motors
and other hazardous outputs disconnected. The example uses default telemetry
values, so the visible rows are a transport smoke test rather than live flight
telemetry.
