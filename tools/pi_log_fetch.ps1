param(
    [ValidateSet("Auto", "Cable", "Mobile", "Zero", "ZeroMobile")]
    [string]$Link = "Auto",
    [Alias("RemoteFile")]
    [string]$RemoteLog = "",
    [string]$LocalLogDir = "logs\remote_probe",
    [switch]$Latest,
    [string]$PiHost = "",
    [string]$PiUser = "",
    [string]$RemoteLogDir = "",
    [string]$DebuggerRepo = ""
)

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\ferrodebugger_bridge.ps1"

Add-IgnoredCompatibilityWarning -Names @(
    $(if ($Latest) { "-Latest" }),
    $(if ($PiHost) { "-PiHost" }),
    $(if ($PiUser) { "-PiUser" }),
    $(if ($RemoteLogDir) { "-RemoteLogDir" })
)

$forwardParams = @{ Link = $Link; OutDir = (Join-Path (Get-FerroWaspRepoRoot) $LocalLogDir) }
if (-not [string]::IsNullOrWhiteSpace($RemoteLog)) {
    $forwardParams.RemoteFile = $RemoteLog
}

Invoke-FerroDebuggerScript -ScriptName "pi_log_fetch.ps1" -DebuggerRepo $DebuggerRepo -Parameters $forwardParams
