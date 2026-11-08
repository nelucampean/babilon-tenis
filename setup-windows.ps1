<#
.SYNOPSIS
  One-shot Windows setup for Babilon Tennis: installs Rust + the C++ linker,
  builds the release exe, stages exe + assets, and launches the game.

.DESCRIPTION
  Downloads (needs internet in the VM, ~6 GB free disk):
    - rustup-init.exe  (~30 MB)  -> Rust stable toolchain
    - vs_BuildTools.exe (small bootstrapper, installs ~2-4 GB of C++ tools)
    - Rust crates on first build (~500 MB, cached afterwards)

.USAGE
  From this folder (the one containing Cargo.toml) in PowerShell:
    powershell -ExecutionPolicy Bypass -File .\setup-windows.ps1
    powershell -ExecutionPolicy Bypass -File .\setup-windows.ps1 -NoRun   # build only

  Idempotent: safe to re-run; already-installed parts are skipped.
#>
param([switch]$NoRun)
$ErrorActionPreference = 'Stop'

function Write-Step($msg) { Write-Host "`n=== $msg ===" -ForegroundColor Cyan }

# --- 0. Must run from the workspace root -------------------------------------
$Root = $PSScriptRoot
if (-not (Test-Path (Join-Path $Root 'Cargo.toml'))) {
    throw "Run this script from the Babilon-Tennis-Rust folder (Cargo.toml not found in $Root)."
}
if (-not (Test-Path (Join-Path $Root 'babilon-desktop\assets\sunete'))) {
    throw "Game assets missing: babilon-desktop\assets\sunete not found in $Root."
}
Set-Location $Root

$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Write-Host 'NOTE: not running as Administrator - accept the UAC prompt when the C++ Build Tools installer asks.' -ForegroundColor Yellow
}

# --- 1. Rust toolchain ---------------------------------------------------------
Write-Step 'Rust toolchain'
$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
if (Get-Command cargo -ErrorAction SilentlyContinue) {
    Write-Host ('cargo already installed: ' + (cargo --version))
} else {
    Write-Host 'Downloading rustup-init...'
    $rustupExe = Join-Path $env:TEMP 'rustup-init.exe'
    Invoke-WebRequest -Uri 'https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe' -OutFile $rustupExe
    Write-Host 'Installing Rust stable (this downloads the toolchain, ~300 MB)...'
    & $rustupExe -y --default-toolchain stable --profile default | Out-Null
    Remove-Item $rustupExe -ErrorAction SilentlyContinue
}
# Make cargo usable in THIS session (installer only updates future shells).
if ($env:Path -notlike "*$cargoBin*") { $env:Path = "$cargoBin;$env:Path" }
cargo --version

# --- 2. C++ linker (link.exe) for the MSVC toolchain ---------------------------
Write-Step 'C++ build tools (linker)'
$linkFound = $false
if (Get-Command link.exe -ErrorAction SilentlyContinue) { $linkFound = $true }
if (-not $linkFound) {
    $vsLink = Get-ChildItem "${env:ProgramFiles(x86)}\Microsoft Visual Studio\*\BuildTools\VC\Tools\MSVC\*\bin\Hostx64\x64\link.exe" -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if ($vsLink) { $linkFound = $true }
}
if ($linkFound) {
    Write-Host 'link.exe already available, skipping Build Tools install.'
} else {
    Write-Host 'Downloading Visual Studio Build Tools bootstrapper...'
    $vsExe = Join-Path $env:TEMP 'vs_BuildTools.exe'
    Invoke-WebRequest -Uri 'https://aka.ms/vs/17/release/vs_BuildTools.exe' -OutFile $vsExe
    Write-Host 'Installing C++ workload (quiet, ~10-20 min, accept the UAC prompt)...'
    $proc = Start-Process -FilePath $vsExe -ArgumentList @(
        '--passive', '--wait', '--norestart',
        '--add', 'Microsoft.VisualStudio.Workload.VCTools',
        '--includeRecommended'
    ) -PassThru -Wait
    Remove-Item $vsExe -ErrorAction SilentlyContinue
    if ($proc.ExitCode -ne 0 -and $proc.ExitCode -ne 3010) {
        throw "Build Tools installer exited with code $($proc.ExitCode). Re-run the script; it resumes where it left off."
    }
    if ($proc.ExitCode -eq 3010) {
        Write-Host 'Installer requests a reboot (3010). Reboot, then re-run this script.' -ForegroundColor Yellow
        exit 3010
    }
}

# --- 3. Build ------------------------------------------------------------------
Write-Step 'Building release (first build downloads crates, ~5-15 min in a VM)'
cargo build --release -p babilon-desktop
$exe = Join-Path $Root 'target\release\babilon-desktop.exe'
if (-not (Test-Path $exe)) { throw "Build finished but $exe was not produced." }

# --- 4. Stage a portable folder --------------------------------------------------
Write-Step 'Staging portable folder'
$dist = Join-Path $Root 'dist-windows'
if (Test-Path $dist) { Remove-Item $dist -Recurse -Force }
New-Item -ItemType Directory $dist | Out-Null
Copy-Item $exe (Join-Path $dist 'BabilonTennis.exe')
Copy-Item (Join-Path $Root 'babilon-desktop\assets') (Join-Path $dist 'assets') -Recurse
Copy-Item (Join-Path $Root 'README.md') $dist -ErrorAction SilentlyContinue
Write-Host "Portable game ready in: $dist"
$sizeMB = [math]::Round(((Get-ChildItem $dist -Recurse | Measure-Object Length -Sum).Sum) / 1MB, 1)
Write-Host "Total size: $sizeMB MB (mostly umpire MP3s + fonts)."

# --- 5. Run ----------------------------------------------------------------------
if ($NoRun) {
    Write-Host 'Build only (-NoRun). Launch dist-windows\BabilonTennis.exe to play.'
    exit 0
}
Write-Step 'Launching (S = serve, Space = hit, arrows = lanes, T = sound test)'
Write-Host 'Close the game window when done; your console output stays above.'
& (Join-Path $dist 'BabilonTennis.exe')
