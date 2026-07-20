# FerroWasp Flight Test Quick Commands

Last updated: 2026-07-14

Use these from the repo root on the Windows laptop.

## Network Link Choice

Most remote helpers accept `-Link Cable`, `-Link Mobile`, `-Link Zero`, or
`-Link ZeroMobile`.

- Use `-Link Cable` when the laptop is on normal internet Wi-Fi and the Pi is
  connected by Ethernet.
- Use `-Link Mobile` when the laptop and Pi are both on the configured mobile
  router.
- Use `-Link Zero` for the Raspberry Pi Zero 2 W FerroDebugger gateway. This is
  the current small-gateway path.
- Use `-Link ZeroMobile` when the laptop and Zero 2 W are both on the mobile
  router.

Examples below use `Zero`. Swap to `ZeroMobile` for the mobile-router field
setup, or to `Cable`/`Mobile` if you move back to the Pi 4B gateway.

## Current Flight-Test Build

Normal flight-test firmware:

- feature set: `blackbox_defmt`
- no bench motor feature
- shared motor numbering: Betaflight Quad X, motor 1 rear-right, motor 2
  front-right, motor 3 rear-left, motor 4 front-left
- FCU3 motor output remap: `MOTOR_OUTPUT_MAP = [3, 4, 2, 1]`, preserving the
  previously flight-tested physical corner behavior
- gains: roll `P=0.2`, pitch `P=0.25`, yaw `P=0.30`; yaw `I=0.04`,
  other `I=0.0`, all `D=0.0`
- RC rate deadband: `8` raw channel counts around center
- startup gyro bias calibration: keep the FCU still for the first couple
  seconds after boot
- reason: yaw I `0.04` was the best July 14 diagnostic result. It reduced the
  clean centered-yaw hover rate from about `+55.6 dps` on yaw I `0.02` to about
  `+17.4 dps`. Yaw I `0.06` was worse in the clean hover slice, so `0.04` is
  the current baseline.

Current flight status:

- first stable LOS flight achieved
- first conservative FPV characterization flight achieved
- yaw is manageable on yaw I `0.04`, but a persistent yaw/torque asymmetry
  remains visible in logs
- after any motor-map, board-profile, output-backend, or wiring change, re-test
  motor order, direction, and stick/tilt response with propellers removed before
  flight on every actuator-capable board
- keep this as characterization, not aggressive FPV flight

## O4 Fan / Fast Flight Workflow

The DJI O4 unit may need fan cooling during setup. Start the Pi logger while the
fan is cooling the aircraft, but after any flash/reset/boot keep the FCU still
for the first couple seconds so startup gyro-bias calibration sees a stationary
frame. Avoid letting the fan physically shake the FCU during that calibration
window.

After calibration and log start, move the aircraft to the ground and take off
promptly. If you must reset or reflash again, repeat the stationary calibration
pause before moving the aircraft.

## Preflight Checks

```powershell
.\tools\remote_info.ps1 -Link Cable
```

Zero 2 W gateway:

```powershell
.\tools\remote_info.ps1 -Link Zero
```

```powershell
cd apps/stm32f405-flight
cargo check --features blackbox_defmt
```

## Flash FCU

```powershell
.\tools\remote_run.ps1 -Link Zero -Build -Features blackbox_defmt
```

If `remote_run.ps1` is left attached, stop it with `Ctrl+C` after confirming the
firmware is running.

## Sync ELF For Pi-Side Logging

The Pi-side logger needs the matching ELF for `defmt` decoding:

```powershell
.\tools\pi_elf_sync.ps1 -Link Zero
```

## Start Detached Pi Logger

Start this before the hop. It keeps logging if SSH disconnects:

```powershell
.\tools\pi_log_start.ps1 -Link Zero -Restart
```

## Stop And Fetch Log

After the test:

```powershell
.\tools\pi_log_stop.ps1 -Link Zero
```

```powershell
.\tools\pi_log_fetch.ps1 -Link Zero
```

## Analyze Latest Log

For a short hop or restrained flight-control check:

```powershell
python tools\blackbox_analyzer.py --mode auto --trim-start 1 --trim-end 1 --csv logs\remote_probe\flight_test.csv
```

For a stationary props-off check:

```powershell
python tools\blackbox_analyzer.py --mode rest --trim-start 3 --trim-end 3 --csv logs\remote_probe\bench_check.csv
```

## Useful Attach Commands

Attach live without reflashing:

```powershell
.\tools\remote_attach.ps1 -Link Zero
```

Attach live and log locally on the PC:

```powershell
.\tools\remote_attach_log.ps1 -Link Zero
```

Start live viewer:

```powershell
python tools\imu_live_view.py --remote --link zero --ui-hz 15
```

Mobile-router Zero live viewer:

```powershell
python tools\imu_live_view.py --remote --link zeromobile --ui-hz 15
```

## Stop Conditions

Disarm immediately on:

- wrong motor/stick response
- sudden roll/pitch/yaw runaway
- hard oscillation or bounce after liftoff
- smoke, hot smell, rough motor sound, or visible motor/ESC issue
- current spike or current climbing at steady throttle
- loss of video, RC, or debug confidence
