# FerroWasp Active Work Handoff

Last updated: 2026-07-20

## Current State - 2026-07-20

The standard FCU3 DShot600 image completed its first controlled outdoor flight
after the powered props-off, telemetry-qualified arming, motor-map, rotation,
RC-loss, reset/rearm-interlock, and extended-runtime checkpoints below. The
operator reports that the aircraft flew well, could perform strong maneuvers,
and no longer showed the previous unwanted yawing. This is pilot-observed
flight evidence; no new BB2 flight capture was supplied with the report.

Pitch response felt weaker than desired. Keep the validated baseline recorded
as pitch P/I/D `0.25 / 0.0 / 0.0`. The next isolated tuning experiment is pitch
P `0.30`, leaving roll, yaw, I, D, filtering, rate scaling, mixer, and output
limits unchanged. If command tracking improves without oscillation,
bounce-back, excessive noise, or motor heating, a later `0.35` trial may be
considered. Check BB2 command-versus-gyro tracking and motor headroom before
attributing all remaining authority loss to P gain. The current OSD PID step is
`0.1`, so it is too coarse for the intended first `0.05` increment.

The flight is a successful experimental prototype checkpoint. It does not
close the outstanding logic-analyzer measurements for DShot pulse width,
jitter, M4 polarity margin, or TIM1/TIM8 phase alignment, and it is not an
airworthiness or production-readiness claim.

One current arming gap must remain explicit for publication: IMU
initialization, gyro-bias calibration, and freshness are not pre-arm
prerequisites. A stale IMU can pass idle-eRPM qualification and reach
`SYSTEM ARMED`; the first post-arm stale-IMU control check then requests
disarm. Requiring healthy, calibrated, fresh IMU state before granting the
temporary preparation permit remains future safety work.

The agreed work order is now:

1. prepare and sanitize FerroWasp for public source publication;
2. bring up and target-verify FerroWasp on the Foxeer F405 V2 while retaining
   its current arming inhibit until its hardware gates pass;
3. establish a reproducible WSL/Docker development environment;
4. return to isolated FCU3 pitch tuning with the `0.30` P trial.

The remaining sections are a chronological engineering handoff. Statements
such as “PWM is the default” describe the dated checkpoint in which they were
written and are not current configuration claims; use this section and
`mdbook/src/current_support.md` for the live baseline.

## Refactor Branch Note - 2026-07-17

The unified STM32F4 RTIC 2 I/O and DMA refactor is being staged on
`refactor/hardware`.

Current branch rules:

- preserve the live FCU dev-board pin map;
- keep motor outputs inside the existing safety-gated actuator path;
- keep the four-lane DShot path inside the safety-owned actuator boundary;
- retain four-channel PWM as an explicit fallback while standard DShot target
  validation is completed;
- record architecture decisions in `project_docs/ARCHITECTURE_DECISIONS.md`;
- use `project_docs/UNIFIED_IO_DMA_HANDOFF.md` for the work-package handoff.

## FerroWasp FCU3 BSP Checkpoint - 2026-07-18

The active board is now the explicit BSP target
`ferrowasp-bsp::stm32f4::ferrowasp_fcu3`, named **FerroWasp FCU3**. Its typed
manifest freezes the validated USART2, UART4, SPI1, ADC1, motor PWM, debug LED,
DMA, IRQ, and timer assignments. SWD and optional USB pins are recorded as
reservations.

Board aliases, safe RTIC DMA-storage shaping, pin conversion, and device
construction live in the FCU3 BSP. Reusable chip-family mechanisms remain in
`ferrowasp-stm32f4`, which no longer depends on the BSP. The transitional
`src/stm32f4` adapter is removed. The F405 RTIC shell now lives in the isolated
`apps/stm32f405-flight` package.

Static verification passes for normal debug/release firmware, SPI timeout and
stale motor-command injection builds, calibration and representative motor
bench features, USB, 148 host tests plus doc tests, embedded and host Clippy,
mdBook, formatting, and diff checks. A normal target smoke test was also
required because initialization and static DMA-storage ownership moved.

The normal-build target smoke test passed on 2026-07-18. The user reported the
system appeared to be working properly after flashing the explicit FCU3 BSP
build. This clears the immediate runtime checkpoint for the initialization and
DMA-storage relocation.

## Historical FCU3 Single-Motor DShot600 Checkpoint

An opt-in FCU3 DShot600 backend now owns physical motor output 1 only:
PA8/TIM1_CH1 through DMA2 Stream1 Channel6. The external pin map is unchanged.
The normal four-channel 400 Hz RC PWM image remains the default.

The DShot image requires the exact feature pair
`dshot bench_motor1_only`. Other selected-motor modes, equal-motor mode, and
PWM calibration are compile-time incompatible. PC9, PC8, and PB15 are GPIO
outputs held low in this image. The existing physical-M1 bench path still
requires normal RC qualification and arming and remains capped at 250 command
counts.

The priority-13 actuator service is the sole frame starter and sends one frame
every 2 ms. Actuator commands only update the authorized value and lease, so
the cadence remains 500 Hz. Disarmed output uses continuous stop frames.
Nonzero values carry a 20 ms lease; expiry selects stop and requests disarm.
The existing guarded arming sequence sends stop for 2.5 seconds. During its
500 ms M1 idle hold, it rechecks permission, RC link, arm switch, and throttle
every 10 ms before renewing the normal 20 ms lease. DMA2 Stream1 completion
runs at priority 16, stops TIM1 after the final falling edge, loads CCR1 zero,
and leaves PA8 low. Transfer/direct-mode errors fault the backend, force the
line low, and cause disarm. An interrupt without a recognized completion/error
flag is also treated as a backend fault.

Frame startup keeps active CCR1 at zero, stages the first duty in the preload,
and starts CNT from ARR. The first timer overflow therefore creates the first
rising edge in hardware instead of exposing a software-latency-dependent high
level while CEN is clear.

The service also imposes a 1 ms in-flight deadline on each approximately
27 us frame. A lost completion interrupt therefore stops TIM1/DMA, latches a
fault, and requests disarm instead of leaving the backend busy indefinitely.

Project-local unsafe code is isolated in `ferrowasp-stm32f4::dshot`. It
documents the owned TIM1 CCR1 address and halfword width, the fixed
DMA2 Stream1 Channel6 route, and bounded PAC writes. DMA buffer exchange and
fencing remain owned by the HAL `Transfer`; there is no project `static mut`
or DMA-active buffer aliasing.

Static packet, checksum, DShot600 timing, compare-sequence, route, default PWM,
and DShot feature builds pass. The final verification includes 195 host tests,
strict host/ARM Clippy, default and DShot release builds, mdBook, formatting,
and feature-conflict guards. The optimized DShot image binds IRQ 57 to
`DMA2_STREAM1` (`0x0800464C`, vector word `0x0800464D`). Its ELF SHA-256 is
`8388A10B45376B5D26CC570812FFB2F8F969F91500791B0AC258501F5D7908DA`;
loadable flash ends at `0x08011D20`.

The initial unpowered FCU3 target checkpoint passed on 2026-07-18. The
reported run reached at least 12,000 DShot frame starts at the intended 500 Hz
service rate. Every snapshot had exactly one frame in flight
(`completed = started - 1`), with zero busy, lease-expiry, timeout, and fault
counts. IMU sequence numbers continued advancing and no DShot warning or
runtime crash was reported.

The first powered props-off M1 attempt on 2026-07-18 showed that the ESC
decoded idle frames and spun the motor. The original one-shot 520 ms lease
then expired before the asynchronous 500 ms hold returned. The DShot service
selected stop frames and the priority-16 safety master disarmed, demonstrating
the intended containment path. The implementation now renews the normal 20 ms
lease every 10 ms only after a successful arming-guard check; host coverage
also verifies renewal and subsequent stalled-renewal expiry.

The corrected-image powered arming-idle checkpoint then passed on 2026-07-18.
M1 value `112` spun the motor during the guarded idle hold, the sequence
reached `BLHeli ESCs idling` and `SYSTEM ARMED`, and zero RC throttle then
selected value `0` as designed by `bench_motor1_only`. Frame completion
continued with zero busy, lease-expiry, timeout, and fault counts.

The final powered props-off run completed physical-M1 interoperability. The
motor followed RC throttle as expected through requested DShot values `112`,
`177`, `167`, and `219`, returned to value `0` on explicit disarm, and reached
10,000 frame starts with zero busy, lease-expiry, timeout, and fault counts
while IMU sampling continued.

Logic-analyzer pulse timing remains open. The props-off result validates ESC
interoperability but does not establish electrical pulse widths or jitter.

This checkpoint has now been superseded by the four-motor bench backend below.
Its physical-M1 evidence remains useful as lane-1 ESC interoperability
evidence. Bidirectional telemetry, special-command policy, and flight use
remain deferred.

## FCU3 Four-Motor DShot600 Powered Bench Checkpoint

The FCU3 DShot backend now owns all four existing external motor pins without
changing the board connector map:

| Motor | Pin and timer output | DMA route |
|---|---|---|
| M1 | PA8 / TIM1_CH1 | DMA2 Stream1 Channel6 |
| M2 | PC9 / TIM8_CH4 | DMA2 Stream7 Channel7 |
| M3 | PC8 / TIM8_CH3 | DMA2 Stream4 Channel7 |
| M4 | PB15 / TIM1_CH3N | DMA2 Stream6 Channel6 |

The default image remains four-channel 400 Hz RC PWM. Every current four-lane
DShot image requires the base pair `dshot bench_equal_motors`; one logical
selection or the fixed unequal-vector feature may be added, but not combined.
Physical selected-motor modes and PWM calibration are compile-time
incompatible. Every bench path still goes through the safety-owned actuator
task, requires qualified RC and the normal arming sequence, and remains below
the 250-count cap.

One `DshotMotorBank` owns TIM1, TIM8, all four pins, four DMA transfers, and
their static buffers. TIM1 emits its CEN signal as TRGO and TIM8 uses ITR0
trigger mode, so TIM1 starts both timer domains in hardware after all four DMA
streams have been enabled. M4 uses the complementary TIM1_CH3N output with
`CC3NP=0` active-high polarity, so with the ordinary TIM1_CH3 output disabled
the ESC pad follows `OC3REF`, matching the pulse convention of M1-M3.

The priority-13 service is the sole frame-set starter and keeps the established
500 Hz cadence and 20 ms nonzero-command lease. Four priority-16 DMA handlers
account for lane completion. A frame set is complete only after all four lanes
finish. A transfer/direct-mode error, unexpected or duplicate interrupt, or
1 ms completion timeout faults the entire bank, disables all DMA requests,
closes both advanced-timer MOE gates, drives every timer output to its
configured low idle state, and requests disarm.

Project-local unsafe code remains isolated in
`ferrowasp-stm32f4::dshot`. Each private endpoint implementation documents its
fixed timer compare address, halfword transfer width, DMA stream/channel route,
and exclusive ownership assumptions. The RTIC app remains
`#![deny(unsafe_code)]`; HAL transfers retain DMA buffer ownership and fencing.

Final static verification passes 196 host tests plus doc tests, DShot-feature
tests, strict host/default/DShot Clippy, default PWM and four-motor DShot
release builds, all four formatting scopes, mdBook, feature-conflict guards, the
unsafe-source scan, and diff checks. The optimized image binds
`DMA2_STREAM1`, `DMA2_STREAM4`, `DMA2_STREAM6`, and `DMA2_STREAM7` at
`0x080048F4`, `0x08004CFC`, `0x08004D54`, and `0x08004DAC`. Its ELF SHA-256 is
`AA3A5A3D96B0B97BD1FA99031A4AA6FCD2A0A37650B8F4AEAA8121BFBF5FF13C`;
the loadable flash image ends at `0x08012EC0`. Target validation is
deliberately staged:

1. With the ESC power disconnected, run at least 10,000 frame sets and require
   all four lane counters to advance together with zero busy, lease-expiry,
   timeout, or fault counts while IMU/control activity continues.
2. With props removed and ESC power connected, verify guarded arming, equal
   idle spin on all four motors, equal capped response to RC throttle, explicit
   disarm to zero, and continued zero backend faults.
3. Treat motor numbering, direction, waveform timing/jitter, bidirectional
   telemetry, special commands, and flight use as separate open checkpoints.

Stage 1 passed on the unpowered FCU3 target on 2026-07-18. The release image
reached at least 12,000 frame starts. Every reported snapshot had exactly one
set in flight, all four lane completion counters were identical to the
completed-set count, and busy, lease-expiry, timeout, and fault counts remained
zero. No spurious-DMA warning was reported. IMU sequence numbers advanced from
at least 3,204 through 28,829 during the capture, providing concurrent runtime
progress evidence. This cleared the original unpowered gate but was not ESC
interoperability or waveform evidence.

The first Stage 2 powered attempt on 2026-07-18 did not pass. M1, M2, and M3
spun, but physical M4/front-right on PB15 did not. The requested DShot values
were equal on all four lanes, all completion counters stayed equal through
explicit disarm, and busy, lease-expiry, timeout, and fault counts remained
zero. This isolates the failure beyond command generation and DMA completion.
That attempt used the pre-correction ELF SHA-256
`5FE1AE6883E6448A89541731CA3F61F5758E065B9BA2CBAD32F4E81380F702F2`.

The static audit found that the tested image set `CC3NP=1`, which RM0090
defines as active-low. Because TIM1_CH3 is disabled and only TIM1_CH3N is
enabled, RM0090 Table 96 gives `OC3N = OC3REF xor CC3NP`; the old image
therefore inverted only M4. The backend now uses the PAC's typed
`active_high()` selection (`CC3NP=0`). The same latent polarity mistake was
corrected in the still-arming-inhibited Foxeer RC PWM bank; the Foxeer
correction remains without target evidence.

The corrected FCU3 Stage 2 powered props-off checkpoint passed on 2026-07-18.
The operator intentionally skipped the separate 2,000-frame unpowered repeat
and proceeded directly to the motor test. Before arming, the powered capture
showed synchronized stop-frame accounting through 3,000 starts. All four
motors, including physical M4/front-right, then spun and responded equally to
RC throttle. Reported values progressed from armed idle `112` through `123`,
`129`, and `158`, returned to `112`, and selected four zeros on explicit
disarm. The run reached 10,000 frame starts with `completed = started - 1`,
identical per-lane counters, and zero busy, lease-expiry, timeout, and fault
counts. IMU sequence numbers continued from at least 4,805 through 24,024.

This establishes corrected M4 polarity interoperability at the ESC and
four-lane equal-throttle behavior through the safety-owned actuator path. It
does not establish electrical pulse widths, polarity margin, jitter,
TIM1/TIM8 phase alignment, motor direction, mixed-command motor ordering, or
flight readiness. Physical inspection of the previously suspect motor/ESC pair
remains required before each further powered run.

The powered props-off DShot RC-loss/recovery checkpoint passed functionally on
2026-07-18. The operator reported that link loss stopped all four motors, link
recovery while arm remained high did not automatically rearm, and only a fresh
low-to-high arm sequence entered the normal guarded arming flow. The retained
RTT excerpt starts after timeout invalidation and the initial stop transition,
so it does not preserve those two events or establish stop latency. It does
show prolonged `[0, 0, 0, 0]` values through at least starts 44,000 to 47,000,
equal lane counters, zero backend counters, and continuing IMU progress. A
later fresh arm request produced four idle values `112` and `SYSTEM ARMED`;
explicit disarm restored four zeros through at least 52,000 starts. IMU
sequence advanced through 126,527. Exact stop latency remains a separate
instrumented measurement.

The logical-to-physical identity stage passed on target on 2026-07-18. DShot
still requires `bench_equal_motors`, but may additionally accept exactly one
`bench_logical_motorN_only` feature. The existing control path
remaps that logical selection through committed `MOTOR_OUTPUT_MAP = [3, 4, 2,
1]`, retains the 250-count cap, and publishes it through the same fresh
`MotorCmd` queue and safety-owned actuator task. All four lanes remain one
synchronized fault domain. Physical selected-motor modes, multiple logical
selections, PWM calibration, uncapped mixer output, and default DShot remain
compile-time excluded. The operator ran the exact logical-motor 1/2/3/4
feature commands and observed rear-right/front-right/rear-left/front-left
respectively, confirming physical outputs 3/4/2/1 and committed map
`[3, 4, 2, 1]`. Rotation directions were subsequently verified on target on
2026-07-20 as M1 rear-right CW, M2 front-right CCW, M3 rear-left CCW, and M4
front-left CW; retained per-run RTT diagnostics are recorded below.

The equal-motor RC-loss candidate built from `c4eeb90` plus the current
working-tree changes has ELF SHA-256
`F36B3D1C9B468FBC4999CC9771A0E9E72F4DA2A6913F11B9B0F0684782E97224`.
It retains `DMA2_STREAM1`, `DMA2_STREAM4`, `DMA2_STREAM6`, and
`DMA2_STREAM7` at `0x080048F4`, `0x08004CFC`, `0x08004D54`, and
`0x08004DAC`; loadable flash ends at `0x08012FB0`. Static verification passes
201 reusable host tests plus doc tests, DShot-feature tests, strict host and
embedded workspace Clippy, strict default/equal/logical F405 checks, default,
logical, and equal release builds, all four formatting scopes, mdBook, YAML,
feature-conflict, and diff checks. This artifact remains candidate evidence
for source identification; the functional target observations are recorded
above, with the retained-log limitation stated explicitly.

Four logical-motor release candidates were rebuilt and identified after the
RC-loss gate passed:

| Logical motor | ELF SHA-256 |
|---|---|
| 1 | `A3E569EDFADC5E64DED1A1AC4147610E540A6F56B337262672AF4D5B33D9BA9A` |
| 2 | `8626339FEDDAD7A7EB9CFA606E669A354443AFCD96DF115419C4983510AFE152` |
| 3 | `373D40848C86BE9FBEE2360E97E728C088B1E164588979DBDFFFA99EDF6C0C04` |
| 4 | `1BE803EF05BF3B5694400538F903FC3BD46C3554E37CB9222870136F3607C65D` |

They were built from `c4eeb90` plus the current working-tree changes. Each
retains DShot IRQ symbols `DMA2_STREAM1/4/6/7` at
`0x080048F4/0x08004CFC/0x08004D54/0x08004DAC` and has loadable flash end
`0x080130D8`. They are ready for sequential props-off target identification;
the operator's command-by-command report now supplies physical motor-order
evidence for all four, including CW/CCW direction. The retained logs also
confirm per-run backend counters; their hashes are recorded in the DShot book
checkpoint.

The next bounded mixed-packet stage is code-ready behind
`bench_dshot_unequal_motors`. It does not run the PID mixer. Throttle below
100 command counts publishes four stops; at or above the trigger, reusable
task logic publishes fixed logical commands `[140, 120, 100, 80]`. The
validated `[3, 4, 2, 1]` remap produces physical commands
`[80, 100, 140, 120]`, which map to exact DShot values
`[127, 147, 187, 167]`. Host tests pin both transformations.

The image retains qualified RC, guarded arming, the fresh bounded `MotorCmd`
queue, armed-only actuator ownership, the 20 ms nonzero-command lease, the
500 Hz service, and whole-bank fault containment. Unequal mode without DShot,
DShot without the equal-motor base gate, logical plus unequal selection, and
PWM calibration all fail at compile time. Target validation is staged:

1. hold the exact unequal vector for at least five status reports and perform
   three clean vector-to-stop transitions;
2. only after that passes, interrupt RC while the unequal vector is active and
   require stop, sustained zeros, no automatic arm-high restart, and fresh
   low-to-high recovery.

Props remain removed. Rotation-direction recording, waveform instrumentation,
and full mixer output remain separate gates.

The unequal-vector source-side release candidate built from `c4eeb90` plus the
current working-tree changes has ELF SHA-256
`78FD890B9D93DCA8D1A456548F0C89DB6153561496C1BB42B8D42238676DABF3`.
It retains DShot IRQ symbols `DMA2_STREAM1/4/6/7` at
`0x080048F4/0x08004CFC/0x08004D54/0x08004DAC` and has loadable flash end
`0x080130E8`. Static verification passes 203 reusable host tests plus doc
tests, strict host and embedded workspace Clippy, strict default/equal/all
logical/unequal/unequal-blackbox F405 checks, default/equal/unequal release
builds, and all intended feature-conflict guards. This candidate remains
source-identified; the flashed ELF hash was not independently read back from
the target.

Unequal-vector Part A passed functionally on target on 2026-07-18. The
operator reports completing the full five-report hold, three clean
command-to-stop transitions, and explicit disarm. The retained RTT excerpt
directly preserves four consecutive exact `[127, 147, 187, 167]` reports from
sets `46999/47000` through `49999/50000`. Every lane matched the completed-set
count, exactly one frame set remained in flight, all backend fault counters
were zero, and IMU sequence advanced from `105706` through `124925`. Explicit
disarm returned to sustained `[0, 0, 0, 0]` at `50999/51000` and
`51999/52000`. The fifth active report and other repeated transitions are
operator-observed but absent from the retained excerpt. Physical RPM ordering
and exact stop latency were not measured.

Unequal-vector Part B passed functionally on target on 2026-07-19. The
operator reports that active-command RC loss stopped the motors, stop remained
selected through link absence and arm-high recovery, no automatic restart
occurred, a fresh low-to-high arm transition restored normal guarded arming,
and final disarm passed.

The local `tools/terminal_embed.py` logger now accepts `--release`, `--locked`,
and `--features`, builds the selected F405 image, runs it through `probe-rs`,
strips host ANSI sequences, and flushes clean decoded output to
`logs/terminal_embed` line by line.

The repeat capture
`logs/terminal_embed/20260719_164203_rtt.log` closes the Part B RTT evidence
gap. Exact unequal values `[127, 147, 187, 167]` appeared at `3999/4000` and
`4999/5000`; frame-timeout invalidation was followed by zeros at `5999/6000`.
Zeros persisted through initial recovery, additional link flaps, and
`19999/20000` without automatic rearm. A fresh guarded arm remained zero at
`20999/21000`, restored the exact vector at `21999/22000`, and explicit
disarm returned to zeros at `22999/23000`. All 23 DShot reports had equal lane
counters, one set in flight, and zero busy, expiry, backend-timeout, and fault
counts. IMU sequence advanced from 1 through 56,056. The release ELF hash
exactly matched the recorded candidate. Physical stop latency remains
unmeasured because RTT status cadence is insufficient for that evidence.

The fresh post-arming-change Part B release ELF built on 2026-07-19 has
SHA-256
`07A44529265B9895818F57C0B5CD608E35A9E6C95CF381373BE1372B60C24F1D`.
It retains DShot IRQ symbols `DMA2_STREAM1/4/6/7` at
`0x080048F4/0x08004CFC/0x08004D54/0x08004DAC`, ends its loadable flash data at
`0x08012CC8`, and contains defmt metadata for both unequal-vector mode and the
new stop-only arming sequence.

## FCU3 DShot-Specific Arming And Idle Checkpoint

The FCU3 DShot branch no longer runs the legacy 2.5-second PWM low hold and
500 ms pre-armed idle spin. After the existing 200 ms arm-switch
qualification, the safety-owned actuator task explicitly selects four DShot
stop values for a profiled 100 ms dwell. It rechecks actuator permission, RC
armability, arm-switch state, and throttle every 10 ms, reports preparation
complete while outputs remain stopped, and leaves the final guard and
`SYSTEM ARMED` transition to the priority-16 safety master. Only a later
armed-only control command can request a nonzero DShot value.

The normal FCU3 PWM build and Foxeer PWM app retain their existing 2.5-second
low and 500 ms idle sequence. No pin, timer, DMA route, motor map, interrupt
priority, lease, or actuator-ownership boundary changed.

`DshotFourMotorProfile` now carries `prearm_stop_hold_ms = 100` and
`idle_throttle_command = 65`. The latter maps to DShot value `112`, preserving
the value already shown to spin all four motors. It is a separate DShot
setting and no longer depends on changing the shared PWM idle. A compile-time
cap keeps it at or below the existing 250-count bench limit. Unequal-vector
builds additionally require idle `<= 80`, preventing a tune from silently
clamping the smallest fixed packet command. The reusable active-output
validator accepts a protocol-specific idle floor and rejects non-finite or
invalid profile values.

This checkpoint is source-verified and has initial powered props-off target
evidence. The retained 2026-07-19 run shows:

1. four stop commands through the 100 ms dwell and pre-arm completion;
2. `SYSTEM ARMED` before the first reported four-value `112` frame set;
3. all four motors running at value `112`;
4. explicit disarm followed by sustained zeros through at least 25,000 starts;
5. synchronized lanes with `completed = started - 1`, advancing IMU sequence,
   and zero busy, expiry, timeout, or fault counters.

The operator subsequently confirmed that the motors ran only while armed and
that all four idled at value `112`. This closes the physical pre-arm-stop
observation and accepts command `65` / DShot value `112` as the current FCU3
bench idle without a BSP tuning change. The retained excerpt does not contain
the startup profile line or independently identify the flashed ELF hash, but
those evidence gaps do not block unequal-vector Part B. Repeat cold-start
margin testing before any future flight/default promotion.

Static verification passes:

- all 205 host tests plus doc tests;
- strict workspace host and embedded Clippy;
- strict FCU3 default, equal-motor DShot, and unequal-vector DShot Clippy;
- all four logical-motor checks, blackbox combinations, and default/equal/
  unequal release builds;
- all formatting scopes, mdBook, the added-unsafe scan, and diff checks.

The final props-off equal-motor release candidate is
`apps/stm32f405-flight/target/thumbv7em-none-eabihf/release/FerroWasp`, with
SHA-256
`34CB9BFB9225AC9AE3B9771F4647F52FABE9176D392ABC54DE50BCC0AE764807`.
It retains the four expected DMA2 IRQ symbols at `0x080048F4`, `0x08004CFC`,
`0x08004D54`, and `0x08004DAC`, and its loadable flash image ends at
`0x08012BA0`. Its defmt metadata contains the new DShot profile, stop-dwell,
and pre-arm-complete messages and contains neither legacy PWM idle message.

## FCU3 Standard DShot Mixed Control

The FCU3 default now exposes the normal controller and Quad X mixer through
DShot. The former `dshot_mixed_control` feature remains a compatibility alias.
Existing capped bench commands are unchanged, and four-channel PWM remains an
explicit `--no-default-features --features board-ferrowasp-fcu3` fallback.

The candidate reuses the established path rather than adding another motor
authority:

```text
400 Hz PID/mixer
    -> measured FCU3 physical map [3, 4, 2, 1]
    -> bounded fresh MotorCmd queue
    -> armed-only actuator validation
    -> 20 ms DShot command lease
    -> synchronized 500 Hz four-lane service
```

The BSP's four-lane profile is named `DshotFourMotorProfile` because the same
unchanged pins, timer synchronization, DMA routes, 100 ms stop dwell, and idle
command `65` apply to both bench and normal mixed control. DShot is the
declared default output profile.

Source verification passes standard mixed DShot, explicit PWM fallback,
equal-motor, logical-motor, and unequal-vector checks. Host tests pin
physical-vector DShot mapping including stop, bounds, saturation, and lane
order.

The pre-promotion clean release candidate without `blackbox_defmt` had SHA-256
`757F918B29771F0D62E7EBCC14B6DCE4A840E0062626B0A21F8F3EA7C6453269`.
It retains `DMA2_STREAM1/4/6/7` at
`0x08004B44/0x08004F4C/0x08004FA4/0x08004FFC`, with matching Thumb vector
entries, and loadable flash data ends at `0x08013968`. Its defmt metadata
contains the mixed-candidate and stop-only preparation identities while
excluding equal/unequal bench and legacy PWM-idle identities.

The first retained unpowered target capture,
`logs/terminal_embed/20260719_172718_rtt.log`, exactly matched the earlier
candidate hash
`3EDC1D7767B119D9124874D96316EAEF16AF8EC6A950BBC926E4747CB7E66DAF`.
It reached 67,000 started sets. Every one of 67 DShot reports contained four
zeros, equal per-lane and completed counters, exactly one set in flight, and
zero busy, lease-expiry, timeout, and fault counts. All 101 IMU reports were
monotonic from sequence 0 through 160,160. There was no warning, error, panic,
arm request, armed state, or other safety event.

The same capture exposed a service-cadence defect before powered testing.
Relative RTIC timer-queue delays add one SysTick to guarantee the requested
minimum duration, so the 1 kHz monotonic made `delay(2 ms)` a persistent 3 ms,
roughly 333 Hz loop. The service now sets an absolute two-tick deadline from
each release and uses `delay_until`; periodic status appends `at <ms>` so the
corrected frame-set rate can be measured without inferring it from IMU data.
The corrected source passes strict mixed-control Clippy, all 206 host tests,
PWM fallback and equal-motor DShot release checks, formatting, and diff checks.

The promoted standard image passed corrected unpowered runtime on 2026-07-20.
Retained log `logs/terminal_embed/20260720_181201_rtt.log` used firmware
SHA-256
`5B91A09C509138D4762BBA0F9DC295A591BA474EF7587269435BC4BDBED84979`
and reached 12,000 started frame sets in 23,999 ms. All 12 reports retained
four stop values, equal lane/completed counters, exactly one in-flight set,
and zero busy, expiry, timeout, and fault counters. Every 1,000-set interval
took exactly 2,000 ms, proving the corrected 500 Hz service across the
22-second measurement span. IMU sequence advanced from 0 through 19,220 with
no warning, error, panic, arm request, or armed state.

Target status before the powered run below: corrected 500 Hz unpowered runtime
passed; reset/boot-high checks remained pending.

The standard image passed the powered props-off arming, idle, throttle,
explicit-disarm, and active-command RC-loss/recovery checkpoints on
2026-07-20. Retained log
`logs/terminal_embed/20260720_181535_rtt.log` used the same firmware SHA-256
`5B91A09C509138D4762BBA0F9DC295A591BA474EF7587269435BC4BDBED84979`;
the log SHA-256 is
`2E174B54D67FEDCC0B01C3D7155F3A604ECD6964DC80DDE0995AD6DBE501B4D3`.

Stop-only preparation preceded each arm, all motors idled at DShot value `112`
only after `SYSTEM ARMED`, and modest throttle produced live mixed vectors.
RC timeout during active output selected sustained four-zero output. Arm-high
link recovery did not restart a motor; a fresh disarm-then-arm command
completed the guarded dwell and restored idle. Explicit disarm selected four
zeros. The operator confirmed immediate physical stop on RC loss and disarm
and that all motors followed throttle.

All 69 retained DShot reports through 69,000 starts had equal lane/completed
counters, exactly one in-flight set, and zero busy, expiry, timeout, and fault
counters while IMU sequence advanced through 110,511. RTT cannot measure the
reported physical stop latency. Cold boot/reset with arm high and waveform
timing/phase remain open before a short flight hop; stick/tilt directions and
motor rotation direction are now closed.

The operator then tested interruption while the controller arm signal remained
high on 2026-07-20. In `logs/terminal_embed/20260720_200838_rtt.log` (SHA-256
`A57B817C5969E9C46775BF805C1F00CBB878B805C25B391141CAD8ECA0BFE0AA`), motors
were already armed and producing live mixed DShot values before the probe
session ended with an ST-Link `SwdApError`/`SwdDpError`; no automatic restart
was observed. The subsequent retained sessions
`20260720_200945_rtt.log` (SHA-256
`1BE1B389D36753695A98BF2E983528E84C1DDA0BFA7AD4C47005765872B63023`) and
`20260720_201007_rtt.log` (SHA-256
`F61E8B4F17B24B2E752B43DE6D36DB047DCF352718C87623C9AA53847127F3E3`) each
contain exactly one `Begin system init` and start with four-zero output. The
latter also records an explicit disarm and four-zero output. These captures
support the required stop behavior and held-arm rearm inhibition, but do not
by themselves provide a clean second-boot trace after battery removal or
flashing; repeat that interlock check with the RTT session surviving the reset
before flight.

An extended props-off blackbox run then covered disarmed, armed/idle, throttle,
disarmed, and a second arm/disarm cycle. Retained log
`logs/terminal_embed/20260720_201625_rtt.log` (SHA-256
`E017596D88099EF316C0C8BBE3236A3F98F4ADAC140D9C85E1CB66CDA30FFA16`) contains
52,087 contiguous BB2 samples (`seq 1..52087`) with no sequence gaps. The
blackbox flags show disarmed `2 -> armed 3 -> disarmed 2 -> armed 3 ->
disarmed 2`; throttle spans `0..191`, with 10,836 frames containing nonzero
motor commands. Both arm requests reached `SYSTEM ARMED`, both disarm requests
reached `SYSTEM DISARMED`, and all 65 DShot status reports retain equal lane
counters with zero busy, expiry, timeout, and fault counts. This closes the
extended bench endurance checkpoint; only the controlled outdoor hop remains.

## FCU3 BLHeli Legacy UART Telemetry Candidate

The default DShot FCU3 image now owns PA10 as RX-only USART1 AF7 and DMA2
Stream 5 Channel 4 for BLHeli/KISS-style legacy ESC telemetry at 115,200 baud.
PA9 remains unconfigured. The explicit PWM fallback does not run the ESC
manager or request legacy telemetry. The fixed-storage parser validates
ten-byte frames with CRC-8 polynomial `0x07`, decodes temperature, voltage,
current, consumption, and eRPM/100, and uses byte-slip recovery after a CRC
failure.

Because the wire frame carries no motor identifier, a dedicated low-priority
ESC manager owns parsing, per-output samples, rotation, cadence, request
sequence, response association, and timeouts. It sends typed operations
through a bounded SPSC request queue. The DShot actuator service is the sole
consumer and returns a second bounded-queue acknowledgement only after the
selected telemetry bit was included in a frame that actually started.

A CRC-valid response can arrive while its request is still queued because the
higher-priority actuator task may not yet have had its acknowledgement drained.
The manager quarantines that response and publishes it only after the exact
sequence/output acknowledgement. It distinguishes actuator-ack timeout from
ESC-response timeout and rejects mismatched acknowledgements. Either
association timeout latches telemetry off until reboot, preventing a late
response from being attributed to the next output.

Requests rotate physical outputs 1 through 4 after the five-second ESC boot
window, are limited to 50 Hz aggregate, and cannot overlap; an unanswered
response expires after 100 ms. The physical/logical identity is output 1 =
M4/front-left, output 2 = M3/rear-left, output 3 = M1/rear-right, and output 4
= M2/front-right.

The manager never owns the DShot backend, timer, DMA stream, motor pin, arming
state, or safety authority. UART, parser, queue, and timeout faults cannot
grant authority. When they prevent fresh telemetry during guarded idle, the
1.2-second qualification fails closed and selects four stops. Telemetry loss
after `SYSTEM ARMED` is currently observational and does not itself disarm.
An arm attempt made before the manager's five-second delay has elapsed can
enter guarded idle and fail this way; a switch-low observation and a new
low-to-high arm request are required before retrying. If the FC stays powered
without the ESC bank through the first post-delay request, the response timeout
latches telemetry off; power the ESCs and reboot the FC before another powered
arming attempt.

Host parser, serial-profile, BSP route, manifest, DMA-conflict, manager
sequence/timeout, and RTIC-preemption tests pass. The default embedded image
compiles.

The props-off powered telemetry checkpoint passed on 2026-07-20. Firmware
SHA-256
`B7F23AF92B8ADE1A81785FF5CC08B3DEEFF1C09D4CFC2EA31FDBB6F6220319A1`
produced retained log
`logs/terminal_embed/20260720_185949_rtt.log`, SHA-256
`4CE58E3286F4CA2DF00FB47F47C8D5CE3379CCB1C12CA5871C05D0984C727692`.

At the final retained checkpoint the manager had queued 3,950 requests,
received 3,950 exact actuator frame-start acknowledgements, and decoded 3,950
valid responses. Actuator-ack timeouts, ESC-response timeouts, mismatched
acknowledgements, unsolicited frames, CRC failures, and discarded bytes all
remained zero. This is exact evidence for the bounded request/ack architecture
under live DShot600 and UART DMA traffic.

All four ESCs reported zero eRPM while stopped. DShot idle value `112` produced
approximately 6,500-7,200 eRPM. Modest mixed values near `140-142` produced
approximately 11,300-12,100 eRPM, and values near `235-238` produced
approximately 23,300-23,800 eRPM. RC loss selected four DShot zeros; physical
motor stop was immediate by operator observation and all four telemetry
channels reported zero by the following two-second report. Explicit disarm
behaved the same way. Nonzero values in the report coincident with each stop
are consistent with motor coast-down and the manager's latest-sample snapshot,
not continued DShot command.

Voltage, current, consumption, and temperature remained zero in these ESC
responses despite the independent ADC reporting 23.7 V. Treat those auxiliary
fields as unsupported/unvalidated on the installed ESC configuration; only
eRPM and frame integrity are closed by this checkpoint. Props-on first-hop
validation remains separate.

## Telemetry-Qualified DShot Arming Candidate

The standard DShot path now treats idle spin as an actuator-owned arming
qualification, not evidence gathered after the system is armed. Following the
existing 100 ms stop dwell, the sole actuator owner applies idle command `65`
(DShot value `112`) under the temporary arm permit and renews its short output
lease every 10 ms. It continues to require valid arm permission, qualified RC,
arm switch high, and low throttle throughout the attempt.

The ESC manager remains outside motor and safety authority. It publishes
timestamped, physical-output-associated telemetry observations through a
bounded SPSC queue. The actuator owner requires three consecutive fresh
observations from each of the four ESCs between 3,000 and 10,000 eRPM, after a
250 ms spin-up grace. Missing, zero, or stale RPM fails at the 1.2-second qualification
deadline; an eRPM above the ceiling fails immediately after the grace period.
Every failure selects four stop values and reports an explicit arming-abort
reason. Only successful four-motor qualification allows the safety master to
perform its final guard check and emit `SYSTEM ARMED`.

Host tests cover complete four-motor qualification, zero-RPM timeout,
overspeed rejection, and stale-sample rejection. Default DShot and explicit
PWM-fallback release checks pass. The powered props-off validation of this new
gate passed as recorded below.

The powered props-off positive checkpoint passed on 2026-07-20 with firmware
SHA-256
`8E81BE35FFD7E2E809CF8EAD18C77D4619F5C4DCB0C85E753D92C40C247EEB70`
and retained log `logs/terminal_embed/20260720_192246_rtt.log`, SHA-256
`F1546E7154C260FC1716362D4FD65D91E365DDC1DED13502AE6FB9F6AD78A2CC`.
Three arming attempts each reached `[3, 3, 3, 3]` before `SYSTEM ARMED`; idle
telemetry was approximately 6,500-7,100 eRPM. All 2,050 queued requests
started and decoded successfully with zero acknowledgement timeout, response
timeout, mismatched acknowledgement, unsolicited frame, CRC failure, or
discarded byte.

An opt-in `bench_dshot_idle_output1_not_running` negative-test feature now
substitutes zero eRPM for physical output 1 (logical M4/front-left) only at the
actuator-owned qualification input. The old
`bench_dshot_idle_motor1_not_running` spelling remains a compatibility alias.
It does not change the four real DShot idle outputs or ESC manager telemetry.
The image must time out, identify physical output 1 as unproven, select four
stop values, and never report `SYSTEM ARMED`.

The props-off negative checkpoint passed on 2026-07-20 using fault-image
SHA-256
`1CEFE0E47030C2AECB3BC1BB15244A65E3608F14D79FF0D16D408C54AC175B80`
and retained log `logs/terminal_embed/20260720_193148_rtt.log`, SHA-256
`7FAFC3A7F9AD85DA4546525D76C91113F16D2D4AA01AD57BD3730867D34DD477`.
Three attempts each timed out with counts `[0, 12, 12, 12]`, explicitly named
`ESC1` as not proven running, selected sustained four-zero DShot output, and
never emitted `SYSTEM ARMED`. In that dated log, `ESC1` means physical output
1 / logical M4, not logical M1. The operator observed motor stop after 1.2
seconds. The manager completed 1,450/1,450 valid telemetry transactions with
no reported transport or parser error. Reflash the standard image before any
subsequent validation.

The standard `blackbox_defmt` unpowered stationary checkpoint then passed with
firmware SHA-256
`0B9EA10FDDFD399E35E89D950A9D02D651A0F28FD18F76F72730C11696738624`
and retained log `logs/terminal_embed/20260720_194205_rtt.log`, SHA-256
`34B1C07ACFE5C996E6E2EBA0673AC80A19EA58476BBBB0A835636B00BE5BB699`.
It contains 5,255 contiguous 400 Hz BB2 samples with no control-sequence gap;
the IMU sequence advanced by exactly two for every control sample and all
samples reported fresh. After trimming three startup seconds and two ending
seconds, filtered gyro mean/std were roll `+0.09/0.71 dps`, pitch
`-0.11/0.64 dps`, and yaw `+0.15/0.70 dps`; all axes classified quiet.
Commands, PID terms, throttle, and motors stayed exactly zero. DShot lanes
remained synchronized with zero backend faults. Battery voltage was 0.1 V, so
zero valid ESC telemetry and accumulated response timeouts are expected from
the deliberately unpowered ESC bank and are not a telemetry regression. That
dated image predates the current fail-closed association-timeout latch; the
current manager stops issuing requests after the first acknowledgement or
response timeout until reboot.

Two powered props-off `blackbox_defmt` captures on 2026-07-20 close the
standard-image telemetry/BB2 coexistence and correction-direction checkpoints.
Both used firmware SHA-256
`0B9EA10FDDFD399E35E89D950A9D02D651A0F28FD18F76F72730C11696738624`.

Retained log `logs/terminal_embed/20260720_194720_rtt.log`, SHA-256
`3371D58D4298124DD31A1BBD3B912DDDFCC34EF6818B6627C0B1ED94B483EA21`,
contains 15,047 contiguous 400 Hz BB2 samples with no sequence gap, unexpected
IMU step, or stale sample. Commands remained zero during the hand-motion
portion. On roll, pitch, and yaw, PID output tracked `command - gyro`, and the
corresponding physical motor-pair differential was negatively correlated with
measured motion, confirming that the rate correction opposed motion on all
three axes. The run qualified `[3, 3, 3, 3]`, armed, completed 1,550/1,550
valid ESC telemetry transactions without a reported error, and disarmed to
four stop values.

Retained log `logs/terminal_embed/20260720_194906_rtt.log`, SHA-256
`962D29DF359B9CCEDB8E78B5BD5103716FF365A34A5F6C572A6883AC81FC8D43`,
contains 13,053 contiguous 400 Hz BB2 samples with the same sequence/freshness
properties. Roll commands ranged from `-563` to `+851 dps` and pitch commands
from `-585` to `+635 dps`; physical motor-pair differences followed the
corresponding PID outputs with the expected signs. Yaw command remained
exactly zero, so those captures did not cover yaw-stick input. This run
also qualified `[3, 3, 3, 3]`, armed, completed 1,350/1,350 valid ESC
telemetry transactions without a reported error, and disarmed to stop.

The terminal display could not keep up with BB2 volume, but the retained files
did not overflow: both BB2 sequences are complete. The only intervening
`20260720_194834` and `20260720_194852` files are failed probe-open attempts
with no target runtime.

The remaining yaw-stick observation closed in
`logs/terminal_embed/20260720_195451_rtt.log`, SHA-256
`2EE0BEE796E2C3B2A90161F9FBEA82E6E76E9BD2BA9B2207FF0745F5ED00DEA6`,
using the same standard `blackbox_defmt` firmware. The file contains 13,565
contiguous 400 Hz BB2 samples, no unexpected IMU step, and no stale sample.
Yaw command ranged from `-1000` to `+1000 dps` across 4,642 command-active
samples. The physical yaw-diagonal differential followed controller output
with the expected sign. In the subsequent 1,940-sample zero-command hand-yaw
slice, PID output tracked negative measured yaw with correlation `0.9999`, and
the motor-diagonal differential was negatively correlated with measured yaw,
confirming opposing correction. The run qualified `[3, 3, 3, 3]`, armed,
completed 1,350/1,350 valid telemetry transactions without a reported error,
and selected four DShot stop values on disarm. This closes the props-off
roll/pitch/yaw stick-mixing and motion-opposition checkpoints for the standard
DShot image.

## NUCLEO-F401RE Multi-Target Bring-Up

An isolated `apps/stm32f401-bringup` application and typed `nucleo_f401re` BSP
target now provide the first F401/F405 multi-target proof. The Nucleo target
owns only PA5 LD2, PA2 USART2 TX to the ST-LINK virtual COM port, SysTick as
the RTIC monotonic, and EXTI0 as the RTIC software-task dispatcher. Its
manifest explicitly declares no attached IMU and no actuator outputs.

The app uses the STM32F401 PAC/HAL feature, 512 KiB flash / 96 KiB SRAM linker
map, 84 MHz HSI clock, and an STM32F401RE ST-LINK runner. It is now a minimal
RTIC 2 shell with initialization and one persistent priority-1 async heartbeat
task. Static compilation passes.

The original polling bring-up image passed target validation on 2026-07-18.
The connected target reported
STM32F401 DBGMCU device code `0x433`; the flashed image ran with the debugger
detached; GPIOA ODR bit 5 changed state; and COM4 produced monotonic heartbeat
lines through ten sampled 30-second checkpoints. The final checkpoint advanced
from sequence 802 to 833 with no output timeout or sequence rollback.

The replacement RTIC image passed its target smoke test on 2026-07-18. The
user flashed it to the connected Nucleo and confirmed that it works. This
closes the immediate scheduler/LED/USART bring-up checkpoint; the polling
image's five-minute run remains the longer-duration soak evidence.

## Foxeer F405 V2 BSP Bring-Up

The third board target is `foxeer_f405_v2`, board name **Foxeer F405 V2**,
with a separate `apps/foxeer-f405-v2` RTIC flight shell. The initial contract
implements only the FerroWasp services needed now:

- SPI1 mode-3 identity probe and runtime-selected MPU6500 or ICM42688-P data
  path on PA4-PA7;
- USART2 SBUS on PA2/PA3;
- UART4 DJI MSP DisplayPort on PA0/PA1;
- ADC1 voltage/current observation on PC0/PC1;
- four conventional 400 Hz RC PWM outputs on PA8, PC9, PC8, and PB15.

Motor 4 is the advanced-timer complementary output `TIM1_CH3N`; it is not an
ordinary `TIM1_CH3` output. The custom board PWM bank starts with both
advanced-timer `MOE` gates closed, and only the safety-owned actuator path may
open them. A forced-off command closes both gates and clears/latches all four
compares.

Flight arming is compile-time inhibited until the fitted IMU identity and
axis orientation, ADC scales, logical motor order, and M4 physical pulse
polarity are verified.
`WHO_AM_I=0x70` selects the MPU6500 driver and burst beginning at `0x3b`;
`WHO_AM_I=0x47` selects the ICM42688-P driver and burst beginning at `0x1d`.
An unsupported or failed IMU identity/configuration is nonfatal and disables
periodic sampling, allowing the remaining bring-up services and heartbeat to
continue. The ICM42688-P starts at 1 MHz SPI, 1 kHz ODR, +/-2000 dps, and
+/-16 g. PC4 data-ready/EXTI remains deferred; the current app polls at 800 Hz.
The provisional logical-to-physical map is `[1, 2, 3, 4]` under the shared
Betaflight Quad X convention; unlike FCU3's measured physical output order and
current `[3, 4, 2, 1]` remap, it is not treated as target evidence.
PA13/PA14 remain untouched for SWD because the board status LEDs share those
pins. The active motor protocol is 400 Hz, 1000..2000 us RC PWM under the
safety-owned actuator task. PWM DMA/DShot, M5-M8, SPI flash, analog OSD, I2C,
buzzer, camera control, and LED strip support remain deferred.

An opt-in `usb_serial` image now provides read-only Foxeer CDC diagnostics on
PA11/PA12. The existing two-second heartbeat requests a bounded `FWDBG1`
snapshot containing IMU/control sequences, raw gyro, RC state, throttle,
arming state, voltage, and current. OTG_FS runs below the safety, control,
IMU, and RC paths. Host bytes are drained and ignored; USB has no command,
safety, or actuator authority. Build or flash it with
`apps/foxeer-f405-v2/flash-dfu.ps1 -UsbDebug`.

Foxeer's app-local Cargo runner now programs its ELF through
STM32CubeProgrammer, so `cargo run --release --locked` is the normal ROM-DFU
command. The runner validates the ELF origin at `0x08000000`, detects the
available `USBn` port, verifies after programming, and starts execution at
`0x08000000`. Use
`FERROWASP_DFU_DRY_RUN=1` for a guaranteed no-flash integration check.

Physical ROM-DFU programming was verified on 2026-07-18 with
STM32CubeProgrammer 2.23.0: one Foxeer device was detected as `USB1`, the
77.80 KiB USB-debug ELF was programmed and verified, and the explicit start
operation succeeded.

The subsequent normal USB reconnect enumerated the Foxeer diagnostic image
as COM6 and produced the expected read-only header and advancing `FWDBG1`
frames. `imu=icm42688p ready=1` confirms the mode-3 `WHO_AM_I=0x47` probe and
driver configuration succeeded. The observed IMU and control sequences
advanced at approximately 800 Hz and 400 Hz respectively with `stale=0`.
A subsequent 300.584-second COM6 soak captured 152 continuous frames at
799.996 Hz IMU and 400.001 Hz control rates. The maximum report gap was
2001 ms, with no malformed/stale frames, readiness failures, timestamp or
sequence regressions, serial errors, disconnects, or safety-state changes.
Accel/temperature and explicit RTT-warning observation, orientation, and ADC
calibration remain open; USB-only `current_cA=793..820` is not calibrated.

The source hardware analysis is
`foxeer_f405v2_ferrowasp_hardware_map.md`. Formatting, host tests, strict host
and embedded Clippy, all three app release builds, the Foxeer feature matrix,
DFU image generation, mdBook, diff checks, and the unsafe-source scan pass.
Physical target validation is the current checkpoint. Do not flash this image
to FCU3 or enable Foxeer arming by assumption.

## Isolated Firmware App Packages

The repository root is now a virtual workspace containing reusable crates.
Deployable firmware is intentionally excluded into independent Cargo graphs:

- `apps/stm32f405-flight` owns the F405 RTIC flight contract and selects
  FerroWasp FCU3 through the default `board-ferrowasp-fcu3` feature;
- `apps/stm32f401-bringup` owns the F401 RTIC LED/USART bring-up contract and
  selects Nucleo through the default `board-nucleo-f401re` feature;
- `apps/foxeer-f405-v2` owns the Foxeer F405 V2 flight contract and selects
  its BSP through the default `board-foxeer-f405-v2` feature.

Each app owns its lockfile, target configuration, linker map, runner, and
binary. This prevents STM32F401 and STM32F405 PAC features from being unified.
The F405 binary remains named `FerroWasp`; runtime behavior, external pins,
DMA/timer routes, priorities, and actuator authority are unchanged.

Static verification passes for all three app release builds and Clippy, the
F405 bench/diagnostic feature matrix, 186 reusable-crate host tests, host and
embedded reusable-crate Clippy, all four formatting scopes, mdBook, the
workspace unsafe scan, and the FerroDebugger release-build bridge. One normal
FCU3 smoke test with actuators unpowered remains because the package now owns
the F405 linker and runner configuration.

This file is for Codex sessions, especially cloud/parallel sessions, so they do
not accidentally undo or invalidate the current bench-debugging work.

Offline flight-test commands are in:

```text
project_docs/FLIGHT_TEST_QUICK_COMMANDS.md
```

Remote bench helpers now accept named network links:

- `-Link Cable` for laptop normal Wi-Fi plus direct Ethernet to the Pi.
- `-Link Mobile` for laptop and Pi both on the configured mobile router.
- `-Link Zero` for the Raspberry Pi Zero 2 W FerroDebugger gateway.
- `-Link ZeroMobile` for the Zero 2 W gateway on the mobile router.

The PowerShell helpers in `tools/` now delegate Pi/probe/log transport to the
sibling `..\ferro-debugger` repository. Keep firmware build features and BB2
schema ownership in FerroWasp; keep Pi setup, link config, service install,
ELF sync, detached logging, and log fetch implementation in FerroDebugger.

## Current Focus

The active work is publication preparation for the experimental FerroWasp
source repository. The current tree must be consolidated deliberately, public
documentation must describe the DShot/telemetry-qualified baseline accurately,
all supported app checks must pass, and sensitive or oversized historical Git
objects must not become public.

After publication preparation, the next target is Foxeer F405 V2 bring-up. Its
existing compile-time arming inhibit remains mandatory until its physical IMU
orientation, RC, OSD, ADC, motor waveform, motor order, and reset/forced-off
gates are measured. WSL/Docker development-environment work follows the Foxeer
checkpoint.

FCU3 flight tuning is deliberately parked while publication work is active.
The previous yaw tendency was absent in the latest flight. The next isolated
flight-control experiment is the recorded pitch P `0.25 -> 0.30` step, not a
broad controller or mixer refactor.

## Do Not Casually Change

Avoid changing these unless the user explicitly asks:

- motor output authority, actuator gating, arming, failsafe, or watchdog paths
- motor order, motor remap, board orientation, gyro axis/sign mapping
- RC channel mapping or throttle scaling
- blackbox `BB2` frame format
- remote probe/Pi logging scripts and token workflow
- `bench_equal_motors` behavior or throttle cap
- tuning gains as a broad "quick fix" without an isolated change and flight or
  BB2 evidence; the recorded pitch P `0.25 -> 0.30` trial is the one currently
  authorized follow-up

If a change touches any of those, explain the reason first and keep the patch
small.

## Important Safety State

The operator inspected the motor associated with the earlier smoke report on
2026-07-20 and reports that it appears normal. This closes the outstanding
hardware-inspection blocker. Continue ordinary powered-test stop rules: stop
immediately on smoke, hot smell, abnormal jitter, rapid heating, rough
rotation, or unexpected current rise.

## Current Firmware/Test Support

An opt-in bench feature exists:

```powershell
.\tools\remote_run.ps1 -Link Cable -Build -Features "blackbox_defmt bench_equal_motors"
```

Behavior:

- normal firmware builds are unchanged
- while armed, PID/mixer output is bypassed
- all four motors receive equal requested throttle
- requested throttle is capped to `250` PWM-style units
- output still goes through `actuator_output`
- `BB2` logs PID as `[0, 0, 0]` and motors as `[throttle; 4]`

This feature is intended to measure motor/frame vibration without controller
feedback.

Another opt-in bench feature exists for motor numbering:

```powershell
.\tools\remote_run.ps1 -Link Cable -Build -Features "blackbox_defmt bench_motor1_only"
```

For motor/output 2:

```powershell
.\tools\remote_run.ps1 -Link Cable -Build -Features "blackbox_defmt bench_motor2_only"
```

For motor/output 3:

```powershell
.\tools\remote_run.ps1 -Link Cable -Build -Features "blackbox_defmt bench_motor3_only"
```

For motor/output 4:

```powershell
.\tools\remote_run.ps1 -Link Cable -Build -Features "blackbox_defmt bench_motor4_only"
```

Behavior:

- normal arming still required
- PID/mixer output is bypassed
- only the selected firmware/physical output is commanded above low/stop
- selected output is capped to `250` PWM-style units
- other outputs are held at `ESC_LOW_THROTTLE` by `actuator_output`
- `BB2` logs PID as `[0, 0, 0]` and only the selected motor field as nonzero

Use this only with propellers removed and only long enough to identify the
physical motor connected to each selected output.

Mapped logical motor verification modes also exist:

```powershell
.\tools\remote_run.ps1 -Link Cable -Build -Features "blackbox_defmt bench_logical_motor1_only"
```

```powershell
.\tools\remote_run.ps1 -Link Cable -Build -Features "blackbox_defmt bench_logical_motor2_only"
```

```powershell
.\tools\remote_run.ps1 -Link Cable -Build -Features "blackbox_defmt bench_logical_motor3_only"
```

```powershell
.\tools\remote_run.ps1 -Link Cable -Build -Features "blackbox_defmt bench_logical_motor4_only"
```

Expected physical order after the current Betaflight Quad X logical remap:
rear-right, front-right, rear-left, front-left.

All actuator-capable boards must re-test motor order, motor direction, and
stick/tilt response with propellers removed before flight after any board
profile, output backend, motor-map, or wiring change. The NUCLEO-F401RE target
has no actuator outputs.

Normal non-`bench_equal_motors` builds currently start with the first-hop tuning
profile:

- roll P/I/D = `0.2 / 0.0 / 0.0`
- pitch P/I/D = `0.25 / 0.0 / 0.0`
- yaw P/I/D = `0.30 / 0.04 / 0.0`
- RC rate deadband = `8` raw channel counts
- startup gyro zero-rate bias calibration is present in the control-frame gyro
  path; keep the FCU still for the first couple seconds after boot

Yaw I `0.04` is the current baseline. Do not use yaw I `0.06` as the next
baseline without a specific reason; it was worse in the clean centered-yaw
slice. The only planned gain change is pitch P `0.25 -> 0.30`, performed in
isolation after publication/Foxeer/environment work. Abort on oscillation,
bounce, yaw wag, excessive noise, heat, or stronger maneuver instability.

## Latest Evidence

See `project_docs/BENCH_TEST_PLAN.md` for the full evidence trail.

Key results:

- manual hand-swing test: gyro path tracks low-frequency motion with about one
  control tick delay
- still/disarmed test: the earlier `+605 dps` pitch mean did not reproduce
- phased low-throttle test: pitch stayed quiet; higher throttle introduced
  roll spikes and motor spread
- higher-throttle reference: yaw and roll vibration increased with throttle;
  worst motor spread was yaw-dominated
- equal-motor props-off test: PID stayed zero, motor spread stayed zero, and no
  abnormal physical behavior was observed at capped low output
- motor numbering check: firmware/physical output 1 spun the front-left motor;
  this is Betaflight Quad X motor 4
- motor numbering check: firmware/physical output 2 spun the rear-left motor;
  this is Betaflight Quad X motor 3
- motor numbering check: firmware/physical output 3 spun the rear-right motor;
  this is Betaflight Quad X motor 1
- motor numbering check: firmware/physical output 4 spun the front-right motor;
  this is Betaflight Quad X motor 2
- firmware now uses Betaflight Quad X logical numbering with
  `MOTOR_OUTPUT_MAP = [3, 4, 2, 1]` on FCU3, so logical mixer motor 1 maps to
  physical output 3, motor 2 to output 4, motor 3 to output 2, and motor 4 to
  output 1
- mapped logical motor verification previously passed under the older
  FerroWasp logical convention; after the Betaflight renumbering patch, repeat
  the mapped logical motor checks before any further FCU3 flight
- gyro sign step check passed: pitch elevation/nose-up is positive, roll
  clockwise from the FPV camera point of view is positive, and yaw clockwise
  viewed from above is positive
- RC stick-to-axis mapping issue found: right stick roll was commanding pitch,
  and right stick pitch was commanding roll; roll/pitch command signs were also
  inverted relative to expected motor response
- RC mapping patch applied: roll channel index `0`, pitch channel index `1`,
  roll invert `false`, pitch invert `false`, yaw unchanged
- props-off stick-to-motor re-check passed after the RC mapping patch: right
  stick right/left affects left/right motors, right stick forward/back affects
  rear/front motors, and yaw stick affects the expected diagonals
- field props-off free-hand tilt log `pi_20260712_165653_attach.log` found a
  pitch correction sign error: positive pitch gyro drove negative pitch PID and
  front motors rose relative to rear; negative pitch gyro drove rear motors up
- firmware patch inverted `CONTROL_PITCH_GYRO_SIGN` from `1` to `-1`; this must
  be re-verified with props removed before any further prop-on attempt
- after-patch field log `pi_20260712_170645_attach.log` captured BB2 data; in
  controller-frame terms, logged negative pitch now drives rear motors higher
  and logged positive pitch drives front motors higher. This is consistent with
  the intended patch if physical nose-up now appears as negative pitch in the
  viewer/log.
- first post-patch prop-on LOS attempt achieved near-vertical lift, then hard
  back-and-forth oscillation. No log was captured because the telemetry harness
  was bulky.
- softened first-hop tune after the oscillation report: roll/pitch P changed
  from `2.0` to `1.0`, yaw P changed from `1.0` to `0.3`; I and D remain zero.
- second softened prop-on attempt still oscillated, seemed more roll-like, and
  still had plenty of control authority. Further softened first-hop tune:
  roll P `0.5`, pitch P `0.7`, yaw P `0.25`; I and D remain zero.
- aggressive lower-limit test tune selected after the user observed plenty of
  authority remained: roll P `0.2`, pitch P `0.25`, yaw P `0.1`; I and D remain
  zero. This is diagnostic, not a final flight tune.
- lower-limit tune produced a stable, flyable LOS flight, but the drone yawed
  clockwise hands-off and needed continuous correction. Added `RC_RATE_DEADBAND`
  of `8` raw channel counts and restored yaw P to `0.25` while keeping roll and
  pitch at lower-limit values.
- fresh stationary BB2 bench log `rtt-20260713-190555.log` showed command input
  mean/std/p2p/drift all exactly zero, so RC command jitter is not currently the
  yaw/pitch drift source. Filtered gyro means were roll `-0.14 dps`, pitch
  `14.15 dps`, yaw `8.98 dps`; startup gyro bias calibration was added after
  this finding.
- follow-up stationary BB2 log `rtt-20260713-191331.log` after startup gyro-bias
  calibration kept centered commands at zero and reduced filtered means to roll
  `-0.09 dps`, pitch `-0.27 dps`, yaw `0.32 dps`.
- short flight log `rtt-20260713-224405.log` showed yaw drift was improved but
  still present. During armed, throttle-on, centered-yaw slices, filtered yaw
  remained roughly `39 dps` with opposite yaw PID correction around `-10`, so
  the next diagnostic tune adds yaw P `0.30` and yaw I `0.02`.
- stationary bias recheck `rtt-20260714-223924.log` showed filtered gyro means
  near zero: roll `-0.24 dps`, pitch `-0.23 dps`, yaw `0.09 dps`; disarmed PID
  and motors stayed zero.
- clean zero-stick hover baseline `rtt-20260714-224537.log` on yaw I `0.02`
  showed throttle `>=500` centered-yaw rate about `+55.62 dps`, yaw PID
  `-25.34`, and motor 3 highest.
- yaw I `0.04` hover `rtt-20260714-230458.log` improved the clean
  centered-yaw slice to about `+17.41 dps`, yaw PID `-13.59`, and smaller motor
  spread.
- yaw I `0.06` hover `rtt-20260714-231137.log` was worse in the clean
  centered-yaw slice at about `+35.84 dps`; firmware was reverted to yaw I
  `0.04`.
- conservative FPV characterization `rtt-20260714-231751.log` on yaw I `0.04`
  was manageable by pilot report, had about `75 s` armed time, and showed no
  near-zero or high motor saturation in the throttle `>=500` slice. Motor 3
  remained highest on average and motor 4 lowest.
- the standard DShot600/legacy-telemetry image subsequently completed a
  controlled outdoor flight on 2026-07-20. The operator reports strong
  maneuver capability and no recurrence of the previous unwanted yawing.
  Pitch authority felt low; the next isolated experiment is pitch P `0.30`
  from the current `0.25` baseline. No new BB2 flight log accompanied this
  report.
- restrained PID/mixer tests are not clean vibration measurements

## Known Bugs / Open Issues

- the earlier yaw/torque asymmetry was not observed during the latest
  controlled flight; retain the older evidence as historical until a logged
  repeat confirms the improvement
- pitch authority felt low during the latest flight; distinguish low P tracking
  from mixer saturation, center-of-gravity, or directional thrust imbalance
  before increasing beyond the planned `0.30` P trial
- no log captured for the first successful flyable flight because the telemetry
  harness was bulky
- the earlier motor-smoke concern was inspected and closed by the operator on
  2026-07-20
- FCU3 logical motor numbering and CW/CCW rotation direction were verified on
  target on 2026-07-20: M1 rear-right CW, M2 front-right CCW, M3 rear-left CCW,
  M4 front-left CW
- bounded actuator-owned arming-idle abort and the boot/reconnect arm-high
  latch are target-validated
- active and bench motor values now use the SPSC `MotorCmd` queue with
  actuator-owned latest/fresh validation; stale-command injection passed on
  target and the normal-build checkpoint remains
- independent actuator deadline detection for total control-loop command loss
  is not yet implemented
- IMU initialization, gyro-bias calibration, and freshness are not arming
  prerequisites. The current first post-arm stale-IMU check requests disarm,
  leaving a brief armed transition possible; RC/IMU/ADC freshness and loss
  policies remain prototype-level

## Recommended Next Session

1. Finish the publication-preparation checklist: clean supported-target builds,
   current public documentation, concrete security reporting, dependency and
   toolchain pinning, and an intentional clean publication tree.
2. Rotate the historical probe credential before repository visibility changes.
   Publish from sanitized history; do not expose the old raw-capture branch.
3. Run the final checks from a fresh clone of the proposed public history.
4. Begin the Foxeer F405 V2 physical checklist with arming still inhibited.
5. After Foxeer works, add and verify the WSL/Docker development environment.
6. Return to FCU3 pitch tuning with the isolated P `0.30` trial and preferably
   a BB2 flight capture.

## Verification Commands Used

These passed after the current bench-mode changes:

```powershell
cd apps/stm32f405-flight
cargo check --features blackbox_defmt
```

```powershell
cd apps/stm32f405-flight
cargo check --features "blackbox_defmt bench_equal_motors"
```

```powershell
cd apps/stm32f405-flight
cargo check --features "blackbox_defmt bench_motor3_only"
```

```powershell
python -m py_compile tools\blackbox_analyzer.py
```

The build still emits existing warnings; they are not new blockers for this
bench workflow.

## UART4 Async TX Target Checkpoint

The UART4 OSD transmit path now uses the portable bounded async writer, a
persistent priority-4 DMA owner worker, and the priority-6 DMA1 Stream 4
completion IRQ. PA0/PA1, DMA1 Stream 4 Channel 4, UART settings, and the fixed
70-byte zero-padded MSP transfer are unchanged.

Before continuing serial migration, flash the normal build with the O4/MSP link
powered and verify:

1. firmware init, heartbeat, IMU sampling, and gyro calibration remain healthy;
2. the OSD appears and battery/throttle values continue updating;
3. throttle still follows live RC stick input;
4. MSP request/response traffic remains active for several minutes;
5. no `UART4 TX writer stopped`, `UART4 TX writer found invalid transport
   state`, `UART4 TX DMA start failed`, `UART4 TX completion arrived`,
   `UART4 TX DMA transfer error`, or `UART4 TX DMA direct-mode error` warning
   appears.

No powered motor test is required. PWM/DShot DMA integration remains explicitly
deferred.

The first target attempt produced repeated `UART4 TX writer rejected MSP frame`
warnings while the IMU continued running. Inspection found that the new IRQ
handler treated the STM32 HAL's advisory FIFO-error flag as a terminal DMA
failure. The corrected handler clears FIFO error and leaves the transfer in
flight, while true transfer/direct-mode errors remain terminal. TX fault
reporting is now one-shot so a terminal channel cannot flood RTT. Repeat the
normal target checkpoint above before continuing serial migration.

Corrected target validation passed on 2026-07-18. OSD display and live throttle
updates worked, battery voltage read 23.0 V, IMU sequence advanced at about
800 Hz through 28,831, gyro calibration completed, and no UART4 TX warning
appeared. The single startup IMU-stale tick before the first sample remains
expected.

## USART2 Owned RX And RC-Loss Checkpoint

USART2 SBUS now publishes detached DMA data into a bounded owned-RX channel and
wakes one persistent priority-10 async parser. The IRQ path immediately reports
DMA errors, queue overflow, invalid chunks, and transport discontinuities to
the safety master.

The RC link starts invalid, qualifies after three healthy SBUS frames, and
expires after 100 ms without a healthy frame. Parser errors and SBUS
frame-lost/failsafe flags invalidate it immediately. An invalidation revokes
actuator permission, disarms through the existing actuator owner, neutralizes
published RC inputs, and requires a newly observed low arm switch before a
later arm request can qualify.

Static verification passes for the normal and SPI timeout-injection firmware
builds, all 145 host unit tests plus doc tests, embedded Clippy, formatting,
and diff checks.

Target validation is required before the next serial migration:

1. Flash the normal build with the receiver powered and arm switch low.
2. Confirm `RC link valid after healthy-frame qualification` appears once.
3. Confirm IMU, OSD, throttle response, and heartbeat remain healthy with no
   USART2 warning.
4. With actuators unpowered, interrupt the RC link for longer than 100 ms.
5. Confirm one timeout, SBUS failsafe, or SBUS frame-lost warning appears and
   does not flood RTT.
6. Restore the link while the arm switch is high; it must not request arming.
7. Move the switch low, then high for the normal hold time; only then may an
   arm request occur.

No powered motor test is required. PA2/PA3, DMA1 Stream 5 Channel 4, UART
settings, timer assignments, motor resources, and actuator authority are
unchanged. PWM/DShot DMA remains explicitly deferred.

Target validation passed on 2026-07-18:

- the link qualified normally through the new owned async USART2 path;
- removing the DJI Goggles 3 link produced one RC frame-timeout invalidation
  without an RTT warning flood;
- restoring the link recovered after healthy-frame qualification;
- restoring while the DJI RC3 arm toggle was logically high did not request
  arming;
- the first arm-button press supplied the required low observation and did not
  arm;
- the second press supplied a fresh low-to-high transition and produced
  `RC Requests ARM!` followed by the existing BLHeli PWM arming sequence;
- IMU sampling and the rest of the firmware remained active throughout.

This completes the USART2 owned-RX, RC-loss, recovery, and rearm-interlock
target checkpoint. Actuators remained unpowered, so no powered motor evidence
was required or collected.

## Arming-Idle Abort Fix

The previous BLHeli arming sequence slept for 2.5 seconds at low output and
500 ms at idle inside the single-instance actuator task. Safety could revoke
permission during those sleeps, but a separate `Disarm` spawn could be rejected
while that task executor remained occupied.

The actuator owner now checks permission, RC link armability, arm-switch state,
and throttle every 10 ms during both holds. Any failed guard writes all motor
outputs low, clears idle completion, and only then reports `ArmingAborted` to
the safety master. The whole sequence uses `ARMING_MAX_THROTTLE`, currently 65
command counts.

Successful idle completion is relayed through a priority-13 notifier. This
allows the priority-15 actuator task to return before the priority-16 safety
master performs its final checks, so a failed final check can enqueue a
forced-low actuator command instead of colliding with the still-running
actuator executor.

The 2.5-second low hold, 500 ms idle hold, PWM configuration, motor mapping,
pins, timers, DMA routes, and authority split are unchanged. Static
verification and props-off target validation are required before this bug is
closed.

Props-off target validation passed on 2026-07-18 with actuators unpowered:

- two arming attempts with throttle above 65 aborted during the low-throttle
  hold;
- each attempt emitted one `Arming aborted: throttle exceeds 65` warning;
- neither aborted attempt reached `Applying idle throttle` or `SYSTEM ARMED`;
- returning throttle low and making a fresh arm request completed the original
  low hold, idle hold, and `SYSTEM ARMED` transition;
- IMU sequence continued at approximately 800 Hz without new stale-sample
  warnings throughout the guard polling.

The boot-high/reconnect-high arm case is also covered by the RC-link rearm
latch: link state starts non-armable and requires a newly observed low arm
state before a later high transition can qualify. This behavior was previously
target-validated during the USART2 RC-loss checkpoint.

This closes the arming-idle abort bug.
