# Bench Test Plan

This page records the current FerroWasp bench evidence and the next tests needed
before another free-flight attempt.

The current focus is not tuning for flight performance. The focus is proving the
basic sign conventions, motor mapping, mixer behavior, gyro filtering, and
safety-gated actuator path on the bench.

## Safety Setup

Default bench assumptions:

- propellers removed unless a test explicitly says otherwise
- normal arming path only
- no bypass of arming, actuator gating, failsafe, watchdog, or health checks
- only the actuator-output path may touch motor hardware
- test operator can disarm immediately
- Pi/debugger power should be independent from the flight battery where practical

## Tools And Data

Current blackbox workflow:

```powershell
.\tools\remote_run.ps1 -Link Zero -Build -Features blackbox_defmt
```

```powershell
.\tools\pi_elf_sync.ps1 -Link Zero
```

```powershell
.\tools\pi_log_start.ps1 -Link Zero -Restart
```

```powershell
.\tools\pi_log_stop.ps1 -Link Zero
```

```powershell
.\tools\pi_log_fetch.ps1 -Link Zero
```

Use `-Link Mobile` instead when the laptop and Pi are both on
the configured mobile router, or `-Link Cable` for the Pi 4B cable setup.

The `BB2` blackbox frames include:

- unfiltered gyro rates as `raw10`
- filtered gyro rates as `gyro10`
- commanded rates as `cmd10`
- PID output
- throttle
- mixed motor commands
- armed and IMU-fresh flags

Analysis tool:

```powershell
python tools\blackbox_analyzer.py --mode swing --trim-start 6 --trim-end 6 --csv logs\remote_probe\swing_trimmed.csv
```

For stationary or motor-vibration tests:

```powershell
python tools\blackbox_analyzer.py --mode rest --trim-start 6 --trim-end 6 --csv logs\remote_probe\motor_vibration.csv
```

For a still gyro sanity check, keep the drone disarmed and untouched:

```powershell
python tools\blackbox_analyzer.py --mode rest --trim-start 3 --trim-end 3 --csv logs\remote_probe\still_gyro.csv
```

## Completed Tests

### Test 1: Manual Hand-Swing Gyro Tracking

Purpose:

- confirm 400 Hz logging works while disarmed
- check whether filtered gyro tracks raw gyro during real body motion
- estimate filter delay during low-frequency manual movement

Setup:

- drone disarmed
- propellers removed
- operator held and manually swung/rotated the drone
- first and last 6 seconds trimmed to remove pickup/place-down movement

Evidence:

- log: `logs/remote_probe/pi_20260712_001900_attach.log`
- CSV: `logs/remote_probe/swing_trimmed.csv`
- analyzer command:

```powershell
python tools\blackbox_analyzer.py --mode swing --trim-start 6 --trim-end 6 --csv logs\remote_probe\swing_trimmed.csv
```

Result summary:

| Axis | Raw std dps | Filtered std dps | Residual std dps | Lag | Corr | Peak | Verdict |
|---|---:|---:|---:|---:|---:|---:|---|
| Roll | 1116.30 | 1116.13 | 13.07 | 2.5 ms | 1.000 | 0.6 Hz | motion |
| Pitch | 1028.44 | 1028.21 | 14.83 | 2.5 ms | 1.000 | 0.6 Hz | motion |
| Yaw | 780.15 | 779.96 | 11.57 | 2.5 ms | 1.000 | 0.5 Hz | motion |

Interpretation:

- The data is dominated by deliberate hand motion, not high-frequency vibration.
- Filtered gyro tracks raw gyro very closely.
- Estimated delay is approximately one 400 Hz control tick.
- This supports that the gyro path is coherent and low-latency for low-frequency
  body motion.
- This does not prove motor-vibration filtering is sufficient.

Status: pass for low-frequency manual motion tracking.

### Test 2: Props-Off Held-Down Throttle / Vibration Attempt

Purpose:

- inspect gyro behavior while motors run
- look for motor-induced vibration reaching the filtered gyro/PID input
- observe mixed motor outputs during a restrained test

Setup:

- propellers removed
- drone held to the table
- wool sweater placed between drone and bench to dampen some vibration
- normal arming path used
- throttle changed through several modest levels
- first and last 6 seconds trimmed

Evidence:

- log: `logs/remote_probe/pi_20260712_003850_attach.log`
- CSV: `logs/remote_probe/motor_vibration.csv`
- analyzer command:

```powershell
python tools\blackbox_analyzer.py --mode rest --trim-start 6 --trim-end 6 --csv logs\remote_probe\motor_vibration.csv
```

Whole-log analyzer summary:

| Axis | Raw std dps | Filtered std dps | Residual std dps | Attenuation | Verdict |
|---|---:|---:|---:|---:|---|
| Roll | 23.74 | 12.84 | 13.40 | 54.1% | noisy |
| Pitch | 8.54 | 8.33 | 1.22 | 97.6% | noisy |
| Yaw | 15.24 | 9.87 | 7.78 | 64.8% | noisy |

Control/motor output summary:

| Signal | Std | Peak-to-peak |
|---|---:|---:|
| PID roll | 139.82 | 3876 |
| PID pitch | 817.83 | 2000 |
| PID yaw | 98.49 | 2784 |
| Motor 1 | 455.08 | 2000 |
| Motor 2 | 430.47 | 2000 |
| Motor 3 | 57.17 | 991 |
| Motor 4 | 80.59 | 992 |

Additional observation from CSV:

| Signal | Observed behavior |
|---|---|
| Throttle | max approximately 1051 |
| Motor 1 | reached 2000 |
| Motor 2 | reached 2000 |
| Motor 3 | stayed near or below 991 |
| Motor 4 | stayed near or below 992 |

User observation:

- Two of four motors appeared to rotate considerably slower than the others.
- This matches the logged asymmetry.
- Earlier flight testing consistently produced a right roll.
- Gyro signs have since been changed, but the current asymmetric output still
  suggests a sign, axis, motor-order, or mixer problem may remain.

Interpretation:

- This is not a clean vibration-vs-RPM test.
- The controller/mixer appears to fight the restrained frame and drives a large
  asymmetric correction.
- The motor asymmetry is more concerning than the gyro noise numbers.
- Follow-up CSV analysis found the pitch gyro mean was approximately `+605 dps`,
  including while disarmed. With zero rate command and bench P gain of `10.0`,
  this drives the pitch PID output to `-2000`, which explains the observed motor
  asymmetry through the mixer.
- The motor path is therefore behaving consistently with its inputs; the suspect
  is upstream of the mixer, most likely gyro interpretation, gyro bias,
  calibration, axis mapping, burst parsing, or board orientation.
- The data is consistent with a possible sign, axis mapping, motor order, board
  orientation, or mixer-direction issue.
- If the corrective direction is wrong, a free-flight attempt could flip rather
  than hover.

Status: fail as a clean vibration test; high-priority follow-up for sign/mixer
validation.

### Test 3: Phased Still / Arm / Low-Throttle Check

Purpose:

- check whether the approximately `+605 dps` pitch mean from Test 2 repeats with
  a freshly built firmware and synced ELF
- separate disarmed still behavior, armed zero-throttle behavior, and modest
  motor spin-up behavior
- inspect whether motor asymmetry is caused by a fixed pitch error or by
  throttle/vibration-dependent control response

Setup:

- propellers removed
- drone restrained
- fresh `blackbox_defmt` firmware flashed
- matching ELF synced to the Pi before logging
- phases included disarmed still, armed zero throttle, very low throttle, modest
  throttle, and disarm
- first and last 3 seconds trimmed

Evidence:

- log: `logs/remote_probe/pi_20260712_011728_attach.log`
- CSV: `logs/remote_probe/phased_motor_test.csv`
- analyzer command:

```powershell
python tools\blackbox_analyzer.py --mode rest --trim-start 3 --trim-end 3 --csv logs\remote_probe\phased_motor_test.csv
```

Whole-log analyzer summary:

| Axis | Raw mean dps | Filtered mean dps | Raw std dps | Filtered std dps | Verdict |
|---|---:|---:|---:|---:|---|
| Roll | -0.00 | -0.00 | 3.27 | 1.80 | watch |
| Pitch | -1.66 | -1.66 | 0.54 | 0.33 | quiet |
| Yaw | 1.65 | 1.65 | 1.86 | 1.36 | watch |

Additional CSV summary:

| Region | Samples | Throttle | Gyro std roll/pitch/yaw | Motor spread avg/peak |
|---|---:|---:|---:|---:|
| Disarmed | 15680 | 0 | 0.16 / 0.10 / 0.12 | 0 / 0 |
| Armed, throttle 0 | 9315 | 0 | 0.16 / 0.11 / 0.29 | 0 / 0 |
| Armed, throttle >0 | 22939 | 1..322 | 2.59 / 0.46 / 1.96 | 84.3 / 682 |
| Armed, throttle 1..199 | 8272 | mean 129.9 | 0.23 / 0.29 / 0.26 | 65.0 / 139 |
| Armed, throttle 200..399 | 14667 | mean 277.5 | 3.23 / 0.52 / 2.44 | 95.1 / 682 |

Interpretation:

- The impossible `+605 dps` pitch mean did not reproduce.
- Pitch is quiet and near zero through the phased test.
- The strongest remaining asymmetry appears during higher throttle and is mainly
  associated with roll-rate spikes, including brief roll PID saturation.
- This points away from a fixed pitch gyro parse/bias failure in the current
  build and toward throttle/vibration, restraint interaction, roll sign/mapping,
  or physical/motor-path effects.

Status: pass for rejecting a persistent pitch gyro offset; still not a clean
motor-vibration pass because higher-throttle roll spikes and motor spread remain.

### Test 4: Higher-Throttle Reference Run

Purpose:

- capture a modestly higher throttle reference after Test 3
- check whether the fixed pitch offset returns
- identify which axis dominates the remaining motor spread

Evidence:

- log: `logs/remote_probe/pi_20260712_013326_attach.log`
- CSV: `logs/remote_probe/still_gyro.csv`
- analyzer command:

```powershell
python tools\blackbox_analyzer.py --mode rest --trim-start 3 --trim-end 3 --csv logs\remote_probe\still_gyro.csv
```

Whole-log analyzer summary:

| Axis | Raw mean dps | Filtered mean dps | Raw std dps | Filtered std dps | Peak | Verdict |
|---|---:|---:|---:|---:|---:|---|
| Roll | -0.05 | -0.05 | 9.08 | 5.32 | 4.7 Hz | noisy |
| Pitch | -1.57 | -1.58 | 2.27 | 2.02 | 0.6 Hz | watch |
| Yaw | 1.43 | 1.43 | 13.69 | 11.28 | 4.1 Hz | noisy |

Additional CSV summary:

| Region | Samples | Throttle | Gyro std roll/pitch/yaw | Motor spread avg/peak |
|---|---:|---:|---:|---:|
| Disarmed | 11344 | 0 | 0.63 / 2.35 / 4.53 | 0 / 0 |
| Armed, throttle 0 | 2000 | 0 | 0.19 / 0.17 / 0.55 | 0 / 0 |
| Armed, throttle >0 | 3916 | 1..357 | 11.13 / 1.41 / 22.39 | 263.0 / 784 |
| Armed, throttle 1..199 | 814 | mean 96.6 | 0.46 / 0.35 / 3.95 | 71.1 / 393 |
| Armed, throttle 200..399 | 3102 | mean 300.7 | 12.50 / 1.57 / 25.07 | 313.4 / 784 |

Interpretation:

- The fixed `+605 dps` pitch offset still did not return.
- Pitch remains comparatively quiet.
- At the higher throttle bucket, yaw is the largest vibration/control input,
  followed by roll.
- Worst motor-spread samples are yaw dominated: yaw gyro spikes of roughly
  `35..52 dps` create yaw PID outputs around `350..517`, and the mixer splits
  diagonal motor pairs strongly.
- This makes yaw/roll vibration, yaw sign/motor direction, and restraint
  interaction the next focus before another flight attempt.

Status: useful reference run; still not a vibration pass for flight because
filtered yaw and roll energy rise substantially with throttle.

### Test 5: Equal-Motor Props-Off Vibration Check

Purpose:

- measure motor/frame vibration with PID and mixer disabled
- verify that all four motors receive equal outputs through the normal
  actuator-output task
- check for obvious motor/ESC health problems after the earlier smoke event

Setup:

- propellers removed
- `bench_equal_motors` and `blackbox_defmt` firmware flashed
- normal arming path used
- firmware capped armed motor output to `250` PWM-style units
- no abnormal physical behavior was observed during the run
- first and last 3 seconds trimmed

Evidence:

- log: `logs/remote_probe/pi_20260712_122256_attach.log`
- CSV: `logs/remote_probe/equal_motor_vibration.csv`
- analyzer command:

```powershell
python tools\blackbox_analyzer.py --mode rest --trim-start 3 --trim-end 3 --csv logs\remote_probe\equal_motor_vibration.csv
```

Whole-log analyzer summary:

| Axis | Raw mean dps | Filtered mean dps | Raw std dps | Filtered std dps | Peak | Verdict |
|---|---:|---:|---:|---:|---:|---|
| Roll | -0.28 | -0.28 | 4.43 | 2.87 | 0.0 Hz | watch |
| Pitch | -12.54 | -12.54 | 3.65 | 2.14 | 0.5 Hz | watch |
| Yaw | 11.68 | 11.68 | 12.30 | 12.02 | 0.2 Hz | noisy |

Additional CSV summary:

| Region | Samples | Throttle | Gyro std roll/pitch/yaw | Motor spread avg/peak |
|---|---:|---:|---:|---:|
| Disarmed | 22396 | 0 motor output | 2.12 / 2.37 / 17.23 | 0 / 0 |
| Armed, throttle 0 | 9858 | 0 | 1.49 / 1.03 / 2.48 | 0 / 0 |
| Armed, throttle >0 | 15424 | 7..250 | 4.18 / 2.32 / 3.04 | 0 / 0 |
| Armed, throttle 1..99 | 286 | mean 59.1 | 1.42 / 1.00 / 1.41 | 0 / 0 |
| Armed, throttle 100..199 | 10147 | mean 121.1 | 1.77 / 2.05 / 1.75 | 0 / 0 |
| Armed, throttle 200..250 | 4991 | mean 247.5 | 6.88 / 2.83 / 4.72 | 0 / 0 |

Interpretation:

- The equal-motor feature behaved as intended: PID remained zero and motor
  spread was zero.
- The earlier asymmetric motor output was therefore caused by the normal
  controller/mixer reacting to gyro input, not unequal command generation in the
  actuator path.
- Armed equal-motor gyro noise is much lower than the earlier restrained
  PID/mixer run.
- Vibration increases near the `250` cap, especially roll and yaw, but no
  abnormal physical behavior was observed.
- The large whole-log yaw number is dominated by non-throttle/disarmed portions;
  armed throttle-on yaw standard deviation was about `3.04 dps`.

Status: pass for equal-output command symmetry and basic low-power motor health;
watch roll/yaw vibration near the cap before increasing throttle or tuning P.

### Test 6: Disarmed Gyro Bias And Command Jitter Check

Purpose:

- separate RC command-input jitter from gyro zero-rate bias
- verify whether centered sticks are responsible for the hands-off yaw drift
- validate startup gyro-bias calibration on the stationary bench

Setup:

- propellers removed
- drone disarmed and physically still on the bench
- Raspberry Pi Zero 2 W FerroDebugger link used
- first capture used the current `blackbox_defmt` build before startup gyro-bias
  calibration
- second capture used the same workflow after adding startup gyro-bias
  calibration
- first and last 3 seconds trimmed for analysis

Evidence before calibration:

- log: `logs/remote_probe/rtt-20260713-190555.log`
- CSV: `logs/remote_probe/bench_jitter_drift_fresh.csv`
- analyzer command:

```powershell
python tools\blackbox_analyzer.py --mode rest --trim-start 3 --trim-end 3 --csv logs\remote_probe\bench_jitter_drift_fresh.csv
```

Before-calibration analyzer summary:

| Axis | Raw mean dps | Filtered mean dps | Raw std dps | Filtered std dps | Drift dps | Verdict |
|---|---:|---:|---:|---:|---:|---|
| Roll | -0.14 | -0.14 | 2.18 | 1.22 | -0.40 | watch |
| Pitch | 14.15 | 14.15 | 1.54 | 0.85 | -0.70 | quiet |
| Yaw | 8.98 | 8.98 | 1.21 | 0.71 | -0.80 | quiet |

Command-input summary before calibration:

| Axis | Mean dps | Std dps | Peak-to-peak dps | Drift dps |
|---|---:|---:|---:|---:|
| Roll command | 0.00 | 0.00 | 0.00 | 0.00 |
| Pitch command | 0.00 | 0.00 | 0.00 | 0.00 |
| Yaw command | 0.00 | 0.00 | 0.00 | 0.00 |

Evidence after calibration:

- log: `logs/remote_probe/rtt-20260713-191331.log`
- CSV: `logs/remote_probe/bench_jitter_drift_fresh.csv`
- analyzer command:

```powershell
python tools\blackbox_analyzer.py --mode rest --trim-start 3 --trim-end 3 --csv logs\remote_probe\bench_jitter_drift_fresh.csv
```

After-calibration analyzer summary:

| Axis | Raw mean dps | Filtered mean dps | Raw std dps | Filtered std dps | Drift dps | Verdict |
|---|---:|---:|---:|---:|---:|---|
| Roll | -0.09 | -0.09 | 2.21 | 1.22 | 1.70 | watch |
| Pitch | -0.27 | -0.27 | 1.55 | 0.84 | 0.00 | quiet |
| Yaw | 0.32 | 0.32 | 1.23 | 0.71 | -0.50 | quiet |

Command-input summary after calibration:

| Axis | Mean dps | Std dps | Peak-to-peak dps | Drift dps |
|---|---:|---:|---:|---:|
| Roll command | 0.00 | 0.00 | 0.00 | 0.00 |
| Pitch command | 0.00 | 0.00 | 0.00 | 0.00 |
| Yaw command | 0.00 | 0.00 | 0.00 | 0.00 |

Interpretation:

- RC command input is not the current source of stationary yaw/pitch drift.
- Centered commands were exactly zero in both captures.
- Before calibration, the stationary gyro carried about `14 dps` pitch bias and
  about `9 dps` yaw bias.
- Startup gyro-bias calibration reduced the stationary pitch/yaw means to near
  zero.
- Remaining roll noise is small and classified as `watch` only because filtered
  standard deviation is slightly above the current `1.0 dps` quiet threshold.
- Keep the FCU still for the first couple seconds after boot so startup bias
  calibration has a valid stationary window.

Status: pass for command-input jitter rejection and startup gyro-bias
calibration effectiveness.

### Test 7: Short Hover Yaw-Drift Flight Log

Purpose:

- check whether startup gyro-bias calibration reduced hands-off yaw drift
- compare commanded yaw, measured yaw, and yaw PID during an actual short flight
- decide whether the next diagnostic step should be yaw tune or another bench
  hardware/mixer check

Evidence:

- log: `logs/remote_probe/rtt-20260713-224405.log`
- CSV: `logs/remote_probe/flight_yaw_drift.csv`
- analyzer command:

```powershell
python tools\blackbox_analyzer.py logs\remote_probe\rtt-20260713-224405.log --mode auto --trim-start 3 --trim-end 3 --csv logs\remote_probe\flight_yaw_drift.csv
```

Whole-log analyzer summary:

| Axis | Filtered mean dps | Filtered std dps | Drift dps | Verdict |
|---|---:|---:|---:|---|
| Roll | 8.19 | 107.86 | 1.00 | motion |
| Pitch | -4.34 | 161.01 | 1.00 | motion |
| Yaw | 16.85 | 140.39 | 4.30 | motion |

Command and control summary:

| Signal | Mean | Std | Peak-to-peak |
|---|---:|---:|---:|
| Command yaw | -10.13 dps | 115.88 dps | 867 dps |
| PID yaw | - | 26.57 | 812 |
| Motor 1 | - | 367.93 | 1117 |
| Motor 2 | - | 381.22 | 1074 |
| Motor 3 | - | 397.80 | 1117 |
| Motor 4 | - | 352.22 | 1030 |

Additional slice read:

- in armed, throttle-on, near-centered yaw command slices, filtered yaw remained
  roughly `39 dps` while yaw PID applied opposite correction around `-10`
- this supports the user's observation that gyro-bias calibration helped but did
  not remove the remaining in-air yaw tendency

Interpretation:

- RC yaw command jitter is still not the main explanation.
- The remaining yaw looks like an in-air torque/authority/tune problem or a
  physical asymmetry, not a stationary gyro zero-rate issue.
- The next diagnostic firmware change is intentionally small: yaw P `0.30`, yaw
  I `0.02`, yaw D unchanged at `0.0`.
- Abort if yaw wag appears or maneuver instability gets worse.

Status: logged flight achieved; proceed only with short LOS characterization.

### Test 8: Yaw-I/P Diagnostic Flight And U-Turn Pitch Event

Purpose:

- test the small yaw tune change: yaw P `0.30`, yaw I `0.02`, yaw D `0.0`
- capture whether the stronger yaw correction reduces hands-off clockwise drift
- inspect the user's reported U-turn behavior where the aircraft pitched hard
  and then self-corrected for about a second

Evidence:

- log: `logs/remote_probe/rtt-20260713-230632.log`
- CSV: `logs/remote_probe/flight_yaw_i_p_test.csv`
- plots:
  - `logs/remote_probe/visualizations/flight_yaw_i_p_overview.png`
  - `logs/remote_probe/visualizations/flight_yaw_i_p_pitch_gyro_zoom.png`
  - `logs/remote_probe/visualizations/flight_yaw_i_p_pitch_pid_zoom.png`
  - `logs/remote_probe/visualizations/flight_yaw_i_p_rate_error_overview.png`
  - `logs/remote_probe/visualizations/flight_yaw_i_p_rate_error_pitch_zoom.png`
- analyzer command:

```powershell
python tools\blackbox_analyzer.py --mode auto --trim-start 3 --trim-end 3 --csv logs\remote_probe\flight_yaw_i_p_test.csv
```

Whole-log analyzer summary:

| Axis | Filtered mean dps | Filtered std dps | Drift dps | Verdict |
|---|---:|---:|---:|---|
| Roll | -4.48 | 66.08 | 4.30 | motion |
| Pitch | -0.42 | 59.54 | 1.20 | motion |
| Yaw | -10.08 | 71.85 | 0.80 | motion |

Command and control summary:

| Signal | Mean | Std | Peak-to-peak |
|---|---:|---:|---:|
| Command roll | -4.48 dps | 39.44 dps | 548 dps |
| Command pitch | 24.49 dps | 64.99 dps | 543 dps |
| Command yaw | -13.29 dps | 80.36 dps | 1012 dps |
| PID roll | - | 9.11 | 719 |
| PID pitch | - | 17.23 | 491 |
| PID yaw | - | 14.30 | 663 |

Centered-yaw, throttle-on slice:

- with `abs(cmd_yaw) <= 5 dps` and throttle at or above `500`, yaw command was
  centered but pitch/roll were still active enough that this was not a clean
  hands-off hover segment
- filtered yaw remained about `+36 dps`
- yaw PID averaged about `-15`, stronger than the previous run's roughly `-10`
  correction, but the measured centered-yaw drift did not clearly reduce

Pitch event notes:

- strongest pitch PID correction occurred around `59.63 s`
- at that point: `cmd_pitch = +275 dps`, `gyro_pitch = -366 dps`,
  `pid_pitch = +160`, throttle `680`
- strongest pitch gyro moment followed around `60.28 s`
- at that point: `cmd_pitch = +505 dps`, `gyro_pitch = +466 dps`,
  `pid_pitch = +10`, throttle `762`
- the event coincides with active roll, pitch, and yaw commands during the
  user's U-turn and landing sequence, not a quiet hands-off hover

Interpretation:

- The small yaw I/P change made the controller push harder against yaw, but did
  not prove that centered-yaw drift is fixed.
- The U-turn pitch behavior looks like aggressive maneuver coupling with a
  controller self-correction, not an obvious persistent pitch sign bug.
- Pitch PID did not saturate in this event; maximum observed pitch PID in the
  zoom was about `+160`.
- Do not increase yaw gains again from this evidence alone.

Status: useful logged diagnostic flight. Stop testing for the day; next useful
log would be a deliberately boring centered-stick hover segment.

### Test 9: Stationary Bias Recheck And Zero-Stick Hover Baseline

Purpose:

- confirm startup gyro-bias calibration was not the cause of the in-flight trim
  and yaw observations
- capture a cleaner zero-stick hover segment before changing yaw integral again

Stationary bias evidence:

- log: `logs/remote_probe/rtt-20260714-223924.log`
- CSV: `logs/remote_probe/still_gyro.csv`
- analyzer command:

```powershell
python tools\blackbox_analyzer.py --mode rest --trim-start 3 --trim-end 3 --csv logs\remote_probe\still_gyro.csv
```

Stationary result:

| Axis | Filtered mean dps | Filtered std dps | Drift dps | Verdict |
|---|---:|---:|---:|---|
| Roll | -0.24 | 1.17 | 0.40 | watch |
| Pitch | -0.23 | 0.80 | -0.10 | quiet |
| Yaw | 0.09 | 0.71 | -1.00 | quiet |

Interpretation:

- Startup gyro-bias calibration was effective on the bench.
- Disarmed PID and motor outputs stayed exactly zero.
- The sustained in-flight yaw tendency is not explained by stationary gyro
  zero-rate bias.

Zero-stick hover baseline:

- log: `logs/remote_probe/rtt-20260714-224537.log`
- CSV: `logs/remote_probe/still_gyro.csv` from that run
- tune: yaw P `0.30`, yaw I `0.02`, yaw D `0.0`
- last clean hop, throttle `>= 500`, all stick commands zero:

| Signal | Mean |
|---|---:|
| Throttle | 729.96 |
| Gyro yaw | +55.62 dps |
| PID yaw | -25.34 |
| Motor 1 | 748.95 |
| Motor 2 | 705.49 |
| Motor 3 | 761.64 |
| Motor 4 | 703.75 |

Interpretation:

- This was the cleanest evidence so far of a real steady positive yaw rate with
  centered sticks.
- The controller raised the `1/3` diagonal and lowered the `2/4` diagonal,
  which is the expected correction direction for the current mixer and yaw sign.
- Motor 3 was the most heavily commanded motor in this slice, with motor 1 next.

Status: yaw drift is real and persists with centered commands; proceed with a
small yaw-I diagnostic step rather than changing signs or motor mapping.

### Test 10: Yaw Integral Sweep

Purpose:

- determine whether the steady yaw drift responds to more yaw integral authority
- avoid raising yaw P unless evidence shows an actual rate-response problem

Yaw I `0.04` evidence:

- firmware tune: roll P/I/D `0.20 / 0.00 / 0.00`, pitch P/I/D
  `0.25 / 0.00 / 0.00`, yaw P/I/D `0.30 / 0.04 / 0.00`
- log: `logs/remote_probe/rtt-20260714-230458.log`
- CSV: `logs/remote_probe/yaw_i_004_hover.csv`
- throttle `>= 500`, centered yaw command:

| Signal | Mean |
|---|---:|
| Throttle | 761.97 |
| Gyro yaw | +17.41 dps |
| PID yaw | -13.59 |
| Motor 1 | 774.40 |
| Motor 2 | 746.73 |
| Motor 3 | 776.71 |
| Motor 4 | 750.03 |

Yaw I `0.06` evidence:

- firmware tune: yaw P/I/D `0.30 / 0.06 / 0.00`
- log: `logs/remote_probe/rtt-20260714-231137.log`
- CSV: `logs/remote_probe/yaw_i_006_hover.csv`
- throttle `>= 500`, centered yaw command:

| Signal | Mean |
|---|---:|
| Throttle | 765.63 |
| Gyro yaw | +35.84 dps |
| PID yaw | -19.98 |
| Motor 1 | 780.76 |
| Motor 2 | 744.20 |
| Motor 3 | 790.46 |
| Motor 4 | 747.10 |

Comparison:

| Yaw I | Centered-yaw rate | Yaw PID | Motor spread |
|---:|---:|---:|---:|
| 0.02 | +55.62 dps | -25.34 | 57.89 |
| 0.04 | +17.41 dps | -13.59 | 29.98 |
| 0.06 | +35.84 dps | -19.98 | 46.26 |

Interpretation:

- Yaw I `0.04` was the best value tested today.
- Yaw I `0.06` made the clean centered-yaw slice worse, despite a near-zero
  whole-log yaw mean.
- Do not raise yaw P from this evidence. The drift responded to integral, and
  excessive integral appears to create more coupling or poorer correction.

Status: firmware was reverted to yaw P/I/D `0.30 / 0.04 / 0.00`.

### Test 11: Conservative FPV Characterization On Yaw I 0.04

Purpose:

- verify that the `0.04` yaw-I tune remains manageable in a conservative FPV
  flight
- check for obvious saturation, yaw wag, or runaway motor spread under real
  pilot input

Evidence:

- log: `logs/remote_probe/rtt-20260714-231751.log`
- CSV: `logs/remote_probe/conservative_fpv_i004.csv`
- analyzer command:

```powershell
python tools\blackbox_analyzer.py --mode auto --trim-start 1 --trim-end 1 --csv logs\remote_probe\conservative_fpv_i004.csv
```

Whole-log summary:

| Axis | Filtered mean dps | Filtered std dps | Drift dps | Verdict |
|---|---:|---:|---:|---|
| Roll | 26.41 | 123.84 | 1.60 | motion |
| Pitch | -0.98 | 137.77 | 1.30 | motion |
| Yaw | 28.22 | 167.11 | -1.80 | motion |

Throttle-on flight summary:

- armed time: about `75 s`
- throttle `>= 500`: about `65 s`
- no motor went near zero in the throttle `>= 500` slice
- no motor saturated high in the throttle `>= 500` slice
- pilot report: aircraft flew pretty well and yaw felt manageable

Throttle `>= 500` motor means:

| Motor | Mean |
|---|---:|
| Motor 1 | 816.43 |
| Motor 2 | 810.55 |
| Motor 3 | 862.65 |
| Motor 4 | 782.88 |

Low-yaw-command slice:

- throttle `>= 500`, `abs(cmd_yaw) <= 5 dps`
- gyro yaw mean: about `+45.52 dps`
- yaw PID mean: about `-34.46`
- this slice still included roll/pitch commands and normal FPV motion, so do
  not tune from it as aggressively as the clean hover slices

Interpretation:

- Yaw I `0.04` is the best current flight tune.
- The aircraft is now FPV-flyable in conservative conditions, but this is still
  characterization, not a mature flight stack.
- Motor 3 remains the highest average output and motor 4 the lowest under
  throttle-on FPV flight, consistent with a persistent yaw/torque asymmetry or
  physical/ESC response difference.
- More gain tuning is lower priority than improving logging, freshness handling,
  arming behavior, and repeatability.

Status: stop tuning for the day. Keep yaw P/I/D `0.30 / 0.04 / 0.00` for the
next session unless fresh evidence says otherwise.

## Open Risks

High-priority risks before another free-flight attempt:

- startup gyro-bias calibration depends on the FCU being still for the first
  couple seconds after boot; movement during that window can leave residual bias
- O4 fan setup can shake the FCU; fan cooling is fine during setup, but any
  flash/reset/boot must be followed by a still calibration pause before moving
  the aircraft
- any future large stationary gyro mean after calibration must be treated as a
  stop condition
- one motor emitted smoke during bench testing; Test 5 did not reproduce
  abnormal physical behavior at capped low equal output, but the hardware remains
  suspect until repeated/current-checked runs are clean
- persistent yaw/torque asymmetry remains visible: motor 3 tends to command
  highest and motor 4 lowest during throttle-on flight
- yaw I `0.04` is flyable and improved drift, but yaw is not fully explained
- yaw I `0.06` was worse in the clean centered-yaw slice and should not be used
  as the next baseline
- restrained motor tests can drive the controller into saturation and obscure
  real vibration measurements
- arming-idle abort is target-validated; stale motor command rejection and
  RC/IMU/ADC freshness policies remain prototype-level

Do not treat the current props-off throttle test as proof of filter adequacy.
The test is dominated by asymmetric controller response.

## Tomorrow Start Here

Current status:

- the pitch-sign/motor-remap blockers were resolved enough for a stable LOS
  flight
- the lower-limit P-only tune is flyable, though not FPV-quality
- the main remaining flight issue is steady clockwise yaw drift with no user
  yaw input
- centered RC commands have been proven stable at `0.00 dps`
- startup gyro-bias calibration reduced stationary pitch/yaw gyro means from
  about `14/9 dps` to near zero
- a July 14 stationary bias recheck showed roll/pitch/yaw means near zero
- yaw I `0.04` reduced the clean centered-yaw hover drift from about
  `+55.62 dps` to about `+17.41 dps`
- yaw I `0.06` was worse in the clean centered-yaw slice
- the first conservative FPV characterization flight on yaw I `0.04` was
  manageable and did not show obvious motor saturation in normal throttle-on
  flight
- the current firmware includes RC rate deadband, startup gyro-bias calibration,
  yaw P `0.30`, and yaw I `0.04`

Recommended first session:

- inspect the motor/ESC that smoked earlier before applying flight power
- flash the current normal `blackbox_defmt` build with yaw P/I/D
  `0.30 / 0.04 / 0.00`
- keep the FCU still for the first couple seconds after boot so gyro-bias
  calibration completes on a stationary frame
- do a short disarmed still BB2 capture and confirm gyro means remain near zero
- if the O4 unit needs cooling, use the fan during setup/log start, but do not
  let fan vibration disturb the first couple seconds after FCU boot/reset
- start BB2 logging before the flight
- repeat conservative hover/FPV characterization only if hardware and link
  confidence are good
- stop immediately on hard oscillation, bounce, yaw wag, heat/smell, rough
  motor sound, or loss of control confidence
- if yaw wag appears or maneuver instability increases, land and keep/revert to
  the yaw I `0.04` baseline before changing motor/mixer signs

Hardware inspection before powering motors:

- spin each motor by hand and compare drag/noise
- check the smoked motor for burnt smell, discoloration, loose bell, damaged
  wires, or debris
- check ESC and solder joints for heat damage or loose connections
- if the motor feels different from the others, replace or isolate it before
  continuing vibration tests

Equal-motor vibration workflow:

```powershell
.\tools\remote_run.ps1 -Link Zero -Build -Features "blackbox_defmt bench_equal_motors"
```

```powershell
.\tools\pi_elf_sync.ps1 -Link Zero
```

```powershell
.\tools\pi_log_start.ps1 -Link Zero -Restart
```

Run a short restrained test:

- 10 seconds disarmed still
- arm normally
- 10 seconds at zero throttle
- 10 seconds very low throttle
- 10 seconds slightly higher throttle, still below the `250` cap
- note current draw at each throttle step if a current measurement is available
- disarm

Fetch and analyze:

```powershell
.\tools\pi_log_stop.ps1 -Link Zero
```

```powershell
.\tools\pi_log_fetch.ps1 -Link Zero
```

```powershell
python tools\blackbox_analyzer.py --mode rest --trim-start 3 --trim-end 3 --csv logs\remote_probe\equal_motor_vibration.csv
```

Expected read:

- `pid` should remain `0` on all axes
- `motors` should be equal while armed and throttled
- any rise in gyro std is motor/frame vibration, not PID/mixer correction
- current should be modest, smooth, and repeatable for each throttle step
- current is currently a manual/evidence signal unless the sensor scaling has
  been verified; do not rely on uncalibrated current as the only safety gate

## Flight-Trial Preflight Gate

Use this for the next conservative hover or very conservative FPV
characterization flight. The operator completed the previously open
motor/ESC inspection on 2026-07-20 and reports that the motor appears normal.

Normal flight-trial firmware build:

```powershell
.\tools\remote_run.ps1 -Link Zero -Build -Features blackbox_defmt
```

The normal firmware currently boots with the first-hop tuning profile:

| Axis | P | I | D |
|---|---:|---:|---:|
| Roll | 0.2 | 0.0 | 0.0 |
| Pitch | 0.25 | 0.0 | 0.0 |
| Yaw | 0.30 | 0.04 | 0.0 |

RC rate deadband is currently `8` raw channel counts around center.
Startup gyro-bias calibration averages the first stationary disarmed samples;
keep the FCU still for the first couple seconds after boot.

If the DJI O4 unit needs fan cooling, start the Pi logger while cooling it, then
move to the ground and fly promptly. Do not let fan vibration or handling disturb
the first couple seconds after any FCU flash/reset/boot.

Required checks before the next prop-on hover or conservative FPV flight:

- smoked motor/ESC inspected before power
- motor numbering verified against the physical frame
- gyro sign step test completed for roll, pitch, and yaw
- manual tilt correction direction verified with props removed
- no reinforcing correction observed on any axis
- disarmed still BB2 capture after boot shows gyro means near zero
- fresh `blackbox_defmt` build flashed and matching ELF synced to the Pi
- log capture started before the attempt if the harness is practical

Abort conditions:

- any motor/ESC smoke, hot smell, or rapid heating
- sudden current spike, current climbing at steady throttle, or current much
  higher than expected
- wrong motor spins for a given motor index
- gyro sign does not match the expected control convention
- manual tilt correction reinforces the disturbance
- filtered gyro shows a large steady mean while the frame is still
- yaw/roll vibration is severe before takeoff throttle
- steady yaw drift gets worse, yaw wag appears, or stronger maneuvers become
  less stable on the yaw I `0.04` baseline

Do not use `bench_equal_motors` for flight. It is a props-off measurement mode
only.

## Next Tests

### Test 0: Disarmed Still Gyro Bias Check

Goal:

- prove whether the gyro reports near-zero rates while the aircraft is physically
  still
- confirm startup gyro-bias calibration has completed correctly before any
  powered motor or hover test

Method:

- props removed
- drone disarmed
- place the drone on a stable bench and do not touch it
- keep the FCU still for the first couple seconds after boot
- log `BB2` for 20-30 seconds
- trim the first and last 3 seconds
- analyze with `--mode rest`

Command:

```powershell
python tools\blackbox_analyzer.py --mode rest --trim-start 3 --trim-end 3 --csv logs\remote_probe\still_gyro.csv
```

Pass criteria:

- raw and filtered gyro means are close to `0 dps` on roll, pitch, and yaw
- no axis shows a large steady rate while the frame is still
- centered `cmd10` values remain zero on roll, pitch, and yaw
- PID outputs stay near zero while disarmed

Fail action:

- if any axis has a large mean offset, debug the IMU/gyro data path before
  continuing to motor-output or flight tests

### Test A: Motor Numbering And Output Mapping

Goal:

- prove Betaflight Quad X logical `motor1..motor4` in firmware correspond to
  the expected physical motors
- avoid confusing physical FCU output pad numbers with logical motor numbering

Current Motor 1 command:

```powershell
.\tools\remote_run.ps1 -Link Cable -Build -Features "blackbox_defmt bench_motor1_only"
```

Current Motor 2 command:

```powershell
.\tools\remote_run.ps1 -Link Cable -Build -Features "blackbox_defmt bench_motor2_only"
```

Current Motor 3 command:

```powershell
.\tools\remote_run.ps1 -Link Cable -Build -Features "blackbox_defmt bench_motor3_only"
```

Current Motor 4 command:

```powershell
.\tools\remote_run.ps1 -Link Cable -Build -Features "blackbox_defmt bench_motor4_only"
```

Method:

- props removed
- inspect the previously smoked motor/ESC before applying ESC power
- flash the selected `bench_motor*_only` build
- sync the matching ELF to the Pi if logging
- arm normally
- slowly raise throttle just enough to identify which physical motor responds
- firmware caps the selected motor to `250`
- firmware logs only the selected motor field as nonzero
- other outputs are held at ESC low by the actuator-output task
- disarm immediately after identifying the motor
- record the physical position of the selected firmware/physical output

Current observation:

- commanding firmware/physical output 1 spun the front-left motor
- in the Betaflight Quad X diagram, that physical corner is labeled motor 4
- in the current Betaflight Quad X logical convention, that physical corner is
  motor 4
- commanding firmware/physical output 2 spun the rear-left/lower-left motor
- in the Betaflight Quad X diagram, that physical corner is labeled motor 3
- commanding firmware/physical output 3 spun the rear-right/lower-right motor
- in the Betaflight Quad X diagram, that physical corner is labeled motor 1
- commanding firmware/physical output 4 spun the front-right/upper-right motor
- in the Betaflight Quad X diagram, that physical corner is labeled motor 2
- these measurements define the FCU3 physical output order used by the
  Betaflight logical remap

Current Betaflight Quad X logical convention:

| Physical corner | Betaflight logical motor | FCU3 physical output |
|---|---:|---:|
| Rear-right | 1 | 3 |
| Front-right | 2 | 4 |
| Rear-left | 3 | 2 |
| Front-left | 4 | 1 |

Observation log:

| Test command | Observed physical corner | Betaflight diagram label | Status |
|---|---|---:|---|
| `bench_motor1_only` | Front-left | 4 | physical output 1 identified |
| `bench_motor2_only` | Rear-left / lower-left | 3 | physical output 2 identified |
| `bench_motor3_only` | Rear-right / lower-right | 1 | physical output 3 identified |
| `bench_motor4_only` | Front-right / upper-right | 2 | physical output 4 identified |

Measured physical output order:

| Firmware output | Physical corner | Current expected corner |
|---:|---|---|
| 1 | Front-left | Front-left |
| 2 | Rear-left | Front-right |
| 3 | Rear-right | Rear-right |
| 4 | Front-right | Rear-left |

Firmware remap applied:

| Logical mixer motor | Intended corner | Physical output |
|---:|---|---:|
| 1 | Rear-right | 3 |
| 2 | Front-right | 4 |
| 3 | Rear-left | 2 |
| 4 | Front-left | 1 |

Implementation:

- `MOTOR_OUTPUT_MAP = [3, 4, 2, 1]`
- normal BB2 motor fields now report physical output commands after this remap
- the single-motor bench features still command one physical output directly and
  remain useful for re-verifying the hardware map

Mapped logical-motor verification commands:

```powershell
.\tools\remote_run.ps1 -Link Cable -Build -Features "blackbox_defmt bench_logical_motor1_only"
```

Expected: rear-right / lower-right.

```powershell
.\tools\remote_run.ps1 -Link Cable -Build -Features "blackbox_defmt bench_logical_motor2_only"
```

Expected: front-right / upper-right.

```powershell
.\tools\remote_run.ps1 -Link Cable -Build -Features "blackbox_defmt bench_logical_motor3_only"
```

Expected: rear-left / lower-left.

```powershell
.\tools\remote_run.ps1 -Link Cable -Build -Features "blackbox_defmt bench_logical_motor4_only"
```

Expected: front-left / upper-left.

Mapped verification result:

- user previously re-ran the logical motor checks under the old
  front-left-first FerroWasp logical convention
- after switching firmware labels to Betaflight Quad X, re-run all four mapped
  logical motor checks before any further flight
- motor rotation directions must match the Betaflight Quad X diagram

Pass criteria:

- only one physical motor responds above low/stop output
- all four firmware outputs are mapped to physical corners
- the remapped normal motor commands are consistent with the Betaflight Quad X
  mixer convention
- rotation directions match the Betaflight Quad X diagram
- no smoke, hot smell, abnormal jitter, rough sound, or current spike
- every actuator-capable board must repeat this props-off order/direction check
  before flight after any board profile, output backend, motor-map, or wiring
  change

Output:

- update the board/motor mapping documentation with physical position for each
  motor index
- keep the current FerroWasp logical numbering for the MVP, with the physical
  output remap documented and tested

Status: pass after firmware remap.

### Test B: Manual Tilt Correction Direction

Goal:

- prove the controller increases the motors that would oppose a physical roll or
  pitch disturbance

Method:

- props removed
- arm normally at idle only, or use a safety-gated bench observer mode
- keep throttle low
- manually roll the frame right, left, pitch forward, pitch backward
- log `BB2` motor outputs and gyro values
- compare commanded motor changes against expected corrective torque

Pass criteria:

- right roll disturbance commands correction that would oppose right roll
- left roll disturbance commands correction that would oppose left roll
- nose-down pitch disturbance commands correction that would oppose nose-down
  pitch
- nose-up pitch disturbance commands correction that would oppose nose-up pitch
- no axis produces a reinforcing correction

Important:

- If a tilt produces reinforcing motor output, do not fly.

RC stick mapping issue found before flight:

- right stick right/roll-right increased the front motors and reduced the rear
  motors
- right stick forward/pitch-forward increased the right motors and reduced the
  left motors
- throttle direction was correct
- left stick right/yaw-right increased front-right and rear-left
- left stick left/yaw-left increased front-left and rear-right

Interpretation:

- roll and pitch stick inputs were swapped before the controller
- roll and pitch command signs were inverted relative to the expected motor
  response
- yaw response matched the expected yaw diagonal for the current mixer/motor
  rotation convention

Firmware fix:

- `RC_ROLL_CHANNEL_INDEX = 0`
- `RC_PITCH_CHANNEL_INDEX = 1`
- `RC_INVERT_ROLL = false`
- `RC_INVERT_PITCH = false`
- `RC_INVERT_YAW = false`

Expected props-off motor response after this fix:

| Stick input | Expected motor response |
|---|---|
| right stick right | front-left and rear-left increase |
| right stick left | front-right and rear-right increase |
| right stick forward | rear-left and rear-right increase |
| right stick back/down | front-left and front-right increase |
| left stick left | front-left and rear-right increase |
| left stick right | front-right and rear-left increase |

Observed props-off response after this fix:

- right stick right: left motors spun up
- right stick left: right motors spun up
- right stick forward/up: rear motors spun up
- right stick back/down: front motors spun up
- left stick left: front-left and rear-right spun up
- left stick right: front-right and rear-left spun up

Status: pass for props-off stick-to-motor response after RC mapping fix.

### Test C: FCU3 Gyro Sign Step Test

Goal:

- verify that positive/negative gyro rates align with the firmware control
  convention

Method:

- disarmed
- props removed
- log `BB2`
- rotate the frame by hand around one axis at a time
- annotate expected positive/negative direction in notes
- compare `raw10` and `gyro10` signs for roll, pitch, yaw

Pass criteria:

- each physical rotation direction maps to the expected control-axis sign
- filtered gyro follows raw gyro with expected one-sample delay

Observed result:

- pitch: elevating/nose-up motion produced positive pitch rate
- roll: clockwise roll from the FPV camera point of view produced positive roll
  rate
- yaw: clockwise yaw viewed from above produced positive yaw rate

Status: superseded by field tilt log. Positive physical nose-up pitch rate was
shown to drive reinforcing front-motor correction in the normal control path.
`CONTROL_PITCH_GYRO_SIGN` was inverted from `1` to `-1`; re-run this test and
the manual tilt correction test with props removed before any further prop-on
attempt.

### Test C2: FCU3 Field Tilt Log After Failed Takeoff Attempts

Log:

```text
logs/remote_probe/pi_20260712_165653_attach.log
```

Context:

- first prop-on takeoff attempts tended to front/back flip
- field props-off free-hand tilt test was run with large pitch, roll, yaw, and
  mixed movements
- log was converted to `logs/remote_probe/field_tilt_20260712.csv`

Key pitch result:

| Region | Gyro pitch | PID pitch | Front - rear motor average |
|---|---:|---:|---:|
| positive pitch dominant | `+1687 dps` | `-1879` | `+149` |
| negative pitch dominant | `-1614 dps` | `+1800` | `-143` |

Interpretation:

- with the documented convention that physical nose-up is positive pitch, the
  controller increased the front motors during nose-up motion
- that reinforces the pitch disturbance and explains front/back flips on
  takeoff
- this is not a tuning issue

Firmware patch:

- changed `CONTROL_PITCH_GYRO_SIGN` from `1` to `-1`
- added a controller regression test requiring a mapped nose-up disturbance to
  command rear motors up

Status: fail before patch; patch applied. Must pass props-off manual tilt
correction before flight.

### Test C3: FCU3 After-Patch Field Tilt Log

Log:

```text
logs/remote_probe/pi_20260712_170645_attach.log
```

CSV:

```text
logs/remote_probe/field_tilt_after_pitch_sign_20260712.csv
```

Context:

- `CONTROL_PITCH_GYRO_SIGN = -1` firmware was flashed
- props-off field tilt log captured `BB2` data
- 29,649 samples, 400 Hz estimated rate, 18,129 armed samples

Controller-frame pitch result:

| Region | Logged gyro pitch | PID pitch | Front - rear motor average |
|---|---:|---:|---:|
| positive pitch dominant | `+1471 dps` | `-1734` | `+193` |
| negative pitch dominant | `-1491 dps` | `+1751` | `-189` |

Interpretation:

- logged negative pitch now raises rear motors relative to front
- logged positive pitch raises front motors relative to rear
- this is consistent with the intended patch if physical nose-up now appears as
  negative pitch in the viewer/log

Status: useful after-patch evidence, but do not clear for prop-on flight until
the physical props-off tilt check is observed directly: nose-up raises rear,
nose-down raises front.

### Test C4: Foxeer First-Hop Pitch Positive Feedback

The first Foxeer prop-on departure on 2026-07-22 attempted an immediate
forward flip. The recovered archive is
`logs/foxeer-hop-front-flip.fwbb`, SHA-256
`DB6E82BFE6E6902BAB26658C4BC9F2FDB3B71FE6E8FF1C05E1ACB0C9AC348537`.
It contains 16,974 CRC-valid pages and 84,827 records; flight IDs 12 and 13
contain the powered departure evidence.

At flight-12 sequence 11,407, commanded pitch was zero, controller pitch was
`-241.0 dps`, pitch PID was `+60`, throttle was `550`, and physical
M1/M2/M3/M4 were `624/487/597/492`. Increasing rear M1/M3 while the aircraft
was moving nose-down reinforced the disturbance. Flight 13 independently
shows the same polarity.

Status: fail. The pre-fix image is withdrawn from flight use. This is a pitch
sign defect, not a gain-tuning result.

### Test C5: Foxeer Corrected Props-Off Opposition

Run both SWD phases with all propellers removed:

1. With ESC power disconnected, physical nose-up must remain positive body
   pitch while controller pitch is negative; nose-down must be the inverse.
   Roll and yaw signs must match between body and controller.
2. Reflash `blackbox_defmt`, return level, and connect ESC power. Under the
   normal DShot mixer, nose-up must produce positive pitch PID and raise rear
   M1/M3; nose-down must produce negative pitch PID and raise front M2/M4. Roll
   and yaw must also oppose hand motion.

Any reinforcing correction, unexplained motor mapping, failure to stop on
disarm, or automatic rearm is an immediate fail/no-flight result. Retain the
exact ELF hash plus RTT and onboard logs. A second hop remains blocked until
both phases pass and the evidence is reviewed.

Observed 2026-07-22 result: pass. The unpowered image
`C639C8BD3476E8415632644E970D4BAB3B42417FD9C624428B6D5D343D37F7FA`
showed the intended physical/controller pitch inversion while preserving roll
and yaw. Powered image
`FF6606EFACC55C9C88CCDE5EC044C0CB3B3881A5229319C26820621654DD136F`
then produced opposing PID and motor-pair polarity for pitch 594/594, roll
175/175, and yaw 72/72 selected centred-stick motion samples. DShot and
telemetry counters remained clean, and explicit disarm retained four zero
outputs. Retained RTT log:
`logs/terminal_embed/20260722_212914_rtt.log`.

### Test D: Clean Motor Vibration Test With PID Disabled

Goal:

- measure motor-induced vibration without the rate controller fighting the bench

Required implementation:

- implemented as the `bench_equal_motors` firmware feature
- the feature commands equal motor outputs through the existing
  safety/actuator-output task
- normal arming and actuator gating must remain active
- no direct motor peripheral access outside actuator output
- PID/mixer is disabled for this test mode
- requested throttle is capped to `250` PWM-style units while the smoke event is
  unresolved

Build:

```powershell
.\tools\remote_run.ps1 -Link Cable -Build -Features "blackbox_defmt bench_equal_motors"
```

Method:

- props removed
- inspect the motor that smoked before applying ESC power again
- drone restrained
- connect an external current measurement if available
- arm normally
- step equal motor output through fixed low levels, staying below the firmware
  cap
- note current draw at each level and watch for spikes or rising current at
  steady throttle
- hold each level long enough for analysis
- log `BB2`
- analyze with `--mode rest`
- stop immediately if any motor or ESC smokes, smells hot, jitters abnormally, or
  heats rapidly
- stop immediately if current jumps sharply, climbs at steady throttle, or is
  clearly high for props-off low-throttle operation

Pass criteria:

- filtered gyro standard deviation and dominant frequency are known at each
  motor level
- no severe vibration peak reaches the PID input at the intended initial hover
  power range
- motor outputs remain symmetric because they are commanded symmetrically
- current draw is smooth and plausible at each equal-motor throttle level, if
  measured

### Test E: Repeat Props-Off Throttle Test After Sign/Mixer Fix

Goal:

- confirm that the earlier motor asymmetry is resolved

Method:

- props removed
- normal arming path
- low throttle only
- log `BB2`
- analyze motor outputs and PID response

Pass criteria:

- no unexpected saturation of one side or diagonal
- motor outputs remain plausible for a restrained low-throttle bench test
- PID output does not rail persistently in one direction

### Test F: First Successful LOS Hover

Context:

- after the pitch-sign fix and props-off tilt confirmation, the aircraft first
  lifted almost vertically but oscillated hard
- roll/pitch/yaw P gains were progressively reduced to probe the lower limit
- the lower-limit tune was:
  - roll P `0.2`
  - pitch P `0.25`
  - yaw P `0.1`
  - all I/D `0.0`

Observed result:

- aircraft flew in LOS and was stable enough to control
- handling was not FPV-quality but was flyable
- no immediate front/back flip tendency remained
- no BB2 log was captured for this flight

Remaining issue:

- aircraft steadily yawed clockwise without yaw stick input
- pilot needed continuous correction to fly straight

Follow-up patch:

- added RC rate deadband of `8` raw channel counts around center
- restored yaw P from `0.1` to `0.25`
- kept roll P `0.2`, pitch P `0.25`, and all I/D zero

Logged follow-up:

- startup gyro-bias calibration reduced stationary yaw offset and the user's
  short flight report said yawing was less
- flight log `rtt-20260713-224405.log` still showed positive yaw rate during
  centered-yaw, throttle-on slices
- later July 14 testing found yaw P/I/D `0.30 / 0.04 / 0.00` to be the best
  current baseline

Status: first stable LOS flight and first conservative FPV characterization
flight achieved. Current baseline is yaw P/I/D `0.30 / 0.04 / 0.00`.

### Test G: Standard DShot600 Controlled Outdoor Flight

Date: 2026-07-20

Configuration:

- standard FCU3 DShot600 mixed-control image;
- telemetry-qualified idle arming through the safety-owned actuator path;
- roll P/I/D `0.20 / 0.00 / 0.00`;
- pitch P/I/D `0.25 / 0.00 / 0.00`;
- yaw P/I/D `0.30 / 0.04 / 0.00`;
- RC rate deadband `8`;
- verified Betaflight logical motor map and rotation directions.

Operator-observed result:

- the flight went well and the aircraft remained controllable through strong
  maneuvers;
- the previous unwanted yawing was not observed;
- pitch authority felt lower than desired;
- no new BB2 flight capture was supplied with this report.

Follow-up:

- preserve this configuration as the known flight baseline;
- test pitch P `0.30` as the next isolated change, leaving every other gain,
  filter, mapping, rate scale, mixer setting, and output limit unchanged;
- if tracking improves without oscillation, bounce-back, excessive noise, or
  heat, consider `0.35` only as a later separate step;
- inspect command-versus-gyro pitch tracking and motor headroom so mixer
  saturation, center of gravity, or directional thrust imbalance are not
  mistaken for insufficient P;
- the current OSD step is `0.1`, so do not use it for the intended first
  `0.05` increment without changing the menu granularity.

Status: successful experimental first flight of the standard DShot/legacy
telemetry baseline. Logic-analyzer timing, jitter, polarity-margin, and
cross-timer phase evidence remain open.

## Current Flight Gate

The original control-direction and motor-identity blockers have been cleared,
and the standard DShot600 image completed a controlled outdoor flight:

- motor numbering and rotation were verified under the Betaflight Quad X
  logical labels
- physical output order is consistent with the mixer via
  `MOTOR_OUTPUT_MAP = [3, 4, 2, 1]`
- pitch correction sign was fixed after field-log evidence
- props-off tilt correction looked promising after the patch
- lower-limit P-only tune produced a stable, flyable LOS flight
- yaw I `0.04` reduced clean centered-yaw hover drift, and the unwanted yawing
  was absent by operator report during the latest flight
- telemetry-qualified arming, RC-loss stop, explicit disarm, and arm-high reset
  interlocks passed their powered checkpoints

Keep subsequent flights experimental and bounded until:

- the pitch-authority observation is characterized with an isolated P step and
  preferably a BB2 flight capture
- DShot waveform width, jitter, M4 polarity margin, and TIM1/TIM8 phase are
  measured independently
- the remaining stale-IMU, total-command-loss deadline, and ADC-freshness
  policies are addressed before more ambitious testing

## SPSC Motor Command Target Checkpoint

The active mixer and all equal/selected-motor bench modes now send motor values
through the safety-owned `MotorCmd` SPSC queue. RTIC actuator spawns contain
only a wake command. The actuator owner drains to the newest entry and rejects
missing or more-than-20-ms-old commands to low output.

Normal props-off check with actuators unpowered:

1. Flash the normal build.
2. Qualify RC with arm low and throttle zero.
3. Arm normally and confirm `SYSTEM ARMED`.
4. Move throttle through a few positions and confirm IMU/heartbeat/OSD remain
   healthy.
5. Confirm no `motor command queue full`, `Actuator command wake rejected`,
   `motor command queue empty`, or `stale motor command` warning.
6. Disarm and confirm `SYSTEM DISARMED`.

Stale-command injection check:

```powershell
cd apps/stm32f405-flight
cargo embed --features bench_motor_cmd_stale_rejection
```

1. Keep actuators unpowered, qualify RC, and arm normally.
2. The first armed control command is deliberately timestamped 21 ms old.
3. Confirm exactly one `Actuator command refused: stale motor command seq 1`
   warning.
4. Confirm no queue-overflow or rejected-wake warning and that IMU, heartbeat,
   and RC/OSD activity continue.
5. Disarm normally.

This proves rejection of an expired command presented to actuator output. It
does not prove detection of a total loss of future control-loop wakes; an
independent actuator deadline watchdog remains future work.

Injected target result on 2026-07-18: passed with actuators unpowered.

- the normal BLHeli low/idle sequence completed and reached `SYSTEM ARMED`;
- sequence 1 was rejected exactly once at an observed age of 21 ms;
- no queue-overflow or rejected-wake warning appeared;
- IMU sequence continued from 9,612 through 12,815 after the rejection,
  consistent with the intended approximately 800 Hz rate;
- the startup SBUS parser error recovered through healthy-frame qualification
  before arming.

The normal-build no-warning checkpoint remains before this work package is
fully target-validated.

## Foxeer USB RC-Tuning Checkpoint

Purpose: prove that ordinary stick-feel experiments no longer require an SWD
reflash while preserving the disarmed-only configuration boundary.

Keep propellers removed throughout this checkpoint.

1. Install one Foxeer `flash_blackbox` image containing the persisted Actual
   Rates schema, then disconnect the debugger if desired.
2. With the aircraft disarmed, capture the starting configuration:

   ```powershell
   python tools\ferrowasp_storage.py --port COM6 config-show
   ```

3. Confirm unsafe or malformed values are rejected, for example
   `rc_deadband 12.5`, `roll_expo 1.1`, or a maximum rate below its current
   center rate. Confirm `config-show` still reports the previous values.
4. Arm normally with propellers removed and issue one harmless `config-set`.
   Confirm the FCU returns `ERR config changes disabled while armed`, then
   disarm.
5. Stage and save a deliberately distinguishable but bounded props-off
   profile. Use two short armed blackbox captures with full roll-stick input
   to prove that the logged roll command reaches the selected maximum:
   first `150 deg/s`, then `350 deg/s`.
6. Apply the second profile with `config-save` and repeat without rebooting.
   This is the immediate-runtime-application check.
7. Power-cycle the FCU, run `config-show`, and confirm the second profile
   survived. This is the SPI-NOR persistence and legacy-config migration
   check.
8. Restore and save the flight baseline before installing propellers:

   - deadband `8`;
   - roll/pitch center `70 deg/s`, max `300 deg/s`, expo `0.50`;
   - yaw center `70 deg/s`, max `200 deg/s`, expo `0.50`;
   - roll/pitch/yaw P `1.0 / 1.0 / 2.0`;
   - every I and D value `0.0`.

9. Run `config-show` once more and retain its output with the test log.

Pass criteria:

- all configuration writes are refused while armed;
- invalid values do not replace the staged configuration;
- a successful disarmed save changes subsequent RC commands without reboot;
- saved values survive a cold boot;
- PID and newer RC fields survive each other's save paths;
- the documented baseline is restored and verified before flight.

## Foxeer Selective Blackbox Download and Boot Grouping

Purpose: prove that the field tool can retrieve one flight without transferring
the complete append-only archive, and that new captures retain explicit
power-on grouping.

Keep propellers removed. Use the normal `flash_blackbox` image.

1. Boot once and record two short armed/disarmed sessions.
2. While disarmed, run:

   ```powershell
   python tools\ferrowasp_storage.py --port COM6 flights
   ```

   Confirm both new flight IDs appear under one new `boot N` heading. Older
   pre-marker flights may correctly remain under `boot unknown`.
3. Cold-power-cycle the FCU and record one more short armed/disarmed session.
   Run `flights` again and confirm the new flight begins the next boot group.
4. Download only that flight:

   ```powershell
   python tools\ferrowasp_storage.py --port COM6 read `
     --flight-id latest `
     --output logs\foxeer-selective-latest.fwbb
   ```

   Confirm its page count is the selected flight's count, not total used pages,
   then analyze it with `blackbox_analyzer.py`.
5. Interrupt a second selected-flight transfer, then repeat the identical
   command with `--resume`. Confirm it continues from the validated local page
   count.
6. As a negative check, try to resume that file while selecting another
   numeric flight ID. Confirm the tool refuses the mismatch without appending.

Pass criteria:

- same-boot flights are grouped together and the first post-power-cycle flight
  starts a new group;
- pre-marker history is labeled unknown rather than guessed;
- `latest` and numeric selection transfer only their contiguous page ranges;
- every downloaded/resumed page passes magic, version, CRC, flight-ID, and
  per-flight sequence validation;
- reads remain refused while armed and no logging or actuator behavior changes.
