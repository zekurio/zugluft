param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$')]
    [string] $Version,

    [ValidatePattern('^[A-Za-z0-9_-]+$')]
    [string] $Profile = "release",

    [switch] $CargoTimings,

    [string] $OutDir = "dist"
)

$ErrorActionPreference = "Stop"

function Invoke-Native {
    param(
        [Parameter(Mandatory = $true)]
        [string] $FilePath,

        [Parameter(ValueFromRemainingArguments = $true)]
        [string[]] $Arguments
    )

    & $FilePath @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$FilePath failed with exit code $LASTEXITCODE"
    }
}

function Get-MakeNsis {
    $existing = Get-Command makensis.exe -ErrorAction SilentlyContinue
    if ($existing) {
        return $existing.Source
    }

    $version = "3.11"
    $toolsDir = Join-Path $Root "target\tools"
    $nsisDir = Join-Path $toolsDir "nsis-$version"
    $makensis = Join-Path $nsisDir "makensis.exe"
    if (Test-Path -LiteralPath $makensis) {
        return $makensis
    }

    New-Item -ItemType Directory -Force -Path $toolsDir | Out-Null
    $archive = Join-Path $toolsDir "nsis-$version.zip"
    $url = "https://sourceforge.net/projects/nsis/files/NSIS%203/$version/nsis-$version.zip/download"

    Write-Host "Downloading NSIS $version..."
    $curl = Get-Command curl.exe -ErrorAction SilentlyContinue
    if ($curl) {
        Invoke-Native $curl.Source -L --fail --silent --show-error --output $archive $url
    } else {
        Invoke-WebRequest -Uri $url -OutFile $archive
    }

    $signature = [byte[]]::new(2)
    $stream = [System.IO.File]::OpenRead($archive)
    try {
        $read = $stream.Read($signature, 0, 2)
    } finally {
        $stream.Dispose()
    }
    if ($read -ne 2 -or $signature[0] -ne 0x50 -or $signature[1] -ne 0x4b) {
        throw "Downloaded NSIS archive is not a ZIP file: $archive"
    }

    if (Test-Path -LiteralPath $nsisDir) {
        Remove-Item -LiteralPath $nsisDir -Recurse -Force
    }
    Expand-Archive -LiteralPath $archive -DestinationPath $toolsDir -Force

    if (-not (Test-Path -LiteralPath $makensis)) {
        throw "NSIS compiler was not found after extracting $archive"
    }
    return $makensis
}

$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$DistRoot = Join-Path $Root $OutDir
$PayloadName = "zugluft-v$Version-windows-x64"
$PayloadDir = Join-Path $DistRoot $PayloadName
$InstallerName = "zugluft-setup-v$Version-windows-x64.exe"
$InstallerPath = Join-Path $DistRoot $InstallerName
$ReleaseDir = Join-Path $Root "target\$Profile"
$ReleaseBuildDir = Join-Path $ReleaseDir "build"
$BridgeProject = Join-Path $Root "crates\zugluft-hw\lhm-bridge\zugluft-lhm-bridge.csproj"
$BridgeTargetDir = Join-Path $Root "target\lhm-bridge\$Profile"
$BridgePublishDir = Join-Path $Root "target\lhm-bridge\$Profile\publish"
$BridgeIntermediateDir = Join-Path $BridgeTargetDir "obj"
$NsisScript = Join-Path $Root "installer\zugluft.nsi"
$IconPath = Join-Path $Root "crates\zugluft-app\assets\app-icon.ico"

function Find-LhmBridgeDll {
    foreach ($candidate in @(
        (Join-Path (Join-Path $BridgePublishDir "native") "zugluft-lhm-bridge.dll"),
        (Join-Path $BridgePublishDir "zugluft-lhm-bridge.dll")
    )) {
        $bridge = Get-Item -LiteralPath $candidate -ErrorAction SilentlyContinue
        if ($bridge) {
            return $bridge
        }
    }

    Get-ChildItem -Path $ReleaseBuildDir -Recurse -Filter "zugluft-lhm-bridge.dll" -File -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTimeUtc -Descending |
        Select-Object -First 1
}

function Publish-LhmBridge {
    $dotnet = Get-Command dotnet -ErrorAction SilentlyContinue
    if (-not $dotnet) {
        throw ".NET SDK is required to publish the LibreHardwareMonitor bridge"
    }

    if (-not $env:NUGET_PACKAGES) {
        $env:NUGET_PACKAGES = Join-Path $BridgeTargetDir "nuget"
    }

    Write-Host "Publishing LibreHardwareMonitor bridge..."
    Invoke-Native $dotnet.Source `
        "publish" `
        $BridgeProject `
        "-c" `
        "Release" `
        "-r" `
        "win-x64" `
        "-o" `
        $BridgePublishDir `
        "/p:NativeLib=Shared" `
        "/p:OutputPath=$BridgePublishDir/" `
        "/p:BaseIntermediateOutputPath=$BridgeIntermediateDir/"
}

Set-Location $Root

if (Test-Path -LiteralPath $PayloadDir) {
    Remove-Item -LiteralPath $PayloadDir -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $DistRoot | Out-Null
New-Item -ItemType Directory -Force -Path $PayloadDir | Out-Null

$env:ZUGLUFT_REQUIRE_LHM_BRIDGE = "1"
$env:ZUGLUFT_LHM_BRIDGE_PROFILE = $Profile
$cargoArgs = @("build", "--locked", "--profile", $Profile)
if ($CargoTimings) {
    $cargoArgs += "--timings"
}
Invoke-Native cargo @cargoArgs

foreach ($file in @("zugluft.exe", "zugluft-service.exe", "zugluftctl.exe")) {
    $source = Join-Path $ReleaseDir $file
    if (-not (Test-Path -LiteralPath $source)) {
        throw "expected release binary was not built: $source"
    }
    Copy-Item -LiteralPath $source -Destination $PayloadDir
}

$bridge = Find-LhmBridgeDll
if (-not $bridge) {
    Publish-LhmBridge
    $bridge = Find-LhmBridgeDll
}
if (-not $bridge) {
    throw "zugluft-lhm-bridge.dll was not built; check the .NET SDK/NativeAOT output"
}
Copy-Item -LiteralPath $bridge.FullName -Destination $PayloadDir

Copy-Item -LiteralPath (Join-Path $Root "README.md") -Destination $PayloadDir

@"
Third-party notices

zugluft uses LibreHardwareMonitorLib through a NativeAOT bridge.
LibreHardwareMonitor is licensed under MPL-2.0:
https://github.com/LibreHardwareMonitor/LibreHardwareMonitor

The zugluft installer can launch the separately distributed PawnIO driver
installer from the official winget package or GitHub release:
https://github.com/namazso/PawnIO.Setup/releases

The bridge is published self-contained; users do not need to install the .NET
runtime for this package.
"@ | Set-Content -LiteralPath (Join-Path $PayloadDir "THIRD-PARTY-NOTICES.txt") -Encoding utf8

$makensis = Get-MakeNsis

if (Test-Path -LiteralPath $InstallerPath) {
    Remove-Item -LiteralPath $InstallerPath -Force
}

Invoke-Native $makensis `
    "/DVERSION=$Version" `
    "/DPAYLOAD_DIR=$PayloadDir" `
    "/DOUT_FILE=$InstallerPath" `
    "/DICON_FILE=$IconPath" `
    $NsisScript

if (-not (Test-Path -LiteralPath $InstallerPath)) {
    throw "NSIS did not produce the expected installer: $InstallerPath"
}

$Hash = Get-FileHash -Algorithm SHA256 -LiteralPath $InstallerPath
$ChecksumPath = "$InstallerPath.sha256"
"$($Hash.Hash.ToLowerInvariant())  $(Split-Path $InstallerPath -Leaf)" |
    Set-Content -LiteralPath $ChecksumPath -Encoding ascii

Write-Host "Packaged $InstallerPath"
Write-Host "SHA256  $($Hash.Hash.ToLowerInvariant())"

if ($env:GITHUB_OUTPUT) {
    $ResolvedInstaller = (Resolve-Path $InstallerPath).Path
    $ResolvedChecksum = (Resolve-Path $ChecksumPath).Path
    "artifact_name=$InstallerName" | Out-File -FilePath $env:GITHUB_OUTPUT -Append -Encoding utf8
    "installer_path=$ResolvedInstaller" | Out-File -FilePath $env:GITHUB_OUTPUT -Append -Encoding utf8
    "checksum_path=$ResolvedChecksum" | Out-File -FilePath $env:GITHUB_OUTPUT -Append -Encoding utf8
}
