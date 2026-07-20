[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [string]$Elf,

    [string]$Port,

    [switch]$ListOnly,

    [switch]$DryRun
)

$ErrorActionPreference = "Stop"

function Resolve-Stm32ProgrammerCli {
    $candidates = @()

    if (-not [string]::IsNullOrWhiteSpace($env:STM32_PROGRAMMER_CLI)) {
        $candidates += $env:STM32_PROGRAMMER_CLI
    }

    $candidates += @(
        "$env:ProgramFiles\STMicroelectronics\STM32Cube\STM32CubeProgrammer\bin\STM32_Programmer_CLI.exe",
        "${env:ProgramFiles(x86)}\STMicroelectronics\STM32Cube\STM32CubeProgrammer\bin\STM32_Programmer_CLI.exe"
    )

    $pathCommand = Get-Command STM32_Programmer_CLI.exe -ErrorAction SilentlyContinue
    if ($null -ne $pathCommand) {
        $candidates += $pathCommand.Source
    }

    if (Test-Path -LiteralPath "C:\ST" -PathType Container) {
        $cltPaths = Get-ChildItem -LiteralPath "C:\ST" -Directory -Filter "STM32CubeCLT_*" |
            Sort-Object Name -Descending |
            ForEach-Object {
                Join-Path $_.FullName "STM32CubeProgrammer\bin\STM32_Programmer_CLI.exe"
            }
        $candidates += $cltPaths
    }

    $cli = $candidates |
        Where-Object {
            -not [string]::IsNullOrWhiteSpace($_) -and
            (Test-Path -LiteralPath $_ -PathType Leaf)
        } |
        Select-Object -Unique |
        Select-Object -First 1

    if ($null -eq $cli) {
        throw @"
STM32CubeProgrammer CLI was not found.

Install STM32CubeProgrammer, add STM32_Programmer_CLI.exe to PATH, or set:
  `$env:STM32_PROGRAMMER_CLI = 'C:\path\to\STM32_Programmer_CLI.exe'
"@
    }

    return (Resolve-Path -LiteralPath $cli).Path
}

function Get-Stm32DfuPorts {
    param(
        [Parameter(Mandatory)]
        [string]$Cli
    )

    Write-Host "Searching for STM32 devices in USB DFU mode..."
    $listOutput = @(& $Cli -l usb 2>&1)
    $listExitCode = $LASTEXITCODE
    $listOutput | ForEach-Object { Write-Host $_ }

    if ($listExitCode -ne 0) {
        throw "STM32CubeProgrammer failed to enumerate USB DFU devices (exit $listExitCode)."
    }

    $text = $listOutput -join "`n"
    $matches = [regex]::Matches(
        $text,
        "(?im)^\s*Device\s+Index\s*:\s*(USB[0-9]+)\s*$"
    )
    if ($matches.Count -eq 0) {
        $matches = [regex]::Matches($text, "(?i)\b(USB[0-9]+)\b")
    }

    return @(
        $matches |
            ForEach-Object { $_.Groups[1].Value.ToUpperInvariant() } |
            Select-Object -Unique
    )
}

function Resolve-AndValidateElf {
    param(
        [Parameter(Mandatory)]
        [string]$Path
    )

    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Firmware ELF does not exist: $Path"
    }

    $resolved = (Resolve-Path -LiteralPath $Path).Path
    $stream = [System.IO.File]::OpenRead($resolved)
    try {
        $magic = [byte[]]::new(4)
        if ($stream.Read($magic, 0, $magic.Length) -ne $magic.Length -or
            $magic[0] -ne 0x7f -or
            $magic[1] -ne [byte][char]'E' -or
            $magic[2] -ne [byte][char]'L' -or
            $magic[3] -ne [byte][char]'F') {
            throw "Cargo runner expected an ELF executable, but '$resolved' is not ELF."
        }
    } finally {
        $stream.Dispose()
    }

    $readElf = Get-Command arm-none-eabi-readelf -ErrorAction SilentlyContinue
    if ($null -ne $readElf) {
        $segments = @(& $readElf.Source --segments $resolved 2>&1)
        if ($LASTEXITCODE -ne 0) {
            throw "arm-none-eabi-readelf could not inspect '$resolved'."
        }

        $segmentText = $segments -join "`n"
        if ($segmentText -notmatch "(?im)^\s*LOAD\s+\S+\s+0x08000000\s+0x08000000\b") {
            throw @"
Refusing to flash: the ELF does not contain a load segment at 0x08000000.
Check apps/foxeer-f405-v2/memory.x before using direct ROM-DFU flashing.
"@
        }
        Write-Host "ELF flash origin: 0x08000000 (validated)"
    } else {
        Write-Warning "arm-none-eabi-readelf is unavailable; ELF flash-origin validation was skipped."
    }

    $hash = (Get-FileHash -LiteralPath $resolved -Algorithm SHA256).Hash
    Write-Host "Firmware ELF:     $resolved"
    Write-Host "Firmware SHA-256: $hash"
    return $resolved
}

$cli = Resolve-Stm32ProgrammerCli
Write-Host "Programmer:       $cli"

$ports = @(Get-Stm32DfuPorts -Cli $cli)
if ($ListOnly) {
    if ($ports.Count -eq 0) {
        Write-Warning "No STM32 ROM-DFU device is currently connected."
    } else {
        Write-Host "Detected DFU ports: $($ports -join ', ')"
    }
    return
}

if ([string]::IsNullOrWhiteSpace($Elf)) {
    throw "An ELF path is required unless -ListOnly is used."
}

$resolvedElf = Resolve-AndValidateElf -Path $Elf
$dryRunRequested = $DryRun -or
    $env:FERROWASP_DFU_DRY_RUN -match "^(1|true|yes)$"

if ($ports.Count -eq 0) {
    if ($dryRunRequested) {
        Write-Host "Dry run complete. Cargo runner and ELF layout are valid; no DFU device is connected."
        return
    }

    throw @"
No STM32 device in ROM DFU mode was detected.

Disconnect USB, hold BOOT, reconnect USB, wait one second, release BOOT,
then retry. Expected USB identity: VID 0483, PID DF11.
"@
}

if ([string]::IsNullOrWhiteSpace($Port)) {
    $Port = $env:STM32_DFU_PORT
}
if (-not [string]::IsNullOrWhiteSpace($Port)) {
    $Port = $Port.ToUpperInvariant()
    if ($Port -notmatch "^USB[0-9]+$") {
        throw "Invalid DFU port '$Port'. Expected a value such as USB1."
    }
    if ($ports -notcontains $Port) {
        throw "Requested DFU port $Port was not detected. Available: $($ports -join ', ')"
    }
    $selectedPort = $Port
} elseif ($ports.Count -eq 1) {
    $selectedPort = $ports[0]
} else {
    throw @"
Multiple STM32 DFU devices were detected: $($ports -join ', ')
Set `$env:STM32_DFU_PORT to the intended port, then retry.
"@
}

Write-Host "Selected DFU port: $selectedPort"
if ($dryRunRequested) {
    Write-Host "Dry run complete. Firmware would be programmed, verified, and started; flash was not changed."
    return
}

Write-Host "Programming and verifying the Foxeer firmware..."
$programmerElf = $resolvedElf
$temporaryProgrammerElf = $null

if ([System.IO.Path]::GetExtension($resolvedElf) -ne ".elf") {
    $temporaryProgrammerElf = "$resolvedElf.stm32cube-$([guid]::NewGuid().ToString('N')).elf"
    Copy-Item -LiteralPath $resolvedElf -Destination $temporaryProgrammerElf
    $programmerElf = $temporaryProgrammerElf
    Write-Host "CubeProgrammer ELF: $programmerElf"
}

try {
    & $cli -c "port=$selectedPort" -w $programmerElf -v
    if ($LASTEXITCODE -ne 0) {
        throw "STM32CubeProgrammer programming failed with exit code $LASTEXITCODE."
    }
} finally {
    if ($null -ne $temporaryProgrammerElf -and
        (Test-Path -LiteralPath $temporaryProgrammerElf -PathType Leaf)) {
        Remove-Item -LiteralPath $temporaryProgrammerElf -Force
    }
}

Write-Host "Starting firmware at 0x08000000..."
& $cli -c "port=$selectedPort" -s 0x08000000
if ($LASTEXITCODE -ne 0) {
    throw "Firmware was programmed and verified, but CubeProgrammer could not start it (exit $LASTEXITCODE)."
}

Write-Host "Firmware programmed, verified, and started successfully."
