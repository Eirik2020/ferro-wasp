param(
    [ValidateSet("Auto", "Cable", "Mobile", "Zero", "ZeroMobile")]
    [string]$Link = "Auto",
    [string]$Token = "",
    [string]$PiHost = "",
    [string]$PiUser = "",
    [string]$ServiceName = "",
    [string]$ProbeRsPath = "",
    [int]$Port = 0,
    [string]$Address = "",
    [int]$RestartSec = 0,
    [string]$DebuggerRepo = ""
)

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\ferrodebugger_bridge.ps1"

Add-IgnoredCompatibilityWarning -Names @(
    $(if ($PiHost) { "-PiHost" }),
    $(if ($PiUser) { "-PiUser" }),
    $(if ($ServiceName) { "-ServiceName" }),
    $(if ($ProbeRsPath) { "-ProbeRsPath" }),
    $(if ($Port) { "-Port" }),
    $(if ($Address) { "-Address" }),
    $(if ($RestartSec) { "-RestartSec" })
)

$forwardParams = @{ Link = $Link }
if ([string]::IsNullOrWhiteSpace($Token)) {
    $Token = Get-FerroWaspConfigString -Name "probe_token" -EnvName "FERROWASP_PROBE_TOKEN"
}
if (-not [string]::IsNullOrWhiteSpace($Token)) {
    $forwardParams.Token = $Token
}

Invoke-FerroDebuggerScript -ScriptName "install_probe_rs_service.ps1" -DebuggerRepo $DebuggerRepo -Parameters $forwardParams
