# Communication Protocols

FerroWasp currently has early support for several communication paths. RC input is SBUS or CRSF, selected by configuration; SBUS is the target-tested baseline.

## Current Status

| Protocol/path | Status |
|---|---|
| SBUS | Active prototype RC input, by default on `uart2` (Foxeer) or `uart1` (Lucid) |
| BLHeli legacy ESC telemetry | Active in the flight DShot image, by default on `uart1` (Foxeer) or `uart8` (Lucid); eRPM and frame integrity target-validated on Foxeer |
| USB CDC serial | Standard on both flight boards: `FWDBG1` status lines and the configurator command line |
| MSPv1 / DJI O4 OSD | Active prototype, by default on `uart4` (Foxeer) or `uart3` (Lucid), using MSPv1 responses and DisplayPort OSD frames |
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
   remains observational. Bidirectional DShot is planned to replace it as the
   arming eRPM source; see the [Roadmap](roadmap.md).
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

The decoder in `ferrowasp-crsf` validates each frame's CRC-8/DVB-S2 and
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

## USB

Each flight board presents a USB CDC serial port. Every two seconds or so it
sends a self-describing ASCII `FWDBG1` status line: the selected IMU, sample
and control sequences, raw gyro, stale state, RC qualification, throttle and
arm switch, arm state, pack voltage, and current.

The same port accepts a bounded ASCII command line, which FerroConfigurator
drives: configuration get,
set, and save; serial port bindings; flash information and the scratch-sector
test; log listing, page reads, sync acknowledgement, and erase; and the props-off
motor test. The firmware validates every command. Writes and the motor test
are refused while armed, a motor test is a request the safety master may
refuse, and nothing on the command line can arm the craft. A UART bound to the
`configurator` function accepts the same command line except the motor test,
the unconditional log erase, and `reboot` and `reboot dfu`. A reboot is
accepted only while disarmed with no flash maintenance or flight log open;
`reboot dfu` restarts into the chip's ROM USB DFU bootloader, from which
FerroConfigurator flashes, and holding BOOT at power-on stays the recovery
path.

USB service runs below the control, IMU, RC, and safety priorities, with
fixed-size buffers.

## ESC Telemetry

The BLHeli legacy UART telemetry protocol, how a reply is associated with the
motor that was asked, and how telemetry recovers after a timeout are described
in [DShot](./dshot.md#esc-telemetry).
