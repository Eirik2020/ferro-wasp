# RTT Debug Tools

The current bench workflow includes Python tools for flashing the STM32F405
target, decoding `defmt-rtt` output through `probe-rs`, logging terminal output,
and live-viewing IMU data.

These tools are for bring-up and bench analysis. They are not safety authority,
and they do not command motors.

## Terminal Embed

Run from the repository root:

```powershell
python tools\terminal_embed.py
```

FCU3 is the default. Select the isolated Foxeer app after fitting its SWD
connection:

```powershell
python tools\terminal_embed.py --board foxeer-f405-v2 --release --locked
```

The Foxeer board support leaves PA13/SWDIO and PA14/SWCLK untouched. The first SWD run
must be unpowered at the ESC side; USB DFU remains the recovery path.

Release and feature-selected images use the same logger:

```powershell
python tools\terminal_embed.py --release --locked --features "bench_equal_motors bench_dshot_unequal_motors"
```

The script performs a quiet firmware build, runs:

```text
probe-rs run --chip STM32F405RG --protocol swd --no-location --no-timestamps
```

and prints decoded `defmt` RTT lines in the terminal.

Successful build warnings are hidden to keep the terminal readable. If the build
fails, compiler output is shown.

Every line printed by the tool is also written to:

```text
logs\terminal_embed\YYYYMMDD_HHMMSS_rtt.log
```

Each line is flushed immediately, so an interrupted run retains all output
received up to that point. Prefer this wrapper over piping cargo-embed through
`Tee-Object`; cargo-embed's interactive terminal stream may include cursor and
ANSI control sequences rather than a clean line-oriented log. Default
build-and-run mode records the firmware ELF path and SHA-256 before flashing,
so a retained target log identifies its exact artifact.

Use `--log-file` to choose a specific output path:

```powershell
python tools\terminal_embed.py --log-file logs\bench_rtt.log
```

## Remote Pi Probe

The bench setup can also use a Raspberry Pi as a network-connected SWD/RTT
gateway. In the current workshop setup:

| Role | Value |
|---|---|
| Pi host | configured in FerroDebugger or local config |
| probe-rs server | configured in FerroDebugger or local config |
| probe selector | `0:0:/dev/spidev0.0` |
| target chip | `STM32F405RG` |

Most helper scripts also accept a named link:

| Link | Use When | Host Choice |
|---|---|---|
| `Cable` | Laptop stays on normal internet Wi-Fi and is cabled to the Pi | link details live in FerroDebugger/local config |
| `Mobile` | Laptop and Pi are both on the configured mobile router | SSH alias from FerroDebugger config |
| `Zero` | Raspberry Pi Zero 2 W gateway on bench Wi-Fi | link details live in FerroDebugger/local config |
| `ZeroMobile` | Zero 2 W gateway on the mobile router | link details live in FerroDebugger/local config |

Examples:

```powershell
.\tools\remote_info.ps1 -Link Cable
.\tools\remote_info.ps1 -Link Mobile
.\tools\remote_info.ps1 -Link Zero
.\tools\remote_info.ps1 -Link ZeroMobile
.\tools\pi_log_fetch.ps1 -Link Cable
.\tools\pi_log_fetch.ps1 -Link Mobile
python tools\imu_live_view.py --remote --link cable
python tools\imu_live_view.py --remote --link mobile
python tools\imu_live_view.py --remote --link zero
python tools\imu_live_view.py --remote --link zeromobile
```

Start `probe-rs serve` on the Pi before using the Windows-side helpers. The
server must have at least one configured user/token; otherwise remote clients
connect and then fail authentication.

From the repository root on Windows:

```powershell
.\tools\remote_info.ps1 -Link Cable
.\tools\remote_run.ps1 -Link Cable -Build
.\tools\remote_attach.ps1 -Link Cable
.\tools\remote_attach_log.ps1 -Link Cable
.\tools\pi_log_start.ps1 -Link Cable
.\tools\pi_log_stop.ps1 -Link Cable
.\tools\pi_log_fetch.ps1 -Link Cable
```

Command meaning:

| Command | Purpose |
|---|---|
| `remote_info.ps1` | Checks the remote SWD connection through the Pi. It does not flash or reset the FCU. |
| `remote_run.ps1` | Optionally builds, flashes the ELF, resets/runs the STM32, and streams RTT/defmt output. Pass `-Features blackbox_defmt` to build the 400 Hz compact blackbox frames. |
| `remote_attach.ps1` | Attaches to already-running firmware and streams RTT/defmt output without reflashing. |
| `remote_attach_log.ps1` | Attach-only RTT/defmt session that also writes full terminal output to a log file. |
| `pi_log_start.ps1` | Starts a detached attach logger on the Pi so logging survives SSH disconnects. |
| `pi_log_stop.ps1` | Stops the detached Pi-side logger. |
| `pi_log_fetch.ps1` | Copies the latest Pi-side log into this repository under `logs\remote_probe`. |
| `pi_elf_sync.ps1` | Copies the current local ELF to the Pi path used by `pi_log_start.ps1` for `defmt` decoding. |

The helpers default to the current bench host, token, probe path, and chip. Use
environment variables to avoid putting a shared token in command history:

```powershell
$env:FERROWASP_PROBE_HOST = "ws://your-probe-host.local:3000"
$env:FERROWASP_PROBE_TOKEN = "your-token"
```

For onboard use, install `probe-rs serve` as a boot-starting `systemd` service
on the Pi:

```powershell
.\tools\install_probe_rs_service.ps1 -Token "your-token"
```

The service starts after network-online and restarts if it exits. Useful Pi-side
checks:

```powershell
ssh your-pi-host 'systemctl status probe-rs-serve --no-pager'
ssh your-pi-host 'ss -ltnp | grep probe-rs'
ssh your-pi-host 'sudo systemctl stop probe-rs-serve'
```

`remote_attach.ps1` needs a local ELF matching the firmware running on the FCU
so `defmt` decoding uses the correct metadata.

By default, `remote_attach_log.ps1` writes logs under:

```text
logs\remote_probe\YYYYMMDD_HHMMSS_attach.log
```

Choose a specific log path with:

```powershell
.\tools\remote_attach_log.ps1 -Link Cable -LogFile logs\bench_attach.log
```

For onboard or disconnected logging, start the logger on the Pi, disconnect as
needed, then reconnect later to stop and fetch the log:

```powershell
.\tools\pi_log_start.ps1 -Link Cable
# disconnect/reconnect later
.\tools\pi_log_stop.ps1 -Link Cable
.\tools\pi_log_fetch.ps1 -Link Cable
```

The Pi-side logger writes under:

```text
/home/pi/logs/ferrowasp-rtt
```

Fetched logs are copied into:

```text
logs\remote_probe
```

## 400 Hz Blackbox Frames

The current firmware uses `defmt-rtt` 1.2. That backend owns the SEGGER RTT
control block and one RTT up-channel, so the MVP logger does not link
`rtt-target` beside it. Instead, blackbox data is emitted as compact fixed-point
`defmt` frames over the existing RTT stream.

Build and flash the blackbox-enabled firmware:

```powershell
.\tools\remote_run.ps1 -Link Cable -Build -Features blackbox_defmt
```

For the Foxeer F405 V2 over its retrofitted local SWD connection:

```powershell
python tools\terminal_embed.py --board foxeer-f405-v2 --release --locked --features blackbox_defmt
```

Begin with ESC power disconnected and hold the board stationary for at least
ten seconds. The offline analyzer's `Sequence/timing` section should estimate
about `1000 Hz` for the EXTI-driven Foxeer IMU, with contiguous IMU sequence
deltas normally alternating between `2` and `3` at the unchanged 400 Hz control
rate. Repeated IMU samples should be zero. Missing BB2 frames indicate RTT
transport loss and are reported independently from IMU progress.

For a clean motor-vibration check that disables the PID/mixer and commands all
four motors equally through the normal actuator-output task, build with:

```powershell
.\tools\remote_run.ps1 -Link Cable -Build -Features "blackbox_defmt bench_equal_motors"
```

The craft must still be armed normally. This mode caps requested throttle to
`250` PWM-style units, emits `BB2` frames with PID set to zero, and logs motors
as `[throttle; 4]`. Use propellers removed only. If any motor or ESC smokes,
smells hot, jitters abnormally, or heats rapidly, stop the test and inspect the
hardware before running again.

The 400 Hz control loop emits `BB2` frames. Disarmed frames still include raw
gyro, filtered gyro, command, throttle, and freshness flags; PID and motor
fields are zeroed until the craft is armed.

```text
BB2 seq 1234 imu 5678 flags 3 raw10 [12, -21, 6] gyro10 [10, -20, 5] cmd10 [0, 0, 0] pid [1, -2, 0] thr 1175 motors [1175, 1176, 1174, 1175]
```

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

Future work: replace the RTT backend deliberately so channel 0 can carry
`defmt` and channel 1 can carry raw binary blackbox samples.

The live viewer auto-detects `BB1` and `BB2` frames:

```powershell
python tools\imu_live_view.py --remote --link cable
```

When blackbox frames appear, it shows control-loop plots for gyro-vs-command
rates, PID output, throttle, and motors.
The viewer suppresses raw RTT echo by default to avoid lag with 400 Hz frames.
Use `--echo-rtt` only when raw terminal output is more important than smooth
plotting. If the PC still struggles, lower the UI refresh rate:

```powershell
python tools\imu_live_view.py --remote --link cable --ui-hz 15
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

After a local `terminal_embed.py` capture, the analyzer selects the newest RTT
log automatically:

```powershell
python tools\blackbox_analyzer.py --mode rest --csv logs\terminal_embed\latest_blackbox.csv
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

## Heartbeat

The default firmware heartbeat logs once at startup and then alternates the
red/green debug LEDs about once per second as a visible liveness indicator.
High-rate IMU and control-loop observation is handled by BB2 frames when the
`blackbox_defmt` feature is enabled.

## IMU Live View

Run:

```powershell
python tools\imu_live_view.py
```

The viewer launches `terminal_embed.py` or attaches through the remote probe
helpers. With `blackbox_defmt` firmware it parses BB2 frames and shows:

- measured roll/pitch/yaw gyro-rate history,
- commanded roll/pitch/yaw rates,
- PID output, throttle, and motor commands.

Older heartbeat-IMU firmware can still use the attitude-view options. The
default attitude mode seeds roll and pitch once from accelerometer gravity, then
integrates gyro rates only:

```powershell
python tools\imu_live_view.py --attitude gyro
```

For a bench view that continuously nudges roll and pitch back toward gravity:

```powershell
python tools\imu_live_view.py --attitude accel
```

To attach through the remote Pi probe-rs server and live-view already-running
firmware:

```powershell
python tools\imu_live_view.py --remote --link cable
```

The remote viewer defaults to the cable link, `0:0:/dev/spidev0.0`, and
`STM32F405RG`. It honors the same `FERROWASP_PROBE_HOST` and
`FERROWASP_PROBE_TOKEN` environment variables as the PowerShell helpers when
`--link auto` is used. Pass `--link mobile` on the mobile-router network.

Yaw remains gyro-only in both modes and will drift.

## IMU Noise Analyzer

The noise analyzer gives a quick bench answer to:

```text
Is the filtered gyro signal quiet enough for the PID to use?
```

Analyze the newest RTT log:

```powershell
python tools\imu_noise_analyzer.py --latest
```

Or capture a still FCU/IMU run directly:

```powershell
python tools\imu_noise_analyzer.py --capture 30
```

The analyzer:

- parses BB2 or older heartbeat IMU lines,
- remaps raw gyro sensor axes into the current PID roll/pitch/yaw convention,
- compares raw gyro noise against filtered control-axis rates,
- reports standard deviation, peak-to-peak noise, attenuation, drift, and a
  rough quiet/watch/noisy verdict.

Use this as a rest-noise screen only. It does not replace props-off motor output
checks, motor-temperature checks, or later vibration/frequency analysis.

## Safety Notes

- Run motor/axis checks with props removed.
- `remote_run.ps1` flashes, resets, and starts firmware. Keep ESC power in the
  intended bench-safe state before using it.
- Do not leave `probe-rs serve` exposed on an untrusted network; the remote
  server provides debug-level control of the FCU.
- Treat BB2 `raw10` as unfiltered control-axis gyro data and `gyro10` as
  PID-convention measured data.
- The Python tools are observation tools only; motor authority must remain in
  the firmware safety/actuator-output path.
