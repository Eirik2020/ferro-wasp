# FerroWasp Active Work Handoff

Last updated: 2026-09-24

## Current State - 2026-09-24

### FerroForge adoption candidate - flown, every gate passed

Foxeer runs on `ferroforge::app!`: every task except `usb_fs` and
`flash_manager_task` is an instance of a `ferrowasp-stm32f4-tasks` definition,
including the whole safety and actuator path. Bodies moved verbatim;
priorities and bindings are unchanged. It has flown.

Every gate passed on candidate `a921ffe` (binary `16f6e8ed`): SW-COMMON-001,
BUILD-FOX-001, BENCH-COMMON-001, BENCH-FOX-USB-001, BENCH-FOX-001, then
PREFLIGHT-FOX-001 and FLIGHT-FOX-001 flown 2026-09-24 and accepted by operator
decision. Records under `testing/evidence/runs/2026/09/`; BENCH-FOX-001 keeps
both a `fail` and the `pass` that supersedes it, `__01` for the reasoning.

**The adoption question is answered.** Measured pre- against post-conversion
over ~133k armed samples: identical control law, identical loop timing, and
zero sequence gaps, repeats or CRC failures in ~10 MB of logs - including
through flight 22's 1592 deg/s cartwheel with the mixer saturated. No
oscillation in either era. Detail is in the run records; logs are under
`logs/ferroforge-flights/` and `logs/preconversion-flights/`, gitignored.
Command tracking was never evaluated, because every flight with stick input
ended in deliberate ground contact; one ordinary hop would close it.

FerroForge 0.3.0 is published and `main` pushed, so the path override is gone
and this repo builds against the registry. That changes the image, so tag
`foxeer-candidate-16f6e8ed` is historical and any future flight re-gates.
Left: convert `usb_fs` and `flash_manager_task` shaped by Foxeer alone, and
retire `tools/rtic-app-builder`, whose phase 6 entry condition was this
flight. Branches: `ferrowasp-cleanup`, `foxeer-post-flight-work`,
`ferrowasp-configurator-gui`.

Two open bugs carried forward. Neither can stop a running motor - current
sense drives only OSD and MSP, and `EscManager::is_faulted` has one consumer,
a log line - and neither is accepted behaviour.

1. Battery current reads a constant `0.1 A` with four motors at 6300..7700
   eRPM. `centiamps = adc_mv * 10000 / 70 / 10` puts raw PC1 near `1 mV`, the
   noise floor, so the fault is upstream of the scale-70 change this candidate
   adopted, which could never have fixed it. Next: read `adc_current_mv` from
   the USB debug status under load - still ~1 mV means the sense input, not
   the math.
2. The ESC telemetry manager latched faulted after a single response timeout
   for logical M4 (one miss in 10132 requests, zero CRC failures) during an
   armed RC-loss stop, and both later arm attempts then correctly aborted on
   idle telemetry qualification. `esc_manager.rs` and `blheli_telemetry.rs`
   are byte-identical to pre-conversion; the moved UART plumbing in
   `ferrowasp-stm32f4-tasks/src/esc.rs` is new, so a conversion-induced
   dropped response is not excluded by code identity alone.

A latch also costs per-motor eRPM logging for the rest of that power cycle.
The bench run does **not** independently reproduce the ESC-only power-cycle
bug below - that latch was already set 100 s before the battery was
reconnected - but avoid ESC-only power cycles with USB attached regardless.

Motor identity is confirmed unchanged by the conversion, in code and by the
operator's roll and pitch differential response; see the `BENCH-FOX-001`
record.

Third open bug, pre-existing and cosmetic: the first storage-CLI command after
each USB port open is rejected once with `ERR invalid command`, then succeeds
on retry. `CommandParser` in `crates/ferrowasp-tasks/src/flash_storage.rs`
accumulates a line with no reset across port open, so a stray byte corrupts
the first line and the parse error clears the buffer.

Closed: the `UART4 RX free-buffer pool exhausted on IDLE` warning was noise on
an unterminated line, absent throughout the powered run with the VTX
connected.

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
[`archive/CODEX_ACTIVE_WORK_HISTORY_2026-07-27.md`](archive/CODEX_ACTIVE_WORK_HISTORY_2026-07-27.md).
Use that archive for provenance only; this file is the live work handoff.
