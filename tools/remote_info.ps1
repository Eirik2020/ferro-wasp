param(
    [ValidateSet("Auto", "Cable", "Mobile", "Zero", "ZeroMobile")]
    [string]$Link = "Auto",
    [switch]$VerboseProbe,
    [string]$HostUri = "",
    [string]$Token = "",
    [string]$Probe = "",
    [int]$Speed = 0,
    [string]$DebuggerRepo = ""
)

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\ferrodebugger_bridge.ps1"

Add-IgnoredCompatibilityWarning -Names @(
    $(if ($HostUri) { "-HostUri" }),
    $(if ($Token) { "-Token" }),
    $(if ($Probe) { "-Probe" }),
    $(if ($Speed) { "-Speed" })
)

$forwardParams = @{ Link = $Link }
if ($VerboseProbe) {
    $forwardParams.VerboseProbe = $true
}

Invoke-FerroDebuggerScript -ScriptName "remote_info.ps1" -DebuggerRepo $DebuggerRepo -Parameters $forwardParams
