# Remaining Work

What is left to build, grouped by area. Evidence owed for changed code is not
listed here: `reqtool check` and `reqtool gate` report it. What the blackbox
must carry is owned by [the logging format](LOGGING_FORMAT.md).

## Safety and Flight

- **Actuator deadline.** Detect the complete absence of future motor commands
  independently of the control loop, and stop the motors.
- **Freshness and fault injection.** IMU initialization, gyro-bias calibration,
  and freshness are pre-arm prerequisites without negative target fault
  injection. RC link quality and ADC/OSD freshness policy are prototype-level.
- **DShot timing.** Pulse width, jitter, polarity margin, cross-timer phase, and
  physical stop latency need logic-analyzer measurement; a flight does not
  establish them.
- **Bidirectional DShot and RPM filter** (ADR-0045 to ADR-0048), Foxeer
  first. Pieces, in order:
  - the stored bidirectional-DShot setting, in place of the props-off
    `bench_bidirectional_dshot` feature, and FerroConfigurator's Motors tab
    for it and the motor pole count;
  - `ferrowasp-flight::rpm_filter` in the control loop, with its stored
    configuration;
  - the qualification source moved to bidirectional eRPM;
  - extended telemetry: the enable command before guarded idle;
  - blackbox eRPM, reply-loss and extended-telemetry fields;
  - `ferrowasp-flight::dynamic_idle` (ADR-0049) on a mixer output floor,
    enabled only after the RPM filter has target evidence. Its stale hold
    (provisionally 100 ms) and decay (500 ms) are set from logged reply-loss
    data.

  Then, on target:
  - the installed BLHeli_32 version and the EDT values it actually sends;
  - logic-analyzer captures of both directions, including the turnaround:
    the line released and pulled high, and the reply inside the 71 us window;
  - reply loss under SPI1, ADC and USART1 DMA load;
  - the filter's cost per loop;
  - props-off arming;
  - spectral evidence before the gyro low-pass corner moves;
  - a stale-reply fault-injection test for dynamic idle.

  Open: the TBS Lucid H7 receive path.
- **ESC-only power cycle.** PR #16 resumes ESC telemetry while disarmed after a
  quiet window. Cycling only ESC power while USB powers the board is untested
  with it; until a props-off run shows the first arm fails closed and a later
  arm qualifies on fresh eRPM from every ESC, do not do it.
- **I-term.** Foxeer stays P-only (2.5/2.5/2.0) and I stays zero until target
  evidence shows every disarm, RC loss, aborted arm, and rearm starts with clean
  controller state. Roll P 3.0 stays withdrawn after a 10-13 Hz oscillation.
  Command tracking has never been evaluated.
- **IMU calibration.** Accelerometer level calibration is built (ADR-0044);
  the gyro still calibrates at every boot. Storing gyro bias, and whether boot
  calibration then becomes a check, remains open. A self-level mode should
  require a calibrated level.
- **Self-level mode.** A bounded attitude outer loop over the rate loop, after
  the rate-command and controller-reset changes are flight-verified, keeping
  the safety and actuator boundary.
- **ADC after a DMA error.** `dma_adc1` logs a DMA fault and returns without
  restarting the transfer, so battery voltage freezes at its last value with
  no fault raised. It was seen frozen on the Foxeer's bidirectional DShot
  bench image. The fix (restart, or a stale-voltage fault) is a user decision.
- **Battery current** reads a constant 0.1 A on Foxeer. The firmware matches
  Betaflight's FOXEERF405V2 target (PC1, scale 70, offset 0), so the sense
  wiring is next; current is not a safety input until calibrated.

## Requirements and Architecture

- **Safety-critical architecture** (ADR-0054 to ADR-0064): critical and
  non-critical crates, SPSC links, the border roles in `ferrowasp-border` (the
  coordinator, the observer and its ledger, the Health Manager) with their
  heartbeats, the fault classes and the hard-fault latch, the Betaflight-style
  failsafe, the independent watchdog, Actuator Output's arming confirmation
  and arm-state cross-check, the OSD and telemetry warning on a stalled
  ledger, message contracts, the gyro-synced loop, `ferrowasp-config` and the
  new crate map. Not built. The redesign comes before the next hardware
  gate run; its order, scope and open items are in the
  [requirements design](requirements/design.md#17-order-of-work).
- **The `app!` parser** in `ferroforge-contracts`, and a FerroForge release, so
  board requirements fingerprint the tasks they name instead of the whole app
  closure.
- **`reqtool`**: the analysis-level checks, and running tests behind opt-in
  features, so they can be evidence.

## Logging and Analysis

- **Foxeer blackbox rate.** At the 2 kHz loop, `log_rate_divisor` 1 needs about
  400 pages/s. The drain fix that reached that rate on the Lucid's SD card has
  not run on Foxeer's SPI-NOR; measure it there, or default the divisor to 2.
- **Periodic record loss on Foxeer.** Flights 1-13 (older firmware, logged at
  2 kHz with divisor 2) lose exactly one record in six: four 2-tick steps, then
  one 4-tick step, every time. Check whether current firmware still does it
  before relying on uniform sampling.
- **Timestamp resolution.** Blackbox `timestamp_us` has 1 ms resolution from
  the 1 kHz monotonic; use `control_sequence` as the time base. A 1 MHz timer
  monotonic would fix it, and is a timer-assignment decision for the user.
- **Command-to-eRPM characterization**, offline, from synchronized command,
  fresh eRPM, and battery voltage under steady-state qualifiers: a bounded
  monotonic per-motor LUT with source-flight provenance, validated across pack
  voltage, load, temperature, and manoeuvring before any use as feedforward.
  Never close a fast RPM loop around sequential legacy telemetry.
- **Crash and stall classification**, offline first: an armed acceleration
  impulse followed by a commanded motor giving fresh, valid eRPM below its
  expectation for a bounded time. Missing telemetry is distinct from valid low
  eRPM. The pilot's later disarm is a validation label, never a detector input.
  Test throttle cuts, low-command motors, hard landings, prop unloading, ESC
  desync, and telemetry loss for false positives. It gains no safety effect
  until validated.
- **RTT channels.** Replace single-channel `defmt-rtt` with `rtt-target`:
  channel 0 `defmt`, channel 1 compact binary telemetry copied at low priority.
  Neither blocks the control loop; a full buffer increments a drop counter.

## Boards and Platform

- **Foxeer status LEDs.** The shared `status_leds` task (green heartbeat, blue
  armed) runs on the TBS Lucid H7 only; wire it on Foxeer to bring the golden
  app level.
- **Chip-family backends.** Board assembly has left both backends, and DShot
  lanes are named by each board. What remains is added when a board needs it:
  F4 DShot covers only the advanced timers (TIM1, TIM8) and the Foxeer's four
  DMA routes; the H7 covers TIM2-TIM5. A board whose motors use both of the
  H743's 32-bit timers (the Lucid V3: TIM2 and TIM5) needs the microsecond
  timebase moved to a 16-bit timer with wrap extension.
- **Board data from existing targets.** Generate a board's data crate from its
  ArduPilot or Betaflight target definition.
- **Host-testable flash manager.** The configuration save
  (`ferrowasp-flight::config_save`), the log erase, sync-ledger mark and
  scratch test (`storage_maintenance`), the flight log writer (`log_writer`),
  the storage command line (`storage_commands`) and the MSPv2 handler
  (`rpc_commands`) are host-tested; the task keeps startup recovery and the
  calibration and reboot glue as wiring. None of these has run on hardware
  since its extraction except the configuration save, maintenance and scratch
  test; a `bench_blackbox` run needs the BOOT button to leave.
- **SITL**, deferred by the user: a `ferrowasp-sitl` crate, with RTIC task bodies
  moved into pure step functions.
- **TBS Lucid H7.** The bench board is dead. A replacement sold now may be V3
  (ArduPilot TBS_LUCID_H7V3: motors on TIM2/TIM5, RC on UART4, no USART1), which
  needs a board variant.
- **Configurator flashing** (ADR-0043). The GUI's Firmware tab and Reboot
  button have run only against the browser mock, not a desktop build on a
  board. Deferred with the Lucid: the H743's jump address (`0x1FF09800`, from
  AN2606) is unverified on hardware, and FerroConfigurator has no Lucid board
  profile.
- **FerroForge adoption.** Retire `tools/rtic-app-builder`. FerroForge 0.4
  expands a group whose members are imported in one brace, `pub use
  crate::{a, b};`, into an empty group without an error, so
  `ferrowasp-stm32-tasks::groups` imports each member on its own line; the fix
  belongs in FerroForge.

## Tooling and Environment

- **Flight report rate.** `tools/flight_report.py` converts sequence numbers to
  time at a fixed 400 Hz, so on Foxeer's 2 kHz loop its durations and trims are
  wrong unless `--sample-rate` is given. Measure the rate from the log, as
  `blackbox_analyzer.py` does.
- **Reference container.** After the `.devcontainer` relocation, run
  `tools/dev/compose-with-ssh-agent.sh config` and
  `tools/dev/check-rebuilt-container.sh`; confirm the VS Code and Compose entry
  points both authenticate to GitHub through the forwarded agent; replace
  `seccomp=unconfined` with a minimal reviewed profile if Docker and WSL allow;
  and write recovery steps for WSL or Docker upgrades.
