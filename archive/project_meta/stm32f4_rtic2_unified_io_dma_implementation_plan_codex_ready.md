# STM32F4 RTIC 2 Unified I/O and DMA Implementation Plan

> Historical implementation plan. Several work packages have since been
> completed or revised. It is retained for design history and must not be used
> as the current board, task, or actuator contract. See
> `project_docs/CODEX_PROJECT_CONTEXT.md`, `project_docs/CODEX_ACTIVE_WORK.md`,
> and `mdbook/src/current_support.md` for the live baseline.

**Revision:** Codex-ready architecture baseline  
**Date:** 2026-07-17  
**Primary reference target:** STM32F405RGT6, LQFP64  
**Family scope:** STM32F4 devices supported by the selected `stm32f4xx-hal` revision  
**Runtime:** RTIC 2.2  
**Language:** Rust, `#![no_std]`  
**Safety policy:** no `unsafe` code in FerroWasp-owned application, driver, BSP, task, or STM32F4 backend crates  
**Subsystems:** DMA-backed serial, DMA-backed SPI, static PWM, timer-DMA finite waveforms, DShot, and continuous waveform streaming  
**Portable API baseline:** `embedded-hal` 1.0, `embedded-hal-async` 1.0, `embedded-hal-nb` 1.0, `embedded-io` 0.7, and `embedded-io-async` 0.7

This document supersedes the earlier unified plan and the three original UART, SPI, and PWM/timer-DMA plans. It is intended to be sufficiently explicit for Codex to implement in bounded work packages without inventing architecture.

The examples use FerroWasp naming, but the architecture is deliberately usable by unrelated STM32F4 RTIC applications and by portable device-driver crates.

---

## 1. Final decisions

The following decisions are frozen for the first implementation. Changing one requires an Architecture Decision Record and updated acceptance tests.

| Topic | Decision |
|---|---|
| Family implementation | One `ferrowasp-stm32f4` crate owns reusable STM32F4 serial-DMA, SPI-DMA, timer-DMA, clock, memory, fault, and recovery mechanics. |
| Portable driver boundary | External-device and protocol crates depend on Rust Embedded traits, not RTIC, STM32, DMA, or BSP types. |
| Board routing | Each PCB target provides typed Rust construction plus a descriptive resource manifest. Runtime pin or DMA remapping is not supported. |
| RTIC shell | Concrete IRQ bindings, priorities, `Shared`, `Local`, `init`, and task declarations stay in the target firmware binary. |
| RTIC code generation | No RTIC binding generator or procedural macro during initial implementation. Repetitive wrappers remain explicit until two targets are working. |
| Modularity | Closely coupled state and recovery logic stay in the same subsystem module. Do not split components merely to create more crates. |
| Serial ownership | One configured serial backend is an RTIC shared resource because USART, RX-DMA, and TX-DMA tasks all require bounded access. |
| SPI ownership | One SPI engine is local to one owner hardware task. Other producers publish atomic event flags or a single owned request and pend that owner IRQ. |
| Actuator ownership | The paired actuator engine is an RTIC shared resource used only by actuator DMA IRQs, the control-output task, and the highest-priority safety shutdown path. |
| Queues | Use `rtic_sync::channel::Channel` for ordered bounded data, `Signal` for latest-only wakeups/completions, and `Watch` for observable latest state. |
| Static allocation | Use `StaticCell`, RTIC static locals, and fixed-capacity `heapless` storage. No heap. |
| SPI concurrency | Initial target has one device per physical SPI bus. Each `SpiDevice` handle is unique and non-clone. Shared-bus support is deferred. |
| SPI cancellation | Async SPI transactions are cancellation-safe. Dropping the future requests cancellation; the owner safely aborts or completes and discards the result. |
| Timeout source | Reserve TIM2 as the first target’s RTIC monotonic/timebase. A periodic high-priority I/O watchdog checks active transfer deadlines. |
| Output modes | Static PWM, DShot, and continuous-waveform demonstrations use separate firmware profiles/binaries. They are not runtime-polymorphic in the first implementation. |
| Timer DMA-burst | Use a pinned safe API from an upstream or project-maintained `stm32f4xx-hal` fork. Do not add local unsafe code to FerroWasp crates. |
| SPI priorities | All SPI owner IRQ tasks use the same RTIC priority. SPI RX DMA uses equal hardware priority across buses; TX is one DMA priority level lower. |
| First Codex scope | Codex initially implements Work Packages 0–2 only. Later work packages are separate reviewed tasks. |

### 1.1 Core architectural rule

> Standard traits define portable operations. Typed BSP targets define electrical reality. The STM32F4 backend defines DMA mechanics. RTIC defines scheduling and ownership. Domain and safety layers define policy and actuator authority.

### 1.2 Why there is no universal DMA engine

Serial, SPI, and timer-DMA have different invariants:

- Serial RX is an indefinite stream with IDLE/full-buffer races and transport discontinuities.
- SPI is a bounded transaction with chip-select scope, paired RX/TX DMA, timeout, and cancellation.
- Timer-DMA controls physical outputs and requires synchronized start, terminal-low behavior, and fail-low shutdown.

They share support conventions, error style, timestamps, statistics, and resource validation. They do not share one state machine.

---

## 2. Scope

### 2.1 Required capabilities

The implementation shall support:

- Multiple logical serial ports backed by independently selected STM32 USART instances and legal DMA routes.
- Reboot-applied serial protocol selection within each target port’s declared electrical capabilities.
- MAVLink, CRSF, and SBUS parser/transport integration without protocol parsing in hardware IRQs.
- Multiple independent SPI buses using one generic STM32F4 SPI-DMA engine implementation.
- Portable IMU and NOR-flash drivers generic over `SpiDevice`.
- Ordinary PWM through `embedded_hal::pwm::SetDutyCycle`.
- Finite DMA waveform transmission, initially DShot.
- Continuous double-buffered waveform streaming.
- A complete compile-time board resource map and host-testable conflict manifest.
- Safe fixed-capacity allocation and explicit overload behavior.
- Structured faults, health, statistics, and latched actuator failure behavior.
- A thin RTIC app with no register, pin-AF, DMA-stream, or HAL-transfer algorithms.

### 2.2 Initial use cases

```text
UART1 -> SBUS RC input
UART2 -> CRSF RC/telemetry or MAVLink
UART3 -> MAVLink or CRSF
SPI1  -> primary IMU
SPI2  -> blackbox NOR flash
TIM1 + TIM3 -> four synchronized DShot outputs
```

These are reference assignments, not hardcoded family policy.

### 2.3 Non-goals

The first implementation does not include:

- Runtime physical pin, DMA, timer, or IRQ remapping.
- Automatic route solving inside firmware.
- A universal board-description language.
- Dynamic allocation or trait objects in real-time paths.
- Arbitrary numbers of SPI clients on one bus.
- Software SBUS inversion.
- Bidirectional DShot on the PB15 complementary-output route.
- Protocol auto-detection.
- Dynamic switching between static PWM, DShot, and waveform mode while running.
- A generated RTIC app before explicit wrappers have been validated on two boards.
- A claim of safety certification.

---

## 3. Design principles

### 3.1 Standards first

Use a Rust Embedded trait whenever it correctly expresses the operation:

| Function | Public interface |
|---|---|
| Blocking SPI device | `embedded_hal::spi::SpiDevice<u8>` |
| Async SPI device | `embedded_hal_async::spi::SpiDevice<u8>` |
| Blocking serial stream | `embedded_io::{Read, Write}` |
| Async serial stream | `embedded_io_async::{Read, Write}` |
| Optional polling serial compatibility | `embedded_hal_nb::serial::{Read, Write}` |
| GPIO controls | `embedded_hal::digital::{InputPin, OutputPin}` |
| Blocking delay | `embedded_hal::delay::DelayNs` |
| Async delay | `embedded_hal_async::delay::DelayNs` |
| Static PWM duty | `embedded_hal::pwm::SetDutyCycle` |
| Optional blocking NOR storage | `embedded_storage::nor_flash` traits |

Use FerroWasp-specific traits only where no adequate standard exists:

- explicit stream discontinuity and generation metadata;
- finite timer-DMA waveform ownership;
- continuous waveform refill ownership;
- safety-authorized actuator output;
- transport health/recovery control.

### 3.2 Driver crates target devices, not buses

SPI device drivers depend on `SpiDevice`, not `SpiBus`. The device abstraction owns chip-select scope and guarantees that a transaction is not interleaved with another device.

### 3.3 Information hiding

Each subsystem hides its state representation. Other layers interact only through stable public methods and event/result types.

Do not introduce “hooks” into another subsystem’s private queues or state machine. If two pieces require intimate knowledge of one another, keep them in the same subsystem until a stable interface is demonstrated.

### 3.4 Fail closed

Any unresolved configuration, transfer fault, deadline violation, resource conflict, or actuator-state inconsistency must result in one of:

- initialization failure while outputs remain inactive;
- a recoverable non-actuator transport fault with explicit discontinuity;
- a latched actuator fault with all outputs forced low.

### 3.5 Build profiles over runtime type erasure

When hardware mode materially changes the valid API, use a different target profile or firmware binary. Do not expose methods that are invalid for the active timer mode.

---

## 4. Workspace and crate structure

```text
workspace/
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml
│
├── crates/
│   ├── ferrowasp-io-core/
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── health.rs
│   │       ├── time.rs
│   │       ├── stats.rs
│   │       ├── serial/
│   │       │   ├── mod.rs
│   │       │   ├── profile.rs
│   │       │   ├── chunk.rs
│   │       │   ├── discontinuity.rs
│   │       │   └── fault.rs
│   │       ├── spi/
│   │       │   ├── mod.rs
│   │       │   ├── transaction_id.rs
│   │       │   └── fault.rs
│   │       └── waveform/
│   │           ├── mod.rs
│   │           ├── finite.rs
│   │           ├── streaming.rs
│   │           └── fault.rs
│   │
│   ├── ferrowasp-waveform/
│   │   └── src/{lib.rs,dshot.rs,sine.rs,q15.rs,phase.rs,pack.rs}
│   │
│   ├── ferrowasp-actuator/
│   │   └── src/{lib.rs,authority.rs,command.rs,state.rs,mapping.rs,fault.rs}
│   │
│   ├── protocol-sbus/
│   │   └── src/{lib.rs,parser.rs,encoder.rs,io_blocking.rs,io_async.rs}
│   ├── protocol-crsf/
│   │   └── src/{lib.rs,parser.rs,encoder.rs,io_blocking.rs,io_async.rs}
│   ├── protocol-mavlink/
│   │   └── src/{lib.rs,parser.rs,encoder.rs,io_blocking.rs,io_async.rs}
│   ├── device-imu/
│   │   └── src/{lib.rs,registers.rs,blocking.rs,async.rs}
│   ├── device-nor-flash/
│   │   └── src/{lib.rs,commands.rs,blocking.rs,async.rs,storage.rs}
│   │
│   ├── ferrowasp-stm32f4/
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── clocks.rs
│   │       ├── memory.rs
│   │       ├── deadline.rs
│   │       ├── dma/
│   │       │   ├── mod.rs
│   │       │   ├── event.rs
│   │       │   ├── priority.rs
│   │       │   ├── flags.rs
│   │       │   └── fault.rs
│   │       ├── serial/
│   │       │   ├── mod.rs
│   │       │   ├── port.rs
│   │       │   ├── configured.rs
│   │       │   ├── rx.rs
│   │       │   ├── tx.rs
│   │       │   ├── reader.rs
│   │       │   ├── writer.rs
│   │       │   ├── sbus_words.rs
│   │       │   ├── irq.rs
│   │       │   └── recovery.rs
│   │       ├── spi/
│   │       │   ├── mod.rs
│   │       │   ├── engine.rs
│   │       │   ├── owner.rs
│   │       │   ├── device.rs
│   │       │   ├── job.rs
│   │       │   ├── operation.rs
│   │       │   ├── event.rs
│   │       │   ├── cancellation.rs
│   │       │   ├── irq.rs
│   │       │   └── recovery.rs
│   │       └── timer_dma/
│   │           ├── mod.rs
│   │           ├── endpoint.rs
│   │           ├── static_pwm.rs
│   │           ├── group.rs
│   │           ├── paired.rs
│   │           ├── dshot.rs
│   │           ├── streaming.rs
│   │           ├── start.rs
│   │           ├── irq.rs
│   │           └── shutdown.rs
│   │
│   ├── ferrowasp-bsp/
│   │   └── src/
│   │       ├── lib.rs
│   │       └── stm32f4/
│   │           └── ferrowasp_fcu3/
│   │               ├── mod.rs
│   │               ├── aliases.rs
│   │               ├── config.rs
│   │               ├── manifest.rs
│   │               ├── pins.rs
│   │               ├── resources.rs
│   │               ├── storage.rs
│   │               └── init.rs
│   │
│   └── ferrowasp-tasks/
│       └── src/
│           ├── lib.rs
│           ├── serial.rs
│           ├── imu.rs
│           ├── blackbox.rs
│           ├── control.rs
│           └── safety.rs
│
├── firmware/
│   └── ferrowasp_fcu3/
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs
│           ├── priorities.rs
│           ├── tasks.rs
│           └── profiles/
│               ├── flight_dshot.rs
│               ├── pwm_bringup.rs
│               └── waveform_demo.rs
│
├── tests/
│   ├── target_manifest/
│   ├── trait_conformance/
│   ├── protocol_vectors/
│   ├── spi_fake/
│   └── waveform_vectors/
│
└── project_docs/
    ├── ARCHITECTURE_DECISIONS.md
    ├── ACTIVE_WORK.md
    └── TEST_EVIDENCE_INDEX.md
```

### 4.1 Why `ferrowasp-io-bridge` was removed

The earlier plan introduced a separate bridge crate for converting RTIC services into standard traits. That abstraction is premature. The bridge depends closely on the selected runtime, queue primitives, DMA ownership, and cancellation model.

For the first implementation:

- STM32F4 serial and SPI handles implementing standard traits live in `ferrowasp-stm32f4`.
- Host fakes live in driver tests.
- A shared runtime-adapter crate may be extracted only after an STM32H7 backend proves that the code and invariants are genuinely identical.

This avoids splitting closely coupled functionality and prevents internal queue hooks from becoming a false public API.

### 4.2 BSP naming

`ferrowasp-bsp` means board support package. It contains physical board wiring and construction only. It does not contain:

- parsers;
- external-device command logic;
- control loops;
- RTIC task priorities;
- actuator permission policy.

### 4.3 Task logic

`ferrowasp-tasks` may contain reusable functions called by RTIC tasks, but it should avoid RTIC attributes and generated context types where practical.

Example:

```rust
pub async fn run_mavlink<R, W>(reader: R, writer: W) -> !
where
    R: embedded_io_async::Read,
    W: embedded_io_async::Write,
{
    // Portable task body.
}
```

The firmware binary owns the `#[task]` wrapper and passes the required handles.

---

## 5. Dependency policy

### 5.1 Dependency direction

```text
protocol-* -------------> embedded-io / embedded-io-async
                           ferrowasp-io-core

device-* ---------------> embedded-hal / embedded-hal-async
                           optional embedded-storage

ferrowasp-waveform ------> ferrowasp-io-core
ferrowasp-actuator ------> ferrowasp-io-core, ferrowasp-waveform

ferrowasp-stm32f4 -------> stm32f4xx-hal, RTIC, rtic-sync,
                           embedded-hal*, embedded-io*,
                           ferrowasp-io-core, ferrowasp-actuator

ferrowasp-bsp -----------> ferrowasp-stm32f4
ferrowasp-tasks ---------> portable drivers and domain crates
firmware ----------------> BSP, tasks, RTIC
```

### 5.2 Forbidden dependencies

Portable driver/protocol crates must not depend on:

- `stm32f4xx-hal` or any PAC;
- RTIC, `rtic-sync`, IRQ names, or RTIC mutex types;
- board targets;
- concrete queues or DMA buffers.

The STM32F4 backend must not depend on:

- MAVLink message IDs;
- SBUS channel mapping;
- CRSF packet semantics;
- IMU registers;
- flash commands;
- vehicle geometry or mixer policy.

The BSP must not depend on protocol parsers or flight-control policy.

### 5.3 Pinned dependency baseline

Use exact workspace dependency pins during implementation and evidence collection:

```toml
[workspace.dependencies]
embedded-hal = "=1.0.0"
embedded-hal-async = "=1.0.0"
embedded-hal-nb = "=1.0.0"
embedded-hal-bus = "=0.3.0"
embedded-io = "=0.7.1"
embedded-io-async = "=0.7.0"
embedded-storage = "=0.3.1"

rtic = { version = "=2.2.0", features = ["thumbv7-backend"] }
rtic-sync = "=1.5.0"

stm32f4xx-hal = {
    version = "=0.23.0",
    features = [
        "stm32f405",
        "rtic2",
        "rtic-tim2",
        "defmt",
    ],
}

heapless = "=0.9.1"
static_cell = "=2.1.1"
portable-atomic = "=1.11.1"
```

Codex must verify exact available patch versions in the repository before editing `Cargo.toml`. If a pin is unavailable, it must report the mismatch rather than silently selecting another version.

### 5.4 `embedded-io` version boundary

`stm32f4xx-hal` 0.23.0 depends on `embedded-io` 0.6.1, while the workspace-facing API uses 0.7.x.

Rules:

1. No public FerroWasp type may expose a HAL serial type implementing the 0.6.1 trait.
2. Local wrapper types implement the workspace-selected 0.7 traits.
3. `cargo tree -d` is recorded in the evidence log.
4. `embedded-io-adapters` is not used as a version-conversion mechanism.
5. Removal of the duplicate version is deferred until the HAL updates or the wrapper is no longer required.

### 5.5 HAL async caution

Enabling the HAL’s `rtic2` feature does not imply that every peripheral has an appropriate DMA-backed async implementation. FerroWasp supplies the serial and SPI service semantics described here and implements standard traits on local handles.

---

## 6. Hardware identity and configuration model

Keep four concepts separate:

1. Logical application identity.
2. Physical BSP route.
3. Reboot-applied logical configuration.
4. Firmware build profile.

### 6.1 Logical identities

```rust
pub enum LogicalSerial {
    Uart1,
    Uart2,
    Uart3,
}

pub enum LogicalSpi {
    Spi1,
    Spi2,
}

pub enum LogicalOutputBank {
    Actuators,
    AuxiliaryWaveform,
}
```

Logical `Uart1` is not necessarily physical `USART1` on every board.

### 6.2 Compile-time BSP facts

The selected BSP fixes:

- MCU part and package;
- clocks;
- pins and alternate functions;
- peripheral instances;
- DMA controller, stream, and channel;
- IRQ vectors;
- EXTI routes;
- chip-select pins;
- electrical inversion/transceiver paths;
- timer groups and supported output modes;
- static capacities.

### 6.3 Reboot-applied serial configuration

```rust
pub enum SerialProtocol {
    Disabled,
    Sbus,
    Crsf,
    Mavlink { baud: u32 },
}

pub struct SerialConfig {
    pub uart1: SerialProtocol,
    pub uart2: SerialProtocol,
    pub uart3: SerialProtocol,
}
```

The BSP validates protocol requests against port capabilities. It does not accept raw parity, stop bits, DMA priority, inversion, or word size from user configuration.

### 6.4 Build profiles for output hardware

The first implementation uses separate binaries or Cargo features:

```text
flight-dshot     -> paired DShot actuator engine
pwm-bringup      -> independent static PWM channels
waveform-demo    -> continuous waveform engine
```

A profile selects one timer interpretation. It is not legal to reinterpret the same timer while armed or while DMA is active.

### 6.5 No top-level target trait initially

Do not introduce a generic `BoardTarget` trait for the first target. Each board exports:

```rust
pub const MANIFEST: HardwareManifest;
pub fn validate_config(config: &BoardConfig) -> Result<(), ConfigError>;
pub fn init_flight(... ) -> Result<FlightTargetInit, InitError>;
pub fn init_pwm_bringup(... ) -> Result<PwmTargetInit, InitError>;
pub fn init_waveform_demo(... ) -> Result<WaveformTargetInit, InitError>;
```

A common trait may be introduced after a second target shows a real common interface.

---

## 7. Frozen FerroWasp FCU3 STM32F405 target

The first target is the FerroWasp FCU3 and records its complete frozen
allocation.

### 7.1 MCU and clocks

```text
Target ID:       ferrowasp_fcu3
Board name:      FerroWasp FCU3
MCU:             STM32F405RGT6
Package:         LQFP64
HSE:             8 MHz
SYSCLK:          168 MHz
APB1:            42 MHz, timer kernel 84 MHz
APB2:            84 MHz, timer kernel 168 MHz
Timebase:        TIM2 reserved for RTIC monotonic / timestamping
Debug:           PA13 SWDIO, PA14 SWCLK
Optional USB FS: PA11 DM, PA12 DP
```

### 7.2 Serial allocation

| Logical port | Peripheral | TX | RX | RX DMA | TX DMA | Allowed protocols |
|---|---|---|---|---|---|---|
| UART1 | USART1 | PA9 AF7 | PA10 AF7 through external inverter | DMA2 S2 C4 | DMA2 S7 C4 | Disabled, SBUS |
| UART2 | USART2 | PA2 AF7 | PA3 AF7 | DMA1 S5 C4 | DMA1 S6 C4 | Disabled, CRSF, MAVLink |
| UART3 | USART6 | PC6 AF8 | PC7 AF8 | DMA2 S1 C5 | DMA2 S6 C5 | Disabled, CRSF, MAVLink |

Reference electrical assumptions:

- UART1 RX has a fixed external inverter suitable for common SBUS receivers.
- UART1 is not exposed as a non-inverted byte port in this profile.
- UART2 and UART3 are ordinary non-inverted full-duplex TTL UARTs.
- CRSF is full duplex by default.

### 7.3 SPI allocation

| Logical bus | Peripheral | SCK | MISO | MOSI | CS | RX DMA | TX DMA | Device |
|---|---|---|---|---|---|---|---|---|
| SPI1 | SPI1 | PA5 AF5 | PA6 AF5 | PA7 AF5 | PA4 GPIO | DMA2 S0 C3 | DMA2 S3 C3 | Primary IMU |
| SPI2 | SPI2 | PB10 AF5 | PC2 AF5 | PC3 AF5 | PB12 GPIO | DMA1 S3 C0 | DMA1 S4 C0 | NOR flash |

The SPI2 route deliberately avoids PB15 because PB15 is used by the actuator reference route.

### 7.4 IMU interrupt

```text
IMU DRDY: PB0 / EXTI0
```

### 7.5 Actuator allocation

| Output | Pin | Timer output | Group |
|---|---|---|---|
| Output0 | PA8 | TIM1_CH1 | A |
| Output1 | PB15 | TIM1_CH3N | A |
| Output2 | PC8 | TIM3_CH3 | B |
| Output3 | PC9 | TIM3_CH4 | B |

| Group | Timer DMA request | DMA route | Channel | Burst layout |
|---|---|---|---:|---|
| A | TIM1_UP | DMA2 Stream 5 | 6 | CCR1, filler CCR2, CCR3 |
| B | TIM3_UP | DMA1 Stream 2 | 5 | CCR3, CCR4 |

### 7.6 Optional ADC allocation

```text
ADC1 DMA: DMA2 Stream 4, selected legal channel
```

The ADC route remains optional but DMA2 Stream 4 is reserved in the manifest so later code cannot allocate it accidentally.

### 7.7 Complete DMA claim table

| DMA resource | Owner |
|---|---|
| DMA2 Stream 0 | SPI1 RX |
| DMA2 Stream 1 | USART6 RX |
| DMA2 Stream 2 | USART1 RX |
| DMA2 Stream 3 | SPI1 TX |
| DMA2 Stream 4 | ADC1 reserve |
| DMA2 Stream 5 | TIM1_UP actuator group A |
| DMA2 Stream 6 | USART6 TX |
| DMA2 Stream 7 | USART1 TX |
| DMA1 Stream 0 | Free |
| DMA1 Stream 1 | Free |
| DMA1 Stream 2 | TIM3_UP actuator group B |
| DMA1 Stream 3 | SPI2 RX |
| DMA1 Stream 4 | SPI2 TX |
| DMA1 Stream 5 | USART2 RX |
| DMA1 Stream 6 | USART2 TX |
| DMA1 Stream 7 | Free |

This profile has no duplicate DMA stream claim.

### 7.8 Known physical limitation

PB15 is `TIM1_CH3N`, a complementary output. The BSP must verify on hardware:

- active DShot pulse polarity is high;
- compare zero produces external low;
- disabled and fault states are low;
- the corresponding main output is disabled;
- no accidental dead-time or complementary inversion changes pulse widths.

This route is transmit-only. A later board should prefer four main outputs on TIM1 or TIM8.

---

## 8. Resource manifest and validation

The BSP provides a descriptive manifest in addition to typed construction.

```rust
pub struct HardwareManifest {
    pub board: &'static str,
    pub mcu: &'static str,
    pub pins: &'static [PinClaim],
    pub peripherals: &'static [PeripheralClaim],
    pub dma: &'static [DmaClaim],
    pub irqs: &'static [IrqClaim],
    pub dispatchers: &'static [IrqName],
    pub timer_groups: &'static [TimerGroupDescription],
    pub serial_caps: &'static [SerialPortCapabilities],
}
```

### 8.1 Validation layers

1. Rust ownership and HAL trait bounds.
2. BSP unit tests over `MANIFEST`.
3. Compile tests for target aliases and standard-trait conformance.
4. Hardware tests.

### 8.2 Mandatory host checks

- No duplicate exclusive pin.
- No duplicate exclusive peripheral.
- No duplicate DMA stream.
- Correct DMA channel for each request.
- No IRQ used as both hardware task and dispatcher.
- Sufficient RTIC dispatchers for software priority levels.
- No timer group assigned incompatible modes.
- UART protocol is electrically supported.
- Mandatory SWD pins are not reassigned.
- Optional USB pins do not collide when USB is enabled.
- All required IRQ bindings are represented in the firmware app.

### 8.3 Board TOML and generator policy

A human-readable board TOML and BSP generator are useful later, but are not prerequisites for the family backend.

When added:

- typed Rust BSP code remains authoritative;
- the generator is a separate developer tool, not `build.rs`;
- generated code is committed and reviewed;
- it edits only marked/generated files;
- it generates a validation report and BSP skeleton, not application safety policy;
- it is introduced after the second manually implemented target.

---

## 9. Exact RTIC ownership model

### 9.1 Shared resources

```rust
#[shared]
struct Shared {
    uart1: target::Uart1Port,
    uart2: target::Uart2Port,
    uart3: target::Uart3Port,

    actuators: target::ActuatorEngine,
    system_health: SystemHealth,
}
```

Why:

- Each UART is accessed by USART, RX-DMA, and TX-DMA tasks.
- Actuators are accessed by two DMA completion IRQs, the control-output task, and safety shutdown.
- All locks are short and bounded. No parser, encoding, copying of large buffers, or control calculation occurs inside the lock.

### 9.2 Local resources

SPI engines are local to their owner hardware tasks:

```rust
#[task(
    binds = DMA2_STREAM0,
    priority = priorities::SPI_OWNER,
    local = [spi1_owner],
)]
fn spi1_owner(cx: spi1_owner::Context) {
    cx.local.spi1_owner.service_irq();
}
```

```rust
#[task(
    binds = DMA1_STREAM3,
    priority = priorities::SPI_OWNER,
    local = [spi2_owner],
)]
fn spi2_owner(cx: spi2_owner::Context) {
    cx.local.spi2_owner.service_irq();
}
```

The IMU DRDY pin is local to its EXTI task. Per-IRQ channel senders are local to the respective IRQ tasks.

### 9.3 SPI event producers

The SPI owner’s RX DMA IRQ is the single owner IRQ. Other contexts do not access the engine:

- async `SpiDevice` handle fills the unique owned job, sets `REQUEST`, and pends owner IRQ;
- TX DMA IRQ records error flags, sets `DMA_ERROR`, and pends owner IRQ;
- I/O watchdog sets `TIMEOUT`, and pends owner IRQ;
- dropped transaction future sets `CANCEL`, and pends owner IRQ.

The mailbox uses atomics or a safe RTIC-sync primitive and contains no borrowed buffers.

### 9.4 Serial IRQ ownership

For each UART:

- USART IRQ and RX-DMA IRQ use the same priority.
- TX-DMA IRQ uses the same priority for the first implementation.
- Each task locks the serial resource only long enough to snapshot/clear flags, replace/release DMA buffers, update state, and return a small result.
- Channel send occurs after the lock is released.

### 9.5 Actuator ownership

The shared actuator engine owns:

- both timer groups;
- both active DMA transfers;
- all waveform buffers;
- group completion mask;
- armed/inhibited/fault state;
- sequence counter and deadline;
- forced-low operation.

No raw timer group is exposed to another task.

### 9.6 Priority baseline

```rust
pub mod priorities {
    pub const EMERGENCY: u8 = 15;
    pub const ACTUATOR_DMA: u8 = 14;
    pub const CONTROL_OUTPUT: u8 = 13;
    pub const IMU_DRDY: u8 = 12;
    pub const SPI_OWNER: u8 = 11;
    pub const RC_UART: u8 = 10;
    pub const IO_WATCHDOG: u8 = 9;
    pub const OTHER_UART: u8 = 8;
    pub const IMU_SERVICE: u8 = 7;
    pub const PROTOCOL: u8 = 6;
    pub const LOGGING: u8 = 4;
}
```

This is the initial measurable baseline, not a proof of schedulability. It must be revised from WCET and latency measurements.

All SPI owner tasks use `SPI_OWNER`. No hidden SPI1-over-SPI2 priority exists.

### 9.7 DMA hardware priority

```text
Actuator update DMA: VeryHigh
SPI RX DMA:          VeryHigh, equal on all buses
SPI TX DMA:          High, equal on all buses
RC RX DMA:           High
Other serial RX:     Medium or High after measurement
Serial TX:           Medium
Logging/other DMA:   Low or Medium
```

---

## 10. Synchronization and static allocation

### 10.1 Primitive selection

| Need | Primitive |
|---|---|
| Ordered RX chunks | `rtic_sync::channel::Channel` |
| Ordered TX chunks | `rtic_sync::channel::Channel` |
| Latest-only DRDY/request notification | `rtic_sync::signal::Signal` |
| Latest health/status visible to readers | `rtic_sync::watch::Watch` |
| Future shared SPI bus | `rtic_sync::arbiter::spi` or verified `embedded-hal-bus` adapter |
| IRQ event bits | `portable_atomic` bitfield or backend-owned atomic mailbox |
| Fixed vectors/queues | `heapless` |
| Long-lived buffers/endpoints | `StaticCell` |

Do not use `Arbiter` in an IRQ. Do not hold an async arbiter guard across an unbounded operation unless the bus contract requires it and the timeout/cancellation behavior is verified.

### 10.2 Initial capacities

Capacities are target constants and may be tuned after profiling.

```rust
pub const UART_BYTE_RX_DMA: usize = 256;
pub const UART_SBUS_RX_WORDS: usize = 64;
pub const UART_RX_CHUNK: usize = 256;
pub const UART_RX_QUEUE_DEPTH: usize = 4;
pub const UART_TX_CHUNK: usize = 256;
pub const UART_TX_QUEUE_DEPTH: usize = 4;

pub const SPI1_MAX_BYTES: usize = 64;
pub const SPI2_MAX_BYTES: usize = 260;
pub const SPI_MAX_OPERATIONS: usize = 8;

pub const DSHOT_BITS: usize = 16;
pub const DSHOT_RESET_SLOTS: usize = 4;
pub const DSHOT_SLOTS: usize = 20;
```

### 10.3 DMA memory

STM32F405 DMA cannot access CCM. Requirements:

- DMA buffers use ordinary SRAM-backed `.bss`/static storage.
- The linker script maps ordinary RAM to SRAM1/SRAM2 and does not place these cells in CCM.
- No application-owned `#[link_section]` or raw pointer is introduced merely for placement.
- CI produces a linker map and checks buffer symbols.
- Hardware startup asserts or reports address ranges in debug builds where practical.

### 10.4 No borrowed data in queues

A borrowed user buffer may not be stored in a queue or static job across task/IRQ boundaries.

Serial and SPI adapters use bounded copies into owned static storage. This is the safe baseline. A later zero-copy design requires a separate reviewed ownership proof and benchmark justification.

---

## 11. Time, deadlines, and cancellation

### 11.1 Time source

The first target reserves TIM2 as a 32-bit RTIC monotonic/timebase with microsecond-scale ticks.

The BSP exposes a common timestamp type from `ferrowasp-io-core`, not the HAL monotonic type.

### 11.2 I/O watchdog

A persistent RTIC software task runs at a bounded period, initially 125 microseconds:

```text
8 kHz watchdog cadence
```

It checks deadline records and publishes timeout events to active SPI and actuator owners. It performs no peripheral recovery itself.

### 11.3 Default transport deadlines

```text
SPI1 IMU wire transaction:       250 us
SPI2 flash wire transaction:       2 ms
DShot600 finite frame:             75 us hard deadline
Serial TX chunk: target-derived from baud and length, plus margin
```

NOR-flash internal program/erase time is not an SPI wire timeout. The portable flash driver polls status with async delays and a separate operation deadline.

### 11.4 SPI cancellation states

```rust
pub enum JobState {
    Free,
    Prepared,
    Pending,
    Active,
    CancelRequested,
    Completing,
    Completed,
    Faulted,
}
```

Cancellation rules:

1. The async future never places caller borrows in the owner mailbox.
2. Writes and operation metadata are copied into the owned job before submission.
3. Read results are copied back only while the future still exists.
4. Dropping the future sets `CancelRequested` and pends the owner IRQ.
5. If pending, the owner discards the job without touching hardware.
6. If active, the owner aborts at a safe boundary, deasserts CS, releases DMA ownership, and discards received data.
7. A generation ID rejects stale DMA IRQs after abort/restart.
8. The job slot returns to `Free` only after the owner has reclaimed every resource.

### 11.5 Late IRQ rule

Every DMA transfer carries a monotonically wrapping generation counter. Completion or error flags from a prior generation are cleared and ignored; they never complete a newer request.

---

## 12. Common fault, health, and statistics model

### 12.1 Health

```rust
pub enum Health {
    Unknown,
    Healthy,
    Degraded,
    Unavailable,
    Faulted,
}
```

### 12.2 Fault classes

```rust
pub enum IoFault {
    Configuration(ConfigurationFault),
    Serial(SerialFault),
    Spi(SpiFault),
    Waveform(WaveformFault),
    Resource(ResourceFault),
}
```

Concrete backend errors retain:

- peripheral status flags;
- DMA transfer/direct/FIFO error flags;
- generation ID;
- start and fault timestamps;
- timeout deadline;
- recovery count;
- queue high-water mark.

They also implement the relevant standard error trait.

### 12.3 Recovery policy

- Serial and non-actuator SPI faults may perform bounded automatic recovery.
- Every recovery creates an explicit discontinuity or transaction error.
- Repeated recovery escalates health.
- Actuator faults are latched and never auto-rearm.
- Initialization errors leave outputs inactive and enter a fatal-safe loop or controlled reset path.

### 12.4 Saturating statistics

Use saturating counters for:

- IRQ count;
- bytes/transactions completed;
- queue full;
- RX chunks dropped;
- discontinuities;
- DMA faults;
- timeouts;
- cancellations;
- stale IRQs;
- recoveries;
- coalesced IMU requests;
- actuator busy submissions;
- actuator deadline faults.

---

## 13. Serial DMA subsystem

### 13.1 Responsibilities

`ferrowasp-stm32f4::serial` owns:

- USART configuration from a validated protocol profile;
- normal-mode RX DMA with active/spare buffers;
- USART IDLE handling;
- DMA transfer-complete/error handling;
- generation-based race suppression;
- TX DMA state;
- safe buffer replacement and recovery;
- standard `embedded-io` handles;
- statistics and transport discontinuity.

It does not parse MAVLink, CRSF, or SBUS.

### 13.2 Configured port type

Each logical target port uses a concrete enum:

```rust
pub enum ConfiguredPort<BYTE, SBUS> {
    Disabled(DisabledPort),
    Byte {
        transport: BYTE,
        protocol: ByteProtocol,
    },
    Sbus {
        transport: SBUS,
    },
}
```

`ByteProtocol` distinguishes CRSF and MAVLink when their hardware representation is the same.

Disabled ports retain ownership of their hardware parts in a disabled state, allowing IRQ wrappers to safely delegate to a no-op variant.

### 13.3 SBUS baseline

SBUS uses:

```text
100000 baud
8 data bits
Even parity
2 stop bits
external inversion on the reference target
```

The baseline DMA representation is `u16` because the STM32 USART normally uses a 9-bit word configuration when receiving 8 data bits plus parity. The adapter normalizes each received word to `u8` after validating status/error flags.

A byte-DMA optimization may be accepted only after a hardware test proves all eight payload bits are preserved with the selected HAL configuration.

### 13.4 RX algorithm

1. DMA receives into active buffer A.
2. Spare buffer B remains exclusively owned by the backend.
3. USART IDLE or RX DMA completion occurs.
4. Under the serial RTIC lock, snapshot flags and determine valid length.
5. Suppress stale duplicate events using generation state.
6. Replace A with B using safe HAL ownership APIs.
7. Restart RX DMA before protocol work.
8. Return detached A and metadata as a small completion result.
9. Outside the lock, normalize/copy the valid prefix into `RxChunk`.
10. `try_send` the chunk to the parser/reader channel.
11. Recycle detached storage.
12. On channel full, record loss, set discontinuity, and immediately invalidate RC validity when this port is the active RC source.

### 13.5 IDLE and full-buffer race

The state includes `active_generation` and `last_delivered_generation`.

- DMA-complete first: full buffer is detached once; later IDLE observes the new generation.
- IDLE first: partial buffer is detached once; later stale DMA completion is cleared without redelivery.
- Zero-length events are not emitted.

Hardware tests must force both orderings.

### 13.6 `RxChunk`

```rust
pub struct RxChunk<const N: usize> {
    pub bytes: [u8; N],
    pub len: u16,
    pub timestamp: Timestamp,
    pub completion: RxCompletion,
    pub generation: u32,
    pub discontinuity_before: bool,
    pub uart_error_seen: bool,
}
```

### 13.7 Async reader

`SerialReader` owns the RX channel receiver and an internal current-chunk offset. It implements the workspace `embedded_io_async::Read` trait.

Semantics:

- `read` returns available bytes from the current chunk.
- If empty, it awaits the next chunk.
- Chunk boundaries are not protocol frame boundaries.
- A separate `DiscontinuityReader`/control handle exposes discontinuity and health metadata.
- A pure `Read` consumer must not be used as the only RC safety monitor.

### 13.8 TX architecture

```text
protocol writer
    -> bounded TX chunk channel
    -> persistent uartX_tx_worker
    -> short RTIC lock to start DMA
    -> TX-DMA IRQ
    -> completion signal/watch
```

`SerialWriter` implements `embedded_io_async::Write`:

- `write` copies as much as fits into one owned TX chunk and returns that byte count;
- it may await queue capacity;
- `flush` waits until the queue is empty and USART transmission is complete;
- it never busy-waits in a control-critical task.

### 13.9 Parser structure

Each protocol crate contains an incremental pure parser:

```rust
pub trait ByteParser {
    type Output;

    fn reset(&mut self);
    fn consume(&mut self, bytes: &[u8], emit: &mut impl FnMut(Self::Output));
}
```

Async runners are thin wrappers over `Read`/`Write`. The parser:

- preserves partial frames across reads;
- accepts multiple frames per read;
- resets after explicit discontinuity;
- bounds work per byte;
- reports protocol validity separately from transport health.

### 13.10 Serial acceptance tests

- 115200 8N1 loopback and continuous stream.
- CRSF at 420000 baud full duplex.
- SBUS 100000 8E2 through inverter.
- IDLE before TC and TC before IDLE.
- RX queue overflow and immediate RC invalidation.
- framing/parity/overrun recovery.
- TX queue full and flush semantics.
- no byte-by-byte RXNE path during normal operation.

---

## 14. SPI DMA subsystem

### 14.1 Public contract

Portable drivers see:

```rust
embedded_hal_async::spi::SpiDevice<u8>
```

Blocking driver modules use `embedded_hal::spi::SpiDevice<u8>` and are independent implementations or thin shared protocol logic, not blocking wrappers around an RTIC async service.

### 14.2 Initial bus policy

- SPI1 has one IMU device.
- SPI2 has one NOR-flash device.
- Each device handle is unique and not cloneable.
- `&mut self` prevents concurrent transactions on one handle.
- No request queue is required in the first implementation; one static job slot exists per bus.
- SPI1 and SPI2 can operate concurrently because they have separate peripherals and DMA streams.

### 14.3 Owned transaction representation

The standard transaction accepts borrowed operations. The adapter converts them into a bounded owned job:

```rust
pub struct OwnedSpiJob<const MAX_OPS: usize, const MAX_BYTES: usize> {
    pub id: TransactionId,
    pub operations: heapless::Vec<OwnedOperation, MAX_OPS>,
    pub tx: [u8; MAX_BYTES],
    pub rx: [u8; MAX_BYTES],
    pub tx_len: u16,
    pub rx_len: u16,
    pub deadline: Timestamp,
    pub state: JobState,
}
```

Supported standard operations:

- `Read`;
- `Write`;
- `Transfer`;
- `TransferInPlace`;
- `DelayNs` when bounded and represented without busy-waiting.

For each operation, the adapter records offsets and lengths. Read data is copied back only after successful completion.

### 14.4 Transaction limits

If a transaction exceeds `MAX_OPS` or `MAX_BYTES`, return a structured error before asserting CS.

Target defaults:

```text
SPI1: 8 operations, 64 aggregate bytes
SPI2: 8 operations, 260 aggregate bytes
```

### 14.5 Owner task

The RX DMA interrupt is the owner IRQ because RX completion normally marks completion of the full-duplex frame.

`service_irq()` processes events in this order:

1. forced cancellation or timeout;
2. DMA error flags;
3. valid RX completion;
4. pending request when idle;
5. stale flags/generation cleanup.

The handler remains bounded and does not execute device protocol logic.

### 14.6 Transfer sequence

1. Validate job and state.
2. Configure operation’s DMA lengths/directions.
3. Clear SPI and DMA flags.
4. Assert CS.
5. Start RX DMA.
6. Start TX DMA.
7. Return from owner IRQ.
8. On RX completion, stop/release both transfers.
9. Verify SPI not busy and flush as required.
10. Advance to the next operation without deasserting CS.
11. After final operation, deassert CS.
12. Set result and signal the waiting future.
13. Return job state to reusable only after copy-back or cancellation disposal is resolved.

### 14.7 Error and timeout behavior

On any bus/DMA/timeout error:

- disable both DMA streams;
- disable SPI DMA requests;
- clear flags;
- deassert CS;
- release/reconstruct safe backend ownership;
- mark the transaction failed;
- increment generation;
- signal the waiter;
- attempt bounded peripheral recovery if allowed.

A CS deassertion error is recorded, but the primary bus error remains the standard return error in accordance with `SpiDevice` semantics.

### 14.8 IMU service

The portable IMU driver knows:

- registers;
- command bit format;
- dummy bytes;
- burst length;
- response validation;
- sample decoding.

The IMU task handles DRDY policy:

- non-FIFO sensor: use latest-only `Signal`; count replaced requests;
- FIFO sensor: request a bounded FIFO drain and reconstruct timestamps;
- no queue of unrecoverable stale register reads.

### 14.9 NOR-flash service

The portable flash driver owns:

- JEDEC commands;
- write enable;
- page boundaries;
- page program;
- status WIP polling;
- erase geometry;
- operation deadlines.

Each wire transaction is one ordinary `SpiDevice` transaction. Internal flash waiting occurs in the async driver with `DelayNs`, not inside the SPI engine or an ISR.

### 14.10 Shared bus extension

When a real board places multiple devices on one SPI bus:

1. First evaluate `rtic_sync::arbiter::spi` or `embedded-hal-bus`.
2. Verify CS behavior, cancellation, timeout, and maximum lock duration.
3. Keep drivers generic over `SpiDevice`.
4. Do not alter the internal device-driver API.
5. Add a separate work package; do not generalize the initial owner prematurely.

### 14.11 SPI acceptance tests

- Exact CS scope across multiple operations.
- Read, write, transfer, and transfer-in-place.
- Maximum-size job and one-byte job.
- Too-many-operations rejection before CS.
- RX completion, TX error, timeout, cancel-before-start, and cancel-while-active.
- Late IRQ after cancellation cannot complete a newer generation.
- Concurrent SPI1 and SPI2 transfers.
- IMU DRDY-to-sample timing.
- Flash page program/status polling.

---

## 15. Static PWM and timer-DMA subsystem

### 15.1 Static PWM

The PWM bring-up profile uses ordinary HAL PWM channels and local wrapper types implementing:

```rust
embedded_hal::pwm::SetDutyCycle
```

Rules:

- `max_duty_cycle` reflects the configured ARR range.
- out-of-range input returns an error or is rejected by a checked higher-level API; it is not silently wrapped.
- fully-off produces the verified inactive electrical level.
- static PWM wrappers do not exist in the DShot binary.

Static PWM is used to validate pin routing, polarity, timer clocks, and ESC behavior before timer DMA-burst work.

### 15.2 Timer DMA-burst boundary

`stm32f4xx-hal` 0.23.0 does not expose the full safe timer DMA-burst API required by the permanent paired DShot design.

Decision:

- consume a pinned HAL fork or upstream patch exposing a safe typed endpoint;
- the endpoint encapsulates any required low-level unsafe implementation in the HAL dependency;
- every FerroWasp workspace crate remains `#![forbid(unsafe_code)]`;
- Codex must not implement raw timer register addresses, raw DMA peripheral pointers, or unsafe trait implementations in the FerroWasp workspace;
- if the safe HAL endpoint is unavailable, stop after static PWM/direct safe bring-up and report the blocker.

Expected conceptual endpoint:

```rust
pub struct TimerDmaBurst<TIM, RANGE> {
    // HAL-private representation.
}

impl<TIM, RANGE> TimerDmaBurst<TIM, RANGE> {
    pub fn prime(&mut self, first: &[u16]) -> Result<(), Error>;
    pub fn start(&mut self, remaining: &'static mut [u16]) -> Result<(), Error>;
    pub fn on_irq(&mut self) -> IrqResult;
    pub fn abort(&mut self) -> Result<(), Error>;
    pub fn force_low(&mut self);
}
```

The actual API may differ, but must provide equivalent ownership and safety.

### 15.3 Logical and physical mapping

The actuator crate works with logical motor order. The BSP maps logical physical outputs to timer lanes. The mixer never sees TIM or pin identities.

### 15.4 DShot producer

The portable waveform crate produces one logical waveform per motor:

- 11-bit command/value;
- telemetry bit;
- four-bit checksum;
- 16 data slots;
- four terminal zero slots.

DShot600 timing is calculated from actual timer kernel frequency. It is not hardcoded from SYSCLK.

### 15.5 Target packing

Reference group A layout:

```text
slot0: CCR1, 0, CCR3
slot1: CCR1, 0, CCR3
...
```

Reference group B layout:

```text
slot0: CCR3, CCR4
slot1: CCR3, CCR4
...
```

Filler lanes are a BSP/backend concern and never appear in the public motor API.

### 15.6 First-slot priming

Required start sequence:

1. stop both timers;
2. disable update-DMA requests;
3. clear timer and DMA flags;
4. reset counters;
5. write slot zero into active compare registers;
6. configure DMA for slots one through terminal zero slots;
7. arm both DMA streams;
8. enable update-DMA requests;
9. start both counters from a known phase;
10. measure inter-timer skew.

Do not assume enabling DMA automatically emits slot zero correctly.

### 15.7 Paired actuator engine

The engine accepts one four-motor frame only when both groups are idle.

```rust
pub trait ActuatorOutput {
    type Error;

    fn submit(
        &mut self,
        frame: ValidatedMotorFrame<4>,
        now: Timestamp,
    ) -> Result<SequenceId, Self::Error>;

    fn force_inhibit(&mut self, cause: InhibitCause);
    fn state(&self) -> ActuatorState;
}
```

Rules:

- no stale queue;
- `Busy` is returned if a prior frame is active;
- both groups are armed before either counter starts;
- completion occurs only when both group bits are set;
- any group fault aborts both groups and forces all outputs low;
- a deadline miss is a latched fault;
- recovery requires disarmed state and explicit safety authorization.

### 15.8 Continuous waveform streaming

The waveform demo uses double buffering:

```text
DMA owns active buffer
producer owns ready buffer
completion swaps active/ready
producer refills released buffer in software task
```

The ISR does not generate sine samples. It only completes/switches buffers and signals refill work.

If a ready buffer is not available by the refill deadline:

- stop the output or apply a documented safe terminal pattern;
- record underrun;
- do not repeat stale actuator data silently.

### 15.9 Actuator authority

Only the safety/actuator path can construct or obtain `ValidatedMotorFrame`. Outer tasks may request setpoints but cannot directly receive timer/PWM/DMA capabilities.

Rust visibility should enforce this where practical:

- constructors for validated frames are crate-private to the safety/control authority crate;
- BSP and STM32F4 crates know only electrical output values, not arming policy;
- experimental tasks receive no actuator engine resource.

### 15.10 Timer/PWM acceptance tests

- Static PWM 0%, 50%, and 100% waveform/polarity.
- Exact DShot known vector and checksum.
- First slot is neither missing nor duplicated.
- Terminal low interval.
- TIM1/TIM3 start skew.
- PB15 complementary output polarity.
- Busy submission rejection.
- One group error forces both groups low.
- Timeout forces low before another command is accepted.
- Buffer cannot be modified while DMA owns it.
- Continuous waveform underrun produces documented safe behavior.

---

## 16. Initialization API and fail-closed boot

### 16.1 Target result types

The target returns subsystem-specific initialization bundles rather than one ambiguous aggregate with missing fields.

```rust
pub struct SerialInit<PORT, READER, WRITER, RX_SENDER, TX_RECEIVER> {
    pub port: PORT,
    pub reader: Option<READER>,
    pub writer: Option<WRITER>,
    pub irq_rx_sender: RX_SENDER,
    pub tx_worker_receiver: TX_RECEIVER,
}

pub struct SpiInit<OWNER, DEVICE> {
    pub owner: OWNER,
    pub device: DEVICE,
}

pub struct FlightTargetInit {
    pub uart1: target::Uart1Init,
    pub uart2: target::Uart2Init,
    pub uart3: target::Uart3Init,

    pub spi1: target::Spi1Init,
    pub spi2: target::Spi2Init,

    pub actuators: target::ActuatorEngine,
    pub imu_drdy: target::ImuDrdyPin,
    pub diagnostics: TargetDiagnostics,
}
```

Target-specific aliases make the concrete channel endpoint and HAL wrapper types manageable.

### 16.2 Disabled serial behavior

A disabled serial port returns:

- `ConfiguredPort::Disabled` for the shared hardware resource;
- `None` reader/writer handles;
- inert IRQ behavior;
- no parser task spawn.

The app never fabricates a reader for a disabled port.

### 16.3 Compile-shaped RTIC `init`

RTIC `init` does not return `Result`. It must handle all failures explicitly.

```rust
#[init]
fn init(cx: init::Context) -> (Shared, Local) {
    target::Mono::start(cx.device.TIM2, &mut /* target clock resources */);

    let config = load_and_validate_config_or_default();

    let board = match target::init_flight(
        cx.device,
        cx.core,
        &config,
        target::storage(),
    ) {
        Ok(board) => board,
        Err(error) => target::fatal_init(error),
    };

    spawn_serial_task_if_enabled(
        board.uart1.reader,
        board.uart1.writer,
        LogicalSerial::Uart1,
    );
    spawn_serial_task_if_enabled(
        board.uart2.reader,
        board.uart2.writer,
        LogicalSerial::Uart2,
    );
    spawn_serial_task_if_enabled(
        board.uart3.reader,
        board.uart3.writer,
        LogicalSerial::Uart3,
    );

    spawn_or_fatal(imu_service::spawn(board.spi1.device));
    spawn_or_fatal(blackbox_service::spawn(board.spi2.device));
    spawn_or_fatal(io_watchdog::spawn());

    (
        Shared {
            uart1: board.uart1.port,
            uart2: board.uart2.port,
            uart3: board.uart3.port,
            actuators: board.actuators,
            system_health: SystemHealth::initializing(),
        },
        Local {
            spi1_owner: board.spi1.owner,
            spi2_owner: board.spi2.owner,
            imu_drdy: board.imu_drdy,
            uart1_irq_sender: board.uart1.irq_rx_sender,
            uart2_irq_sender: board.uart2.irq_rx_sender,
            uart3_irq_sender: board.uart3.irq_rx_sender,
        },
    )
}
```

This is structural pseudocode. Codex must adapt the exact monotonic and spawn signatures to the pinned RTIC/HAL APIs, while preserving the ownership split and explicit error handling.

### 16.4 Fatal initialization

`target::fatal_init(error) -> !` must:

1. leave or force all actuator outputs inactive;
2. disable actuator timer/DMA requests;
3. emit a bounded diagnostic through an available safe channel such as defmt;
4. optionally store a reset reason;
5. enter watchdog reset or a low-power fault loop.

It must not return partially initialized hardware.

---

## 17. Thin RTIC wrappers

### 17.1 Serial USART wrapper

```rust
#[task(
    binds = USART1,
    priority = priorities::RC_UART,
    shared = [uart1],
    local = [uart1_irq_sender],
)]
fn uart1_usart(mut cx: uart1_usart::Context) {
    let result = cx.shared.uart1.lock(|port| port.on_usart_irq());
    handle_uart_irq_result(result, cx.local.uart1_irq_sender);
}
```

### 17.2 Serial RX-DMA wrapper

```rust
#[task(
    binds = DMA2_STREAM2,
    priority = priorities::RC_UART,
    shared = [uart1],
    local = [uart1_irq_sender],
)]
fn uart1_rx_dma(mut cx: uart1_rx_dma::Context) {
    let result = cx.shared.uart1.lock(|port| port.on_rx_dma_irq());
    handle_uart_irq_result(result, cx.local.uart1_irq_sender);
}
```

### 17.3 SPI owner wrapper

```rust
#[task(
    binds = DMA2_STREAM0,
    priority = priorities::SPI_OWNER,
    local = [spi1_owner],
)]
fn spi1_owner(cx: spi1_owner::Context) {
    cx.local.spi1_owner.service_irq();
}
```

### 17.4 SPI TX error wrapper

```rust
#[task(
    binds = DMA2_STREAM3,
    priority = priorities::SPI_OWNER,
    local = [spi1_events],
)]
fn spi1_tx_dma(cx: spi1_tx_dma::Context) {
    cx.local.spi1_events.record_tx_irq_and_pend_owner();
}
```

The event handle is a lightweight clone/capability around static atomic state, not the SPI engine.

### 17.5 Actuator DMA wrapper

```rust
#[task(
    binds = DMA2_STREAM5,
    priority = priorities::ACTUATOR_DMA,
    shared = [actuators],
)]
fn actuator_group_a(mut cx: actuator_group_a::Context) {
    cx.shared.actuators.lock(|engine| {
        engine.on_group_irq(ActuatorGroup::A)
    });
}
```

### 17.6 Safety shutdown wrapper

```rust
#[task(
    binds = /* selected emergency/watchdog IRQ */,
    priority = priorities::EMERGENCY,
    shared = [actuators],
)]
fn emergency_shutdown(mut cx: emergency_shutdown::Context) {
    cx.shared.actuators.lock(|engine| {
        engine.force_inhibit(InhibitCause::Emergency)
    });
}
```

All wrapper bodies remain short and explicit. No macro generation is permitted before Work Package 8.

---

## 18. Unsafe-code and HAL-fork policy

### 18.1 Workspace policy

Apply where practical:

```rust
#![forbid(unsafe_code)]
```

Required in:

- firmware binaries;
- BSP;
- `ferrowasp-stm32f4`;
- portable drivers;
- protocol crates;
- actuator/waveform/core crates;
- task crate.

### 18.2 Dependency boundary

The PAC, HAL, RTIC, and supporting crates necessarily contain audited unsafe internals. The application trusts those dependencies through pinned versions.

### 18.3 Timer DMA-burst fork

If a HAL fork is required:

- keep it in a separate repository or dependency source;
- pin a full commit hash in `Cargo.lock`/workspace documentation;
- document the upstream base revision and patch;
- expose only a safe typed API;
- add hardware regression tests;
- do not copy the unsafe implementation into `ferrowasp-stm32f4`;
- prefer submitting the API upstream.

### 18.4 Stop condition

Codex must stop and report a blocker if implementing a required endpoint would require new unsafe code inside this workspace.

---

## 19. Verification strategy

### 19.1 Static architecture checks

CI checks:

- no `unsafe` token in workspace-owned Rust sources, excluding comments/test fixtures if needed;
- no STM32 or RTIC dependency in portable driver crates;
- no protocol constants in `ferrowasp-stm32f4`;
- no HAL pin/DMA/transfer types in firmware task logic beyond target aliases;
- no duplicate manifest claims;
- `cargo tree -d` recorded;
- all public driver APIs compile with host fakes.

### 19.2 Trait conformance tests

```rust
fn assert_async_spi<T: embedded_hal_async::spi::SpiDevice<u8>>() {}
fn assert_blocking_spi<T: embedded_hal::spi::SpiDevice<u8>>() {}
fn assert_async_read<T: embedded_io_async::Read>() {}
fn assert_async_write<T: embedded_io_async::Write>() {}
fn assert_pwm<T: embedded_hal::pwm::SetDutyCycle>() {}
```

Only static PWM target aliases are asserted as `SetDutyCycle`.

### 19.3 Host tests

- DShot checksum and known frames.
- Timer packing for sparse lanes.
- Q15 sine/phase logic.
- parser fragmentation and multiple-frame input.
- transport discontinuity resets parser.
- SPI operation packing/copy-back.
- SPI cancel state transitions.
- manifest conflict tests.
- health escalation and saturating counters.

### 19.4 Hardware tests

Use a logic analyzer/oscilloscope for:

- UART framing and maximum-rate stream;
- IDLE/TC race;
- SPI mode, clock, CS scope, and timeout recovery;
- DRDY-to-SPI-start and DRDY-to-sample latency;
- static PWM polarity;
- DShot pulse timing and inter-timer skew;
- fail-low behavior;
- concurrent DMA stress.

### 19.5 Evidence record

Each hardware test records:

```text
Test ID
Git commit
Cargo.lock hash
Target profile
Hardware revision
Configuration checksum
Procedure
Expected result
Measured result
Logic-analyzer/oscilloscope file
Pass/fail
Known limitation
```

### 19.6 Performance acceptance

Measure worst case, not only average:

- IRQ execution time;
- RTIC lock duration;
- request-to-DMA-start latency;
- DMA-completion-to-wakeup latency;
- serial lost-byte rate;
- queue high-water marks;
- SPI timeout recovery time;
- actuator start skew;
- control-loop jitter under concurrent DMA;
- maximum stack and static memory use.

---

## 20. Codex implementation work packages

Codex should execute one work package at a time. It must inspect the actual repository before editing and update `ACTIVE_WORK.md` with exact results.

### Work Package 0 — Repository and dependency freeze

**Objective:** establish a buildable, reviewable baseline without implementing hardware algorithms.

Deliverables:

1. Inspect current workspace and report conflicts with this plan.
2. Create/update `ARCHITECTURE_DECISIONS.md` with the frozen decisions.
3. Pin workspace dependencies and commit `Cargo.lock`.
4. Add `#![forbid(unsafe_code)]` to workspace-owned crates where applicable.
5. Create crate skeletons and dependency direction.
6. Add CI commands:
   - `cargo fmt --check`;
   - `cargo clippy` for host crates;
   - target `cargo check`;
   - host unit tests;
   - duplicate dependency report;
   - unsafe-source scan.
7. Do not implement UART, SPI, or timer DMA yet.

Exit criteria:

- workspace builds/checks at the skeleton level;
- no cyclic dependencies;
- dependency versions are explicit;
- all architecture deviations are documented rather than silently changed.

### Work Package 1 — Portable types and host tests

**Objective:** establish portable contracts before HAL code.

Deliverables:

- health, timestamp, deadline, fault, and saturating statistics types;
- serial protocol profiles and `RxChunk`;
- DShot encoder and timing calculator;
- actuator command/state types;
- portable incremental parser test scaffolding;
- SPI owned-operation metadata types that contain no RTIC/HAL types;
- host tests and trait-bound examples.

Exit criteria:

- all crates compile on host without STM32 dependencies;
- no duplicate standard traits;
- DShot and parser vectors pass.

### Work Package 2 — Frozen reference BSP and manifest

**Objective:** encode the complete first resource map with no transport implementation.

Deliverables:

- `ferrowasp_fcu3` BSP files;
- target aliases for all pins, peripherals, DMA streams, and IRQs;
- complete `MANIFEST` matching Section 7;
- manifest conflict tests;
- compile tests proving the HAL accepts the selected pin and DMA trait mappings;
- static storage declarations/capacity report;
- explicit dispatcher IRQ allocation;
- three firmware profile skeletons.

Exit criteria:

- every exclusive hardware resource has one owner;
- pin/DMA aliases compile with the pinned HAL;
- no unresolved `TODO` exists in the resource map;
- no DMA or pin conflict remains.

**Initial Codex assignment ends here.**

### Work Package 3 — One byte UART RX path

Implement UART2 at 115200 8N1:

- active/spare RX DMA;
- USART IDLE;
- RX DMA full/error;
- generation suppression;
- RX channel and async reader;
- loopback/stress test.

Do not add protocols or all UARTs yet.

### Work Package 4 — Serial generalization and SBUS path

- extract one reusable STM32F4 serial implementation;
- instantiate UART1/UART2/UART3;
- add u16 SBUS transport and normalization;
- add TX worker/writer;
- add configured port enums and boot validation.

### Work Package 5 — SPI1 owner and IMU transaction

- unique owned job;
- owner IRQ and event mailbox;
- async `SpiDevice`;
- cancellation and watchdog timeout;
- one IMU register burst;
- logic-analyzer test.

### Work Package 6 — SPI2 and flash

- instantiate second SPI engine without copied algorithms;
- portable flash driver;
- async status polling;
- concurrent SPI1/SPI2 stress.

### Work Package 7 — Static PWM and safe timer endpoint

- static PWM profile and tests;
- integrate the safe HAL timer DMA-burst endpoint;
- stop if local unsafe would be required;
- verify one finite timer group.

### Work Package 8 — Paired DShot actuator engine

- paired groups;
- first-slot priming;
- atomic start;
- forced-low and latched fault;
- control/safety integration;
- DShot600 evidence.

### Work Package 9 — Protocol integration

- SBUS parser and RC validity;
- CRSF full duplex;
- MAVLink stream runner;
- application routing;
- overflow/discontinuity tests.

### Work Package 10 — Continuous waveform profile

- double buffer;
- software refill task;
- underrun policy;
- coherent multi-channel waveform.

### Work Package 11 — Second BSP and deduplication review

- implement a second STM32F4 board manually;
- identify actual duplicated target boilerplate;
- decide whether a small declarative macro or BSP generator is justified;
- do not hide priorities, locks, or safety behavior.

### Work Package 12 — Full-system validation

- concurrent serial/SPI/actuator/ADC/logging load;
- WCET and jitter report;
- linker-map audit;
- fault injection;
- evidence index;
- supported-target freeze.

---

## 21. Codex execution contract

For every work package, Codex must:

1. Read this document, `ARCHITECTURE_DECISIONS.md`, and `ACTIVE_WORK.md`.
2. Inspect the actual code before proposing changes.
3. State which files it will edit.
4. Implement the smallest coherent change.
5. Run the specified tests/checks.
6. Report exact commands and results.
7. Update affected documentation.
8. Stop on missing hardware facts, unsupported HAL APIs, or any need for local unsafe code.

Codex must not:

- invent pin or DMA routes;
- change the reference resource allocation silently;
- create a universal DMA abstraction;
- add a generator before the second BSP;
- move protocol logic into IRQ handlers;
- queue stale actuator commands;
- expose HAL types in portable driver APIs;
- use `unwrap`/`expect` in runtime IRQ/task paths;
- weaken arming, failsafe, actuator authority, or forced-low behavior;
- implement all work packages in one unreviewed change.

### 21.1 Required handoff format

```markdown
# Objective

# Repository state inspected

# Decisions applied

# Files changed

# Implementation summary

# Commands run

# Test results

# Known limitations/blockers

# Documentation updated

# Exact next work package
```

---

## 22. Acceptance criteria

The architecture is accepted when all of the following are true.

### 22.1 Portability

- IMU and flash drivers compile against host fakes and STM32F4 handles using standard `SpiDevice` traits.
- Protocol cores compile without RTIC or STM32.
- Async serial runners use `embedded-io-async`.
- Static PWM clients use `SetDutyCycle` only in static PWM builds.
- Custom traits cover only discontinuity, waveform ownership, health/recovery, and actuator authority.

### 22.2 No same-family duplication

- Each serial DMA algorithm exists once.
- Each SPI DMA algorithm exists once.
- Timer group and paired actuator algorithms exist once.
- A new board supplies typed routes and storage, not copied engine code.

### 22.3 Resource correctness

- Complete manifest has no duplicate DMA stream, pin, peripheral, or IRQ claim.
- All target aliases compile with the pinned HAL.
- Disabled ports remain inert.
- Unsupported protocol/electrical combinations fail before enabling interrupts or outputs.

### 22.4 Serial

- IDLE and full-buffer events deliver each byte range exactly once.
- Maximum-rate continuous input does not silently stop DMA.
- Queue overflow produces explicit discontinuity.
- RC validity is revoked immediately on transport loss.
- SBUS framing and u16 normalization are verified.
- TX `flush` means queue empty and hardware transmission complete.

### 22.5 SPI

- Standard transaction CS semantics are preserved across all operations.
- Oversize jobs fail before CS.
- Cancellation and timeout reclaim all resources.
- Late IRQs cannot complete a newer transaction.
- SPI1 and SPI2 operate concurrently.
- IMU and flash contain no STM32-specific code.

### 22.6 Timer/PWM

- Static PWM obeys `SetDutyCycle` semantics.
- DShot first slot and terminal low are correct.
- Both groups start within measured skew limits.
- Busy frames are rejected, not queued.
- Any group fault forces all outputs low and latches fault.
- Continuous waveform generation occurs outside hardware ISR.

### 22.7 Safety and determinism

- All workspace-owned crates forbid unsafe code.
- Required low-level unsafe remains inside pinned dependencies.
- Actuator hardware is accessible only through the actuator/safety resource path.
- Worst-case IRQ/lock/latency/jitter results meet documented system budgets.
- Fault recovery is bounded and observable.

---

## 23. Deferred decisions

These are intentionally deferred and must not block Work Packages 0–2:

- exact production board(s) beyond the reference target;
- shared SPI bus support;
- zero-copy queue/transaction optimization;
- bidirectional DShot;
- SDIO blackbox storage;
- a board-config wizard or TOML generator;
- RTIC wrapper macros;
- STM32H7 extraction/common backend factoring;
- dynamic runtime output-mode selection.

A deferred feature becomes active only with a concrete use case, resource map, and acceptance tests.

---

## 24. Reconciliation with the previous plan

This revision changes the previous document in the following important ways:

1. Removes the premature `ferrowasp-io-bridge` crate and keeps runtime-coupled adapters with the STM32F4 backend.
2. Freezes a complete, conflict-free first STM32F405 resource map, including UART routes.
3. Corrects the SPI2/actuator PB15 pin collision by moving SPI2 to PB10/PC2/PC3.
4. Defines exact RTIC ownership: shared serial, local SPI owners, shared actuator engine.
5. Replaces ambiguous aggregate `hardware_owners` examples with subsystem initialization bundles.
6. Defines disabled serial ports through configured enums and optional reader/writer endpoints.
7. Makes output mode a build-profile decision for the first implementation.
8. Chooses concrete RTIC synchronization primitives and initial capacities.
9. Defines the TIM2 timebase, periodic I/O watchdog, and initial deadlines.
10. Defines cancellation-safe async SPI behavior and late-IRQ generation handling.
11. Resolves the timer DMA-burst unsafe boundary through a pinned safe HAL fork/API.
12. Preserves equal SPI owner priority and separates it from DMA hardware arbitration priority.
13. Corrects file naming from `asynch.rs` to `async.rs`.
14. Removes `?` from RTIC `init` examples and specifies fail-closed initialization.
15. Defers RTIC macros and BSP generation until a second manually implemented target exists.
16. Limits the first Codex assignment to Work Packages 0–2.

---

## 25. Final implementation rule

A change belongs in:

- a portable driver crate when it describes device/protocol meaning through standard traits;
- `ferrowasp-io-core`, `ferrowasp-waveform`, or `ferrowasp-actuator` when it describes portable semantics not covered by Rust Embedded traits;
- `ferrowasp-stm32f4` when it describes STM32F4 peripheral, DMA, IRQ, timeout, or recovery mechanics;
- `ferrowasp-bsp` when it describes one PCB’s immutable electrical routing and construction;
- the RTIC firmware binary when it describes task ownership, priorities, concrete IRQ bindings, and system integration;
- the safety/control layer when it decides whether actuation is authorized.

If a proposed module requires private knowledge of another module’s queues or state representation, the interface is wrong or the modules were split too early.

The expected result is a family backend that is reusable across STM32F4 boards, portable drivers that are useful outside FerroWasp, and an RTIC app that declares the system without duplicating low-level peripheral code.
