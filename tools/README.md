# FerroWasp RTT Terminal MVP

This is the smallest host path for seeing drone firmware text in a Python terminal.

The firmware already uses `defmt-rtt`, so RTT bytes are not plain UTF-8. The Python
script runs a quiet build from `apps/stm32f405-flight`, then launches
`probe-rs run` so probe-rs decodes
the defmt RTT stream and keeps printing live firmware lines. Build warnings are
hidden on successful builds, but shown if the build fails.

## Run

Connect the probe and target, then run from the repository root:

```powershell
python tools\terminal_embed.py
```

You can also run `python terminal_embed.py` from inside the `tools` directory.

Use `--release`, `--locked`, and `--features` to build and run a specific
bench image. For example:

```powershell
python tools\terminal_embed.py --release --locked --features "dshot bench_equal_motors bench_dshot_unequal_motors"
```

Expected first useful lines include boot, IMU detection, battery, and heartbeat
startup messages:

```text
DRONE: [INFO ] FerroWasp RTT hello from drone
DRONE: [INFO ] Running heartbeat!
```

The default firmware heartbeat is intentionally quiet in RTT after startup, but
it alternates the red/green debug LEDs about once per second as a visible
liveness indicator. Use the `blackbox_defmt` feature when you want continuous
IMU/control-loop samples.

Every terminal line is also written to a timestamped log file:

```text
logs\terminal_embed\YYYYMMDD_HHMMSS_rtt.log
```

The file is flushed after every line, so output already received remains
readable after `Ctrl+C` or an interrupted target run. This line-oriented
capture is preferable to piping cargo-embed through `Tee-Object`, because
cargo-embed's interactive terminal output can contain cursor and ANSI control
sequences. Default build-and-run mode also records the exact firmware ELF path
and SHA-256 before `probe-rs` starts, keeping each retained log tied to its
artifact.

To choose the log path:

```powershell
python tools\terminal_embed.py --log-file logs\bench_rtt.log
```

To pass a custom command:

```powershell
python tools\terminal_embed.py -- probe-rs run --chip STM32F405RG --protocol swd --no-location --no-timestamps apps\stm32f405-flight\target\thumbv7em-none-eabihf\debug\FerroWasp
```

## IMU Live View

To build, flash/run, log, and live-plot the current telemetry stream:

```powershell
python tools\imu_live_view.py
```

The viewer launches `terminal_embed.py`. With `blackbox_defmt` firmware it reads
BB2 frames and shows control-loop plots for gyro-vs-command rates, PID output,
throttle, and motors. By default it does not echo every RTT line because 400 Hz
blackbox output can make the UI lag; pass `--echo-rtt` when raw terminal echo is
more important than plot smoothness.

Older heartbeat-IMU firmware can still use the attitude-view options. To
continuously ground roll/pitch with accel while still integrating gyro motion:

```powershell
python tools\imu_live_view.py --attitude accel
```

The default mode is:

```powershell
python tools\imu_live_view.py --attitude gyro
```

To attach through the remote Pi probe-rs server and live-view the already-running
firmware:

```powershell
python tools\imu_live_view.py --remote --link cable
```

For the Raspberry Pi Zero 2 W gateway:

```powershell
python tools\imu_live_view.py --remote --link zero
```

For the Zero 2 W on the mobile router:

```powershell
python tools\imu_live_view.py --remote --link zeromobile
```

The remote viewer uses `FERROWASP_PROBE_HOST` and `FERROWASP_PROBE_TOKEN` when
they are set. It can also read ignored local defaults from
`local_config/ferrowasp.local.json`; see `local_config/ferrowasp.example.json`.

The old `accel_live_plot.py` name still works as a compatibility wrapper.

## IMU Noise Analyzer

To estimate whether the filtered gyro rates are quiet enough for the PID while
the FCU is sitting still, run:

```powershell
python tools\imu_noise_analyzer.py --latest
```

Or capture directly from the FCU/IMU without running motors:

```powershell
python tools\imu_noise_analyzer.py --capture 30
```

The analyzer compares raw gyro data, remapped into the PID roll/pitch/yaw
convention, against the filtered control-axis rates that the PID sees. It prints
per-axis standard deviation, peak-to-peak noise, filter attenuation, drift, and a
rough quiet/watch/noisy verdict. Keep the frame still during capture.

## Remote Pi Probe

The PowerShell remote-probe helpers in this repo are compatibility wrappers
around the sibling FerroDebugger repo. FerroWasp still owns building the
firmware ELF and the blackbox/RTT schema; FerroDebugger owns Pi links,
`probe-rs serve`, Pi setup, Zero 2 W migration, detached logging, and log fetch.

By default the wrappers look for FerroDebugger at:

```text
..\ferro-debugger
```

Override that with:

```powershell
$env:FERRODEBUGGER_REPO = "C:\path\to\ferro-debugger"
```

When `probe-rs serve` is running on a configured FerroDebugger link, these
helpers use the Raspberry Pi GPIO/SPI SWD probe through the remote probe-rs
WebSocket API:

```powershell
.\tools\remote_info.ps1 -Link Cable
.\tools\remote_run.ps1 -Link Cable -Build
.\tools\remote_attach.ps1 -Link Cable
.\tools\remote_attach_log.ps1 -Link Cable
.\tools\pi_log_start.ps1 -Link Cable
.\tools\pi_log_stop.ps1 -Link Cable
.\tools\pi_log_fetch.ps1 -Link Cable
.\tools\pi_elf_sync.ps1 -Link Cable
```

Zero 2 W equivalents use the `Zero` link on bench Wi-Fi:

```powershell
.\tools\remote_info.ps1 -Link Zero -VerboseProbe
.\tools\remote_run.ps1 -Link Zero -Build -Features blackbox_defmt
.\tools\pi_elf_sync.ps1 -Link Zero
.\tools\pi_log_start.ps1 -Link Zero -Restart
.\tools\pi_log_stop.ps1 -Link Zero
.\tools\pi_log_fetch.ps1 -Link Zero
```

Zero 2 W on the mobile router uses `ZeroMobile`:

```powershell
.\tools\remote_info.ps1 -Link ZeroMobile -VerboseProbe
.\tools\remote_run.ps1 -Link ZeroMobile -Build -Features blackbox_defmt
.\tools\pi_elf_sync.ps1 -Link ZeroMobile
.\tools\pi_log_start.ps1 -Link ZeroMobile -Restart
.\tools\pi_log_stop.ps1 -Link ZeroMobile
.\tools\pi_log_fetch.ps1 -Link ZeroMobile
```

Command summary:

- `remote_info.ps1` checks the remote SWD connection through the Pi. It does
  not flash or reset the FCU.
- `remote_run.ps1` flashes the selected ELF, resets/runs the STM32, and streams
  RTT/defmt output. Use `-Build` to build a fresh release first. Pass
  `-Features blackbox_defmt` to include the 400 Hz compact blackbox frames.
- `remote_attach.ps1` attaches to firmware that is already running and streams
  RTT/defmt output. It does not reflash; the local ELF should match the running
  firmware so defmt decoding is correct.
- `remote_attach_log.ps1` does the same attach-only RTT session, while also
  writing the complete terminal output to `logs\remote_probe\*_attach.log`.
- `pi_log_start.ps1` starts a detached attach logger on the Pi. It keeps
  running if the SSH session disconnects.
- `pi_log_auto.ps1` starts a detached attach logger on the Pi that writes
  armed-window `rtt-auto-*.log` files by watching BB1/BB2 armed flags.
- `pi_log_stop.ps1` stops the detached Pi logger.
- `pi_log_fetch.ps1` copies the latest Pi-side log into `logs\remote_probe`.
- `pi_elf_sync.ps1` copies the current local ELF to the Pi path used by
  `pi_log_start.ps1` for `defmt` decoding.

Link defaults, tokens, Pi users, remote ELF paths, and probe settings now live
in FerroDebugger config:

```text
..\ferro-debugger\config\ferrodebugger.local.json
```

Most helpers accept a named Pi link:

```powershell
.\tools\remote_info.ps1 -Link Cable
.\tools\remote_info.ps1 -Link Mobile
.\tools\remote_info.ps1 -Link Zero
.\tools\remote_info.ps1 -Link ZeroMobile
.\tools\remote_run.ps1 -Link Cable -Build
.\tools\remote_run.ps1 -Link Mobile -Build
.\tools\remote_run.ps1 -Link Zero -Build
.\tools\remote_run.ps1 -Link ZeroMobile -Build
python tools\imu_live_view.py --remote --link cable
python tools\imu_live_view.py --remote --link mobile
python tools\imu_live_view.py --remote --link zero
python tools\imu_live_view.py --remote --link zeromobile
```

Use `Cable` when the laptop is on normal internet Wi-Fi and the Pi is connected
by Ethernet. Use `Mobile` when the laptop and Pi are both on the configured
mobile router.
Use `Zero` for the Raspberry Pi Zero 2 W gateway on bench Wi-Fi. Use
`ZeroMobile` when the Zero and laptop are both on the mobile router.

Legacy parameters such as `-HostUri`, `-Token`, `-PiUser`, `-RemoteElf`,
`-Chip`, `-Probe`, and `-Speed` are accepted by the compatibility wrappers but
ignored. Configure those values in FerroDebugger instead.

To install the Pi boot/restart service:

```powershell
.\tools\install_probe_rs_service.ps1 -Link Zero -Token "your-token"
```

For local convenience without committing private bench defaults, copy
`local_config\ferrowasp.example.json` to `local_config\ferrowasp.local.json` and
set `mobile_ssid` and `probe_token` there. The local file is ignored by Git.

To choose a specific attach log path:

```powershell
.\tools\remote_attach_log.ps1 -Link Cable -LogFile logs\bench_attach.log
```

For onboard or disconnected logging:

```powershell
.\tools\pi_log_start.ps1 -Link Cable
# disconnect/reconnect later
.\tools\pi_log_stop.ps1 -Link Cable
.\tools\pi_log_fetch.ps1 -Link Cable
```

On the mobile router:

```powershell
.\tools\pi_log_start.ps1 -Link Mobile -Restart
.\tools\pi_log_stop.ps1 -Link Mobile
.\tools\pi_log_fetch.ps1 -Link Mobile
```

For armed-window auto logging on the Zero/mobile-router setup:

```powershell
.\tools\pi_log_auto.ps1 -Link ZeroMobile -Restart
# fly one or more armed tests
.\tools\pi_log_auto.ps1 -Link ZeroMobile -Stop
.\tools\pi_log_fetch.ps1 -Link ZeroMobile
```

The auto logger attaches once, keeps a short pre-arm buffer, writes while BB
frames report `armed`, and closes the segment a few seconds after disarm. The
normal `pi_log_fetch.ps1` command fetches the latest closed or active auto log.

Bench safety: `remote_run.ps1` resets and starts the flashed firmware. Keep
props off and ESC power in the intended safe state before running it.

## 400 Hz Blackbox Defmt Frames

The current firmware cannot safely link `rtt-target` beside `defmt-rtt`; the
installed `defmt-rtt` backend owns the SEGGER RTT control block and exposes one
RTT up-channel. For the MVP, high-rate bench logging therefore uses compact
fixed-point `defmt` frames over the existing RTT stream.

Build and flash the blackbox-enabled firmware:

```powershell
.\tools\remote_run.ps1 -Link Cable -Build -Features blackbox_defmt
```

For a clean motor-vibration check that disables the PID/mixer and commands all
four motors equally through the normal actuator-output task, build with the
bench equal-motor feature:

```powershell
.\tools\remote_run.ps1 -Link Cable -Build -Features "blackbox_defmt bench_equal_motors"
```

In this mode the aircraft must still be armed normally. The firmware caps the
requested throttle to `250` PWM-style units, logs PID as zero, and logs motors as
`[throttle; 4]`. Use it only with propellers removed, and stop immediately if a
motor or ESC smokes, smells hot, jitters abnormally, or heats rapidly.

For the normal FCU3 DShot600 firmware, build without `bench_equal_motors`:

```powershell
.\tools\remote_run.ps1 -Link Cable -Build -Features blackbox_defmt
```

The current normal profile is intentionally conservative: roll P/I/D
`0.20 / 0.00 / 0.00`, pitch P/I/D `0.25 / 0.00 / 0.00`, and yaw P/I/D
`0.30 / 0.04 / 0.00`.

The legacy `bench_motorN_only` modes select physical PWM outputs and are
compile-time incompatible with the default DShot image. Do not add them to the
commands above. For DShot motor identity, use the capped logical modes below;
they exercise the committed mixer-to-output map through the normal safety-owned
actuator path.

To verify the active mixer-to-output remap, use the logical motor modes instead.
These command a logical mixer motor and then apply `MOTOR_OUTPUT_MAP` before
touching the physical outputs.

```powershell
.\tools\remote_run.ps1 -Link Cable -Build -Features "blackbox_defmt bench_equal_motors bench_logical_motor1_only"
```

```powershell
.\tools\remote_run.ps1 -Link Cable -Build -Features "blackbox_defmt bench_equal_motors bench_logical_motor2_only"
```

```powershell
.\tools\remote_run.ps1 -Link Cable -Build -Features "blackbox_defmt bench_equal_motors bench_logical_motor3_only"
```

```powershell
.\tools\remote_run.ps1 -Link Cable -Build -Features "blackbox_defmt bench_equal_motors bench_logical_motor4_only"
```

With the current remap, logical M1/M2/M3/M4 spin rear-right, front-right,
rear-left, and front-left respectively. Verified rotation is M1 CW, M2 CCW,
M3 CCW, and M4 CW. These modes require propellers removed and remain capped.

The control loop emits one `BB2` frame at the 400 Hz PID rate. Disarmed frames
still include raw gyro, filtered gyro, command, throttle, and freshness flags;
PID and motor fields are zeroed until the craft is armed.

```text
BB2 seq 1234 imu 5678 flags 3 raw10 [12, -21, 6] gyro10 [10, -20, 5] cmd10 [0, 0, 0] pid [1, -2, 0] thr 1175 motors [1175, 1176, 1174, 1175]
```

Field meaning:

| Field | Meaning |
|---|---|
| `seq` | Control-rate sample sequence |
| `imu` | IMU sample sequence used by this control update |
| `flags` | Bit 0 armed, bit 1 IMU fresh |
| `raw10` | Unfiltered roll, pitch, yaw gyro rates in 0.1 deg/s |
| `gyro10` | Filtered roll, pitch, yaw gyro rates in 0.1 deg/s |
| `cmd10` | Commanded roll, pitch, yaw rates in 0.1 deg/s |
| `pid` | Total PID output per axis in controller output units |
| `thr` | Throttle setpoint in PWM-style command units |
| `motors` | Mixed motor commands in PWM-style command units |

This is still human-decodable `defmt` output, not a dedicated binary RTT
channel. The future path is to replace the RTT backend deliberately so channel 0
can carry `defmt` and channel 1 can carry raw binary blackbox samples.

The live viewer auto-detects these frames. With blackbox firmware running:

```powershell
python tools\imu_live_view.py --remote --link cable
```

On the Zero 2 W gateway:

```powershell
python tools\imu_live_view.py --remote --link zero
```

On the Zero 2 W mobile-router link:

```powershell
python tools\imu_live_view.py --remote --link zeromobile
```

When `BB1` or `BB2` lines appear, the viewer shows control-loop plots for
gyro-vs-command rates, PID output, throttle, and motors.
If the PC still struggles, lower the UI refresh rate:

```powershell
python tools\imu_live_view.py --remote --link cable --ui-hz 15
```

```powershell
python tools\imu_live_view.py --remote --link zero --ui-hz 15
```

```powershell
python tools\imu_live_view.py --remote --link zeromobile --ui-hz 15
```

For offline analysis after a Pi-side recording:

If you rebuilt/flashed from the PC and plan to log on the Pi, sync the matching
ELF first so `defmt` decoding uses the right metadata:

```powershell
.\tools\pi_elf_sync.ps1 -Link Cable
```

```powershell
.\tools\pi_log_start.ps1 -Link Cable -Restart
# run the bench test
.\tools\pi_log_stop.ps1 -Link Cable
.\tools\pi_log_fetch.ps1 -Link Cable
python tools\blackbox_analyzer.py --csv logs\remote_probe\latest_blackbox.csv
```

For deliberate hand-motion/swing tests, the default `--mode auto` should classify
large low-frequency gyro movement as motion. Use `--mode rest` for stationary
noise-floor or motor-vibration checks where large filtered gyro standard
deviation should be treated as noise:

```powershell
python tools\blackbox_analyzer.py --mode rest
```

For a hand-swing recording, trim the pickup and place-down sections by time:

```powershell
python tools\blackbox_analyzer.py --mode swing --trim-start 6 --trim-end 6 --csv logs\remote_probe\swing_trimmed.csv
```

With no log argument, the analyzer uses the newest `.log` in `logs\remote_probe`
or `logs\terminal_embed`. You can also pass a specific fetched log:

```powershell
python tools\blackbox_analyzer.py logs\remote_probe\pi_20260710_193000_attach.log --csv logs\remote_probe\swing_test.csv
```

## Flight Reports

To create a report folder with a markdown summary and plots:

```powershell
python tools\flight_report.py logs\remote_probe\rtt-YYYYMMDD-HHMMSS.log --trim-start 3 --trim-end 3
```

With no source argument, the newest `.log` or `.csv` in `logs\remote_probe` is
used:

```powershell
python tools\flight_report.py --trim-start 3 --trim-end 3
```

Reports are written under:

```text
logs\remote_probe\reports\<source-name>\report.md
```

The generated plots include setpoint-vs-measured rate, rate tracking error, PID
output, motor output, and zooms around the worst tracking-error events.

## Troubleshooting

- Check that the probe is visible to `probe-rs`.
- Check that `Embed.toml` uses `STM32F405RG`.
- Check that the firmware is built with `defmt-rtt`.
- If you only see build output, wait for flashing/reset and RTT attach to finish.
- If another debugger owns the probe, close it before running this script.
