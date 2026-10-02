#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

fn main() {
    let args = match translator_core::args::Args::from_process_env() {
        Ok(args) => args,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };

    let config = translator_core::settings::load_config();
    let hotkey_spec = config
        .hotkey
        .clone()
        .or_else(|| std::env::var("TRANSLATOR_HOTKEY").ok())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Ctrl+Alt+T".to_string());

    let show_on_start = !args.autostart || args.stdin || args.settings;

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main(app);
        }))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        show_main(app);
                        let _ = app.emit("translate", ());
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![hide_window, quit_app, resize_window])
        .setup(move |app| {
            let handle = app.handle().clone();

            let menu = Menu::with_items(
                &handle,
                &[
                    &MenuItem::with_id(&handle, "show", "显示窗口", true, None::<&str>)?,
                    &MenuItem::with_id(
                        &handle,
                        "translate",
                        "立即翻译选中文本",
                        true,
                        None::<&str>,
                    )?,
                    &MenuItem::with_id(&handle, "quit", "退出", true, None::<&str>)?,
                ],
            )?;

            TrayIconBuilder::with_id("main")
                .icon(Image::new_owned(make_icon_rgba(), 32, 32))
                .tooltip(format!("OpenTranslator（{hotkey_spec}）"))
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main(app),
                    "translate" => {
                        show_main(app);
                        let _ = app.emit("translate", ());
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(&handle)?;

            if let Err(error) = app.global_shortcut().register(hotkey_spec.as_str()) {
                eprintln!("failed to register {hotkey_spec}: {error}");
            }

            if show_on_start {
                show_main(&handle);
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running OpenTranslator");
}

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[tauri::command]
fn hide_window(window: WebviewWindow) {
    let _ = window.hide();
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn resize_window(window: WebviewWindow, height: f64) {
    if let Ok(size) = window.inner_size() {
        let height = height.clamp(120.0, 2000.0) as u32;
        let _ = window.set_size(tauri::PhysicalSize::new(size.width, height));
    }
}

fn make_icon_rgba() -> Vec<u8> {
    const SIZE: u32 = 32;
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];

    let center = (SIZE as f32 - 1.0) / 2.0;
    let radius = 15.0;

    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 - center;
            let dy = y as f32 - center;

            if (dx * dx + dy * dy).sqrt() <= radius {
                let index = ((y * SIZE + x) * 4) as usize;
                rgba[index] = 0x25;
                rgba[index + 1] = 0x63;
                rgba[index + 2] = 0xeb;
                rgba[index + 3] = 0xff;
            }
        }
    }

    for top in [11u32, 19] {
        for y in top..(top + 2) {
            for x in 9..23 {
                let index = ((y * SIZE + x) * 4) as usize;
                rgba[index] = 0xff;
                rgba[index + 1] = 0xff;
                rgba[index + 2] = 0xff;
                rgba[index + 3] = 0xff;
            }
        }
    }

    rgba
}
