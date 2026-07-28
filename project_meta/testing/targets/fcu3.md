# FerroWasp FCU3 Test Procedure

This is the current operator procedure for `apps/stm32f405-flight` on the
FerroWasp FCU3. FCU3 retains its own target evidence and is a secondary flight
app; the Foxeer F405 V2 app is the golden reference for established runtime
and safety behavior.

Historical measurements and dated passes are routed through
`../EVIDENCE_INDEX.md`. Historical evidence is not a substitute for running
the gates below on the proposed image.

An `active` catalog entry means that its definition may be selected. It does
not mean the test has passed or that the aircraft is cleared to fly. The user
operates all target hardware. Stop conditions override completion and evidence
collection.

## Reviewed Baseline

Use one exact flight candidate and record:

- commit plus dirty-working-tree identity;
- release ELF SHA-256 and the matching ELF supplied to the logger;
- target `fcu3`;
- the exact feature set: `board-ferrowasp-fcu3`, `dshot`, and
  `blackbox_defmt`, with no bench, diagnostic, PWM-fallback, or fault-injection
  feature;
- propeller and actuator-power state;
- logger link and retained artifact path.

Build or flash the candidate from the repository root with the current remote
workflow:

```powershell
.\tools\remote_run.ps1 -Link Zero -Build -Features blackbox_defmt
```

The app-local compile check for the same selected features is:

```powershell
cd apps\stm32f405-flight
cargo check --release --locked --features blackbox_defmt
```

The compiled first-hop control baseline is:

| Axis | P | I | D | Center | Maximum | Expo |
|---|---:|---:|---:|---:|---:|---:|
| Roll | 0.20 | 0.00 | 0.00 | 70 deg/s | 300 deg/s | 0.50 |
| Pitch | 0.25 | 0.00 | 0.00 | 70 deg/s | 300 deg/s | 0.50 |
| Yaw | 0.30 | 0.00 | 0.00 | 70 deg/s | 200 deg/s | 0.50 |

The shared RC deadband is `8` raw channel counts. The FCU3 app can adjust its
runtime tuning profile through the disarmed OSD menu, but this procedure does
not authorize a tuning change. Start from a fresh boot, record the displayed
profile where practical, and make no menu adjustment during these gates.

Yaw I `0.04` and pitch P `0.30` appear in historical flight notes. Neither is
the current compiled baseline. Any isolated tuning candidate requires a new
review, exact image identity, repeated lower gates, and its own evidence.

The logical-to-physical output map is fixed at
`MOTOR_OUTPUT_MAP = [3, 4, 2, 1]`:

| Logical motor | Physical output | Location | Rotation |
|---|---:|---|---|
| M1 | 3 | rear-right | CW |
| M2 | 4 | front-right | CCW |
| M3 | 2 | rear-left | CCW |
| M4 | 1 | front-left | CW |

The DShot idle command `65` maps to protocol value `112`. Current arming keeps
stop frames selected through the guarded dwell, then applies bounded idle
under temporary permission and requires three fresh in-range eRPM observations
from every ESC before `SYSTEM ARMED`. Counter and ESC interoperability evidence
does not establish pulse width, jitter, complementary-output margin, or
TIM1/TIM8 phase; those electrical measurements remain open.

## FCU3 Powered Props-Off DShot Gate

Catalog ID: `BENCH-FCU3-DSHOT-001`.

The user must confirm that every propeller is removed before actuator power is
connected. Secure the airframe, keep clear of the motors, use a bounded power
source where available, and keep an immediate disarm/power-removal path.

1. Record the exact commit/working tree, feature set, release ELF SHA-256,
   propeller state, and power state. Reject `bench_equal_motors`,
   selected-motor, unequal-vector, smoke/diagnostic, and fault-injection features for
   the final exact-image run.
2. With ESC power disconnected, boot the candidate and keep the FCU stationary
   through gyro-bias calibration. Require the expected FCU3 board and IMU,
   advancing IMU/control status, the
   `DShot600 standard motor output active` identity, synchronized four-lane
   DShot completion counters, no more than one frame set in flight, four stop
   values, and zero busy, lease-expiry, timeout, spurious-IRQ, or fault
   reports.
3. Sync the exact candidate ELF used for decoding:

   ```powershell
   .\tools\pi_elf_sync.ps1 -Link Zero
   ```

   Record the local and logger-side identity. A logger using a different ELF
   fails the evidence gate.
4. If motor mapping, output routing, ESC wiring, or the airframe changed, run
   the capped logical-motor identification procedure from the FCU3 app README
   before the exact-image run. Require M1/M2/M3/M4 to resolve to physical
   outputs `3/4/2/1` and the locations/directions in the baseline table.
   Reflash the clean `blackbox_defmt` flight candidate afterward.
5. Connect actuator power with propellers still removed. Reboot the FCU with
   ESC power present if a prior no-ESC session reached the telemetry response
   timeout; applying ESC power alone does not clear that latch.
6. Keep ARM low through RC and ESC-manager qualification. Confirm centered
   sticks inside the deadband command zero. Require right roll, forward pitch,
   and right yaw to use the current positive controller directions, with
   full-stick bounds near `+/-300`, `+/-300`, and `+/-200 deg/s`.
7. Verify boot or reset with ARM held high cannot arm. Require a qualified
   ARM-low observation followed by a new low-to-high transition.
8. Arm at low throttle. Require the stop dwell, bounded idle, three fresh
   in-range samples from all four ESCs, and only then `SYSTEM ARMED`. At idle,
   all four telemetry identities must be present with plausible in-range eRPM
   and no request/acknowledgement/response fault.
9. Apply only modest throttle and small, separated stick inputs. Confirm:

   - right roll raises the left motors; left roll raises the right motors;
   - forward pitch raises the rear motors; back pitch raises the front motors;
   - right yaw raises front-right and rear-left; left yaw raises front-left
     and rear-right.

10. With sticks centered and modest throttle, gently impose each axis motion.
    Every PID and motor-pair response must oppose the disturbance. Physical
    nose-up must appear as negative controller pitch and raise the rear motors;
    nose-down must appear as positive controller pitch and raise the front
    motors. Roll and yaw must likewise oppose rather than reinforce motion.
11. Explicitly disarm and require sustained four-zero DShot values and zero
    eRPM. Re-arm only after the required ARM-low transition, then cause RC loss
    using the reviewed link-loss method. Require immediate stop, no automatic
    rearm after recovery, and a fresh ARM-low then ARM-high sequence before
    another request.
12. Disarm and retain several seconds of RTT after the stop so the log captures
    the boundary. Preserve the exact log and matching ELF; do not overwrite
    either.

Stop on installed propellers; candidate, feature, or logger-ELF mismatch;
unexpected arming; wrong stick, motor, rotation, telemetry, or correction
identity; a motor starting outside the guarded sequence; failure to stop on
disarm or RC loss; automatic rearm; DShot, telemetry, stale-command, RC, or
IMU faults; smoke, heat, abnormal current, rough motor sound, or any loss of
operator confidence.

Required evidence: commit and working-tree identity, exact features, candidate
and logger ELF SHA-256, propeller/power state, board/IMU identity, unpowered
DShot counters, mapping evidence when required, arming/eRPM/telemetry
transcript, stick and correction observations, disarm/RC-loss behavior,
retained RTT log, and operator observations.

## FCU3 Final Preflight Gate

Catalog ID: `PREFLIGHT-FCU3-001`.

This gate is for the same candidate exercised by the powered props-off gate.
Any firmware, feature, runtime tuning, motor/ESC, receiver, wiring, propeller,
logger ELF, or airframe change invalidates the affected evidence and requires
the relevant lower gate again.

1. Confirm the current software and exact embedded-build checks passed for the
   recorded commit/working tree and `board-ferrowasp-fcu3 dshot
   blackbox_defmt` feature set.
2. Confirm `BENCH-FCU3-DSHOT-001` passed on the exact ELF and that its evidence
   is available. A dated historical pass is not enough.
3. Reboot and verify the compiled P-only profile and Actual Rates baseline in
   this file. All I/D values must remain zero. Make no OSD tuning change during
   preflight.
4. Confirm the logger receives the exact matching ELF:

   ```powershell
   .\tools\pi_elf_sync.ps1 -Link Zero
   .\tools\pi_log_start.ps1 -Link Zero -Restart
   ```

   Require the detached logger to be healthy and record its session/artifact
   identity before proceeding.
5. Power-cycle into the proposed flight arrangement. Keep the FCU stationary
   for the first couple seconds after every flash, reset, or power cycle.
   If the DJI O4 unit needs cooling, keep fan vibration out of that
   calibration window.
6. With propellers still removed, require expected board/IMU identity, healthy
   RC, telemetry-qualified DShot arming, OSD/battery/video status, correct
   stick and motor response, motion-opposing correction, explicit disarm, RC
   loss to four stops, and no panic or latched fault.
7. Confirm the retained log contains correctly decoded, advancing disarmed and
   armed BB2 frames from this exact image. Verify centered disarmed setpoints,
   PID terms, throttle, and motor outputs reset rather than carrying state
   across the arm/disarm boundary.
8. Disconnect power. Inspect frame, battery retention, receiver/antennas,
   motor fasteners, motor order/direction, propeller condition/orientation,
   center of gravity, and loose wiring. Install propellers only after the
   inspection and exact-image props-off evidence pass.
9. Establish a clear controlled area, conservative weather, reliable RC/video
   and logger links, an observer where appropriate, and an immediate
   abort/landing plan. The user explicitly confirms the bounded flight before
   power is restored.
10. After final power-up, do not move the aircraft during gyro calibration.
    Require disarmed controls, healthy logging, the reviewed runtime profile,
    and no new warning before proceeding.

Stop if any prerequisite is incomplete; exact image, features, runtime tuning,
matching logger ELF, or evidence is unknown; a bench/diagnostic image is
selected; any props-off behavior differs; the airframe, propellers, battery,
RC/video/logger links, environment, or operating area is unsuitable; or the
operator does not explicitly confirm the flight.

Required evidence: completed preflight checklist, commit/working-tree identity,
candidate and logger ELF SHA-256, exact features and runtime tuning, props-off
gate reference, logger session/artifact identity, decoded preflight sample,
aircraft/environment observations, and operator confirmation.

## FCU3 Bounded Logged Flight Test

Catalog ID: `FLIGHT-FCU3-001`.

Run only after the final preflight gate passes on the still-current candidate.
This remains an experimental characterization flight.

1. Do not change firmware, features, tuning, RC rates, filters, hardware, or
   logger identity after preflight. Change no more than one reviewed variable
   between separate future flights. This baseline flight must not include the
   historical yaw-I or pitch-P candidates.
2. Confirm the detached logger is active before arming. Arm from level ground.
   If idle or control behavior differs from the props-off gate, disarm without
   lifting off.
3. Perform one short, low, controlled hover or conservative flight. Apply
   small, separated roll, pitch, and yaw inputs sufficient to evaluate command
   tracking. Avoid aggressive stick steps, altitude, speed, or prolonged
   flight.
4. Land and disarm promptly. Leave the FCU and logger running for several
   seconds so the log contains the complete disarm boundary.
5. Stop and fetch the detached log:

   ```powershell
   .\tools\pi_log_stop.ps1 -Link Zero
   .\tools\pi_log_fetch.ps1 -Link Zero
   ```

   Retain the original log, matching ELF, and their SHA-256 values.
6. Analyze the selected flight interval without combining unrelated runs:

   ```powershell
   python tools\blackbox_analyzer.py --mode auto --trim-start 1 --trim-end 1 --csv logs\remote_probe\flight_test.csv
   ```

   Use a unique output name when an existing artifact would be overwritten.
7. Review:

   - commanded versus measured roll, pitch, and yaw rates;
   - autonomous or command-excited oscillation and centered-stick drift;
   - mixer headroom, rescaling, clipping, and sustained motor imbalance;
   - missing BB2 frames, repeated IMU samples, timing gaps, and logger loss;
   - arm, disarm, RC-loss, or rearm boundaries for nonzero retained setpoint,
     PID, throttle, filter, or motor state.

8. Treat clean observed boundaries as bounded target evidence, not proof of
   every internal reset or output-timing path. Electrical DShot timing remains
   unclaimed until measured independently.
9. Review the log and post-flight inspection before proposing any tuning
   change or another flight.

Disarm/land immediately on wrong motor or stick response, rapid attitude
departure, hard oscillation or bounce, reinforcing correction, unexpected
yaw/roll/pitch drift, mixer saturation, smoke, heat, abnormal current, rough
motor sound, loss of RC/video/logger confidence, or any loss of control
authority.

Required evidence: operator and environmental conditions, candidate and
matching logger ELF hashes, exact features and runtime tuning, preflight
reference, original RTT log and SHA-256, analyzer command/results,
command-tracking and oscillation review, mixer-headroom review, controller
boundary observations, stop-condition observations, and post-flight
inspection.
