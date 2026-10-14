# FerroWasp RTT Terminal MVP

This is the smallest host path for seeing drone firmware text in a Python terminal.

The firmware already uses `defmt-rtt`, so RTT bytes are not plain UTF-8. The
Python script runs a quiet build from the selected firmware app, then launches
`probe-rs run` so probe-rs decodes
the defmt RTT stream and keeps printing live firmware lines. Build warnings are
hidden on successful builds, but shown if the build fails.

## Run

Connect the probe and target, then run from the repository root:

```powershell
python tools\terminal_embed.py
```

The default board is the Foxeer F405 V2, once an SWD connection has been fitted
to it. Its app defaults to the normal DShot600/eRPM-qualified flight path:

```powershell
python tools\terminal_embed.py --board foxeer-f405-v2 --release --locked
```

This builds `firmware/foxeer-f405-v2`, flashes its own
`FerroWaspFoxeerF405V2` ELF through `probe-rs`, and decodes RTT. PA13/SWDIO and
PA14/SWCLK are reserved by the Foxeer board support and are not configured as LEDs.
The host prints explicit milestones so a quiet or slow probe cannot be mistaken
for a stalled command:

```text
HOST: === FLASH STARTED: connecting, erasing, and programming ===
HOST: === FLASH ACTIVE: erase/program transfer observed ===
HOST: === FLASH PROGRAMMED: waiting for firmware boot/RTT ===
HOST: === FLASH SUCCEEDED: firmware boot and RTT observed ===
```

An attach/program failure prints `HOST: === FLASH FAILED: ... ===` and returns
a nonzero exit code. `FLASH PROGRAMMED` alone is not a complete success; wait
for `FLASH SUCCEEDED`, which proves the programmed firmware booted far enough
to produce RTT.

For the first retrofitted SWD connection, use the self-terminating smoke test:

```powershell
python tools\terminal_embed.py --foxeer-smoke
```

This uses the normal arming-inhibited release image, waits through programming,
then listens for 14 seconds after firmware boot. It checks boot/RTT/IMU/EXTI
evidence, prints a PASS/FAIL summary, and interrupts `probe-rs` cleanly. It does
not enable a motor commissioning feature. The command automatically adds the
Foxeer `imu_transport_rtt` diagnostic and `smoke_actuator_inhibit` features, so
the flight-capable default remains compile-time actuator-locked for this test.
Routine Foxeer builds omit periodic IMU raw/DRDY lines to keep RTT available
for operational evidence.

You can also run `python terminal_embed.py` from inside the `tools` directory.

Use `--board`, `--release`, `--locked`, and `--features` to build and run a specific
bench image. For example:

```powershell
python tools\terminal_embed.py --release --locked --features "bench_equal_motors bench_dshot_unequal_motors"
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
python tools\terminal_embed.py -- probe-rs run --chip STM32F405RG --protocol swd --no-location --no-timestamps firmware\foxeer-f405-v2\target\thumbv7em-none-eabihf\debug\FerroWaspFoxeerF405V2
```

## Live View, Blackbox Frames, and Analyzers

`imu_live_view.py`, the `blackbox_defmt` RTT frames, `blackbox_analyzer.py`,
and `imu_noise_analyzer.py` are described in the book's
[RTT Debug Tools](../mdbook/src/rtt_debug_tools.md) chapter.

## Foxeer Onboard Flash CLI

The preferred end-user path is the Rust
[`ferro-configurator`](ferro-configurator/README.md), which ships with the
ready Foxeer image and supports all current settings, selective/resumable
downloads, range downloads, confirmed erase, and ULog conversion without
Python:

```powershell
ferro-configurator.exe --port COM7 config show
ferro-configurator.exe --port COM7 blackbox flights
ferro-configurator.exe --port COM7 blackbox download `
  --flight latest --output logs\foxeer-latest.fwbb `
  --ulog logs\foxeer-latest.ulg
```

The Python tools below remain lower-level development and analysis references.

`ferrowasp_storage.py` talks to the Foxeer USB CDC storage endpoint. Install
its only optional host dependency with `python -m pip install pyserial`.
Start with the standard image and identify the actual JEDEC device before any destructive command:

```powershell
python tools\terminal_embed.py --board foxeer-f405-v2 --release --locked
python tools\ferrowasp_storage.py --port COM7 info
python tools\ferrowasp_storage.py --port COM7 list
```

The expected capacity code for 16 MiB is `18`. Do not assume the manufacturer
or memory-type bytes; record what the fitted device reports. For the first destructive verification, remain disarmed and test only the permanently reserved scratch sector:

```powershell
python tools\terminal_embed.py --board foxeer-f405-v2 --release --locked
python tools\ferrowasp_storage.py --port COM7 test --confirm
```

After the standard blackbox service has recorded a props-off armed/disarmed session,
download its CRC-protected raw pages and analyze them with the existing tool:

```powershell
python tools\terminal_embed.py --board foxeer-f405-v2 --release --locked --probe-speed-khz 1800 --connect-under-reset
python tools\ferrowasp_storage.py --port COM7 list
python tools\ferrowasp_storage.py --port COM7 erase --confirm
python tools\ferrowasp_storage.py --port COM7 list
# Reboot or reflash the same image after erase, then record the armed session.
python tools\ferrowasp_storage.py --port COM7 read --output logs\foxeer-props-off.fwbb
python tools\blackbox_analyzer.py logs\foxeer-props-off.fwbb --mode auto --csv logs\foxeer-props-off.csv
```

If a long USB download is interrupted, validate the complete pages already on
disk and continue without restarting from page zero:

```powershell
python tools\ferrowasp_storage.py --port COM7 --timeout 10 read --resume --output logs\foxeer-props-off.fwbb
```

Downloads may contain several armed sessions. Select one flight for sequence,
rate, and jitter analysis so boundaries between flight IDs are not counted as
missing frames:

```powershell
python tools\blackbox_analyzer.py logs\foxeer-props-off.fwbb --flight-id latest --mode auto --csv logs\foxeer-latest.csv
```

For routine downloads, inspect the flight catalog and transfer only the desired
contiguous page range:

```powershell
python tools\ferrowasp_storage.py --port COM7 flights
python tools\ferrowasp_storage.py --port COM7 read --flight-id latest --output logs\foxeer-latest.fwbb
```

`--flight-id` also accepts a numeric ID. The host finds the range with bounded
page probes instead of downloading older flights. `--resume` validates every
existing page against the selected flight ID, page sequence, and CRC before
continuing:

```powershell
python tools\ferrowasp_storage.py --port COM7 read --flight-id 12 --resume --output logs\foxeer-flight12.fwbb
```

New firmware marks the first stored flight after each MCU boot. The `flights`
view groups subsequent flights beneath that boot heading. Pre-marker pages are
reported as `boot unknown`; no timestamp heuristic is used to invent historical
power-cycle boundaries.

`erase --confirm` erases every sector in the log partition, not the
configuration slots or scratch sector. Storage reads and all writes are
rejected while armed; in-progress erase/config/self-test maintenance is
aborted if arming begins. The CLI has no actuator or safety authority.
The initial erase must end with `used pages: 0` and `writable: True`. Rebooting
afterward resets the expected disarmed-record drops accumulated while flash
maintenance was busy, so record-drop evidence from the actual capture starts
clean.
For `.fwbb` input the analyzer rejects every invalid or truncated page and
prints the CRC-valid page count, record count, flight IDs, page-sequence
endpoints, partial-page count, and final-page record count before its normal
BB2 analysis. `--flight-id N` selects an explicit stored flight and
`--flight-id latest` selects the highest available ID.

## FWBB to ULog Converter

FerroConfigurator now provides the normal native conversion command:

```powershell
ferro-configurator.exe convert logs\foxeer-flight.fwbb `
  --flight latest `
  --output logs\foxeer-flight-latest.ulg
```

`fwbb_to_ulog.py` remains the Python reference implementation. It converts one
CRC-validated onboard flight into the same compact ULog file and selects the
latest flight by default:

```powershell
python tools\fwbb_to_ulog.py logs\foxeer-flight.fwbb `
  --flight-id latest `
  --output logs\foxeer-flight-latest.ulg
```

The single `ferrowasp_rate_control` topic contains the BB2 raw/filtered gyro
rates, rate setpoints, PID effort, throttle, four motor commands, sequence
counters, and safety/freshness flags. Its timestamp is elapsed microseconds
from the first selected sample. Sequence gaps are represented with standard
ULog dropout messages. The conversion is deterministic and never combines
separate flight IDs.

This is a host-side migration/visualization path. Firmware continues to record
the existing fixed-size `.fwbb` pages until a bounded native ULog storage path
is designed and verified.

## Flight Reports

To create a report folder with a markdown summary and plots:

```powershell
python tools\flight_report.py logs\terminal_embed\YYYYMMDD_HHMMSS_rtt.log --trim-start 3 --trim-end 3
```

With no source argument, the newest `.log` or `.csv` in `logs\terminal_embed` is
used:

```powershell
python tools\flight_report.py --trim-start 3 --trim-end 3
```

Reports are written under:

```text
logs\terminal_embed\reports\<source-name>\report.md
```

The generated plots include setpoint-vs-measured rate, rate tracking error, PID
output, motor output, and zooms around the worst tracking-error events.

## App Context Router

`app_context.py` is a deterministic, read-only map for the large Foxeer RTIC
app shell. It reports relevant task/helper ranges, priorities,
interrupt bindings, feature gates, resource-block anchors, companion files,
and potential test-catalog routes without printing the complete source.

Run it from the repository root:

```powershell
python tools\app_context.py --list-topics
python tools\app_context.py --topic control
python tools\app_context.py --board foxeer-f405-v2 --topic arming
python tools\app_context.py --validate
```

Output is capped at 8 KiB. Route validation fails on missing source symbols,
companion paths, test IDs, unassigned RTIC tasks, or budget growth. A route
selects context only; it does not change firmware, operate hardware, determine
which tests ultimately apply, or claim that a test passed.

## Test Evidence Metadata

New test results use bounded JSON metadata records described in
`project_meta/testing/evidence/README.md`. Raw logs and reports remain under
ignored `logs/` paths.

Validate the record layout, catalog identity, hardware execution boundary,
artifact metadata, and size limits with:

```powershell
python tools\test_evidence.py
```

The repository-wide `python tools\check_repository_context.py` command runs
the same evidence checks and also validates document registration, immutable
archives, context budgets, and the ADR index/record relationships. Neither
command decides that a test passed or that hardware is safe to operate.

The python tools/check_rtic_boundaries.py command enforces the mechanical
thin-app rules: internal-only RTIC imports, RTIC-only declarations, shared
board data models, no direct timer-register sequencing in board support, and
no exact cross-board Rust source copies.

## Bench Gates

`tools/bench_rig.py` runs BENCH-COMMON-001 (`boot-idle --board <target>`) on
any flight board over SWD, and `tools/foxeer_bench_gates.py` runs
BENCH-FOX-USB-001 (`usb-config`) on a connected Foxeer. The operator confirms
propellers removed and actuator power disconnected, and performs each USB power
cycle when prompted; the tools drive the rest, stop at the first failed check,
keep every capture in a new `logs/bench/` directory, and draft a schema 2 run
record for review. They never commit.

```text
python3 tools/bench_rig.py boot-idle --board tbs-lucid-h7 --allow-no-storage
python3 tools/foxeer_bench_gates.py usb-config --build-record <BUILD-FOX-001 record>
```

`bench_rig.py check-boot-idle` and `foxeer_bench_gates.py check-usb-config`
apply the same checks to existing captures, so a manual run is held to the
same standard.

## Troubleshooting

- Check that the probe is visible to `probe-rs`.
- Check that `Embed.toml` uses `STM32F405RG`.
- Check that the firmware is built with `defmt-rtt`.
- If you only see build output, wait for flashing/reset and RTT attach to finish.
- If another debugger owns the probe, close it before running this script.
