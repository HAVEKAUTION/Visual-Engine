# Visual Engine: one-time setup for Windows 10 / 11.
#
# Installs what the app needs to build and run, then builds it:
#   1. Microsoft C++ Build Tools  (the compiler Rust uses on Windows)
#   2. Rust                      (the language the app is written in)
#   3. ffmpeg                    (optional, for video files)
# Anything already installed is skipped, so it is safe to run again.
# Everything comes from winget (Windows' own package manager), from the
# official publishers. Nothing else is downloaded.
#
# Run it by double-clicking setup.bat (next to this file).

param(
    # Skip ffmpeg (video files won't play; everything else works).
    [switch]$NoFfmpeg,
    # Install only, don't build.
    [switch]$NoBuild
)

$ErrorActionPreference = 'Continue'
Set-Location -LiteralPath $PSScriptRoot

function Step($text) { Write-Host ""; Write-Host "==> $text" -ForegroundColor Cyan }
function Ok($text) { Write-Host "    OK: $text" -ForegroundColor Green }
function Warn($text) { Write-Host "    $text" -ForegroundColor Yellow }
function Fail($text) {
    Write-Host ""
    Write-Host "SETUP STOPPED: $text" -ForegroundColor Red
    exit 1
}

# Re-read PATH so programs installed a moment ago can be found without
# opening a new window.
function Refresh-Path {
    $machine = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $user = [Environment]::GetEnvironmentVariable('Path', 'User')
    $cargo = Join-Path $env:USERPROFILE '.cargo\bin'
    $env:Path = "$machine;$user;$cargo"
}

function Have($cmd) { [bool](Get-Command $cmd -ErrorAction SilentlyContinue) }

function Winget-Install($id, $extra) {
    $wargs = @('install', '--id', $id, '-e', '--source', 'winget',
        '--accept-package-agreements', '--accept-source-agreements') + $extra
    & winget @wargs
    # winget returns non-zero for "already installed / no upgrade"; we check the
    # result ourselves afterwards instead of trusting the exit code.
}

Write-Host "Visual Engine setup" -ForegroundColor White
Write-Host "This can take 10-30 minutes the first time (mostly downloads and the first build)."

# --- 0. Windows version and winget -----------------------------------------
Step "Checking Windows"
if (-not [Environment]::Is64BitOperatingSystem) { Fail "Visual Engine needs 64-bit Windows." }
if (-not (Have winget)) {
    Fail ("winget is missing. Open the Microsoft Store, search for 'App Installer', " +
        "install or update it, then run setup.bat again.")
}
Ok "64-bit Windows with winget"

# --- 1. C++ Build Tools ------------------------------------------------------
Step "1/3  Microsoft C++ Build Tools"
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
function Have-Msvc {
    if (-not (Test-Path $vswhere)) { return $false }
    $found = & $vswhere -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    return [bool]$found
}
if (Have-Msvc) {
    Ok "already installed"
} else {
    Warn "Installing (about 2-6 GB). Windows will ask for permission: click Yes."
    Warn "A Visual Studio Installer window shows progress. Let it finish."
    Winget-Install 'Microsoft.VisualStudio.2022.BuildTools' @('--override',
        '--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended')
    if (-not (Have-Msvc)) {
        Fail ("the C++ Build Tools didn't install. Install them by hand: " +
            "https://visualstudio.microsoft.com/visual-cpp-build-tools/ , tick " +
            "'Desktop development with C++', press Install, then run setup.bat again.")
    }
    Ok "installed"
}

# --- 2. Rust -------------------------------------------------------------------
Step "2/3  Rust"
Refresh-Path
if (-not (Have rustup)) {
    Warn "Installing Rust..."
    Winget-Install 'Rustlang.Rustup' @()
    Refresh-Path
    if (-not (Have rustup)) {
        Fail "Rust didn't install. Install it from https://rustup.rs , then run setup.bat again."
    }
}
# Make sure the latest stable Rust is the one used (the app needs a recent one).
& rustup toolchain install stable --profile minimal
& rustup default stable | Out-Null
if (-not (Have cargo)) { Fail "Rust is installed but 'cargo' isn't found. Restart the PC and run setup.bat again." }
Ok (& rustc --version)

# --- 3. ffmpeg (optional) -------------------------------------------------------
Step "3/3  ffmpeg (for video files)"
if ($NoFfmpeg) {
    Warn "skipped (-NoFfmpeg). Pictures, cameras and scenes still work; video files won't."
} elseif ((Have ffmpeg) -and (Have ffprobe)) {
    Ok "already installed"
} else {
    Warn "Installing ffmpeg..."
    Winget-Install 'Gyan.FFmpeg' @()
    Refresh-Path
    if ((Have ffmpeg) -and (Have ffprobe)) {
        Ok "installed"
    } else {
        Warn "ffmpeg didn't install. Everything except video files still works."
        Warn "To add it later, open Terminal and run:  winget install Gyan.FFmpeg"
    }
}

# --- Build -------------------------------------------------------------------------
if ($NoBuild) {
    Step "Done (build skipped)."
    exit 0
}
Step "Building Visual Engine (5-10 minutes the first time)"
& cargo build --release
if ($LASTEXITCODE -ne 0) {
    Fail ("the build failed (see the messages above). Copy them into an issue on " +
        "https://github.com/HAVEKAUTION/Visual-Engine/issues")
}
Ok "built: target\release\visual-engine.exe"

Write-Host ""
Write-Host "All set. Double-click run.bat to start Visual Engine." -ForegroundColor Green
