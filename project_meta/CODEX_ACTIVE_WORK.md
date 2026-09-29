# FerroWasp Active Work Handoff

Last updated: 2026-09-29

## Current State - 2026-09-29

### Flown candidate, and what has changed since

FLIGHT-FOX-001 flew candidate `a921ffe` (binary `16f6e8ed`) on
`ferroforge::app!` on 2026-09-24, and every gate passed. Its record is tag
`foxeer-candidate-16f6e8ed` in FerroForge's repository plus the binaries in
`logs/foxeer-candidates/`, never a rebuild; the runs are under
`testing/evidence/runs/2026/09/`. Pre- against post-conversion over ~133k
armed samples: identical control law and loop timing, zero sequence gaps or
CRC failures. Command tracking was never evaluated. Detail is in the
2026-09-24 history.

The image has changed since, so **the next flight re-gates**:

- Foxeer's IMU and control loop run at 2 kHz (`65af671`), and the rate is a
  per-board fact (`cb03082`): FCU3 is back at 800/400. Bench: 2000.0 Hz with
  no shortfall and no stale samples.
- The gyro filter is stored as a corner, config schema 3, `imu_lpf_hz`
  (`1111e69`). The device reports its own loop rate (`52713f3`).
- The blackbox store drains a bounded batch per pass (`1c74368`): 250
  pages/s, up from 84. The flown 400 Hz needed 80 - earlier logs fit by five
  percent. **At 2 kHz, `log_rate_divisor` 1 asks 400 pages/s and drops about
  750 records/s: set divisor 2 (200 pages/s) before the next flight, or raise
  the store's ceiling.** `bench_blackbox` records while disarmed; it fills the
  log in minutes and belongs only in a bench image.
- The storage CLI's first-command rejection is fixed (`cbc892b`), not flown.
- `CHIPSERIE` is gone (`e841718`); the image hashed the same without it.

An armed props-off run is owed first: the 2 kHz blackbox has only run
disarmed.

### FerroForge 0.4

FerroForge 0.4.0 is published; this branch still builds against 0.3. Moving
is mechanical but touches about 40 sites - see its book's adoption chapter:
`systick-64bit`, `Mono::now()` timestamps become `u64` where this repository's
APIs take `u32`, `u64::from` on `u32` durations, and typed literals in the
Foxeer app. Each narrowed timestamp must keep its old wrap before it flies. A
copy migrated end to end builds and passes the host tests. 0.4 also lets a
config entry carry a doc comment, which `cb03082` could not, and adds task
groups and hardware-timer monotonics; a 1 MHz timer monotonic would lift
blackbox `timestamp_us` off its 1 ms resolution, but is a timer-assignment
decision.

Left of the adoption: convert `usb_fs` and `flash_manager_task`, shaped by
Foxeer alone, and retire `tools/rtic-app-builder`.

### Open bugs

Neither can stop a running motor - current sense drives only OSD and MSP,
and `EscManager::is_faulted` has one consumer, a log line - and neither is
accepted behaviour.

1. Battery current reads a constant `0.1 A` with four motors at 6300..7700
   eRPM. `centiamps = adc_mv * 10000 / 70 / 10` puts raw PC1 near `1 mV`, the
   noise floor, so the fault is upstream of the scale. Next: read
   `adc_current_mv` from the USB debug status under load - still ~1 mV means
   the sense input, not the math.
2. The ESC telemetry manager latched faulted after a single response timeout
   for logical M4 (one miss in 10132 requests, zero CRC failures) during an
   armed RC-loss stop, and both later arm attempts correctly aborted on idle
   telemetry qualification. `esc_manager.rs` and `blheli_telemetry.rs` are
   byte-identical to pre-conversion; the moved UART plumbing in
   `ferrowasp-stm32f4-tasks/src/esc.rs` is new, so a conversion-induced
   dropped response is not excluded. A latch also costs per-motor eRPM
   logging for the rest of that power cycle.

### Carried forward from 2026-07-27

Open ESC-only power-cycle recovery bug:

- With USB supplying the FCU, removing and restoring only ESC/battery power
  resets the ESCs while the legacy-telemetry manager remains live. Its first
  missing response latches telemetry fail-closed until an FCU reboot. Later
  arm attempts spin temporary DShot idle but correctly abort because no fresh
  four-ESC eRPM qualification can complete. Preserve the fail-closed outcome,
  but add a disarmed, explicit ARM-low-gated telemetry reinitialization with
  the normal five-second boot delay and a fresh four-ESC qualification. Do not
  clear this state automatically while armed or with ARM high.

The shared RC curve now follows Betaflight Actual Rates, using the official
three-option model rather than its more advanced profiles and adjustment
features. Roll and pitch use center sensitivity `70 deg/s`, maximum rate
`300 deg/s`, and expo `0.50`; yaw uses `70 deg/s`, `200 deg/s`, and `0.50`.
These are the shared defaults and are now disarmed-only USB configuration
values. A successful `config-save` applies them without a reboot or reflash.
References: Betaflight's
[Rate Calculator](https://www.betaflight.com/docs/wiki/guides/current/Rate-Calculator)
and official
[`applyActualRates`](https://github.com/betaflight/betaflight/blob/master/src/main/fc/rc.c)
implementation.

Foxeer remains P-only: roll/pitch/yaw `2.5/2.5/2.0`, all I/D zero. A
2026-07-27 confined-area hop and flight handled substantially better at `2.5`;
the operator classifies it flyable, not well tuned. BB2 `PID/error` identifies
flights 29-32 as `1/1/2` and 33-34 as `2.5/2.5/2`. Flight 34 had no mixer
rescaling or high-frequency oscillation. Flight 33 ended in a 1.09-second
authority-exhausting event the operator attributes to a strong gust; current
logs cannot distinguish gust/tumble, contact, or thrust-system failure. Repeat
in calmer conditions. Roll P `3.0` remains withdrawn after a separate logged
`10-13 Hz` autonomous oscillation.

The same `2.5/2.5/2.0`, all-I/D-zero profile is now the golden Foxeer
fresh-storage/default-reset baseline. Existing valid stored configuration
continues to win across a firmware update. FCU3 retains its separate legacy
initial profile.

Half of the 2026-07-28 Foxeer OSD report is resolved: with the VTX powered,
OSD `VBAT` and `CELL` agreed with a multimeter at 6 cells and 4.18 V per
cell, so the target values and latched 4.30 V detection work. Current does
not; see stop condition 1 above.

FerroConfigurator has no actuator/arming authority. It verifies ROM-DFU
images, exposes 21 USB parameters with readback, resumes selected downloads,
and converts BB2 to minimal ULog. Its bundled image requires exact-image
props-off acceptance.

Flights 12-20 in `logs/foxeer-rear-battery-hop.fwbb` exposed retained
yaw-integral state across disarm/rearm boundaries. The shared reset fix is now
implemented, but I must remain zero until target evidence proves that every
disarm, RC loss, aborted arm, and subsequent rearm starts with clean controller
state.

Open IMU-calibration TODO:

- Add a user-requested, disarmed-only IMU calibration workflow for a stationary
  aircraft placed on known level ground. It must reject motion or excessive
  tilt, use a bounded sample window, report success/failure, and persist the
  resulting calibration atomically with integrity protection. Define clearly
  which gyro bias, accelerometer level offsets, and attitude trim are stored,
  how stored calibration is validated at boot, and whether automatic boot gyro
  calibration remains a fallback or a verification step. Expose the operation
  through the existing USB configuration CLI without granting any actuator
  authority.

Open logging-format TODO: see [`LOGGING_FORMAT.md`](LOGGING_FORMAT.md). Six
items, all open: per-motor eRPM at its real bounded rate, body-frame
accelerometer logging, self-describing flights carrying their configuration,
ULog adoption, and the migration and retrieval work already done.

Open flight-mode TODO:

- Add a simple self-level attitude mode after the rate-command and controller
  reset changes are flight-verified. Rate mode correctly controls angular rate
  but cannot remove a sustained tilt or position drift caused by trim, CG, or
  wind. Keep the outer-loop authority bounded and preserve the existing safety
  and actuator boundary.

## Historical handoff

Completed checkpoints and superseded state were moved to
[`archive/CODEX_ACTIVE_WORK_HISTORY_THROUGH_2026-07-22.md`](archive/CODEX_ACTIVE_WORK_HISTORY_THROUGH_2026-07-22.md)
and
[`archive/CODEX_ACTIVE_WORK_HISTORY_2026-07-27.md`](archive/CODEX_ACTIVE_WORK_HISTORY_2026-07-27.md)
and
[`archive/CODEX_ACTIVE_WORK_HISTORY_2026-09-24.md`](archive/CODEX_ACTIVE_WORK_HISTORY_2026-09-24.md).
Use that archive for provenance only; this file is the live work handoff.
