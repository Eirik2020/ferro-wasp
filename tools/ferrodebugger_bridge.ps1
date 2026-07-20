Set-StrictMode -Version Latest

function Get-FerroWaspRepoRoot {
    return (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
}

function Get-FerroWaspLocalConfig {
    $configPath = Join-Path (Get-FerroWaspRepoRoot) "local_config\ferrowasp.local.json"
    if (-not (Test-Path -LiteralPath $configPath)) {
        return $null
    }

    return Get-Content -LiteralPath $configPath -Raw | ConvertFrom-Json
}

function Get-FerroWaspConfigString {
    param(
        [Parameter(Mandatory=$true)][string]$Name,
        [string]$EnvName = "",
        [string]$Default = ""
    )

    if (-not [string]::IsNullOrWhiteSpace($EnvName)) {
        $envValue = [Environment]::GetEnvironmentVariable($EnvName)
        if (-not [string]::IsNullOrWhiteSpace($envValue)) {
            return $envValue
        }
    }

    $config = Get-FerroWaspLocalConfig
    if ($null -ne $config) {
        $property = $config.PSObject.Properties[$Name]
        if (($null -ne $property) -and -not [string]::IsNullOrWhiteSpace([string]$property.Value)) {
            return [string]$property.Value
        }
    }

    return $Default
}

function Resolve-FerroDebuggerRepo {
    param([string]$DebuggerRepo = "")

    if (-not [string]::IsNullOrWhiteSpace($DebuggerRepo)) {
        return (Resolve-Path $DebuggerRepo).Path
    }

    if ($env:FERRODEBUGGER_REPO) {
        return (Resolve-Path $env:FERRODEBUGGER_REPO).Path
    }

    return (Resolve-Path (Join-Path (Get-FerroWaspRepoRoot) "..\ferro-debugger")).Path
}

function Resolve-FerroWaspElf {
    param(
        [string]$ElfPath = "",
        [string]$TargetDir = "target-codex-fresh"
    )

    $repoRoot = Get-FerroWaspRepoRoot
    if ([string]::IsNullOrWhiteSpace($ElfPath)) {
        $ElfPath = Join-Path $TargetDir "thumbv7em-none-eabihf\release\FerroWasp"
    }

    if (-not [System.IO.Path]::IsPathRooted($ElfPath)) {
        $ElfPath = Join-Path $repoRoot $ElfPath
    }

    return $ElfPath
}

function Invoke-FerroDebuggerScript {
    param(
        [Parameter(Mandatory=$true)][string]$ScriptName,
        [string]$DebuggerRepo = "",
        [hashtable]$Parameters = @{}
    )

    $debuggerRoot = Resolve-FerroDebuggerRepo -DebuggerRepo $DebuggerRepo
    $script = Join-Path $debuggerRoot "scripts\$ScriptName"
    if (-not (Test-Path -LiteralPath $script)) {
        throw "FerroDebugger script not found: $script"
    }

    & $script @Parameters
}

function Add-IgnoredCompatibilityWarning {
    param([string[]]$Names)

    $presentNames = @($Names | Where-Object { -not [string]::IsNullOrWhiteSpace($_) })
    if ($presentNames.Count -gt 0) {
        Write-Warning "These legacy FerroWasp parameters are now configured in FerroDebugger and were ignored: $($presentNames -join ', ')"
    }
}

function Invoke-FerroWaspBuild {
    param(
        [string]$TargetDir = "target-codex-fresh",
        [string]$Features = ""
    )

    $repoRoot = Get-FerroWaspRepoRoot
    $appRoot = Join-Path $repoRoot "apps\stm32f405-flight"
    $resolvedTargetDir = if ([System.IO.Path]::IsPathRooted($TargetDir)) {
        $TargetDir
    } else {
        Join-Path $repoRoot $TargetDir
    }
    $previousTargetDir = $env:CARGO_TARGET_DIR

    Push-Location $appRoot
    try {
        Write-Host "Building release firmware into $TargetDir"
        $env:CARGO_TARGET_DIR = $resolvedTargetDir
        $cargoArgs = @("build", "--release")
        if (-not [string]::IsNullOrWhiteSpace($Features)) {
            $cargoArgs += @("--features", $Features)
        }
        cargo @cargoArgs
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build failed with exit code $LASTEXITCODE"
        }
    } finally {
        if ($null -eq $previousTargetDir) {
            Remove-Item Env:CARGO_TARGET_DIR -ErrorAction SilentlyContinue
        } else {
            $env:CARGO_TARGET_DIR = $previousTargetDir
        }
        Pop-Location
    }
}
