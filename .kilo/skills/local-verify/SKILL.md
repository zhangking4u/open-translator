---
name: local-verify
description: Build the OpenTranslator Windows Tauri client from the current working tree, stop the installed instance, replace it and relaunch for real-machine verification. Use when the user asks for 本地编译/真机验证/停掉旧实例启动新实例/在本机试试最新改动, or to try a code change in the desktop client on this Windows dev machine.
---

# Local build + real-machine verification (Windows)

Rebuild the Tauri desktop client from the current source, swap it into the installed location, and start it for a manual test. This verifies uncommitted local changes, so do not require a clean tree. Record the revision being tested for the report (`git log -1 --oneline`).

macOS/Linux: this skill is Windows-only (the dev machine is Windows). For macOS use `desktop/install-macos.sh`; for Linux use the `local-verify-linux` skill.

## Key paths

| What | Path |
| --- | --- |
| Crate | `desktop/translator-popup-tauri` (run cargo from the repo root) |
| Build output | `%LOCALAPPDATA%\OpenTranslator\build\release\translator-popup-tauri.exe` |
| Installed copy (what the Startup shortcut points at) | `%LOCALAPPDATA%\Programs\OpenTranslator\translator-popup-tauri.exe` |
| Model | `%LOCALAPPDATA%\open-translator\models\hy-mt1.5-1.8b-q4_k_m.gguf` (or `model_path` in `%APPDATA%\open-translator\config`) |

The short `%LOCALAPPDATA%\OpenTranslator\build` target dir is mandatory: `llama-cpp-sys-2` fails with MSB6003/FileTracker when the repo path plus `<target>\release\build\llama-cpp-sys-2-*\out\build\CMakeScratch\...` is too deep.

## 1. Preflight

```powershell
cargo --version
cmake --version
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"   # if unset; needs libclang.dll there
Test-Path "$env:LOCALAPPDATA\open-translator\models\hy-mt1.5-1.8b-q4_k_m.gguf"
Get-Process translator-popup-tauri, translator-popup-desktop -ErrorAction SilentlyContinue |
    Select-Object Id, StartTime, Path
```

- Warn if the model is missing (app still starts, first translation guidance only).
- If a running process path is the build-dir exe, it locks the build output: stop it (step 3) before building.

## 2. Build

```powershell
$env:CARGO_TARGET_DIR = "$env:LOCALAPPDATA\OpenTranslator\build"
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
cargo build --release --manifest-path desktop/translator-popup-tauri/Cargo.toml
```

- PowerShell 5.1 turns cargo's stderr progress into a `NativeCommandError` record; that is not a failure. Judge by the final `Finished \`release\` profile` line and `$LASTEXITCODE` (cargo writes progress to stderr).
- Incremental rebuild ~2-3 min once llama.cpp is cached; a clean llama.cpp build takes much longer. Confirm the artifact `LastWriteTime` is fresh.

## 3. Stop the old instance

```powershell
Get-Process translator-popup-tauri, translator-popup-desktop -ErrorAction SilentlyContinue |
    Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 500
```

Windows keeps the old image locked while it exits; step 4 retries the copy instead of waiting longer.

## 4. Replace the binary in place

Copy over the exe the Startup shortcut / running instance uses (normally the installed copy). Do not run `desktop/install-windows.ps1` for this: it repoints the Startup shortcut at the build dir and starts from there, which is a different (dev) install.

```powershell
$Source = Join-Path $env:LOCALAPPDATA "OpenTranslator\build\release\translator-popup-tauri.exe"
$Exe = Join-Path $env:LOCALAPPDATA "Programs\OpenTranslator\translator-popup-tauri.exe"
$deadline = (Get-Date).AddSeconds(10)
while ($true) {
    try { Copy-Item $Source $Exe -Force -ErrorAction Stop; break }
    catch { if ((Get-Date) -ge $deadline) { throw }; Start-Sleep -Milliseconds 200 }
}
Get-Item $Exe | Select-Object FullName, Length, LastWriteTime
```

If the shortcut points at the build dir instead, replace that exe (or just run the build-dir exe after stopping the old one).

## 5. Start and verify

```powershell
$Exe = Join-Path $env:LOCALAPPDATA "Programs\OpenTranslator\translator-popup-tauri.exe"
Start-Process -FilePath $Exe -ArgumentList "--autostart"    # tray, mirrors login start
Start-Sleep -Seconds 4
Start-Process -FilePath $Exe -ArgumentList "--settings"     # single-instance forward -> settings window
Start-Sleep -Seconds 3
Get-Process translator-popup-tauri | Select-Object Id, StartTime, MainWindowTitle
```

- Expect exactly one process with `MainWindowTitle` `OpenTranslator`; a lingering second process means single-instance forwarding did not work.
- The settings page is where the update UI lives; then hand over for manual checks: Ctrl+Alt+T selection translation, tray 设置/历史, and whatever the change touched (`--translate`, `--history`, `--print` exist for targeted checks).
- An instance started from a terminal prints tracing logs there; use that when the user reports an error.

## Troubleshooting

- `MSB6003`/FileTracker during build: use the short `CARGO_TARGET_DIR` (step 2) and stop any build-dir instance first.
- Copy access denied: old process still exiting; the retry loop handles it, else re-run step 3.
- No model warning: place the gguf or set `model_path`; a release package downloads it on first run.
- `--settings` shows nothing: re-run it after the tray instance settled; check for a stuck second process and kill it.
- Changes in `core/inference` or `desktop/translator-core` need no extra steps; they are path dependencies of the client.
