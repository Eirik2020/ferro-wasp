[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [ValidatePattern("^COM[0-9]+$")]
    [string]$Port,

    [ValidateRange(1, 4000000)]
    [int]$BaudRate = 115200,

    [ValidateRange(0, 86400)]
    [int]$DurationSeconds = 0
)

$ErrorActionPreference = "Stop"

$serial = [System.IO.Ports.SerialPort]::new(
    $Port,
    $BaudRate,
    [System.IO.Ports.Parity]::None,
    8,
    [System.IO.Ports.StopBits]::One
)
$serial.DtrEnable = $true
$serial.NewLine = "`r`n"
$serial.ReadTimeout = 1000

try {
    $serial.Open()
    if ($DurationSeconds -gt 0) {
        Write-Host "Reading read-only FerroWasp USB diagnostics from $Port for $DurationSeconds seconds."
    } else {
        Write-Host "Reading read-only FerroWasp USB diagnostics from $Port. Press Ctrl+C to stop."
    }

    $elapsed = [System.Diagnostics.Stopwatch]::StartNew()
    while ($DurationSeconds -eq 0 -or $elapsed.Elapsed.TotalSeconds -lt $DurationSeconds) {
        try {
            Write-Output $serial.ReadLine()
        } catch [System.TimeoutException] {
            continue
        }
    }
    $elapsed.Stop()
    Write-Host "USB diagnostic capture completed after $([math]::Round($elapsed.Elapsed.TotalSeconds, 1)) seconds."
} finally {
    if ($serial.IsOpen) {
        $serial.Close()
    }
    $serial.Dispose()
}
