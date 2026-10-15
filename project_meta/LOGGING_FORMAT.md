# FerroWasp Flight Logging Format

What the recorded flight log must carry, and the move to ULog. Every item is
open.

## Open work

- Where bidirectional DShot runs (ADR-0045), record each motor's eRPM for
  every frame set, its consecutive-invalid-reply count, and its invalid-reply
  total (ADR-0047), plus each extended-telemetry temperature, voltage and
  current value when it arrives, with its motor identity (ADR-0048).
- Record every fresh per-motor legacy-UART eRPM observation at its actual
  bounded rate, including time/age, physical and logical motor identity, request
  sequence, freshness, and transport health; never present repeated stale
  values as new control-rate data.
- Log timestamped body-frame accelerometer data, range/clipping, and bounded
  peaks for offline crash-detector development. Prefer raw or minimally
  filtered samples; if storage cannot sustain the rate, keep bounded per-window
  peaks plus enough surrounding samples to replay an impact. Validate against
  landings, maneuvers, gusts, and impacts before allowing any safety-state
  effect.
- Make every recorded flight self-describing by storing the exact active
  configuration at its flight boundary and after any accepted runtime change:
  gains, filter, RC deadband, rates and expo, motor map, gyro axis/sign map,
  output limits, board and firmware identity, and the persisted configuration
  sequence. A boot-only record is insufficient because several flights and tune
  changes can share one boot. Readers must expose configuration directly and
  warn when it is absent.
- Mark events and phases: boot, gyro-bias calibration start and done, arm
  requested, armed idle, armed active, disarm, failsafe, and logger session
  start, plus a pilot-controlled marker for moments such as "clean hover" or
  "abort".
- Log per-axis P, I, and D contributions and integrator state, and per-motor
  saturation or clamp flags on every update, rather than inferring saturation
  from outputs.
- Log RC link health (frame age, dropped and error frames, failsafe status,
  channel decode health) and IMU health (bias ready, calibration sample count,
  raw bias, stale ticks, calibration rejection reason).
- Carry a session ID so a log aligns with pilot notes, video, and field events.
- Adopt ULog as FerroWasp's standard persisted flight-log format so recorded
  data can use the existing PX4 logging, telemetry, visualization, and analysis
  ecosystem. Define stable FerroWasp message schemas and units, board/firmware
  metadata, parameters, timestamps, dropout reporting, and flight boundaries.
  Keep the hard real-time producer allocation-free and bounded: control tasks
  should publish fixed-size typed records to the existing bounded logging
  boundary, while a lower-priority owner performs ULog framing and storage I/O.
  Provide host tests with known-good ULog readers. FerroConfigurator already
  converts one validated `.fwbb` flight to the compact
  `ferrowasp_rate_control` topic; native firmware ULog framing, parameters,
  richer metadata, and replacing `.fwbb` remain.
