param([switch]$Uninstall)

$ErrorActionPreference = "Stop"

$Source = Join-Path $PSScriptRoot "translator-popup-desktop.exe"
$InstallDir = Join-Path $env:LOCALAPPDATA "Programs\OpenTranslator"
$Exe = Join-Path $InstallDir "translator-popup-desktop.exe"
$Startup = Join-Path ([Environment]::GetFolderPath("Startup")) "OpenTranslator.lnk"

if ($Uninstall) {
    Remove-Item $Startup -ErrorAction SilentlyContinue
    Remove-Item -Recurse -Force $InstallDir -ErrorAction SilentlyContinue
    Write-Host "OpenTranslator uninstalled."
    exit 0
}

if (-not (Test-Path $Source)) {
    throw "translator-popup-desktop.exe not found next to this script"
}

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Copy-Item $Source $Exe -Force

$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($Startup)
$shortcut.TargetPath = $Exe
$shortcut.WorkingDirectory = $InstallDir
$shortcut.Description = "OpenTranslator selection translation"
$shortcut.Save()

Write-Host ""
Write-Host "Installed: $InstallDir"
Write-Host "Started automatically at login. The model (~1.1 GB) downloads on first run."
Write-Host "Select text and press Ctrl+Alt+T. Uninstall: install.ps1 -Uninstall"
