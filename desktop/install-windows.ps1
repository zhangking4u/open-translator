# Install the OpenTranslator Windows desktop client for the current user:
#   - builds the release binaries (core service + popup)
#   - adds a shortcut to the Startup folder so the popup starts with Windows
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File .\install-windows.ps1
#   powershell -ExecutionPolicy Bypass -File .\install-windows.ps1 -NoStart
#   powershell -ExecutionPolicy Bypass -File .\install-windows.ps1 -Uninstall

param([switch]$Uninstall, [switch]$NoStart)

$ErrorActionPreference = "Stop"

$DesktopDir = $PSScriptRoot
$RepoRoot = Split-Path -Parent $DesktopDir
$PopupManifest = Join-Path $DesktopDir "translator-popup-desktop\Cargo.toml"
$CoreManifest = Join-Path $RepoRoot "core\translator-service\Cargo.toml"
# Keep the cargo target dir short: MSBuild FileTracker (used while building
# llama.cpp for the embedded engine) fails with MSB6003 when the repo path plus
# <target>\release\build\llama-cpp-sys-2-*\out\build\CMakeScratch\... is too deep.
$BuildDir = Join-Path $env:LOCALAPPDATA "OpenTranslator\build"
$Exe = Join-Path $BuildDir "release\translator-popup-desktop.exe"
$Startup = [Environment]::GetFolderPath("Startup")
$Link = Join-Path $Startup "OpenTranslator.lnk"

if ($Uninstall) {
    if (Test-Path $Link) {
        Remove-Item $Link
        Write-Host "Removed $Link"
    } else {
        Write-Host "No OpenTranslator shortcut found in $Startup"
    }
    exit 0
}

# Windows locks a running executable against overwrite, and MSBuild cannot
# relink the binary while it runs, so stop any running instance before the
# build; it can be started again (or comes back at next login).
$running = Get-Process -Name "translator-popup-desktop" -ErrorAction SilentlyContinue
if ($running) {
    Write-Host "Stopping the running OpenTranslator instance..."
    $running | Stop-Process -Force -ErrorAction SilentlyContinue
}

# The old process can keep the image locked while it finishes exiting; wait
# for the binary to become writable so the linker can replace it.
if (Test-Path $Exe) {
    $deadline = (Get-Date).AddSeconds(10)
    while ($true) {
        try {
            $stream = [System.IO.File]::Open($Exe, 'Open', 'ReadWrite', 'None')
            $stream.Close()
            break
        } catch {
            if ((Get-Date) -ge $deadline) {
                break
            }
            Start-Sleep -Milliseconds 200
        }
    }
}

Write-Host "Building release binaries..."
$env:CARGO_TARGET_DIR = $BuildDir

function Add-PathEntry([string]$Dir) {
    if ($Dir -and (Test-Path $Dir) -and (($env:PATH -split ';') -notcontains $Dir)) {
        $env:PATH = "$Dir;$env:PATH"
    }
}

if (-not (Get-Command cmake -ErrorAction SilentlyContinue)) {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path $vswhere) {
        $vsRoot = & $vswhere -latest -products * -property installationPath
        if ($vsRoot) {
            Add-PathEntry (Join-Path $vsRoot "Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin")
        }
    }
    Add-PathEntry (Join-Path $env:ProgramFiles "CMake\bin")
    Add-PathEntry (Join-Path ${env:ProgramFiles(x86)} "CMake\bin")
    if (-not (Get-Command cmake -ErrorAction SilentlyContinue)) {
        throw "cmake not found; install CMake or the Visual Studio C++ build tools"
    }
}

if (-not $env:LIBCLANG_PATH) {
    $llvmBin = Join-Path $env:ProgramFiles "LLVM\bin"
    if (Test-Path (Join-Path $llvmBin "libclang.dll")) {
        $env:LIBCLANG_PATH = $llvmBin
    }
}

# Cargo writes progress to stderr; with $ErrorActionPreference = "Stop" Windows
# PowerShell 5.1 turns that into a terminating NativeCommandError, so relax it
# around the native calls and check their exit codes instead.
$ErrorActionPreference = "Continue"
cargo build --release --manifest-path $CoreManifest
if ($LASTEXITCODE -ne 0) {
    throw "cargo build failed with exit code $LASTEXITCODE`: $CoreManifest"
}
cargo build --release --manifest-path $PopupManifest
if ($LASTEXITCODE -ne 0) {
    throw "cargo build failed with exit code $LASTEXITCODE`: $PopupManifest"
}
$ErrorActionPreference = "Stop"

if (-not (Test-Path $Exe)) {
    throw "popup binary not found after build: $Exe"
}

$ModelDir = Join-Path $env:LOCALAPPDATA "open-translator\models"
$ModelFile = Join-Path $ModelDir "hy-mt1.5-1.8b-q4_k_m.gguf"

if (-not (Test-Path $ModelFile)) {
    Write-Host "warning: model not found at $ModelFile" -ForegroundColor Yellow
    Write-Host "         place a .gguf there, or set model_path in %APPDATA%\open-translator\config" -ForegroundColor Yellow
}

$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($Link)
$shortcut.TargetPath = $Exe
$shortcut.WorkingDirectory = Split-Path $Exe
$shortcut.Description = "OpenTranslator selection translation"
$shortcut.Arguments = "--autostart"
$shortcut.Save()

if (-not $NoStart) {
    Start-Process -FilePath $Exe -ArgumentList "--autostart"
}

Write-Host ""
Write-Host "Installed:"
Write-Host "  shortcut: $Link"
Write-Host "  binary:   $Exe"
Write-Host ""
Write-Host "Select text anywhere and press Ctrl+Alt+T. The app starts with Windows,"
Write-Host "hides on Esc/close, and translates through the local service (started on demand)."
