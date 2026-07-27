# Testing

FerroWasp testing is staged: static checks, props-off bench checks, then one
bounded flight change at a time. The user operates powered hardware. Stop on
any unexpected motor response, oscillation, heat, smoke, loss of RC/video, or
loss of confidence.

Current test definitions and stop conditions are in the
[test catalog](../../project_docs/testing/README.md). This page is the current
Foxeer F405 V2 USB field cheatsheet; it deliberately excludes flashing,
debugger, and remote-Pi workflows.

## Foxeer USB Field Cheatsheet

Run from the repository root. The aircraft must be disarmed for USB
configuration, log maintenance, and download operations.

```powershell
Set-Location C:\ws\ferro-wasp
$Port = "COM6"
```

If the port is missing, connect the FCU USB cable and ensure the debugger is
not holding reset low.

### Check storage and flights

```powershell
python tools\ferrowasp_storage.py --port $Port info
python tools\ferrowasp_storage.py --port $Port list
python tools\ferrowasp_storage.py --port $Port flights
```

New captures are grouped by MCU boot session. Older recordings made before
boot markers are shown under `boot unknown`.

### Verify or restore the current tuning baseline

The current Foxeer P-only baseline is roll/pitch/yaw P `1 / 1 / 2`, all I/D
terms `0`, LPF alpha `0.55`, and RC deadband `8`.

```powershell
python tools\ferrowasp_storage.py --port $Port config-show

python tools\ferrowasp_storage.py --port $Port config-set roll_p 1
python tools\ferrowasp_storage.py --port $Port config-set pitch_p 1
python tools\ferrowasp_storage.py --port $Port config-set yaw_p 2
python tools\ferrowasp_storage.py --port $Port config-set roll_i 0
python tools\ferrowasp_storage.py --port $Port config-set pitch_i 0
python tools\ferrowasp_storage.py --port $Port config-set yaw_i 0
python tools\ferrowasp_storage.py --port $Port config-set roll_d 0
python tools\ferrowasp_storage.py --port $Port config-set pitch_d 0
python tools\ferrowasp_storage.py --port $Port config-set yaw_d 0
python tools\ferrowasp_storage.py --port $Port config-save
python tools\ferrowasp_storage.py --port $Port config-show
```

Make one tuning change per flight and retain I/D at zero until separate target
evidence supports them.

### Adjust RC stick feel without reflashing

FerroWasp uses a small Betaflight Actual Rates-style curve. Configuration is
accepted only while disarmed; changes are staged until `config-save`.

```powershell
python tools\ferrowasp_storage.py --port $Port config-set rc_deadband 8
python tools\ferrowasp_storage.py --port $Port config-set roll_center_rate 70
python tools\ferrowasp_storage.py --port $Port config-set roll_max_rate 300
python tools\ferrowasp_storage.py --port $Port config-set roll_expo 0.5
python tools\ferrowasp_storage.py --port $Port config-set pitch_center_rate 70
python tools\ferrowasp_storage.py --port $Port config-set pitch_max_rate 300
python tools\ferrowasp_storage.py --port $Port config-set pitch_expo 0.5
python tools\ferrowasp_storage.py --port $Port config-set yaw_center_rate 70
python tools\ferrowasp_storage.py --port $Port config-set yaw_max_rate 200
python tools\ferrowasp_storage.py --port $Port config-set yaw_expo 0.5
python tools\ferrowasp_storage.py --port $Port config-save
python tools\ferrowasp_storage.py --port $Port config-show
```

Allowed values: deadband `0..100`; center rate `10..500 deg/s`; maximum rate
from its center rate through `1200 deg/s`; expo `0.0..1.0`. Set a higher
maximum before raising center rate; lower center rate before lowering maximum.

### Download, analyze, graph, and convert one flight

After landing, disarm and wait at least two seconds for the final flash page.

```powershell
$Stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$Blackbox = "logs\foxeer-flight-$Stamp.fwbb"
$Csv = "logs\foxeer-flight-$Stamp.csv"
$Ulog = "logs\foxeer-flight-$Stamp.ulg"
$Report = "logs\reports\foxeer-flight-$Stamp"

python tools\ferrowasp_storage.py --port $Port flights
python tools\ferrowasp_storage.py --port $Port read --flight-id latest --output $Blackbox
python tools\blackbox_analyzer.py $Blackbox --flight-id latest --mode swing --flight-window-throttle-min 500 --drift-report --csv $Csv
python tools\fwbb_to_ulog.py $Blackbox --flight-id latest --output $Ulog
python tools\flight_report.py $Csv --out-dir $Report
```

If a transfer times out, resume the same selected flight rather than beginning
again:

```powershell
python tools\ferrowasp_storage.py --port $Port read --flight-id latest --output $Blackbox --resume
```

Open the `.ulg` in PlotJuggler and inspect `ferrowasp_rate_control`. Open
`$Report\report.md` for setpoint-versus-measured, PID/error/motor, and
worst-error plots. `flight_report.py` requires Matplotlib:

```powershell
python -m pip install matplotlib
```

### Clear logs

Only erase after the required flight has downloaded and passed analysis.

```powershell
python tools\ferrowasp_storage.py --port $Port erase --confirm
python tools\ferrowasp_storage.py --port $Port list
```

The expected empty result is `used pages: 0; next flight: 1`.

## Final preflight and first hop

Use the active [Foxeer target procedure](../../project_docs/testing/targets/foxeer-f405-v2.md)
for the exact preflight and controlled-hop gates. For the current candidate:

- Verify the persisted `1 / 1 / 2` P-only baseline, I/D zero, and writable
  log store before flight.
- Power the FCU and ESCs together from the flight battery; keep it still during
  gyro calibration. Do not use USB as FCU power for flight.
- Confirm a disarmed healthy boot, correct battery/OSD indication, motor and
  propeller condition/orientation, CG, battery retention, RC/video link, clear
  area, and immediate abort plan.
- Start with a low, brief hover/hop and gentle separated inputs. Land, disarm,
  wait two seconds, then download and analyze that one flight before changing
  any tune value.
