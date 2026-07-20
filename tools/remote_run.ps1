param(
    [switch]$Build,
    [string]$TargetDir = "target-codex-fresh",
    [ValidateSet("Auto", "Cable", "Mobile", "Zero", "ZeroMobile")]
    [string]$Link = "Auto",
    [string]$ElfPath = "",
    [string]$Features = "",
    [switch]$DryRun,
    [string]$HostUri = "",
    [string]$Token = "",
    [string]$Chip = "",
    [string]$Probe = "",
    [int]$Speed = 0,
    [switch]$ConnectUnderReset,
    [string]$DebuggerRepo = ""
)

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\ferrodebugger_bridge.ps1"

Add-IgnoredCompatibilityWarning -Names @(
    $(if ($HostUri) { "-HostUri" }),
    $(if ($Token) { "-Token" }),
    $(if ($Chip) { "-Chip" }),
    $(if ($Probe) { "-Probe" }),
    $(if ($Speed) { "-Speed" })
)

if ($Build) {
    Invoke-FerroWaspBuild -TargetDir $TargetDir -Features $Features
}

$elf = Resolve-FerroWaspElf -ElfPath $ElfPath -TargetDir $TargetDir
if (-not (Test-Path -LiteralPath $elf)) {
    throw "ELF not found: $elf. Run with -Build, or pass -ElfPath."
}

Write-Host "Bench safety: props off; keep ESC power in the intended safe state."
$forwardParams = @{ Link = $Link; Elf = $elf }
if ($DryRun) {
    $forwardParams.DryRun = $true
}
if ($ConnectUnderReset) {
    $forwardParams.ConnectUnderReset = $true
}

Invoke-FerroDebuggerScript -ScriptName "remote_run.ps1" -DebuggerRepo $DebuggerRepo -Parameters $forwardParams
