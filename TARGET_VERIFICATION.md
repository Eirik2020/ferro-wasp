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
- [ ] Confirm FCU3 IMU sequence increments at its intended 800 Hz poll rate;
  confirm Foxeer follows its approximately 1 kHz PC4/EXTI4 data-ready rate.
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
- [ ] Record worst observed time from stale-IMU detection and RC loss to
  actuator inhibit; target-verify the implemented IMU pre-arm guard negatively.
- [ ] Save logic analyzer traces for PWM and future DShot output.
- [ ] Save serial/RTT logs for arm, disarm, RC loss, IMU stale, ADC, and OSD test cases.
- [ ] Link each passed target check to a commit and board configuration.

## Foxeer F405 V2 Initial Gate

This section records the staged Foxeer gate. The compile-time flight profile is
now promoted from the captured evidence below, but propellers must remain off
until the final normal-mixer/OSD handoff at the end of this section passes.

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
- [x] Build `flash_storage`, run `tools/ferrowasp_storage.py --port COMn info`,
  and record the fitted SPI2 NOR JEDEC ID. On 2026-07-21 the read-only image
  reported `jedec=ef:40:18 bytes=16777216 ready=1`, confirming capacity code
  `0x18` and the advertised 16 MiB capacity without inferring the manufacturer
  or memory-type bytes from board documentation. The image SHA-256 was
  `2771E422EDB7D08A26AFBDBD8C18D8A4A0B28F9B516D4A5404E555AA9D8208F9`.
- [x] In the read-only image, run `list` and confirm existing foreign flash
  contents produce `writable: False` rather than being overwritten. The
  2026-07-21 result was `used pages: 0; next flight: 1; capacity pages: 65488;
  writable: False`; no erase or program command was issued.
- [ ] Confirm rejected out-of-range reads do not disturb IMU, RC, OSD, USB
  status, or actuator-inhibit behavior. During the successful `info`/`list`
  session, `logs/terminal_embed/20260721_215409_rtt.log` retained approximately
  1.012 kHz IMU/DRDY progress for about 72 seconds with zero rejected DRDY
  events and no SPI/storage fault; the explicit rejected-read case and live
  RC/OSD coexistence remain open.
- [x] Build `flash_writes` and run `test --confirm` while disarmed. Capture
  `OK flash scratch erase/program/read verified`; repeat after a cold reset.
  This test may alter only the reserved sector at `0x2000`. The initial
  2026-07-21 run passed with `OK flash scratch test started` followed by
  `OK flash scratch erase/program/read verified`; the subsequent read-only
  `list` still reported zero FerroWasp pages and `writable: False`. The image
  SHA-256 was
  `3747FD6C20A12A7661F0CB656C199DA1FFFDAAAA04215392935B28FBB1DBD8C6`,
  with RTT retained in `logs/terminal_embed/20260721_220055_rtt.log`. After a
  cold power cycle, the first USB attempts correctly found no COM port because
  the debugger held NRST low. Releasing NRST allowed the already-programmed
  image to enumerate as COM6 without reflashing; the repeated `info`, scratch
  erase/program/readback, and `list` sequence passed with the same identity and
  protected-log result.
- [ ] Stage an allowed configuration value, save it, reboot, and confirm it is
  restored from the newer CRC-valid copy-on-write slot. Interrupt power during
  a later save and confirm either the old or new complete slot is selected,
  never a torn value. Restore the intended default before actuator work. On
  2026-07-21 `log_rate_divisor` was staged from `1` to `2`, saved, and recovered
  as `2.0000` after a cold boot without reflashing. It was then staged and saved
  back to `1`; the live read returned `1.0000` and the foreign log region stayed
  `writable: False`. One final cold-boot read of the restored value and the
  interrupted-save case remain open.
- [ ] While armed only under an appropriate props-off commissioning gate,
  confirm configuration changes, log reads/erase, and the scratch self-test
  are rejected. Start maintenance while disarmed, initiate arming, and confirm
  maintenance aborts without granting USB any safety or motor authority.
- [x] Build `dshot flash_blackbox bench_actuator_validation
  bench_equal_motors`,
  perform one props-off disarmed/armed/throttle/disarmed run, download the
  `.fwbb`, and verify it with
  `tools/blackbox_analyzer.py`. Confirm CRC-valid records, a final partial page,
  plausible IMU/control sequences, motor values, and zero reported storage
  write faults or unexplained record drops. This passed on 2026-07-22 with
  image SHA-256
  `47B78242AF0DC46C69C9225042C96BCC4D4F3371AE68FFDB1CE7ACE3C5CAB844`
  and RTT log `logs/terminal_embed/20260722_014010_rtt.log`. The capture armed
  only after `[3, 3, 3, 3]` eRPM qualification, logged idle and equal throttle,
  explicitly disarmed to four DShot zeros, and settled at 551 pages with zero
  dropped records and write faults. Download
  `logs/foxeer-blackbox.fwbb` has SHA-256
  `4950B505EDA22BA34AA25A69AC85E560B4EC29A3A0B4B9D071FF026546A71DCE`:
  all 551 pages pass CRC, flight ID is 1, page sequences are contiguous
  `0..550`, 550 pages contain five records, and the final flushed page contains
  four records. Its 2,754 stored records are contiguous at 400 Hz with no
  missing BB2 or repeated IMU sample; the analyzer reports 2,751 after its
  default three-record warm-up trim. Estimated IMU rate is 1,011.2 Hz with
  deltas only 2/3, and all four motor channels capture the same 120-count
  peak-to-peak bench command.
- [x] Measure control-loop/IMU timing with onboard logging enabled. The
  CRC-valid 2026-07-22 capture contains 2,750 contiguous timestamp intervals:
  mean 2,500 us, 2,000/3,000 us min/max, 500 us standard deviation and p99
  absolute jitter, and no interval above 3,000 us. This is the expected
  quantization of the 1 ms recorded timebase and proves no missed 400 Hz log
  deadline at that resolution; it does not replace a finer electrical timing
  measurement.
- [ ] Confirm UART4 OSD remains live while onboard logging is active and no
  OSD/SBUS transport fault appears.
- [ ] Confirm cold boot, reset, RTT heartbeat, and a sustained run without panic.
  Begin with `python tools/terminal_embed.py --foxeer-smoke`; retain its log,
  firmware hash, and PASS/FAIL summary. Keep ESC power disconnected.
  The 2026-07-21 SWD reset/program/RTT smoke passed at
  `9A3AB250FD48D27BCA32099BAB04DFD7A6E396D4082AFAC6728A30825D0DF268`;
  `logs/terminal_embed/20260721_214256_rtt.log` contains six approximately
  1 kHz DRDY intervals with zero rejected triggers. A separate cold-power soak
  remains open.
- [x] Read and record the fitted SPI1 IMU identity using mode 3. `FWDBG1`
  reported `imu=icm42688p ready=1`; this state is published only after the
  mode-3 probe matches `WHO_AM_I=0x47` and ICM42688-P configuration succeeds.
- [x] Functionally validate the new PC4/EXTI4 path. Each two-second RTT heartbeat should
  report an IRQ delta near 2,000 for the 1 kHz ICM42688-P, advancing IMU
  sequence, and zero or explainably bounded rejected triggers. Scope PC4 to
  confirm an active-high data-ready pulse and record pulse width and cadence.
  The earlier five-minute USB soak used timer polling and does not close this
  checkpoint. The 2026-07-21 and 2026-07-22 captures repeatedly measured
  2,024-2,025 IRQ/sample increments per two seconds with zero rejected triggers
  and no post-startup stale warning. The electrical scope portion was
  intentionally skipped and remains unclaimed.
- [ ] Capture at least ten stationary seconds with `blackbox_defmt` and run
  `tools/blackbox_analyzer.py --mode rest`. Record the estimated IMU rate,
  repeated-sample count, missing-BB2 count, and contiguous IMU-delta histogram.
  At 1 kHz IMU / 400 Hz control, expect about 1,000 Hz, no repeated samples,
  and primarily sequence deltas 2 and 3. Missing BB2 text frames are RTT
  transport loss and do not by themselves indicate lost sensor samples.
- [ ] Confirm the selected IMU produces advancing sequence numbers and
  plausible stationary accel/gyro/temperature values for at least five
  minutes without SPI timeout, invalid-frame, or stale-IMU warnings. The
  five-minute USB sequence/stale/gyro portion passed; accel, temperature, and
  explicit RTT warning observation remain.
- [x] Verify raw accelerometer/gyro axes and signs against board motion. Use
  the observational `imu_orientation_rtt` image with ESC power disconnected;
  capture level, then sustain slow nose-up, right-side-down, and clockwise-yaw
  motions for at least four seconds each. Its `IMU ORIENT sensor` line reports
  coherent sensor-frame acceleration in mg, gyro in 0.1 degrees/second, and
  temperature in 0.1 degrees Celsius without changing control or actuator
  behavior. The
  2026-07-21 capture `logs/terminal_embed/20260721_232959_rtt.log`, image
  SHA-256 `70E04FDDAAD7EC297B35BC1BE770FE1CEDDCB22A99187E7673AEC7B84A234FF8`,
  established level acceleration near `[0, 0, +995]` mg, nose-up motion on
  negative sensor gyro X with gravity moving toward negative sensor Y, and
  opposite yaw signs on sensor gyro Z. A follow-up explicitly lifted both left
  motors, producing the required right-side-down/positive-roll motion in
  `logs/terminal_embed/20260721_233738_rtt.log`: sensor gyro Y was negative,
  held acceleration moved to approximately `[+780, -30, +620]` mg, and the
  return motion reversed gyro Y. Together the captures establish
  `FrameRotation::new([1, 0, 2], [-1, -1, -1])` for gyro, with negated mapped
  specific force supplying the estimator's drone-frame gravity vector.
- [x] Reflash `imu_orientation_rtt` with the measured BSP mapping and confirm
  its `IMU ORIENT body` line: level gravity must be approximately `[0, 0,
  +1000]` mg; right-side-down must produce positive body Y gravity and positive
  roll rate; nose-up must produce negative body X gravity and positive pitch
  rate; nose-right/CW yaw must produce positive body yaw. Keep flight arming
  inhibited while collecting this implementation evidence. The 2026-07-21
  mapped capture `logs/terminal_embed/20260721_234528_rtt.log`, image SHA-256
  `3832A346EACDD86B910EF21CE88821D17FAE8B6F39844A549407424FD6405591`,
  passed: level gravity was approximately `[0, 0, +995]` mg; lifting the left
  side produced positive roll and positive body Y gravity; nose-up produced
  positive pitch and negative body X gravity; nose-right yaw was positive and
  the return yaw was negative. Final level gravity returned to approximately
  `[0, 0, +995]` mg. All 2,024-2,025-sample DRDY intervals had zero rejects,
  with no post-startup stale or transport warning.
- [x] Confirm USART2 SBUS qualification, timeout invalidation, and rearm latch.
  The powered props-off M4 session on 2026-07-22 confirmed healthy-frame
  qualification, immediate motor stop on link loss, no automatic rearm after
  recovery, and the required explicit arm-low then arm-high sequence. It also
  confirmed that raising throttle during PWM preparation aborts arming.
  However, the M3 image armed after a flash/reset while the transmitter arm
  switch had remained high at zero throttle. Log
  `logs/terminal_embed/20260722_000038_rtt.log` records link qualification
  followed by `RC Requests ARM!` and `SYSTEM ARMED FOR CAPPED FOXEER ACTUATOR
  VALIDATION`. The shared RC latch now ignores arm-low observations received
  before link qualification completes. The exact rebuilt
  `bench_actuator_validation bench_motor3_only` candidate has SHA-256
  `9482D89270F4D7D6C1F5E83D60F42817DB90AABEB75D119D6F66E15C2FACCA3D`.
  Its 2026-07-22 repeat in
  `logs/terminal_embed/20260722_001436_rtt.log` held the transmitter arm switch
  high across flashing: the RC link qualified, then ran for approximately 14
  seconds without `RC Requests ARM!`, PWM preparation, idle, or an armed
  transition. This closes the observed boot-high regression. Together with
  the earlier powered M4 loss/recovery test, startup and reconnect both require
  a valid arm-low observation before a later high transition can request arm.
- [ ] Confirm UART4 DJI MSP DisplayPort and live throttle/battery updates.
- [ ] Calibrate PC0 voltage and PC1 current against external instruments.
  The USB-only runs correctly reported `vbat_dV=0`, but observed
  `current_cA=793..820` is an uncalibrated offset and must not be accepted as
  a physical current reading. The flight baseline now uses the upstream
  Betaflight default VBAT scale 110 (11.0 ratio), Foxeer's published current
  scale 70, and repeated powered observations of 23.2-24.0 V. Fine calibration
  remains open; firmware forces displayed `current_cA` to zero until the PC1
  zero offset is calibrated while retaining raw `adc_i_mV` for that work.
- [ ] Scope PA8, PC9, PC8, and PB15 with ESC power disconnected. This
  electrical check was intentionally skipped for the current bring-up; do not
  claim measured pulse width, frequency, idle level, or cross-timer phase.
- [ ] Verify the active RC PWM protocol is 400 Hz with a 1000..2000 us pulse
  range on all four outputs. All four ESCs decoded the configured PWM output,
  but exact timing remains unmeasured because the electrical check was
  skipped.
- [x] Functionally verify PB15's configured active-high `TIM1_CH3N` polarity.
  The M4 image drove front-left normally through idle and capped throttle,
  then stopped on disarm and RC loss. This validates the selected functional
  polarity but is not an electrical waveform measurement.
- [x] Verify Betaflight logical rear-right/front-right/rear-left/front-left maps
  to Foxeer outputs M1/M2/M3/M4. Powered props-off PWM selection on 2026-07-22
  confirmed M1 rear-right CW, M2 front-right CCW, M3 rear-left CCW, and M4
  front-left CW. The retained logs and image hashes are:
  `20260721_235448_rtt.log` / `15B4598653AB7315658CC186A8047908E87F2E50E01B81481174B7DCD70DBA1F`,
  `20260721_235936_rtt.log` / `D82149A61BA74AA8204539B9D3FD851856A3584972D2C817C9C4008B10112E21`,
  `20260722_000038_rtt.log` / `7CCB9A51AA39D6A47AE84D5BF728398C81DA42A62A3C42463F172D7C67846708`,
  and `20260722_000213_rtt.log` /
  `99FE33235FCAC50F62D8C8132CD253B2123FCF96D2C120F32C65A7F0EA84B745`.
- [x] Run the capped equal-motor PWM image and verify all four outputs enter
  idle together, respond together to a small throttle command, and stop on
  explicit disarm. The 2026-07-22 powered props-off test passed by operator
  observation using image SHA-256
  `F84433707C03FC7E477981C6760189D920F507B2B8BB2A9C55A2C5DD41A1ABE9`.
  `logs/terminal_embed/20260722_001849_rtt.log` records guarded arming, explicit
  disarm, continued approximately 1.012 kHz IMU/DRDY progress with zero
  rejected triggers, and no automatic arm observed at boot.
- [x] Build and statically identify the first Foxeer DShot600 commissioning
  candidate with `dshot bench_actuator_validation bench_equal_motors`. The
  2026-07-22 release ELF has SHA-256
  `2D3BC3824405D9FED247F4FF52BB266FD1ABC893A1A113854E9E9EFC7EDAB245`;
  `DMA2_STREAM1/2/6/7` are bound at
  `0x08004BD4/0x08004BF0/0x08004F42/0x08004F5E`.
- [x] With ESC power disconnected, run the Foxeer DShot600 candidate for at
  least 20 seconds. Require synchronized lane counters, one or fewer frame
  sets in flight, and zero busy, expiry, timeout, fault, and spurious-IRQ
  warnings. Exact electrical timing remains unmeasured because the analyzer
  checkpoint was skipped. The 2026-07-22 run in
  `logs/terminal_embed/20260722_002358_rtt.log` reached 25,000 starts over 50
  seconds. Every report contained values `[0, 0, 0, 0]`, completed exactly one
  frame behind started, and four identical lane counters; busy, expiry,
  timeout, and fault counters remained zero. No spurious IRQ warning appeared.
  IMU DRDY advanced by 2,024 samples per two-second interval, apart from one
  explainable 2,025 interval, with zero rejected triggers.
- [x] Diagnose the first powered Foxeer DShot arming attempt. Although the
  shared timer-DMA backend was active, Foxeer's app-local `EnterIdle` branch
  still executed the legacy 2.5-second PWM-low plus 500 ms idle preparation.
  The operator observed all four motors spin during that inappropriate
  pre-armed idle interval and stopped the test. This was an app-shell
  orchestration defect, not a DShot transport or safety-authority bypass.
- [x] Split Foxeer actuator preparation by protocol, matching FCU3's DShot
  branch structure. PWM retains its existing low/idle holds. DShot now emits
  stop frames for a guarded 100 ms safety-recheck dwell, reports preparation
  complete while all outputs remain stopped, and permits idle only after the
  safety master sets `SYSTEM ARMED`. The corrected DShot candidate SHA-256 is
  `06AB74FCA6678AA2116297DA8CDFC19DD2F6677004D077DC2A4B6853132D69A6`.
  Its ELF contains `Attempting DShot safety arming`, `Preparing DShot
  actuators`, and `DShot pre-arm complete; motor outputs remain stopped`, with
  none of the PWM arming/idle identities. The default PWM ELF retains the PWM
  identities and none of the DShot preparation identities.
- [x] Exercise complete arm/disarm, throttle, and RC-loss transitions on the
  corrected DShot image. Receiver and ESC power cannot be separated on this
  installation, so the ordering regression was completed powered with
  propellers removed after the corrected-image stop-only soak in
  `logs/terminal_embed/20260722_004059_rtt.log` reached 20,000 starts over 40
  seconds with four zeros, synchronized lanes, and zero backend errors, but
  no RC link. The powered run in `20260722_004429_rtt.log` then showed the
  exact DShot 100 ms stop preparation before `SYSTEM ARMED`; values remained
  zero before that transition, idled at `112`, followed equal throttle through
  `172`, `244`, and `138`, and returned to four zeros on explicit disarm.
  A second arm held `112` until RC frame timeout, after which the next DShot
  report contained four zeros. Restoring the link with arm high did not rearm;
  a later explicit low-to-high request completed the same DShot preparation.
  All lane counters stayed synchronized and busy, expiry, timeout, and fault
  counts remained zero through 59,000 starts. Short repeats
  `20260722_004643_rtt.log` and `20260722_004712_rtt.log` additionally captured
  throttle value `297`, idle `112`, explicit disarm to zero, and no automatic
  arm after flash/boot. Operator observation confirmed all motors idled only
  once armed, followed throttle, and stopped on disarm and RC loss. No PWM
  preparation message appeared.
- [x] Cross-check and build the observational Foxeer PA10 BLHeli telemetry
  candidate against the FCU3 golden app. Both use the shared bounded ESC
  manager at 2 ms, PA10 / USART1 RX with DMA2 Stream 5 Channel 4, and a
  sequenced request/ack path whose only request consumer is the safety-owned
  DShot service. Foxeer intentionally does not use eRPM to qualify arming yet.
  The first 2026-07-22 release ELF SHA-256 was
  `8DFAF9EC34D22F91D2410D93FB2DA285A13557A37C389E486C05968843188C41`.
- [x] Diagnose the first powered Foxeer telemetry run. Log
  `logs/terminal_embed/20260722_005707_rtt.log` proved PA10 received 14,498
  CRC-valid frames with no CRC failure or discarded byte and preserved clean
  DShot/motor safety behavior. It did not prove request association:
  `queued/started` remained `0/0` while mismatched acknowledgements reached
  14,499 and unsolicited frames reached 14,498. Foxeer had put the
  state-changing `mark_request_queued` call inside `debug_assert!`, so release
  compilation removed it and the actuator emitted a request every 2 ms. The
  call now always executes, matching FCU3; only its result is debug-asserted.
  The corrected release ELF SHA-256 is
  `CCD4B30A5CA900E28BF1C07D75E63E59353206EE870427A32A93787D74DBD16F`.
- [x] With propellers removed, power the FC, receiver, and ESC bank together
  and validate observational PA10 telemetry. Require a valid sample from all
  four explicitly labelled physical/logical outputs, zero eRPM while stopped,
  nonzero eRPM at armed idle, rising eRPM with throttle, and return to zero
  after disarm. Require no latched manager fault, acknowledgement/response
  timeout, mismatched acknowledgement, or unsolicited response; CRC/discard
  counters must remain stable. The corrected candidate passed in
  `logs/terminal_embed/20260722_010421_rtt.log`: all four outputs reported zero
  stopped, approximately 6,600-7,200 eRPM at DShot idle value `112`, and
  approximately 10,100-10,700 eRPM at throttle value `133`. Disarming while
  throttle was raised selected four zero DShot values and all four eRPM
  readings returned to zero by the next report. `queued/started/valid` reached
  `2450/2450/2450`; manager fault, acknowledgement timeout, response timeout,
  mismatched acknowledgement, unsolicited frame, CRC failure, discarded byte,
  and every DShot backend fault counter remained zero.
- [x] Promote Foxeer DShot arming to the FCU3 golden eRPM-qualified policy.
  Feature `dshot` now includes `esc_telemetry`; after the guarded 100 ms stop
  dwell, the actuator owner applies idle only under its temporary permit and
  requires three consecutive fresh 3,000-10,000 eRPM observations from every
  physical output after a 250 ms spin-up grace. The deadline is 1.2 seconds
  and maximum sample age is 200 ms. The normal candidate SHA-256 is
  `28C5E4BDC4C9B51998385A434983D83035B5FAE8136E8437E8B07ABF9A8A2B70`;
  the physical-output-1/logical-M1 zero-eRPM injection candidate is
  `9A7A14751747969CDE80265AD2ADA7AB0464A423A1D4A64DC1DE6CED9E8A9073`.
- [x] With propellers removed, wait at least five seconds after boot, request
  arm at zero throttle, and verify the positive candidate reports the guarded
  stop dwell, temporary idle, four `[3, 3, 3, 3]` qualification counts, then
  and only then `SYSTEM ARMED`. Candidate
  `28C5E4BDC4C9B51998385A434983D83035B5FAE8136E8437E8B07ABF9A8A2B70`
  passed in `logs/terminal_embed/20260722_012353_rtt.log`: all four outputs
  qualified before the armed transition, idle telemetry remained in range,
  explicit disarm selected four zeros, and every DShot/telemetry fault counter
  remained zero.
- [x] Run the injected candidate with propellers removed. Physical output 1 /
  logical M1 rear-right may physically spin, but its qualification evidence is
  forced to zero. Require a 1.2-second timeout naming that output, four stop
  values, no `SYSTEM ARMED`, and a fresh arm-low observation before any later
  arm request. Candidate
  `9A7A14751747969CDE80265AD2ADA7AB0464A423A1D4A64DC1DE6CED9E8A9073`
  passed twice in `logs/terminal_embed/20260722_012445_rtt.log`: qualification
  counts were `[0, 12, 12, 12]` and `[0, 13, 12, 12]`, output 1 / logical M1
  was named, all outputs returned to zero, and `SYSTEM ARMED` never appeared.
  Normal diagnostic telemetry still observed M1 physically turning, proving
  that the injection changed evidence rather than motor authority.
- [ ] Add deterministic qualification-interruption testing rather than asking
  an operator to break RC, raise throttle, or disarm inside the 1.2-second
  qualification window. Prefer host-controlled/fault-injected guard revocation
  with a timestamped expected stop deadline. Existing RC-loss tests establish
  live stop/rearm behavior, and the injected eRPM test establishes the
  qualification failure stop path, but their composition is not a direct
  measurement of guard revocation during qualification.
- [x] Functionally verify reset/boot with arm held high cannot automatically
  arm the system. The original 2026-07-22 M3 test failed, but the tightened
  candidate repeat in `20260722_001436_rtt.log` remained disarmed throughout
  the observation window. Exact reset-time pad behavior remains unmeasured
  because the electrical analyzer test was intentionally skipped.
- [x] Keep PA13/PA14 available for SWD; do not depend on the shared status LEDs.
  Retrofitted SWD repeatedly flashed and streamed RTT at 1,800 kHz with
  connect-under-reset once the physical reset connection was handled.
- [x] Update the BSP verification constants only from captured evidence. The
  flight profile now combines target-verified IMU identity/orientation,
  motor order, functional M4 polarity, DShot/eRPM behavior, the documented ADC
  baseline, and explicit deferred-fine-calibration flags. Review the resulting
  diff again before the final powered normal-mixer test.
- [x] Add a shared pre-arm IMU health policy before enabling the Foxeer flight
  profile. Both FCU3 and Foxeer now require an IMU sample, completed gyro-bias
  calibration, and fresh data before actuator preparation, during guarded
  preparation, and before the final armed transition. Stale samples cannot
  advance bias calibration. Host tests cover unavailable, uncalibrated, and
  stale rejection.
- [x] Promote DShot600 plus PA10 eRPM qualification to the Foxeer default motor
  protocol. RC PWM remains an explicit no-default-features fallback, and the
  smoke preset adds a separate compile-time actuator lockout.
- [x] Final normal-mixer props-off flight handoff: boot still for bias
  calibration, confirm the healthy IMU gate permits arm, exercise low roll,
  pitch, yaw, and throttle commands, confirm live OSD voltage/throttle/armed
  state while `flash_blackbox` records, then verify explicit disarm and RC loss
  select four stop values. Do not install propellers until this passes and its
  exact ELF hash/log are retained. This passed on 2026-07-22: OSD reported
  24.9 V against 25.05 V at the pack, all four outputs qualified `[3,3,3,3]`,
  low commands exercised the motors, explicit disarm and RC timeout selected
  four stop values, restored arm-high did not rearm, and telemetry completed
  12,150/12,150 requests without a protocol fault. The retained RTT log is
  `logs/terminal_embed/20260722_180528_rtt.log`; the tested release ELF is
  SHA-256
  `E4BAE2A6229D1B340E4DF72BF0727D00506989FE9A1DCDE3B71935B4D6BC9758`.
- [x] Immediately before the normal handoff, build `bench_prearm_imu_stale`,
  wait for real gyro-bias calibration, and request arm at zero throttle. Require
  the explicit forced-stale banner and `Arming aborted: IMU sample is stale`,
  with no temporary idle or `SYSTEM ARMED`. Reflash without the injection before
  any further motor test. This passed without temporary idle or
  `SYSTEM ARMED`; the retained RTT log is
  `logs/terminal_embed/20260722_180351_rtt.log` and the tested injection ELF is
  SHA-256
  `EFFC154E59F61896B3AB3EE3D91A424B0110C461012807B84BF399E72BD155F5`.

The final onboard download contains 11,097 CRC-valid pages across historical
flight IDs 1-7. Analyzing flight 7 independently yields 3,477 contiguous
400 Hz samples, zero missing BB2 frames, zero repeated IMU samples, an
estimated 1,011.2 Hz IMU rate, and no recorded control interval above 3 ms.
All three filtered gyro axes classify quiet. The retained files are
`logs/foxeer-final-props-off.fwbb` and
`logs/foxeer-final-props-off-flight7.csv`.

### Foxeer first-hop corrective gate (2026-07-22)

- [x] Stop after the first prop-on departure attempted an immediate forward
  flip. Do not reuse the previously retained
  `E4BAE2A6229D1B340E4DF72BF0727D00506989FE9A1DCDE3B71935B4D6BC9758`
  image for flight.
- [x] Recover the interrupted onboard download before diagnosing the event.
  `ferrowasp_storage.py read --resume` now validates every existing complete
  page's magic and CRC before appending. The completed archive contains 16,974
  CRC-valid pages, 84,827 records, and flight IDs 1-22; IDs 12 and 13 contain
  the powered departure evidence. Retained archive
  `logs/foxeer-hop-front-flip.fwbb` has SHA-256
  `DB6E82BFE6E6902BAB26658C4BC9F2FDB3B71FE6E8FF1C05E1ACB0C9AC348537`.
- [x] Diagnose the feedback sign from the recorded control data rather than
  changing gains. With zero pitch command, flight 12 sequence 11,407 recorded
  controller pitch `-241.0 dps`, pitch PID `+60`, throttle `550`, and
  M1/M2/M3/M4 `624/487/597/492`. Raising rear M1/M3 over front M2/M4
  reinforces a physical nose-down motion, establishing positive pitch
  feedback. Flight 13 independently shows the same polarity.
- [x] Preserve the measured right-handed physical body map
  `[-sensor Y, -sensor X, -sensor Z]`, but add an explicit physical-body to
  FCU3-controller compatibility transform `[roll, -pitch, yaw]`. Foxeer rate
  control therefore consumes `[-sensor Y, +sensor X, -sensor Z]`, while
  accelerometer/orientation reporting and the complementary estimator remain
  in the physical body convention. Host regressions cover nose-up, nose-down,
  preserved roll/yaw signs, transform composition, and opposing front/rear
  mixer output.
- [x] With propellers and ESC power removed, flash `imu_orientation_rtt` over
  SWD and verify the physical/controller transform. This passed with image
  SHA-256
  `C639C8BD3476E8415632644E970D4BAB3B42417FD9C624428B6D5D343D37F7FA`
  and `logs/terminal_embed/20260722_212252_rtt.log`. Nose-up at sequence 6,075
  produced body/control pitch `+64/-64` dps10 with body gravity X `-372` mg;
  the nose-down return at sequence 12,150 produced `-60/+60` dps10. A
  right-side-down region reached body/control roll `+95/+95` dps10 with
  gravity Y `+530` mg. Nose-right yaw reached `+284/+284` dps10 and the return
  reached `-318/-318` dps10. Two-second DRDY deltas were 2,025 except one
  2,026 interval, with zero rejected triggers. The single ESC response timeout
  is expected with ESC power disconnected; all four DShot values stayed zero
  and actuator counters remained fault-free.
- [x] With propellers removed and ESC power connected, flash
  `blackbox_defmt` over SWD and repeat the normal-mixer opposition check under
  modest throttle. Physical nose-down must produce positive controller pitch,
  negative pitch PID, and front M2/M4 above rear M1/M3. Nose-up must produce
  negative controller pitch, positive pitch PID, and rear M1/M3 above front
  M2/M4. Confirm roll and yaw still oppose motion, then disarm and retain the
  RTT evidence. This passed with image SHA-256
  `FF6606EFACC55C9C88CCDE5EC044C0CB3B3881A5229319C26820621654DD136F`
  and `logs/terminal_embed/20260722_212914_rtt.log`, SHA-256
  `C13C1352041105CBBCDC95E17FB6AC2A82B7BB4C6901A3C6E1D0ADB08D8FB9A8`.
  With centred axis commands and throttle above 100, every selected motion
  sample had both the opposing PID sign and correct motor-pair polarity:
  pitch 594/594, roll 175/175, and yaw 72/72. Representative pitch samples
  were sequence 13,501 at `+35.2 dps`, PID `-9`, M1/M2/M3/M4
  `92/110/92/110`, and sequence 14,531 at `-19.0 dps`, PID `+5`, motors
  `106/96/106/96`. Roll and yaw likewise raised the motion-opposing pairs.
  The run armed only after normal eRPM qualification, completed 2,950/2,950
  associated telemetry requests without a protocol fault, kept DShot lane and
  fault counters clean, explicitly disarmed, and retained four zero outputs.
  The analyzer recovered 25,239 BB2 samples with zero repeated IMU samples and
  an estimated 1,011.9 Hz IMU rate; 572 missing BB2 sequence values are RTT
  text-transport loss, not control or IMU loss. Derived CSV
  `logs/foxeer-corrected-props-off-rtt.csv` has SHA-256
  `CC0FF31814B1656DA8143A96DC9C2876BBA27571F2A74AC32CD07F066530EFA5`.
- [x] After reviewing both corrective props-off gates, program and boot the
  clean `flash_blackbox` image. This passed over SWD with exact SHA-256
  `B85DB4F43897EF628EFF0C368CF0670F34FEEDB3FC59C895F21ECFB91D3E6FC4`
  and retained RTT log `logs/terminal_embed/20260722_213858_rtt.log`. It
  recovered flash at page 16,974 with next flight 23, reported 24.9 V, completed
  gyro-bias and RC qualification, qualified all four ESCs `[3,3,3,3]`, armed,
  and wrote 273 pages with zero dropped records or write faults. Explicit
  disarm returned all four DShot values and eRPM readings to zero; DShot and
  telemetry counters remained fault-free.
- [ ] Disconnect SWD/NRST and USB, inspect the airframe and propellers, then
  repeat a conservative controlled-field hop. Abort on any unexpected motor
  response, rapid attitude departure, oscillation, or loss of control
  authority. The successful build/program/boot check does not itself clear the
  hop result.
