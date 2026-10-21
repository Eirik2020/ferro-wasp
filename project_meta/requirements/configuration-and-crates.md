# Requirements Design: Configuration, Classifications and Crates

Sections 14 to 16 of [the requirements design](design.md), which
maps every section to its chapter. The ADRs it names are the binding
records; this chapter carries the reasoning behind them.

## 14. Stored configuration

ADR-0059.

Storage is untrusted input; validation is the trust boundary, the same pattern
as the SBUS parser behind Command Input.

```
BOOT     flash driver ─raw page─▶ init: decode → validate(config, board) → select slot or default
         (non-critical)                (safety-critical, HAL-free, host-tested)

RUNTIME  CLI / configurator / OSD ─Request─▶ coordinator: validate(config', board)
                                               → live keys: arm inhibit → participants stage/commit → release
                                               → boot-only keys: stored as "pending until reboot"
         save ─▶ flash manager saves the config owner's next-boot image, read from the ledger
```

`StoredConfig` held tuning (gains, rates), `log_rate_divisor`,
`serial_bindings`, `rc_protocol` and `accel_level`. Tuning flies the craft and
the RC protocol decides whether RC works, so the values matter; the storage
mechanism need not be safety-critical.

| Safety-critical (core, HAL-free) | Non-critical |
|---|---|
| page decode and schema migration | SPI NOR driver |
| `validate(config, board)` | save sequencing (`config_save`) |
| slot selection at boot, and fallback | CLI, configurator and OSD front ends |
| the config owner (active and pending images) | |
| participants applying live keys | |

**One `validate()`** runs at boot and in the coordinator, so a change the
coordinator accepts can never be rejected at the next boot (the craft working
until its next power-up would be a nasty failure). It checks each value's spec
and the rules across fields and against the board (from the host-tested board
data crate): the motor map is a permutation; the loop-rate rule; the
actuator-feedback source is valid for the board; RC is bound and no UART is
bound twice; the RC protocol is supported on its port.

**Saving bypasses the coordinator.** Saving changes nothing safety-critical;
it copies validated state into storage. The config owner publishes the
next-boot image (live keys as active, boot-only keys as pending) as one
ledger topic with a sequence number. The command line asks the flash manager
directly (non-critical to non-critical); it writes that image with the
existing two-slot scheme (erase the spare slot, program, read back, adopt only
on a match) and refuses while armed (blackbox logging already writes flash in
flight, so this keeps config changes on the ground, not timing). A requester
asks for "save", never supplies bytes, so it cannot persist an unvalidated
config; if the save path corrupts the page, boot decode and `validate()`
reject it. The save path needs only ordinary non-critical tests (slot scheme,
readback, power loss mid-save).

**Boot fallback.**

| Case | Boot does | Arming |
|---|---|---|
| fresh board: both slots erased | board default | allowed, expected on first boot |
| newest slot damaged (CRC or decode), older valid | older slot, the pilot's previous save | refused until acknowledged |
| decodes but fails `validate()` (e.g. a firmware update lowered the tested loop rate) | older slot, then default; never a clamped value, which would be a guessed state (W2) | refused until acknowledged |
| older schema version | migrates; new keys take defaults | allowed, reported |

A silent fallback could fly different gains, or a different motor map (a flip
on takeoff). Of three options (refuse on any fallback; refuse on damage or
validation failure only; never refuse, Betaflight-like), the second was
chosen. The safety master owns the refusal; the pilot clears it with
`AcknowledgeConfigFallback` through the coordinator, disarmed only. It recurs
on every boot until a good save overwrites the damaged slot; the two-slot
scheme writes the slot not in use, which is the damaged one, so one save fixes
it.

**`ferrowasp-config`** is its own safety-critical, HAL-free crate: schema,
value specs, `validate()`, page encode and decode, migration, and the
motor-map type (used by the mixer, by actuator authority and the bench test,
and validated by config). Decoding untrusted bytes is the trust boundary, so
the page format cannot live in the non-critical protocol crate: a
safety-critical crate cannot depend on a non-critical one. The flash manager
and `ferrowasp-protocol` depend on it.

---

## 15. Classifications

ADR-0054.

| Item | Class | Reason |
|---|---|---|
| actuator feedback (formerly ESC telemetry) | safety-critical | eRPM qualifies arming; the RPM filter and dynamic idle feed control |
| stored config | split | safety-critical gate, non-critical store (section 14) |
| attitude estimator and `accel_level` | safety-critical | stabilized mode, which needs an angle estimate, is next |
| `system_reset`, `system_bootloader` | safety-critical | code that can end the flight firmware; only the safety master calls it |
| `adc` (battery voltage and current) | non-critical for now | display, RC telemetry and the blackbox only; a low-voltage failsafe or arming check would reclassify it |
| `pwm_config` (servo PWM) | non-critical for now | unused on a quad |

**The estimator.** `accel_level.rs` itself said "the rate loop never reads
the accelerometer, so this changes the estimated attitude, and nothing that
flies the craft today". The estimate (`imu_angle_integrator`) ran inside the
control loop, read only by the OSD horizon and blackbox. Making it
non-critical (calibration and estimator on the non-critical side, reclassified
when angle mode arrives) was proposed; the user chose safety-critical now,
because stabilized mode is coming soon. Consequences: the estimator stays in
the control loop with its own requirements; accelerometer calibration moves
from the flash manager into the safety-critical core as a coordinator
transaction (arm inhibit; the estimator's owner applies ADR-0044's stillness,
lean and magnitude checks to its own samples; stage the offsets; commit;
release); `ImuSample`'s accel fields become safety-critical; the OSD reads
attitude from the ledger. `gyro_bias_calibrator` was always safety-critical,
because the rate loop subtracts gyro bias.

The one estimator requirement now: in rate mode, an estimator fault must not
affect motor output. Angle-mode fault handling is decided when angle mode is
built; further detail now is scope creep.

---

## 16. Crate layout

ADR-0060.

**Splitting the core.** Bench evidence decides granularity, because unit
evidence costs nothing to re-run.

| Option | Shape | Verdict |
|---|---|---|
| A. one core crate | `flight-core` | any core change re-owes all safety-critical bench evidence |
| B. along bench-evidence lines (chosen) | `safety`, control, `input`, `feedback` | a tuning or filter change leaves the arming evidence standing, and the reverse |
| C. one crate per module | about a dozen | finest invalidation, little gain over B |

The user then split control into general control and flight-controller-
specific code:

| `ferrowasp-control` (general, reusable maths) | `ferrowasp-flight-control` (how FerroWasp flies) |
|---|---|
| PID | control loop task |
| biquad, low-pass and notch filters | rate profiles and expo, the tuning profile |
| notch-bank maths for the RPM filter | mixer and how the motor map is applied |
| attitude estimator algorithm | dynamic idle, throttle handling |
| gyro bias estimation, accel-level calibration maths | RPM filter wiring: which harmonics feed which notches |
| frame rotation maths | stale-sample handling, rate-dependent setup |

The general crate changes rarely and is proven almost entirely by unit tests;
flight and bench evidence attach to the flight-controller crate.

**Where tasks live.** Task code is target-only: `ferrowasp-stm32-tasks`
depended on `rtic` with `thumbv7-backend`, `cortex-m` and `defmt`, while the
logic crates are host-tested.

| Option | Shape | Verdict |
|---|---|---|
| A. tasks in their subsystem crate behind a `tasks` feature (chosen) | `#[cfg(feature = "tasks")] mod tasks` | fewest crates; a subsystem is one unit of evidence; tests cover both feature sets, as the one-entry-point runner already does |
| B. a paired `-tasks` crate per subsystem | `ferrowasp-safety` + `ferrowasp-safety-tasks` | twice the crates |
| C. one task crate per class | `ferrowasp-tasks-critical`, `ferrowasp-tasks` | an IMU-group wiring change would re-owe the arming path |

AGENTS.md's rule still holds as written: a task definition wires resources,
locks and spawns; each decision it makes is a host-tested function. The
prelude goes away, and each task imports only what its own crate depends on.

**Contract crates.** The count evolved. First two boundaries were seen
(hardware ↔ core, critical ↔ non-critical); then four (chip ↔ core;
critical → observer; requesters → coordinator; firmware ↔ host, found
because FerroConfigurator depended on `ferrowasp-core` by path for protocol
and blackbox types). Re-owing unit evidence is free, so crate boundaries follow
the expensive bench-evidence boundaries, giving three crates:

| Crate | Holds | Class |
|---|---|---|
| `ferrowasp-io-contracts` | hardware message types | safety-critical, minimal: a change costs bench runs on every chip |
| `ferrowasp-boundary-contracts` | `Request`, results, messages to the observer, the ledger's topic types | safety-critical |
| `ferrowasp-protocol` | wire protocol, blackbox format | non-critical, W6; the configurator depends on this, not the core |

Combining the two safety-critical contract crates was the alternative; a new
telemetry field would then re-owe the hardware traits. Config keys first went
to `ferrowasp-protocol` and moved to `ferrowasp-config` (section 14).
`ferrowasp-core` mixed contract types with `safety.rs` (1,193 lines of logic),
`blackbox.rs` and `config.rs`; `io-core` held implementations (the serial
reader and writer, over 500 lines each, the SPI mailbox and async device), so
neither was a contract crate as it stood.

**Chip backends.** Each of `ferrowasp-stm32` (shared), `ferrowasp-stm32f4`
and `ferrowasp-stm32h7` mixed critical and non-critical hardware.

| Critical hardware | Non-critical hardware |
|---|---|
| motor output: `dshot_bank`, `dshot_lanes`, `dshot`, `timer_dma` | USB: `usb_serial` |
| SPI and IMU: `spi_common`, `spi`, `spi1_imu`, `spi_dma`, `exti` | status LED |
| UART: `uart_common`, `uart_port`, `uart_dma` | storage: `app_storage`, `sd_storage` |
| timing: `clocks`, `timebase`, `watchdog` | `adc` |
| routing: `board_routes`, `board_manifest`, `memory` | |
| `system_reset`, `system_bootloader` | |
| `scheduler`, `timer_tick` (deleted with gyro-sync) | |

Each splits into a `-critical` crate and the unmarked non-critical one. A
module split was the alternative (section 4). The expensive evidence is the
per-family conformance suite for UART, SPI and DShot, which a USB or SD change
must not invalidate.

**The map.**

Safety-critical (`critical = true`):

| Crate | Holds | From |
|---|---|---|
| `ferrowasp-io-contracts` | `RxChunk`, `ImuSample`, `ActuatorFeedback`, `MotorCommand`, `DshotCommand`, the link type and `Stamped` | parts of `core`, `io-core` |
| `ferrowasp-boundary-contracts` | `Request`, results, outbound messages | new |
| `ferrowasp-config` | schema, `validate()`, page format, migration, motor map | `core::config`, part of `core::blackbox`, `flight::flash_storage` |
| `ferrowasp-safety` | safety master, arming, actuator authority | `core::safety`, `flight::arming`, `flight::actuator` |
| `ferrowasp-border` | the border roles, one module each: coordinator, observer and its ledger, Health Manager (section 19, ADR-0061) | new; the observer replaces `stm32-tasks::snapshots` |
| `ferrowasp-control` | general control (above) | `pid`, parts of `flight::drone_toolbox`, `accel_level`, `rpm_filter`, `core::frames` |
| `ferrowasp-flight-control` | flight-controller-specific (above) | the rest of `drone_toolbox`, `dynamic_idle`, `core::actuator` |
| `ferrowasp-input` | Command Input, RC protocol trait, RC group | `flight::rc_receiver` |
| `ferrowasp-feedback` | actuator feedback core, ESC-telemetry trait and group | `flight::esc_manager` |
| `ferrowasp-imu` | IMU trait and group | `stm32-tasks::spi1`, `io-core::spi` |
| per-sensor IMU crates | one embedded-hal driver each | `drivers::mpu6000`, `mpu6500`, `icm42688p` |
| CRSF, SBUS, BLHeli-telemetry crates | standalone embedded-hal/io protocol crates | `drivers::crsf`, `drivers::blheli_telemetry`, `sbus-rs` (external) |
| `ferrowasp-output` | DShot and PWM output groups, waveform encode, bidirectional decode | `waveform`, `stm32-tasks::dshot` |
| `ferrowasp-serial` | UART transport group, RX reader, TX writer | `io-core::serial`, `stm32-tasks::uart_port` |
| `ferrowasp-stm32-critical`, `-stm32f4-critical`, `-stm32h7-critical` | the critical backend modules | the backends |

The serial TX writer is safety-critical because the OSD and telemetry write
through the same transport that may carry RC; a port's function is chosen at
boot from `serial_bindings`, so the transport code is safety-critical for
every port, and the observer separates the consumers (the OSD encoder feeds a
TX queue and never touches the transport).

Non-critical: `ferrowasp-protocol`, `ferrowasp-blackbox` (log
writer, blackbox storage, sync ledger, storage maintenance),
`ferrowasp-storage` (flash manager, config save, NOR storage, SPI NOR),
`ferrowasp-cli` (command link, RPC, storage commands, USB debug),
`ferrowasp-osd`, `ferrowasp-mspv1`, `ferrowasp-mspv2`, `rc-pwm`, and the
non-critical `ferrowasp-stm32`, `-stm32f4`, `-stm32h7`.

Dissolved: `ferrowasp-core`, `ferrowasp-io-core`, `ferrowasp-flight`,
`ferrowasp-drivers`, `ferrowasp-pid`, `ferrowasp-waveform`.
`ferrowasp-stm32-tasks` became `ferrowasp-stm32-app`, the application glue,
and the flash manager went to `ferrowasp-cli` (ADR-0065).

Names are working names until each crate is created.
