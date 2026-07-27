FerroWasp ready package for Windows x86-64
===========================================

This folder includes FerroConfigurator, the Foxeer F405 V2 release image,
dfu-util, libusb, the release manifest, licenses, and corresponding source.
Rust, Python, STM32CubeProgrammer, and SWD are not required.

REMOVE EVERY PROPELLER AND DISCONNECT THE FLIGHT BATTERY BEFORE USB
MAINTENANCE. Flashing replaces firmware already on the MCU, including
Betaflight.

READY FOXEER USB FLASH
----------------------

Open PowerShell in this extracted folder:

  .\ferro-configurator.exe doctor
  .\ferro-configurator.exe flash --board foxeer-f405-v2 --dry-run
  .\ferro-configurator.exe flash --board foxeer-f405-v2

The first flash command validates the bundled manifest, SHA-256, board memory
map, and vector table without touching USB. The second starts the guided BOOT
button/STM32 ROM DFU workflow. Require its final successful message.

Windows may require a one-time WinUSB association for STM32 BOOTLOADER
0483:DF11. Diagnose it with:

  .\ferro-configurator.exe dfu list

CONFIGURATION
-------------

After the normal FerroWasp COM port returns:

  .\ferro-configurator.exe device list
  .\ferro-configurator.exe --port COM6 device info
  .\ferro-configurator.exe --port COM6 config show
  .\ferro-configurator.exe --port COM6 config export backup.toml
  .\ferro-configurator.exe --port COM6 config set roll.p 2.5

Replace COM6 with the actual port. Writes are accepted only while disarmed and
are not reported successful until persistent readback matches.

FLIGHT LOGS AND ULOG
--------------------

  .\ferro-configurator.exe --port COM6 blackbox flights
  .\ferro-configurator.exe --port COM6 blackbox download --flight latest `
    --output latest.fwbb --ulog latest.ulg

If interrupted, repeat the exact download with --resume. Preserve the raw
.fwbb file; the .ulg file is derived for PlotJuggler.

Erase only after retaining every required flight:

  .\ferro-configurator.exe --port COM6 blackbox erase --confirm

The configurator never arms the controller or commands motors. See the
repository Foxeer F405 V2 USB Quick Start for the full workflow and preflight
requirements.
