# RTT Debug Tools

These Python tools flash a board over SWD, decode its `defmt` RTT output
through `probe-rs`, keep a log of it, and plot or analyze what it reports.
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

## Heartbeat

Every two seconds the heartbeat logs the isolated stale IMU ticks it
tolerated, storage identity and capacity, and blackbox pages written, records
dropped, write faults, and the log-rate divisor. The `imu_transport_rtt`
feature adds raw gyro and data-ready interrupt counts, and
`imu_orientation_rtt` an orientation snapshot; [IMU](imu.md) describes them.

## Blackbox Frames over RTT

The `blackbox_defmt` feature emits one compact `BB2` frame per control tick
over the same RTT stream, which `defmt-rtt` owns:

```powershell
python tools\terminal_embed.py --release --locked --features blackbox_defmt
```

```text
BB2 seq 1234 imu 5678 flags 3 raw10 [12, -21, 6] gyro10 [10, -20, 5] cmd10 [0, 0, 0] pid [1, -2, 0] thr 1175 motors [1175, 1176, 1174, 1175]
```

| Field | Meaning |
|---|---|
| `seq` | Control-tick sequence |
| `imu` | IMU sample sequence used by this tick |
| `flags` | Bit 0 armed, bit 1 IMU fresh |
| `raw10` | Unfiltered roll, pitch, yaw rates in 0.1 deg/s |
| `gyro10` | Filtered roll, pitch, yaw rates in 0.1 deg/s, as the PID sees them |
| `cmd10` | Commanded roll, pitch, yaw rates in 0.1 deg/s |
| `pid` | Total PID output per axis |
| `thr` | Throttle setpoint in PWM-style command units |
| `motors` | Mixed motor commands in PWM-style command units |

Disarmed, PID and motor fields are zero. With `bench_equal_motors`, PID is zero
and every motor carries the capped throttle. A gap in `seq` is RTT transport
loss, reported separately from IMU progress.

## Blackbox Analyzer

With no argument, the analyzer reads the newest RTT log and measures the
control rate from it:

```powershell
python tools\blackbox_analyzer.py --mode rest --csv logs\terminal_embed\latest_blackbox.csv
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

The viewer runs `terminal_embed.py` and plots what it reads. From `BB2` frames
it shows measured against commanded rates, PID output, throttle, and motors. It
does not echo every RTT line, which would make the plot lag; `--echo-rtt`
turns echo on, and `--ui-hz 15` lowers the redraw rate on a slow PC.

From heartbeat IMU lines it can also draw an attitude estimate. `--attitude
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

## Safety Notes

- `terminal_embed.py` flashes, resets, and starts firmware. Put the ESCs in the
  intended bench-safe power state first.
- Run every motor or axis check with propellers removed.
- Motor authority stays in the firmware's safety and actuator-output path;
  none of these tools can command a motor.
