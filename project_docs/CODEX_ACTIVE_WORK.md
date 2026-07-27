# FerroWasp Active Work Handoff

Last updated: 2026-07-27

## Current State - 2026-07-27

### Today's flight-issue worklist

- [x] Replace the linear `+/-1000 deg/s` RC mapping with one small,
  Betaflight-referenced Actual Rates model shared by FCU3 and Foxeer.
- [x] Use independently readable center sensitivity, maximum rate, and expo
  values; retain the existing bounded deadband and command signs.
- [x] Add a shared control-state reset and invoke it throughout every unarmed
  control interval in both apps. Host tests prove that integral,
  derivative/filter history, previous setpoint/measurement state, throttle,
  and mixed outputs are cleared.
- [x] Add analyzer support for selecting the longest contiguous throttle-on
  flight window, warn when multiple flight IDs are combined, and keep
  frequency analysis coverage through 100 Hz on long logs.
- [x] Reconcile the flight cheatsheet with the current P-only baseline and new
  analysis option.
- [x] Extend the disarmed-only USB configuration with persisted RC deadband,
  per-axis Actual Rates center/max/expo values, and a `config-show` helper.
  Legacy 44-byte stored configurations load with the current safe RC defaults
  and migrate on the next successful save.
- [x] Run the exact FCU3 and Foxeer embedded build matrix and retain the
  command results with this change.
- [x] Props-off validate USB RC tuning: reject invalid values, save a temporary
  profile, verify immediate application without reboot, cold-boot persistence,
  and restoration of the documented baseline before flight.
- [x] Props-off verify RC directions, full-stick limits, arming/disarming, and
  motor opposition on the exact Foxeer flight image before another hop.
- [x] Perform one conservative logged hop with the new RC curve and inspect
  command tracking, oscillation frequency, and mixer headroom before changing
  gains again.

The USB RC configuration gate passed on 2026-07-27: an invalid roll expo was
rejected, the `60 / 250 / 0.4` temporary roll profile applied and persisted
over a cold reboot, and the complete documented baseline was restored and
confirmed after a second reboot. The exact candidate used for the subsequent
Foxeer checks was programmed through SWD at 17:45 local time with
`flash_blackbox`, SHA-256
`08D03F07AFD768CB387FDF7D41BEA1D3927DA83A75674BCEB02243F254A7411A`.
`probe-rs` completed programming in 9.04 seconds and RTT confirmed the
Foxeer application boot, valid 48 MHz PLL domain, ICM42688-P `WHO_AM_I 71`,
supported 16 MiB SPI2 flash (`ef:40:18`), enabled runtime IMU arming health,
and PA10 telemetry-qualified DShot. Retained transcript:
`logs/terminal_embed/20260727_174525_rtt.log`. The final powered props-off
check also
produced new flash captures before and after reboot: flight 23 is boot session
5 (`33121..33909`) and flight 24 is boot session 6 (`33910..35188`). This
confirms boot-session markers and append-only flight cataloguing for those
captures; their contents still need the normal selected-flight download and
analysis before being used as flight evidence.

Powered props-off test steps 4 and 5 also passed by operator observation on
2026-07-27: the arm-high interlock/recovery behavior remained correct and the
normal guarded DShot arm path passed. The appended boot-6 capture contains
flights 25-27. Flight 27 was selectively downloaded as
`logs/foxeer-props-off-20260727-194828.fwbb` (128 CRC-valid pages, 636
records, SHA-256
`5D18E2EB5AEB40D7FD0C77317F61F9FDEE41963DC957D1A1BFD4357E1C1EB28B`). Its
633 analyzed samples are contiguous at 400 Hz, with no missing BB2 frames,
no repeated IMU samples, a 1,011.4 Hz estimated IMU rate, and no control
timestamp over 3 ms. The captured interval recorded zero stick commands and
zero logged motor commands, so it corroborates timing/recording integrity but
does not independently prove the observed temporary idle/motor response.

The final exact-image props-off sequence passed on 2026-07-27. All four motors
had the verified physical location and rotation, imposed roll/pitch/yaw motion
produced corrective rather than reinforcing response, explicit disarm stopped
the motors immediately, and the previously repeated RC-loss stop/rearm
interlock remained accepted. Flight 28 was selectively retained as
`logs/foxeer-props-off-final-20260727-201354.fwbb` (4,593 CRC-valid pages,
22,963 records, SHA-256
`F5F8B29AD662A1871548EC6F5B506F5CB00CA0A40C64661D39C931552B72BCDD`). The
selected 22,960-sample analysis has no missing BB2 frames or repeated IMU
samples, an estimated 1,011.6 Hz IMU rate, and no control timestamp above
3 ms. The 0.8/0.8/1.2 Hz roll/pitch/yaw peaks are consistent with the
operator-imposed movement; no axis was classified as high-frequency/noise-like.
Stick commands were intentionally centred during this correction test, so the
zero command fields are expected. The separately tracked USB-held,
ESC-only-power-cycle telemetry-recovery bug remains open and is explicitly
outside this normal FCU-and-ESC power-up gate.

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
- The first migration path now exists as `tools/fwbb_to_ulog.py`. It validates
  every `.fwbb` page through the existing parser, converts exactly one selected
  flight, and emits schema version 1 of the compact
  `ferrowasp_rate_control` topic. Synthetic format/timing/dropout tests pass,
  and PyULog 1.2.3 independently accepted the converted real Foxeer flight 11
  archive as uncorrupted with all 24,612 samples and its 61.527-second
  timeline. Native firmware ULog framing, parameters, richer metadata, and
  replacement of `.fwbb` remain open work.
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
[`archive/CODEX_ACTIVE_WORK_HISTORY_THROUGH_2026-07-22.md`](archive/CODEX_ACTIVE_WORK_HISTORY_THROUGH_2026-07-22.md).
Use that archive for provenance only; this file is the live work handoff.
