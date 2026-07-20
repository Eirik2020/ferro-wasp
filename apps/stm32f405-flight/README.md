# STM32F405 Flight App

This is the RTIC 2 flight-runtime contract for STM32F405-class flight
controllers. It currently selects the FerroWasp FCU3 BSP by default.

The app shell owns scheduling, priorities, RTIC resources, and task wiring.
Board pin maps, connected devices, DMA/timer routes, and construction belong
to `ferrowasp-bsp`. Reusable runtime logic belongs to the crates under
`../../crates`.

The FCU3 safety boundary is unchanged: only the safety-owned actuator
subsystem may command motor peripherals.

## Build

Run firmware commands from this directory so Cargo uses the F405 target,
linker, and runner configuration:

```powershell
cd apps/stm32f405-flight
cargo build --locked
```

The default features select `board-ferrowasp-fcu3` and the normal
controller/mixer over four-lane DShot600. The legacy four-channel PWM backend
is an explicit fallback:

```powershell
cargo build --locked --no-default-features --features board-ferrowasp-fcu3
```

Bench and diagnostic features remain available:

```powershell
cargo build --locked --features bench_spi_timeout_recovery
```

## Four-Motor DShot Bench Image

The DShot600 backend owns all four existing FCU3 motor pads through
synchronized TIM1/TIM8 frame sets. It is the standard FCU3 output backend.

Build the equal-motor image with:

```powershell
cargo build --locked --features "dshot bench_equal_motors"
```

The equal-motor RC-loss checkpoint and logical-to-physical identity checks
passed on FCU3 on 2026-07-18. One capped logical motor can be selected for
props-off mapping regression:

```powershell
cargo build --locked --features "dshot bench_equal_motors bench_logical_motor1_only"
cargo build --locked --features "dshot bench_equal_motors bench_logical_motor2_only"
cargo build --locked --features "dshot bench_equal_motors bench_logical_motor3_only"
cargo build --locked --features "dshot bench_equal_motors bench_logical_motor4_only"
```

The next staged image applies one fixed unequal four-motor vector without
enabling the flight mixer:

```powershell
cargo build --locked --features "dshot bench_equal_motors bench_dshot_unequal_motors"
```

Select either one logical-motor feature or the unequal-vector feature, never
both. Physical selected-motor features, multiple logical selections,
`pwm_cal`, and unequal-vector mode without DShot remain compile-time errors.
Those capped modes continue to exclude the normal PID/mixer output.

The existing arming, RC-loss, stale-command, and throttle-cap rules still
apply. Nonzero DShot commands expire after 20 ms without actuator renewal and
fall back to stop. DShot arming keeps all four outputs stopped for a 100 ms
pre-arm dwell while the actuator owner revalidates permission, RC link, arm
switch, and throttle every 10 ms. It then applies the profiled idle command
under the temporary arm permit and requires three fresh telemetry samples from
every ESC between 3,000 and 10,000 eRPM. Missing/stalled telemetry times out
after 1.2 seconds; overspeed aborts immediately after the 250 ms spin-up grace.
Either failure selects four stop values. Only successful qualification is
reported to the safety master, which performs the final guard check before
marking the system armed. The legacy PWM arming timing is unchanged.

Only the dedicated service starts DMA frame sets, preserving the fixed 500 Hz
cadence. All four motors receive the same command in the base equal-motor
image, capped at 250 PWM-style command counts. Logical and unequal-vector
images remain below that cap. These bench features replace the normal
mixed-control branch without changing the DShot transport.

## Standard DShot Mixed Control

The normal build connects the rate controller and Quad X mixer to the
four-lane DShot owner:

```powershell
cargo build --release --locked
```

The old `dshot_mixed_control` feature remains as a compatibility alias for
existing scripts. When named explicitly, it cannot be combined with
equal-motor, selected-motor, unequal-vector, stale-command, SPI-timeout, or
PWM-calibration features.

The candidate uses the normal 400 Hz controller path:

```text
PID/mixer physical vector
    -> bounded fresh MotorCmd queue
    -> armed-only actuator validation
    -> 20 ms leased DShot request
    -> synchronized four-lane 500 Hz service
```

Startup must identify it with:

```text
DShot600 standard motor output active
```

The default image also enables read-only BLHeli legacy telemetry on PA10
(USART1 RX, 115200 baud). A low-priority ESC manager owns parsing, per-ESC
state, request cadence, response association, and timeouts. It passes typed
operations to the DShot actuator owner through a bounded SPSC queue; a second
bounded queue acknowledges the exact frame start. A CRC-valid frame that
arrives while a request is queued is quarantined until that exact acknowledgement
arrives. After five seconds the manager requests one physical output at a time,
at no more than 50 requests/second aggregate. A request timeout latches
telemetry off until reboot because legacy frames carry no motor identity. An
arm attempt during the initial five-second delay can therefore enter guarded
idle, fail closed after 1.2 seconds, and require a switch-low/new-arm cycle.
For a powered test, apply ESC power before the first post-delay request. If the
FC times out while the ESC bank is unpowered, apply ESC power and reboot the FC
before retrying; power alone does not clear the telemetry latch.
Validated frames are periodically reported over RTT. The manager never owns
motor hardware or safety authority.

The standard path does not have the 250-count bench cap. On 2026-07-20 it
completed a controlled experimental outdoor flight; the operator reported
strong maneuver capability and no recurrence of the earlier unwanted yawing.
Pitch authority felt low, so pitch P `0.30` is recorded as the next isolated
candidate from the `0.25` baseline. No new BB2 flight capture accompanied that
report. Logic-analyzer timing, jitter, polarity-margin, and lane-phase evidence
remain open; see `../../mdbook/src/dshot.md` for the full evidence boundary.

For historical comparison, the dated pre-promotion candidate without
`blackbox_defmt` had SHA-256
`757F918B29771F0D62E7EBCC14B6DCE4A840E0062626B0A21F8F3EA7C6453269`.
That image's four DShot DMA IRQ symbols were at
`0x08004B44/0x08004F4C/0x08004FA4/0x08004FFC`, and loadable flash data ends at
`0x08013968`. It is not a hash for the current working tree. Periodic DShot
status includes `at <ms>` so the staged unpowered test can directly verify the
intended 500 Hz frame-set cadence.

## Flash And Attach

```powershell
cargo embed
```

For the four-motor DShot bench image:

```powershell
cargo embed --release --features "dshot bench_equal_motors"
```

For standard mixed-control DShot:

```powershell
cargo embed --release
```

For a clean, continuously flushed RTT capture, run the repository logger from
the repository root. It builds, flashes, displays decoded RTT, and writes to
`logs\terminal_embed`:

```powershell
python tools\terminal_embed.py --release --locked --features "dshot bench_equal_motors"
```

Start with propellers removed and ESC power disconnected. Confirm advancing
`DShot values ... sets completed/started, lanes [...]` RTT counters before any
powered interoperability test. All four lane counts must advance together,
with zero busy, expiry, timeout, or fault counts. A logic analyzer is still
required to validate pulse timing, jitter, and lane-to-lane phase.

During DShot arming, all four lanes receive configured idle command `65`,
which maps to protocol value `112`, before `SYSTEM ARMED` so telemetry can
provide associated, in-range start evidence for every physical output.
Successful qualification requires three fresh in-range samples from each ESC.
The 2026-07-20 powered props-off qualification run passed three arming
attempts at approximately 6,500-7,100 eRPM. Value `112` remains the accepted
candidate idle without a tuning change. The separate physical-output-1 zero-eRPM
fault-injection checkpoint also passed three attempts: output 1 (logical M4,
front-left) was identified, all outputs stopped after 1.2 seconds, and the
system never armed. Reflash the standard image before further validation.

In a logical-motor image, all four ESCs still participate in guarded arming.
Once `SYSTEM ARMED` is reached, zero RC throttle selects stop on every lane and
a small throttle increase commands only the selected logical motor. The
current committed mapping predicts:

| Logical motor | Expected physical output | Measured location |
|---|---:|---|
| 1 | 3 | rear-right |
| 2 | 4 | front-right |
| 3 | 2 | rear-left |
| 4 | 1 | front-left |

Record the actual motor and rotation direction for each image. Any mismatch is
a failed mapping checkpoint; do not compensate by changing the map during the
test.

For the unequal-vector image, throttle below 100 command counts selects four
stop values. At or above 100, the fixed logical command
`[140, 120, 100, 80]` maps to physical commands `[80, 100, 140, 120]` and RTT
DShot values `[127, 147, 187, 167]`. This exact vector, equal completion
counters, zero backend faults, and return to four zeros must be observed
before any active-command RC-loss test.

Unequal-vector Part A passed functionally on the FCU3 on 2026-07-18. Retained
RTT shows four consecutive exact-vector reports with synchronized lanes, zero
backend faults, continued IMU progress, and explicit disarm to sustained
zeros. The operator reports completing the full five-report hold and three
command-to-stop transitions; those additional actions are not present in the
retained excerpt.

Active-command RC loss passed with retained RTT on 2026-07-19. The exact
vector changed to sustained four-lane stop after frame-timeout invalidation,
remained stopped through arm-high link recovery and additional link flaps,
required a fresh guarded arm before restarting, and returned to stop on
explicit disarm. All 23 retained status reports had synchronized lanes, one
frame set in flight, and zero backend fault counters. Exact physical stop
latency remains an instrumented future checkpoint.

The existing binary name remains `FerroWasp`.
