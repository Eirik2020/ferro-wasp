# Requirements Design: Hardware Contracts and the Control Loop

Sections 12, 13 and 21 of [the requirements design](design.md), which
maps every section to its chapter.

## 12. Hardware contracts

**Principle.** Traits inside a task group let a device be swapped; messages
between a group and the core mean nothing has to be swapped.
`ferrowasp-io-contracts` holds only message types, and the core depends on no
hardware trait. A trait earns its place only where a task calls into a device
during its own run; since every link is a queue, hardware groups produce
messages and the core consumes them.

A contract is more than its types: behaviour such as "RX reports a
discontinuity on overrun" is part of it. Each contract therefore carries its
own requirements, verified by one shared conformance suite run against every
implementation. Porting to a new chip family means passing the conformance
suite on it, with no core re-test.

**Serial RX.** The user asked what serial RX is for. It is the UART box in
the path: it hands the core the received bytes plus what the core needs to
judge whether they can be trusted, which only the hardware layer can know.
`RxChunk` already carried it:

| Field | Why the core needs it |
|---|---|
| `bytes`, `len` | the data, in order |
| `timestamp` | arrival time, from which Command Input measures a frame's age |
| `discontinuity_before` and cause (`QueueOverflow`, `DmaError`, `FramingError`, `PortReconfigured`, `TransportReset`) | bytes were lost: discard a partial frame and resync, never join bytes across a gap |
| `uart_error_seen` | a noise or framing error, a corruption signal separate from the protocol CRC |
| `generation` | the port was reconfigured; partial data from before is invalid |

How chunks are produced is chip-specific (F4: DMA stream, IDLE-line interrupt,
error flags; H7: different DMA and cache maintenance). Every serial consumer
uses the same contract: RC (CRSF, SBUS), actuator feedback over BLHeli UART,
the CLI and configurator, GPS later. Serial RX became a message contract, not
a trait. The `SerialRxMailbox` (a `Deque` behind `critical_section::Mutex`,
shared between interrupt and reader task) becomes an SPSC link, which removes a
global critical section from the RC path.

**IMU.** The IMU has two kinds of hardware: the MCU (how bytes move: SPI, DMA,
EXTI) and the sensor (what the bytes mean: registers, scaling, FIFO, faults),
which vary independently. The user's chain:

```
IMU driver crate        FerroWasp IMU trait        IMU task group               core
(embedded-hal SPI) ───▶ (one impl per sensor) ───▶ (generic over I: Imu) ─SPSC─▶ consumes ImuSample
                                                    data-ready, scheduling,
                                                    timestamp, sequence
```

The MCU dependence sits only under the driver, in the HAL's
`embedded-hal(-async)` `SpiDevice` implementation, DMA-backed per family. The
data-ready pin can be `embedded_hal_async::digital::Wait`, so the group is
chip-agnostic too.

| Unit | Verified by | Re-owed by |
|---|---|---|
| each IMU's trait implementation | host tests against a mock `SpiDevice` (`embedded-hal-mock`) with golden register dumps; the trait's conformance suite (decode, faults, ID check); one MCU-level run on any board with that sensor | that driver or the trait |
| the IMU task group | unit tests with a mock `Imu`; path evidence | the group or the trait |
| MCU SPI and DMA | `SpiDevice` conformance per MCU family | that backend |
| core | unit and SITL on `ImuSample` | the core or `ImuSample` |

This delivers the user's 2026-10-12 aim: the MPU-6000 driver is proven once and
stays proven on every board while unchanged. An external driver crate is
`trust: external` and verified through the same conformance suite.

`ImuSample` (in `io-contracts`) carries gyro and accel in typed units in the
sensor's own frame plus a sensor id; `data_ready_at` (the edge time, not the
read-completion time, which is where clock drift shows); a sequence number
(gaps, separate from staleness); per-axis saturation (a clipped gyro reading
is not a valid rate); and a fault (not responding, ID mismatch, transfer
error). Rotation into the body frame stays an explicit core step using the
board's orientation data: AGENTS.md forbids a sign change hidden in a sensor
or mixer layer. The trait and group live in `ferrowasp-imu`, so a
trait change re-owes drivers and group, never the core. IMU → core is a
`state` link.

**Actuator feedback** (renamed from ESC telemetry; safety-critical because
eRPM qualifies arming):

|  | BLHeli/KISS over UART | bidirectional DShot |
|---|---|---|
| transport | a separate wire | the DShot wire, reversed after each frame |
| rate | round robin, one motor per request over the telemetry bit | every frame, every motor |
| delivers | eRPM, temperature, voltage, current, consumption | eRPM; temperature, voltage, current only with EDT, interleaved |
| decoder | `drivers::blheli_telemetry` | `waveform::dshot_reply` (GCR) |

No trait: bidirectional decode must live in the DShot output group, which
owns the pin and turns it around; BLHeli decode lives in an ESC-telemetry
group over `RxChunk`. What they share is the output, `ActuatorFeedback`:
physical output index (logical mapping is the W6 motor map, applied
explicitly in the core), eRPM, observed-at, sequence, source, decode-fault
counts. Rules:
- one declared source per quantity, chosen at boot by section 21's stored
  setting, fixed until reboot, never switched automatically. Switching
  between UART and DShot telemetry is possible by changing the setting and
  rebooting; boot validates the choice against the board and fails closed if
  invalid. This follows "never falling back" and "diagnostic features never
  replace [the motor path] silently";
- the producer splits by criticality: eRPM goes to the core; temperature,
  voltage and current go straight to the observer. Over-temperature or
  current protection would move them into the core message deliberately.

**Motor output.** Before, `actuator_output` (15) locked the shared
`dshot_motors` and wrote throttles through `apply_with_lease`. Under the
design actuator authority sends messages to the output group:

- `MotorCommand`: per physical output `Stop` or `Throttle(normalized)`, with
  issued-at and sequence. `Stop` is distinct from `Throttle(0)` (DShot 0 means
  disarmed, 48+ is throttle; PWM has its own minimum); the core says what it
  means and each protocol encodes it;
- `DshotCommand`: direction, beep, EDT enable, save; accepted only while every
  output is `Stop`, which the output group also checks;
- the output group makes the final freshness check: if its newest command is
  older than the maximum age, it outputs `Stop` on every output whatever
  happened upstream. It is a conformance requirement ("a stopped command
  stream stops the motors within N ms") and the last link of stale-command
  containment (W1, W2);
- the queue adds a hop; control-to-wire latency becomes a measured path
  requirement, so the hop is shown acceptable, not assumed.

**Protocol crates.** The user's requirement: CRSF, SBUS and BLHeli telemetry
are each their own crate following the embedded-hal / embedded-io standards,
so they work outside FerroWasp, behind traits like the IMU's (an RC protocol
trait in `ferrowasp-input`, an ESC-telemetry trait in `ferrowasp-feedback`).
Each has golden-frame tests plus the trait's conformance suite: valid frames,
the failsafe flag, frame lost, out of range, corrupted, and a split across a
discontinuity.

- The RC protocol is chosen at boot (`rc_protocol`), unlike the board-fixed
  IMU, so the group needs runtime dispatch without allocation. The existing
  `RcReceiver` enum (`ferrowasp-flight/src/rc_receiver.rs`:
  `Sbus(sbus_rs::StreamingParser)` or `Crsf { parser, .. }`, producing a
  common `RcFrame` of 16 channels and the failsafe and frame-lost flags) is
  that dispatch type.
- `sbus-rs 0.1.3` is external: pinned, `trust: external`, verified by the
  conformance suite. CRSF and BLHeli telemetry become standalone crates.
- Discontinuities: `embedded-io` has no concept of lost bytes. The serial reader
  already carries them out-of-band (a `DiscontinuityReader`), because `Read`
  alone cannot prove nothing was lost before a valid-looking frame. Each
  parser therefore has `push` and `reset`; the RC group observes the
  out-of-band handle and calls `reset`, discarding any partial frame. Turning
  discontinuities into read errors was considered and not adopted, in favour
  of that design.

The SBUS pipeline: UART → frame assembly → SBUS parser → Command Input →
safety master / control loop / actuator authority. Command Input is the first
fully owned boundary and the place for defensive validation (range,
staleness, failsafe corroboration). It mostly trusted its input, with unit
tests that had no requirement behind them; goal 2 writes that requirement.

---

## 13. The gyro-synced control loop

Before, `control_loop` was bound to the TIM4 interrupt at
`control_loop_rate_hz` and began with `acknowledge_control_tick` through a
`TimerTick` trait bound, the one place a core task called into hardware during
its own run. The timer ran independently of the IMU's data-ready, which caused
the 2026-10-09 problem: the ICM-42688-P runs about 1% fast against the 2 kHz
timer, the two beat every ~89 samples, and a tick finds no new sample.
Tolerance versus gyro-sync had been an open user decision since then.

| Option | Driven by | Trade-off |
|---|---|---|
| A. timer (before) | TIM4 tick | fixed rate; the beat needs a stale-tick tolerance (`IMU_TOLERATED_STALE_TICKS`), and the core keeps a hardware trait |
| B. gyro-synced (chosen) | each new `ImuSample` | no beat, no tolerance; rate follows the sensor clock, so it is ~1% off nominal |

Why B:
- `io-contracts` becomes pure message types: the tick trait was the core's
  last hardware trait;
- fail-closed by construction: if the IMU stops, no samples arrive, the loop
  sends no `MotorCommand`, and the output group's freshness check stops the
  motors. Stale IMU data cannot produce motor output;
- a control-loop heartbeat over a `state` link lets the safety master disarm
  and report "control loop not running". This is also the writer-liveness
  cover section 9 requires before safety-critical tasks read flags.

The user's condition was that Betaflight uses gyro-sync. Its scheduler locks
to the gyro's data-ready interrupt and runs the PID loop at gyro rate ÷
`pid_process_denom`; this is from knowledge of its source and is confirmed in
the Betaflight code at implementation.

**Rates.** Gyro-sync gives the sensor's output rate divided by an integer.
Per the datasheets as recalled, not yet checked:

| Sensor | Output rates | 500 Hz | 1 kHz | 2 kHz | 3 kHz | 8 kHz |
|---|---|---|---|---|---|---|
| MPU-6000 / 6500 | 8 kHz internal (low-pass off) ÷ (1 + `SMPLRT_DIV`): 8k, 4k, 2.67k, 2k, 1.6k, 1.33k, 1k, … | yes | yes | yes | no | yes |
| ICM-42688-P | 32k, 16k, 8k, 4k, 2k, 1k, 500, 200, … | yes | yes | yes | no | yes |

Betaflight has the same constraint, which is why it offers 2.67 kHz, not
3 kHz. Running the sensor faster than the loop (8 kHz sampling, every fourth
sample for 2 kHz) lets the samples in between be filtered at full rate. A
timer-driven loop with the sensor at least twice the loop rate would also
avoid stale ticks at any rate, with sample-age jitter of up to one sensor
period, at the cost of keeping the tick trait and an explicit stale check;
since 500 Hz to 8 kHz in the divider set suffice, gyro-sync was chosen.

**The rate rule** (the user's refinement): the configured rate is the
sensor's output rate ÷ N, at or below the board's highest timing-tested rate
(execution time does not depend on the rate, so every lower rate inherits the
timing evidence), and at or above a minimum the core declares for its filters
and control. Rate-dependent logic is unit-tested at every rate the sensor
offers: filter coefficients and PID `dt` from measured sample intervals, not
`control_loop_rate_hz`; freshness limits that scale; the RPM filter's Nyquist
limit at low rates (at 500 Hz nothing above 250 Hz can be notched, so filtering
happens at the sensor rate before decimation). At 8 kHz the whole loop must
finish within 125 µs on the F405; that is timing evidence, not something a
config value can promise.

Costs: a behaviour change to the flight path and loop rate (W6), so props-off
checks and a flight re-gate; and the loop's priority stays above the IMU
group's work, stated as a path requirement.

---

## 21. Motor speed feedback

Bidirectional DShot ([the public DShot chapter](../../mdbook/src/dshot.md#bidirectional-dshot)
describes the wire) gives each motor's eRPM on every frame. Three users of it
are agreed and host-tested, not yet wired into the control loop: the RPM
filter, arming from reply eRPM, and dynamic idle. Each changes W6 behaviour,
so each needs the props-off checks and a flight re-gate when it is enabled.

**The RPM filter** follows Betaflight's `flight/rpm_filter.c`,
`drivers/dshot.c` and `common/filter.c` at commit `41a5b43`, except that the
loop rate is fixed, so there is no cycle-time compensation:
- one state-variable notch per motor and harmonic, on all three axes of the
  unfiltered gyro, ahead of the gyro low-pass; its output is the input minus
  weight times 1/Q times the band-pass;
- a motor's frequency is eRPM x 100 / 60 / (poles / 2), smoothed by a 150 Hz
  PT1 at the loop rate; each notch sits at its harmonic of it, clamped between
  the minimum and 0.48 x the loop rate;
- a notch's weight rises from 0 at the minimum to 1 at the minimum plus the
  fade range, times the harmonic's weight, so a stopped motor's notches fade
  out;
- coefficients are refreshed round-robin, every notch within 1 ms, each
  computed once per motor and harmonic and shared by the three axes;
- defaults: 14 poles, 3 harmonics at 100 %, a 100 Hz minimum, a 50 Hz fade
  range, Q 5.0 and the 150 Hz frequency low-pass, all stored configuration.

An invalid reply is dropped and the last valid eRPM is held, with no age limit
in flight; reply loss neither disarms nor triggers failsafe. This departs from
W2's treatment of stale data, and is bounded by scope: stale eRPM moves gyro
filtering only, never motor authority. Arming still fails closed without fresh
eRPM, and dynamic idle bounds stale eRPM itself. The blackbox records each
motor's eRPM, its consecutive invalid replies and its invalid-reply total, so
a held notch can be found in analysis. The gyro low-pass corner is raised only
in steps, after logged raw gyro and eRPM show the notches remove the motor
noise. The filter's cost per loop is measured on target.

**Arming from reply eRPM.** Where bidirectional DShot runs, arming
qualification takes its eRPM from the replies, not from the UART ESC manager,
with every other rule of the guarded sequence unchanged (the stop dwell,
guarded idle and 10 ms rechecks, the 250 ms grace, three consecutive fresh
samples between 3,000 and 10,000 eRPM from every output, the 1.2 s abort, and
four stops on every abort). A reply's output comes from its lane, not from
request matching. A sample is fresh when decoded from the reply to a frame set
started during the qualification; consecutive samples come from consecutive
frame sets, and a dropped or invalid reply breaks the run.

Temperature, voltage and current come from extended DShot telemetry (EDT).
Before guarded idle on every arm attempt, with every motor stopped, each ESC is
sent command 13 (EDT enable) ten times under the special-command rules; an ESC
acknowledges with `0x0E00` and then interleaves temperature (degrees C),
voltage (0.25 V steps) and current (1 A steps) frames with its eRPM. An EDT
reply is not an eRPM sample: it neither counts toward a qualification run nor
breaks one, and the RPM filter keeps its last eRPM through it. EDT values are
observational; a missing acknowledgement never blocks arming. The Foxeer's
Reaper 55A runs BLHeli_32, bidirectional since 32.7 and EDT since 32.10; its
installed version and the EDT it sends are confirmed on target first.

A stored setting selects bidirectional DShot, with each board's default off
until that board has target evidence. With it off, the UART manager and the
current sequence apply unchanged. The UART manager is retired for temperature,
voltage and current only once EDT is confirmed on that board, which on the
Foxeer frees USART1 and DMA2 stream 5. The zero-eRPM fault-injection image is
re-pointed at the reply input.

**Dynamic idle** follows Betaflight's `flight/mixer.c` and `mixer_init.c` at
commit `41a5b43`. The mixer gets an explicit output floor: throttle and the
axis mix are spread between the floor and the maximum, and the axis
rescaling's room below is the throttle above the floor; Actuator Output's idle
clamp stays as the safety bound. A PID loop compares the slowest motor's
smoothed frequency with `dyn_idle_min_rpm` and raises the floor of all four
motors together, never one alone. The increase is bounded between zero and the
static idle until throttle is first raised after arming, and between zero and
`dyn_idle_max_increase` after that; the I-term rises fast and falls slowly
within the same cap, and the D-term acts on the smoothed frequency. With
dynamic idle on, the static idle offset is removed and throttle has a 1 %
minimum. Defaults: off (`dyn_idle_min_rpm` 0), P/I/D 50/50/50 and a 15 %
maximum increase, all stored configuration.

One departure from Betaflight: while any motor's eRPM is stale, the increase
holds and the I-term stops integrating, and if the data stays stale the
increase decays to the static idle. The hold and decay times (provisionally
100 ms and 500 ms) are set from logged reply-loss data. Stale eRPM therefore
never winds the floor up. Dynamic idle runs only while armed; guarded-idle
qualification keeps its fixed idle command. The blackbox records the
increase, its P, I and D terms and the stale-hold state. It is enabled only
after the RPM filter and bidirectional DShot have target evidence, and needs a
stale-reply fault-injection test.

**The Lucid.** The TBS Lucid H7's lanes are ordinary TIM3 and TIM5 channels,
which may capture replies on the same stream instead of sampling; that is
decided when the Lucid is brought level with the Foxeer.
