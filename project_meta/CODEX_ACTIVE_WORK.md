# FerroWasp Active Work Handoff

Last updated: 2026-09-18

## Current State - 2026-09-18

### FerroForge adoption candidate - hardware gates pending

Foxeer now runs on `ferroforge::app!`: every task except `usb_fs` and
`flash_manager_task` is an instance of a `ferrowasp-stm32f4-tasks`
definition, including the whole safety and actuator path. Bodies moved
verbatim; priorities and bindings are unchanged. None of it has run on
hardware.

Candidate: revision `a921ffe`, clean tree, default features only
(`board-foxeer-f405-v2`). DFU image
`logs/foxeer-candidates/20260918T211248Z-a921ffe-FerroWaspFoxeerF405V2.bin`,
152,664 bytes, SHA-256
`16f6e8ed493d9002be700317b2c78c7a7265028d7e567cab8f82027aae578435`.

- [x] `SW-COMMON-001` and `BUILD-FOX-001` passed; run records under
  `testing/evidence/runs/2026/09/`.
- [ ] `BENCH-COMMON-001` - unpowered boot and idle.
- [ ] `BENCH-FOX-USB-001` - unpowered USB RC configuration.
- [ ] `BENCH-FOX-001` - powered props-off exact-image gate.
- [ ] `PREFLIGHT-FOX-001`, then `FLIGHT-FOX-001`.

Watch in `BENCH-FOX-001`, first exercised on this image: arming and abort
paths, DShot service and DMA completion, control-loop correction signs, RC
loss, and ADC cell voltage and current (the latched cell detection below).
Avoid ESC-only power cycles with USB attached - the open bug below.

Open observation, deferred to the VTX check: a 1-minute USB-only run of the
converted image logged `UART4 RX free-buffer pool exhausted on IDLE` twice
with no OSD/VTX attached; one pre-conversion run did not. UART4 RX floats
without the VTX, so noise can deliver junk frames faster than `osd_refresh`
recycles the four buffers, and the idle path drops and warns where the DMA
path panics. Re-check with the VTX connected and powered during
`BENCH-FOX-001`; only then is it worth comparing against a pre-conversion
image (built from 18521d5, SHA-256
`9737292a4c0f4a6444710dc3fcfce79dd440e025938655aba299e9ef66043f19`).

Open decisions before FCU3 follows: the FCU3 drift table in FerroForge's
`docs/src/ferro-wasp-adoption.md`, above all the differing
`ActuatorHardware` validation.

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

The 2026-07-28 Foxeer OSD report found zero current and no cell voltage. The
next candidate uses target values (VBAT 110, current 70/offset zero) and
latched 4.30 V cell detection. Target comparison remains required.

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

Open logging-format TODO:

- Record every fresh per-motor legacy-UART eRPM observation at its actual
  bounded rate, including time/age, identity, freshness, and transport health;
  never present repeated stale values as new control-rate data.
- Log timestamped body-frame accelerometer data, range/clipping, and bounded
  peaks for offline crash-detector development. Validate against landings,
  maneuvers, gusts, and impacts before allowing any safety-state effect.
- Make every recorded flight self-describing by storing the exact active
  configuration at its flight boundary and after any accepted runtime change.
  In the 2026-07-27 session, BB2 `PID/error` identifies flights 29-32 as P
  `1/1/2` and 33-34 as `2.5/2.5/2`; future readers must expose configuration
  directly and warn when it is absent.
- Adopt ULog as FerroWasp's standard persisted flight-log format so recorded
  data can use the existing PX4 logging, telemetry, visualization, and analysis
  ecosystem. Define stable FerroWasp message schemas and units, board/firmware
  metadata, parameters, timestamps, dropout reporting, and flight boundaries.
  Keep the hard real-time producer allocation-free and bounded: control tasks
  should publish fixed-size typed records to the existing bounded logging
  boundary, while a lower-priority owner performs ULog framing and flash I/O.
  Provide host tests with known-good ULog readers and a migration/conversion
  path for retained BB2/`.fwbb` evidence before replacing the current format.
- Migration paths now exist in both `tools/fwbb_to_ulog.py` and the packaged
  native FerroConfigurator. Both validate every `.fwbb` page, convert exactly
  one selected flight, and emit schema version 1 of the compact
  `ferrowasp_rate_control` topic. Synthetic format/timing/dropout tests pass;
  the native output is byte-identical to the Python reference for retained
  flight 27, and PyULog 1.2.3 previously accepted the reference converter's
  real Foxeer flight 11 output as uncorrupted. Native firmware ULog framing,
  parameters, richer metadata, and replacement of `.fwbb` remain open work.
- Onboard BB2 retrieval is now flight-aware. The USB host tool can catalog
  contiguous flight page ranges, download `--flight-id latest` or a numeric
  ID, and resume only after validating the selected flight ID, per-flight page
  sequence, and every page CRC. A backward-compatible record flag marks the
  first successfully assembled record of the first recorded flight after each
  MCU boot, allowing `flights` to group new captures by power-on session.
  Existing pages remain readable and are deliberately labeled `boot unknown`
  rather than grouped using ambiguous wrapping MCU timestamps.

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
