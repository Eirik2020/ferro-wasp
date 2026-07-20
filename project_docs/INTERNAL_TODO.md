# Internal Development Backlog

This is maintainer/session backlog material. Public roadmap-level items should
be reflected in `mdbook/src/roadmap.md` or `mdbook/src/mvp_next.md`.

## MVP Focus
- Narrow scope to a simple MVP that flies.
- Prioritize the minimum safe flight path: RC input, IMU sampling, arming/disarming, rate loop, mixer, safety-gated motor output, and basic bench/flight telemetry.
- Defer non-essential expansion until the MVP flies predictably: broad protocol support, configurator polish, board generator work, and advanced companion modules.

## Current Work Order - 2026-07-20

1. Prepare a sanitized, accurate public source repository and pass every
   supported target/CI check.
2. Bring up FerroWasp on Foxeer F405 V2 with arming inhibited until its physical
   verification gates pass.
3. Add a reproducible WSL/Docker development environment.
4. Resume FCU3 tuning with the isolated pitch P `0.25 -> 0.30` experiment.

## Recently Closed

- The safety-owned DShot arming sequence now stops all motors on failed idle
  qualification; the injected physical-output-1 / logical-M4-not-running case
  aborted after 1.2 seconds and never armed.
- Boot, battery reconnect, and FCU reflash while the controller arm level
  remained high did not automatically rearm. A fresh low-to-high transition is
  required.
- The standard DShot600/legacy-telemetry image completed a controlled outdoor
  flight. The previous unwanted yawing was absent by operator report; pitch
  authority is the remaining tuning observation.

## Bug List

- Independent actuator-deadline detection for a total loss of future motor
  commands is not yet implemented.
- IMU initialization, gyro-bias calibration, and freshness are not yet pre-arm
  prerequisites. The first post-arm stale-IMU check requests disarm, but the
  intervening armed transition remains an open gap. ADC/OSD freshness policy
  also remains prototype-level.
- DShot waveform timing, jitter, M4 polarity margin, and TIM1/TIM8 phase still
  require logic-analyzer evidence.

## Telemetry / Debug TODO
- Flight-test telemetry / blackbox improvements from July 14 FPV bring-up:
  - add explicit event/phase markers to BB2 or its successor: boot, gyro-bias
    calibration start/done, arm requested, armed idle, armed active,
    throttle-on, disarm, failsafe, and logger/session start
  - log separate PID contributions per axis, especially yaw P/I/D terms and
    yaw integrator state, so tuning can distinguish rate response from steady
    torque bias and windup
  - log actuator saturation/clamp flags per motor and per update, instead of
    inferring saturation from motor output values after the fact
  - emit the active tune/config at boot and after any runtime change: roll,
    pitch, yaw P/I/D, filter alpha, RC deadband, motor map, gyro axis/sign map,
    and output limits
  - add RC link quality and freshness fields: frame age, dropped/error frames,
    failsafe status, and channel decode health
  - add IMU freshness and bias-calibration fields: bias ready, calibration
    sample count, raw bias values, stale ticks, and calibration rejection reason
  - calibrate and log battery current/voltage well enough to compare motor/ESC
    load during flight tests; current is currently a manual evidence signal, not
    a trusted safety gate
  - add a blackbox session id plus monotonic timestamp in microseconds so logs
    can be aligned with pilot notes, video, and field events
  - add a pilot-controlled marker or simple firmware-side test marker for
    "clean hover", "FPV characterization", and "abort/land" moments
  - improve log transport reliability: direct USB CDC, onboard flash, or
    Bluetooth/NRF52 blackbox transport so field testing does not depend on
    Pi/mDNS/Wi-Fi behavior near DJI equipment
- Document and prototype the Pi Zero 2 W remote debug gateway:
  - laptop Wi-Fi -> Pi Zero 2 W -> debug probe -> SWD -> FCU
  - Pi runs local RTT logging during flight and keeps logging if Wi-Fi drops
  - MVP assumption: the remote debug server may be able to flash/reset/halt the
    FCU during flight; acceptable for early bring-up, but not acceptable for a
    later safety-reviewed flight configuration
  - expected networks for MVP are home Wi-Fi and a trusted mobile router only
  - add a boot-starting `probe-rs serve` service for the Pi, because onboard
    power will be removed frequently
  - document how to stop/disable the service before manual probe ownership or
    later flight configurations
  - use a strong token before leaving the service bound to the network
  - later add a maintenance/debug-enable mode, e.g. config flag, physical
    button, GPIO strap, or explicit service enable, so remote debug is not left
    active accidentally
  - account for single ownership of `/dev/spidev0.0`; only one `probe-rs`
    process can own the Pi GPIO/SPI SWD backend at a time
  - harden Pi storage for frequent battery removal: no disk-backed swap,
    bounded/volatile logs, and eventually read-only root or overlay where
    practical
  - maintenance/flashing mode must stop the flight logger before owning the probe
  - flashing/reset must be rejected if the FCU armed-state heartbeat is armed, stale, or unknown
- Future RTT firmware migration:
  - replace single-channel `defmt-rtt` with `rtt-target`
  - RTT up-channel 0: `defmt` logs
  - RTT up-channel 1: compact binary telemetry
  - both channels must use non-blocking/no-skip-or-drop behavior; never block the control loop on RTT
  - telemetry task should copy latest state at lower priority and increment drop counters if the RTT buffer is full
