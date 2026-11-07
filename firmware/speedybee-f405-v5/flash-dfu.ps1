[CmdletBinding()]
param(
    [switch]$BuildOnly,
    [string]$DfuPort
)

$ErrorActionPreference = "Stop"

$targetTriple = "thumbv7em-none-eabihf"
$binaryName = "FerroWaspSpeedyBeeF405V5"
$elfPath = Join-Path $PSScriptRoot "target/$targetTriple/release/$binaryName"
$binPath = "$elfPath.bin"

Push-Location $PSScriptRoot
try {
    $cargoArguments = @("build", "--release", "--locked")
    & cargo @cargoArguments
    if ($LASTEXITCODE -ne 0) {
        throw "SpeedyBee firmware build failed with exit code $LASTEXITCODE."
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
        throw "SpeedyBee DFU runner failed with exit code $LASTEXITCODE."
    }
} finally {
    Pop-Location
}
