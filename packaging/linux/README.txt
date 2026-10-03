OpenTranslator for Linux (GNOME)
================================

1. Register the global shortcut once, as your desktop user (the deb's postinst
   prints this; the shortcut is not registered automatically):

     open-translator-setup [--binding "<Control><Alt>t"]

   This also writes the --autostart entry so the app starts hidden in the tray.

2. Select text in any application and press the shortcut (default Ctrl+Alt+T).
   The client embeds llama.cpp (no separate service); the first run downloads
   the model (~1.1 GB) from ModelScope into
   ~/.local/share/open-translator/models/ .

Configuration: ~/.config/open-translator/config

  service_url = http://127.0.0.1:17890
  source = auto
  target = zh
  model_path = <path to a .gguf model>   (optional; default is the models dir)
  prompt_style = hymt                    (generic / translategemma / hymt)
  auto_download = true                   (download the default model on first run)
  check_updates = true                   (check GitHub for a newer release)
  hotkey = Ctrl+Alt+T                    (global shortcut)

Environment overrides: TRANSLATOR_MODEL_PATH (existing GGUF),
TRANSLATOR_HOTKEY (shortcut), TRANSLATOR_CHECK_UPDATES=false (disable the
update check).

Upgrading: the client's "更新到 v..." action downloads the new deb, verifies
its SHA-256 (GitHub release asset digest) and installs it through pkexec (a
policykit password prompt), then restarts itself. After a manual `apt install`
the running tray client keeps the old binary until it is restarted or you log
in again.

Remove the shortcut and the autostart entry: open-translator-setup --uninstall

Requirements: glibc >= 2.39 (built on Ubuntu 24.04), a Wayland (XWayland) or
X11 session with wl-clipboard. Reading the selection falls back to the X11
PRIMARY selection, so plain Xorg sessions work too. Runtime dependencies
(libwebkit2gtk-4.1-0, libgtk-3-0, libayatana-appindicator3-1, wl-clipboard,
libnotify-bin, libgomp1) are installed by apt; speech-dispatcher enables
reading translations aloud and pkexec enables the in-client deb update.
