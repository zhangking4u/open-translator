param([switch]$Uninstall, [switch]$NoStart)

$ErrorActionPreference = "Stop"

$Source = Join-Path $PSScriptRoot "translator-popup-tauri.exe"
$InstallDir = Join-Path $env:LOCALAPPDATA "Programs\OpenTranslator"
$Exe = Join-Path $InstallDir "translator-popup-tauri.exe"
$LegacyExe = Join-Path $InstallDir "translator-popup-desktop.exe"
$Startup = Join-Path ([Environment]::GetFolderPath("Startup")) "OpenTranslator.lnk"

function Stop-RunningApp {
    # Both the Tauri client and the legacy eframe client block the copy.
    foreach ($name in "translator-popup-tauri", "translator-popup-desktop") {
        Get-Process -Name $name -ErrorAction SilentlyContinue |
            Stop-Process -Force -ErrorAction SilentlyContinue
    }
}

if ($Uninstall) {
    Stop-RunningApp
    Remove-Item $Startup -ErrorAction SilentlyContinue
    Remove-Item -Recurse -Force $InstallDir -ErrorAction SilentlyContinue
    Write-Host "OpenTranslator uninstalled."
    exit 0
}

if (-not (Test-Path $Source)) {
    throw "translator-popup-tauri.exe not found next to this script"
}

# Windows locks a running executable against overwrite, so stop any running
# instance first; it can be started again (or comes back at next login).
Stop-RunningApp

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

# The old process can keep the image locked while it finishes exiting, so
# retry the copy briefly.
$deadline = (Get-Date).AddSeconds(10)
while ($true) {
    try {
        Copy-Item $Source $Exe -Force -ErrorAction Stop
        break
    } catch {
        if ((Get-Date) -ge $deadline) {
            throw
        }
        Start-Sleep -Milliseconds 200
    }
}

# Upgrading from the eframe client: drop its binary once the new one is in place.
if (Test-Path $LegacyExe) {
    Remove-Item $LegacyExe -Force -ErrorAction SilentlyContinue
}

# Screenshot-OCR runtime shipped next to the executable; a stopping process
# can keep it loaded briefly, so retry like the binary copy above.
$OrtSource = Join-Path $PSScriptRoot "onnxruntime.dll"
if (Test-Path $OrtSource) {
    $deadline = (Get-Date).AddSeconds(10)
    while ($true) {
        try {
            Copy-Item $OrtSource (Join-Path $InstallDir "onnxruntime.dll") -Force -ErrorAction Stop
            break
        } catch {
            if ((Get-Date) -ge $deadline) {
                throw
            }
            Start-Sleep -Milliseconds 200
        }
    }
}

$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($Startup)
$shortcut.TargetPath = $Exe
$shortcut.WorkingDirectory = $InstallDir
$shortcut.Description = "OpenTranslator selection translation"
$shortcut.Arguments = "--autostart"
$shortcut.Save()

# Bring the app back after an in-place upgrade; --autostart keeps it in the
# tray so the upgrade stays silent.
if (-not $NoStart) {
    Start-Process -FilePath $Exe -ArgumentList "--autostart"
}

Write-Host ""
Write-Host "Installed: $InstallDir"
Write-Host "Started automatically at login and in the tray. The model (~1.1 GB) downloads on first run."
Write-Host "Select text and press Ctrl+Alt+T. Uninstall: install.ps1 -Uninstall"
