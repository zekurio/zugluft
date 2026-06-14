param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$')]
    [string] $Version
)

$ErrorActionPreference = "Stop"

$Root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path

function Update-FileText {
    param(
        [Parameter(Mandatory = $true)]
        [string] $Path,

        [Parameter(Mandatory = $true)]
        [scriptblock] $Update
    )

    $text = Get-Content -LiteralPath $Path -Raw
    $updated = & $Update $text
    if ($updated -ne $text) {
        Set-Content -LiteralPath $Path -Value $updated -NoNewline
    }
}

$cargoToml = Join-Path $Root "Cargo.toml"
Update-FileText -Path $cargoToml -Update {
    param($text)
    $text -replace '(?m)^version\s*=\s*"[^"]+"', "version = `"$Version`""
}

$cliToml = Join-Path $Root "crates\zugluft-cli\Cargo.toml"
Update-FileText -Path $cliToml -Update {
    param($text)
    $text -replace 'zugluft-ipc\s*=\s*\{\s*version\s*=\s*"[^"]+"\s*,\s*path\s*=\s*"\.\./zugluft-ipc"\s*\}',
        "zugluft-ipc = { version = `"$Version`", path = `"../zugluft-ipc`" }"
}

$lockPath = Join-Path $Root "Cargo.lock"
$originalLock = Get-Content -LiteralPath $lockPath -Raw
$lock = $originalLock
foreach ($package in @(
    "zugluft-app",
    "zugluft-cli",
    "zugluft-hw",
    "zugluft-ipc",
    "zugluft-service"
)) {
    $pattern = "(?ms)(\[\[package\]\]\r?\nname = `"$([regex]::Escape($package))`"\r?\nversion = `")[^`"]+(`")"
    $lock = [regex]::Replace($lock, $pattern, "`${1}$Version`${2}")
}
if ($lock -ne $originalLock) {
    Set-Content -LiteralPath $lockPath -Value $lock -NoNewline
}

Write-Host "Stamped release version $Version"
