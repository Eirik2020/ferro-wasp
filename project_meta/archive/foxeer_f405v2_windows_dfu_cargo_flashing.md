# Flashing FerroWasp to the FOXEER F405 V2 over USB DFU on Windows

> Historical developer workflow. The supported user workflow now lives in
> `mdbook/src/user/foxeer_f405_v2.md`.

This guide shows how to flash Rust firmware directly to the FOXEER F405 V2 without WSL and without exposed SWD pads.

The workflow is:

1. Put the STM32F405 into its built-in USB DFU bootloader.
2. Build FerroWasp with Cargo.
3. Let Cargo invoke STM32CubeProgrammer automatically.
4. Program the Cargo-generated ELF directly into internal flash.

---

## 1. Requirements

Install:

- Rust
- The `thumbv7em-none-eabihf` Rust target
- STM32CubeProgrammer for Windows
- The STM32 USB DFU driver included with STM32CubeProgrammer

Install the Rust target:

```powershell
rustup target add thumbv7em-none-eabihf
```

STM32CubeProgrammer normally installs its CLI at:

```text
C:\Program Files\STMicroelectronics\STM32Cube\STM32CubeProgrammer\bin\STM32_Programmer_CLI.exe
```

The exact path may vary slightly between releases.

---

## 2. Firmware memory layout

For direct flashing without a flash-resident bootloader, FerroWasp must start at the beginning of STM32 internal flash:

```text
0x08000000
```

Example `memory.x`:

```ld
MEMORY
{
    FLASH : ORIGIN = 0x08000000, LENGTH = 1024K
    RAM   : ORIGIN = 0x20000000, LENGTH = 128K
}
```

The STM32F405 also contains 64 KiB of CCM RAM at:

```text
0x10000000
```

DMA controllers cannot access CCM RAM. Keep DMA buffers in normal SRAM.

A later layout may expose CCM as a separate region:

```ld
MEMORY
{
    FLASH : ORIGIN = 0x08000000, LENGTH = 1024K
    RAM   : ORIGIN = 0x20000000, LENGTH = 128K
    CCM   : ORIGIN = 0x10000000, LENGTH = 64K
}
```

---

## 3. Cargo configuration

Create or update:

```text
.cargo/config.toml
```

Use:

```toml
[build]
target = "thumbv7em-none-eabihf"

[target.thumbv7em-none-eabihf]
runner = [
    "powershell",
    "-NoProfile",
    "-ExecutionPolicy", "Bypass",
    "-File", "tools/dfu-runner.ps1"
]
```

Cargo appends the compiled executable path to the runner automatically.

Therefore:

```powershell
cargo run --release
```

will:

1. Compile FerroWasp.
2. Pass the generated ELF to `tools/dfu-runner.ps1`.
3. Invoke STM32CubeProgrammer.
4. Flash and verify the ELF through USB DFU.

---

## 4. PowerShell DFU runner

Create:

```text
tools/dfu-runner.ps1
```

Contents:

```powershell
param(
    [Parameter(Mandatory = $true)]
    [string]$Elf
)

$ErrorActionPreference = "Stop"

$possibleCliPaths = @(
    "$env:ProgramFiles\STMicroelectronics\STM32Cube\STM32CubeProgrammer\bin\STM32_Programmer_CLI.exe",
    "${env:ProgramFiles(x86)}\STMicroelectronics\STM32Cube\STM32CubeProgrammer\bin\STM32_Programmer_CLI.exe"
)

$cli = $possibleCliPaths |
    Where-Object { Test-Path $_ } |
    Select-Object -First 1

if (-not $cli) {
    throw @"
STM32CubeProgrammer CLI was not found.

Expected one of:
$($possibleCliPaths -join "`n")

Install STM32CubeProgrammer or update the path in tools/dfu-runner.ps1.
"@
}

$resolvedElf = (Resolve-Path $Elf).Path

Write-Host "Firmware ELF: $resolvedElf"
Write-Host "Programmer:    $cli"
Write-Host ""
Write-Host "Searching for STM32 devices in USB DFU mode..."

& $cli -l usb

if ($LASTEXITCODE -ne 0) {
    throw "Failed to enumerate USB DFU devices."
}

Write-Host ""
Write-Host "Programming firmware..."

& $cli `
    -c port=USB1 `
    -w $resolvedElf `
    -v `
    -rst

if ($LASTEXITCODE -ne 0) {
    throw "STM32CubeProgrammer failed with exit code $LASTEXITCODE."
}

Write-Host ""
Write-Host "Firmware programmed and verified successfully."
```

### Why program the ELF directly?

STM32CubeProgrammer reads load addresses from the ELF file. This avoids:

- Converting the ELF to a raw binary
- Manually specifying the binary load address
- Accidentally programming a binary at the wrong address

The ELF must still be linked with flash beginning at `0x08000000`.

---

## 5. Entering USB DFU mode

Use the STM32F405 factory ROM bootloader.

Recommended sequence:

1. Remove all propellers.
2. Disconnect the flight battery.
3. Disconnect USB.
4. Hold the board's **BOOT** button.
5. Connect the USB cable.
6. Release the BOOT button.
7. Run:

```powershell
cargo run --release
```

The board should appear in Windows Device Manager as an STM32 bootloader or STM32 DFU device.

A typical USB identifier is:

```text
VID 0483
PID DF11
```

---

## 6. Verify DFU detection manually

Before testing Cargo integration, verify that STM32CubeProgrammer can see the board:

```powershell
& "C:\Program Files\STMicroelectronics\STM32Cube\STM32CubeProgrammer\bin\STM32_Programmer_CLI.exe" -l usb
```

A detected device normally appears as a USB DFU port such as:

```text
USB1
```

You can also open the STM32CubeProgrammer GUI and select `USB` as the connection type.

---

## 7. Manual flashing command

To test flashing without Cargo:

```powershell
& "C:\Program Files\STMicroelectronics\STM32Cube\STM32CubeProgrammer\bin\STM32_Programmer_CLI.exe" `
    -c port=USB1 `
    -w "target\thumbv7em-none-eabihf\release\ferrowasp" `
    -v `
    -rst
```

Replace `ferrowasp` with the actual executable name produced by the project.

Rust ELF files on Windows commonly have no `.elf` extension.

---

## 8. Normal development command

Once configured:

```powershell
cargo run --release
```

Expected behavior:

1. Cargo builds FerroWasp.
2. The runner receives the generated ELF path.
3. STM32CubeProgrammer connects to the DFU port.
4. Internal flash is erased as needed.
5. The firmware is programmed.
6. The programmed contents are verified.
7. The MCU is reset.

You must normally enter DFU mode manually before each flash.

---

## 9. Restoring Betaflight

Direct flashing at:

```text
0x08000000
```

overwrites the installed Betaflight firmware.

It does not overwrite the STM32 factory ROM bootloader. Betaflight can be restored later through the same BOOT-button DFU procedure.

Before replacing Betaflight, save:

- `diff all`
- `dump all`
- Receiver configuration
- UART assignments
- OSD settings
- Motor protocol
- Board orientation
- Voltage and current calibration values

---

## 10. Common problems

### No USB DFU device appears

Check:

- The BOOT button was held before connecting USB.
- The USB cable supports data.
- The board is powered from USB.
- The STM32CubeProgrammer DFU driver is installed.
- Windows Device Manager does not show an unknown device.
- Another program is not holding the DFU interface.

Retry:

1. Disconnect USB.
2. Hold BOOT.
3. Reconnect USB.
4. Wait one second.
5. Release BOOT.
6. Run the USB-list command again.

### `USB1` is not found

List USB DFU devices:

```powershell
& "C:\Program Files\STMicroelectronics\STM32Cube\STM32CubeProgrammer\bin\STM32_Programmer_CLI.exe" -l usb
```

If the board appears under another USB port identifier, update:

```powershell
-c port=USB1
```

in the runner script.

### Firmware flashes but does not start

Check:

- `FLASH` begins at `0x08000000`.
- The vector table is linked at `0x08000000`.
- The correct STM32F405 PAC/HAL feature is enabled.
- Clock initialization expects the 8 MHz HSE.
- No watchdog is causing immediate reset.
- No early panic occurs before diagnostics are initialized.
- The MCU was reset or power-cycled after programming.

Disconnect and reconnect USB without holding BOOT.

### Flashing works once, then the board disappears

This is expected after leaving DFU mode. Normal FerroWasp firmware does not enumerate as the STM32 ROM DFU device.

To flash again:

1. Disconnect USB.
2. Hold BOOT.
3. Connect USB.
4. Release BOOT.
5. Run `cargo run --release`.

### Windows blocks the PowerShell script

The Cargo runner already specifies:

```text
-ExecutionPolicy Bypass
```

To test manually:

```powershell
powershell `
    -NoProfile `
    -ExecutionPolicy Bypass `
    -File tools\dfu-runner.ps1 `
    target\thumbv7em-none-eabihf\release\ferrowasp
```

---

## 11. Optional batch-file runner

Create:

```text
tools\dfu-runner.cmd
```

```bat
@echo off
setlocal

set "ELF=%~1"
set "CLI=%ProgramFiles%\STMicroelectronics\STM32Cube\STM32CubeProgrammer\bin\STM32_Programmer_CLI.exe"

if not exist "%CLI%" (
    echo STM32CubeProgrammer CLI not found:
    echo %CLI%
    exit /b 1
)

"%CLI%" -l usb
if errorlevel 1 exit /b %errorlevel%

"%CLI%" -c port=USB1 -w "%ELF%" -v -rst
exit /b %errorlevel%
```

Cargo configuration:

```toml
[target.thumbv7em-none-eabihf]
runner = ["cmd", "/C", "tools\\dfu-runner.cmd"]
```

The PowerShell runner is preferred because it provides clearer diagnostics and stronger error handling.

---

## 12. Safety notes

Before flashing or testing actuator firmware:

- Remove propellers.
- Disconnect the LiPo unless it is specifically required.
- Do not connect an ESC during initial motor-output work unless necessary.
- Initialize every motor pin to a safe inactive state.
- Do not enable TIM1 main output until the actuator safety state permits it.
- Remember that M4 uses TIM1_CH3N, a complementary output.
- Check motor waveforms with an oscilloscope or logic analyzer first.
- Ensure panic and fault paths disable actuator outputs.

---

## 13. Recommended repository layout

```text
ferrowasp/
├── .cargo/
│   └── config.toml
├── memory.x
├── tools/
│   └── dfu-runner.ps1
├── src/
│   └── main.rs
└── Cargo.toml
```

This keeps native-Windows flashing reproducible for contributors.

---

## 14. Recommended first test firmware

Before enabling the IMU or motors, use a minimal firmware that:

1. Starts from the 8 MHz HSE.
2. Configures the system clock.
3. Initializes a diagnostic interface.
4. Periodically reports or toggles a known signal.
5. Leaves all motor pins as low GPIO outputs.
6. Does not configure PA13 or PA14 as LEDs during early development.

Because SWD is not exposed, useful diagnostics include:

- A temporary UART output
- USB CDC implemented by FerroWasp
- Existing LEDs after startup
- Buzzer patterns
- Observable GPIO toggling
- Re-entering ROM DFU using the BOOT button

A temporary UART diagnostic output is strongly recommended for initial board bring-up.

---

## 15. Summary

The FOXEER F405 V2 can be flashed natively from Windows without WSL and without exposed SWD.

The normal command is:

```powershell
cargo run --release
```

with Cargo configured to invoke:

```text
STM32_Programmer_CLI.exe
```

through a PowerShell runner.

Critical requirements:

- Enter ROM DFU manually with the BOOT button.
- Link FerroWasp at `0x08000000`.
- Program the Cargo-generated ELF directly.
- Keep DMA buffers out of CCM RAM.
- Preserve safe motor-output states during startup.
- Save the Betaflight configuration before overwriting it.
