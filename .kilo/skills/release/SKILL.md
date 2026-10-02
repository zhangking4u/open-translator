---
name: release
description: Tag and publish an OpenTranslator GitHub release (Windows zip + macOS dmg + Linux deb via the Release workflow). Use when the user asks to cut a release, 发版, 发布 release, or push a v* tag.
---

# Release OpenTranslator

## Preflight

- `git status -sb` — working tree clean, no commits ahead of `origin/main` (push them first).
- `gh run list --limit 3` — the latest `CI` run on `main` should be green; release builds the same code.
- Tag format `v<version>` (e.g. `v0.1.0`). Crate versions are not wired to the tag; bump them only if you want the version surfaced in binaries.

## Publish

```bash
git push                                   # main up to date
git tag -a v0.1.0 -m "OpenTranslator v0.1.0"
git push origin v0.1.0
```

Pushing a `v*` tag triggers `.github/workflows/release.yml`, which:

1. builds the Tauri client (`desktop/translator-popup-tauri`) on `windows-latest`, `macos-latest` and `ubuntu-latest` (the Linux job then runs the deb packaging; no core-service or GTK-popup build);
2. packages `OpenTranslator-windows-x64.zip` (`translator-popup-tauri.exe` + `packaging/windows/install.ps1` + README), `OpenTranslator-macos-<arch>.dmg` (`packaging/macos/make-dmg.sh` + `Info.plist`) and `OpenTranslator-linux-x64.deb` (`packaging/linux/make-deb.sh`; the Linux build needs `libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev patchelf pkg-config`, and the deb ships `/usr/lib/open-translator/translator-popup` + the `/usr/bin/translator-popup` symlink, `open-translator-setup`, desktop entry, icon, and a glibc ≥ 2.39 dependency);
3. creates the GitHub release (`gh release create --generate-notes`) and uploads all three assets; the build steps inject `OPEN_TRANSLATOR_VERSION` from the tag for the startup update check.

The workflow also supports manual `workflow_dispatch`: it then only uploads workflow artifacts and does not touch releases — useful for testing packaging before tagging.

## Verify

```bash
gh run list --workflow=release.yml
gh run watch <run-id>
gh release view v0.1.0
```

Expected assets: `OpenTranslator-windows-x64.zip`, `OpenTranslator-macos-arm64.dmg`, `OpenTranslator-linux-x64.deb`.
Artifacts can be downloaded with `gh run download <run-id> -n <name>` (can be slow).

## Polish / troubleshoot

- Better release body: `gh release edit v0.1.0 --title "OpenTranslator v0.1.0" --notes "..."` — mention: local inference (no cloud), first run downloads the model (~1.1 GB from ModelScope), Windows runs `install.ps1` (v0.2.x eframe clients cannot one-click update to the Tauri package: tell users to download the new zip and run `install.ps1` once), Linux `sudo apt install ./OpenTranslator-linux-x64.deb` then run `open-translator-setup` once to bind the GNOME shortcut (default Ctrl+Alt+T; needs Ubuntu 24.04+/glibc ≥ 2.39), macOS needs Accessibility permission, unsigned builds may trigger SmartScreen/Gatekeeper.
- Failed job: `gh run rerun <run-id> --failed`.
- Missing/stale assets after a rerun: `gh release upload v0.1.0 <file> --clobber`.
- Re-do a release: `git tag -d v0.1.0 && git push origin :refs/tags/v0.1.0`, then tag and push again.
- Known limitations: builds are unsigned (code-signing/notarization pending); the macOS artifact matches the runner arch (arm64 for `macos-latest`); the desktop client downloads the model on first run rather than bundling it.
