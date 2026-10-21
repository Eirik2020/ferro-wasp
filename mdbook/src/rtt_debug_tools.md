# Bench and Analysis Tools

These Python tools flash a board over SWD, decode its `defmt` RTT output
through `probe-rs`, keep a log of it, read the onboard log store, and plot or
analyze what they collect.
They observe; they are not safety authority and never command motors. The
flight log is the onboard blackbox, downloaded with FerroConfigurator; RTT is
for the bench.

## Terminal Embed

From the repository root:

```powershell
python tools\terminal_embed.py --board foxeer-f405-v2 --release --locked
```

Foxeer F405 V2 is the only board it knows, and the default. It builds the image
quietly, showing compiler output only if the build fails, then runs
`probe-rs run` against the STM32F405RG over SWD and prints the decoded lines.
Feature-selected images use the same command:

```powershell
python tools\terminal_embed.py --release --locked --features "bench_actuator_validation bench_equal_motors"
```

Foxeer's board support leaves PA13/SWDIO and PA14/SWCLK untouched. Make the
first SWD connection with the ESCs unpowered; USB DFU remains the recovery path.

Every printed line is also written, and flushed at once, to
`logs\terminal_embed\YYYYMMDD_HHMMSS_rtt.log`, so an interrupted run keeps
everything received. Before flashing, the log records the ELF's path and
SHA-256, so a retained log names its exact image. `--log-file` chooses another
path. Prefer this tool over piping `cargo embed` into a file, whose interactive
output carries cursor and colour control sequences.

The tool prints milestones so a slow probe is not mistaken for a hang:

```text
HOST: === FLASH STARTED: connecting, erasing, and programming ===
HOST: === FLASH ACTIVE: erase/program transfer observed ===
HOST: === FLASH PROGRAMMED: waiting for firmware boot/RTT ===
HOST: === FLASH SUCCEEDED: firmware boot and RTT observed ===
```

Only `FLASH SUCCEEDED` means the image booted far enough to produce RTT. A
failure prints `FLASH FAILED` and exits nonzero.

`--foxeer-smoke` is a self-terminating first check of a new SWD connection. It
adds `imu_transport_rtt` and `smoke_actuator_inhibit`, so the image cannot
drive motors, listens for 14 seconds after boot, checks the boot, RTT, IMU, and
data-ready evidence, and prints PASS or FAIL. `--probe-speed-khz` and
`--connect-under-reset` help a marginal SWD connection. Anything after `--`
runs instead of the default `probe-rs` command, with the same logging.

If nothing appears: check that `probe-rs` sees the probe, that no other
debugger holds it, and that the image was built with `defmt-rtt`.

## Heartbeat

Every two seconds the heartbeat logs the isolated stale IMU ticks it
tolerated, storage identity and capacity, and blackbox pages written, records
dropped, write faults, and the log-rate divisor. The `imu_transport_rtt`
feature adds raw gyro and data-ready interrupt counts, and
`imu_orientation_rtt` an orientation snapshot; [IMU](imu.md) describes them.

## Blackbox Analyzer

The analyzer reads one onboard flight exported by FerroConfigurator's
`convert FLIGHT.fwbb --csv`, or an RTT capture carrying `BB1` or `BB2` frames
from firmware that sent them, and measures the control rate from the log:

```powershell
python tools\blackbox_analyzer.py --mode rest flight.csv
```

`--mode auto`, the default, classifies large low-frequency movement as motion,
for hand-motion tests. `--mode rest` treats all filtered-gyro variation as
noise, for stationary noise-floor and props-off motor-vibration checks. For a
hand swing, trim the pickup and put-down:

```powershell
python tools\blackbox_analyzer.py --mode swing --trim-start 6 --trim-end 6
```

Pass a log path to analyze a specific capture.

## IMU Live View

```powershell
python tools\imu_live_view.py
```

The viewer runs `terminal_embed.py` and plots what it reads. The firmware no
longer sends `BB2` rate frames, so the measured-against-commanded plots stay
empty; the onboard blackbox and its CSV export carry those values. It does not
echo every RTT line, which would make the plot lag; `--echo-rtt` turns echo
on, and `--ui-hz 15` lowers the redraw rate on a slow PC.

From heartbeat IMU lines it draws an attitude estimate. `--attitude
gyro`, the default, seeds roll and pitch from gravity once and then integrates
gyro rates; `--attitude accel` keeps pulling roll and pitch back toward
gravity. Yaw is gyro-only in both and drifts. `accel_live_plot.py` is an older
name for the same viewer.

## IMU Noise Analyzer

A quick bench answer to whether the filtered gyro is quiet enough for the PID,
with the board still:

```powershell
python tools\imu_noise_analyzer.py --latest
python tools\imu_noise_analyzer.py --capture 30
```

It remaps raw gyro axes into the PID's roll, pitch, and yaw convention,
compares them with the filtered rates, and reports standard deviation,
peak-to-peak noise, attenuation, drift, and a rough quiet, watch, or noisy
verdict. It is a rest-noise screen, not a substitute for props-off motor checks
or vibration analysis.

## Onboard Logs

FerroConfigurator is the one tool that reads the onboard log format: it
downloads a flight, checking every page's CRC and sequence, and converts it.
The analyzer reads its CSV export, one flight per file:

```powershell
.\ferro-configurator.exe --port COM7 blackbox download --flight latest --output logs\foxeer-latest.fwbb
.\ferro-configurator.exe convert logs\foxeer-latest.fwbb --csv
python tools\blackbox_analyzer.py logs\foxeer-latest.csv --mode auto
```

`convert` without `--csv` writes ULog instead: one flight as the
`ferrowasp_rate_control` topic. The log does not store its control rate, so
both formats measure it over the flight and time each record by its control
tick at that rate; ULog carries it as the `control_rate_hz` info field and
sequence gaps as dropouts. A flight under 2 s is too short to measure; its CSV
export is refused, and its ULog keeps the records' millisecond timestamps.

## Flight Reports

`tools/flight_report.py` writes a Markdown summary and plots of
setpoint against measured rate, tracking error, PID output, motor output, and
the worst tracking-error events, under `logs\terminal_embed\reports\`:

```powershell
python tools\flight_report.py --trim-start 3 --trim-end 3
```

With no source it uses the newest log. It assumes a 400 Hz control rate, so on
a faster loop pass the real rate with `--sample-rate`.

## Safety Notes

- `terminal_embed.py` flashes, resets, and starts firmware. Put the ESCs in the
  intended bench-safe power state first.
- Run every motor or axis check with propellers removed.
- Motor authority stays in the firmware's safety and actuator-output path;
  none of these tools can command a motor.
