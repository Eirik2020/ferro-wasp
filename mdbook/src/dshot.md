# DShot

FerroWasp uses a four-motor DShot600 backend by default on **FerroWasp FCU3**.
It supports isolated capped bench images and the normal mixed-control path.
The PA10 legacy BLHeli telemetry service is active only with this default
DShot backend; it is inactive in the explicit PWM fallback.
The previous four-channel 400 Hz RC PWM backend remains an explicit fallback:

```powershell
cd apps/stm32f405-flight
cargo build --locked --no-default-features --features board-ferrowasp-fcu3
```

The capped bench image remains behind the existing equal-motor gate:

```powershell
cd apps/stm32f405-flight
cargo build --locked --features "dshot bench_equal_motors"
```

Physical selected-motor modes, multiple logical-motor selections, and PWM
calibration are compile-time errors in a DShot image. The base bench image
sends the same requested throttle to all four lanes. For props-off mapping
validation, exactly one `bench_logical_motorN_only` feature may be added.
Those modes retain the 250 PWM-style command cap and normal RC, arming,
freshness, lease, and actuator-output validation.

The normal controller and mixer use DShot in the default build:

```powershell
cargo build --release --locked
```

The former `dshot_mixed_control` feature remains a compatibility alias for
existing commands.

## FCU3 Routes

The external motor pads are unchanged. The DShot image selects alternate
timer functions that provide independent compare DMA requests:

| Physical output lane | External pad | DShot timer output | DMA route |
|---|---|---|---|
| 1 | PA8 | TIM1_CH1, AF1 | DMA2 Stream1 Channel6 |
| 2 | PC9 | TIM8_CH4, AF3 | DMA2 Stream7 Channel7 |
| 3 | PC8 | TIM8_CH3, AF3 | DMA2 Stream4 Channel7 |
| 4 | PB15 | TIM1_CH3N, AF1 | DMA2 Stream6 Channel6, CCR3 request |

Physical output 4 is a complementary advanced-timer output, but the ordinary TIM1_CH3 output
is disabled. RM0090 Table 96 therefore defines the pad as
`OC3N = OC3REF xor CC3NP`. The backend selects the PAC's active-high setting,
`CC3NP=0`, so PB15 follows the same active-high pulse reference as the ordinary
timer outputs. This still requires physical waveform validation.

All four DMA2 streams are separate from the active ADC1 and SPI1 streams. The
PWM fallback retains its existing routes and alternate functions.

## Waveform

A DShot packet contains 16 bits:

```text
[ 11-bit value ][ telemetry bit ][ 4-bit checksum ]
```

Values have these meanings:

```text
0       stop
1..47   special commands
48..2047 throttle
```

The actuator path emits only stop or mapped throttle values and does not expose
special commands. After the ESC boot window, its service sets the telemetry bit
on one lane at a time for legacy UART telemetry. Requests are limited to one
every 20 ms and a new request cannot overlap an outstanding response.

## BLHeli Legacy UART Telemetry

The default FCU3 DShot image receives the combined ESC telemetry wire on
**PA10 / USART1 RX AF7** at 115,200 baud, 8N1. USART1 RX uses DMA2 Stream 5
Channel 4; PA9 is not configured or claimed. The explicit PWM fallback does
not run the ESC manager or request legacy telemetry. A frame has ten bytes:
temperature, big-endian voltage/current, consumption, eRPM/100, and CRC-8. The
bounded parser validates CRC polynomial `0x07` and advances one byte after
failure to regain framing without a sync byte.

Frames contain no motor identifier. A low-priority ESC manager therefore
rotates physical DShot output lanes 1 through 4 and owns the single pending
association. It sends typed operations through a bounded SPSC request queue to
the DShot actuator service. The actuator service is the only consumer and
acknowledges the exact sequence/output request only after its telemetry bit is
present in a frame that actually started. A second bounded SPSC queue returns
that acknowledgement to the manager.

Because the actuator service can preempt the manager, a CRC-valid response may
already be complete while its request is still marked queued. The manager
quarantines at most that pending response and publishes it only after receiving
the matching sequence/output frame-start acknowledgement. A mismatched
acknowledgement cannot relabel the response. The manager distinguishes a
missing actuator acknowledgement from a missing ESC response and never
overlaps requests. An association timeout latches telemetry off until reboot,
so a late acknowledgement or wire frame cannot be assigned to a later output.
An unanswered response expires after 100 ms. The aggregate request limit is
50 Hz, or 12.5 Hz per ESC when all respond.

Telemetry identity is physical-output identity, not mixer motor order:

| Physical ESC output | Logical motor and location |
|---|---|
| 1 | M4, front-left |
| 2 | M3, rear-left |
| 3 | M1, rear-right |
| 4 | M2, front-right |

This path does not own motor peripherals or safety state. UART, DMA, parser,
queue, and timeout faults cannot grant authority. If they prevent the fresh
observations required during guarded idle, pre-arm qualification fails closed
and all four outputs return to stop. Once the system is armed, legacy telemetry
is currently observational: losing it does not itself request disarm.

The manager intentionally waits five seconds after boot before issuing its
first request. An arm attempt that reaches guarded idle during that window may
not collect enough samples and can fail closed at the 1.2-second qualification
deadline. After any arming abort, the arm switch must be moved low before a new
low-to-high arm request can qualify.

For a powered telemetry or arming test, power the ESCs before that first
post-delay request. If the FC remains on without ESC power, the request receives
no response and the association timeout latches telemetry off. Powering the
ESCs afterward is not enough; reboot the FC after ESC power is present.

### Powered props-off result

The PA10 legacy telemetry path passed a powered props-off run on 2026-07-20.
The retained log is
`logs/terminal_embed/20260720_185949_rtt.log` (SHA-256
`4CE58E3286F4CA2DF00FB47F47C8D5CE3379CCB1C12CA5871C05D0984C727692`).
Its final manager totals were 3,950 queued operations, 3,950 acknowledged frame
starts, and 3,950 valid UART responses, with zero acknowledgement/response
timeouts, mismatches, unsolicited frames, CRC failures, or discarded bytes.

All motors reported zero eRPM while stopped, approximately 6,500-7,200 eRPM at
DShot idle value `112`, and increasing eRPM with throttle. RC loss and explicit
disarm selected four zero DShot values; operator observation confirmed
immediate physical stop and telemetry reached zero on every motor by the next
two-second report. A report coincident with the stop can retain coast-down or
the latest sample from before that report boundary.

The installed ESCs returned zero for voltage, current, consumption, and
temperature while the independent FCU ADC reported 23.7 V. Those auxiliary
fields remain unsupported or unvalidated for this ESC configuration; this
checkpoint validates eRPM, framing, request/ack association, and stop behavior.

The checksum is:

```rust,ignore
(payload ^ (payload >> 4) ^ (payload >> 8)) & 0x0f
```

At the FCU3 advanced-timer clock of 168 MHz, DShot600 uses:

| Property | Timer ticks | Approximate time |
|---|---:|---:|
| Bit period | 280 | 1.667 us |
| Logical `0` high | 105 | 0.625 us |
| Logical `1` high | 210 | 1.250 us |

The packet, mapping, timing, and compare-sequence code is host-tested in
`ferrowasp-waveform::dshot`.

## Timer Synchronization

TIM1 owns physical output lanes 1 and 4. TIM8 owns lanes 2 and 3. Both use PWM mode 1 with compare
preload enabled.

TIM1 is the frame master:

```text
TIM1 CR2.MMS = Enable
TIM8 SMCR.TS = ITR0
TIM8 SMCR.SMS = Trigger mode
```

Before each frame set, the backend:

1. disables both timer counters and all four compare DMA requests;
2. enables and prepares all four DMA streams;
3. stages each first-bit duty and parks both counters at ARR;
4. enables all four compare DMA requests;
5. starts TIM1, whose CEN rising edge starts TIM8 through ITR0.

The internal trigger avoids four software-start operations. Logic-analyzer
evidence is still required before claiming a measured lane-to-lane phase
bound.

Each DMA buffer contains the remaining 15 bit duties followed by zero:

```text
bit N falls at its CCR match
    -> lane DMA writes bit N+1 into the CCR preload
    -> timer update loads it for the next bit
```

Each lane has a priority-16 completion IRQ. A frame set is complete only after
all four lanes report transfer completion. The timers then stop with every
compare at zero. A transfer error, direct-mode error, unexpected interrupt,
duplicate completion, or missing completion faults the complete bank and
forces every output low.

## Cadence And Leases

The priority-13 actuator service is the sole frame-set starter. It repeats the
current authorized four-lane vector every 2 ms, giving a fixed 500 Hz cadence.
Actuator commands only update the requested vector and its lease.

Nonzero commands use the normal 20 ms motor-command lease. If the
control/actuator path does not renew the lease, the service selects four stop
values and requests disarm.

The DShot-specific arming path does not use the legacy PWM low/idle sequence.
After the 200 ms arm-switch qualification, the actuator owner keeps four stop
values selected for a 100 ms pre-arm dwell and revalidates permission, RC
armability, arm-switch state, and low throttle every 10 ms. It then selects
the profiled idle value under the temporary arm permit while continuing those
guard checks. The ESC manager publishes timestamped telemetry observations
through a bounded queue; it does not command an actuator or change safety
state. The actuator owner requires three fresh samples from every motor in the
3,000-10,000 eRPM window. A missing/stalled motor times out after 1.2 seconds;
overspeed aborts after the 250 ms spin-up grace. Every abort selects four stop
values. Only successful qualification is reported to the safety master, which
performs its final guard check before setting `SYSTEM ARMED`. The PWM path
retains its existing timing and behavior.

A DShot600 frame lasts about 27 us. A frame set still in flight after 1 ms is
treated as a lost completion: both timers stop, all four DMA streams are
fenced, both advanced-timer output gates close, and the backend requests
disarm.

## Safety Ownership

The DShot peripheral remains inside the actuator boundary:

```text
control loop
    -> bounded MotorCmd queue
    -> actuator_output validation
    -> leased four-lane DShot request
    -> DShot actuator service
    -> TIM1/TIM8 and DMA2 hardware
```

No control, RC, USB, OSD, or experimental task owns a motor timer, pad, or
DShot DMA stream. Disarmed operation continuously transmits stop frame sets.
The four-motor image still requires ordinary RC qualification and the
boot/reconnect arm-low interlock.

## Standard Mixed Control

Status: **500 Hz runtime, powered props-off mixed control, all-axis correction,
qualified arming, RC-loss/recovery, and initial controlled flight have passed
their recorded checkpoints**. Electrical waveform/timing and measured stop
latency remain open.

The default DShot build activates the existing normal controller branch rather
than adding another mixer. At 400 Hz, that branch generates the physical
four-motor vector through the measured FCU3 map `[3, 4, 2, 1]`, publishes it
through the bounded `MotorCmd` queue, and wakes the sole actuator owner. The
owner drains to the newest command, rejects data older than 20 ms or any
non-finite value, applies the profiled DShot idle floor, and renews the
four-lane backend lease. The independent 500 Hz DShot service selects stop
and requests disarm if renewal ceases.

No pin, timer, DMA route, interrupt priority, motor map, RC policy, arming
state, or actuator authority changes between the bench and mixed images.
Unlike the bench images, the standard image has no 250-count command cap. Its
powered props-off stages were therefore completed before the controlled flight
reported below.

Build, flash, display RTT, and retain the exact default run from the repository
root:

```powershell
python tools\terminal_embed.py --release --locked
```

Require the unique startup identity:

```text
DShot600 standard motor output active
```

The pre-promotion source-verified release candidate built without
`blackbox_defmt` had
SHA-256
`757F918B29771F0D62E7EBCC14B6DCE4A840E0062626B0A21F8F3EA7C6453269`.
It retains `DMA2_STREAM1/4/6/7` at
`0x08004B44/0x08004F4C/0x08004FA4/0x08004FFC`; its vector table contains the
corresponding Thumb addresses, and loadable flash data ends at `0x08013968`.
Defmt metadata contains the standard DShot identity and stop-only preparation
messages but not the equal/unequal bench identities or legacy PWM
`Applying idle throttle` message.

### Stage 1: Unpowered Runtime

1. Disconnect ESC power and start with the RC arm switch low.
2. Require four DShot stop values for at least 10,000 frame sets.
3. Require equal lane counters, at most one set in flight, zero busy/expiry/
   timeout/fault counts, and continuously advancing IMU sequence. Use the
   appended `at <ms>` fields across at least ten reports to require an average
   service rate of 490-510 frame sets per second.
4. With the ESC bank deliberately unpowered, expect the manager's first
   post-five-second request to produce one ESC-response timeout and latch
   telemetry off. This is separate from the DShot backend timeout counter,
   which must remain zero.
5. Reset several times, including once with the transmitter arm toggle high.
   No reset or link recovery may request arming until a fresh low-to-high arm
   transition is observed.
6. Before a later powered stage, apply ESC power and reboot the FC so the
   telemetry latch starts clear.

The first retained unpowered capture,
`logs/terminal_embed/20260719_172718_rtt.log`, used the earlier candidate with
SHA-256
`3EDC1D7767B119D9124874D96316EAEF16AF8EC6A950BBC926E4747CB7E66DAF`.
It reached 67,000 started frame sets. All 67 reports contained four zeros,
identical lane and completed-set counters, exactly one set in flight, and zero
busy, expiry, timeout, and fault counts. IMU sequence advanced monotonically
from 0 through 160,160 across 101 reports, with no warning, error, panic,
arming, or safety event.

That capture also exposed persistent roughly 333 Hz frame-set service:
`Mono::delay(2.millis())` uses RTIC's minimum-duration relative delay, which
adds one 1 ms SysTick. The service now computes an absolute deadline from each
release and uses `delay_until`, preserving the intended two-tick/500 Hz
cadence. The corrected image also prints its monotonic report time.

The promoted standard image passed the corrected unpowered cadence checkpoint
on 2026-07-20. Retained log
`logs/terminal_embed/20260720_181201_rtt.log` used firmware SHA-256
`5B91A09C509138D4762BBA0F9DC295A591BA474EF7587269435BC4BDBED84979`;
the log itself has SHA-256
`388901C486004D11F51CDB8EFC7BA77B70CF406AE83ECE0E510E27F77AE3774A`.
It reached 12,000 started frame sets in 23,999 ms. Across all 12 DShot reports,
the requested vector remained `[0, 0, 0, 0]`, all four lane counters equaled
the completed-set count, exactly one set was in flight, and busy, expiry,
timeout, and fault counters remained zero. Every 1,000-set interval occupied
exactly 2,000 ms, establishing 500 frame sets per second across the retained
22-second measurement span. IMU sequence advanced from 0 through 19,220 with
no warning, error, panic, arm request, or armed state.

This clears the corrected cadence/runtime portion of Stage 1. The operator
subsequently confirmed that interrupting power or reflashing while the
controller arm signal remained high stopped the motors and did not rearm them
after reboot; a fresh low-to-high arm transition was required. That restart
result is operator-observed rather than a single uninterrupted RTT reset trace.

### Stage 2: Powered Props-Off Mixer

1. Remove every propeller, inspect all four motor/ESC assemblies, start at
   zero throttle, and confirm the airframe is restrained.
2. Arm and require the 100 ms stop dwell, then observe all four motors start
   at idle value `112` during qualification. Require three fresh samples per
   motor between 3,000 and 10,000 eRPM before `SYSTEM ARMED`.
3. Apply only modest throttle. Confirm all four values respond and remain
   bounded, lane counters stay synchronized, and backend faults remain zero.
4. Use small, brief stick steps and verify the physical DShot vectors:
   roll-right raises physical left outputs 1/2 relative to right outputs 3/4;
   pitch-forward raises rear outputs 2/3 relative to front outputs 1/4; and
   yaw-right separates diagonals 2/4 from 1/3.
5. With sticks centered, tilt the restrained frame briefly about roll and
   pitch. Corrections must oppose the motion. Stop on reinforcing response,
   severe oscillation, heat, smell, roughness, or unexpected current.
6. Disarm from idle and from a nonzero command. Both must select four zeros
   and physically stop every motor.

### Stage 3: Active-Command RC Loss

1. Rearm, hold a modest nonzero mixed command, and confirm unequal DShot
   values are updating.
2. Interrupt the DJI O4/RC3 link by powering off Goggles 3.
3. Require one RC frame-timeout invalidation, prompt physical stop, and
   sustained `[0, 0, 0, 0]` values with healthy lane counters.
4. Restore the link with arm still high. Healthy-frame qualification must not
   request arming or restart a motor.
5. Move arm low for at least one second, then high at zero throttle. Require
   the complete guarded arming sequence before output resumes.
6. Explicitly disarm and retain RTT from before link loss through final stop.

RTT proves state and requested values but cannot measure electrical timing or
physical stop latency. Those remain instrumented checkpoints.

### Standard-Image Powered Result

Status on 2026-07-20: **arming, idle, throttle, explicit disarm,
active-command RC-loss/recovery, and roll/pitch/yaw stick and hand-motion
direction checks passed props-off**.

Retained log `logs/terminal_embed/20260720_181535_rtt.log` used the same
standard firmware SHA-256 as the unpowered run:
`5B91A09C509138D4762BBA0F9DC295A591BA474EF7587269435BC4BDBED84979`.
The log has SHA-256
`2E174B54D67FEDCC0B01C3D7155F3A604ECD6964DC80DDE0995AD6DBE501B4D3`.

The first arm retained the required ordering: 100 ms of stop frames,
`DShot pre-arm complete; motor outputs remain stopped`, `SYSTEM ARMED`, then
six reports at idle `[112, 112, 112, 112]`. The operator confirmed that all
motors idled only while armed.

During later armed intervals, modest throttle produced mixed DShot vectors
including `[148, 148, 149, 149]`, `[152, 152, 152, 151]`, and
`[143, 143, 144, 143]`. The operator confirmed that all motors followed
throttle. No claim about roll, pitch, yaw, or motion-opposing correction is
made from this run because those direction steps were not reported.

RC frame timeout during both idle and active mixed output selected sustained
`[0, 0, 0, 0]`. Link recovery with the controller arm state still high did
not emit an arm request or restart a motor. A fresh disarm-then-arm command
completed the guarded stop dwell and restored idle. The final explicit disarm
reported `SYSTEM DISARMED`, selected four zeros, and physically stopped the
motors by operator observation.

Across all 69 DShot reports through 69,000 started frame sets, every lane
counter equaled the completed-set count, exactly one set was in flight, and
busy, expiry, timeout, and fault counters remained zero. IMU sequence advanced
through 110,511. The operator observed immediate physical stops on RC loss and
explicit disarm; RTT's two-second status cadence cannot establish measured
stop latency.

### Initial Controlled Flight Result

On 2026-07-20 the operator reported that the first controlled experimental
flight on the standard DShot image went well, supported strong manoeuvres, and
did not reproduce the previous unwanted yawing. Pitch authority felt lower
than desired. No new BB2 flight capture accompanies this observation, so it
does not quantify rate tracking, saturation, timing, or control margin. The
parked tuning follow-up is to change pitch P alone from `0.25` to `0.30` in a
controlled test after checking motor headroom.

This result establishes an initial interoperability/flight checkpoint; it does
not make the implementation routinely flight-ready, waveform-validated,
airworthy, or production-ready.

## DShot Arming And Idle Policy

Status: **telemetry-qualified powered props-off and physical-output-1 negative
fault-injection checkpoints passed**.

The FCU3 DShot profile owns these explicit settings:

| Setting | Initial value | Meaning |
|---|---:|---|
| `prearm_stop_hold_ms` | 100 ms | Guard recheck window with all lanes stopped |
| `idle_throttle_command` | 65 | Qualification and armed-idle input, mapped to DShot value `112` |
| `idle_qualification_min_erpm_div100` | 30 | A motor must report at least 3,000 eRPM |
| `idle_qualification_max_erpm_div100` | 100 | A motor must not exceed 10,000 eRPM |
| `idle_qualification_spinup_grace_ms` | 250 ms | Ignore startup transients before judging RPM |
| `idle_qualification_timeout_ms` | 1,200 ms | Stop and abort if all motors do not qualify |
| `idle_qualification_max_sample_age_ms` | 200 ms | Reject stale observations |
| `idle_qualification_consecutive_samples` | 3 | Required fresh in-range samples per motor |

Command `65` preserves the value that previously spun all four motors in the
powered props-off test. It is intentionally not changed without new target
evidence. The profile caps any idle-tuning value at the existing 250-count
bench limit. The shared active-output validator accepts the protocol-specific
idle floor without changing the legacy PWM idle.

The actuator owner applies configured idle during qualification using a short,
renewed command lease. The system is still logically disarmed at this point.
It selects stop immediately if the arm permit, RC link, arm switch, or
low-throttle guard becomes invalid. Logical-motor and unequal-vector images
still qualify all four ESCs before their post-arm bench behavior begins.

The ESC manager does not begin requests until five seconds after boot. An arm
attempt during that interval can therefore enter guarded idle, receive no
qualifying samples, stop at the 1.2-second deadline, and remain disarmed. Move
the arm switch low and make a fresh arm request after telemetry is available.
If the FC has already issued a request while the ESC bank was unpowered, its
response timeout latches telemetry off; power the ESCs and reboot the FC before
retrying.

Use this props-off checkpoint before any prop-on work:

1. Inspect every motor and ESC, remove all propellers, and flash
   `dshot bench_equal_motors`.
2. At startup, require
   `DShot arming profile: 100 ms stop dwell, idle command 65 -> value 112`.
3. Arm at zero throttle. Require this ordering:
   `RC Requests ARM!`, `Attempting DShot safety arming`,
   `Preparing DShot actuators with 100 ms of stop frames`,
   `DShot pre-arm stop complete; qualifying idle eRPM 3000..10000 with 3 samples/physical output`,
   all four motors starting at idle,
   `DShot idle eRPM qualified; physical outputs 1/2/3/4 samples [3, 3, 3, 3]`, then
   `SYSTEM ARMED`.
4. Require no movement during the stop dwell. During qualification, require
   all four motors to start and run smoothly at DShot value `112`; no motor
   may be missing, stalled, or above 10,000 eRPM.
5. Explicitly disarm. Require prompt physical stop, sustained four zero values,
   equal lane counters, and zero busy, expiry, timeout, and fault counts.
6. Optional failure injection, still props-off: disconnect only the telemetry
   signal and repeat arming. Motors may idle for at most the bounded
   qualification interval, then all four must stop, no `SYSTEM ARMED` line may
   appear, and RTT must report the telemetry-timeout arming abort.

For a repeatable physical-output-1 negative test without obstructing a motor or
changing the real DShot vector, use the compile-time fault image:

```powershell
python tools\terminal_embed.py --release --locked --features bench_dshot_idle_output1_not_running
```

The old `bench_dshot_idle_motor1_not_running` spelling remains a compatibility
alias; it does not mean logical M1.

This image still commands idle to all four motors, but substitutes zero eRPM
for physical output 1, which is logical M4/front-left, only at the
actuator-owned qualification input. Require the loud `FAULT INJECTION ACTIVE`
startup warning. After at most 1.2 seconds of idle, RTT must identify physical
output 1 (logical M4) with `0 of 3 samples`, all four DShot lanes must return to
zero, and `SYSTEM ARMED` must never appear. Disarm before returning to the
standard image; never use this build with propellers installed.

The checkpoint passed on 2026-07-20 using fault-image SHA-256
`1CEFE0E47030C2AECB3BC1BB15244A65E3608F14D79FF0D16D408C54AC175B80`.
Retained log `logs/terminal_embed/20260720_193148_rtt.log`, SHA-256
`7FAFC3A7F9AD85DA4546525D76C91113F16D2D4AA01AD57BD3730867D34DD477`,
contains three `[0, 12, 12, 12]` timeouts and three explicit `ESC1` failures,
where that dated log label means physical output 1 / logical M4, with no
`SYSTEM ARMED`. The operator observed motor stop after the configured
1.2-second interval. Each following DShot report selected four zeros; telemetry
returned to zero as the motors stopped. All 1,450 telemetry requests started
and decoded without a reported transport/parser error.

The standard image subsequently passed powered props-off coexistence with
`blackbox_defmt`. Logs `20260720_194720_rtt.log` and
`20260720_194906_rtt.log` retained 15,047 and 13,053 contiguous 400 Hz BB2
samples respectively, while DShot lanes remained synchronized and the ESC
manager completed 1,550/1,550 and 1,350/1,350 valid telemetry transactions
without reported errors. Both runs qualified all four idle RPMs, armed, and
disarmed to stop. Zero-command hand motion produced opposing roll, pitch, and
yaw correction. Roll/pitch stick mixing was captured with expected signs;
yaw-stick input remained zero and was covered by the subsequent run below.
Terminal display saturation did not create a gap in either retained file.

The yaw observation subsequently passed in
`logs/terminal_embed/20260720_195451_rtt.log`. Yaw stick command covered
`-1000..+1000 dps`; the physical motor diagonals followed yaw-controller
output with the expected sign. During the following zero-stick hand rotation,
the controller output tracked negative measured yaw and the yaw-diagonal
differential opposed the motion. All 13,565 BB2 samples were contiguous and
fresh, 1,350/1,350 ESC telemetry transactions were valid without reported
errors, and explicit disarm selected four stop values. This closes all three
props-off stick-mixing and motion-opposition axes for the standard image.

If any motor does not start reliably at `112`, use the equal-motor image and
raise RC throttle slowly to identify the lowest value at which all four motors
run smoothly. Record the DShot value and conditions; change
`idle_throttle_command` only in small reviewed increments, then repeat cold
starts and the full props-off check. Do not tune with propellers installed or
infer a final margin from one run.

The unequal-vector image additionally requires idle command `<= 80`, its
smallest fixed command. A higher tuned idle deliberately makes that image fail
to compile until its vector and expected evidence are reviewed; it must never
be silently clamped into a different packet test.

The source-verified props-off equal-motor candidate has SHA-256
`34CB9BFB9225AC9AE3B9771F4647F52FABE9176D392ABC54DE50BCC0AE764807`.
Its four expected DMA2 IRQ symbols are unchanged, and its defmt metadata
contains the new stop-dwell ordering without the legacy PWM idle messages.

The 2026-07-19 powered props-off run retained the stop-dwell and
pre-arm-complete messages before `SYSTEM ARMED`, followed by four values
`112`. The operator reported all four motors running. Explicit disarm restored
sustained four-lane zeros through at least 25,000 starts. Every lane counter
remained equal with one frame set in flight, IMU sampling continued, and busy,
expiry, timeout, and fault counts remained zero.

The operator subsequently confirmed that every motor remained physically
stopped until `SYSTEM ARMED`, all four idled at value `112`, and motors ran
only while armed. Command `65` / DShot value `112` is therefore retained as
the current FCU3 prototype idle without a BSP tuning change. Continued
cold-start and temperature-margin testing remains necessary before treating
the initial flight result as broad readiness.

## Unsafe Boundary

The application remains `#![deny(unsafe_code)]`. Project-local unsafe code is
isolated in `ferrowasp-stm32f4::dshot`.

The only unsafe source declarations are two documented trait-implementation
templates, expanded for four private DMA endpoints. They prove:

- each private endpoint addresses the compare register of an exclusively
  owned TIM1 or TIM8;
- timer compare DMA writes are 16-bit halfwords;
- each fixed STM32F405 stream/channel mapping is valid in the
  memory-to-peripheral direction.

Endpoint constructors are private. HAL `Transfer` values own active buffers,
DMA addresses, stream fencing, and buffer exchange. There is no project
`static mut`, manual DMA buffer address, or DMA-active buffer aliasing.

## Four-Motor Bench Test

Propellers must remain removed. The operator inspected the motor associated
with an earlier smoke report on 2026-07-20 and reports that it appears normal,
closing that inspection blocker. Continue to stop immediately on heat, smell,
roughness, jitter, or abnormal current.

First flash with actuator power disconnected:

```powershell
cargo embed --release --features "dshot bench_equal_motors"
```

Expected RTT begins with:

```text
DShot600 four-motor equal-throttle bench backend active
DShot values [0, 0, 0, 0], sets <completed>/<started>, lanes [<m1>, <m2>, <m3>, <m4>], busy 0, expired 0, timeouts 0, faults 0
```

For at least 10,000 frame sets:

- all four lane counts must be equal;
- each lane count must equal completed frame sets;
- started minus completed must be zero or one;
- busy, expiry, timeout, and fault counts must remain zero;
- no spurious-DMA warning may appear;
- IMU and heartbeat activity must continue.

After any waveform-affecting correction, repeat at least 2,000 unpowered frame
sets with the same counter and fault requirements. Only then:

1. inspect all four motor/ESC assemblies, especially the previously suspect
   pair;
2. power the ESCs with propellers still removed and RC throttle at zero;
3. arm and confirm all four motors decode the guarded idle phase;
4. disarm immediately if any motor fails to idle, and do not raise throttle;
5. only after all four idle normally, rearm and apply a small throttle increase;
6. confirm all four motors follow the same capped command;
7. disarm and confirm every motor returns to stop.

Zero RC throttle is clamped to the validated armed-idle command by the active
actuator path. All four motors should therefore continue idling after
`SYSTEM ARMED` until an explicit disarm or safety fault selects stop.

Without a logic analyzer, the test establishes ESC interoperability only. It
does not establish electrical pulse widths, jitter, physical-output-4 polarity margins, or a
measured synchronization bound.

## Historical Physical-Output-1 Evidence

The earlier `dshot bench_motor1_only` image validated the PA8/TIM1_CH1 lane on
2026-07-18. It reached at least 12,000 unpowered frame starts and 10,000
powered props-off frame starts with zero busy, lease-expiry, timeout, or fault
counts. M1 followed requested values `112`, `177`, `167`, and `219`, then
returned to stop on disarm.

Here `M1` was the backend's historical name for physical output lane 1 on PA8,
not current Betaflight logical motor 1. That evidence remains valid for lane 1
and the guarded lease behavior. It does not validate the TIM8 lanes,
physical-output-4 complementary polarity, or four-lane synchronization.

## Four-Motor Unpowered Evidence

The release four-motor image passed its first FCU3 target checkpoint on
2026-07-18 with ESC power disconnected. It reached at least 12,000 frame
starts. Every reported snapshot showed:

- `completed = started - 1`;
- four equal lane counters matching completed frame sets;
- zero busy, lease-expiry, timeout, and fault counts;
- no spurious-DMA warning;
- continuing IMU progress through at least sequence 28,829.

This validates concurrent four-lane DMA completion and runtime stability. It
does not validate ESC decoding, motor behavior, electrical timing, polarity
margins, or measured synchronization.

## First Four-Motor Powered Attempt

The first powered props-off attempt on 2026-07-18 did not pass. Physical output
lanes 1, 2, and 3 spun, but lane 4/front-right on PB15 did not. All four requested values
and DMA completion counters remained equal through explicit disarm; busy,
lease-expiry, timeout, and fault counters stayed zero.

The tested image had `CC3NP=1`, selecting active-low for TIM1_CH3N. Since
TIM1_CH3 was disabled, this inverted only the physical-output-4 waveform despite its healthy
DMA accounting. That image had SHA-256
`5FE1AE6883E6448A89541731CA3F61F5758E065B9BA2CBAD32F4E81380F702F2`.
Source now selects active-high `CC3NP=0`; the corrected release ELF has
SHA-256
`AA3A5A3D96B0B97BD1FA99031A4AA6FCD2A0A37650B8F4AEAA8121BFBF5FF13C`.

## Corrected Four-Motor Powered Evidence

The corrected-image powered props-off checkpoint passed on 2026-07-18. The
operator skipped the separate short unpowered regression and proceeded
directly to the motor test. The powered capture nevertheless showed
synchronized stop-frame accounting through 3,000 starts before arming.

All four motors, including physical output 4/front-right, spun and responded to equal capped
RC throttle. Reported values progressed from armed idle `112` through `123`,
`129`, and `158`, returned to `112`, and selected four zeros on explicit
disarm. At 10,000 frame starts:

- `completed = started - 1`;
- all four lane counters equaled the completed count;
- busy, lease-expiry, timeout, and fault counters remained zero;
- IMU sampling continued through at least sequence 24,024.

This establishes four-lane ESC interoperability, corrected physical-output-4 polarity at the
ESC, guarded arming, equal-throttle response, and explicit disarm behavior. It
does not establish electrical pulse widths, polarity margin, jitter,
TIM1/TIM8 phase alignment, motor direction, mixed-command motor ordering, or
flight readiness.

## Props-Off RC-Loss Test

Status: **functional target checkpoint passed on 2026-07-18**. This test
validates that RC loss revokes actuator permission, selects DShot stop on all
four lanes, and cannot cause an automatic rearm when the link returns. It does
not measure the physical stop latency.

The current policy declares the RC link stale after 100,000 us without a
healthy frame and requires three consecutive healthy frames for recovery.

### Target Evidence

The operator reported that the powered props-off RC-loss/recovery procedure
completed as expected. The retained RTT excerpt begins after the timeout
invalidation and initial transition to stop, so those two events were not
captured directly. The retained evidence does show:

- prolonged four-lane stop values `[0, 0, 0, 0]` through at least frame-set
  starts 44,000 to 47,000, with no automatic arm request;
- equal per-lane completion counters and zero busy, lease-expiry, timeout, and
  fault counts throughout;
- continuing IMU progress through sequence 126,527;
- a later fresh `RC Requests ARM!` followed by the normal stop-frame guard,
  four equal idle values `112`, `BLHeli ESCs idling`, and `SYSTEM ARMED`;
- explicit `RC Requests Disarm!` and `SYSTEM DISARMED`, followed by continued
  four-lane stop values through at least 52,000 frame-set starts.

This closes the functional RC-loss, no-automatic-rearm, manual recovery, and
continued-runtime gate. The exact stop latency and a retained event-by-event
trace from timeout through the first stop frame remain unavailable; measuring
latency still requires timestamped instrumentation or a logic analyzer.

### Preconditions

1. Remove all propellers.
2. Inspect each motor and ESC for damage, abnormal smell, or unexpected heat.
3. Flash the exact `dshot bench_equal_motors` image under test.
4. Start with RC throttle at zero and the arm switch low.
5. Wait for `RC link valid after healthy-frame qualification`.
6. Confirm four equal zero DShot values, synchronized lane counters, and zero
   busy, lease-expiry, timeout, and fault counts.

### Procedure

1. Move the arm switch high and wait for all four motors to enter idle and for
   `SYSTEM ARMED`.
2. Keep throttle at zero and leave the arm switch high.
3. Power off the DJI Goggles 3 to interrupt the O4/RC3 SBUS path without
   removing FCU or ESC power.
4. Require one `RC link invalidated by frame timeout` warning. Confirm all four
   motors stop and the next DShot status reports values `[0, 0, 0, 0]`.
5. Leave the RC link absent for at least two DShot status reports. Values must
   remain zero, lane counters must remain equal, backend fault counters must
   remain zero, and IMU sequence numbers must continue advancing.
6. Restore Goggles 3 power while leaving the arm switch high. After
   `RC link valid after healthy-frame qualification`, require no
   `RC Requests ARM!`, no motor movement, and continued DShot stop values.
7. Move the arm switch low and hold it there for at least one second.
8. Move the arm switch high and verify that the normal guarded arming sequence
   can run again. Confirm all four motors idle, then explicitly disarm.

Remove ESC power immediately if any motor continues running after link
invalidation, restarts during arm-high reconnection, or accelerates without a
new command. RTT status cadence is too slow to establish stop latency; that
requires separate timestamped or logic-analyzer evidence.

## Logical-Motor Identification

Status: **logical-to-physical identity and rotation direction passed on
2026-07-20**. The prerequisite equal-motor
RC-loss/recovery checkpoint passed on 2026-07-18. This stage validates the
committed logical-to-physical map without enabling full mixed control.

For each logical motor, flash one image:

```powershell
cargo embed --release --features "dshot bench_equal_motors bench_logical_motor1_only"
cargo embed --release --features "dshot bench_equal_motors bench_logical_motor2_only"
cargo embed --release --features "dshot bench_equal_motors bench_logical_motor3_only"
cargo embed --release --features "dshot bench_equal_motors bench_logical_motor4_only"
```

The 2026-07-18 release candidates built from commit `c4eeb90` plus the current
working-tree changes are:

| Logical motor | ELF SHA-256 |
|---|---|
| 1 | `A3E569EDFADC5E64DED1A1AC4147610E540A6F56B337262672AF4D5B33D9BA9A` |
| 2 | `8626339FEDDAD7A7EB9CFA606E669A354443AFCD96DF115419C4983510AFE152` |
| 3 | `373D40848C86BE9FBEE2360E97E728C088B1E164588979DBDFFFA99EDF6C0C04` |
| 4 | `1BE803EF05BF3B5694400538F903FC3BD46C3554E37CB9222870136F3607C65D` |

All four retain `DMA2_STREAM1/4/6/7` at
`0x080048F4/0x08004CFC/0x08004D54/0x08004DAC` and end their loadable flash
image at `0x080130D8`. These identifiers describe source-side candidates; each
becomes target evidence only after its startup identity and physical behavior
are observed.

Use this procedure for each image:

1. Remove all propellers and start with throttle zero and arm low.
2. Confirm the startup message identifies the intended logical motor.
3. Arm normally. All four motors may idle during the guarded arming sequence.
4. After `SYSTEM ARMED`, confirm zero throttle returns all four lanes to stop.
5. Apply only a small throttle increase. Exactly one motor must spin.
6. Record its physical output, airframe location, and rotation direction.
7. Disarm and require four zero DShot values with equal lane counters and zero
   backend faults.

The committed map predicts:

| Logical motor | Physical output | Expected location | Observed location | Expected DShot vector |
|---|---:|---|---|---|
| 1 | 3 | rear-right | rear-right, pass | `[0, 0, value, 0]` |
| 2 | 4 | front-right | front-right, pass | `[0, 0, 0, value]` |
| 3 | 2 | rear-left | rear-left, pass | `[0, value, 0, 0]` |
| 4 | 1 | front-left | front-left, pass | `[value, 0, 0, 0]` |

The operator ran each exact feature command above and reported that only the
expected airframe motor spun. Follow-up target observations and retained logs
then supplied CW/CCW direction, synchronized completion counters, explicit
stop after disarm, and zero backend/telemetry error evidence as summarized
below.

Stop at the first mismatch. Do not edit the motor map or ESC direction as part
of the observation run; record the result and review it separately.

The current FCU3 checkpoint passed with these observed results:

| Logical motor | Physical lane | Location | Rotation viewed from above |
|---|---:|---|---|
| 1 | 3 | rear-right | CW |
| 2 | 4 | front-right | CCW |
| 3 | 2 | rear-left | CCW |
| 4 | 1 | front-left | CW |

Retained logs `20260720_200200_rtt.log`, `20260720_200253_rtt.log`,
`20260720_200358_rtt.log`, and `20260720_200501_rtt.log` each showed only the
selected physical lane at DShot value `112`, synchronized lane counters, zero
backend/telemetry errors, and four-zero output after disarm. The intervening
`20260720_200141_rtt.log` was only a failed probe-open attempt.

## Capped Unequal-Vector Validation

Status: **Parts A and B passed functionally on target with retained RTT
evidence**. This stage proves that all four lanes can carry different packets
in the same synchronized frame set. It does not enable the PID mixer or
authorize flight output.

Build and flash:

```powershell
cargo embed --release --features "dshot bench_equal_motors bench_dshot_unequal_motors"
```

The 2026-07-18 source-side release candidate built from commit `c4eeb90` plus
the current working-tree changes has ELF SHA-256
`78FD890B9D93DCA8D1A456548F0C89DB6153561496C1BB42B8D42238676DABF3`.
It retains `DMA2_STREAM1/4/6/7` at
`0x080048F4/0x08004CFC/0x08004D54/0x08004DAC` and ends its loadable flash
image at `0x080130E8`. This is not target evidence until flashed and observed.
The runtime result below validates the behavior, but the flashed ELF hash was
not independently read back from the target.

The fresh candidate rebuilt after the DShot-specific arming change on
2026-07-19 has SHA-256
`07A44529265B9895818F57C0B5CD608E35A9E6C95CF381373BE1372B60C24F1D`.
It retains the same four DMA IRQ addresses, ends its loadable flash data at
`0x08012CC8`, and contains defmt metadata for unequal-vector mode and the
stop-only arming sequence.

The feature cannot be combined with a logical-motor selection, physical
selected-motor mode, or PWM calibration. It requires both `dshot` and
`bench_equal_motors`. The normal RC qualification, guarded arming, fresh
`MotorCmd` queue, 20 ms output lease, actuator owner, and whole-bank fault
containment remain active.

Throttle below 100 command counts selects four stop values. At or above the
trigger, the reusable bench task emits:

| Representation | Index 1 | Index 2 | Index 3 | Index 4 |
|---|---:|---:|---:|---:|
| Logical command | 140 | 120 | 100 | 80 |
| Physical command after `[3, 4, 2, 1]` remap | 80 | 100 | 140 | 120 |
| Expected DShot value | 127 | 147 | 187 | 167 |

All command values remain below the established 250-count bench cap.

### Part A: Unequal Packets And Stop Transitions

1. Remove all propellers and inspect every motor and ESC.
2. Start with throttle zero and arm low. Confirm the unequal-vector startup
   message and four stop values.
3. Arm normally. All four motors may idle during the guarded arming sequence.
4. After `SYSTEM ARMED`, keep throttle below the trigger and require
   `[0, 0, 0, 0]`.
5. Raise throttle just above the trigger. Require DShot values
   `[127, 147, 187, 167]` and confirm all four motors run. Different unloaded
   motor speeds are supporting evidence, not a strict pass criterion.
6. Hold the vector for at least five status reports. Require equal advancing
   lane counters, at most one frame set in flight, zero busy/expiry/timeout/
   fault counts, and continuing IMU progress.
7. Lower throttle below the trigger. Require all motors to stop and the next
   status to report four zeros. Repeat the command/stop transition three
   times.
8. Explicitly disarm and require continued four-lane stop.

Stop immediately on any vector mismatch, lane-counter divergence, warning, or
motor that does not stop.

Target result on 2026-07-18: **functional pass**. The retained RTT excerpt
preserves four consecutive active reports from sets `46999/47000` through
`49999/50000`, each with exact physical DShot values
`[127, 147, 187, 167]`. Every lane counter matched the completed-set count,
there was exactly one frame set in flight, and busy, expiry, timeout, and fault
counts remained zero. IMU sequence advanced from `105706` through `124925`.
Explicit disarm was followed by sustained `[0, 0, 0, 0]` reports at
`50999/51000` and `51999/52000`.

The operator reports completing all of Part A, including the five-report hold
and three command-to-stop transitions. The retained excerpt itself contains
four consecutive active reports and the final explicit-disarm transition, so
the additional hold and repeated transitions remain operator-observed rather
than retained RTT evidence. This test does not establish physical RPM ordering
or stop latency.

### Part B: RC Loss During A Nonzero Vector

Run this only after Part A and the DShot-specific arming checkpoint pass.

From the repository root, build, flash, display decoded RTT, and retain a
clean timestamped log with:

```powershell
python tools\terminal_embed.py --release --locked --features "dshot bench_equal_motors bench_dshot_unequal_motors"
```

The wrapper uses `probe-rs run`, shows decoded `defmt` lines in the terminal,
and flushes each line to
`logs\terminal_embed\YYYYMMDD_HHMMSS_rtt.log`. Output already received remains
available after terminal scrollback overflows or the run is stopped with
`Ctrl+C`. To inspect the newest capture:

```powershell
$log = Get-ChildItem logs\terminal_embed\*_rtt.log | Sort-Object LastWriteTime -Descending | Select-Object -First 1
Get-Content -LiteralPath $log.FullName -Tail 200
```

1. Rearm at zero throttle, cross the trigger, and confirm the exact unequal
   vector.
2. While the vector is active, power off DJI Goggles 3 to interrupt the
   O4/RC3 SBUS path without removing FCU or ESC power.
3. Require one frame-timeout invalidation, prompt physical stop, and four
   DShot zero values with equal counters and zero backend faults.
4. With the link still absent and arm still high, lower RC throttle to zero.
   Leave the link absent for at least two status reports while IMU sequence
   continues advancing.
5. Restore the link with arm still high and throttle zero. Require healthy
   link qualification but no automatic arm request or motor restart. Require
   at least two more four-zero status reports.
6. Move arm low for at least one second, then high at zero throttle, and verify
   normal guarded rearming remains possible. Explicitly disarm.
7. Retain RTT from one unequal-vector report before link loss through recovered
   stop, fresh rearm, and final disarm. The expected RC frame-timeout warning
   is distinct from the DShot backend timeout counter, which must remain zero.

RTT cadence cannot measure physical stop latency; that remains an instrumented
checkpoint.

Target result on 2026-07-19: **pass with retained RTT**. The clean capture is
`logs/terminal_embed/20260719_164203_rtt.log`, produced by the exact
release-feature command above. Its ELF SHA-256 matches the recorded candidate:
`07A44529265B9895818F57C0B5CD608E35A9E6C95CF381373BE1372B60C24F1D`.

Before link loss, sets `3999/4000` and `4999/5000` carried the exact unequal
vector `[127, 147, 187, 167]`. The frame-timeout warning was followed by four
zeros at `5999/6000`; zeros then persisted through sets `6999/7000` to
`9999/10000`. Link qualification was followed by two more zero reports at
`10999/11000` and `11999/12000` without an arm request or restart. Additional
link timeout/requalification cycles also remained stopped through
`19999/20000`.

A fresh low-to-high request completed the guarded 100 ms stop dwell, remained
zero at `20999/21000`, and restored the exact vector at `21999/22000`.
Explicit disarm returned to four zeros at `22999/23000`. Across all 23 DShot
status reports, every lane matched the completed-set count, exactly one set
was in flight, and busy, expiry, backend-timeout, and fault counters were
zero. IMU sequence advanced from 1 through 56,056. RTT cadence still cannot
measure physical stop latency.

## Deferred Work

- logic-analyzer timing, jitter, polarity, and synchronization evidence;
- instrumented RC-loss and explicit-disarm stop latency;
- continued cold-start, temperature-margin, and controlled flight evidence;
- a retained BB2 flight capture for quantitative rate tracking and saturation;
- making IMU initialization, calibration, and freshness an explicit pre-arm
  prerequisite; the present first post-arm stale-IMU check disarms instead;
- bidirectional DShot telemetry;
- DShot special-command policy;
- a reviewed pitch-authority tuning progression.

The Foxeer F405 V2 backend remains conventional RC PWM. FCU3 timer and DMA
routes must not be assumed valid for that board.
