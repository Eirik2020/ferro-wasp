# FerroWasp Active Work History - 2026-07-27

Archived 2026-09-18 from `project_meta/CODEX_ACTIVE_WORK.md`, section
`Current State - 2026-07-27`. It holds the completed 2026-07-27 worklist and
the dated gate reports for that day's candidate, verbatim. The open
ESC-only power-cycle bug, the RC-rate and P-only baselines, the withdrawn
roll P, the zero-I rule and the open TODOs from the same section still bind
and remain in the active handoff. Use this file for provenance only.

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
