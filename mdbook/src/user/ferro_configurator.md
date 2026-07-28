# FerroConfigurator

FerroConfigurator is the Windows-first companion application for the Foxeer
F405 V2. The release package combines the command-line tool, a
manifest-verified firmware image, and the reviewed USB DFU utility.

It can:

- validate and flash the packaged Foxeer image through STM32 ROM DFU;
- inspect board, flash, IMU, RC, and arming status over USB CDC;
- back up, validate, apply, and verify disarmed configuration;
- list and selectively download CRC-protected FWBB flights;
- resume interrupted downloads and convert FWBB data to ULog.

The configurator cannot arm the aircraft or command motors. Firmware owns
configuration validation and rejects storage changes while armed.

## Typical Workflow

1. Download and extract the current Windows package from
   [GitHub Releases](https://github.com/Eirik2020/ferro-wasp/releases).
2. Run `ferro-configurator.exe doctor`.
3. Validate the bundled image with
   `ferro-configurator.exe flash --board foxeer-f405-v2 --dry-run`.
4. Flash the board through the guided USB DFU workflow.
5. Connect to its USB CDC port to inspect configuration or retrieve logs.

The [Foxeer F405 V2 guide](foxeer_f405_v2.md) provides complete commands and
safety checks. Contributors building the tool or assembling packages should
use [Developer Setup](../developer_getting_started.md).
