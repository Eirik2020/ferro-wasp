[CmdletBinding()]
param(
    [string]$Version = "0.1.0",
    [string]$FirmwareElf,
    [string]$GitCommit
)

$ErrorActionPreference = "Stop"

$configRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$repoRoot = (Resolve-Path (Join-Path $configRoot "..\..")).Path
$distRoot = Join-Path $configRoot "dist"
$releaseName = "ferrowasp-v$Version-windows-x86_64"
$releaseRoot = Join-Path $distRoot $releaseName
$archivePath = "$releaseRoot.zip"
$checksumPath = "$archivePath.sha256"
$app = Join-Path $configRoot "target\release\ferro-configurator.exe"
$thirdParty = Join-Path $configRoot "third_party\dfu-util\windows-x86_64"
$dfu = Join-Path $thirdParty "dfu-util.exe"
$libusb = Join-Path $thirdParty "libusb-1.0.dll"

if ($Version -notmatch '^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$') {
    throw "Version must be a SemVer release or pre-release without a leading v."
}
if (-not $FirmwareElf) {
    $FirmwareElf = Join-Path $repoRoot (
        "apps\foxeer-f405-v2\target\thumbv7em-none-eabihf\" +
        "release\FerroWaspFoxeerF405V2"
    )
}
if (-not $GitCommit) {
    $GitCommit = (& git -C $repoRoot rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0 -or -not $GitCommit) {
        throw "Could not determine the FerroWasp Git commit."
    }
}

$expectedDfuHash = "4C3242657D492BFF4220CE412DBF55DE1564BC3CB1508F0BB2FFD4DECB47FFB7"
$expectedLibusbHash = "3C3C1F47C8040D841A940218874BCF88A47E24F58A115A6BAFF0869D2532FB8E"

foreach ($required in @($app, $dfu, $libusb, $FirmwareElf)) {
    if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
        throw "Required release file is missing: $required"
    }
}
if ((Get-FileHash -LiteralPath $dfu -Algorithm SHA256).Hash -ne $expectedDfuHash) {
    throw "Bundled dfu-util.exe does not match the reviewed official binary."
}
if ((Get-FileHash -LiteralPath $libusb -Algorithm SHA256).Hash -ne $expectedLibusbHash) {
    throw "Bundled libusb-1.0.dll does not match the reviewed official binary."
}

& $app flash $FirmwareElf --board foxeer-f405-v2 --dry-run
if ($LASTEXITCODE -ne 0) {
    throw "The selected Foxeer firmware failed FerroConfigurator ELF validation."
}

New-Item -ItemType Directory -Force -Path $distRoot | Out-Null
$resolvedDist = (Resolve-Path $distRoot).Path
if (Test-Path -LiteralPath $releaseRoot) {
    $resolvedRelease = (Resolve-Path $releaseRoot).Path
    if (-not $resolvedRelease.StartsWith($resolvedDist)) {
        throw "Refusing to replace a release directory outside dist."
    }
    Remove-Item -LiteralPath $resolvedRelease -Recurse -Force
}
if (Test-Path -LiteralPath $archivePath) {
    $resolvedArchive = (Resolve-Path $archivePath).Path
    if (-not $resolvedArchive.StartsWith($resolvedDist)) {
        throw "Refusing to replace an archive outside dist."
    }
    Remove-Item -LiteralPath $resolvedArchive -Force
}
if (Test-Path -LiteralPath $checksumPath) {
    $resolvedChecksum = (Resolve-Path $checksumPath).Path
    if (-not $resolvedChecksum.StartsWith($resolvedDist)) {
        throw "Refusing to replace a checksum outside dist."
    }
    Remove-Item -LiteralPath $resolvedChecksum -Force
}

$licenseRoot = Join-Path $releaseRoot "licenses"
$sourceRoot = Join-Path $releaseRoot "sources"
$firmwareRoot = Join-Path $releaseRoot "firmware"
$boardFirmwareRoot = Join-Path $firmwareRoot "foxeer-f405-v2"
New-Item -ItemType Directory -Force -Path (
    $releaseRoot,
    $licenseRoot,
    $sourceRoot,
    $firmwareRoot,
    $boardFirmwareRoot
) | Out-Null

Copy-Item -LiteralPath $app -Destination $releaseRoot
Copy-Item -LiteralPath $dfu -Destination $releaseRoot
Copy-Item -LiteralPath $libusb -Destination $releaseRoot
Copy-Item -LiteralPath (
    Join-Path $configRoot "packaging\README-WINDOWS.txt"
) -Destination $releaseRoot
Copy-Item -LiteralPath (
    Join-Path $configRoot "THIRD_PARTY_NOTICES.md"
) -Destination $releaseRoot

$packagedFirmware = Join-Path $boardFirmwareRoot "FerroWaspFoxeerF405V2.elf"
Copy-Item -LiteralPath $FirmwareElf -Destination $packagedFirmware
$firmwareHash = (Get-FileHash -LiteralPath $packagedFirmware -Algorithm SHA256).Hash
$firmwareBytes = (Get-Item -LiteralPath $packagedFirmware).Length
$manifest = [ordered]@{
    manifest_version = 1
    release_version = $Version
    git_commit = $GitCommit
    artifacts = @(
        [ordered]@{
            board = "foxeer-f405-v2"
            target = "thumbv7em-none-eabihf"
            features = @(
                "board-foxeer-f405-v2"
            )
            file = "foxeer-f405-v2/FerroWaspFoxeerF405V2.elf"
            sha256 = $firmwareHash
            bytes = $firmwareBytes
        }
    )
}
$manifestJson = $manifest | ConvertTo-Json -Depth 5
$manifestPath = Join-Path $firmwareRoot "manifest.json"
[System.IO.File]::WriteAllText(
    $manifestPath,
    $manifestJson,
    [System.Text.UTF8Encoding]::new($false)
)

$ferroLicense = Join-Path $licenseRoot "FerroWasp"
$dfuLicense = Join-Path $licenseRoot "dfu-util"
$libusbLicense = Join-Path $licenseRoot "libusb"
New-Item -ItemType Directory -Force -Path (
    $ferroLicense,
    $dfuLicense,
    $libusbLicense
) | Out-Null
Copy-Item -LiteralPath (Join-Path $repoRoot "LICENSE") -Destination $ferroLicense
Copy-Item -LiteralPath (Join-Path $repoRoot "NOTICE") -Destination $ferroLicense
Copy-Item -LiteralPath (Join-Path $thirdParty "COPYING") -Destination $dfuLicense
Copy-Item -LiteralPath (Join-Path $thirdParty "AUTHORS") -Destination $dfuLicense
Copy-Item -LiteralPath (
    Join-Path $configRoot "third_party\libusb\COPYING"
) -Destination $libusbLicense
Copy-Item -LiteralPath (
    Join-Path $configRoot "third_party\sources\dfu-util-0.11.tar.gz"
) -Destination $sourceRoot
Copy-Item -LiteralPath (
    Join-Path $configRoot "third_party\sources\libusb-1.0.24.tar.bz2"
) -Destination $sourceRoot

Compress-Archive -Path (Join-Path $releaseRoot "*") `
    -DestinationPath $archivePath -CompressionLevel Optimal
$archiveHash = (
    Get-FileHash -LiteralPath $archivePath -Algorithm SHA256
).Hash.ToLowerInvariant()
$archiveName = Split-Path -Leaf $archivePath
[System.IO.File]::WriteAllText(
    $checksumPath,
    "$archiveHash  $archiveName`n",
    [System.Text.UTF8Encoding]::new($false)
)

Write-Host "Release folder: $releaseRoot"
Write-Host "Release archive: $archivePath"
Write-Host "Release checksum: $checksumPath"
Write-Host "Archive SHA-256: $archiveHash"
Write-Host "Firmware SHA-256: $firmwareHash"
