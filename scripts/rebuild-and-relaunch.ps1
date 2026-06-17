param(
    [string] $ServiceName = "zugluft",
    [string] $Profile = "release",
    [switch] $SkipStatus
)

$ErrorActionPreference = "Stop"

$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$TargetDir = Join-Path $Root "target\$Profile"
$GuiExe = Join-Path $TargetDir "zugluft.exe"
$CtlExe = Join-Path $TargetDir "zugluftctl.exe"

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

function Invoke-ElevatedServiceAction {
    param(
        [Parameter(Mandatory = $true)]
        [ValidateSet("Start", "Stop", "Restart")]
        [string] $Action
    )

    $helper = Join-Path ([System.IO.Path]::GetTempPath()) "zugluft-service-$($Action.ToLowerInvariant())-$PID.ps1"
    @'
param(
    [Parameter(Mandatory = $true)]
    [string] $Name,

    [Parameter(Mandatory = $true)]
    [ValidateSet("Start", "Stop", "Restart")]
    [string] $Action
)

$ErrorActionPreference = "Stop"

if ($Action -eq "Stop") {
    Stop-Service -Name $Name -Force
    $target = "Stopped"
} elseif ($Action -eq "Start") {
    Start-Service -Name $Name
    $target = "Running"
} else {
    Restart-Service -Name $Name -Force
    $target = "Running"
}

for ($i = 0; $i -lt 30; $i++) {
    $service = Get-Service -Name $Name
    if ($service.Status.ToString() -eq $target) {
        exit 0
    }
    Start-Sleep -Seconds 1
}

throw "Service '$Name' did not reach $target state"
'@ | Set-Content -LiteralPath $helper -Encoding UTF8

    try {
        $process = Start-Process `
            -FilePath powershell.exe `
            -Verb RunAs `
            -WindowStyle Hidden `
            -Wait `
            -PassThru `
            -ArgumentList @(
                "-NoProfile",
                "-ExecutionPolicy", "Bypass",
                "-File", "`"$helper`"",
                "-Name", "`"$ServiceName`"",
                "-Action", $Action
            )

        if ($process.ExitCode -ne 0) {
            throw "Elevated service $Action failed with exit code $($process.ExitCode)"
        }
    } finally {
        Remove-Item -LiteralPath $helper -Force -ErrorAction SilentlyContinue
    }
}

function Stop-Gui {
    Get-Process -Name "zugluft" -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -ne $null } |
        Stop-Process -Force
}

Set-Location $Root

Write-Host "Closing zugluft GUI..."
Stop-Gui

$serviceStopped = $false
try {
    Write-Host "Stopping $ServiceName service (UAC may prompt)..."
    Invoke-ElevatedServiceAction -Action Stop
    $serviceStopped = $true

    Write-Host "Building cargo profile '$Profile'..."
    Invoke-Native cargo build "--$Profile"

    Write-Host "Starting $ServiceName service (UAC may prompt)..."
    Invoke-ElevatedServiceAction -Action Start
    $serviceStopped = $false

    if (-not (Test-Path -LiteralPath $GuiExe)) {
        throw "GUI executable not found: $GuiExe"
    }

    Write-Host "Launching GUI..."
    Start-Process -FilePath $GuiExe -WorkingDirectory $TargetDir

    if (-not $SkipStatus -and (Test-Path -LiteralPath $CtlExe)) {
        Write-Host "Checking service status..."
        & $CtlExe status
        if ($LASTEXITCODE -ne 0) {
            throw "$CtlExe status failed with exit code $LASTEXITCODE"
        }
    }

    Write-Host "Done."
} catch {
    if ($serviceStopped) {
        Write-Warning "Build/relaunch failed after the service was stopped. Trying to start it again..."
        try {
            Invoke-ElevatedServiceAction -Action Start
        } catch {
            Write-Warning "Could not restart $ServiceName service: $_"
        }
    }
    throw
}
