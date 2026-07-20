[CmdletBinding()]
param(
    [switch]$BuildOnly,
    [switch]$UsbDebug,
    [string]$DfuPort
)

$ErrorActionPreference = "Stop"

$targetTriple = "thumbv7em-none-eabihf"
$binaryName = "FerroWaspFoxeerF405V2"
$elfPath = Join-Path $PSScriptRoot "target/$targetTriple/release/$binaryName"
$binPath = "$elfPath.bin"

Push-Location $PSScriptRoot
try {
    $cargoArguments = @("build", "--release", "--locked")
    if ($UsbDebug) {
        $cargoArguments += @("--features", "usb_serial")
    }

    & cargo @cargoArguments
    if ($LASTEXITCODE -ne 0) {
        throw "Foxeer firmware build failed with exit code $LASTEXITCODE."
    }

    if (-not (Test-Path -LiteralPath $elfPath -PathType Leaf)) {
        throw "Expected firmware ELF was not produced at '$elfPath'."
    }

    $objcopy = Get-Command arm-none-eabi-objcopy -ErrorAction SilentlyContinue
    if ($null -eq $objcopy) {
        $objcopy = Get-Command rust-objcopy -ErrorAction SilentlyContinue
    }
    if ($null -eq $objcopy) {
        throw "Install arm-none-eabi-objcopy or cargo-binutils/rust-objcopy to create the DFU image."
    }

    & $objcopy.Source -O binary $elfPath $binPath
    if ($LASTEXITCODE -ne 0) {
        throw "Firmware binary conversion failed with exit code $LASTEXITCODE."
    }

    Write-Host "Built DFU image: $binPath"
    if ($UsbDebug) {
        Write-Host "USB CDC read-only diagnostics are enabled in this image."
    }
    if ($BuildOnly) {
        return
    }

    $runnerPath = Join-Path $PSScriptRoot "tools/dfu-runner.ps1"
    $runnerArguments = @(
        "-NoProfile",
        "-ExecutionPolicy", "Bypass",
        "-File", $runnerPath,
        "-Elf", $elfPath
    )
    if (-not [string]::IsNullOrWhiteSpace($DfuPort)) {
        $runnerArguments += @("-Port", $DfuPort)
    }

    & powershell @runnerArguments
    if ($LASTEXITCODE -ne 0) {
        throw "Foxeer DFU runner failed with exit code $LASTEXITCODE."
    }
} finally {
    Pop-Location
}
