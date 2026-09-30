# Install the OpenTranslator Windows desktop client for the current user:
#   - builds the release binaries (core service + popup)
#   - adds a shortcut to the Startup folder so the popup starts with Windows
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File .\install-windows.ps1
#   powershell -ExecutionPolicy Bypass -File .\install-windows.ps1 -Uninstall

param([switch]$Uninstall)

$ErrorActionPreference = "Stop"

$DesktopDir = $PSScriptRoot
$RepoRoot = Split-Path -Parent $DesktopDir
$PopupManifest = Join-Path $DesktopDir "translator-popup-desktop\Cargo.toml"
$CoreManifest = Join-Path $RepoRoot "core\translator-service\Cargo.toml"
$Exe = Join-Path $DesktopDir "translator-popup-desktop\target\release\translator-popup-desktop.exe"
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

Write-Host "Building release binaries..."
cargo build --release --manifest-path $CoreManifest
cargo build --release --manifest-path $PopupManifest

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
$shortcut.Save()

Write-Host ""
Write-Host "Installed:"
Write-Host "  shortcut: $Link"
Write-Host "  binary:   $Exe"
Write-Host ""
Write-Host "Select text anywhere and press Ctrl+Alt+T. The app starts with Windows,"
Write-Host "hides on Esc/close, and translates through the local service (started on demand)."
