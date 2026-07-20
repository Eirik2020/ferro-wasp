param(
    [ValidateSet("Auto", "Cable", "Mobile", "Zero", "ZeroMobile")]
    [string]$Link = "Auto",
    [string]$ElfPath = "target-codex-fresh\thumbv7em-none-eabihf\release\FerroWasp",
    [string]$HostUri = "",
    [string]$Token = "",
    [string]$Chip = "",
    [string]$Probe = "",
    [int]$Speed = 0,
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

$elf = Resolve-FerroWaspElf -ElfPath $ElfPath
if (-not (Test-Path -LiteralPath $elf)) {
    throw "ELF not found: $elf. Build first, or pass -ElfPath."
}

Invoke-FerroDebuggerScript -ScriptName "remote_attach.ps1" -DebuggerRepo $DebuggerRepo -Parameters @{ Link = $Link; Elf = $elf }
