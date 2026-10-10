# Board Bring-Up Procedure

This is the board-agnostic gate set for a flight board whose
`src/board/profiles.rs` still has a verification flag set to `false`. Run it
again on a verified board after a change to its `src/board/` hardware facts.
Each board supplies its own facts in a target file, such as
`tbs-lucid-h7.md`. The user operates all target hardware.

A check belongs here only if it meets all three of these conditions:

- it catches a failure specific to the board, such as pins, timers, DMA, clocks,
  orientation or flash layout;
- that failure would fail open, meaning the board would arm or fly wrong;
- no cheaper or earlier step already proves it.

Host tests already cover shared logic on every change. A fault that only
refuses arming shows up at the next step on its own, so it is recorded, not
gated. Functional checks that fail no safety path are listed separately and
recorded only.

Every gate needs the same baseline record: the commit and dirty-tree identity,
the image SHA-256, the exact feature set, the propeller and actuator-power state,
the fitted IMU kind, and the board's target file.

## Bring-Up Unpowered Board Facts

Catalog ID: `BRINGUP-COMMON-001`. Run it after `BENCH-COMMON-001` on the same
image, with actuator power disconnected.

1. **Loop rate.** Read `uptime_ms`, `control_sequence`, and `control_loop_hz`
   from two status samples at least 10 s apart. Require the measured rate to be
   within 1% of the board's `CONTROL_LOOP_RATE_HZ`. A wrong HSE or PLL
   configuration skews every timer. A banner or a heartbeat cannot show that.
2. **IMU identity.** Require the status to report the IMU kind the target file
   expects, with `imu_ready` set and `imu_stale` clear.
3. **IMU orientation, per fitted IMU kind.** Build with `imu_orientation_rtt`.
   Move the board as in `mdbook/src/imu.md`, "Foxeer Bring-Up Checklist" step 5:
   nose up, right side down, then clockwise yaw, holding each motion for at
   least four seconds. Require the mapped body rate for each motion to be
   positive (pitch, roll and yaw respectively), and the other two axes to stay
   near zero. A board may fit more than
   one IMU kind, and each kind needs its own pass, because the parts can sit in
   different package orientations. Only after a pass may the user set
   `IMU_SENSOR_IDENTITY_VERIFIED` and `IMU_ORIENTATION_VERIFIED`, and add that
   kind to `imu_kind_flight_verified`.
4. **RC decode.** With the receiver bound, require `rc_valid` and each stick
   and switch channel to move on its own when the transmitter moves it.
5. **Arm-high at boot.** Reset with the arm switch high. Require the board to
   stay disarmed until the switch is seen low and then goes high again.
6. **Config persistence.** Set one tuning value, run `config save`, reset, and
   require the value to read back. Restore it. The config sector's address and
   erase size are chip-specific.

Stop on: any motor activity; an armed state; an orientation sign that disagrees
with the convention; an unknown image or feature set.

Required evidence: the baseline record, the two status samples, the IMU kind,
the orientation transcript for each fitted kind, the arm-high-at-boot
observation, and the persistence readback.

## Bring-Up Unpowered Fault Injection

Catalog ID: `BRINGUP-COMMON-002`. Each fault build is a separate image, never
the flight image. Actuator power stays disconnected.

1. **`bench_prearm_imu_stale`.** Require the "FAULT INJECTION ACTIVE" warning
   at boot, and require every arm request to be refused. The stale detector
   depends on this board's SPI, DMA and interrupt priorities.
2. **`bench_spi_timeout_recovery`.** Run this only on the first board of a
   timer/DMA backend, such as the first `ferrowasp-stm32h7` board. Require the
   injected SPI timeout to be reported, then require IMU samples to resume
   without a reset.

Stop on: an arm request that is accepted; a missing injection warning; a
board that does not recover.

Required evidence: each fault image's hash and features, and its RTT transcript.

## Bring-Up Powered Props-Off Gate

Catalog ID: `BRINGUP-COMMON-003`. The user confirms that the propellers are
off before actuator power is connected. Secure the airframe, keep clear of the
motors, and keep an immediate way to remove power.

1. **Motor order and direction.** Use the configurator Motors tab or the
   `bench_logical_motorN_only` images. Require each logical motor at its
   Betaflight Quad X position and spinning the right way: M1 rear-right CW,
   M2 front-right CCW, M3 rear-left CCW, M4 front-left CW. Only after a pass may
   the user set `MOTOR_OUTPUT_ORDER_VERIFIED`.
2. **Complementary or inverted outputs.** Do this only if a motor output uses an
   N-channel or inverted timer channel, like the Foxeer's M4 on TIM8 CH3N.
   Require that motor to arm and spin like the others. Only after a pass may the
   user set the board's polarity flag.
3. **Stick directions and bounds.** Do this whenever the board's receiver port or
   channel map differs from a board that already passed. Require centered sticks
   to command zero, and right roll, forward pitch and right yaw to command
   positive. Require full deflection to stay inside the configured maximum rates.
4. **Arm.** With the arm switch low first, arm once. Require the guarded stop
   dwell, fresh eRPM from all four ESCs, and only then the armed state. This
   also proves the ESC telemetry path.
5. **Correction opposes motion.** Armed at idle, tilt the airframe nose down.
   Require the front motors (M2, M4) to rise above the rear (M1, M3). Nose up
   must do the inverse. Roll and yaw must also oppose the imposed motion. This
   is the only check that sees IMU orientation, mixer, motor order and direction
   together, so it gates even when each of them passed alone.
6. **Disarm.** Require four zero DShot values and zero eRPM.
7. **RC loss.** Re-arm after a fresh low-to-high, then cut the link by the
   reviewed method. Require an immediate stop, and no rearm after the link
   recovers until a fresh low-to-high.
8. **Battery voltage.** Compare the reported pack voltage against a meter.
   Require agreement within 0.2 V. No firmware logic reads it. It is the pilot's
   only low-battery cue, so it gates through `documented_baseline_verified`.
9. **DShot signal, once per backend.** Do this only on the first board of a
   timer/DMA backend. Put a scope on all four outputs and require clean
   DShot600 bit timing. A dead line already fails closed through step 4. This
   checks the timing margin, not whether the line works.

Recorded, not gated:

- the current sensor reading against a meter;
- storage: the blackbox records and downloads, and config commits;
- OSD drawing;
- ESC telemetry values other than eRPM.

Stop on: installed propellers; a motor starting before the armed state; a wrong
motor, direction, stick sign or correction sign; a failure to stop on disarm or
RC loss; an automatic rearm; smoke, heat, rough motor sound, or any loss of
operator confidence.

Required evidence: the baseline record, the motor order and direction
observations, the stick transcript, the arm/disarm/RC-loss transcript with DShot
and eRPM counters, the correction observations, the meter and reported
voltages, the recorded-only observations, and any scope captures.

## Bring-Up Powered Fault Injection

Catalog ID: `BRINGUP-COMMON-004`. Separate fault images, under the same
props-off conditions as `BRINGUP-COMMON-003`.

1. **`bench_dshot_idle_output1_not_running`.** Attempt to arm. Require the
   idle qualification to fail and name physical output 1, require a sustained
   stop, and require that the armed state is never reached.
2. **`bench_motor_cmd_stale_rejection`.** Arm. Require the RTT line "Actuator
   command refused: stale motor command" and no motor output from that command.
   The injected command is the control loop's first. If the refusal appears
   only before arming, record that the armed path was not exercised.

Stop on: an armed state with output 1 unqualified; motor output from a stale
command; any stop condition of `BRINGUP-COMMON-003`.

Required evidence: each fault image's hash and features, and its RTT transcript.
