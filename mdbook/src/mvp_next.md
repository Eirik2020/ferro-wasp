# MVP Status and Next Work

This page records the near-term implementation order for the rapid prototype.
It replaces the earlier PWM-era MVP plan; completed work remains summarized in
the changelog and detailed DShot evidence page.

The MVP is not a complete flight stack. It demonstrates that RC input, IMU
sampling, control, motor output, telemetry, and visible failure behavior can
operate together without adding another motor-authority path.

## Demonstrated FCU3 Chain

```text
SBUS RC input
  -> safety-qualified arm request
  -> MPU6500 sample path
  -> 400 Hz rate controller and Quad X mixer
  -> bounded fresh MotorCmd queue
  -> safety-owned 500 Hz DShot600 output
  -> PA10 legacy ESC telemetry and idle-eRPM qualification (default DShot only)
  -> DJI O4 OSD / defmt / BB2 visibility
```

The previous PWM backend remains an explicit fallback. DShot600 is the default
FCU3 output.

## Current Evidence Boundary

Completed target checkpoints include:

- qualified SBUS startup, RC-loss invalidation, and arm-high recovery inhibit;
- guarded DShot arming requiring three fresh 3,000-10,000 eRPM samples from
  every motor;
- an injected physical-output-1 / logical-M4 zero-eRPM failure that remained
  disarmed and returned all lanes to stop;
- logical motor order and direction: M1 rear-right CW, M2 front-right CCW, M3
  rear-left CCW, and M4 front-left CW;
- props-off roll, pitch, and yaw stick mixing plus motion-opposing correction;
- synchronized DShot runtime, throttle response, explicit disarm, restart
  interlock, and BLHeli legacy telemetry.

On 2026-07-20 the operator reported a successful controlled experimental FCU3
flight with strong manoeuvres and no recurrence of the previous unwanted
yawing. Pitch authority felt low. No new BB2 flight capture accompanies that
report, and electrical waveform/timing and measured stop latency remain open.
The result does not imply routine flight readiness.

## Priority 1: Publish a Clean Experimental Source Repository

- create a sanitized public history after rotating historical credentials;
- omit old large capture branches and private bench material;
- make formatting, host tests, strict Clippy, all application checks, and the
  mdBook build pass from a clean checkout;
- align the public default branch with CI and Pages triggers;
- document setup, support status, limitations, security reporting, licences,
  and third-party attribution;
- pin moving safety-relevant dependencies and the Rust toolchain;
- scan the final public history for secrets and oversized objects.

Publication is a source-sharing milestone, not a firmware release or safety
approval.

## Priority 2: Run FerroWasp on Foxeer F405 V2

The Foxeer image uses default DShot and has evidence for ROM-DFU/SWD, USB,
ICM42688-P/EXTI, physical orientation, RC interlocks, motor order/direction,
eRPM-qualified arming, and onboard blackbox recording. Its first prop-on
departure exposed positive pitch feedback and attempted a forward flip. The
pre-fix image is withdrawn. The correction has since passed unpowered
frame-sign and powered normal-mixer props-off opposition checks, corrected
hops, and a confined-area flight. The operator classifies the current P-only
setup as flyable but not well tuned.

Next checks:

1. Repeat the flight in calmer conditions and continue one-variable-at-a-time
   tuning from the `2.5 / 2.5 / 2.0` P-only baseline.
2. Add self-describing flight configuration, per-motor eRPM, accelerometer,
   saturation, and crash-analysis evidence to the onboard log.
3. Repair and validate bounded I-term behavior before enabling I gains.
4. Fine-calibrate PC0/PC1 later; current display remains disabled meanwhile.

The Foxeer app provides an explicit `bench_actuator_validation` commissioning
gate for repeated motor checks. It only compiles with a capped equal-motor,
physical-motor, or logical-motor bench mode and cannot select normal PID/mixer
flight output. RC qualification,
arming guards, command freshness, failsafe/disarm handling, and actuator
ownership remain active. The arming sequence briefly applies PWM idle to all
four outputs, so the mode is props-off only even when one motor is selected.

## Priority 3: Maintain the WSL/Docker Development Environment

The checked-in Dockerfile, Compose service, Dev Container entry point, and
environment smoke check now pin the Rust, Python, and documentation tools used
across the isolated workspaces. A path-filtered CI workflow guards this setup
without rebuilding the image for unrelated firmware changes. Hardware
flashing and USB/SWD access remain explicit host workflows; a successful
container build is not target evidence.

## Parked Flight-Tuning Follow-Up

The current pitch P value is `0.25`. A future controlled test may raise only
pitch P to `0.30`, leaving roll, yaw, I, D, and filtering unchanged. Review
tracking error and motor headroom first: more P cannot create authority if the
mixer is already saturated. Stop the progression on rapid oscillation,
bounce-back, abnormal noise, or motor heating. Capture BB2 when practical.

This tuning follow-up is intentionally parked behind the remaining publication
and Foxeer validation work.

## Safety and Reliability Follow-Up

- add an actuator deadline that detects complete absence of future motor
  commands, not only stale data observed on a wake;
- make IMU initialization, gyro-bias calibration, and freshness explicit
  pre-arm prerequisites; the current first post-arm stale-IMU check requests
  disarm, but a brief armed transition is still possible;
- complete setpoint and ADC/OSD freshness behavior;
- instrument DShot waveform timing, jitter, cross-timer phase, and stop latency;
- expand duration and fault-injection evidence;
- keep telemetry, OSD, USB, configurators, and experiments outside arming and
  actuator authority.

## Later Features

- CRSF/ELRS with SBUS retained as a fallback;
- bidirectional DShot telemetry as a separate path from current legacy UART
  telemetry;
- read-mostly USB telemetry and a small validated configuration interface;
- persistent parameters with validation and rollback;
- board/resource generation and broader MCU targets.
