# Foxeer F405 V2 USB Quick Start

This is the normal Windows workflow for the Foxeer F405 V2. The ready release
package can:

- flash the included FerroWasp image through the STM32 ROM DFU bootloader;
- read, change, export, and restore flight parameters while disarmed;
- list and selectively download onboard flights;
- convert downloaded FWBB evidence to ULog for PlotJuggler.

Users do not need Rust, Python, STM32CubeProgrammer, an SWD debugger, or a
source checkout.

FerroWasp remains experimental flight-control firmware. Flashing replaces
firmware already installed in STM32 internal flash, including Betaflight.
Remove every propeller, disconnect the flight battery, and keep the ESCs
unpowered throughout USB maintenance.

## Download and extract

Download the current `ferrowasp-v*-windows-x86_64.zip` release and extract the
complete folder. Do not run the executable from inside the ZIP.

The folder contains the configurator, the reviewed DFU utility and licenses,
and a manifest-verified Foxeer firmware image:

```text
ferro-configurator.exe
dfu-util.exe
libusb-1.0.dll
firmware/
README-WINDOWS.txt
licenses/
sources/
```

Open PowerShell in that folder and inspect the host:

```powershell
.\ferro-configurator.exe doctor
```

The STM32 ROM bootloader may require a one-time Windows WinUSB association.
If DFU detection fails, check that the `STM32 BOOTLOADER` device with USB
identity `0483:DF11` is using WinUSB, then rerun:

```powershell
.\ferro-configurator.exe dfu list
```

## Validate the ready image

This command verifies the release manifest, image SHA-256, board identity,
memory range, and vector table without accessing USB or changing the board:

```powershell
.\ferro-configurator.exe flash --board foxeer-f405-v2 --dry-run
```

Require output naming the bundled release and Foxeer F405 V2, with a valid
flash base of `0x08000000`. Do not continue if manifest, hash, board, or ELF
validation fails.

## Flash through USB DFU

Start the guided flasher:

```powershell
.\ferro-configurator.exe flash --board foxeer-f405-v2
```

Follow its prompts:

1. Disconnect USB.
2. Hold the board's BOOT button.
3. Reconnect USB, wait approximately one second, and release BOOT.
4. Allow the configurator to detect exactly one STM32 ROM DFU device.
5. Type the requested confirmation only after checking the selected board and
   image.
6. Do not disconnect USB during programming.

The command must reach its final successful completion message and exit with
code zero. Detection or programming progress is not, by itself, proof that
the flash completed.

If the normal FerroWasp COM port does not return after programming, disconnect
and reconnect USB normally without holding BOOT.

ROM DFU updates internal MCU flash. It does not intentionally erase the
external SPI-NOR configuration or flight-log store. Verify both after every
flash.

## Connect to FerroWasp

List the known USB controller:

```powershell
.\ferro-configurator.exe device list
.\ferro-configurator.exe device info
```

If automatic selection is unavailable or more than one controller is
connected, select the intended COM port explicitly:

```powershell
$Port = "COM6"
.\ferro-configurator.exe --port $Port device info
```

Require a ready Winbond-compatible external flash, a ready IMU, and plausible
status. Close RTT readers, serial terminals, and other programs holding the
same COM port.

## Back up and inspect configuration

Keep the aircraft disarmed. The firmware rejects configuration writes while
armed.

```powershell
.\ferro-configurator.exe --port $Port config show
.\ferro-configurator.exe --port $Port config export foxeer-backup.toml
.\ferro-configurator.exe --port $Port config store my-foxeer
```

Named profiles are stored under
`%APPDATA%\FerroConfigurator\profiles` and survive replacement of the release
folder.

The fresh-storage Foxeer fallback is:

```text
roll/pitch/yaw P: 2.5 / 2.5 / 2.0
all I and D:       0
IMU LPF alpha:    0.55
log divisor:      1
RC deadband:      8
roll/pitch rates: center 70, maximum 300, expo 0.50
yaw rates:        center 70, maximum 200, expo 0.50
```

This is a flyable experimental baseline, not a universal tune.

## Change parameters

Change one value and require persistent readback verification:

```powershell
.\ferro-configurator.exe --port $Port config set roll.p 2.5
.\ferro-configurator.exe --port $Port config set pitch.p 2.5
.\ferro-configurator.exe --port $Port config set yaw.p 2.0
```

RC response is configurable in the same way:

```powershell
.\ferro-configurator.exe --port $Port config set rc-deadband 8
.\ferro-configurator.exe --port $Port config set roll-center-rate 70
.\ferro-configurator.exe --port $Port config set roll-max-rate 300
.\ferro-configurator.exe --port $Port config set roll-expo 0.5
```

List every accepted field and scalar range:

```powershell
.\ferro-configurator.exe config keys
```

For a complete profile:

```powershell
.\ferro-configurator.exe config validate foxeer-profile.toml
.\ferro-configurator.exe --port $Port config apply foxeer-profile.toml
.\ferro-configurator.exe --port $Port config show
```

Legacy schema-v1 profiles contain only the original eleven fields. Applying
one overlays those fields onto the connected board and preserves its current
RC-rate settings.

PID changes can cause violent oscillation. Change one controlled variable at a
time, verify the entire configuration, and repeat relevant props-off checks
before flight.

## List and download flights

After landing, disarm and leave the FCU powered for at least two seconds so the
last partial flash page can be committed.

```powershell
.\ferro-configurator.exe --port $Port blackbox flights
```

Download only the newest flight and create a ULog beside the retained raw
evidence:

```powershell
$Stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$Raw = "foxeer-$Stamp.fwbb"
$Ulog = "foxeer-$Stamp.ulg"

.\ferro-configurator.exe --port $Port blackbox download `
  --flight latest `
  --output $Raw `
  --ulog $Ulog
```

Download a known flight:

```powershell
.\ferro-configurator.exe --port $Port blackbox download `
  --flight 34 `
  --output flight-34.fwbb `
  --ulog flight-34.ulg
```

Downloads use a separate `.part` file. If USB is interrupted, repeat the exact
command with `--resume`. Existing pages are accepted only after CRC, flight
identity, and per-flight page-sequence validation:

```powershell
.\ferro-configurator.exe --port $Port blackbox download `
  --flight 34 `
  --output flight-34.fwbb `
  --resume `
  --ulog flight-34.ulg
```

Download a range without retrieving earlier flights:

```powershell
.\ferro-configurator.exe --port $Port blackbox download-range `
  --from 29 `
  --to latest `
  --directory .\flights `
  --ulog
```

Keep each `.fwbb` file. It is the CRC-protected acquisition evidence; ULog is
a derived analysis file.

## Convert an existing FWBB file

```powershell
.\ferro-configurator.exe convert flight-34.fwbb `
  --flight latest `
  --output flight-34.ulg
```

Open the `.ulg` file in PlotJuggler and select
`ferrowasp_rate_control`. The current topic includes raw and filtered gyro
rates, rate setpoints, PID effort, throttle, four motor commands, sequences,
and armed/fresh-IMU flags.

## Erase logs

Erase only after every required flight has been downloaded, retained, and
converted successfully:

```powershell
.\ferro-configurator.exe --port $Port blackbox erase --confirm
.\ferro-configurator.exe --port $Port blackbox flights
```

Erase is destructive, explicitly confirmed, firmware-gated while disarmed,
and verified by reading the empty catalogue.

## Developer image override

Developers may validate or flash an explicit locally built ELF by supplying it
as the positional argument:

```powershell
.\ferro-configurator.exe flash .\FerroWaspFoxeerF405V2 `
  --board foxeer-f405-v2 `
  --dry-run
```

This bypasses selection of the bundled release image. End users should omit
the path and use the manifest-verified image shipped in the release folder.

## Before flight

After any firmware or parameter change:

1. verify device information and the complete saved configuration;
2. cold-boot stationary and require healthy IMU, RC, DShot, ESC telemetry, and
   OSD;
3. repeat props-off motor direction, correction direction, disarm, and RC-loss
   checks when firmware, gains, wiring, motor/ESC hardware, or the airframe
   changed;
4. inspect propellers, motors, frame, wiring, battery retention, antennas, and
   center of gravity;
5. expand the flight envelope only after reviewing the latest log.
