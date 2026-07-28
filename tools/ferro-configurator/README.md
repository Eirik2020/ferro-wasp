# FerroConfigurator

FerroConfigurator is FerroWasp's Windows-first USB command-line application.
It is maintained in this repository as an isolated Rust workspace so host
serial and DFU dependencies remain separate from the embedded Cargo workspace.

End users should follow the
[Foxeer F405 V2 USB Quick Start](../../mdbook/src/user/foxeer_f405_v2.md) and
download the ready Windows package from
[GitHub Releases](https://github.com/Eirik2020/ferro-wasp/releases). They do
not need a source checkout, Rust, Python, STM32CubeProgrammer, or an SWD
debugger.

Locally built ZIPs are generated under this workspace's
[`dist`](dist/) folder:

```powershell
Get-ChildItem .\dist -Filter "ferrowasp-v*-windows-x86_64.zip"
explorer (Resolve-Path .\dist)
```

`dist` is intentionally ignored by Git and is absent until the packaging
script completes.

## Supported workflow

- Discover the FerroWasp USB CDC identity `16c0:27dd`.
- Inspect external flash and live safety/IMU/RC status.
- Read and persist all 21 firmware-whitelisted tuning and RC settings.
- Export, import, and store strict per-drone TOML profiles.
- Preserve new RC fields when applying a legacy eleven-field profile.
- Verify every saved configuration by complete readback.
- List flights grouped by recorded MCU boot session.
- Download `latest`, one flight ID, or an inclusive range without retrieving
  older flights.
- Resume only after validating page CRC, flight identity, and sequence.
- Convert one validated FWBB flight to the minimal
  `ferrowasp_rate_control` ULog topic.
- Erase logs only through an explicit confirmed command and verify the empty
  catalogue afterward.
- Validate and flash the packaged Foxeer F405 V2 image through STM32 ROM DFU.

The configurator cannot arm, command motors, grant actuator authority, bypass
firmware validation, or alter safety policy. Configuration and log mutation
remain firmware-gated while disarmed.

## Commands

```powershell
ferro-configurator.exe doctor
ferro-configurator.exe device list
ferro-configurator.exe --port COM6 device info

ferro-configurator.exe --port COM6 config show
ferro-configurator.exe --port COM6 config export backup.toml
ferro-configurator.exe --port COM6 config set roll.p 2.5
ferro-configurator.exe --port COM6 config apply profile.toml

ferro-configurator.exe --port COM6 blackbox flights
ferro-configurator.exe --port COM6 blackbox download `
  --flight latest --output latest.fwbb --ulog latest.ulg
ferro-configurator.exe --port COM6 blackbox download-range `
  --from 29 --to latest --directory flights --ulog
ferro-configurator.exe --port COM6 blackbox erase --confirm

ferro-configurator.exe convert retained.fwbb --output retained.ulg

ferro-configurator.exe flash --board foxeer-f405-v2 --dry-run
ferro-configurator.exe flash --board foxeer-f405-v2
```

Use `--format json` for machine-readable output.

## Architecture

The standard Foxeer image exposes a bounded ASCII USB protocol.
FerroConfigurator uses that proven contract. The experimental MSPv2 endpoint
is intentionally not required by this workspace.

Configuration key names and scalar ranges come from the firmware-owned
`ferrowasp-core::config` module. Firmware storage remains owned by
`ferrowasp-tasks`; the host cannot create a second authority.

Raw log acquisition, validation, ULog conversion, and analysis are separate
boundaries. A completed `.fwbb` file is retained even if conversion fails.
PlotJuggler remains the normal visualization tool; the larger Python analyzer
remains available to developers and was not duplicated here.

## Development

Run from this directory:

```powershell
cargo fmt --all --check
cargo check --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Build the executable:

```powershell
cargo build --release --locked -p ferro-configurator-cli
```

Build the exact Foxeer image first:

```powershell
Set-Location ..\..\apps\foxeer-f405-v2
cargo build --release --locked
Set-Location ..\..\tools\ferro-configurator
```

Then assemble the offline Windows package:

```powershell
.\packaging\build-release.ps1 -Version 0.1.0
```

The packaging script:

- verifies pinned `dfu-util` and `libusb` hashes;
- validates the Foxeer ELF through FerroConfigurator;
- creates `firmware/manifest.json` with commit, target, features, size, and
  SHA-256;
- includes the exact image, licenses, and corresponding third-party source;
- produces `dist/ferrowasp-v0.1.0-windows-x86_64.zip`.

The complete output folder is
[`tools/ferro-configurator/dist`](dist/). List or open it with:

```powershell
Get-ChildItem .\dist
explorer (Resolve-Path .\dist)
```

CI builds the exact standard Foxeer image and creates the ready Windows
package for tags or a manual workflow run.

## Developer ELF override

Passing a positional ELF deliberately bypasses bundled image selection:

```powershell
ferro-configurator.exe flash .\FerroWaspFoxeerF405V2 `
  --board foxeer-f405-v2 `
  --dry-run
```

The ELF parser rejects wrong architecture, address ranges, vector tables,
conflicting segments, and oversized files before USB access.

## License

FerroConfigurator is part of FerroWasp and is licensed under Apache-2.0. See
the repository [license](../../LICENSE.md) and [notice](../../NOTICE.md).
Bundled DFU components retain their own licenses and corresponding-source
archives.
