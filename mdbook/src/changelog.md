# Changelog

This page records notable FerroWasp changes without implying that a firmware
artifact is stable, airworthy, or production-ready.

## Unreleased

### Added

- Log sync. The firmware lists the flights no host has stored, a host
  acknowledges each one after downloading it, and `logs erase-synced` erases
  only when every flight is acknowledged. A sync ledger in the flash's last
  4 KiB sector records the acknowledgements, so the log region ends one sector
  lower. FerroConfigurator's `blackbox sync` and the GUI's Sync button run it.
- A `configurator` serial function: the text command line on a UART, for a
  Bluetooth serial module. Foxeer binds it to `uart3` (PC10/PC11) by default.
- TBS Lucid H7 support on a new STM32H743 backend, with microSD log storage
  written back in 8 KiB windows, heartbeat and armed LEDs, and SBUS on the
  digital VTX connector's `uart1`. It has run on the bench, not flown.
- An MPU-6000 IMU driver, detected on SPI1 alongside the ICM-42688-P and
  MPU6500.
- Serial ports bound to functions at boot from saved configuration, named by
  the chip's UART number, with `config ports` and `config bind` in
  FerroConfigurator.
- CRSF receivers (ExpressLRS, Crossfire) with battery telemetry, and a
  configurable stick order and arm channel.
- A firmware-gated props-off motor test, ESC direction setting, and a saved
  motor order, from FerroConfigurator or the USB command line.
- FerroConfigurator GUI tabs, a pre-arm checklist, a receiver panel with a
  live stick readout, and a rates curve.
- ULog conversion (`convert`) measures the control rate from the log instead
  of assuming 400 Hz, times records by control tick, and accepts records that
  share a millisecond timestamp.
- `convert --csv` in FerroConfigurator: one flight as CSV for MATLAB, with
  units in the column names, time from the control-tick count at the rate
  measured from the log, and dropped-record gaps reported.
- Accelerometer level calibration: `imu calibrate` on a still, level craft
  saves where the accelerometer reads level, and the attitude estimate uses
  it. FerroConfigurator's Setup tab has a Calibrate button, and `imu
  calibrate` and `imu level` commands. Stored configuration version 7.
- `reboot` and `reboot dfu` on the USB command line, disarmed only: the
  second restarts the board into the STM32 ROM bootloader. FerroConfigurator's
  `flash` uses it, so a board running FerroWasp is flashed without the BOOT
  button, and `device reboot [--dfu]` gives both directly. The GUI gains a
  Firmware tab and a Reboot button. Bench-tested on the Foxeer from the CLI;
  the TBS Lucid H7's entry is not yet tested.
- Requirement-based test evidence: each result verifies named requirements
  and carries a fingerprint derived from the code they cover, so it stands
  while that code is unchanged, on any board. `tools/reqtool` reports what is
  verified, stale or owed, and `tools/bench_rig.py` runs the boot and idle
  gate on any board over SWD.
- Bidirectional DShot on the Foxeer, in the props-off
  `bench_bidirectional_dshot` image only: each lane reads its ESC's reply
  after every frame, giving per-motor eRPM and reply counts. UART telemetry
  still qualifies arming.
- `esc` and `build` on the USB command line, and `device info` showing both:
  each output's UART and reply eRPM, and the commit, dirty tree and features
  the image was built from. FerroConfigurator's `bench esc` samples `esc` to
  CSV, and `motor spin --props-off` idles one motor under the motor-test
  lease while sampling it.

### Changed

- FerroConfigurator is the one reader of the onboard log format.
  `tools/blackbox_analyzer.py` analyzes its `convert --csv` export, and the
  duplicate Python tools `tools/ferrowasp_storage.py` and
  `tools/fwbb_to_ulog.py` are removed; their commands are FerroConfigurator's
  `blackbox` and `convert`.

- Arming is refused while a computer is connected: a USB host has the board
  configured, or the configurator link was used in the last 2 seconds. The
  configurator's pre-arm checklist shows it as "USB unplugged".

- Crate layout. `ferrowasp-tasks` is now `ferrowasp-flight` (portable flight
  logic) and `ferrowasp-stm32f4-tasks` is `ferrowasp-stm32-tasks` (RTIC task
  bodies for every STM32 board). The HAL-neutral half of `ferrowasp-stm32f4`
  moved to a new `ferrowasp-stm32`, which the H7 backend now uses instead of
  the F4 crate; `ferrowasp-stm32f4` keeps only the F4 HAL code.
- Firmware is composed with FerroForge 0.4, and timestamps are 64-bit
  throughout, so no internal timestamp wraps.
- Foxeer's IMU and control loop run at 2 kHz; the loop rate is a per-board
  fact, and the gyro filter is stored as a corner frequency.
- Each board's physical facts live in a host-tested data crate,
  `firmware/<board>/board`, and one flash manager serves every board.
- ESC telemetry resumes after a timeout once the line has been quiet while
  disarmed, backing off to a 4 s window, instead of staying off until reboot.
- One isolated stale IMU tick is tolerated; two in a row still disarm.
- DShot frames follow the control loop: each accepted motor command sends
  one frame set, so commands reach the ESCs at the 2 kHz loop rate instead of
  one in four. A fallback keeps frames going at no less than 500 Hz while
  disarmed, in guarded idle, or if the loop stops.
- The documentation is two books: this public one, and an unpublished
  internal one for maintainers.

### Fixed

- The flash task no longer accepts a log record it cannot store, so log
  pages are not lost when the page assembler is full.
- A torn log page ends its flight instead of corrupting it, and an aborted
  erase rescans the log.
- Storage commands are answered while storage is unavailable, instead of
  timing out.
- The MPU-6000 leaves a gap between initialization transactions; a faster
  core started the next one too soon.
- The IMU parser runs above the UART transports, as designed.
- `terminal_embed.py` reports a failed flash as failed.
- FerroConfigurator names the connected board instead of assuming Foxeer, and
  the blackbox analyzer measures the control rate from the log.

### Removed

- The FerroWasp FCU3 app (`firmware/stm32f405-flight`), obsolete since
  2026-09-22. It is no longer built; its retained evidence stays in the
  project archive.
- The PowerShell wrappers for the Raspberry Pi remote-debug gateway, whose
  FerroDebugger counterpart was superseded by FerroConfigurator.

## 0.1.0 - 2026-07-28

First public experimental pre-release.

### Added

- Isolated RTIC applications for FerroWasp FCU3, Foxeer F405 V2, and
  NUCLEO-F401RE.
- Allocation-free STM32F4 sensor, serial, DMA, DShot600, ESC-telemetry, USB,
  configuration-storage, and blackbox support.
- The isolated FerroConfigurator desktop workspace for Foxeer flashing,
  configuration, blackbox download, and ULog conversion.
- Bounded host-side repository, evidence, and RTIC-boundary checks.

### Changed

- Foxeer F405 V2 is now the golden flight application.
- RTIC `main.rs` files are thin wiring shells; reusable mechanisms live in
  shared crates, while physical board facts remain in each app's `src/board/`.
- DShot600 is the standard flight ESC protocol. RC PWM remains available for
  servo and auxiliary-output use.
- Foxeer USB, onboard flash storage, blackbox logging, and ESC telemetry are
  standard board services rather than optional app-local implementations.

### Fixed

- Strengthened SBUS startup/freshness handling, guarded DShot arming and stop
  behavior, synchronized TIM1/TIM8 output, and bounded ESC-telemetry
  association.
- Corrected FCU3 motor mapping, output polarity, scheduling cadence, and
  roll/pitch control direction issues found during prototype validation.

### Removed

- Removed the standalone `ferrowasp-bsp` crate. Board facts now live with each
  isolated app, while reusable STM32F4 behavior lives in
  `ferrowasp-stm32f4`.

## Development History

- 2025-12: initial repository and SBUS parsing.
- 2026-02 to 2026-04: STM32F4 bring-up, DMA transports, sensors, actuator
  experiments, and control-loop iteration.
- 2026-05 to 2026-06: mixer, arming, OSD, safety documentation, and reusable
  control foundations.
- 2026-07: isolated board apps, bounded STM32F4 mechanisms, DShot600 and ESC
  telemetry promotion, onboard logging/configuration, and the first controlled
  prototype-flight reports.

Current support and limitations belong in [Current Support](current_support.md);
planned work belongs in the [Roadmap](roadmap.md); test evidence remains in the
internal evidence system.
