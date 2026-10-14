# FerroWasp Architecture Decisions

Last updated: 2026-07-19

This file records the frozen decisions for the STM32F4 RTIC 2 unified I/O and
DMA refactor branch. It complements the active implementation plan in:

```text
stm32f4_rtic2_unified_io_dma_implementation_plan_codex_ready.md
```

## ADR-0001: Refactor Branch Boundary

Status: accepted

The unified hardware refactor is developed on:

```text
refactor/hardware
```

The current prototype and flight-test branch should remain independently
recoverable. This branch may introduce workspace structure, portable contract
crates, and BSP resource manifests before replacing the live RTIC app wiring.

## ADR-0002: FerroWasp FCU3 Pin Map Is Frozen

Status: accepted

The first BSP target is `ferrowasp_fcu3`, board name `FerroWasp FCU3`, using
the STM32F405RGT6 in LQFP64. It records the routing already validated by the
firmware, even where it differs from example routes in the original plan.

Current frozen resources:

| Function | Resource |
|---|---|
| SBUS RC | USART2 PA2 TX, PA3 RX, DMA1 Stream 5 Channel 4 RX |
| DJI O4 MSP OSD | UART4 PA0 TX, PA1 RX, DMA1 Stream 2 Channel 4 RX, DMA1 Stream 4 Channel 4 TX |
| MPU6500 IMU | SPI1 PA5 SCK, PA6 MISO, PA7 MOSI, PA4 CS, DMA2 Stream 2 Channel 3 RX, DMA2 Stream 3 Channel 3 TX |
| ADC observation | ADC1 PC0 voltage, PC1 current, internal temperature, DMA2 Stream 0 |
| Motor output 1 | PA8 TIM1 CH1 |
| Motor output 2 | PC9 TIM3 CH4 |
| Motor output 3 | PC8 TIM3 CH3 |
| Motor output 4 | PB15 TIM12 CH2 |
| Control scheduler | TIM4 |
| I/O microsecond timebase | TIM2 |
| I/O deadline watchdog | TIM6 |
| Debug heartbeat LEDs | PB0 red, PB1 green |

External pin assignments remain frozen. Pinless internal timer roles may be
changed by an explicit timer/timebase task with a manifest update and target
validation.

## ADR-0003: Safety Authority Is Preserved During Refactor

Status: accepted

The live firmware continues to command motors only through the existing
`actuator_output` task and safety signal path. New portable crates may define
authority and command types, but must not obtain timer, PWM, DMA, or motor
peripheral ownership.

## ADR-0004: PWM DMA And DShot Timer DMA Are Deferred

Status: accepted

This branch may add portable DShot/waveform encoders and static resource
manifests. It must not integrate the PWM DMA/timer-DMA actuator engine yet.

Reason:

- the current requested scope says the PWM DMA plan will be implemented later;
- the implementation plan requires a safe HAL timer DMA-burst API;
- FerroWasp-owned crates must not add local unsafe code to reach timer DMA.

## ADR-0005: Workspace Split Starts With Contracts

Status: accepted

The first split creates host-buildable crates for:

- portable I/O/fault/statistics contracts;
- waveform/DShot helpers;
- actuator authority and validated command data;
- BSP resource manifests and conflict tests.

The live RTIC app remains in `src/main.rs` until the reference BSP and backend
contracts are validated.

## ADR-0006: STM32F4 Backend Starts As Compile-Shaped Metadata

Status: superseded by ADR-0007

`ferrowasp-stm32f4` exists before the live app is moved into it. Its first role
is to hold safe route metadata, static storage budgets, and compile-time HAL DMA
route checks for the frozen FCU3 map.

It must not construct or own active motor, serial, SPI, ADC, timer, or DMA
peripherals until the matching RTIC task ownership change is implemented as a
separate reviewed patch.

## ADR-0007: BSP Owns Board Policy, MCU Backend Owns HAL Mechanisms

Status: accepted

After repeated target validation, reusable UART, SPI, ADC, and static PWM
mechanisms were moved into `ferrowasp-stm32f4`. Concrete peripheral and static
storage ownership remains in the RTIC app shell.

Board-specific serial protocol selection, SPI device routing, DMA assignments,
static PWM motor routing, reserved pins, IRQ allocation, and timer modes live in
`ferrowasp-bsp::stm32f4::ferrowasp_fcu3`. The FCU3 BSP also owns target HAL
aliases, safe storage shaping, pin conversion, and board device constructors.
The STM32F4 backend retains board-independent peripheral setup and transport
state machines.

This keeps the current FCU map frozen without making the MCU-family crate the
owner of board policy. Timer-DMA/DShot actuator integration remains outside this
decision and is still deferred by ADR-0004.

## ADR-0008: Control Scheduling Is Board-Selected And TIM2 Is The I/O Timebase

Status: accepted

The generic STM32F4 backend can construct and acknowledge a control scheduler
from any suitable HAL timer instance. The RTIC app and target aliases select
TIM4 for the current board. The RTIC hardware interrupt binding remains an
explicit board/app-shell choice because it cannot be represented by a Rust type
alias.

TIM2 is a pinless 32-bit 1 MHz free-running I/O timebase. Portable wrap
extension converts its approximately 71-minute hardware period into
`TimestampMicros` values. SPI transaction deadlines use this source instead of
the 1 ms SysTick monotonic.

Reasons:

- the unified plan assigns TIM2 to microsecond I/O deadlines;
- TIM4 is otherwise unclaimed and needs no external pin;
- motor PWM timer channels and every external pin remain unchanged;
- keeping scheduler construction generic limits a future board change to its
  target alias, peripheral selection, RTIC binding, and manifest.

Timeout checking and peripheral recovery are a later target-validated
checkpoint. This decision only changes the scheduler source and deadline
timestamp source.

## ADR-0009: TIM6 Watchdog Posts Recovery To The SPI Owner Priority

Status: accepted

TIM6 runs a pinless 8 kHz I/O deadline watchdog at RTIC priority 9. The
watchdog reads TIM2 and probes the SPI lifecycle, but it does not manipulate
peripherals. On expiry it posts a bounded software task at priority 13, equal
to the SPI1 RX DMA owner IRQ. Equal-priority serialization and the RTIC shared
resource ceiling prevent timeout recovery and RX completion from mutating the
owner concurrently.

The SPI owner performs bounded DMA recovery:

1. request lifecycle cancellation;
2. pause TX and RX streams through safe HAL APIs;
3. deassert chip select;
4. replace the partial RX buffer with a dedicated recovery buffer;
5. restart RX DMA and retain the discarded partial buffer for the next
   recovery;
6. advance the generation and recycle the owned job.

If RX restart fails, the owner becomes unavailable and rejects future
transactions. It does not continue with ambiguous buffer ownership.

The HAL's split SPI DMA handles do not expose a safe API to disable SPI DMA
request bits, inspect/flush SPI busy state, or reset the peripheral. Therefore
this checkpoint claims bounded DMA ownership recovery, not full SPI peripheral
reinitialization. No workspace `unsafe` is added to bypass that limitation.

## ADR-0010: SPI1 Uses One Owned Async Mailbox Between Task And IRQ Owner

Status: accepted

SPI1 exposes one non-cloneable `embedded-hal-async::SpiDevice` handle to the
priority-12 IMU polling task. The handle does not own or access the peripheral.
It copies operation metadata and TX bytes into one bounded
`SpiRequestMailbox`, then pends a priority-13 owner service task.

The mailbox is protected by `critical-section::Mutex<RefCell<_>>`. Lock windows
are bounded to request packing, state transitions, a 15-byte RX copy, waiter
registration, or result copy-back. No lock is held across `.await`, and no
caller borrow crosses into the owner task or DMA IRQ.

`SpiDmaOwner` remains the only owner of SPI1, both DMA halves, the IMU chip
select, and recovery buffers. The owner service starts pending work; the RX DMA
IRQ copies the completed frame into the owned job before preserving the
existing parser-queue delivery. Timeout and dropped-future cancellation are
generation-scoped and are finalized by the same owner.

The pending request deadline begins at mailbox submission. When the owner
accepts a request, it rebases the 250 us transport deadline to the owner
activation timestamp. This prevents software-dispatch latency from consuming a
deadline intended to bound the active wire/DMA phase while retaining a bound on
requests that never reach the owner.

The portable adapter supports bounded standard SPI operation shapes. The
current STM32F4 backend intentionally accepts only the existing one-operation,
15-byte full-duplex IMU burst. Both MPU6500 and ICM42688-P sample layouts fit
that transaction shape. Variable-length and multi-operation DMA sequencing
require a later measured backend change.

This decision changes no external pin, DMA stream, timer assignment, motor
resource, actuator authority, poll rate, or parser behavior.

## ADR-0011: Serial RX Uses Owned Chunks With Out-Of-Band Continuity

Status: accepted

The workspace serial receive boundary uses exact `embedded-io` 0.7.1 and
`embedded-io-async` 0.7.0 traits. A DMA completion is normalized into one
bounded, owned `RxChunk` containing bytes, valid length, timestamp, completion
cause, stream generation, discontinuity marker, and UART-error marker.

`SerialReader` owns the consumer state and current-chunk offset. It implements
`embedded_io_async::Read`, returns bytes from at most one chunk per call, and
retains no caller buffer while pending. Queue overflow records a discontinuity
and marks the next delivered chunk. A separate `DiscontinuityReader` exposes
the latest event and cumulative health counters.

The byte-stream trait is intentionally not the only safety monitor. RC validity
must observe the discontinuity/health handle because `Read` alone cannot prove
that no bytes were lost before a syntactically valid frame.

UART4 MSP/OSD was the first live bridge: its existing DMA/parser queue detached
and recycled static DMA storage, copied into the owned channel, then parsed
through the standard async reader. USART2 SBUS remained on its target-validated
direct parser path until the lower-priority UART4 bridge had target evidence.
ADR-0013 records the subsequent USART2 migration.

This decision changes no external pin, UART mode, DMA route, IRQ priority,
protocol routing, motor resource, or actuator authority.

## ADR-0012: Serial TX Separates Writer Capacity From Hardware Completion

Status: accepted

Portable serial TX uses one bounded `SerialTxChannel` split into a
`SerialWriter`, serialized peripheral-owner handle, and IRQ-side completion
handle. The writer implements `embedded_io_async::Write` and copies at most one
fixed-capacity owned chunk per `write` call.

A write that is pending on queue capacity has not changed shared channel state,
so dropping it does not enqueue partial data. The owner dequeues one chunk,
marks it in flight, and must explicitly report completion or a terminal fault.
`flush` completes only when both the pending queue is empty and no chunk is in
flight in hardware.

The portable owner is not motor or safety authority. UART4 is the first live
backend: the OSD task owns only the writer, a persistent priority-4 worker owns
the serialized channel owner, and the DMA1 Stream 4 IRQ owns the completion
handle. The STM32F4 DMA resource is shared only between that worker and IRQ.
The IRQ publishes one completion or terminal DMA fault for an in-flight chunk;
stale or duplicate IRQ observations cannot complete another chunk.

The current backend preserves the target-validated fixed 70-byte DMA transfer,
zero-padding each shorter MSP frame. Variable-length safe-HAL DMA buffers are a
later backend checkpoint. UART4 PA0/PA1, DMA1 Stream 4 Channel 4, baud rate,
protocol routing, motor resources, and actuator authority remain unchanged.

STM32F4 DMA FIFO-error flags are advisory on this UART TX path. The IRQ clears
that flag and leaves the transfer in flight, matching the HAL UART DMA policy.
Transfer-error and direct-mode-error flags remain terminal and are reported
separately. This distinction is covered by host IRQ-planning tests.

## ADR-0013: USART2 RC Validity Is Separate From Parsed Setpoints

Status: accepted

USART2 SBUS now uses the same bounded owned-RX contract as UART4. The USART2
DMA and IDLE IRQ owners detach one completed DMA buffer, copy its bytes and
metadata into an owned `RxChunk`, recycle the DMA buffer immediately, and wake
one persistent priority-10 SBUS reader through `embedded_io_async::Read`.

RC validity is an explicit safety state, not an inference from the last parsed
setpoint. The link is invalidated by:

- USART2 transport discontinuity or owned-queue overflow;
- DMA error;
- SBUS parser error;
- SBUS frame-lost or failsafe flags;
- no healthy frame for 100 ms after the link was valid.

Recovery requires three consecutive healthy SBUS frames. An invalidation also
clears the rearm latch, so the arm switch must be observed low after recovery
before a later high transition can request arming. Arming and actuator-idle
completion both recheck the link state. Any accepted invalidation revokes
actuator permission, clears system armed, and posts a disarm command through
the existing actuator owner.

The byte-stream reader is not the sole safety monitor. The USART2 bridge and
protocol task also observe out-of-band discontinuity state because a later
syntactically valid frame cannot prove that earlier bytes were not lost.

This checkpoint preserves USART2 PA2/PA3, 100000 baud with even parity and two
stop bits, DMA1 Stream 5 Channel 4, existing IRQ priorities, all motor pins,
and actuator authority. PWM/DShot DMA remains deferred.

## ADR-0014: The Actuator Owner Guards Long Arming Holds

Status: accepted

The BLHeli PWM arming sequence remains owned by the priority-15 actuator task,
but its 2.5-second low hold and 500 ms idle hold are no longer unchecked
sleeps. The actuator owner checks actuator permission, RC link armability, arm
switch, and throttle every 10 ms.

On any failed guard, the actuator owner writes all four outputs low, clears the
idle-complete signal, and then reports an explicit abort reason to the
priority-16 safety master. This ordering closes the case where a safety
`Disarm` spawn is rejected because the single-instance actuator executor is
already occupied by the arming sequence.

Successful completion is relayed through a priority-13 notifier after the
actuator task returns. The safety master therefore performs its final checks
after the actuator executor is available for a forced-low command if those
checks fail.

The arming throttle boundary is consistently `ARMING_MAX_THROTTLE`, currently
65 command counts. PWM timing, hold durations, external pins, motor order,
timer ownership, and actuator authority are unchanged.

## ADR-0015: Active Motor Values Cross One SPSC Authority Boundary

Status: accepted

The priority-14 control loop exclusively owns `MotorCmdWriter`; the
priority-15 `actuator_output` task exclusively owns `MotorCmdReader`. Active
motor values are no longer RTIC spawn arguments. `ApplyLatestThrottle` and the
bench-selected-motor command are wake-up signals only.

Each command carries a wrapping sequence and millisecond timestamp. The
actuator drains older queued entries, accepts only the newest command when it
is no more than `MOTOR_CMD_MAX_AGE_MS` old, and then applies the existing
finite-value and output-bound policy. Missing, stale, or invalid data is
rejected to low output. Queue overflow or a rejected actuator wake requests
disarm through the safety master.

Disarm and entry into the BLHeli arming sequence drain the queue, preventing a
command from an earlier arm cycle from being replayed. The normal mixer,
equal-motor mode, and individual physical/logical motor bench modes all use
the same queue boundary.

This decision does not add an independent deadline watchdog for complete loss
of future control-loop wakes; that remains a separate safety task. PWM timing,
external pins, motor mapping, timer ownership, and actuator authority are
unchanged. PWM/DShot DMA remains deferred.

## ADR-0016: FCU3 Is An Explicit BSP Target Without An App Adapter

Status: accepted; application-location clause superseded by ADR-0018

The active board is named `FerroWasp FCU3` and has the stable target identifier
`ferrowasp_fcu3`. Its identity, STM32F405RGT6/LQFP64 description, frozen
pin/DMA/timer/IRQ routes, profiles, target aliases, storage shapes, and device
constructors live under:

```text
ferrowasp-bsp::stm32f4::ferrowasp_fcu3
```

The dependency direction is BSP to MCU backend. `ferrowasp-stm32f4` must not
depend on `ferrowasp-bsp`, because reusable chip-family mechanisms cannot select
a board.

The RTIC `#[init]` local resources own the board DMA buffer and queue banks. The
FCU3 BSP converts those resources into backend storage structures through safe
Rust and then invokes the reusable backend. No board-storage
`cortex_m::singleton!` adapter and no workspace `unsafe` are required. The
obsolete `src/stm32f4` compatibility directory is removed. At acceptance,
`src/main.rs` was the sole root application source; ADR-0018 later moves that
unchanged shell into an isolated app package.

This is an ownership and layout change only. External pins, DMA streams, timer
roles, priorities, motor ordering, safety authority, and runtime behavior are
unchanged. PWM/DShot DMA remains deferred by ADR-0004.

## ADR-0017: The First Multi-Target Proof Is An Isolated Nucleo Bring-Up App

Status: accepted

The second BSP target is `nucleo_f401re`, board name `ST NUCLEO-F401RE`, using
the STM32F401RET6 in LQFP64. Its first application is deliberately limited to:

- PA5 onboard LD2 output;
- PA2 USART2 TX to the integrated ST-LINK virtual COM port at 115200 baud;
- SysTick for a one-second LED and serial heartbeat.

The capability manifest declares no attached IMU and no actuator outputs. The
application does not depend on RC, PWM, motor, sensor, or flight-control code.

The application is a sibling Cargo workspace under
`apps/stm32f401-bringup`, selecting Nucleo through its default board feature.
STM32F401 and STM32F405 HAL/PAC features are mutually exclusive in one Cargo
feature-unification graph, so isolating this bring-up package keeps existing
FCU3 workspace commands deterministic. Both applications still consume typed
board definitions from `ferrowasp-bsp`.

The Nucleo uses its own 512 KiB flash / 96 KiB SRAM linker map, 84 MHz HSI
clock plan, ST-LINK runner, and Cargo Embed profile. HSI is selected so the
bring-up does not depend on Nucleo board-revision oscillator bridge settings.

This checkpoint originally proved build and board separation only. ADR-0019
later replaces the polling loop with a minimal RTIC shell; it still does not
claim shared flight-task wiring, sensor portability, flight capability, or
actuator authority.

## ADR-0018: Deployable Firmware Uses Isolated App Packages

Status: accepted

The repository root is a virtual Cargo workspace for reusable crates.
Deployable firmware packages are isolated under `apps/`:

```text
apps/stm32f405-flight
apps/stm32f401-bringup
```

Each app owns its Cargo lockfile, target configuration, linker memory map,
runner, and RTIC entry point. This prevents incompatible STM32F401 and
STM32F405 HAL/PAC features from entering one Cargo feature-unification graph.

The F405 app represents a flight-runtime resource contract, not one physical
board. It selects `board-ferrowasp-fcu3` by default. A future STM32F405 board
such as Foxeer F405V2 may reuse this app only when its BSP satisfies the same
compile-time peripheral, interrupt, DMA, timer, and task contract. A board
requiring different RTIC wiring gets another thin app shell.

The Nucleo app remains separate because its bring-up role has no sensor, RC,
OSD, PWM, motor, or actuator resources.

Moving the existing F405 entry point does not change external pins, DMA
streams, timers, priorities, motor order, runtime policy, or actuator
authority. The binary name remains `FerroWasp`.

## ADR-0019: The F401 Bring-Up App Uses A Minimal RTIC 2 Shell

Status: accepted

The NUCLEO-F401RE application follows the same app-shell boundary as the F405
firmware without copying its flight resources. It uses `#[rtic::app]`, an RTIC
SysTick monotonic at 1 kHz, an initialization task, and one persistent
priority-1 async heartbeat task. EXTI0 is reserved as the RTIC software-task
dispatcher because it is outside the minimal Nucleo board contract; the user
button remains on PC13/EXTI15_10.

The RTIC app preserves the established 84 MHz HSI clock, PA5 LD2 output,
PA2/USART2 TX at 115200 baud, one-second liveness period, linker map, and
external pin mapping. It deliberately omits the F405 app's IMU, RC, OSD, ADC,
control, PWM, motor, and actuator resources.

The earlier polling image's target evidence remains valid for the board and
peripheral mapping, but it is not evidence for the new scheduler. The RTIC
image passed its immediate target smoke test on 2026-07-18. The earlier
five-minute polling run remains the longer-duration soak evidence.

## ADR-0020: Foxeer F405 V2 Uses A Separate Inhibited Flight App

Status: accepted

The Foxeer F405 V2 target has a separate RTIC package under
`apps/foxeer-f405-v2`. It does not share the FCU3 binary because its clock
source, SPI mode, ADC DMA stream, timer ownership, motor channels, debug-pin
policy, and interrupt bindings form a different compile-time resource
contract.

The first BSP surface is deliberately limited to a SPI1 identity probe and
runtime-selected MPU6500/ICM42688-P data path, USART2 SBUS, UART4 MSP
DisplayPort, ADC voltage/current observation, and four conventional RC PWM
motors.
M5-M8, SPI flash, analog OSD, I2C, buzzer, camera control, and LED strip are
outside this checkpoint. PWM timer-DMA and DShot routes are declarative and
remain deferred.

The motor map is:

```text
M1 PA8  TIM1_CH1
M2 PC9  TIM8_CH4
M3 PC8  TIM8_CH3
M4 PB15 TIM1_CH3N
```

The shared controller logical order follows Betaflight Quad X numbering:
motor 1 rear-right, motor 2 front-right, motor 3 rear-left, and motor 4
front-left. Foxeer provisionally maps those lanes to physical outputs
`[1, 2, 3, 4]`, assuming its M1-M4 pads follow the same convention. This is BSP
policy and is independently inhibited until target verification; FCU3 keeps
its separately measured physical output order through the remap
`[3, 4, 2, 1]`.

Every actuator-capable board must re-test motor order, direction, and
stick/tilt response with propellers removed before flight after any
board-profile, backend, wiring, or motor-map change. The NUCLEO-F401RE
bring-up target has no actuator outputs.

M4 is explicitly implemented as a complementary advanced-timer output. Both
TIM1 and TIM8 begin with their main output enable gates closed. Only the
safety-owned actuator task may enable them after preparing a valid command;
forced-off behavior closes the gates and latches zero compares.

The app uses an explicit compile-time arming inhibit while sensor identity,
body-axis orientation, ADC calibration, motor order, and M4 pulse polarity
remain unverified. PA13/PA14 are preserved for SWD instead of status LEDs.
This checkpoint establishes buildable ownership and a safe target-observation
image; it does not claim flight readiness.

The IMU probe does not panic on an unsupported identity. `WHO_AM_I=0x70`
selects MPU6500 configuration and the `0x3b` burst; `WHO_AM_I=0x47` selects
ICM42688-P configuration and the `0x1d` burst. Both fit the same bounded
15-byte full-duplex DMA transaction. Other identities or configuration
failures are logged once and periodic sampling remains disabled.

The Foxeer output protocol is conventional RC PWM at 400 Hz with 1000..2000 us
pulses. The board-local TIM1/TIM8 owner is retained because M4 is the
complementary `TIM1_CH3N` output and all four compares must be prepared before
the two advanced-timer gates open. Timer-DMA PWM and DShot remain deferred.

## ADR-0021: Foxeer USB CDC Debug Is Read-Only And Bounded

Status: accepted

The Foxeer app exposes optional USB CDC diagnostics through the existing
`usb_serial` feature on PA11/PA12 and OTG_FS. The feature remains opt-in so the
non-USB bring-up image and clock requirement can still be tested
independently. The BSP records OTG_FS, PA11, PA12, and its interrupt as an
optional claim rather than adding them to the default active resource set.

The endpoint emits a versioned `FWDBG1` ASCII status snapshot on the existing
roughly two-second heartbeat. Formatting is allocation-free and host-tested
against worst-case integer widths. The CDC class uses a 64-byte bounded RX
buffer and 256-byte bounded TX buffer; a single atomic due flag coalesces
updates when the host is absent or backpressured.

OTG_FS runs at RTIC priority 5, below safety, control, IMU/SPI, RC, and UART
receive processing. The priority-1 heartbeat only snapshots RC status, marks
one update due, and pends OTG_FS. USB RX drains at most one packet per
interrupt and discards it. There is no USB command parser, configuration
writer, safety handle, arming request, or actuator resource, so USB cannot
gain motor authority.

## ADR-0022: Foxeer Cargo Run Uses The STM32 ROM-DFU Bootloader

Status: accepted

The isolated Foxeer app configures `cargo run` to invoke a board-local
PowerShell runner and STM32CubeProgrammer. Cargo passes its ELF directly, so
the programmer consumes linker load addresses rather than relying on a raw
binary plus a manually repeated address. The workflow applies only to
`apps/foxeer-f405-v2`; FCU3 and Nucleo runners remain unchanged.

Before programming, the runner verifies the ELF magic and checks for a load
segment at `0x08000000` when `arm-none-eabi-readelf` is available. It lists
STM32 ROM-DFU devices, selects the only `USBn` port, or requires an explicit
`STM32_DFU_PORT` when more than one device is attached. CubeProgrammer writes,
verifies, and resets the target.

The legacy `flash-dfu.ps1` helper remains for `.bin` artifact generation and
delegates actual programming to the same runner. Discovery and environment
controlled dry-run modes are non-destructive. Entering ROM DFU with the BOOT
button is still a manual physical action, and direct programming replaces any
application, including Betaflight, at the start of internal flash.

## ADR-0023: FCU3 DShot Starts As An Opt-In Physical-M1 Actuator Backend

Status: accepted

The first FCU3 DShot implementation is deliberately restricted to physical
motor output 1. It preserves the validated external map and uses PA8,
TIM1_CH1, and DMA2 Stream1 Channel6 at DShot600. The normal four-channel
400 Hz RC PWM backend remains the default. A DShot image must also enable
`bench_motor1_only`; conflicting bench modes and PWM calibration fail at
compile time, and physical outputs 2-4 are held low as GPIO.

The RTIC actuator boundary remains authoritative. `actuator_output` validates
arming state and the bounded `MotorCmd` queue before issuing a leased M1
request. A priority-13 actuator service repeats the authorized value every
2 ms, while the priority-16 DMA completion handler stops TIM1 and forces CCR1
zero. The priority-13 service is the sole DMA frame starter; command handling
only updates the authorized value and lease, preserving the fixed 500 Hz
cadence. Nonzero requests expire after 20 ms without renewal and cause stop
plus disarm. The existing arming guard remains active during the 2.5-second
stop hold and 500 ms idle hold. During the idle hold, the actuator owner
renews the normal 20 ms lease every 10 ms only after permission, RC link, arm
switch, and throttle pass the guard. This avoids a long arming lease while
tolerating ordinary asynchronous wake timing; a stall longer than the normal
lease still causes stop plus disarm.

DMA is triggered by the TIM1_CH1 compare event so each next duty is loaded
during the current bit's low phase. The first duty is written directly to
CCR1; DMA carries bits 14 through 0 and a terminating zero. This avoids
software edge timing and leaves the signal low after completion.

Unsafe code is permitted only in `ferrowasp-stm32f4::dshot`. The documented
contract proves the TIM1 CCR1 address and 16-bit width, the STM32F405
DMA2 Stream1 Channel6 mapping, and bounded raw PAC writes. HAL `Transfer`
continues to own active buffers, DMA addresses, stream fencing, and buffer
exchange. No application task contains unsafe code.

This decision establishes a bench implementation, not waveform or flight
evidence. Packet and timer encoding are host-tested, but pulse timing and
jitter still require a logic analyzer. Four-motor synchronization,
bidirectional DShot, special commands, telemetry, and flight use remain
separate future decisions.

## ADR-0024: FCU3 Four-Motor DShot Is One Synchronized Fault Domain

Status: accepted

The next FCU3 bench stage extends DShot600 to all four existing external motor
pins without changing the connector map. The fixed routes are PA8/TIM1_CH1 on
DMA2 Stream1 Channel6, PC9/TIM8_CH4 on DMA2 Stream7 Channel7, PC8/TIM8_CH3 on
DMA2 Stream4 Channel7, and PB15/TIM1_CH3N on DMA2 Stream6 Channel6. M4 uses
complementary polarity so its external pulse convention matches the three
ordinary timer channels.

One safety-owned `DshotMotorBank` owns both timers, all four pins, all four DMA
transfers, and all waveform buffers. TIM1 is the start master: its CEN signal
is exported as TRGO, while TIM8 is armed in ITR0 trigger mode. Software enables
all four DMA streams before starting TIM1; hardware then starts both timer
domains from the same master event. This avoids four independently timed
software starts while preserving the validated external pin mapping.

The four lanes form one fault domain. A frame set is accepted as complete only
after every DMA lane reports completion. Any DMA transfer/direct-mode error,
unexpected or duplicate interrupt, or completion deadline miss faults the
entire bank. Fault handling disables timer DMA requests, closes both advanced
timer MOE gates, stops both counters, drives all four timer outputs to their
configured low idle state, and causes the safety path to request disarm.
Continuing on three lanes after one lane fails is not permitted.

The opt-in app requires exactly `dshot bench_equal_motors`. It retains RC
qualification, arming guards, the 250-count equal-motor bench cap, fixed 500 Hz
service cadence, and 20 ms nonzero-command lease. Four-channel RC PWM remains
the default firmware. Selected-motor DShot, PWM calibration, uncapped mixer
output, and direct peripheral ownership outside the actuator path are
compile-time or architectural exclusions.

Unsafe code remains limited to private DMA endpoint trait implementations in
`ferrowasp-stm32f4::dshot`. Each implementation documents its fixed CCR
address, transfer width, route, and ownership contract. The app itself denies
unsafe code.

This decision authorizes a staged props-off bench test, not flight use.
Historical M1 ESC interoperability does not validate synchronized four-motor
operation. Unpowered counter/runtime evidence and powered equal-motor behavior
must pass independently. Logic-analyzer timing/jitter, motor numbering and
direction, bidirectional DShot, telemetry, special commands, and flight output
remain separate checkpoints.

## ADR-0025: IMU Orientation Uses Explicit Sensor, Board, And Drone Frames

Status: accepted

The shared drone body frame is forward/right/down: +X forward, +Y right, and
+Z down. Angular rates follow the right-hand rule about those axes, so positive
roll lowers the right side, positive pitch raises the nose, and positive yaw
turns the nose right when viewed from above.

Board profiles describe IMU orientation in two steps:

```text
IMU sensor frame -> board frame -> drone body frame
```

Both steps are represented as signed axis permutations in
`ferrowasp-core::frames::FrameRotation`. The composed IMU-to-drone rotation
defines the physical body frame used for acceleration, attitude estimation,
and external orientation reporting. FCU3 retains its existing validated gyro
mapping `[1, 0, 2]` with signs `[1, -1, -1]` as the golden controller
behavior. Target evidence established Foxeer's physical sensor-to-body mapping
as `[1, 0, 2]` with signs `[-1, -1, -1]`.

The prototype rate controller predates the explicit body-frame type and uses a
historical nose-down-positive pitch convention. Foxeer therefore composes the
physical gyro mapping with an explicit controller compatibility transform
`[roll, -pitch, yaw]`. The transform applies only at the rate-controller
boundary; controller rates are converted back to physical body rates before
the complementary estimator combines them with gravity. Hiding this
compatibility sign inside Foxeer's measured physical orientation is forbidden.

This decision changes representation only. Any change to either rotation on an
actuator-capable board requires props-off axis/sign, motor order, motor
direction, and stick/tilt response re-validation before flight.

## ADR-0026: DShot Logical-Motor Identification Precedes Mixed Control

Status: accepted

The four-motor DShot backend may now combine its required
`dshot bench_equal_motors` gate with exactly one
`bench_logical_motorN_only` feature. This reuses the existing capped
selected-motor control path to verify the committed logical-to-physical map
without enabling full mixed control. Physical selected-motor features,
multiple logical selections, PWM calibration, DShot without the equal-motor
gate, and uncapped DShot mixer output remain compile-time errors.

The logical command is remapped before entering the bounded `MotorCmd` queue.
The safety-owned actuator task remains the only path that can update
`DshotMotorBank`; all four timer/DMA lanes stay active as one synchronized fault
domain. Guarded arming still idles all four ESCs. After `SYSTEM ARMED`, zero RC
throttle selects stop on every lane and a capped throttle increase selects only
the requested logical motor.

This decision authorizes props-off motor identity and direction observation
after the equal-motor RC-loss/recovery checkpoint passes. It does not authorize
mixed commands, flight use, a DShot default, motor-map changes, or ESC
direction changes. Any observed mismatch must be recorded and reviewed as a
separate controlled-configuration change.

## ADR-0027: DShot Unequal-Packet Validation Uses A Fixed Capped Vector

Status: accepted

After logical-to-physical identity passes, the next DShot stage may add
`bench_dshot_unequal_motors` to the required
`dshot bench_equal_motors` base pair. It cannot be combined with a logical or
physical selected-motor feature or PWM calibration. Full PID/mixer output
remains compile-time excluded.

The mode does not derive motor values from attitude or rate control. Below a
100-count RC throttle trigger it publishes four stops. At or above the trigger
it emits fixed logical commands `[140, 120, 100, 80]`; the committed
`[3, 4, 2, 1]` map produces physical commands `[80, 100, 140, 120]` and DShot
values `[127, 147, 187, 167]`. This supplies four different packets in one
synchronized frame set while staying below the existing 250-count bench cap.

Reusable task logic owns the trigger and remap. The result still passes through
the bounded fresh `MotorCmd` queue and armed-only actuator task. The 20 ms
nonzero-command lease, 500 Hz service, RC qualification, arming guards, and
whole-bank fault handling are unchanged.

This decision authorizes props-off unequal-packet and active-command RC-loss
tests only. It does not authorize full mixed control, flight use, DShot as the
default, waveform claims, or changes to motor mapping, direction, pins,
timers, DMA routes, or safety state.

## ADR-0028: DShot Arming Preparation Remains Stopped Until Safety Arms

Status: superseded by ADR-0031

This records the earlier stop-only preparation policy. The current FCU3 DShot
path uses the telemetry-qualified guarded-idle policy in ADR-0031.

The FCU3 DShot path does not use the legacy PWM 2.5-second low hold followed by
a 500 ms pre-armed idle spin. Disarmed DShot already emits continuous stop
frames. After the existing arm-switch and RC qualification, the actuator owner
therefore holds stop for a profiled 100 ms preparation dwell and revalidates
permission, RC armability, arm-switch state, and low throttle every 10 ms.

Preparation completion is reported while all four lanes remain stopped. The
safety master performs the final guard check and sets the system armed before
the control path may request a nonzero value. Any failed check retains stop and
reports the existing bounded arming-abort event. The 20 ms nonzero-command
lease, whole-bank fault containment, priority model, and sole actuator
ownership remain unchanged.

The FCU3 BSP independently profiles DShot idle as normalized command `65`,
which maps to protocol value `112` and preserves the prior powered props-off
result. Protocol-specific validation applies this floor only to active DShot
commands; legacy PWM retains its existing idle. Idle tuning is capped at the
250-count bench limit and requires repeated props-off target evidence rather
than an assumed generic ESC value. The unequal-vector image also asserts that
idle is no greater than its smallest fixed command (`80`), preventing a later
idle tune from silently changing the expected packet vector.

This decision changes DShot arming behavior only. It does not change PWM,
Foxeer, pin mapping, timers, DMA routes, motor order, flight authorization, or
the requirement to complete the remaining target and waveform gates before
DShot can become the default actuator protocol.

## ADR-0029: Mixed-Control DShot Requires An Explicit Clean Candidate Feature

Status: superseded by ADR-0030

The FCU3 app exposes the normal PID and Quad X mixer through DShot only when
`dshot_mixed_control` is selected. This feature implies the four-motor DShot
transport but is incompatible with equal-motor, physical/logical selected
motor, unequal-vector, stale-command injection, SPI-timeout injection, and
PWM-calibration features. Bare `dshot` remains invalid. Existing capped bench
images retain their `dshot bench_equal_motors` contract.

The candidate does not add a second control or actuator path. The normal
400 Hz controller produces the physical four-lane vector, publishes it through
the bounded fresh `MotorCmd` queue, and wakes the priority-15 actuator owner.
That owner applies the profiled DShot idle floor and renews the normal 20 ms
lease. The priority-13 500 Hz DShot service remains the only frame starter and
selects stop plus disarm on lease expiry or whole-bank fault.
The service uses a two-tick absolute release deadline; RTIC relative delays
add one SysTick and would reduce a nominal 2 ms period to roughly 333 Hz on
the app's 1 kHz monotonic.

The external FCU3 pin map, TIM1/TIM8 synchronization, four DMA2 routes, motor
map `[3, 4, 2, 1]`, interrupt priorities, RC qualification, boot/reconnect
arm-low latch, stop-only arming preparation, and safety authority are
unchanged. Four-channel PWM remains the default output profile.

This decision authorized a separately identifiable props-off target candidate,
not flight use or default promotion. Unpowered runtime, mixed stick/tilt
direction, active-output disarm, active-command RC loss, arm-high recovery
inhibition, and fresh-transition rearm were required as target evidence.

## ADR-0030: FCU3 Promotes DShot600 To The Standard Output

Status: accepted

The DShot-default promotion remains accepted. Its original stop-only pre-arm
statement below was subsequently superseded by ADR-0031.

The FCU3 default feature set now selects the existing four-lane DShot600
backend and normal rate-controller/mixer path. This is a build-selection
promotion only: timer and DMA routes, motor mapping, 500 Hz frame service,
the then-current stop-only pre-arm dwell, active-command idle floor, bounded
`MotorCmd` queue, 20 ms lease, RC-loss behavior, whole-bank fault containment,
and exclusive actuator ownership are unchanged.

The former `dshot_mixed_control` candidate feature remains as a compatibility
alias. Capped equal-motor, logical-motor, and unequal-vector modes remain
separate bench builds. The legacy four-channel PWM backend remains available
with `--no-default-features --features board-ferrowasp-fcu3`.

Default promotion does not convert source or bench evidence into flight
evidence. Prop-on use remains blocked pending the standard mixed-control
props-off checks, active-command RC-loss capture, electrical timing/jitter and
cross-timer phase measurements, motor-direction verification, and resolution
of the existing motor/ESC hardware caution.
Waveform timing, physical stop latency, and flight behavior remain separate
evidence gates.

## ADR-0031: FCU3 DShot Arming Uses Telemetry-Qualified Guarded Idle

Status: accepted

The default FCU3 DShot path supersedes ADR-0028's stop-only preparation after
the initial guarded 100 ms stop dwell. The safety master grants a temporary
preparation permit while the system remains logically disarmed. The sole
actuator owner then applies profiled idle command `65` / DShot value `112`,
renews its short lease, and rechecks permission, RC armability, arm-switch
state, and low throttle every 10 ms.

After a 250 ms spin-up grace, the actuator owner requires three consecutive
fresh observations from each physical ESC output between 3,000 and 10,000
eRPM. Missing, zero, or stale evidence aborts at the 1.2-second deadline;
overspeed aborts after the grace period. Every abort selects four stop values.
Only four-output qualification reports preparation complete, after which the
safety master repeats its guards before declaring `SYSTEM ARMED`.

The PA10 / USART1 legacy BLHeli telemetry service is active only in the default
DShot build; the explicit PWM fallback retains its guarded 2.5-second low and
500 ms idle holds. The manager rotates physical outputs, whose FCU3 mapping is
output 1 = logical M4/front-left, output 2 = M3/rear-left, output 3 =
M1/rear-right, and output 4 = M2/front-right.

The wire response has no motor identifier. A CRC-valid response that completes
while a request is still queued may be quarantined, but it is published only
after the exact sequence/output frame-start acknowledgement arrives from the
safety-owned DShot service. An acknowledgement or response timeout latches the
manager off until reboot, preventing late traffic from being associated with a
later output.

The manager has no motor or safety authority. UART, parser, bounded-queue, and
timeout faults cannot grant authority; if required observations are missing
during guarded idle, qualification fails closed. After the system is armed,
legacy telemetry loss is currently observational and does not itself disarm.
The manager waits five seconds after boot before issuing requests, so an early
arm attempt may enter guarded idle and time out after 1.2 seconds; the operator
must then move the switch low before making a fresh arm request.

IMU initialization, gyro-bias calibration, and freshness are now part of the
arming guard. They are checked before and throughout ESC qualification and
again before `SYSTEM ARMED`; stale samples cannot advance bias calibration.

## ADR-0032: Foxeer Uses A Separate Capped Actuator-Validation Gate

Status: accepted for commissioning; original flight-inhibit premise superseded

The Foxeer F405 V2 flight-readiness gate remained false until its physical IMU
orientation, ADC baseline, motor order, and M4 complementary-output polarity
were target-verified. Those motor checks could not be completed while
every actuator output is blocked, so the app provides a distinct compile-time
`bench_actuator_validation` commissioning gate.

The gate must be combined with `bench_equal_motors` or exactly one physical or
logical selected-motor feature. A gate-only image and a selected-motor image
without the gate are compile-time errors. The mode keeps the 250-command cap,
RC qualification, low-throttle arming guard, recovery latch, safety-owned
  actuator task, bounded fresh-command path, and RC-loss/disarm behavior. It
  does not make BSP verification flags true and cannot select normal PID/mixer
  output. The BSP flight profile was subsequently promoted from recorded target
  evidence; this capped gate remains available for commissioning.

The existing PWM arming sequence briefly applies idle to all four outputs
before selected/capped commands begin. The commissioning image is therefore
props-off only even with one selected motor. Its purpose is to collect the
evidence required to close the normal board gate, not to bypass that gate for
flight.

## ADR-0033: Foxeer IMU Sampling Is Triggered By PC4/EXTI4

Status: accepted, pending target validation

Foxeer replaces its temporary timer-originated IMU request with the board's
dedicated PC4/EXTI4 data-ready signal. MPU6500 initialization retains its
active-high, push-pull raw-data-ready configuration. ICM42688-P initialization
configures pulsed active-high push-pull INT1, clears `INT_ASYNC_RESET`, routes
UI data ready to INT1, and verifies all three register writes.

EXTI4 runs as a short priority-14 hardware task. It verifies and clears the
pending edge, captures the existing TIM2 microsecond timebase, and attempts to
spawn one instance of the existing bounded asynchronous SPI request. It does
not access SPI registers, wait, allocate, or command actuators. RTIC's
single-instance task capacity remains the backpressure boundary: a data-ready
edge arriving while the request task is occupied is counted as rejected rather
than queued without bound.

The SPI owner, DMA completion path, 250 us transaction deadline, timeout
recovery, parser, stale-sample detection, 800 Hz control scheduler tick, and
400 Hz control/output cadence are unchanged. FCU3 retains timer-triggered IMU
polling. Foxeer heartbeat diagnostics report total and two-second-delta EXTI4
and rejected-trigger counts. Flight arming remains inhibited until target
evidence confirms PC4 polarity and cadence, acceptable rejection behavior,
sample progress, deadline recovery, and stale-data handling.

## ADR-0034: Foxeer Onboard Flash Uses A Low-Priority CPU-Driven SPI2 Owner

Status: accepted, pending target validation

Foxeer routes its onboard NOR to SPI2 on PB13/PC2/PC3 with CS on PB12. The
STM32F405 fixes SPI2 TX to DMA1 Stream 4, which is already owned by the
validated UART4 OSD TX path. Rather than displacing working OSD or SBUS routes,
the flash manager performs short mode-0, 10 MHz CPU-driven transactions at
RTIC priority 1. Page programming and sector erasure are started quickly and
their busy state is polled asynchronously; the task never spins through the
device's internal write time.

One task exclusively owns the flash and accepts bounded record and USB-command
queues. The layout reserves two copy-on-write configuration sectors, one
destructive-test scratch sector, and an append-only CRC-protected log region.
Configuration keys and ranges are constrained by the existing tuning policy.
Destructive commands require explicit confirmation, are disarmed-only, and
abort when arming begins. The task owns no actuator peripheral or safety-state
write handle, preserving the central authority boundary.

Feature stages separate discovery from mutation: `flash_storage` is
read-only, `flash_writes` enables maintenance and configuration persistence,
and `flash_blackbox` enables control-record programming. Hardware validation
must establish JEDEC identity, write/readback behavior, torn-write recovery,
and acceptable control-loop jitter before the logger is used in flight.
