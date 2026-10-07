# Communication Protocols

FerroWasp currently has early support for several communication paths. RC input is SBUS or CRSF, selected by configuration; SBUS is the target-tested baseline.

## Current Status

| Protocol/path | Status |
|---|---|
| SBUS | Active prototype RC input, by default on UART2 (Foxeer) or UART6 (Lucid) |
| BLHeli legacy ESC telemetry | Active in the flight DShot image on PA10 / USART1 RX DMA; eRPM and frame integrity target-validated |
| USB CDC serial | Mandatory on Foxeer, which emits bounded read-only `FWDBG1` status lines; was optional on the obsolete FCU3 via `usb_serial` |
| MSPv1 / DJI O4 OSD | Active prototype on UART4 using MSPv1 responses and DisplayPort OSD frames |
| MAVLink | UART mode placeholder/config values exist, no active MAVLink implementation yet |
| CRSF/ELRS | RC input with battery telemetry, selected by `rc_protocol`; not yet run on hardware |

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
3. Validate CRSF/ELRS on the target, link loss included, before preferring it.
4. Keep MSPv1/DJI O4 OSD display-only and freshness-aware.
5. Expand USB serial into useful telemetry.
6. Decide which config path should be first-class: MSP subset, MAVLink subset,
   custom USB, or a small combination.

Runtime configuration should stay tightly validated. A malformed packet or bad parameter value should fail closed, not alter safety authority.

## CRSF

ExpressLRS and TBS Crossfire receivers both speak CRSF to the flight
controller: 420 000 baud, 8N1, not inverted, both ways. Setting
`rc_protocol` to `crsf` starts the port bound to RC input with those line
settings at the next boot. CRSF needs a port that can transmit, so a
receive-only port refuses the binding and RC stays unbound.

The decoder in `ferrowasp-drivers` validates each frame's CRC-8/DVB-S2 and
reads the sixteen packed 11-bit RC channels, which use the same 172-1811 scale
as SBUS, so the RC task treats both protocols alike.

CRSF channel frames carry no failsafe flag. An ExpressLRS receiver stops
sending channels when its link drops, so the 100 ms RC-link timeout is the
primary loss detector. A link-statistics report of zero uplink quality also
marks following channel frames as frame-lost until the uplink recovers.

Battery voltage and current go back to the receiver every 200 ms as CRSF
battery frames, for the radio to show. Consumed capacity and remaining charge
are sent as zero. Telemetry has its own low-priority task and writer, so the
RC task never waits on a transmit; a transmit fault stops telemetry, not RC.

### Channel map

The stick order and arm switch are configurable for either protocol. Every
board's default is AETR stick order with the arm switch on channel 9, where
FerroWasp has always read it. EdgeTX and ExpressLRS radios put the arm switch
on channel 5 out of the box, so a stock radio needs `rc_arm_channel` set to 5
or the radio remixed.

`rc_map` names the one-based channels for roll, pitch, throttle and yaw, in
that order, so AETR is `1234` and TAER is `2314`. Each of channels 1-4 must be
used once. `rc_arm_channel` accepts channels 5-16.

A saved change to the map takes effect when the configuration is saved, while
disarmed. Because the arm switch may now be read from a channel that is
already high, the change invalidates the RC link like a lost receiver: the
link must recover, and the new arm channel must read low, before the craft
can arm.

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
