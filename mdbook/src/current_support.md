# Current Support

This page describes what the repository supports right now. FerroWasp is in a
rapid-prototyping phase, so this is an experimental source and target-evidence
snapshot rather than a stable product matrix.

FerroWasp is open source under Apache-2.0. It is experimental firmware and is
not suitable for operational or safety-critical use.

## Board Targets

The active flight BSP target is `ferrowasp_fcu3`, board name **FerroWasp FCU3**. It
uses an STM32F405RGT6 in LQFP64. Its pin, DMA, timer, serial, ADC, IMU-axis,
and actuator policy are frozen in
`ferrowasp-bsp::stm32f4::ferrowasp_fcu3`.
The RTIC shell lives in `apps/stm32f405-flight` and selects FCU3 through its
default `board-ferrowasp-fcu3` feature.

The first non-flight bring-up target is `nucleo_f401re`, board name
**ST NUCLEO-F401RE**. It uses an STM32F401RET6 and deliberately owns only PA5
for LD2, PA2/USART2 TX for the ST-LINK virtual COM heartbeat, and SysTick for
the RTIC monotonic. EXTI0 is reserved as its RTIC software-task dispatcher.
Its capability manifest declares no attached IMU and no actuator outputs. The
isolated app contains one persistent async heartbeat task and lives in
`apps/stm32f401-bringup`, which selects Nucleo through its default
`board-nucleo-f401re` feature.

The second flight-shaped BSP target is `foxeer_f405_v2`, board name
**Foxeer F405 V2**. Its independent app lives in `apps/foxeer-f405-v2`.
The current image probes the fitted SPI1 IMU and supports either MPU6500 or
ICM42688-P, SBUS, MSP DisplayPort, ADC, a four-channel conventional RC PWM
service set, gated four-lane DShot600, and BLHeli legacy telemetry on PA10.
Telemetry-qualified arming has passed its positive and injected-failure
props-off target checks. Its first prop-on departure exposed positive pitch
feedback and attempted a forward flip. The controller-polarity correction is
implemented and passed repeated unpowered-orientation and powered normal-mixer
props-off opposition checks. Corrected controlled hops and a confined-area
flight have since passed by operator report. The current `2.5 / 2.5 / 2.0`
P-only configuration is classified as a flyable prototype, not a well-tuned or
validated flight-control system.

## Status Summary

| Area | Current status |
|---|---|
| Main target | STM32F405-class flight-controller hardware |
| Runtime model | Isolated `no_std`, `no_main`, RTIC 2 app shells for FCU3, Foxeer F405 V2, and F401 bring-up |
| Logging/debug | `defmt`, RTT, optional BB2 frames, Python tools, and staged Foxeer SPI-NOR blackbox/config storage over USB CDC ASCII or feature-gated MSPv2 RPC |
| RC input | SBUS over USART2 RX DMA |
| IMU | FCU3 MPU6500; Foxeer runtime-selected MPU6500/ICM42688-P; blocking init, async `SpiDevice` DMA samples, 250 us transport deadline |
| Control loop | Timer-driven 400 Hz control; FCU3 retains 800 Hz IMU polling, while Foxeer samples from PC4/EXTI4 data-ready events |
| Estimation | Simple gyro low-pass plus accel-assisted roll/pitch complementary integration |
| Mixer/control | Quad rate controller, PID/FF contributions, and mixer in `crates/ferrowasp-tasks/src/drone_toolbox.rs` |
| Motor output | FCU3 and Foxeer: default four-lane DShot600 with capped bench modes and explicit four-channel PWM fallbacks |
| Safety gate | Prototype safety master, arm qualification, telemetry-qualified DShot idle, guarded disarm and RC-loss paths |
| ADC | ADC1 DMA path for internal temperature, battery voltage, and current-sense input |
| USB | Optional `usb_serial`; Foxeer emits `FWDBG1` status and bounded ASCII storage commands, or selects the opt-in `mspv2_configurator` native endpoint |
| DShot | FCU3 defaults to four-motor DShot600; unpowered, powered props-off, fault-injection, and initial operator-reported flight checkpoints have passed; electrical timing/jitter and measured stop latency remain open |
| ESC telemetry | FCU3 default DShot image: target-validated BLHeli legacy UART telemetry on PA10 / USART1 RX. Foxeer: target-validated request association, eRPM, positive idle qualification, and injected missing-evidence rejection on the same bounded manager/route |
| MSP / OSD | MSPv1 DJI O4 OSD path on UART4 with DisplayPort text frames and status responses |
| Secondary target | NUCLEO-F401RE RTIC LED/USART bring-up; static checks and target smoke pass |
| Additional target | Foxeer F405 V2 isolated RTIC app; ROM-DFU/SWD, USB, ICM42688-P/EXTI, RC, PWM, default DShot, legacy eRPM, telemetry-qualified arming, onboard blackbox, corrected axis opposition, controlled hops, and confined-area prototype flight have target evidence |

## Board and Pin Assumptions

The current firmware is centered on STM32F405-class hardware using `stm32f4xx-hal`.

FerroWasp FCU3 pin/peripheral use:

| Function | Peripheral/pin |
|---|---|
| SBUS RX/TX | USART2 on PA3/PA2 |
| BLHeli legacy ESC telemetry RX | USART1 RX on PA10; PA9 TX is not configured |
| IMU SPI | SPI1 on PA5/PA6/PA7 |
| IMU chip select | PA4 |
| Physical output 1, PWM / DShot600 | TIM1 CH1 on PA8 |
| Physical output 2, PWM / DShot600 | TIM3 CH4 / TIM8 CH4 on PC9 |
| Physical output 3, PWM / DShot600 | TIM3 CH3 / TIM8 CH3 on PC8 |
| Physical output 4, PWM / DShot600 | TIM12 CH2 / TIM1 CH3N on PB15 |
| Control scheduler | TIM4, pinless, 800 Hz interrupt source |
| I/O timebase | TIM2, pinless, 1 MHz free-running counter |
| I/O watchdog | TIM6, pinless, 8 kHz deadline check |
| USB FS | PA11/PA12 when `usb_serial` is enabled |
| DJI O4 MSP OSD | UART4 on PA0/PA1 |
| Debug LEDs | PB0/PB1 alternate red/green as a firmware liveness heartbeat |
| ADC voltage input | PC0 |
| ADC current input | PC1 |

These mappings are the frozen FerroWasp FCU3 BSP policy. The RTIC app owns the
concrete peripheral instances, while FCU3 BSP constructors assemble them
according to this manifest.

Shared flight logic uses Betaflight Quad X logical motor numbering: motor 1 is
rear-right, motor 2 is front-right, motor 3 is rear-left, and motor 4 is
front-left. FCU3's measured physical output order is front-left, rear-left,
rear-right, front-right, so its logical-to-physical output map is
`[3, 4, 2, 1]`. This preserves the flight-tested FCU3 physical correction
behavior while presenting Betaflight-style motor labels. Target observation
confirmed M1 rear-right CW, M2 front-right CCW, M3 rear-left CCW, and M4
front-left CW. Motor order, motor direction, and stick/tilt response must be
re-tested with propellers removed before any flight after a board profile,
output backend, or motor wiring change.

In the inverse physical-output view used by ESC telemetry and actuator logs,
output 1 is logical M4/front-left, output 2 is M3/rear-left, output 3 is
M1/rear-right, and output 4 is M2/front-right.

Foxeer F405 V2 uses a separate board contract:

| Function | Peripheral/pin |
|---|---|
| SBUS RX/TX | USART2 on PA3/PA2 |
| Optional BLHeli legacy ESC telemetry RX | USART1 RX on PA10 with `esc_telemetry`; PA9 TX is unused |
| IMU SPI / CS | SPI1 on PA5/PA6/PA7, CS PA4, mode 3 |
| Physical output 1 PWM | TIM1 CH1 on PA8 |
| Physical output 2 PWM | TIM8 CH4 on PC9 |
| Physical output 3 PWM | TIM8 CH3 on PC8 |
| Physical output 4 PWM | TIM1 CH3N complementary output on PB15 |
| Control scheduler | TIM4, pinless, 800 Hz interrupt source |
| I/O timebase | TIM2, pinless, 1 MHz free-running counter |
| I/O watchdog | TIM6, pinless, 8 kHz deadline check |
| DJI O4 MSP OSD | UART4 on PA0/PA1 |
| ADC voltage/current | PC0/PC1 |
| Optional USB debug | OTG FS on PA11/PA12 with `usb_serial` |
| Optional onboard flash | SPI2 mode 0 on PB13/PC2/PC3 with CS PB12; CPU-driven at 10 MHz |
| Debug | SWD/RTT; PA13/PA14 status LEDs are not claimed |

Foxeer timer-DMA DShot has unpowered and powered props-off target evidence.
PA10 legacy telemetry has powered props-off request-association, stopped/idle/
throttle eRPM, and disarm-under-throttle evidence with clean fault counters.
It now feeds the bounded actuator-owned pre-arm qualification path matching
FCU3; positive and injected missing-evidence target checks passed. Target
testing has confirmed the
fitted ICM42688-P, body-axis map, `[1, 2, 3, 4]` logical-to-physical motor map,
expected motor directions, and functional M4 `TIM1_CH3N` polarity. Exact PWM
timing remains unmeasured because the logic-analyzer checkpoint was skipped.
ADC scale calibration remains the BSP's normal-flight arming inhibit. The
NUCLEO-F401RE bring-up target has no actuator outputs.

## RC Input

SBUS is the currently wired RC path.

The UART setup uses:

- 100000 baud
- even parity
- 2 stop bits
- DMA receive
- fixed-size DMA buffers and `heapless` queues

The serial protocol and frame-size contract now lives in `ferrowasp-io-core`.
Reusable STM32F4 UART/SPI/ADC DMA bridges, static PWM mechanisms, and the HAL
prelude live in `ferrowasp-stm32f4`. FCU3 route conversion, target aliases,
device construction, and safe static-storage shaping live in
`ferrowasp-bsp::stm32f4::ferrowasp_fcu3`. The RTIC app declares the concrete
board DMA storage through `#[init(local = [...])]`.

The portable serial RX API uses owned, bounded chunks and implements
`embedded_io_async::Read`. Continuity and queue-health events are exposed
through a separate observer because a byte stream cannot by itself prove that
no data was lost. Both UART4 MSP/OSD and USART2 SBUS use this live API. The
USART2 DMA/IDLE owners publish completed chunks and wake one persistent async
SBUS parser.

The UART4 bridge has passed target validation with live DJI O4 MSP traffic:
the OSD displayed and updated, throttle followed RC input, the IMU remained
healthy, and RTT reported no transport warnings.

The parser extracts roll, pitch, yaw, throttle, and arm-switch state. The current arm switch is channel 8, with `ARM_THRESHOLD` set to 1500.

The RC path produces:

- roll/pitch/yaw rate requests
- throttle command in a 0..2000 range
- arm-switch-high signal

The prototype RC-loss policy starts invalid, requires three consecutive
healthy frames, and expires after 100 ms without a healthy frame. DMA,
transport, parser, SBUS frame-lost, and SBUS failsafe faults invalidate the
link. Recovery also requires observing the arm switch low before a later arm
request can qualify.

This path is target-validated with the current DJI O4 Air Unit Lite, Goggles 3,
and RC3 setup. Link interruption produced one timeout invalidation, reconnection
while the arm toggle was high did not rearm, and a fresh low-to-high toggle was
required before arming could be requested again.

## IMU Path

`ferrowasp-drivers` contains allocation-free MPU6500 and ICM42688-P drivers.
FCU3 keeps its existing MPU6500 path. Foxeer starts SPI1 in mode 3 at 1 MHz,
reads register `0x75`, and selects:

- MPU6500 for `WHO_AM_I=0x70`, using the 14-byte burst from `0x3b`
- ICM42688-P for `WHO_AM_I=0x47`, using the 14-byte burst from `0x1d`

The ICM42688-P path verifies soft-reset completion and configuration
read-back, disables I2C while preserving big-endian output, and configures
1 kHz low-noise accelerometer/gyro sampling at +/-16 g and +/-2000 dps. It
waits through the documented gyro startup interval before enabling periodic
sampling. Other identities and probe/configuration failures are logged once
and leave sampling disabled while the rest of the firmware continues.

Supported common behavior:

- blocking `embedded-hal` SPI register setup during boot
- sensor-specific identity, reset, power, range, and filter configuration
- checked big-endian accel, gyro, and temperature burst decoding
- physical-unit conversion using the active sensor's scale
- DMA-backed SPI sample reads
- a unique, non-cloneable `embedded-hal-async::SpiDevice` request handle
- owned operation/TX/RX storage with copy-back only after successful completion
- generation-scoped cancellation when a transaction future is dropped
- TIM2-stamped 250 us transaction deadlines
- TIM6 watchdog posting timeout recovery to the SPI owner priority
- static receive buffers returned after both valid and invalid frames

The portable adapter accepts the standard read, write, transfer,
transfer-in-place, and delay operation shapes with bounded storage. The current
STM32F4 DMA engine deliberately accepts only one 15-byte full-duplex
transaction. Both supported sensor bursts fit that fixed shape. Variable-length
and multi-operation hardware sequencing remain future backend work. Foxeer now
configures PC4/EXTI4 data-ready triggering. EXTI4 timestamps and defers bounded
SPI DMA work, while rejected task triggers are counted. Functional target
evidence shows about 1.012 kHz sample progress with zero rejected triggers.
Exact electrical pulse shape and deliberate stale-IMU fault injection remain
open.

The control loop currently uses filtered gyro rates and a simple accel-assisted
angle estimate. Raw gyro values remain sensor-axis data, while the shared
`imu_rates` tuple is mapped into measured roll, pitch, and yaw for the rate PID.
The standard drone body frame is forward/right/down: +X forward, +Y right, and
+Z down, with angular rates following the right-hand rule. BSP profiles describe
orientation as two signed-axis rotations: IMU sensor frame to board frame, then
board frame to drone body frame. FCU3's composed IMU-to-drone mapping preserves
the current bench/flight evidence; Foxeer's orientation is target-verified from
sustained level, roll, pitch, and yaw motions.
The prototype FCU3 controller/mixer predates the physical frame type and uses a
nose-down-positive pitch convention. Foxeer keeps its measured right-handed
physical body map for acceleration and estimation, then applies an explicit
pitch-only compatibility transform to gyro rates passed to that controller.
The optional `blackbox_defmt` feature emits both raw and filtered control-axis
rates as BB2 frames for bench observation. This is useful for bring-up, but it
is not yet a validated estimator.

## Control Loop

The prototype schedules IMU polling at 800 Hz and runs the control/motor update at 400 Hz.

The control loop currently:

- requests an SPI IMU sample
- filters gyro rates with a first-order low-pass filter
- integrates gyro rates into roll/pitch/yaw angles
- blends accelerometer roll/pitch into the angle estimate
- reads RC rates and throttle when armed
- runs a rate controller with PID terms, D-term filtering, I-term windup protection, I-term relax, and optional feedforward
- mixes quad-X motor commands
- requests actuator output

The controller is still prototype-level. Board-specific initial profiles live
in `crates/ferrowasp-tasks/src/drone_toolbox.rs`. FCU3 retains its golden-app
first-hop profile; Foxeer fresh storage defaults to P-only
`2.5 / 2.5 / 2.0` for roll/pitch/yaw with every I and D gain zero. A valid
persisted configuration remains authoritative across firmware updates. The
local `ferrowasp-pid` crate exposes more tuning-relevant behavior:

- loop-time-aware I and D calculations
- D-term on measured gyro rate instead of setpoint error, avoiding D-kick on stick steps
- symmetric output and per-term limiting
- conditional integration for windup protection
- I-term relax on fast setpoint movement
- first-order D-term filtering
- optional feedforward based on setpoint rate
- per-axis logging of P, I, D, feedforward, total output, error, setpoint rate, and measurement rate

`crates/ferrowasp-tasks/src/drone_toolbox.rs` also exposes `RateBlackboxSample` and a
fixed-point `CompactRateBlackboxSample`. With the `blackbox_defmt` feature, the
400 Hz control loop emits compact `BB2` frames over the existing `defmt-rtt`
stream containing raw gyro rates, filtered gyro rates, commanded rates, PID
output, throttle, and mixed motor commands. The current defmt trace logs also
include PID total plus separate P/I/D/feedforward arrays for bench tuning.

## Safety and Arming

The current safety model is a prototype, but the important authority split is already present:

```text
RC input -> Safety Master -> Actuator Output
Control Loop -> Actuator Output
```

The safety master owns the system arm decision. The actuator task owns motor hardware.

Current arming behavior:

- arm switch must remain high long enough to qualify
- throttle must remain at or below `ARMING_MAX_THROTTLE` throughout arming
- the RC path must be qualified and armable; boot or link recovery with arm
  already high cannot request arming until a fresh low-to-high transition
- the safety master grants a temporary, protocol-specific actuator preparation
  permit
- the actuator owner performs the configured PWM or DShot preparation and
  reports completion; this event does not itself declare the system armed
- the safety master re-checks the complete guard before setting system armed
- disarm revokes permission, clears system armed, and commands low output
- the actuator owner rechecks permission, RC validity, arm state, and throttle
  every 10 ms during preparation and forces stop before reporting an abort

The default FCU3 DShot path holds four stop values for 100 ms, then applies
idle command `65` / DShot value `112` under the temporary permit. After a
250 ms spin-up grace, the actuator owner requires three consecutive fresh
telemetry observations from each motor in the 3,000-10,000 eRPM window. A
missing, zero, or stale RPM fails at the 1.2-second deadline; an RPM above the
ceiling fails immediately after the grace period. Every failure selects four
stop values. Only successful four-motor qualification is reported to the
safety master for its final guard check.

The ESC manager waits five seconds after boot before issuing telemetry
requests. An arm attempt that enters guarded idle during that window may fail
closed at the 1.2-second deadline because no qualifying samples are available.
The arm switch must then be observed low before a fresh arm request can
qualify.

For a powered arming test, apply ESC power before the first request after that
delay. If the FC is left running with the ESC bank unpowered, the missing
response latches telemetry off until reboot. Applying ESC power afterward does
not clear the latch; reboot the FC and use a fresh low-to-high arm request.

IMU availability, gyro-bias calibration, and sample freshness are explicit
arming prerequisites in both flight apps. The guard runs before actuator
preparation, throughout the guarded hold/eRPM qualification, and again before
the safety master enters `Armed`. Repeated IMU samples do not advance bias
calibration. Host tests cover each rejection reason; target fault injection is
still required as negative evidence.

The telemetry-qualified positive path and an injected physical-output-1 /
logical-M4 zero-eRPM failure have powered props-off target evidence. The
injected failure named the unproven output, selected sustained four-lane stop,
and never reported `SYSTEM ARMED`. The legacy PWM fallback and Foxeer PWM path
retain the guarded 2.5-second low and 500 ms idle holds.

The Foxeer BSP flight profile accepts its documented Betaflight voltage
baseline and Foxeer-published current scale. Fine current zero-offset
calibration remains open, so displayed current is forced to zero while raw ADC
millivolts remain available. A separate `bench_actuator_validation`
commissioning gate selects only a capped
equal-motor or single physical/logical-motor props-off image. It retains the
normal RC, arming, freshness, failsafe, and actuator-owner boundaries and
cannot enable normal PID/mixer flight output.

Foxeer DShot reuses the same encoder, timer/DMA bank, command lease, and fault
handling as FCU3, but retains board-local RTIC wiring. Its first powered attempt
exposed that the board-local `EnterIdle` branch still selected the PWM
preparation policy. The corrected branch now uses the FCU3-compatible 100 ms
guarded stop-frame dwell and remains stopped until `SYSTEM ARMED`. Foxeer now
implements FCU3's eRPM-qualified idle policy; both its positive four-motor run
and injected missing-M1-evidence rejection have powered props-off evidence.

Current actuator behavior:

- `EnterIdle` is ignored unless actuator idle permission is set
- `ApplyLatestThrottle` is applied only when the system is armed
- control loop motor values cross a four-entry SPSC `MotorCmd` queue
- actuator output drains to the newest command and rejects commands older than
  20 ms
- missing, stale, or invalid commands produce low output
- disarm and arming entry discard queued commands from an earlier arm cycle
- invalid or low motor values are floored to idle while armed
- disarm commands low throttle

RTIC actuator spawns now carry only an `ActuatorCmd` wake-up. Motor values are
owned by the queue, including equal-motor and individual-motor bench modes.
Static verification and the stale-command fault-injection checkpoint pass.

The current age check runs when actuator output is woken. An independent
actuator deadline watchdog for total loss of future control-loop wakes remains
future work.

## Motor Output

The standard FCU3 and Foxeer motor-output paths are four-lane DShot600. Their
RC-PWM backends remain available through explicit no-default-features fallback
builds. Foxeer's complementary physical output 4 (`TIM1_CH3N`) and all four
motor identities have powered props-off evidence.

Current PWM configuration:

- 400 Hz PWM
- 1000..2000 us pulse range
- 0..2000 command range
- 4 motor outputs
- optional `pwm_cal` feature for a single-motor calibration flow

DShot packet creation and encoding helpers live in `ferrowasp-waveform`.
The integrated FCU3 backend uses the existing external pins:

| Physical output lane | Timer output | DMA route |
|---|---|---|
| 1 / PA8 | TIM1 CH1 | DMA2 Stream1 Channel6 |
| 2 / PC9 | TIM8 CH4 | DMA2 Stream7 Channel7 |
| 3 / PC8 | TIM8 CH3 | DMA2 Stream4 Channel7 |
| 4 / PB15 | TIM1 CH3N | DMA2 Stream6 Channel6 |

It includes:

- 16-bit packet generation
- 11-bit value plus telemetry bit
- 4-bit checksum
- compare-event timer DMA encoding
- throttle mapping to DShot range
- one hardware-synchronized frame set across TIM1 and TIM8
- continuous stop frames while disarmed
- leased nonzero commands with stop/disarm fallback after expiry
- all-lane DMA completion accounting
- whole-bank fault containment that forces all four lines low

TIM1 is the start master and TIM8 is armed as its internal-trigger slave. All
four DMA streams are enabled before either timer can emit a pulse. A frame set
is complete only after every lane reports transfer completion. Any lane error,
unexpected interrupt, or completion timeout faults the bank, closes both
advanced-timer output gates, forces all four pads low, and requests disarm.

The capped bench build requires the base feature pair
`dshot bench_equal_motors`. It keeps ordinary RC qualification and arming,
sends the same authorized throttle to all four motors, and retains the
250-count cap. Exactly one optional `bench_logical_motorN_only` feature may
instead select a capped logical motor for mapping validation.

The normal PID/mixer path is the default FCU3 DShot build and has no 250-count
bench cap. It uses the bounded fresh `MotorCmd` queue, safety-owned actuator
owner, 20 ms lease, and synchronized 500 Hz service; normal mixed commands
still require the system armed state. The former
`dshot_mixed_control` candidate feature remains a compatibility alias.
The standard image has passed corrected 500 Hz unpowered runtime with 12,000
synchronized four-stop frame sets and zero backend faults. Props-off
mixed-control arming, idle, throttle, explicit disarm, and active-command
RC-loss/recovery also passed with synchronized lanes and zero backend faults.
Retained BB2 runs subsequently confirmed roll, pitch, and yaw stick mixing and
motion-opposing correction.
The current source replaces DShot's legacy PWM-style pre-armed delay with a
100 ms stop dwell followed by telemetry-qualified idle under a temporary arm
permit. The actuator owner rechecks the complete arming guard every 10 ms,
applies idle command `65` / DShot value `112`, and requires three fresh
3,000-10,000 eRPM observations from every ESC. The system remains logically
disarmed during qualification; only a successful completion report and the
safety master's final guard can set `SYSTEM ARMED`. Any guard or qualification
failure selects four stop values. Value `112` remains the accepted prototype
idle; continued cold-start and temperature-margin characterization remains
prudent.
PWM behavior is unchanged.
The synchronized four-motor image passed its unpowered 12,000-frame runtime
checkpoint with equal lane counters and no backend faults. Its first powered
attempt spun physical output lanes 1-3 but not lane 4/front-right; the healthy
counters led to a TIM1_CH3N polarity audit and a source correction from
active-low to active-high.
The corrected powered props-off image then drove all four motors through armed
idle and equal capped RC throttle, returned all four to stop on explicit
disarm, and reached 10,000 frame starts with equal lane counters and zero
backend faults. A later powered props-off RC-loss/recovery run sustained four
stop values, prevented automatic arm-high reconnection, accepted a fresh
manual arm sequence, and returned to stop on explicit disarm with equal lane
counters and zero backend faults. Its retained RTT excerpt begins after the
initial invalidation/stop transition, so physical stop latency remains
unmeasured. Four subsequent selected-motor runs confirmed logical motors
1/2/3/4 are rear-right/front-right/rear-left/front-left through physical
outputs 3/4/2/1. The same target work confirmed CW/CCW direction and retained
per-run backend diagnostics. BLHeli legacy UART telemetry and standard
mixed-control props-off checks also passed. Bidirectional DShot telemetry,
special commands, and logic-analyzer timing/synchronization remain pending.
The capped unequal-vector Part A target run emitted fixed physical commands
`[80, 100, 140, 120]` after a 100-count RC trigger, reported exact DShot
values `[127, 147, 187, 167]` with synchronized lanes and zero backend faults,
and returned to sustained stop on explicit disarm. The operator reports the
full repeated-transition procedure passed; the retained excerpt contains four
consecutive active reports and the final disarm transition. Unequal-vector
active-command RC loss also passed with a retained log covering timeout,
sustained stop, arm-high recovery inhibition, fresh rearm, and final disarm.

### Initial Flight Status

On 2026-07-20 the operator reported that the controlled experimental FCU3
flight on the standard DShot path went well and allowed strong manoeuvres. The
previous unwanted yawing was absent by pilot observation. Pitch authority felt
lower than desired; the parked follow-up is an isolated pitch-P change from
`0.25` to `0.30` after checking motor headroom. No new BB2 flight capture was
retained, so this report does not close timing, saturation, or quantitative
tracking questions and does not imply routine flight readiness.

## Telemetry and Configuration

The current firmware has early communication and display pieces:

- optional USB CDC serial device via `usb_serial`
- Foxeer `FWDBG1` status every roughly two seconds, including IMU, control,
  RC, arming, voltage, and current snapshots
- staged Foxeer SPI-NOR support: read-only identity/log access with
  `flash_storage`, disarmed-only maintenance and dual-slot whitelisted tuning
  storage with `flash_writes`, and CRC-protected armed-flight logging with
  `flash_blackbox`
- opt-in Foxeer `mspv2_configurator`: bounded native framing, standard `FWSP`
  identity/status commands, versioned whole-config stage/commit/reset, and
  disarmed-only 512-byte CRC-protected blackbox reads over function `0x7A00`
- FCU3's older USB endpoint remains a minimal one-shot hello
- MSPv1 parser/serializer module with tests
- DJI O4 MSP OSD task using UART4 TX/RX DMA
- DisplayPort frames for clear, write string, draw, and heartbeat
- OSD values for armed state, pack voltage, cell voltage, current, and throttle
- BLHeli legacy ESC telemetry on PA10 / USART1 RX DMA at 115,200 baud in the
  default FCU3 DShot image; inactive in the explicit PWM fallback
- UART modes for SBUS, MSP, and MAVLink configuration values

The base Foxeer USB-status image discards input; ordinary flash-enabled images
parse only the bounded ASCII storage/config command set. The opt-in MSPv2 image
uses the same low-priority flash owner and whitelist. Configuration keys and
ranges remain explicit, persistence and blackbox reads require a disarmed
state, active logs are not downloadable, and unsupported erase/reboot requests
fail closed. OSD and USB remain non-authoritative over safety state and motor
output. The native endpoint has static verification but still requires a
target USB interoperability checkpoint with the host configurator.

Legacy ESC frames do not identify their source motor. A low-priority ESC
manager therefore rotates physical DShot output lanes 1-4, owns response
association and timeouts, and uses bounded request and acknowledgement queues.
The safety-owned DShot service remains the sole request consumer and
acknowledges only after the selected telemetry bit appears in a frame that
actually started. A CRC-valid response received while the operation is still
queued may be buffered, but it remains quarantined until that exact
sequence/output acknowledgement arrives. Association timeouts latch telemetry
off until reboot so late traffic cannot be associated with a later output.

The manager publishes timestamped observations but has no motor peripheral,
arming, or failsafe authority. Transport, parser, queue, and timeout faults
cannot grant authority; if they prevent the samples required during guarded
idle, arming qualification fails closed. After the system is armed, telemetry
loss is currently observational and does not itself disarm. Powered props-off
evidence covers eRPM and clean request/response association; voltage, current,
consumption, and temperature remain unsupported or unvalidated on the
installed ESC setup.

The portable serial layer also has a bounded `embedded_io_async::Write`
implementation. UART4 OSD uses this live path: a persistent RTIC worker starts
owned chunks on DMA1 Stream 4, and the DMA IRQ reports completion through a
separate handle. Its `flush` waits for both queue drain and explicit backend
completion of the in-flight chunk. The STM32F4 backend currently preserves the
validated fixed 70-byte zero-padded MSP transfer. The complete async RX/TX OSD
path is functionally target-validated on the current FCU and DJI O4 link.

## Known Prototype Gaps

These are expected at the current stage:

- crate/workspace split is in progress; the live RTIC app correctly retains
  concrete runtime ownership while its board construction policy comes from
  the FCU3 BSP
- the FCU3 manifest is active policy and conflict-test input, but there is no
  source generator for RTIC resource declarations or interrupt bindings yet
- no CRSF/ELRS implementation yet
- FCU3 DShot600 has synchronized unpowered and powered props-off evidence for
  mixed and capped vectors, telemetry-qualified arming, injected stalled-motor
  rejection, logical motor identity/direction, all-axis correction signs, RC
  loss, fresh rearm, explicit disarm, and restart interlock. The initial flight
  result is operator-reported without a new BB2 capture. Logic-analyzer
  timing/jitter, measured stop latency, cross-timer phase, and quantitative
  flight tracking/saturation evidence remain open
- SPI1 async ownership and timeout recovery are functionally target-validated;
  logic-analyzer CS/timing evidence is deferred until suitable equipment is
  available
- SPI transport deadline monitoring exists, but there is no complete
  system-level fault manager, actuator watchdog, or health escalation yet
- RC-loss handling exists for the active SBUS path, but wider link-quality,
  fault-manager, and flight-mode policy remains prototype-level
- healthy/calibrated/fresh IMU is now a pre-arm prerequisite, but negative
  target fault-injection evidence and broader sensor/setpoint freshness policy
  remain incomplete
- the feature-gated MSPv2 configurator endpoint still needs host/target USB
  interoperability evidence and is not enabled in normal flight images
- OSD data freshness is not complete yet
- battery cell count, ADC scale assumptions, and current-board IMU axis mapping
  now live in the FCU3 BSP profile, but still need real board-specific
  calibration before becoming validated configuration
- Foxeer has unvalidated dual-slot persistent tuning storage; FCU3 and general
  parameter persistence remain open
- Foxeer F405 V2 has physical ROM-DFU/SWD, USB enumeration, ICM42688-P/EXTI,
  verified IMU orientation, powered props-off motor order/direction, default
  DShot/eRPM-qualified arming, RC-loss/rearm interlocks, and onboard-blackbox
  evidence. The final normal-mixer/OSD props-off handoff passed, but the first
  prop-on departure exposed positive pitch feedback and attempted a forward
  flip. Its code correction passed new unpowered and powered props-off
  opposition checks, followed by corrected hops and confined-area flight. The
  current P-only tune is operator-classified flyable but not well tuned. Fine
  ADC calibration and exact waveforms remain open
- the prototype estimator and P-only rate loop have flight evidence, but are
  not fully tuned or validated; I/D remain disabled
- Foxeer onboard SPI-NOR blackbox recording has multiple CRC-valid props-off
  and flight captures with final partial-page flushes and no reported timing
  loss in the selected records. Recorded timestamps retain the expected 2/3 ms
  cadence with no interval above 3 ms. Per-flight configuration, per-motor
  eRPM, and accelerometer/crash evidence remain open. FCU3 still relies on RTT
  logging
- no evidence package or formal traceability yet

That is acceptable for rapid prototyping. The main rule is to keep learning fast while preserving the big safety boundary: only the actuator-output path should touch motor hardware.
