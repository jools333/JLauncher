<#
.SYNOPSIS
Builds the Windows NSIS Installer for JLauncher
#>
param(
    [string]$Version,
    [string]$BinaryPath = "target\x86_64-pc-windows-msvc\release\jlauncher.exe"
)

$ErrorActionPreference = "Stop"

Write-Host "=== Building JLauncher Windows Installer ===" -ForegroundColor Cyan

# 1. Determine Version
if (-not $Version) {
    if (Test-Path "Cargo.toml") {
        $content = Get-Content "Cargo.toml" -Raw
        if ($content -match 'version\s*=\s*"([^"]+)"') {
            $Version = $matches[1]
        }
    }
    if (-not $Version) {
        $Version = "0.2.8"
    }
}
Write-Host "Installer Version: $Version" -ForegroundColor Green

# 2. Locate or Copy Executable
if (Test-Path $BinaryPath) {
    Copy-Item $BinaryPath -Destination ".\jlauncher.exe" -Force
} elseif (Test-Path "target\release\jlauncher.exe") {
    Copy-Item "target\release\jlauncher.exe" -Destination ".\jlauncher.exe" -Force
} elseif (-not (Test-Path ".\jlauncher.exe")) {
    Write-Error "Could not find jlauncher.exe at '$BinaryPath', 'target\release\jlauncher.exe', or in root directory!"
}

# 3. Ensure icon assets exist
if (-not (Test-Path "assets\icon.ico")) {
    Write-Host "Generating icon assets..." -ForegroundColor Yellow
    if (Test-Path "scripts\generate-icon.ps1") {
        Get-Content "scripts\generate-icon.ps1" -Raw | Invoke-Expression
    }
}

# 4. Find NSIS Compiler (makensis.exe)
$nsisCandidates = @(
    "makensis",
    "C:\Program Files (x86)\NSIS\makensis.exe",
    "C:\Program Files\NSIS\makensis.exe",
    "$env:LOCALAPPDATA\Programs\NSIS\makensis.exe"
)

$makensis = $null
foreach ($c in $nsisCandidates) {
    $cmd = Get-Command $c -ErrorAction SilentlyContinue
    if ($cmd) {
        $makensis = $cmd.Source
        break
    } elseif (Test-Path $c) {
        $makensis = $c
        break
    }
}

if (-not $makensis) {
    # If in GitHub Actions or winget is present, attempt install
    if ($env:GITHUB_ACTIONS -eq "true") {
        Write-Host "Installing NSIS in GitHub Actions runner..." -ForegroundColor Yellow
        choco install nsis -y
        $makensis = "C:\Program Files (x86)\NSIS\makensis.exe"
    } else {
        Write-Error "NSIS compiler (makensis.exe) was not found! Please install NSIS (e.g., 'winget install NSIS.NSIS' or 'choco install nsis')."
    }
}

Write-Host "Using NSIS compiler: $makensis" -ForegroundColor Green

# 5. Ensure output directory
$outDir = "target\installer"
if (-not (Test-Path $outDir)) {
    New-Item -ItemType Directory -Path $outDir -Force | Out-Null
}

# 6. Compile Installer
$nsiScript = "installer\jlauncher.nsi"
Write-Host "Compiling NSIS script: $nsiScript..." -ForegroundColor Cyan

$process = Start-Process -FilePath $makensis -ArgumentList "/DVERSION=$Version", "$nsiScript" -Wait -PassThru -NoNewWindow
if ($process.ExitCode -ne 0) {
    Write-Error "makensis failed with exit code $($process.ExitCode)!"
}

$setupExe = "$outDir\JLauncher_${Version}_x64-setup.exe"
$genericSetup = "$outDir\jlauncher-setup.exe"

if (Test-Path $setupExe) {
    Copy-Item $setupExe -Destination $genericSetup -Force
    Write-Host "Successfully generated Windows Installer:" -ForegroundColor Green
    Write-Host "  - $setupExe" -ForegroundColor Green
    Write-Host "  - $genericSetup" -ForegroundColor Green
} else {
    Write-Error "Expected installer output was not found at '$setupExe'!"
}
