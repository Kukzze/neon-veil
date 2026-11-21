<#
.SYNOPSIS
    Builds, packages and pushes Neon Veil to itch.io with butler.

.EXAMPLE
    .\tools\deploy\deploy_itch.ps1 -VersionTag 1.0.0
    .\tools\deploy\deploy_itch.ps1 -VersionTag 1.0.0 -SkipBuild -DryRun

.NOTES
    Run from the repository root. Requires butler on PATH and a prior
    `butler login` (or BUTLER_API_KEY in the environment), except with -DryRun.
#>
[CmdletBinding()]
param (
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^v?\d+\.\d+\.\d+([-+][0-9A-Za-z.-]+)?$')]
    [string]$VersionTag,

    [string]$TargetPlatform = "windows-x64",

    [string]$Project = "cutting-onion-studios/neon-veil",

    # Directory holding the built game_client.exe.
    [string]$BuildDir = ".\target\release",

    # Use the existing build instead of running cargo.
    [switch]$SkipBuild,

    # Package only; do not call butler.
    [switch]$DryRun
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$Channel = "${Project}:$TargetPlatform"
$PackageDir = Join-Path ".\target\itch_dist" $TargetPlatform

Write-Host "Neon Veil // itch.io deployment"
Write-Host "  channel : $Channel"
Write-Host "  version : $VersionTag"

if (-not (Test-Path ".\Cargo.toml") -or -not (Test-Path ".\assets")) {
    throw "Run this script from the repository root (Cargo.toml and assets\ not found)."
}

if (-not $DryRun -and -not (Get-Command butler -ErrorAction SilentlyContinue)) {
    throw "butler was not found on PATH. Install it from https://itch.io/docs/butler/ and run 'butler login'."
}

if (-not $SkipBuild) {
    Write-Host "[1/3] Building release client..."
    cargo build --release --locked --bin game_client
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed with exit code $LASTEXITCODE" }
}

$Binary = Join-Path $BuildDir "game_client.exe"
if (-not (Test-Path $Binary)) {
    throw "Client binary not found at $Binary. Build first or pass -BuildDir."
}

Write-Host "[2/3] Packaging into $PackageDir..."
if (Test-Path $PackageDir) { Remove-Item -Recurse -Force $PackageDir }
New-Item -ItemType Directory -Force -Path $PackageDir | Out-Null
Copy-Item $Binary -Destination (Join-Path $PackageDir "NeonVeil.exe")
Copy-Item -Recurse ".\assets" -Destination (Join-Path $PackageDir "assets")

$Files = Get-ChildItem -Recurse -File $PackageDir
$SizeMb = [math]::Round((($Files | Measure-Object -Property Length -Sum).Sum / 1MB), 2)
Write-Host "  packaged $($Files.Count) files, $SizeMb MB"

if ($DryRun) {
    Write-Host "[3/3] Dry run: skipping butler push."
    Write-Host "  would run: butler push $PackageDir $Channel --userversion $VersionTag"
    exit 0
}

Write-Host "[3/3] Pushing with butler..."
butler push $PackageDir $Channel --userversion $VersionTag
if ($LASTEXITCODE -ne 0) { throw "butler push failed with exit code $LASTEXITCODE" }

Write-Host "Deployment to $Channel ($VersionTag) complete."
