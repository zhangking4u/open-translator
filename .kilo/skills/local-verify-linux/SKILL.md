---
name: local-verify-linux
description: Build the OpenTranslator Linux Tauri client from the current working tree, package the deb, install it through pkexec and relaunch it so the user can verify by hand. Use when the user asks for Linux 本地编译/真机验证/上手验证/装 deb 验证/在本机试试最新改动, or to prepare the Ubuntu dev machine for a manual check of desktop-client changes.
---

# Local build + real-machine verification (Linux)

Rebuild the Tauri client from the current source, package and install it, then relaunch it for a manual test. This verifies uncommitted local changes, so do not require a clean tree. Record `git log -1 --oneline` for the hand-over.

The Windows flow is the `local-verify` skill; for macOS use `desktop/install-macos.sh`.

## Key paths

| What | Path |
| --- | --- |
| Crate (run cargo here, no workspace root) | `desktop/translator-popup-tauri` |
| Build output | `desktop/translator-popup-tauri/target/release/translator-popup-tauri` |
| Deb | `packaging/linux/build/OpenTranslator-linux-x64.deb` |
| Installed binary | `/usr/lib/open-translator/translator-popup` (+ `/usr/bin/translator-popup` symlink) |
| Config | `~/.config/open-translator/config` |
| History (do not clobber) | `~/.config/open-translator/history.json` |
| Model | `~/.local/share/open-translator/models/hy-mt1.5-1.8b-q4_k_m.gguf` |
| Health | `http://127.0.0.1:17890/health` |

## 1. Preflight

```bash
git log -1 --oneline
PATH="$HOME/.local/opt/cmake/bin:$PATH" cmake --version   # llama-cpp-sys builds with cmake
test -f "$HOME/.local/share/open-translator/models/hy-mt1.5-1.8b-q4_k_m.gguf" && echo "model ok"
pgrep -a -f 'translator-popup' | grep -v pgrep || echo "no client running"
```

- Build needs the user-space `cmake` (`~/.local/opt/cmake/bin`) and `LIBCLANG_PATH=/usr/lib/llvm-21/lib`.
- A running installed client keeps the old (unlinked) binary live, and the single-instance plugin forwards `--history`/`--settings`/`--translate` from any new binary to it. Stop it in step 4 before launching a local build.

## 2. Build

From the repo root:

```bash
cd desktop/translator-popup-tauri
PATH="$HOME/.local/opt/cmake/bin:$PATH" LIBCLANG_PATH=/usr/lib/llvm-21/lib \
  OPEN_TRANSLATOR_VERSION=0.4.1 cargo build --release --locked
```

- Inject the version the build should report (the release workflow injects the tag; `current_version()` strips a leading `v`). Without it the settings page shows the crate version (`0.1.0`) and the update check compares against that.
- Incremental rebuild is ~20 s once llama.cpp is cached; confirm the artifact mtime is fresh.

## 3. Package (needed for the installed-client check)

From the repo root:

```bash
VERSION=0.4.1 packaging/linux/make-deb.sh
dpkg-deb -I packaging/linux/build/OpenTranslator-linux-x64.deb | grep -E 'Version|Recommends'
```

## 4. Stop the old instance

```bash
pkill -f '/usr/bin/translator-popup --autostart'; sleep 1
pgrep -a -f 'translator-popup' | grep -v pgrep || echo stopped
```

The appindicator helper forks a child with the same command line, so `pkill -f` normally stops both; verify with the `pgrep` above (avoid `pgrep -f translator-popup` matching the agent's own shell command).

## 5. Install (B) or run the local binary (A)

**B — installed deb, closest to a real user.** The dev machine has no passwordless sudo; use pkexec, which pops the polkit dialog for the user to confirm — tell them before running it and allow a long command timeout:

From the repo root (`$PWD` must be the repo root; pkexec clears the environment, so keep the deb path absolute):

```bash
pkexec /usr/bin/env DEBIAN_FRONTEND=noninteractive /usr/bin/apt-get install -y \
  "$PWD/packaging/linux/build/OpenTranslator-linux-x64.deb"
dpkg -s open-translator | grep '^Version'
```

**A — no install.** After step 4 run the local build directly (paths are from the repo root):

```bash
env -u GDK_BACKEND desktop/translator-popup-tauri/target/release/translator-popup-tauri --autostart
# or --history / --settings / --translate for a targeted window
```

- `apt install` never restarts a running client (the old inode keeps serving) — always restart in step 6, regardless of the method.
- pkexec clears the environment; pass absolute paths.

## 6. Start and verify

Start the client as a tracked persistent background process (`background_process` start, `persistent: true`) so it survives the agent session — a plain shell `&` dies with the shell. Command: `env -u GDK_BACKEND /usr/bin/translator-popup --autostart` (or the local binary from 5A).

```bash
sleep 6
curl -s http://127.0.0.1:17890/health
/usr/bin/translator-popup --history                          # forwards to the running instance
```

- `env -u GDK_BACKEND` matters: the dev shell may export `GDK_BACKEND=wayland`; unsetting it lets `prefer_x11_backend` pick XWayland, the intended Linux backend.
- Hand over for manual checks: `Ctrl+Alt+T` selection translation, tray 显示窗口/历史/设置, the settings version, and whatever the change touched (`--translate`, `--history`, `--settings`, `--print` exist for targeted checks).
- To seed the history page for a layout check, back up and restore `~/.config/open-translator/history.json` around the test; never leave test entries behind.

## Notes / troubleshooting

- XTEST input cannot be injected under this Wayland session (Mutter drops synthetic pointer/key events), so click/key-driven checks must be done by the user here or in an Xorg session. Keyboard checks with `XSetInputFocus` + XTEST work only intermittently.
- No window after `--history`/`--settings`: you are talking to an old instance (step 4) or the new one failed to start; check `pgrep -a -f translator-popup`.
- The postinst "restart the tray client" message on upgrade is expected; a manual `apt upgrade` needs the restart to load the new binary.
- Missing model: the app still starts; only the first translation is affected; `model_path` in the config overrides the default.
- After verification, leave the machine in the normal state: the installed client running `--autostart` (hidden) and the user history untouched.
