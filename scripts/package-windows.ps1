param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$')]
    [string] $Version,

    [string] $OutDir = "dist"
)

$ErrorActionPreference = "Stop"

$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$DistRoot = Join-Path $Root $OutDir
$PackageName = "zugluft-v$Version-windows-x64"
$PackageDir = Join-Path $DistRoot $PackageName
$ReleaseDir = Join-Path $Root "target\release"
$ReleaseBuildDir = Join-Path $ReleaseDir "build"

Set-Location $Root

if (Test-Path -LiteralPath $PackageDir) {
    Remove-Item -LiteralPath $PackageDir -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $DistRoot | Out-Null
New-Item -ItemType Directory -Force -Path $PackageDir | Out-Null

$env:ZUGLUFT_REQUIRE_LHM_BRIDGE = "1"
cargo build --release --locked

foreach ($file in @("zugluft.exe", "zugluft-service.exe", "zugluftctl.exe")) {
    $source = Join-Path $ReleaseDir $file
    if (-not (Test-Path -LiteralPath $source)) {
        throw "expected release binary was not built: $source"
    }
    Copy-Item -LiteralPath $source -Destination $PackageDir
}

$bridge = Get-ChildItem -Path $ReleaseBuildDir -Recurse -Filter "zugluft-lhm-bridge.dll" -File |
    Sort-Object LastWriteTimeUtc -Descending |
    Select-Object -First 1
if (-not $bridge) {
    throw "zugluft-lhm-bridge.dll was not built; check the .NET SDK/NativeAOT output"
}
Copy-Item -LiteralPath $bridge.FullName -Destination $PackageDir

Copy-Item -LiteralPath (Join-Path $Root "README.md") -Destination $PackageDir

@"
zugluft v$Version

Files:
- zugluft.exe: unelevated GUI
- zugluftctl.exe: command-line client and development tool
- zugluft-service.exe: privileged Windows service
- zugluft-lhm-bridge.dll: NativeAOT LibreHardwareMonitor bridge

Install:
1. Unzip this package into a stable directory, for example C:\Program Files\zugluft.
2. From an elevated PowerShell in that directory, run:
   .\zugluft-service.exe install
3. Start the GUI normally:
   .\zugluft.exe

The service registration stores the current zugluft-service.exe path. If you
move the folder later, run install again from the new location.
"@ | Set-Content -LiteralPath (Join-Path $PackageDir "INSTALL.txt") -Encoding utf8

@"
Third-party notices

zugluft uses LibreHardwareMonitorLib through a NativeAOT bridge.
LibreHardwareMonitor is licensed under MPL-2.0:
https://github.com/LibreHardwareMonitor/LibreHardwareMonitor

The bridge is published self-contained; users do not need to install the .NET
runtime for this package.
"@ | Set-Content -LiteralPath (Join-Path $PackageDir "THIRD-PARTY-NOTICES.txt") -Encoding utf8

$ZipPath = Join-Path $DistRoot "$PackageName.zip"
if (Test-Path -LiteralPath $ZipPath) {
    Remove-Item -LiteralPath $ZipPath -Force
}
Compress-Archive -Path (Join-Path $PackageDir "*") -DestinationPath $ZipPath -Force

$Hash = Get-FileHash -Algorithm SHA256 -LiteralPath $ZipPath
$ChecksumPath = "$ZipPath.sha256"
"$($Hash.Hash.ToLowerInvariant())  $(Split-Path $ZipPath -Leaf)" |
    Set-Content -LiteralPath $ChecksumPath -Encoding ascii

Write-Host "Packaged $ZipPath"
Write-Host "SHA256  $($Hash.Hash.ToLowerInvariant())"

if ($env:GITHUB_OUTPUT) {
    $ResolvedZip = (Resolve-Path $ZipPath).Path
    $ResolvedChecksum = (Resolve-Path $ChecksumPath).Path
    "package_name=$PackageName" | Out-File -FilePath $env:GITHUB_OUTPUT -Append -Encoding utf8
    "zip_path=$ResolvedZip" | Out-File -FilePath $env:GITHUB_OUTPUT -Append -Encoding utf8
    "checksum_path=$ResolvedChecksum" | Out-File -FilePath $env:GITHUB_OUTPUT -Append -Encoding utf8
}
