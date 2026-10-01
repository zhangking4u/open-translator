OpenTranslator for Linux (GNOME)
================================

1. The global shortcut is registered automatically on first launch (default
   Ctrl+Alt+T). To register or change it manually, as your desktop user:

     open-translator-setup [--binding "<Control><Alt>t"]

   (The popup only auto-registers when the shortcut is missing, so a custom
   binding is never overwritten.)

2. Select text in any application and press the shortcut. The first run
   downloads the model (~1.1 GB) from ModelScope into
   ~/.local/share/open-translator/models/ and starts the translator service
   automatically. Later runs reuse the loaded service.

Configuration: ~/.config/open-translator/config

  service_url = http://127.0.0.1:17890
  source = en
  target = zh
  model_path = <path to a .gguf model>   (optional; default is the models dir)
  prompt_style = hymt                    (generic / translategemma / hymt)
  auto_download = true                   (download the default model on first run)
  check_updates = true                   (check GitHub for a newer release)

The environment can override the auto-start behaviour:
TRANSLATOR_ENGINE=ollama switches back to a local Ollama server,
TRANSLATOR_MODEL_PATH points at an existing GGUF file,
TRANSLATOR_AUTO_DOWNLOAD=false disables the first-run download.

Logs: ~/.local/state/open-translator/
Remove the shortcut: open-translator-setup --uninstall

Requirements: glibc >= 2.39 (built on Ubuntu 24.04), GTK4 >= 4.10, a Wayland
session with wl-clipboard (part of the package dependencies).
