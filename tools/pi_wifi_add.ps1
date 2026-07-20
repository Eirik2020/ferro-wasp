param(
    [ValidateSet("Auto", "Cable", "Mobile", "Zero", "ZeroMobile")]
    [string]$Link = "Auto",
    [string]$PiHost = "",
    [string]$PiUser = "",
    [string]$Ssid = "",
    [securestring]$Password,
    [string]$Interface = "wlan0",
    [int]$Priority = -50,
    [switch]$Recreate,
    [switch]$ConnectNow,
    [string]$DebuggerRepo = ""
)

$ErrorActionPreference = "Stop"
. "$PSScriptRoot\ferrodebugger_bridge.ps1"

Add-IgnoredCompatibilityWarning -Names @(
    $(if ($PiHost) { "-PiHost" }),
    $(if ($PiUser) { "-PiUser" }),
    $(if ($Interface -ne "wlan0") { "-Interface" }),
    $(if ($Priority -ne -50) { "-Priority" }),
    $(if ($Recreate) { "-Recreate" }),
    $(if ($ConnectNow) { "-ConnectNow" })
)

if (-not $Password) {
    if ([string]::IsNullOrWhiteSpace($Ssid)) {
        $Ssid = Get-FerroWaspConfigString -Name "mobile_ssid" -EnvName "FERROWASP_MOBILE_SSID" -Default "your-mobile-router-ssid"
    }
    $Password = Read-Host "Wi-Fi password for $Ssid" -AsSecureString
}

if ([string]::IsNullOrWhiteSpace($Ssid)) {
    $Ssid = Get-FerroWaspConfigString -Name "mobile_ssid" -EnvName "FERROWASP_MOBILE_SSID" -Default "your-mobile-router-ssid"
}

Invoke-FerroDebuggerScript -ScriptName "pi_wifi_add.ps1" -DebuggerRepo $DebuggerRepo -Parameters @{ Link = $Link; Ssid = $Ssid; Password = $Password }
