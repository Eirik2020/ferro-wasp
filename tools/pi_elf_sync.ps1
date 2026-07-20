param(
    [ValidateSet("Auto", "Cable", "Mobile", "Zero", "ZeroMobile")]
    [string]$Link = "Auto",
    [string]$ElfPath = "target-codex-fresh\thumbv7em-none-eabihf\release\FerroWasp",
    [string]$PiHost = "",
    [string]$PiUser = "",
    [string]$RemoteElf = "",
    [string]$DebuggerRepo = ""
)

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\ferrodebugger_bridge.ps1"

Add-IgnoredCompatibilityWarning -Names @(
    $(if ($PiHost) { "-PiHost" }),
    $(if ($PiUser) { "-PiUser" }),
    $(if ($RemoteElf) { "-RemoteElf" })
)

$elf = Resolve-FerroWaspElf -ElfPath $ElfPath
if (-not (Test-Path -LiteralPath $elf)) {
    throw "ELF not found: $elf. Build first, or pass -ElfPath."
}

Invoke-FerroDebuggerScript -ScriptName "pi_elf_sync.ps1" -DebuggerRepo $DebuggerRepo -Parameters @{ Link = $Link; Elf = $elf }
