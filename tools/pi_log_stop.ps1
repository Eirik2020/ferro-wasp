param(
    [ValidateSet("Auto", "Cable", "Mobile", "Zero", "ZeroMobile")]
    [string]$Link = "Auto",
    [string]$PiHost = "",
    [string]$PiUser = "",
    [string]$RemoteLogDir = "",
    [string]$DebuggerRepo = ""
)

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\ferrodebugger_bridge.ps1"

Add-IgnoredCompatibilityWarning -Names @(
    $(if ($PiHost) { "-PiHost" }),
    $(if ($PiUser) { "-PiUser" }),
    $(if ($RemoteLogDir) { "-RemoteLogDir" })
)

Invoke-FerroDebuggerScript -ScriptName "pi_log_stop.ps1" -DebuggerRepo $DebuggerRepo -Parameters @{ Link = $Link }
