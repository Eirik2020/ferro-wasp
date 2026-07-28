# Communication Protocols

FerroWasp currently has early support for several communication paths. Only SBUS is part of the active RC input path today.

## Current Status

| Protocol/path | Status |
|---|---|
| SBUS | Active prototype RC input over USART2 RX DMA |
| BLHeli legacy ESC telemetry | Active only in the default FCU3 DShot image on PA10 / USART1 RX DMA; eRPM and frame integrity target-validated |
| USB CDC serial | Mandatory on Foxeer; optional on FCU3 via `usb_serial`; Foxeer emits bounded read-only `FWDBG1` status lines |
| MSPv1 / DJI O4 OSD | Active prototype on UART4 using MSPv1 responses and DisplayPort OSD frames |
| MAVLink | UART mode placeholder/config values exist, no active MAVLink implementation yet |
| CRSF/ELRS | Intended preferred RC path, not implemented yet |

## Authority Rule

Communication protocols are input and telemetry paths. They must not directly own safety state or motor hardware.

Allowed:

- parse RC input
- report telemetry
- request validated parameter changes
- expose debug/status information

Not allowed:

- directly arm the system
- directly write actuator permission
- directly write motor output
- bypass failsafe or watchdog behavior

## Near-Term Protocol Direction

The practical order is:

1. Keep SBUS working as the current target-tested RC baseline.
2. Keep the BLHeli legacy telemetry manager non-authoritative and bounded;
   missing pre-arm evidence may block qualification, while post-arm telemetry
   remains observational. Consider bidirectional DShot telemetry separately.
3. Add CRSF/ELRS as the preferred RC input.
4. Keep MSPv1/DJI O4 OSD display-only and freshness-aware.
5. Expand USB serial into useful telemetry.
6. Decide which config path should be first-class: MSP subset, MAVLink subset,
   custom USB, or a small combination.

Runtime configuration should stay tightly validated. A malformed packet or bad parameter value should fail closed, not alter safety authority.

## Foxeer USB Debug

The Foxeer app's optional CDC ACM endpoint reports a self-describing ASCII
`FWDBG1` line on the existing roughly two-second heartbeat. It includes the
selected IMU, sample and control sequences, raw gyro, stale state, RC
qualification, throttle and arm switch, system arm state, pack voltage, and
current.

The implementation uses fixed-size buffers and coalesces status requests.
OTG_FS service runs below control, IMU, RC, and safety priorities. Host input
is drained and discarded; there is intentionally no USB command parser,
parameter writer, arming request, or actuator resource.

## FCU3 ESC Telemetry

The combined legacy ESC telemetry wire is received on PA10 / USART1 RX at
115,200 baud in the standard FCU3 flight image. The ESC manager issues
bounded telemetry-bit requests through the actuator-owned DShot service. Since the
ten-byte wire frame does not identify a motor, a low-priority ESC manager
rotates physical-output requests. Both directions use bounded SPSC queues.

The safety-owned DShot service acknowledges an exact sequence/output request
only after the selected telemetry bit was present in a frame that actually
started. If a CRC-valid response completes while that request is still queued,
the manager may buffer it, but it remains quarantined and cannot be published
until the matching acknowledgement arrives. Association timeouts latch the
manager off until reboot so a late response cannot be attributed to a later
output.

The physical-output association maps to the logical Quad X layout as follows:
output 1 is M4/front-left, output 2 is M3/rear-left, output 3 is M1/rear-right,
and output 4 is M2/front-right.

The manager owns parsing, samples, cadence, association, and timeouts. It does
not own a motor peripheral, arming state, actuator permit, or failsafe state.
UART, parser, queue, and timeout faults cannot grant authority. When they
prevent fresh observations during guarded idle, pre-arm qualification fails
closed and selects stop; after `SYSTEM ARMED`, telemetry loss is currently
observational and does not itself disarm.

The manager waits five seconds after boot before its first request. An arm
attempt that enters guarded idle before telemetry is available can fail closed
at the 1.2-second qualification deadline. A switch-low observation and a new
low-to-high arm request are then required. See [DShot](./dshot.md) for the frame
and target-evidence details.
