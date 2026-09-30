#[cfg(any(target_os = "windows", target_os = "macos"))]
mod platform {
    use tray_icon::TrayIcon;
    use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TrayCommand {
        Show,
        Translate,
        Quit,
    }

    pub struct Tray {
        _tray: TrayIcon,
        show_id: MenuId,
        translate_id: MenuId,
        quit_id: MenuId,
    }

    impl Tray {
        pub fn is_supported() -> bool {
            true
        }

        pub fn new(tooltip: &str) -> Result<Self, String> {
            let menu = Menu::new();

            let show = MenuItem::with_id("show", "显示窗口", true, None);
            let translate = MenuItem::with_id("translate", "立即翻译选中文本", true, None);
            let quit = MenuItem::with_id("quit", "退出", true, None);

            menu.append(&show)
                .map_err(|error| format!("failed to build tray menu: {error}"))?;
            menu.append(&translate)
                .map_err(|error| format!("failed to build tray menu: {error}"))?;
            menu.append(&quit)
                .map_err(|error| format!("failed to build tray menu: {error}"))?;

            let tray = tray_icon::TrayIconBuilder::new()
                .with_menu(Box::new(menu))
                .with_tooltip(tooltip)
                .with_icon(make_icon()?)
                .build()
                .map_err(|error| format!("failed to create tray icon: {error}"))?;

            Ok(Self {
                _tray: tray,
                show_id: show.id().clone(),
                translate_id: translate.id().clone(),
                quit_id: quit.id().clone(),
            })
        }

        pub fn poll(&self) -> Option<TrayCommand> {
            let mut command = None;

            while let Ok(event) = MenuEvent::receiver().try_recv() {
                if event.id == self.show_id {
                    command = Some(TrayCommand::Show);
                } else if event.id == self.translate_id {
                    command = Some(TrayCommand::Translate);
                } else if event.id == self.quit_id {
                    command = Some(TrayCommand::Quit);
                }
            }

            command
        }
    }

    fn make_icon() -> Result<tray_icon::Icon, String> {
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

        tray_icon::Icon::from_rgba(rgba, SIZE, SIZE)
            .map_err(|error| format!("failed to build tray icon: {error}"))
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub use platform::{Tray, TrayCommand};

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayCommand {
    Show,
    Translate,
    Quit,
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub struct Tray;

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
impl Tray {
    pub fn is_supported() -> bool {
        false
    }

    pub fn new(_tooltip: &str) -> Result<Self, String> {
        Ok(Self)
    }

    pub fn poll(&self) -> Option<TrayCommand> {
        None
    }
}
