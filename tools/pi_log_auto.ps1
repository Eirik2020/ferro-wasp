param(
    [ValidateSet("Auto", "Cable", "Mobile", "Zero", "ZeroMobile")]
    [string]$Link = "Auto",
    [switch]$Restart,
    [switch]$Stop,
    [double]$PreRoll = 3.0,
    [double]$PostRoll = 5.0,
    [string]$DebuggerRepo = ""
)

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\ferrodebugger_bridge.ps1"

$forwardParams = @{
    Link = $Link
    PreRoll = $PreRoll
    PostRoll = $PostRoll
}
if ($Restart) {
    $forwardParams.Restart = $true
}
if ($Stop) {
    $forwardParams.Stop = $true
}

Invoke-FerroDebuggerScript -ScriptName "pi_log_auto.ps1" -DebuggerRepo $DebuggerRepo -Parameters $forwardParams
