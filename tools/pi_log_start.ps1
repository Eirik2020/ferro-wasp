param(
    [ValidateSet("Auto", "Cable", "Mobile", "Zero", "ZeroMobile")]
    [string]$Link = "Auto",
    [switch]$Restart,
    [string]$PiHost = "",
    [string]$PiUser = "",
    [string]$HostUri = "",
    [string]$Token = "",
    [string]$RemoteElf = "",
    [string]$RemoteLogDir = "",
    [string]$Chip = "",
    [string]$Probe = "",
    [int]$Speed = 0,
    [string]$DebuggerRepo = ""
)

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\ferrodebugger_bridge.ps1"

Add-IgnoredCompatibilityWarning -Names @(
    $(if ($PiHost) { "-PiHost" }),
    $(if ($PiUser) { "-PiUser" }),
    $(if ($HostUri) { "-HostUri" }),
    $(if ($Token) { "-Token" }),
    $(if ($RemoteElf) { "-RemoteElf" }),
    $(if ($RemoteLogDir) { "-RemoteLogDir" }),
    $(if ($Chip) { "-Chip" }),
    $(if ($Probe) { "-Probe" }),
    $(if ($Speed) { "-Speed" })
)

$forwardParams = @{ Link = $Link }
if ($Restart) {
    $forwardParams.Restart = $true
}

Invoke-FerroDebuggerScript -ScriptName "pi_log_start.ps1" -DebuggerRepo $DebuggerRepo -Parameters $forwardParams
