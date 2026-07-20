# Target Verification Checklist

This checklist is for STM32F405-class bench validation of the current FerroWasp prototype. It is not a flight-clearance document. Treat each item as evidence to collect before moving from bench testing toward tethered or prop-on testing.

Recommended result fields for each check: date, board, firmware commit, equipment used, pass/fail, notes, and captured logs or traces.

## Build and Flash

- [ ] Record exact Git commit, branch, feature flags, Rust toolchain, and target triple used for the firmware image.
- [ ] Run `cargo fmt --all --check`.
- [ ] Run `cargo check --workspace` for reusable crates.
- [ ] Run the target-specific check from the selected package under `apps/`.
- [ ] Run host tests for pure logic, including PID, RC remapping, DShot helpers, safety qualification, and mixer behavior.
- [ ] Flash the board with `probe-rs` or the selected tool and record the command.
- [ ] Confirm RTT/defmt logs are visible after reset.
- [ ] Confirm panic output is visible and does not silently hang the bench workflow.

## Boot and Idle State

- [ ] Confirm the board boots reliably from cold power and reset.
- [ ] Confirm all motor outputs remain at low/stop value during boot.
- [ ] Confirm no motor output pulses occur before the actuator-output task is initialized.
- [ ] Confirm boot RTT logs and the red/green LED heartbeat show the scheduler is alive.
- [ ] Confirm the firmware remains disarmed if the RC receiver is disconnected at boot.
- [ ] Confirm brownout or manual reset returns the system to disarmed low-output state.

## Safety and Arming

- [ ] Confirm arm switch high is detected only on the intended RC channel.
- [ ] Confirm arming requires the configured hold time.
- [ ] Confirm arming is rejected when throttle is above `MIN_THROTTLE`.
- [ ] Confirm actuator idle permission is temporary and revoked after the idle sequence.
- [ ] Confirm actuator idle completion is required before system armed state is set.
- [ ] Confirm dropping the arm switch always disarms and commands low output.
- [ ] Confirm disarm works during each phase: disarmed, arming low hold, idle hold, armed idle, and active throttle.
- [ ] Confirm the known arming-idle abort path is reproduced or fixed, with logs captured.
- [ ] Confirm no experimental, telemetry, OSD, USB, or parser path can command motor peripherals directly.

## RC Input

- [ ] Verify SBUS electrical inversion and UART settings on the real receiver path.
- [ ] Verify USART2 DMA receive continues over long runtime without buffer lockup.
- [ ] Verify roll, pitch, yaw, throttle, and arm channel mapping against transmitter stick movement.
- [ ] Verify channel center, min, max, and deadband behavior as seen by firmware logs or OSD.
- [ ] Verify RC frame loss, receiver power loss, and malformed frame behavior.
- [ ] Verify RC-loss behavior inhibits or disarms before prop-on testing.
- [ ] Verify reconnect behavior does not automatically re-arm without an explicit valid arm sequence.

## IMU and Estimator Path

Known current limitation: IMU initialization, gyro-bias calibration, and
freshness are not arming prerequisites. A stale IMU can pass DShot idle-eRPM
qualification and briefly reach `SYSTEM ARMED`; the first post-arm stale-IMU
control check requests disarm. Closing this pre-arm gap is still required.

- [ ] Confirm MPU6500 `WHO_AM_I` response and init sequence on the target board.
- [ ] Confirm SPI1 pin map, chip select, clock mode, and DMA stream behavior with a logic analyzer.
- [ ] Confirm IMU sample sequence increments at the intended 800 Hz poll rate.
- [ ] Confirm raw gyro and accel axes match physical board movement.
- [ ] Confirm the standard drone body frame is forward/right/down for the board:
  +X forward, +Y right, and +Z down.
- [ ] Confirm the BSP's IMU-to-board and board-to-drone rotations compose to
  the measured physical roll, pitch, and yaw signs.
- [ ] Confirm sign conventions for roll, pitch, and yaw match the mixer/control
  assumptions.
- [ ] Confirm filtered rates respond to motion and decay as expected.
- [ ] Confirm stale IMU detection triggers when SPI/IMU data stops updating.
- [ ] Confirm stale IMU data cannot continue driving active motor commands.
- [ ] Record vibration/noise levels on the bench with motors powered but props removed.

## Control Loop and Mixer

- [ ] Confirm control update cadence is 400 Hz under normal load.
- [ ] Confirm loop timing and jitter with RTT timestamps, GPIO toggles, or a logic analyzer.
- [ ] Confirm roll, pitch, and yaw stick inputs produce expected rate setpoints.
- [ ] Confirm PID P/I/D/feedforward logs match expected sign and magnitude for bench motion.
- [ ] Confirm mixer output order follows Betaflight Quad X logical numbering:
  motor 1 rear-right, motor 2 front-right, motor 3 rear-left, motor 4
  front-left.
- [ ] Confirm the selected board profile maps those logical motors to the
  correct physical output pads.
- [ ] Confirm motor direction assumptions match the frame and ESC setup.
- [ ] Repeat the props-off motor order, direction, and stick/tilt response
  checks before flight on every actuator-capable board after any board profile,
  output backend, motor-map, or wiring change.
- [ ] Confirm saturation/rescaling behavior keeps outputs within the configured range.
- [ ] Confirm non-finite or invalid control values cannot reach actuator output.

## Actuator Output - PWM

- [ ] Confirm TIM1 CH1, TIM3 CH4, TIM3 CH3, and TIM12 CH2 are on the intended motor pins.
- [ ] Confirm PWM frequency is 400 Hz on all motor outputs.
- [ ] Confirm pulse width low/high range matches the expected 1000..2000 us ESC range.
- [ ] Confirm disarmed output is low/stop on all channels.
- [ ] Confirm armed idle output is consistent and below lift-producing throttle.
- [ ] Confirm `ApplyLatestThrottle` is ignored unless safety armed state is true.
- [ ] Confirm stale or missing motor commands result in safe low output.
- [ ] Confirm queue overflow or spawn failure does not leave motors at stale active output.
- [ ] Confirm optional `pwm_cal` mode affects only the selected motor and is never enabled in normal bench firmware.

## Actuator Output - FCU3 DShot

- [x] Verify DShot packet encoding and TIM1 compare-sequence timing with host tests.
- [ ] Verify DShot timer duty levels and terminating low state on a logic analyzer before treating the route as timing-validated.
- [x] Verify DShot stop, idle, and throttle mapping against ESC expectations.
  On 2026-07-18, the first powered props-off attempt showed that the ESC
  decoded the M1 idle request and spun the motor, but the original one-shot
  arming-idle lease expired before the asynchronous hold completed. Stop
  frames were selected and the system disarmed. The replacement renews the
  normal 20 ms lease every 10 ms only after a successful arming-guard check.
  The corrected image subsequently transmitted M1 value `112`, reached
  `BLHeli ESCs idling` and `SYSTEM ARMED`, and then selected value `0` for
  zero RC throttle with all expiry, timeout, and fault counters still zero.
  The final powered run followed RC throttle as expected through requested
  DShot values `112`, `177`, `167`, and `219`, then returned to value `0` on
  explicit disarm.
- [x] Verify DMA2 Stream1 Channel6 does not conflict with active SPI, UART, or ADC DMA routes.
- [x] Statically verify DShot commands remain behind actuator-output validation, arming guards, command freshness, and a 20 ms output lease.
- [x] Verify the optimized image binds the DMA2 Stream1 vector to the generated
  DShot completion handler. The final static build placed symbol
  `DMA2_STREAM1` at `0x0800464C` and vector word `0x0800464D`.
- [x] Flash `dshot bench_motor1_only` with actuator power disconnected and
  confirm advancing DMA completion counters with no DShot warning. On
  2026-07-18, the FCU3 completed this checkpoint for at least 12,000 frame
  starts at the intended 500 Hz service rate. Each snapshot showed exactly
  one frame in flight (`completed = started - 1`) and zero busy, lease-expiry,
  timeout, and fault counts while IMU samples continued advancing.
- [x] With propellers removed and the previously smoked motor/ESC inspected,
  verify only physical output 1 responds during a brief capped interoperability
  test. Physical output lane 1 (PA8, now mapped to logical M4/front-left)
  followed throttle input as expected, returned to stop on disarm, and reached
  10,000 frame starts with zero busy, lease-expiry, timeout, and fault counts
  while IMU sampling continued.
- [x] Implement the opt-in four-motor backend without changing the external
  FCU3 pads: physical output 1 PA8, output 2 PC9, output 3 PC8, and output 4
  PB15.
- [x] Statically verify the four DShot routes use free DMA2 streams:
  output 1/TIM1_CH1 Stream1 Channel6, output 2/TIM8_CH4 Stream7 Channel7,
  output 3/TIM8_CH3 Stream4 Channel7, and output 4/TIM1_CH3N Stream6 Channel6.
- [x] Verify the optimized four-motor image binds all four completion
  interrupts. Symbols `DMA2_STREAM1`, `DMA2_STREAM4`, `DMA2_STREAM6`, and
  `DMA2_STREAM7` are at `0x080048F4`, `0x08004CFC`, `0x08004D54`, and
  `0x08004DAC`. The pre-correction target-test ELF had SHA-256
  `5FE1AE6883E6448A89541731CA3F61F5758E065B9BA2CBAD32F4E81380F702F2`.
- [x] Keep the historical four-motor DShot bench images behind
  `dshot bench_equal_motors`, the existing 250-count cap, and the normal
  safety/arming/lease path. At most one logical-motor selection may be added;
  physical selections and PWM calibration remain rejected. This was a
  pre-promotion gate; DShot is now the normal FCU3 default and PWM is an
  explicit fallback.
- [x] Statically separate DShot arming from the legacy PWM low/idle sequence.
  The first promoted DShot stage kept all four lanes stopped for a profiled
  100 ms dwell and permitted nonzero commands only after the safety master set
  armed. It was subsequently superseded by telemetry-qualified arming: the
  actuator owner may apply bounded idle under a temporary arm permit, but the
  safety master cannot set `SYSTEM ARMED` until every ESC supplies fresh,
  in-range eRPM evidence. PWM timing and Foxeer behavior remain unchanged.
- [x] Move the FCU3 DShot idle command into the BSP profile without changing
  its initial value. Command `65` maps to target-proven DShot value `112`;
  protocol-specific active-output validation uses that floor, and compile-time
  policy caps tuning at 250. Unequal-vector builds additionally reject idle
  above their smallest fixed command, `80`.
- [x] Build and identify the exact equal-motor props-off candidate. Its
  SHA-256 is
  `34CB9BFB9225AC9AE3B9771F4647F52FABE9176D392ABC54DE50BCC0AE764807`;
  DShot IRQ symbols remain at `0x080048F4`, `0x08004CFC`, `0x08004D54`, and
  `0x08004DAC`, and loadable flash ends at `0x08012BA0`. Binary metadata
  contains the new stop-only messages and neither legacy PWM idle message.
- [x] With propellers removed, flash and exercise the updated
  `dshot bench_equal_motors` image. The retained 2026-07-19 excerpt contains
  the unique 100 ms stop-dwell and pre-arm-complete messages; the startup
  profile line and flashed-ELF hash were not retained.
- [x] Confirm the RTT arming transition keeps four stop commands selected
  through DShot preparation and reports pre-arm complete before
  `SYSTEM ARMED`. The retained excerpt contains no legacy
  `Applying idle throttle` message.
- [x] Confirm separately that the historical stop-only candidate produced no
  physical motor movement before `SYSTEM ARMED`. On 2026-07-19, the operator
  confirmed that behavior. This is retained as dated evidence and is not a
  description of the current telemetry-qualified idle stage.
- [x] After `SYSTEM ARMED`, confirm all four motors run at value `112`.
  Explicit disarm restored sustained zeros through at least 25,000 frame
  starts. Lane counters stayed equal with `completed = started - 1`, IMU
  sequence advanced from 40,040 through 59,260, and busy, expiry, timeout, and
  fault counters remained zero.
- [x] Accept value `112` as the current FCU3 DShot bench idle. The operator
  confirmed all four motors idled at that value; no BSP tuning change is
  required for the current checkpoint. Continue cold-start margin
  characterization even though DShot has since become the default and
  completed an experimental flight.
- [x] With propellers removed and ESC power disconnected, flash
  `dshot bench_equal_motors`. Confirm all four per-lane completion counters
  advance together for at least 10,000 frame sets, with at most one set in
  flight and zero busy, expiry, timeout, spurious, or fault reports. On
  2026-07-18, the release image reached at least 12,000 starts. Every reported
  snapshot had `completed = started - 1`, all four lane counts equal to
  completed sets, and zero busy, expiry, timeout, and fault counts. No spurious
  warning appeared, and IMU sequence numbers continued advancing.
- [x] Record and diagnose the first powered four-motor attempt. M1, M2, and M3
  spun, but physical M4/front-right on PB15 did not. Requested values and all
  four DMA completion counters remained equal through explicit disarm, with
  zero busy, expiry, timeout, or fault counts. RM0090 Table 96 shows that with
  `CC3E=0` and `CC3NE=1`, TIM1_CH3N is `OC3REF xor CC3NP`; the tested image
  incorrectly selected active-low `CC3NP=1`. The implementation now selects
  active-high `CC3NP=0`. The corrected-image powered checkpoint below validates
  ESC decoding on M4/front-right.
- [x] Build and identify the corrected release ELF. SHA-256
  `AA3A5A3D96B0B97BD1FA99031A4AA6FCD2A0A37650B8F4AEAA8121BFBF5FF13C`
  retains the same four IRQ symbol addresses and loadable flash end
  `0x08012EC0`.
- [ ] Capture a separate corrected-image regression with ESC power
  disconnected. The operator intentionally skipped this repeat and proceeded
  directly to the powered props-off test. Before arming, that powered run still
  showed synchronized stop-frame accounting through 3,000 starts with zero
  backend faults.
- [x] With propellers removed, power the ESCs, arm at zero throttle, and
  confirm all four motors decode the guarded idle phase and remain responsive
  after `SYSTEM ARMED`. On 2026-07-18, all four motors, including physical
  M4/front-right, ran with equal value `112`; lane counters remained equal and
  no backend warning was reported.
- [x] After `SYSTEM ARMED`, apply only a small throttle increase and confirm
  all four motors follow the equal capped command. The corrected-image run
  reported equal values `123`, `129`, and `158`, returned to `112`, and then
  selected four zeros on explicit disarm. It reached 10,000 frame starts with
  `completed = started - 1`, equal lane counters, and zero busy, expiry,
  timeout, or fault counts while IMU sampling continued.
- [x] Before further powered testing, inspect all four motor/ESC assemblies,
  especially the pair that previously emitted smoke, for abnormal heat, smell,
  roughness, jitter, or current. On 2026-07-20 the operator inspected the
  previously suspect motor and reported that it appeared normal.
- [x] Complete the props-off DShot RC-loss/recovery procedure in
  `mdbook/src/dshot.md`. On 2026-07-18, the operator reported the functional
  procedure passed: link loss selected and sustained four stop values, link
  recovery did not automatically rearm, and a fresh low-to-high arm sequence
  completed the normal guarded DShot arming flow. Explicit disarm returned all
  lanes to stop. The retained excerpt begins after timeout invalidation and
  the initial stop transition, so it does not measure stop latency or preserve
  that event pair directly.
- [x] Preserve the retained RC-loss/recovery runtime evidence. Four zero values
  persisted through at least starts 44,000 to 47,000 before the fresh manual
  arm request. All lanes remained equal, all backend counters remained zero,
  and IMU sequence advanced through 126,527. The subsequent guarded idle used
  four values `112`; explicit disarm returned to four zeros through at least
  52,000 starts.
- [x] Statically compile all four capped logical-motor DShot images and reject
  DShot without `bench_equal_motors`, physical selected-motor modes, multiple
  logical selections, and `pwm_cal`.
- [x] Build the current equal-motor RC-loss candidate. The working-tree ELF has
  SHA-256
  `F36B3D1C9B468FBC4999CC9771A0E9E72F4DA2A6913F11B9B0F0684782E97224`,
  retains IRQ symbols `DMA2_STREAM1/4/6/7` at
  `0x080048F4/0x08004CFC/0x08004D54/0x08004DAC`, and ends its loadable flash
  image at `0x08012FB0`. This identifies the source-side candidate built from
  commit `c4eeb90` plus the current working-tree changes for the reported
  target run; the retained target observations are recorded above.
- [x] Build and identify all four logical-motor release candidates from commit
  `c4eeb90` plus the current working-tree changes. Logical motors 1 through 4
  have SHA-256 values
  `A3E569EDFADC5E64DED1A1AC4147610E540A6F56B337262672AF4D5B33D9BA9A`,
  `8626339FEDDAD7A7EB9CFA606E669A354443AFCD96DF115419C4983510AFE152`,
  `373D40848C86BE9FBEE2360E97E728C088B1E164588979DBDFFFA99EDF6C0C04`,
  and
  `1BE803EF05BF3B5694400538F903FC3BD46C3554E37CB9222870136F3607C65D`
  respectively. Every image retains the four expected DMA IRQ symbols and has
  loadable flash end `0x080130D8`. These are source-side identifiers, not yet
  target results.
- [x] Execute the four logical-motor images in `mdbook/src/dshot.md` and
  confirm identity. On 2026-07-18, the operator ran each exact feature command:
  logical motors 1/2/3/4 spun rear-right/front-right/rear-left/front-left,
  confirming physical outputs 3/4/2/1 and committed map `[3, 4, 2, 1]`.
- [x] Record CW/CCW rotation direction for each logical motor. Verified on
  2026-07-20: M1 rear-right CW, M2 front-right CCW, M3 rear-left CCW, and M4
  front-left CW.
- [ ] Retain one logical-motor RTT diagnostic capture showing the selected
  vector, equal advancing lane counters, explicit disarm to four zeros, and
  zero busy, expiry, timeout, and fault counters. These diagnostics were not
  included in the identity report.
- [x] Implement and statically verify
  `dshot bench_equal_motors bench_dshot_unequal_motors`. Below its 100-count
  trigger it selects four stops; above the trigger, fixed logical commands
  `[140, 120, 100, 80]` remap to physical `[80, 100, 140, 120]` and expected
  DShot values `[127, 147, 187, 167]`. Host tests pin both transformations.
  The feature retains the fresh `MotorCmd` queue, armed-only actuator owner,
  20 ms lease, 250-count cap, and whole-bank containment.
- [x] Reject unequal-vector mode without DShot, DShot without
  `bench_equal_motors`, unequal-vector plus a logical selection, and
  unequal-vector plus `pwm_cal`.
- [x] Build and identify the unequal-vector release candidate from commit
  `c4eeb90` plus the current working-tree changes. Its ELF SHA-256 is
  `78FD890B9D93DCA8D1A456548F0C89DB6153561496C1BB42B8D42238676DABF3`,
  its four DMA IRQ symbols remain at
  `0x080048F4/0x08004CFC/0x08004D54/0x08004DAC`, and its loadable flash end is
  `0x080130E8`. This is source-side candidate identity, not target evidence.
- [x] Rebuild the unequal-vector candidate after the DShot-specific arming
  change. The 2026-07-19 ELF has SHA-256
  `07A44529265B9895818F57C0B5CD608E35A9E6C95CF381373BE1372B60C24F1D`,
  retains DMA IRQ symbols at
  `0x080048F4/0x08004CFC/0x08004D54/0x08004DAC`, and ends its loadable flash
  data at `0x08012CC8`. Defmt metadata identifies both unequal-vector mode and
  the new stop-only arming sequence.
- [x] Run unequal-vector Part A in `mdbook/src/dshot.md` with propellers
  removed. On 2026-07-18, the operator reported completing the full
  five-report hold, three clean command-to-stop transitions, and explicit
  disarm. The retained excerpt directly preserves exact values
  `[127, 147, 187, 167]` for four consecutive reports from sets
  `46999/47000` through `49999/50000`, equal lane counters, one frame set in
  flight, zero backend faults, and continued IMU progress. Explicit disarm
  returned to sustained `[0, 0, 0, 0]` at `50999/51000` and `51999/52000`.
  The fifth active report and other repeated transitions are operator-observed
  but not retained in the excerpt.
- [x] Run Part B after Part A and the DShot arming checkpoint. On 2026-07-19,
  the operator reported that the complete unequal-vector RC-loss, sustained
  stop, arm-high recovery interlock, fresh-transition rearm, and final-disarm
  procedure passed.
- [x] Retain a complete Part B RTT capture. On 2026-07-19,
  `logs/terminal_embed/20260719_164203_rtt.log` preserved the exact unequal
  vector at sets `3999/4000` and `4999/5000`, followed by frame-timeout
  invalidation and four zeros from `5999/6000`. Zeros persisted through link
  recovery and additional link flaps, with no automatic arm request. A fresh
  guarded arm restored the exact vector at `21999/22000`; explicit disarm
  returned to zeros at `22999/23000`. All 23 status reports had equal lane
  counters, exactly one set in flight, and zero busy, expiry, timeout, and
  fault counts. IMU sequence advanced from 1 through 56,056. The flashed ELF
  SHA-256 matched
  `07A44529265B9895818F57C0B5CD608E35A9E6C95CF381373BE1372B60C24F1D`.
- [x] Add the explicit `dshot_mixed_control` candidate feature. It uses the
  normal PID/mixer branch through the bounded `MotorCmd` queue and sole
  actuator owner.
- [x] Promote bare `dshot` to the FCU3 default after source verification; keep
  `dshot_mixed_control` as a compatibility alias and preserve the capped DShot
  bench builds.
- [x] Keep four-channel PWM available explicitly with
  `--no-default-features --features board-ferrowasp-fcu3`.
- [x] Host-test four-lane command-to-DShot mapping so physical vector order,
  stop, minimum, maximum, and saturation behavior remain pinned.
- [x] Build and identify the corrected clean mixed-control release candidate.
  SHA-256 is
  `757F918B29771F0D62E7EBCC14B6DCE4A840E0062626B0A21F8F3EA7C6453269`;
  DShot IRQ symbols remain at
  `0x08004B44/0x08004F4C/0x08004FA4/0x08004FFC`, their Thumb addresses are
  present in the vector table, and loadable flash data ends at `0x08013968`.
  Metadata contains the mixed-candidate and stop-only arming identities
  without equal/unequal bench or legacy PWM-idle identities.
- [x] Retain the first unpowered mixed-control diagnostic run. Log
  `logs/terminal_embed/20260719_172718_rtt.log` matched the superseded hash
  `3EDC1D7767B119D9124874D96316EAEF16AF8EC6A950BBC926E4747CB7E66DAF`
  and reached 67,000 starts with synchronized lanes, advancing IMU, four stop
  values, and zero backend counters. It also exposed persistent roughly
  333 Hz service caused by the extra tick in RTIC relative delays.
- [x] Replace the DShot service's relative delay with a two-tick absolute
  deadline and append monotonic milliseconds to each periodic status report.
- [x] With ESC power disconnected, run the corrected standard image for at
  least 10,000 frame sets. Retained log
  `logs/terminal_embed/20260720_181201_rtt.log` used firmware SHA-256
  `5B91A09C509138D4762BBA0F9DC295A591BA474EF7587269435BC4BDBED84979`
  and reached 12,000 starts in 23,999 ms. All 12 reports carried four stops,
  synchronized lanes, exactly one in-flight set, and zero backend counters.
  Each 1,000-set interval took exactly 2,000 ms, establishing 500 frame sets
  per second, while IMU sequence advanced through 19,220.
- [x] In the historical pre-telemetry candidate, with propellers removed,
  verify mixed-control stop-only preparation, idle value `112` only after
  `SYSTEM ARMED`, modest throttle response, and explicit disarm from active
  output. Retained log
  `logs/terminal_embed/20260720_181535_rtt.log` used standard firmware SHA-256
  `5B91A09C509138D4762BBA0F9DC295A591BA474EF7587269435BC4BDBED84979`.
  The operator confirmed all motors idled, followed throttle, and stopped
  immediately on explicit disarm.
- [x] With props removed, verify small roll/pitch/yaw stick steps and
  motion-opposing corrections. The retained 2026-07-20 BB2 captures covered
  commanded roll/pitch/yaw and zero-command hand motion; controller output and
  the expected motor-pair differentials opposed measured motion on all axes.
- [x] Repeat cold boot/reset with arm high. On 2026-07-20 the operator removed
  and restored aircraft battery power while the controller remained armed, and
  separately reflashed the FCU while motors were running. Motors stopped and
  neither reboot automatically rearmed; a fresh low-to-high arm transition was
  still required. The probe session did not remain continuous across battery
  removal, so retain this as operator-observed reset/interlock evidence.
- [x] Validate PA10/USART1 legacy BLHeli telemetry and the bounded ESC-manager
  request/acknowledgement path in the default DShot image with powered ESCs.
  All four channels produced zero eRPM stopped, approximately 6,500-7,200 eRPM
  at idle, increasing eRPM with throttle, and returned to zero after RC loss
  and disarm without reported parser, queue, acknowledgement, or response
  errors. The PWM fallback does not run this telemetry service.
- [x] Record the ESC telemetry identity as physical output, with output 1 =
  logical M4/front-left, output 2 = M3/rear-left, output 3 = M1/rear-right, and
  output 4 = M2/front-right. A CRC-valid frame seen while its request is queued
  remains quarantined until the exact sequence/output frame-start
  acknowledgement arrives.
- [x] Validate telemetry-qualified DShot arming positively and negatively.
  Three normal attempts qualified all four outputs before `SYSTEM ARMED`; the
  canonical `bench_dshot_idle_output1_not_running` injection timed out after
  1.2 seconds, identified physical output 1 / logical M4, stopped all motors,
  and never armed. The old `bench_dshot_idle_motor1_not_running` feature name is
  only a compatibility alias.
- [ ] With propellers removed, exercise an arm attempt during the ESC manager's
  first five seconds. Confirm guarded idle fails closed at the 1.2-second
  deadline when samples are unavailable, all outputs stop, and a switch-low
  observation plus fresh low-to-high arm request is required before retrying.
- [ ] Confirm telemetry power-order behavior: booting the FC without ESC power
  through the first post-delay request latches telemetry off after the response
  timeout; applying ESC power alone does not recover it, while an FC reboot
  with ESC power present restores telemetry requests.
- [x] Interrupt the RC link during a nonzero mixed vector. The retained
  2026-07-20 log shows active vectors followed by sustained four-zero output,
  no arm-high recovery restart, guarded fresh-command rearm, and final
  explicit disarm. All 69 DShot reports through 69,000 starts had synchronized
  lanes, one in-flight set, and zero backend fault counters. The operator
  observed immediate physical stop; exact latency remains unmeasured.
- [ ] Use a logic analyzer to validate pulse widths, jitter, M4 complementary
  polarity, and TIM1/TIM8 lane phase. Counter and ESC interoperability evidence
  alone is not waveform evidence.
- [x] Complete a controlled experimental outdoor flight of the standard FCU3
  DShot600 image. On 2026-07-20 the operator reported a successful flight with
  strong maneuver capability and no recurrence of the previous unwanted
  yawing. No new BB2 flight capture accompanied the report. Pitch authority
  felt low; the next isolated candidate is pitch P `0.30` from `0.25`, with all
  other gains and control settings held constant.
- [ ] Before describing DShot as electrically timing-validated or recommending
  routine prop-on use, measure waveform timing, jitter, M4 polarity margin, and
  cross-timer synchronization. The successful experimental flight is useful
  interoperability evidence but does not close this checkpoint.

## ADC, Power, and Battery Data

- [ ] Confirm ADC channels correspond to internal temperature, voltage input, and current input.
- [ ] Confirm PC0 voltage scale against a bench supply and multimeter.
- [ ] Confirm PC1 current scale against a known load or current-limited supply.
- [ ] Confirm ADC DMA restart behavior over long runtime.
- [ ] Confirm out-of-range voltage/current values are detected or at least logged.
- [ ] Confirm battery cell count and voltage-divider assumptions match the attached board.
- [ ] Confirm power-sense faults cannot be hidden by OSD/display freshness issues.

## OSD, MSP, USB, and Telemetry

- [ ] Confirm UART4 TX/RX pins and 115200 8N1 settings against DJI O4/MSP wiring.
- [ ] Confirm MSPv1 responses are valid with a logic analyzer or serial capture.
- [ ] Confirm OSD displays armed state, IMU stale state, voltage, cell voltage, current, throttle, and sequence values.
- [ ] Confirm OSD loss or malformed MSP input cannot alter arming state or motor output.
- [ ] Confirm OSD update rate does not starve higher-priority control, IMU, RC, or actuator tasks.
- [ ] If `usb_serial` is enabled, confirm USB enumeration and the read-only
  header identifies the expected board.
- [ ] Confirm `FWDBG1` IMU/control sequences advance and RC, throttle, arm
  switch, voltage, and current fields track the corresponding RTT/OSD values.
- [ ] Disconnect and reconnect the USB host and confirm the header/status
  stream resumes without reset, panic, IMU timeout, or RC invalidation.
- [ ] Flood USB RX with arbitrary bytes and confirm they are ignored, arming
  state cannot change, and control/IMU timing remains healthy.
- [ ] Confirm telemetry/config commands remain display/config only unless explicitly validated by safety policy.

## Fault Injection

- [ ] Disconnect RC receiver while disarmed and armed.
- [ ] Disconnect or hold IMU chip select/SPI data to force stale samples.
- [ ] Stop ADC updates or inject out-of-range ADC values if feasible.
- [ ] Block or flood UART4 MSP traffic.
- [ ] Saturate low-priority telemetry paths and observe control-loop timing.
- [ ] Force actuator command queue overflow and confirm safe behavior.
- [ ] Trigger panic/reset path on bench and confirm outputs return low.
- [ ] Record the observed safe-output latency for each injected fault.

## Timing and Evidence

- [ ] Capture IMU poll, control loop, actuator update, RC parse, ADC completion, and OSD update timing.
- [ ] Record interrupt priorities and confirm safety/actuator paths outrank telemetry/display work.
- [ ] Record worst observed control-loop jitter during normal operation and telemetry load.
- [ ] Record worst observed time from disarm request to low motor output.
- [ ] Record worst observed time from the current post-arm stale-IMU check and
  RC loss to actuator inhibit; separately verify the future IMU pre-arm guard.
- [ ] Save logic analyzer traces for PWM and future DShot output.
- [ ] Save serial/RTT logs for arm, disarm, RC loss, IMU stale, ADC, and OSD test cases.
- [ ] Link each passed target check to a commit and board configuration.

## Foxeer F405 V2 Initial Gate

The Foxeer app must remain arming-inhibited until every item in this section is
measured on the physical board. Props and motor power must remain disconnected.

- [x] Record the baseline non-USB `apps/foxeer-f405-v2` build; ELF
  `target/thumbv7em-none-eabihf/release/FerroWaspFoxeerF405V2`, 72,352-byte
  `.bin`, SHA-256
  `F552B767FD834168361E722A74C0626713D58DF013A7F2AC0FDF7DE7D5E90122`,
  generated with `.\flash-dfu.ps1 -BuildOnly`. The active DFU workflow passes
  the ELF to STM32CubeProgrammer through `cargo run --release --locked`.
- [x] Build the opt-in USB diagnostic image with
  `.\flash-dfu.ps1 -BuildOnly -UsbDebug`; 79,672-byte `.bin`, SHA-256
  `120E02F024D4D9C4AE903C075C3D1C9F67BF834F33F0AB0488A8610E74A60173`.
- [x] Validate the native Windows Cargo DFU runner without flashing:
  STM32CubeProgrammer 2.23.0 and the signed STM32 bootloader driver were
  detected, the ELF load origin was confirmed at `0x08000000`, and both
  `cargo run` and `flash-dfu.ps1` completed with
  `FERROWASP_DFU_DRY_RUN=1`.
- [x] Enter ROM DFU physically and confirm `dfu-runner.ps1 -ListOnly` reports
  exactly one intended `USBn` device before the first write. On 2026-07-18,
  CubeProgrammer 2.23.0 detected `USB1` as device `0x0413`, programmed and
  verified the 77.80 KiB USB-debug ELF with SHA-256
  `23DB5D9F400BC3769120280079B83EB2A4DE065216C27E5A50B0B8D1466678F2`,
  then successfully started it at `0x08000000`; the device left ROM DFU.
- [x] Confirm the USB image enumerates as `FerroWasp Foxeer Debug` and emits
  the read-only header plus advancing `FWDBG1` frames. On 2026-07-18 it
  enumerated as COM6 after a normal reconnect; IMU sequence advanced at about
  800 Hz and control sequence at about 400 Hz with `stale=0`.
- [x] Complete a five-minute USB diagnostic soak without a disconnect or
  malformed frame. The 2026-07-18 run captured 152 continuous frames over
  300.584 seconds: IMU 799.996 Hz, control 400.001 Hz, maximum report gap
  2001 ms, and zero stale frames, readiness failures, timestamp regressions,
  IMU/control sequence regressions, serial errors, or safety-state changes.
- [ ] Confirm cold boot, reset, RTT heartbeat, and a sustained run without panic.
- [x] Read and record the fitted SPI1 IMU identity using mode 3. `FWDBG1`
  reported `imu=icm42688p ready=1`; this state is published only after the
  mode-3 probe matches `WHO_AM_I=0x47` and ICM42688-P configuration succeeds.
- [ ] Confirm the selected IMU produces advancing sequence numbers and
  plausible stationary accel/gyro/temperature values for at least five
  minutes without SPI timeout, invalid-frame, or stale-IMU warnings. The
  five-minute USB sequence/stale/gyro portion passed; accel, temperature, and
  explicit RTT warning observation remain.
- [ ] Verify raw accelerometer/gyro axes and signs against board motion.
- [ ] Confirm USART2 SBUS qualification, timeout invalidation, and rearm latch.
- [ ] Confirm UART4 DJI MSP DisplayPort and live throttle/battery updates.
- [ ] Calibrate PC0 voltage and PC1 current against external instruments.
  The USB-only runs correctly reported `vbat_dV=0`, but observed
  `current_cA=793..820` is an uncalibrated offset and must not be accepted as
  a physical current reading.
- [ ] Scope PA8, PC9, PC8, and PB15 with ESC power disconnected.
- [ ] Verify the active RC PWM protocol is 400 Hz with a 1000..2000 us pulse
  range on all four outputs.
- [ ] Verify PB15 is the intended active-high M4 waveform from `TIM1_CH3N`.
- [ ] Verify Betaflight logical rear-right/front-right/rear-left/front-left maps
  to Foxeer outputs M1/M2/M3/M4 before changing
  `MOTOR_OUTPUT_ORDER_VERIFIED`.
- [ ] Verify reset/boot and forced-off behavior produce no unintended pulse.
- [ ] Keep PA13/PA14 available for SWD; do not depend on the shared status LEDs.
- [ ] Update the BSP verification constants only from captured evidence and
  review that change separately before any powered actuator test.
