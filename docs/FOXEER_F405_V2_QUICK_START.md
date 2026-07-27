# Foxeer F405 V2 USB Quick Start

This guide covers the normal Windows USB workflow for the Foxeer F405 V2:

1. build and flash a release image through the STM32 ROM-DFU bootloader;
2. inspect and change flight parameters while disarmed;
3. download one onboard flight log;
4. validate it and convert it to ULog for PlotJuggler or PX4 tooling.

No SWD debugger is required.

FerroWasp remains experimental flight-control firmware. Direct flashing
replaces the firmware already installed in STM32 internal flash, including
Betaflight. Remove propellers and disconnect the flight battery before
flashing. Keep the ESCs unpowered throughout USB-only maintenance.

## Prerequisites

Install:

- Git;
- a current Rust toolchain with the `thumbv7em-none-eabihf` target;
- Python 3;
- STM32CubeProgrammer;
- the STM32 ROM-DFU driver supplied with STM32CubeProgrammer;
- PySerial for the FerroWasp USB CLI.

```powershell
rustup target add thumbv7em-none-eabihf
python -m pip install pyserial
```

Clone the repository and start PowerShell in its root:

```powershell
git clone <repository-url> C:\ws\ferro-wasp
Set-Location C:\ws\ferro-wasp
```

The repository-local DFU runner searches `PATH`, the ordinary
STM32CubeProgrammer installation directories, and STM32CubeCLT under `C:\ST`.
If discovery fails, provide the executable explicitly:

```powershell
$env:STM32_PROGRAMMER_CLI = `
  "C:\Program Files\STMicroelectronics\STM32Cube\STM32CubeProgrammer\bin\STM32_Programmer_CLI.exe"
```

## Build the release flight image

Build from the isolated Foxeer application workspace. `flash_blackbox` adds
USB configuration/storage and onboard armed-flight recording to the default
Foxeer board and DShot image.

```powershell
Set-Location C:\ws\ferro-wasp\apps\foxeer-f405-v2

cargo build --release --locked --features flash_blackbox

$FirmwareElf = Resolve-Path `
  "target\thumbv7em-none-eabihf\release\FerroWaspFoxeerF405V2"

Get-FileHash -LiteralPath $FirmwareElf -Algorithm SHA256
```

Retain the printed SHA-256 with the flight records. Do not add
`--no-default-features` or a `bench_*`, `pwm_cal`, smoke-inhibit, or
fault-injection feature to a flight image.

The app-local Cargo runner currently targets SWD. Use the explicit DFU runner
below; `cargo run` is not the USB-DFU command.

## Enter ROM DFU

1. Remove all propellers.
2. Disconnect the flight battery and debugger.
3. Close programs using the Foxeer COM port.
4. Disconnect the USB cable.
5. Hold the board's BOOT button.
6. Connect USB, wait approximately one second, and release BOOT.

Confirm that exactly one intended device is present:

```powershell
.\tools\dfu-runner.ps1 -ListOnly
```

A normal result identifies `STM32 BOOTLOADER`, device ID `0x0413`, on a port
such as `USB1`. If none appears, repeat the BOOT sequence and check for USB
identity VID `0483`, PID `DF11` in Windows Device Manager.

## Program, verify, and start

With one DFU device connected:

```powershell
.\tools\dfu-runner.ps1 -Elf $FirmwareElf
```

If multiple STM32 DFU devices are attached, select the intended port:

```powershell
.\tools\dfu-runner.ps1 -Elf $FirmwareElf -Port USB1
```

The runner validates that the ELF loads at `0x08000000`, prints its SHA-256,
programs internal flash, performs read-back verification, and starts the
firmware. Require the final message:

```text
Firmware programmed, verified, and started successfully.
```

Garbled progress-bar block characters in PowerShell are a terminal-encoding
artifact, not a programming error. If the application COM port does not appear
after a successful start, disconnect and reconnect USB normally without
holding BOOT.

ROM-DFU programming targets MCU internal flash. It does not intentionally
erase the external SPI-NOR configuration and flight-log store, but always
verify both after flashing.

## Connect to the FerroWasp USB CLI

Return to the repository root and use the COM port assigned by Windows:

```powershell
Set-Location C:\ws\ferro-wasp
$Port = "COM6"

python tools\ferrowasp_storage.py --port $Port info
python tools\ferrowasp_storage.py --port $Port config-show
python tools\ferrowasp_storage.py --port $Port flights
```

Require the Winbond-compatible flash identity, `ready=1`, a plausible
configuration, and the expected retained flight catalog. If the port cannot be
opened, reconnect USB, check its current COM number, and ensure a debugger is
not holding NRST low.

## Inspect and change flight parameters

Parameter writes are accepted only while disarmed. `config-set` stages a
candidate value; it does not persist or apply the candidate until
`config-save` succeeds.

Read all supported values:

```powershell
python tools\ferrowasp_storage.py --port $Port config-show
```

Example: stage roll and pitch P `2.5`, retain yaw P `2.0`, save once, and
verify:

```powershell
python tools\ferrowasp_storage.py --port $Port config-set roll_p 2.5
python tools\ferrowasp_storage.py --port $Port config-set pitch_p 2.5
python tools\ferrowasp_storage.py --port $Port config-set yaw_p 2
python tools\ferrowasp_storage.py --port $Port config-save
python tools\ferrowasp_storage.py --port $Port config-show
```

Example: change the roll stick curve:

```powershell
python tools\ferrowasp_storage.py --port $Port config-set roll_center_rate 70
python tools\ferrowasp_storage.py --port $Port config-set roll_max_rate 300
python tools\ferrowasp_storage.py --port $Port config-set roll_expo 0.5
python tools\ferrowasp_storage.py --port $Port config-save
python tools\ferrowasp_storage.py --port $Port config-show
```

Supported ranges are:

| Parameters | Accepted range |
|---|---:|
| `roll/pitch/yaw_p`, `_i`, `_d` | `0.0..20.0` |
| `imu_lpf_alpha` | `0.0..1.0` |
| `log_rate_divisor` | integer `1..16` |
| `rc_deadband` | integer `0..100` |
| each `*_center_rate` | `10..500 deg/s` |
| each `*_max_rate` | its center rate through `1200 deg/s` |
| each `*_expo` | `0.0..1.0` |

Set a higher maximum before increasing its center rate. When reducing a
maximum below the current center rate, lower the center first. Invalid
intermediate profiles are rejected.

The repository's current experimental Foxeer baseline is:

```text
roll/pitch/yaw P: 2.5 / 2.5 / 2.0
all I and D:       0
IMU LPF alpha:    0.55
log divisor:      1
RC deadband:      8
roll/pitch rates: center 70, maximum 300, expo 0.50
yaw rates:        center 70, maximum 200, expo 0.50
```

This is a flyable prototype baseline, not a universal tune. Changing gains can
cause violent oscillation. Change one variable per flight, verify the complete
saved configuration, and repeat the relevant props-off checks before flying.

## List and download flights

After landing, disarm and leave the FCU powered for at least two seconds so the
last partial flash page can be committed. Connect USB with the aircraft
disarmed:

```powershell
python tools\ferrowasp_storage.py --port $Port list
python tools\ferrowasp_storage.py --port $Port flights
```

`flights` groups recent captures by MCU boot session. Older recordings created
before boot markers appear under `boot unknown` but remain downloadable.

Download only the newest flight:

```powershell
$Stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$Flight = "latest"
$Blackbox = "logs\foxeer-flight-$Stamp-$Flight.fwbb"

python tools\ferrowasp_storage.py --port $Port read `
  --flight-id $Flight `
  --output $Blackbox
```

To download a known flight, replace `latest`:

```powershell
$Flight = 34
$Blackbox = "logs\foxeer-flight-$Stamp-flight-$Flight.fwbb"

python tools\ferrowasp_storage.py --port $Port read `
  --flight-id $Flight `
  --output $Blackbox
```

If a transfer times out, resume the same file and flight:

```powershell
python tools\ferrowasp_storage.py --port $Port read `
  --flight-id $Flight `
  --output $Blackbox `
  --resume
```

Resume validates the existing page sequence and CRC before appending. Keep the
raw `.fwbb` evidence until validation and conversion succeed.

## Validate and convert to ULog

Analyze the selected flight and export CSV:

```powershell
$Csv = [System.IO.Path]::ChangeExtension($Blackbox, ".csv")

python tools\blackbox_analyzer.py $Blackbox `
  --flight-id $Flight `
  --mode swing `
  --flight-window-throttle-min 500 `
  --drift-report `
  --csv $Csv
```

Require CRC-valid pages and review missing frames, repeated IMU samples,
control timing, tracking error, oscillation frequency, and motor effort.

Convert exactly that flight to ULog:

```powershell
$Ulog = [System.IO.Path]::ChangeExtension($Blackbox, ".ulg")

python tools\fwbb_to_ulog.py $Blackbox `
  --flight-id $Flight `
  --output $Ulog
```

Open the `.ulg` file in PlotJuggler and select
`ferrowasp_rate_control`. The current converter exposes gyro rates, rate
setpoints, total PID effort, throttle, four motor commands, sequences, and
armed/fresh-IMU flags. Firmware still records BB2 internally; this tool creates
one ULog file per selected flight.

Optional PX4 PyULog inspection:

```powershell
python -m pip install pyulog
ulog_info $Ulog
ulog2csv -m ferrowasp_rate_control $Ulog
```

Optional FerroWasp graph report:

```powershell
python -m pip install matplotlib
$Report = "logs\reports\foxeer-flight-$Stamp-$Flight"
python tools\flight_report.py $Csv --out-dir $Report
```

Open `$Report\report.md`.

## Erase logs only after validation

Erasing the onboard log region is destructive. Do it only after every required
flight has downloaded and passed CRC validation:

```powershell
python tools\ferrowasp_storage.py --port $Port erase --confirm
python tools\ferrowasp_storage.py --port $Port list
```

An empty store reports `used pages: 0; next flight: 1`.

## Before the next flight

After any flash or parameter change:

1. verify `config-show`, storage readiness, and the exact firmware SHA-256;
2. cold-boot stationary and require healthy IMU, RC, DShot, telemetry, and OSD;
3. repeat props-off direction, correction, disarm, and RC-loss checks when the
   firmware, gains, motor/ESC hardware, wiring, or airframe has changed;
4. inspect propellers, motors, frame, wiring, battery retention, antennas, and
   center of gravity;
5. expand the flight envelope only after reviewing the newest selected log.
