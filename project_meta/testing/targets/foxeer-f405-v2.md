# Foxeer F405 V2 Test Procedure

The operator procedure for the Foxeer F405 V2 flight app. Historical evidence,
located through `../EVIDENCE_INDEX.md`, never substitutes for running the
gates below on the proposed image.

An `active` catalog entry means that its definition may be selected. It does
not mean the test has passed or that the aircraft is cleared to fly. Who may
operate the hardware is set by the Operating Rules in the root `AGENTS.md`;
every powered and flight gate below is the user's. Stop conditions override
completion and evidence collection.

## Reviewed Baseline

Use one exact flight candidate and record:

- commit plus dirty-working-tree identity;
- release ELF or binary SHA-256;
- target `foxeer-f405-v2`;
- the exact feature set, including `board-foxeer-f405-v2` and any opt-in diagnostic features;
- propeller and actuator-power state;
- the complete persisted configuration from `config-show`.

A recorded image hash reproduces only from the same absolute checkout path, so
a mismatch after moving or copying the tree is not evidence that the image
changed. The build is deterministic within one tree; Cargo hashes the shared
`crates/` by absolute path because they lie outside the app's workspace, and
`trim-paths` does not fix it, so do not retry it. Before trusting a rebuilt
hash, `touch build.rs`: its `rerun-if-changed=../../.git/HEAD` does not exist
in every layout; the git revision it embeds stays out of the loadable image.

The reviewed configuration is:

| Field | Roll | Pitch | Yaw |
|---|---:|---:|---:|
| P | 2.5 | 2.5 | 2.0 |
| I | 0.0 | 0.0 | 0.0 |
| D | 0.0 | 0.0 | 0.0 |
| center rate | 70 deg/s | 70 deg/s | 70 deg/s |
| maximum rate | 300 deg/s | 300 deg/s | 200 deg/s |
| expo | 0.50 | 0.50 | 0.50 |

The shared RC deadband is `8` raw channel counts. Roll P `3.0` is withdrawn
after a logged autonomous approximately `10-13 Hz` oscillation. Do not select
it. Keep every I and D gain at zero until separate target evidence establishes
clean controller state across disarm, RC loss, aborted arming, and rearm.
The `2.5 / 2.5 / 2.0` baseline is supported by the 2026-07-27 confined-area
flight session, but its transition is inferred from P-only `PID/error` because
BB2 does not yet embed configuration. Capture `config-show` before the next
flight and do not describe the tune as complete.

Do not fly the pre-pitch-fix image with SHA-256
`E4BAE2A6229D1B340E4DF72BF0727D00506989FE9A1DCDE3B71935B4D6BC9758`.
Any other candidate still needs the gates below; a different hash is not
evidence that the pitch fix or current safety behavior is present.

Foxeer's board data maps logical motors to physical outputs one to one
(`LOGICAL_TO_PHYSICAL_MOTOR_OUTPUT = [1, 2, 3, 4]`), and the reviewed
configuration keeps the pilot's `motor_map` at `1234`:

| Logical motor | Physical output | Location | Rotation |
|---|---:|---|---|
| M1 | 1 | rear-right | CW |
| M2 | 2 | front-right | CCW |
| M3 | 3 | rear-left | CCW |
| M4 | 4 | front-left | CW |

The DShot idle command `65` maps to protocol value `112`. Arming keeps stop
frames selected through the guarded dwell, then applies bounded idle under
temporary permission, and requires three fresh in-range eRPM observations from
every ESC before `SYSTEM ARMED`. Counter and ESC interoperability evidence does
not establish pulse width, jitter, complementary-output margin, or timer phase;
those electrical measurements remain open.

## Foxeer USB RC Configuration Gate

Catalog ID: `BENCH-FOX-USB-001`.

`python3 tools/foxeer_bench_gates.py usb-config --build-record <BUILD-FOX-001
record>` runs and checks these steps on a connected Foxeer. The operator
confirms propellers removed and actuator power disconnected and performs each
USB power cycle when prompted; the tool drives the rest, stops at the first
failed check, keeps every capture in a new `logs/bench/` directory, and drafts
a schema 2 run record for review, never committing it. `check-usb-config`
judges an existing capture directory to the same standard.

Keep the aircraft disarmed, stationary during boot gyro calibration, and with
propellers removed. Keep actuator/ESC power disconnected. If the installation
cannot isolate actuator power, stop and treat the work as a powered
props-off test requiring explicit user confirmation.

1. Record the candidate identity and exact features. Set the USB CDC port and
   capture and export the initial configuration from the extracted ready
   package:

   ```powershell
   $Port = "COM6"
   .\ferro-configurator.exe flash --board foxeer-f405-v2 --dry-run
   .\ferro-configurator.exe --port $Port config show
   .\ferro-configurator.exe --port $Port config export foxeer-baseline.toml
   ```

2. Require the initial values to match the reviewed baseline above. Do not
   overwrite an unexplained profile merely to make the check pass.
3. Verify invalid-value rejection. This command must fail with a nonzero exit
   code and report that the value is outside the allowed range. The subsequent
   read must still show roll expo `0.5`:

   ```powershell
   .\ferro-configurator.exe --port $Port config set roll-expo 1.1
   .\ferro-configurator.exe --port $Port config show
   ```

4. Apply a conservative temporary roll profile. Every command must report
   persistent verification by complete readback:

   ```powershell
   .\ferro-configurator.exe --port $Port config set roll-max-rate 250
   .\ferro-configurator.exe --port $Port config set roll-center-rate 60
   .\ferro-configurator.exe --port $Port config set roll-expo 0.4
   ```

5. Without rebooting or reflashing, run `config show`. Require roll
   center/max/expo `60 / 250 / 0.4` and all unmodified values unchanged. This
   is the required immediate-application evidence.
6. Cold-power the FCU without reflashing, hold it stationary through gyro
   calibration, reconnect USB, and run `config show` again. Require the same
   temporary profile.
7. Restore the complete captured baseline atomically and verify it:

   ```powershell
   .\ferro-configurator.exe config validate foxeer-baseline.toml
   .\ferro-configurator.exe --port $Port config apply foxeer-baseline.toml
   .\ferro-configurator.exe --port $Port config show
   ```

8. Require the restored file to contain P `2.5 / 2.5 / 2.0`, every I/D gain
   zero, and every reviewed RC field above. Cold-power once more and run
   `config show` again. Retain both snapshots. Do not continue to actuator
   power if restoration or cold-boot persistence is uncertain.

Stop on unexpected motor activity, an armed state, an unknown image or feature
set, a rejected valid baseline, an accepted invalid value, a persistence
failure, a reboot requirement, flash/storage faults, or inability to restore
the baseline.

Required evidence: candidate identity and hash, exact features, power/propeller
state, initial snapshot, rejection response, same-boot temporary snapshot,
cold-boot temporary snapshot, restored snapshot before and after cold boot,
and operator observations.

## Foxeer USB Reboot And DFU Flash Gate

Catalog ID: `BENCH-FOX-DFU-001`.

Shows that a disarmed Foxeer running FerroWasp restarts, and restarts into the
STM32 ROM bootloader, on command, that FerroConfigurator flashes it without the
BOOT button and it returns as FerroWasp, and that a reboot is refused while a
flight log is open. ADR-0043 is the design.

Keep the aircraft disarmed and on USB power only. Before flashing anything,
download every flight on the board and export the configuration; a bench
image in step 6 appends a long bench flight to the log.

1. Record the candidate identity, hash, and exact features, and validate the
   image with `flash --board foxeer-f405-v2 --dry-run`.
2. `device reboot`: require the USB port to drop and return, a lower uptime
   in `device info`, and no `0483:df11` device.
3. `device reboot --dfu`: require exactly one `0483:df11` device within 10 s.
   The F405 ROM reports `dfuERROR` / "firmware is corrupt" on its first status
   however it is entered; `dfu-util` clears it.
4. Read back the image's range with `dfu-util -U` and require every byte of
   the ELF's load segments to match; erased gaps between segments read
   `0xFF`. Leave DFU by flashing the candidate.
5. From running FerroWasp, `flash --board foxeer-f405-v2 --yes <candidate>`:
   require exit code zero, the board back on its FerroWasp port with the
   same USB serial number, and `config export` identical to the export from
   before.
6. Flash the same commit built with `bench_blackbox`, which opens a flight log
   at boot. Require `reboot` and `reboot dfu` both answered
   `ERR flight log still being written`, and `flash --yes` to exit nonzero
   with the board still running. The user then enters ROM DFU with the BOOT
   button, and the candidate is flashed back.

Stop on an armed state, any actuator activity, an unknown image or feature
set, a reboot accepted while a flight log is open, a DFU device that is not
the Foxeer's, a readback mismatch inside a load segment, a changed
configuration, or a board that does not return as FerroWasp.

Required evidence: candidate and bench-image identities and hashes, exact
features, power/propeller state, DFU enumeration time, readback comparison,
configuration before and after, both refusal responses, and operator
observations.

## Foxeer Powered Props-Off Exact-Image Gate

Catalog ID: `BENCH-FOX-001`.

Run only after `BENCH-FOX-USB-001` passes. The user must confirm that all
propellers are removed before actuator power is connected. Secure the
airframe, keep clear of the motors, use a bounded power source where
available, and keep an immediate disarm/power-removal path.

This gate is the minimum needed to judge the aircraft safe to fly, not an
exhaustive test schedule. A defect that feeds no safety, arming, or actuator
path is recorded in the run record, not gated on. A fault that fails closed -
refusing to arm - is safe by construction and is likewise recorded rather than
gated, though note it as a limitation when it also degrades flight logging.

1. Confirm the powered image has the same commit, dirty-tree identity, feature
   set, and SHA-256 recorded at the USB gate. Reject bench-only motor selector,
   smoke lockout, retired PWM-output, withdrawn-gain, and pre-fix images.
2. Boot stationary. Require healthy IMU calibration, RC qualification,
   DShot/telemetry status, no panic or latched transport fault, and a disarmed
   state. Verify `config-show` still matches the reviewed baseline and record
   the next onboard flight ID/log capacity. With the flight battery connected,
   require total voltage to remain consistent with a multimeter and cell
   voltage to equal total voltage divided by the detected cell count. Record
   the reported current and any discrepancy, but do not gate on it: current
   sense drives only OSD and MSP display, feeds no safety, arming, or actuator
   logic, and is not calibrated.
3. While disarmed, observe the controller setpoints. Require centered sticks
   inside the configured deadband to command zero. Require right roll, forward
   pitch, and right yaw to use the current positive controller directions.
   Require both full-stick directions to remain bounded at approximately
   `+/-300 deg/s` roll, `+/-300 deg/s` pitch, and `+/-200 deg/s` yaw.
4. With throttle low and ARM initially low, verify that booting or reconnecting
   with ARM held high cannot arm. Require a qualified ARM-low observation and
   a later low-to-high transition before an arm request is accepted.
5. Arm once. Require the guarded DShot stop dwell, four-motor eRPM
   qualification, and only then the armed transition. Exercise only modest
   throttle and small roll, pitch, and yaw commands.
6. Verify motor identity remains Betaflight Quad X: M1 rear-right CW, M2
   front-right CCW, M3 rear-left CCW, and M4 front-left CW.
7. Verify correction opposes motion. In particular, physical nose-down must
   produce positive controller pitch, negative pitch PID, and front M2/M4
   above rear M1/M3. Nose-up must produce the inverse. Roll and yaw PID/motor
   response must also oppose, never reinforce, the imposed motion.
8. Explicitly disarm and require four zero DShot values and zero eRPM. Re-arm
   only after the required ARM-low transition, then cause RC loss using the
   reviewed link-loss method. Require immediate stop, no automatic rearm after
   recovery, and a fresh ARM-low then ARM-high sequence for any later request.
9. Disarm and leave the system running for at least two seconds so the final
   onboard page can commit. Retain the RTT/USB output and the selected onboard
   flight ID. Do not erase logs.

Stop on installed propellers; candidate/configuration mismatch; unexpected
arming; wrong stick, motor, or correction sign; a motor starting before the
armed transition; failure to stop on disarm or RC loss; automatic rearm;
DShot, storage, stale-command, or IMU faults; a telemetry fault that fails
open or that disturbs actuator output while armed; smoke, heat, rough motor
sound, or any loss of operator confidence.

Required evidence: candidate and working-tree identity, image SHA-256, exact
features, persisted configuration, propeller/power state, board and IMU
identity, arm/disarm/RC-loss transcript, motor and correction observations,
DShot/telemetry counters, and onboard flight/log identity.

## Foxeer Final Preflight Gate

Catalog ID: `PREFLIGHT-FOX-001`.

This gate is for the same candidate exercised by the powered props-off gate.
Any firmware, feature, persisted configuration, motor/ESC, receiver, wiring,
propeller, or airframe change invalidates the affected evidence and requires
the relevant lower gate again.

1. Confirm current software and exact embedded-build checks passed for the
   recorded commit/working tree and feature set.
2. Confirm `BENCH-FOX-001` passed on the exact image and that its evidence is
   available. A prior historical pass is not enough.
3. Verify the persisted P-only and RC-rate baseline in this file. I and D must
   remain zero. Make no gain or rate change during this preflight.
4. Verify the flight-log store is writable, has known capacity, and has a
   recorded next flight ID. Preserve any evidence that has not been downloaded
   and validated.
5. Power-cycle into the flight arrangement and keep the aircraft stationary
   through gyro calibration. Require the expected Foxeer board/IMU, healthy
   RC, DShot/telemetry, battery indication, OSD/video, and no fault or panic.
6. With propellers still removed, repeat the short direction, arming,
   disarming, RC-loss, and motor-opposition checks needed to detect assembly or
   setup drift. Require four stop values before installing propellers.
7. Disconnect power. Inspect frame, battery retention, receiver/antennas,
   motor fasteners, motor order/direction, propeller condition/orientation,
   center of gravity, and loose wiring. Install propellers only after the
   inspection and props-off evidence pass.
8. Establish a clear controlled area, conservative weather, reliable RC/video,
   an observer where appropriate, and an immediate abort/landing plan. The
   user explicitly confirms the bounded hop before power is restored.
9. After final power-up, do not move the aircraft during gyro calibration.
   Require disarmed controls, correct baseline/configuration identity, and no
   new warning before proceeding.

Stop if any prerequisite is incomplete; the exact image, features,
configuration, log state, or evidence is unknown; withdrawn gains or the
pre-fix image are selected; any props-off behavior differs; the airframe,
propellers, battery, RC/video, environment, or operating area is unsuitable;
or the operator does not explicitly confirm the hop.

Required evidence: completed preflight checklist, commit/working-tree identity,
image SHA-256 and features, persisted configuration, props-off gate reference,
planned flight/log ID, aircraft and environment observations, and operator
confirmation.

## Foxeer Conservative Logged-Hop Flight Test

Catalog ID: `FLIGHT-FOX-001`.

Run only after the final preflight gate passes on the still-current candidate.
This is one conservative characterization hop, not aggressive FPV flight or a
tuning session.

1. Do not change gains, RC rates, filters, firmware, features, or hardware
   after preflight. Change no more than one reviewed variable between separate
   future flights, and never combine a gain change with this RC-curve
   validation hop.
2. Arm from level ground. If idle or control behavior differs from the
   props-off gate, disarm without lifting off.
3. Lift only enough for a short, low, controlled hover/hop. Apply small,
   separated roll, pitch, and yaw inputs sufficient to evaluate command
   tracking. Avoid aggressive stick steps, altitude, speed, or prolonged
   flight.
4. Land and disarm promptly. Leave power connected and the system disarmed for
   at least two seconds so the final flash page commits.
5. Record the flight ID, download that one flight, and retain the CRC-validated
   `.fwbb` file. Do not erase onboard evidence until download and analysis
   succeed.
6. Analyze only the selected flight with the current analyzer's contiguous
   throttle-on window. Review:

   - commanded versus measured roll, pitch, and yaw rates;
   - autonomous or command-excited oscillation, especially near `10-13 Hz`;
   - mixer headroom, rescaling, clipping, or sustained motor imbalance;
   - missing/repeated samples, timing gaps, and storage faults;
   - the initial response after arm and any later rearm for a retained-state
     kick or nonzero command history.

7. Treat clean observed arm-boundary behavior as bounded target evidence, not
   proof of every internal reset path. Keep I zero until dedicated evidence
   covers disarm, RC loss, aborted arming, and rearm.
8. Review the log and post-flight inspection before proposing any tuning
   change or another flight.

Disarm/land immediately on wrong motor or stick response, rapid attitude
departure, hard oscillation or bounce, reinforcing correction, unexpected
yaw/roll/pitch drift, mixer saturation, smoke, heat, abnormal current, rough
motor sound, loss of RC/video/logging confidence, or any loss of control
authority.

Required evidence: operator and environmental conditions, candidate hash and
features, persisted configuration, preflight reference, selected onboard
flight ID, retained `.fwbb` path and SHA-256, analyzer command/results,
command-tracking and oscillation review, mixer-headroom review, arm-boundary
observations, stop-condition observations, and post-flight inspection.
