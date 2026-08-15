# App Builder V2 Foxeer Serial Vertical Slice

## 1. Objective

Generate a minimal, actuator-inhibited Foxeer F405 V2 RTIC application containing:

```text
USART2 SBUS UART endpoint
    -> reusable RC input software task
    -> RC observer snapshot
              |
              v
UART4 MSP UART endpoint
    -> reusable MSP OSD software task
    -> DJI O4 MSP responses
```

The implementation must prove:

1. Portable physical pin declaration through `PinId`.
2. Protocol-neutral serial-port board resources.
3. Reusable software and hardware tasks.
4. Reusable hardware endpoints.
5. Generated UART peripheral, RX DMA, and TX DMA interrupt tasks.
6. Portable serial reader and writer capability binding.
7. Application-wide pin, DMA, and interrupt conflict validation.
8. Readable generated RTIC source.

The target is not a flight application. It must contain no actuator output or arming path.

---

## 2. Current Implementation That Should Be Retained

The implementation should build incrementally on the current branch rather than replacing it wholesale.

Existing useful foundations include:

```text
hw_resources::PinId
hw_resources::SerialPortId
hw_resources::DmaChannel
hw_resources::UartRxDma

TaskDefinition
TaskDeclaration
HardwareInterrupt
TaskResourceCapability

ComponentLayer::HardwareEndpoint
SERIAL_PORT_COMPONENT

UART_RX_DMA_IRQ
UART_RX_IDLE_IRQ
COMMAND_INPUT

STM32F4 USART2 and UART4 RX route validation
Foxeer F405 generated target
```

The current serial model is receive-only, and the endpoint exports `UartRxParserSide`, exposing raw DMA-buffer ownership to its consumer. The serial endpoint also hardcodes task priority inside its reusable definition. These are the primary parts that must change.

The current command-input task polls the shared parser every millisecond rather than awaiting a portable serial reader. It should be replaced by the new endpoint capability model.

---

## 3. Protected Architectural Decisions

### 3.1 `PinId` Identifies Physical Pins

Retain the existing convention:

```rust
PinId::new(0, 3)  // PA3
PinId::new(0, 1)  // PA1
PinId::new(1, 10) // PB10
```

For STM32:

```text
0 -> GPIOA
1 -> GPIOB
2 -> GPIOC
...
```

`PinId` remains independent of:

- GPIO mode
- alternate function
- UART role
- MCU HAL types

The backend derives alternate-function selection from the resolved serial route.

### 3.2 A Serial Port Is a Physical Board Resource

The board declaration describes physical facts:

```text
peripheral identity
TX pin
RX pin
RX DMA route
TX DMA route
supported electrical capabilities
```

It does not own:

```text
SBUS parsing
MSP parsing
OSD behavior
RC policy
task priority
```

### 3.3 A Hardware Endpoint Owns Transport Mechanics

The UART DMA hardware endpoint owns:

```text
UART configuration
DMA transfer objects
interrupt services
RX DMA buffers
free and filled queues
owned RX channel
owned TX channel
transport lifecycle
transport discontinuity state
```

It exports only:

```text
SerialReader
DiscontinuityReader
SerialWriter
```

### 3.4 Reusable Software Tasks Own Behavior

The RC-input task owns:

```text
SBUS decoder state
RC snapshot publication
RC transport-discontinuity response
```

The MSP OSD task owns:

```text
MSP parser state
OSD state
reply buffer
telemetry construction
periodic overlay generation
```

Neither task owns the UART or DMA hardware.

### 3.5 Scheduling Belongs to `app_composition`

Reusable task and endpoint definitions do not hardcode final priorities.

For the first implementation, each UART endpoint receives:

```text
one interrupt priority
one optional worker priority
```

The UART peripheral, RX DMA, and TX DMA hardware tasks belonging to one endpoint use the same interrupt priority.

---

## 4. Milestone 1 — Complete `PinId`

### Work

Retain the existing structure:

```rust
pub struct PinId {
    pub port: u8,
    pub pin: u8,
}
```

Add:

```rust
impl PinId {
    pub const fn new(port: u8, pin: u8) -> Self;
    pub const fn port(self) -> u8;
    pub const fn pin(self) -> u8;
}
```

Optionally add authoring helpers:

```rust
pub const PA0: PinId = PinId::new(0, 0);
pub const PA1: PinId = PinId::new(0, 1);
pub const PA2: PinId = PinId::new(0, 2);
pub const PA3: PinId = PinId::new(0, 3);
```

A textual parser such as `"PA3" -> PinId` may be implemented for future TOML ingestion, but it is not required for this Rust-authored vertical slice.

### Critical Model Change

Remove the assumption that every `HardwareResource` has exactly one pin.

The current API contains:

```rust
HardwareResource::pin() -> PinId
```

That cannot represent a bidirectional serial port.

Replace it with something equivalent to:

```rust
pub enum PinRole {
    Primary,
    SerialTx,
    SerialRx,
}

pub struct PinClaim {
    pub role: PinRole,
    pub pin: PinId,
}

impl HardwareResource {
    pub fn pin_claims(&self) -> impl Iterator<Item = PinClaim>;
}
```

The board and STM32 backend validators must detect collisions across all claims.

### Acceptance Criteria

- Duplicate GPIO and serial-pin ownership is rejected.
- Serial TX and RX may not use the same physical pin unless a future explicit half-duplex mode permits it.
- Backend diagnostics print names such as `PA3`.
- Current GPIO tests remain valid.

---

## 5. Milestone 2 — Replace `UartRxDma` with `SerialPort`

### New Physical Resource

Replace the RX-only resource with:

```rust
pub struct SerialPort {
    pub id: &'static str,
    pub peripheral: SerialPortId,

    pub tx_pin: Option<PinId>,
    pub rx_pin: Option<PinId>,

    pub rx_dma: Option<DmaRoute>,
    pub tx_dma: Option<DmaRoute>,

    pub capabilities: SerialCapabilities,
}
```

A renamed `DmaRoute` may retain the existing fields:

```rust
pub struct DmaRoute {
    pub controller: u8,
    pub stream: u8,
    pub channel: u8,
}
```

The physical capabilities should include at least:

```rust
pub struct SerialCapabilities {
    pub rx: bool,
    pub tx: bool,
    pub rx_dma: bool,
    pub tx_dma: bool,
    pub idle_detection: bool,
}
```

The board may additionally declare which validated electrical profiles the route supports.

### Foxeer Resources

```rust
pub const SERIAL_2: HardwareResource =
    SerialPort::new("serial_2", SerialPortId::new(2))
        .tx_pin(PinId::new(0, 2))
        .rx_pin(PinId::new(0, 3))
        .rx_dma(DmaRoute::new(0, 5, 4))
        .supports_rx_dma()
        .supports_idle_detection()
        .into_resource();
```

```rust
pub const SERIAL_4: HardwareResource =
    SerialPort::new("serial_4", SerialPortId::new(4))
        .tx_pin(PinId::new(0, 0))
        .rx_pin(PinId::new(0, 1))
        .rx_dma(DmaRoute::new(0, 2, 4))
        .tx_dma(DmaRoute::new(0, 4, 4))
        .supports_bidirectional_dma()
        .supports_idle_detection()
        .into_resource();
```

The profile selected later determines:

```text
USART2:
    SBUS
    100000 baud
    even parity
    2 stop bits

UART4:
    MSP
    115200 baud
    no parity
    1 stop bit
```

### Acceptance Criteria

- Existing USART2 generation works through `SerialPort`.
- UART4 TX and RX routes validate.
- Missing TX DMA is rejected when a bidirectional endpoint is requested.
- DMA stream collisions are detected globally.

---

## 6. Milestone 3 — Introduce the High-Level Hardware-Endpoint Model

Do not immediately delete the existing generic `ComponentDefinition` expansion machinery.

Introduce:

```rust
HardwareEndpointDefinition
HardwareEndpointDeclaration
ResolvedHardwareEndpoint
```

The high-level endpoint API may lower into the existing component/resource/task IR during migration.

This allows the current resolver and renderer to remain useful while the user-facing model becomes explicit.

### Serial Endpoint Definition

Conceptually:

```rust
pub const SERIAL_DMA_ENDPOINT: HardwareEndpointDefinition =
    HardwareEndpointDefinition::new("serial_dma")
        .physical_port("port")
        .exports_rx_reader()
        .exports_discontinuities()
        .optional_tx_writer()
        .uses_peripheral_interrupt()
        .uses_rx_dma_interrupt()
        .optional_tx_dma_interrupt()
        .optional_tx_worker();
```

### Endpoint Declaration

```rust
let rc_uart =
    serial_dma::endpoint("rc_uart")
        .bind_port(board.serial_2)
        .profile(SerialProfile::sbus())
        .rx_buffer_bytes::<25>()
        .rx_buffer_count::<4>()
        .rx_queue_depth::<4>()
        .rx_only()
        .interrupt_priority(11);
```

```rust
let osd_uart =
    serial_dma::endpoint("osd_uart")
        .bind_port(board.serial_4)
        .profile(SerialProfile::msp())
        .rx_buffer_bytes::<70>()
        .rx_buffer_count::<4>()
        .rx_queue_depth::<4>()
        .tx_chunk_bytes::<70>()
        .tx_queue_depth::<16>()
        .bidirectional()
        .interrupt_priority(6)
        .worker_priority(4);
```

The concrete setter implementation may initially use the existing const declarations rather than the final procedural-macro API. The important requirement is that the endpoint declaration lowers into a typed normalized representation.

### Endpoint Exports

```rust
rc_uart.rx_reader
rc_uart.rx_discontinuities

osd_uart.rx_reader
osd_uart.rx_discontinuities
osd_uart.tx_writer
```

Raw parser and buffer-pool handles must remain private.

---

## 7. Milestone 4 — Update the Reusable Task Model

Implement only the task-authoring features required for this vertical slice.

### Required Capabilities

The reusable-task mechanism must support:

```text
synchronous hardware tasks
asynchronous software tasks
externally bound local resources
externally bound shared resources
task-owned local state
compile-time parameters
runtime spawn arguments
generated task-instance declarations
```

Priority must be removed from reusable task definitions.

### Compile-Time Parameters

The reusable task declares the parameter name and type:

```rust
pub async fn msp_osd<
    const REFRESH_PERIOD_MS: u32,
>(
    cx: msp_osd::Context<'_>,
) -> ! {
    // ...
}
```

The task macro generates the declaration method:

```rust
msp_osd::task()
    .refresh_period_ms::<10>()
```

The generated RTIC task itself should be concrete rather than generic.

### Task-Owned State

The authoring API should distinguish:

```text
bound resources:
    supplied by app_composition

task-owned local state:
    initialized once for every task instance
```

For example, the RC task owns its SBUS parser, while its serial reader is bound from an endpoint.

Conceptually:

```rust
#[ferrowasp::task(
    local = [
        decoder: SbusConsumer = SbusConsumer::new(),
    ],
    requires = [
        reader: SerialReader,
        discontinuities: DiscontinuityReader,
        rc_publisher: RcInputPublisher,
    ],
)]
pub async fn rc_input(
    cx: rc_input::Context<'_>,
) -> ! {
    // ...
}
```

The exact syntax can differ, but this ownership distinction is necessary to avoid manually declaring every parser object in the target.

---

## 8. Milestone 5 — Implement Reusable Endpoint Tasks

Create the following reusable tasks or endpoint services.

### UART Peripheral Interrupt

```text
serial_peripheral_irq
```

Responsibilities:

- service RX IDLE
- detect UART overrun
- detect parity, framing, and noise errors
- publish transport discontinuity
- service TX-complete later when required

It does not know whether the endpoint carries SBUS or MSP.

### RX DMA Interrupt

```text
serial_rx_dma_irq
```

Responsibilities:

- inspect RX DMA flags
- rotate DMA buffers
- create an owned RX chunk
- recycle the detached DMA buffer
- wake the portable reader
- record DMA or queue faults

### TX DMA Interrupt

```text
serial_tx_dma_irq
```

Responsibilities:

- acknowledge transfer completion
- acknowledge DMA errors
- signal the endpoint TX completion handle

Add:

```rust
HardwareInterrupt::DmaTx
```

to the existing interrupt-role model.

### TX Endpoint Worker

```text
serial_tx_worker
```

Responsibilities:

- await the next portable TX chunk
- start DMA
- await IRQ completion
- repeat

This is an endpoint-owned software task, not an OSD task.

### Scheduling

All three hardware tasks receive the endpoint’s declared interrupt priority.

The TX worker receives the endpoint worker priority.

---

## 9. Milestone 6 — Consolidate STM32F4 Endpoint Implementation

The backend must map the portable endpoint to the existing FerroWasp STM32F4 UART implementation.

### USART2 Mapping

```text
SerialPortId(2)
TX: PA2 AF7
RX: PA3 AF7
RX DMA: DMA1 Stream 5 Channel 4
Peripheral interrupt: USART2
RX DMA interrupt: DMA1_STREAM5
TX DMA: none for this endpoint
```

### UART4 Mapping

```text
SerialPortId(4)
TX: PA0 AF8
RX: PA1 AF8
RX DMA: DMA1 Stream 2 Channel 4
TX DMA: DMA1 Stream 4 Channel 4
Peripheral interrupt: UART4
RX DMA interrupt: DMA1_STREAM2
TX DMA interrupt: DMA1_STREAM4
```

The current backend already validates USART2 and UART4 receive routes, but its route enum and rendering are RX-specific. It must become a complete serial-route descriptor.

Conceptually:

```rust
struct Stm32f4SerialRoute {
    peripheral: &'static str,
    peripheral_interrupt: &'static str,

    tx_pin: Option<ResolvedAlternatePin>,
    rx_pin: Option<ResolvedAlternatePin>,

    rx_dma: Option<ResolvedDmaRoute>,
    tx_dma: Option<ResolvedDmaRoute>,
}
```

### Returned Endpoint Parts

The initialization renderer should receive:

```rust
SerialEndpointParts {
    peripheral_irq,
    rx_dma_irq,
    tx_dma_irq,
    tx_worker_state,

    rx_reader,
    rx_discontinuities,
    tx_writer,
}
```

The endpoint-private producer, owner, completion, raw queues, DMA buffers, and transfer state remain internal.

### Initial Performance Policy

Retain:

- owned RX chunk copies
- bounded queues
- fixed UART4 DMA buffer
- current fixed-length UART4 transfer if required for the first hardware checkpoint

Do not introduce zero-copy RX during this milestone.

---

## 10. Milestone 7 — Implement Reusable RC-Input Task

Replace the current polling task with an asynchronous portable reader.

Conceptually:

```rust
pub async fn rc_input(
    cx: rc_input::Context<'_>,
) -> ! {
    let mut bytes = [0_u8; 25];

    loop {
        let count = cx.local.reader.read(&mut bytes).await?;

        if let Some(discontinuity) =
            cx.local.discontinuities.take_new()
        {
            cx.local.decoder.reset();
            cx.local.rc_publisher.invalidate(discontinuity);
        }

        for frame in cx.local.decoder.push_bytes(&bytes[..count]) {
            if let Ok(snapshot) = frame {
                cx.local.rc_publisher.publish(snapshot);
            }
        }
    }
}
```

The minimum RC snapshot should contain:

```rust
pub struct RcInputSnapshot {
    pub roll: i16,
    pub pitch: i16,
    pub yaw: i16,
    pub throttle: u16,
    pub arm_high: bool,
    pub frame_lost: bool,
    pub failsafe: bool,
}
```

For this actuator-inhibited application, an observer channel is sufficient. Authoritative safety channels are deferred until the generated flight-control path is implemented.

---

## 11. Milestone 8 — Implement Minimal MSP OSD Task

Add a new reusable software task:

```text
msp_osd
```

It binds:

```text
UART4 SerialReader
UART4 DiscontinuityReader
UART4 SerialWriter
RC observer reader
```

It owns:

```text
MSP parser
OSD state
TX frame buffer
refresh counter
```

### Minimum Behavior

The task should:

1. consume incoming DJI O4 MSP polls;
2. respond using the existing MSP implementation;
3. periodically send the minimum required DisplayPort heartbeat;
4. display a static FerroWasp identifier;
5. display current RC roll, pitch, yaw, and throttle;
6. reset its parser after RX discontinuity;
7. mark TX unhealthy after a terminal transport error.

No battery, IMU, attitude, PID, or menu support is required.

Conceptually:

```rust
msp_osd::task()
    .refresh_period_ms::<10>()
    .bind_reader(osd_uart.rx_reader)
    .bind_discontinuities(osd_uart.rx_discontinuities)
    .bind_writer(osd_uart.tx_writer)
    .bind_rc_observer(rc_input.output)
    .priority(3)
    .spawn_on_init();
```

---

## 12. Milestone 9 — Foxeer Board and Composition

### Board Declaration

The Foxeer board must expose both serial resources:

```rust
pub const BOARD: BoardDeclaration = BoardDeclaration {
    id: "foxeer_f405_v2",
    target: Target::external_crystal(
        Mcu::Stm32F405,
        8_000_000,
        168_000_000,
        true,
    ),
    monotonic: MonotonicDeclaration::SysTick {
        id: "Mono",
        clock_hz: 168_000_000,
    },
    hardware: &[
        SERIAL_2,
        SERIAL_4,
    ],
};
```

### Application Composition

The final user-facing shape should be approximately:

```rust
pub fn app() -> Application {
    let rc_uart =
        serial_dma::endpoint("rc_uart")
            .bind_port(board::SERIAL_2)
            .profile(SerialProfile::sbus())
            .rx_only()
            .interrupt_priority(11);

    let osd_uart =
        serial_dma::endpoint("osd_uart")
            .bind_port(board::SERIAL_4)
            .profile(SerialProfile::msp())
            .bidirectional()
            .interrupt_priority(6)
            .worker_priority(4);

    let rc_input =
        rc_input::task()
            .bind_reader(rc_uart.rx_reader)
            .bind_discontinuities(rc_uart.rx_discontinuities)
            .priority(10)
            .spawn_on_init();

    let osd =
        msp_osd::task()
            .refresh_period_ms::<10>()
            .bind_reader(osd_uart.rx_reader)
            .bind_discontinuities(osd_uart.rx_discontinuities)
            .bind_writer(osd_uart.tx_writer)
            .bind_rc_observer(rc_input.rc_observer)
            .priority(3)
            .spawn_on_init();

    Application::new()
        .endpoint(rc_uart)
        .endpoint(osd_uart)
        .task(rc_input)
        .task(osd)
}
```

The exact API may initially lower into existing `AppDeclaration`, `ComponentDeclaration`, and task declarations.

The important result is that target code no longer binds resources through strings such as:

```rust
resource("rx").to_sw("uart2_rx")
```

except inside the normalized internal representation.

---

## 13. Milestone 10 — Resolver and Validation

The builder must validate the following.

### Pins

- PA2 owned only by USART2 TX.
- PA3 owned only by USART2 RX.
- PA0 owned only by UART4 TX.
- PA1 owned only by UART4 RX.

### DMA

- DMA1 Stream 5 owned by USART2 RX.
- DMA1 Stream 2 owned by UART4 RX.
- DMA1 Stream 4 owned by UART4 TX.
- No stream may be claimed twice.

### Interrupts

- USART2 owned by the RC endpoint.
- UART4 owned by the OSD endpoint.
- DMA vectors owned by their endpoint services.
- No endpoint vector may also be selected as an RTIC software dispatcher.

### Endpoint Completeness

- SBUS endpoint has RX and RX DMA.
- MSP endpoint has RX, TX, RX DMA, and TX DMA.
- TX worker priority exists for a TX-enabled endpoint.
- All services sharing endpoint state have compatible priorities.

### Capability Ownership

- One reader per serial RX stream.
- One writer authority per serial TX stream.
- Endpoint-private handles cannot be bound by functional tasks.
- OSD cannot bind the RC endpoint writer because none exists.

### Task Completeness

- All required compile-time parameters supplied.
- All required endpoint capabilities bound.
- Priority assigned in `app_composition`.
- Task-instance names unique.

---

## 14. Generated RTIC Result

The generated app should contain approximately:

```text
Shared resources:
    rc_uart RX transfer state
    osd_uart RX transfer state
    osd_uart TX DMA state
    RC observer state

Local resources:
    RC serial reader
    RC discontinuity reader
    SBUS parser
    RC observer publisher

    OSD serial reader
    OSD discontinuity reader
    OSD serial writer
    MSP parser
    OSD state
    OSD TX buffer

    OSD TX endpoint owner
    OSD TX completion handle
```

Generated hardware tasks:

```text
USART2
DMA1_STREAM5

UART4
DMA1_STREAM2
DMA1_STREAM4
```

Generated software tasks:

```text
rc_input
msp_osd
osd_uart_tx_worker
```

The RTIC source must remain normal readable Rust.

---

## 15. Verification Checkpoints

### Checkpoint A — Model and Generation

- `PinId` tests pass.
- Serial-port route tests pass.
- Endpoint expansion tests pass.
- Generated Foxeer app is deterministic.
- `cargo check` succeeds.

### Checkpoint B — USART2 RC Input

- Valid SBUS frames are decoded.
- RC observer values change with stick inputs.
- Frame-lost and failsafe flags are observable.
- RX DMA and UART IDLE paths both deliver data.
- DMA discontinuity resets the parser.

### Checkpoint C — UART4 MSP OSD

- DJI O4 polling receives valid MSP replies.
- DisplayPort heartbeat remains active.
- Static FerroWasp text is displayed.
- TX DMA completion wakes the endpoint worker.
- Injected TX DMA failure stops or degrades TX predictably.

### Checkpoint D — Combined Vertical Slice

- SBUS stick movement updates values displayed in the OSD.
- Both UART endpoints operate simultaneously.
- No motor-output peripheral exists in the resolved application.
- Generated resource report lists every pin, DMA stream, vector, buffer, queue, and owner.

---

## 16. Explicitly Deferred Work

Do not include the following in this implementation:

```text
zero-copy UART RX
variable-length UART4 DMA optimization
dynamic serial protocol switching
CRSF
MAVLink
full MSP configuration
OSD menus
battery and IMU telemetry
control loop
safety master
actuator output
DShot
STM32H7
STM32G4
RT1060 shared-vector dispatch
automatic endpoint recovery
wire-complete TX flush semantics
```

The endpoint resolver should not be designed in a way that prevents shared-vector grouping later, but RT1060 support is not part of this vertical slice.

---

## 17. Definition of Done

The implementation is complete when:

1. `PinId` is the only physical pin identity used by board declarations.
2. `SerialPort` represents both RX-only and bidirectional UART hardware.
3. USART2 and UART4 are declared once in the Foxeer board definition.
4. Hardware endpoints own all UART, DMA, interrupt, buffer, and channel mechanics.
5. Endpoint interrupt priorities are supplied by `app_composition`.
6. RC input and MSP OSD are reusable software tasks.
7. The RC task consumes a portable asynchronous reader.
8. The OSD task consumes portable reader and writer handles.
9. No functional task accesses `UartRxParserSide`, DMA buffers, or HAL UART types.
10. The generated app compiles for STM32F405.
11. SBUS stick inputs are visible through the DJI O4 OSD.
12. The generated application remains actuator-inhibited.
