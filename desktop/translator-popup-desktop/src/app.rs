use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use eframe::egui;
use translator_core::args::{Args, read_stdin};
use translator_core::detect;
use translator_core::languages;
use translator_core::settings::{persist_recent_targets, persist_source, persist_target};
use translator_core::update::ReleaseInfo;
use translator_service::domain::prompt::PromptStyle;
use translator_service::domain::translation::TranslationRequest;
use translator_service::engine::llama_cpp::LlamaCppEngine;

use crate::capture;
use crate::hotkey::Hotkey;
use crate::tray::{Tray, TrayCommand};

pub const WINDOW_SIZE: [f32; 2] = [520.0, 420.0];
const WINDOW_WIDTH: f32 = WINDOW_SIZE[0];
const MIN_WINDOW_HEIGHT: f32 = 240.0;
const MAX_WINDOW_FRACTION: f32 = 0.7;
const CARD_CHROME: f32 = 60.0;
const FOOTER_SPACE: f32 = 44.0;
const WINDOW_MARGIN: f32 = 12.0;
const ACCENT: egui::Color32 = egui::Color32::from_rgb(79, 124, 255);
const CARD_RADIUS: u8 = 16;
const COPIED_FEEDBACK: Duration = Duration::from_millis(1500);

#[derive(Clone)]
pub struct ServerPlan {
    pub bind_addr: String,
    pub model_name: String,
}

pub enum Startup {
    Loaded(Result<Arc<LlamaCppEngine>, String>),
    Download {
        dest: PathBuf,
        url: String,
        sha256: String,
        prompt_style: PromptStyle,
    },
}

enum StartupEvent {
    Progress { downloaded: u64, total: Option<u64> },
    Ready(Arc<LlamaCppEngine>),
    Failed(String),
}

enum Progress {
    Delta(String),
    Done(String),
    Error(String),
}

/// Progress and health of the embedded model, independent from translation.
enum ModelState {
    Downloading { downloaded: u64, total: Option<u64> },
    Ready,
    Failed(String),
}

/// Lifecycle of the current selection translation.
enum TranslationState {
    Idle,
    Empty,
    Waiting { text: String },
    Running {
        text: String,
        started: Instant,
        partial: String,
    },
    Done {
        text: String,
        translation: String,
        elapsed: Duration,
    },
    Failed {
        text: String,
        message: String,
    },
}

#[derive(Default)]
struct Actions {
    close_window: bool,
    quit: bool,
    retranslate: bool,
    copy: Option<String>,
    replace: Option<String>,
    swap: bool,
    dismiss_update: bool,
    dismiss_notice: bool,
    open_url: Option<String>,
}

pub struct PopupApp {
    args: Args,
    engine: Option<Arc<LlamaCppEngine>>,
    startup_receiver: Option<Receiver<StartupEvent>>,
    server_plan: Option<ServerPlan>,
    server_started: bool,
    model: ModelState,
    translation: TranslationState,
    source: String,
    detected_source: Option<&'static str>,
    target: String,
    recent_targets: Vec<String>,
    receiver: Option<Receiver<Progress>>,
    update: Option<ReleaseInfo>,
    update_receiver: Option<Receiver<ReleaseInfo>>,
    update_dismissed: bool,
    copied_at: Option<Instant>,
    replaced_at: Option<Instant>,
    replace_window: Option<isize>,
    notice: Option<String>,
    requested_height: Option<f32>,
    hotkey: Option<Hotkey>,
    hotkey_label: String,
    tray: Option<Tray>,
    window: Option<isize>,
    quit: bool,
}

fn system_fonts() -> Vec<(String, Vec<u8>)> {
    // One candidate group per script so a Chinese font (no Hangul) cannot
    // shadow the Korean fallback.
    let groups: Vec<Vec<PathBuf>> = if cfg!(target_os = "windows") {
        let windir = std::env::var_os("WINDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let fonts = windir.join("Fonts");

        [
            &["msyh.ttc", "msyh.ttf", "simhei.ttf", "simsun.ttc", "Deng.ttf"][..],
            &["malgun.ttf", "malgunbd.ttf", "batang.ttc", "gulim.ttc"][..],
            &["YuGothM.ttc", "meiryo.ttc", "msgothic.ttc"][..],
            &["LeelawUI.ttf", "Leelawad.ttf", "tahoma.ttf"][..],
        ]
        .iter()
        .map(|names| names.iter().map(|name| fonts.join(name)).collect())
        .collect()
    } else if cfg!(target_os = "macos") {
        [
            &[
                "/System/Library/Fonts/PingFang.ttc",
                "/System/Library/Fonts/STHeiti Medium.ttc",
                "/System/Library/Fonts/Hiragino Sans GB.ttc",
                "/Library/Fonts/Arial Unicode.ttf",
            ][..],
            &[
                "/System/Library/Fonts/AppleSDGothicNeo.ttc",
                "/System/Library/Fonts/AppleGothic.ttf",
            ][..],
            &[
                "/System/Library/Fonts/Hiragino Sans W3.ttc",
                "/System/Library/Fonts/Hiragino Sans GB.ttc",
            ][..],
            &[
                "/System/Library/Fonts/ThonburiUI.ttc",
                "/System/Library/Fonts/Thonburi.ttc",
                "/System/Library/Fonts/Ayuthaya.ttf",
            ][..],
        ]
        .iter()
        .map(|names| names.iter().map(PathBuf::from).collect())
        .collect()
    } else {
        vec![
            [
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
                "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
                "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
            ]
            .iter()
            .map(PathBuf::from)
            .collect(),
            [
                "/usr/share/fonts/truetype/noto/NotoSansThai-Regular.ttf",
                "/usr/share/fonts/opentype/noto/NotoSansThai-Regular.otf",
            ]
            .iter()
            .map(PathBuf::from)
            .collect(),
        ]
    };

    let mut fonts = Vec::new();

    for group in groups {
        if let Some(font) = group.iter().find_map(|path| {
            std::fs::read(path)
                .ok()
                .map(|bytes| (path.display().to_string(), bytes))
        }) {
            fonts.push(font);
        }
    }

    fonts
}

fn install_cjk_font(ctx: &egui::Context) {
    let fonts = system_fonts();
    if fonts.is_empty() {
        return;
    }

    let mut definitions = egui::FontDefinitions::default();

    for (name, bytes) in fonts {
        definitions
            .font_data
            .insert(name.clone(), Arc::new(egui::FontData::from_owned(bytes)));

        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            definitions
                .families
                .entry(family)
                .or_default()
                .push(name.clone());
        }
    }

    ctx.set_fonts(definitions);
}

fn configure_style(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::System);

    ctx.style_mut_of(egui::Theme::Dark, |style| tune_style(style, true));
    ctx.style_mut_of(egui::Theme::Light, |style| tune_style(style, false));
}

fn tune_style(style: &mut egui::Style, dark: bool) {
    style.spacing.item_spacing = egui::vec2(8.0, 10.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.spacing.interact_size.y = 30.0;
    style.spacing.menu_margin = egui::Margin::same(6);

    style.visuals.window_corner_radius = egui::CornerRadius::same(CARD_RADIUS);
    style.visuals.menu_corner_radius = egui::CornerRadius::same(10);
    style.visuals.selection.bg_fill = ACCENT.gamma_multiply(0.35);
    style.visuals.hyperlink_color = ACCENT;
    style.visuals.button_frame = true;

    let widgets = &mut style.visuals.widgets;
    widgets.noninteractive.corner_radius = egui::CornerRadius::same(8);
    widgets.inactive.corner_radius = egui::CornerRadius::same(8);
    widgets.hovered.corner_radius = egui::CornerRadius::same(8);
    widgets.active.corner_radius = egui::CornerRadius::same(8);
    widgets.open.corner_radius = egui::CornerRadius::same(8);

    if dark {
        style.visuals.panel_fill = egui::Color32::from_rgb(0x15, 0x17, 0x1B);
        style.visuals.window_fill = egui::Color32::from_rgb(0x1E, 0x21, 0x27);
        style.visuals.faint_bg_color = egui::Color32::from_rgb(0x26, 0x2A, 0x31);
        style.visuals.extreme_bg_color = egui::Color32::from_rgb(0x12, 0x14, 0x17);
        style.visuals.widgets.hovered.weak_bg_fill = egui::Color32::from_rgb(0x2E, 0x33, 0x3C);
        style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(0x33, 0x39, 0x44);
        style.visuals.widgets.active.bg_fill = ACCENT.gamma_multiply(0.85);
        style.visuals.widgets.active.weak_bg_fill = ACCENT.gamma_multiply(0.85);
        style.visuals.window_stroke =
            egui::Stroke::new(1.0, egui::Color32::from_white_alpha(18));
    } else {
        style.visuals.panel_fill = egui::Color32::from_rgb(0xF3, 0xF4, 0xF6);
        style.visuals.window_fill = egui::Color32::WHITE;
        style.visuals.faint_bg_color = egui::Color32::from_rgb(0xF3, 0xF4, 0xF7);
        style.visuals.extreme_bg_color = egui::Color32::from_rgb(0xEC, 0xEE, 0xF1);
        style.visuals.widgets.hovered.weak_bg_fill = egui::Color32::from_rgb(0xE9, 0xEC, 0xF1);
        style.visuals.window_stroke =
            egui::Stroke::new(1.0, egui::Color32::from_black_alpha(22));
    }
}

fn card_frame(ui: &egui::Ui) -> egui::Frame {
    let visuals = ui.visuals();

    if cfg!(target_os = "windows") {
        // The window is opaque on Windows (DWM rounds its corners), so the
        // card fills the whole window without margin or shadow.
        egui::Frame::new()
            .fill(visuals.window_fill)
            .inner_margin(egui::Margin::same(16))
    } else {
        egui::Frame::new()
            .fill(visuals.window_fill)
            .stroke(visuals.window_stroke)
            .corner_radius(CARD_RADIUS)
            .inner_margin(egui::Margin::same(16))
            .outer_margin(egui::Margin::same(14))
            .shadow(egui::epaint::Shadow {
                offset: [0, 6],
                blur: 20,
                spread: 0,
                color: egui::Color32::from_black_alpha(if visuals.dark_mode { 110 } else { 40 }),
            })
    }
}

#[cfg(target_os = "windows")]
fn window_handle(cc: &eframe::CreationContext<'_>) -> Option<isize> {
    use raw_window_handle::{HasWindowHandle as _, RawWindowHandle};

    let handle = cc.window_handle().ok()?;
    let RawWindowHandle::Win32(win) = handle.as_raw() else {
        return None;
    };

    Some(win.hwnd.get())
}

#[cfg(target_os = "windows")]
fn enable_native_window_style(hwnd: isize) {
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND,
    };

    let preference = DWMWCP_ROUND;
    unsafe {
        DwmSetWindowAttribute(
            hwnd as _,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            &preference as *const _ as *const core::ffi::c_void,
            std::mem::size_of_val(&preference) as u32,
        );
    }
}

fn max_window_height(ctx: &egui::Context) -> f32 {
    ctx.input(|input| input.viewport().monitor_size)
        .map(|size| size.y * MAX_WINDOW_FRACTION)
        .unwrap_or(760.0)
        .max(MIN_WINDOW_HEIGHT)
}

#[cfg(target_os = "windows")]
struct CursorPlacement {
    cursor: (f32, f32),
    work: (f32, f32, f32, f32),
}

#[cfg(target_os = "windows")]
fn monitor_work_area(
    monitor: windows_sys::Win32::Graphics::Gdi::HMONITOR,
) -> Option<(f32, f32, f32, f32)> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITORINFO};

    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        rcMonitor: RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        },
        rcWork: RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        },
        dwFlags: 0,
    };
    if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
        return None;
    }

    Some((
        info.rcWork.left as f32,
        info.rcWork.top as f32,
        info.rcWork.right as f32,
        info.rcWork.bottom as f32,
    ))
}

#[cfg(target_os = "windows")]
fn cursor_placement() -> Option<CursorPlacement> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONEAREST, MonitorFromPoint};
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut cursor = POINT { x: 0, y: 0 };
    if unsafe { GetCursorPos(&mut cursor) } == 0 {
        return None;
    }

    let monitor = unsafe {
        MonitorFromPoint(
            POINT {
                x: cursor.x,
                y: cursor.y,
            },
            MONITOR_DEFAULTTONEAREST,
        )
    };
    if monitor.is_null() {
        return None;
    }

    Some(CursorPlacement {
        cursor: (cursor.x as f32, cursor.y as f32),
        work: monitor_work_area(monitor)?,
    })
}

#[cfg(target_os = "windows")]
fn window_work_area(hwnd: isize) -> Option<(f32, f32, f32, f32)> {
    use windows_sys::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONEAREST, MonitorFromWindow};

    let monitor = unsafe { MonitorFromWindow(hwnd as _, MONITOR_DEFAULTTONEAREST) };
    if monitor.is_null() {
        return None;
    }

    monitor_work_area(monitor)
}

#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
fn fit_to_work_area(
    position: (f32, f32),
    size: egui::Vec2,
    work: (f32, f32, f32, f32),
    margin: f32,
) -> (f32, f32) {
    let (left, top, right, bottom) = work;
    let min_x = left + margin;
    let min_y = top + margin;
    let max_x = (right - size.x - margin).max(min_x);
    let max_y = (bottom - size.y - margin).max(min_y);

    (
        position.0.clamp(min_x, max_x),
        position.1.clamp(min_y, max_y),
    )
}

#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
fn clamp_to_work_area(
    cursor: (f32, f32),
    size: egui::Vec2,
    work: (f32, f32, f32, f32),
    margin: f32,
) -> (f32, f32) {
    fit_to_work_area((cursor.0 + margin, cursor.1 + margin), size, work, margin)
}

impl PopupApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        args: Args,
        hotkey_spec: &str,
        startup: Startup,
        server_plan: Option<ServerPlan>,
        check_updates: bool,
    ) -> Self {
        install_cjk_font(&cc.egui_ctx);
        configure_style(&cc.egui_ctx);

        #[cfg(target_os = "windows")]
        let window = window_handle(cc);
        #[cfg(not(target_os = "windows"))]
        let window = None;

        #[cfg(target_os = "windows")]
        if let Some(hwnd) = window {
            enable_native_window_style(hwnd);
        }

        let recent_targets = languages::recent_target_list(&args.recent_targets, &args.target);

        let mut app = Self {
            source: args.source.clone(),
            detected_source: None,
            target: args.target.clone(),
            recent_targets,
            args,
            engine: None,
            startup_receiver: None,
            server_plan,
            server_started: false,
            model: ModelState::Ready,
            translation: TranslationState::Idle,
            receiver: None,
            update: None,
            update_receiver: None,
            update_dismissed: false,
            copied_at: None,
            replaced_at: None,
            replace_window: None,
            notice: None,
            requested_height: None,
            hotkey: None,
            hotkey_label: hotkey_spec.to_string(),
            tray: None,
            window,
            quit: false,
        };

        if check_updates {
            let (sender, receiver) = channel::<ReleaseInfo>();
            app.update_receiver = Some(receiver);
            spawn_update_check(sender);
        }

        let mut show_on_start = false;

        match startup {
            Startup::Loaded(Ok(engine)) => {
                app.engine = Some(engine);
                app.maybe_start_server();
            }
            Startup::Loaded(Err(error)) => {
                app.model = ModelState::Failed(format!("模型加载失败：{error}"));
                show_on_start = true;
            }
            Startup::Download {
                dest,
                url,
                sha256,
                prompt_style,
            } => {
                let (sender, receiver) = channel();
                app.startup_receiver = Some(receiver);
                app.model = ModelState::Downloading {
                    downloaded: 0,
                    total: None,
                };
                spawn_startup(dest, url, sha256, prompt_style, sender);
                show_on_start = true;
            }
        }

        match Hotkey::register(hotkey_spec) {
            Ok(hotkey) => app.hotkey = Some(hotkey),
            Err(error) => app.notice = Some(error),
        }

        match Tray::new(&format!("OpenTranslator（{hotkey_spec}）")) {
            Ok(tray) => app.tray = Some(tray),
            Err(error) => {
                if app.notice.is_none() {
                    app.notice = Some(error);
                }
            }
        }

        if app.args.stdin {
            let initial = read_stdin().unwrap_or_default();

            if !initial.is_empty() {
                app.begin_translation(initial);
                show_on_start = true;
            }
        }

        if show_on_start {
            app.show_window(&cc.egui_ctx);
        }

        app
    }

    fn trigger(&mut self) {
        self.replace_window = capture::foreground_window();

        match capture::capture_selection() {
            Ok(text) if text.trim().is_empty() => {
                self.receiver = None;
                self.detected_source = None;
                self.translation = TranslationState::Empty;
            }
            Ok(text) => self.begin_translation(text),
            Err(error) => {
                self.receiver = None;
                self.detected_source = None;
                self.translation = TranslationState::Failed {
                    text: String::new(),
                    message: error,
                };
            }
        }
    }

    fn show_window(&self, ctx: &egui::Context) {
        self.place_near_cursor(ctx);
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    fn place_near_cursor(&self, ctx: &egui::Context) {
        #[cfg(target_os = "windows")]
        {
            let Some(placement) = cursor_placement() else {
                return;
            };

            let ppp = ctx.pixels_per_point().max(0.1);
            let size = ctx
                .input(|input| input.viewport().inner_rect)
                .map(|rect| rect.size())
                .unwrap_or_else(|| {
                    egui::vec2(WINDOW_WIDTH, self.requested_height.unwrap_or(WINDOW_SIZE[1]))
                });

            let (x, y) = clamp_to_work_area(
                (placement.cursor.0 / ppp, placement.cursor.1 / ppp),
                size,
                (
                    placement.work.0 / ppp,
                    placement.work.1 / ppp,
                    placement.work.2 / ppp,
                    placement.work.3 / ppp,
                ),
                WINDOW_MARGIN,
            );

            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
        }

        #[cfg(not(target_os = "windows"))]
        let _ = (self, ctx);
    }

    fn fit_vertically(&self, ctx: &egui::Context, height: f32) {
        #[cfg(target_os = "windows")]
        {
            let Some(window) = self.window else {
                return;
            };
            let Some(work) = window_work_area(window) else {
                return;
            };
            let Some(rect) = ctx.input(|input| input.viewport().outer_rect) else {
                return;
            };

            let ppp = ctx.pixels_per_point().max(0.1);
            let (x, y) = fit_to_work_area(
                (rect.min.x, rect.min.y),
                egui::vec2(WINDOW_WIDTH, height),
                (
                    work.0 / ppp,
                    work.1 / ppp,
                    work.2 / ppp,
                    work.3 / ppp,
                ),
                WINDOW_MARGIN,
            );

            if (x - rect.min.x).abs() > 0.5 || (y - rect.min.y).abs() > 0.5 {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
            }
        }

        #[cfg(not(target_os = "windows"))]
        let _ = (self, ctx, height);
    }

    fn begin_translation(&mut self, text: String) {
        self.detected_source = None;

        let Some(engine) = self.engine.clone() else {
            self.translation = match &self.model {
                ModelState::Failed(message) => TranslationState::Failed {
                    text: String::new(),
                    message: message.clone(),
                },
                _ => TranslationState::Waiting { text },
            };
            return;
        };

        self.translation = TranslationState::Running {
            text: text.clone(),
            started: Instant::now(),
            partial: String::new(),
        };

        let (sender, receiver) = channel();
        self.receiver = Some(receiver);

        let (source, detected) = detect::resolve_source(&self.source, &text);
        self.detected_source = detected;

        spawn_worker(engine, source, self.target.clone(), text, sender);
    }

    fn retranslate(&mut self) {
        let text = match &self.translation {
            TranslationState::Done { text, .. } => Some(text.clone()),
            TranslationState::Failed { text, .. } if !text.is_empty() => Some(text.clone()),
            _ => None,
        };

        if let Some(text) = text {
            self.begin_translation(text);
        }
    }

    fn can_retranslate(&self) -> bool {
        match &self.translation {
            TranslationState::Done { .. } => true,
            TranslationState::Failed { text, .. } => !text.is_empty(),
            _ => false,
        }
    }

    fn change_source(&mut self, source: String) {
        if self.source == source {
            return;
        }

        self.source = source.clone();
        persist_source(&source);

        if self.last_text().is_some() {
            self.retranslate();
        }
    }

    fn change_target(&mut self, target: String) {
        if self.target == target {
            return;
        }

        let previous = std::mem::replace(&mut self.target, target.clone());
        persist_target(&target);
        self.remember_target(&previous);

        if self.last_text().is_some() {
            self.retranslate();
        }
    }

    fn remember_target(&mut self, previous: &str) {
        if previous == self.target {
            return;
        }

        self.recent_targets
            .retain(|entry| entry != &self.target && entry != previous);
        self.recent_targets.insert(0, previous.to_string());
        self.recent_targets.truncate(3);
        persist_recent_targets(&self.recent_targets);
    }

    fn select_recent_target(&mut self, index: usize) -> bool {
        let Some(target) = self.recent_targets.get(index).cloned() else {
            return false;
        };

        self.change_target(target);

        true
    }

    fn swap_languages(&mut self) {
        let Some((next_source, next_target)) =
            languages::swapped_pair(&self.source, &self.target, self.detected_source)
        else {
            return;
        };

        let translated = match &self.translation {
            TranslationState::Done { translation, .. } if !translation.trim().is_empty() => {
                Some(translation.clone())
            }
            _ => None,
        };

        let old_target = std::mem::replace(&mut self.target, next_target);
        self.source = next_source;
        persist_source(&self.source);
        persist_target(&self.target);
        self.remember_target(&old_target);

        // Swap the displayed texts too: the translation becomes the new source.
        if let Some(translation) = translated {
            self.begin_translation(translation);
        } else if self.last_text().is_some() {
            self.retranslate();
        }
    }

    fn hide(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
    }

    fn close_window(&mut self, ctx: &egui::Context) {
        if self.can_restore() {
            self.hide(ctx);
        } else {
            self.quit = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn maybe_start_server(&mut self) {
        if self.server_started {
            return;
        }

        let (Some(engine), Some(plan)) = (self.engine.clone(), self.server_plan.clone()) else {
            return;
        };

        match crate::server::start(engine, plan.bind_addr, plan.model_name) {
            Ok(()) => self.server_started = true,
            Err(crate::server::ServerError::AddrInUse(address)) => {
                self.notice = Some(format!("扩展服务未启动：{address} 已被其他服务占用"));
            }
            Err(error) => {
                self.notice = Some(format!("扩展服务启动失败：{error}"));
            }
        }
    }

    fn can_restore(&self) -> bool {
        (self.hotkey.is_some() && Hotkey::is_supported())
            || (self.tray.is_some() && Tray::is_supported())
    }

    fn tray_active(&self) -> bool {
        self.tray.is_some() && Tray::is_supported()
    }

    fn title(&self) -> String {
        format!(
            "OpenTranslator ({} → {})",
            languages::source_label(&self.source),
            languages::label(&self.target)
        )
    }

    fn source_text(&self) -> Option<&str> {
        match &self.translation {
            TranslationState::Idle | TranslationState::Empty => None,
            TranslationState::Waiting { text }
            | TranslationState::Running { text, .. }
            | TranslationState::Done { text, .. }
            | TranslationState::Failed { text, .. } => {
                if text.is_empty() {
                    None
                } else {
                    Some(text)
                }
            }
        }
    }

    fn last_text(&self) -> Option<&str> {
        match &self.translation {
            TranslationState::Waiting { text }
            | TranslationState::Running { text, .. }
            | TranslationState::Done { text, .. }
            | TranslationState::Failed { text, .. } => {
                if text.is_empty() {
                    None
                } else {
                    Some(text)
                }
            }
            TranslationState::Idle | TranslationState::Empty => None,
        }
    }

    fn translation_text(&self) -> Option<&str> {
        match &self.translation {
            TranslationState::Done { translation, .. } if !translation.is_empty() => {
                Some(translation)
            }
            _ => None,
        }
    }

    fn status_text(&self) -> Option<String> {
        match &self.translation {
            TranslationState::Waiting { .. } => Some("模型准备中，就绪后自动翻译".to_string()),
            TranslationState::Running { text, .. } => {
                Some(format!("翻译中… {} 字符", text.chars().count()))
            }
            TranslationState::Done {
                text, elapsed, ..
            } => Some(format!(
                "{} 字符 · {:.0} ms",
                text.chars().count(),
                elapsed.as_secs_f64() * 1000.0
            )),
            _ => None,
        }
    }

    fn copied_recently(&self) -> bool {
        self.copied_at
            .map(|at| at.elapsed() < COPIED_FEEDBACK)
            .unwrap_or(false)
    }

    fn replaced_recently(&self) -> bool {
        self.replaced_at
            .map(|at| at.elapsed() < COPIED_FEEDBACK)
            .unwrap_or(false)
    }

    fn replace_original(&mut self, text: String) {
        #[cfg(target_os = "windows")]
        {
            let Some(window) = self.replace_window else {
                self.notice = Some("没有可替换的原窗口".to_string());
                return;
            };

            match capture::replace_selection(window, &text) {
                Ok(()) => self.replaced_at = Some(Instant::now()),
                Err(error) => self.notice = Some(error),
            }
        }

        #[cfg(not(target_os = "windows"))]
        let _ = text;
    }
}

impl eframe::App for PopupApp {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        if cfg!(target_os = "windows") {
            // Opaque window: paint the card color everywhere so no black
            // transparent pixels are composited by DWM.
            egui::Rgba::from(visuals.window_fill).to_array()
        } else {
            egui::Rgba::TRANSPARENT.to_array()
        }
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let hotkey_pressed = self
            .hotkey
            .as_ref()
            .map(|hotkey| hotkey.pressed())
            .unwrap_or(false);

        if hotkey_pressed {
            self.trigger();
            self.show_window(ctx);
        }

        if let Some(info) = self
            .update_receiver
            .as_ref()
            .and_then(|receiver| receiver.try_iter().next())
        {
            if let Some(tray) = &self.tray {
                tray.set_update(&format!("有新版本 v{}", info.version));
            }

            self.update = Some(info);
        }

        let tray_command = self.tray.as_ref().and_then(|tray| tray.poll());

        match tray_command {
            Some(TrayCommand::Show) => self.show_window(ctx),
            Some(TrayCommand::Translate) => {
                self.trigger();
                self.show_window(ctx);
            }
            Some(TrayCommand::Update) => {
                if let Some(info) = &self.update {
                    open_url(&info.url);
                }
            }
            Some(TrayCommand::Quit) => {
                self.quit = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            None => {}
        }

        let messages: Vec<Progress> = self
            .receiver
            .as_ref()
            .map(|receiver| receiver.try_iter().collect())
            .unwrap_or_default();

        for message in messages {
            match message {
                Progress::Delta(piece) => {
                    if let TranslationState::Running { partial, .. } = &mut self.translation {
                        partial.push_str(&piece);
                    }
                }
                Progress::Done(translation) => {
                    let (text, elapsed) = match &self.translation {
                        TranslationState::Running { text, started, .. } => {
                            (text.clone(), started.elapsed())
                        }
                        TranslationState::Waiting { text } => (text.clone(), Duration::ZERO),
                        _ => (String::new(), Duration::ZERO),
                    };

                    self.translation = TranslationState::Done {
                        text,
                        translation,
                        elapsed,
                    };
                }
                Progress::Error(error) => {
                    let text = self.last_text().unwrap_or_default().to_string();
                    let retryable = !text.is_empty();
                    self.translation = TranslationState::Failed {
                        text: if retryable { text } else { String::new() },
                        message: error,
                    };
                }
            }
        }

        let startup_messages: Vec<StartupEvent> = self
            .startup_receiver
            .as_ref()
            .map(|receiver| receiver.try_iter().collect())
            .unwrap_or_default();

        for message in startup_messages {
            match message {
                StartupEvent::Progress { downloaded, total } => {
                    self.model = ModelState::Downloading { downloaded, total };
                }
                StartupEvent::Ready(engine) => {
                    self.engine = Some(engine);
                    self.model = ModelState::Ready;
                    self.maybe_start_server();

                    if let TranslationState::Waiting { text } =
                        std::mem::replace(&mut self.translation, TranslationState::Idle)
                    {
                        self.begin_translation(text);
                    }
                }
                StartupEvent::Failed(error) => {
                    self.model = ModelState::Failed(error);
                }
            }
        }

        if ctx.input(|input| input.viewport().close_requested()) {
            if self.quit || !self.can_restore() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.hide(ctx);
            }
        }

        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.close_window(ctx);
        }

        if ctx.input(|input| {
            input.modifiers.command && input.key_pressed(egui::Key::Enter)
        }) {
            self.retranslate();
        }

        if ctx.input(|input| {
            input.modifiers.command
                && input.modifiers.shift
                && input.key_pressed(egui::Key::C)
        }) {
            if let Some(text) = self.translation_text().map(str::to_string) {
                ctx.copy_text(text);
                self.copied_at = Some(Instant::now());
            }
        }

        for (index, key) in [egui::Key::Num1, egui::Key::Num2, egui::Key::Num3]
            .iter()
            .enumerate()
        {
            let pressed = ctx.input(|input| {
                input.modifiers.command
                    && !input.modifiers.shift
                    && input.key_pressed(*key)
            });

            if pressed && self.select_recent_target(index) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.title()));
            }
        }

        // Keep polling while the window is hidden (hotkey events arrive via
        // `logic`); animate the skeleton/caret faster while a translation runs.
        let repaint = if matches!(&self.translation, TranslationState::Running { .. }) {
            Duration::from_millis(33)
        } else {
            Duration::from_millis(100)
        };
        ctx.request_repaint_after(repaint);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let mut actions = Actions::default();
        let mut change_source: Option<String> = None;
        let mut change_target: Option<String> = None;
        let max_window_height = max_window_height(&ctx);
        let mut content_height = 0.0_f32;

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::TRANSPARENT))
            .show(ui, |ui| {
                card_frame(ui).show(ui, |ui| {
                    ui.set_min_width(ui.available_width());

                    (change_source, change_target) = self.header(ui, &ctx, &mut actions);
                    self.banners(ui, &mut actions);
                    content_height = self.body(ui, &mut actions, max_window_height);
                });
            });

        let desired_height = (content_height + CARD_CHROME)
            .clamp(MIN_WINDOW_HEIGHT, max_window_height);

        if (desired_height - self.requested_height.unwrap_or(-1.0)).abs() > 2.0 {
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                WINDOW_WIDTH,
                desired_height,
            )));
            self.requested_height = Some(desired_height);
            self.fit_vertically(&ctx, desired_height);
        }

        if let Some(source) = change_source {
            self.change_source(source);
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.title()));
        }

        if let Some(target) = change_target {
            self.change_target(target);
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.title()));
        }

        if actions.swap {
            self.swap_languages();
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.title()));
        }

        if actions.retranslate {
            self.retranslate();
        }

        if let Some(text) = actions.copy {
            ctx.copy_text(text);
            self.copied_at = Some(Instant::now());
        }

        if let Some(text) = actions.replace {
            self.replace_original(text);
        }

        if actions.dismiss_update {
            self.update_dismissed = true;
        }

        if actions.dismiss_notice {
            self.notice = None;
        }

        if let Some(url) = actions.open_url {
            open_url(&url);
        }

        if actions.close_window {
            self.close_window(&ctx);
        }

        if actions.quit {
            self.quit = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

impl PopupApp {
    fn header(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        actions: &mut Actions,
    ) -> (Option<String>, Option<String>) {
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), 34.0),
            egui::Sense::drag(),
        );

        if response.drag_started() {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }

        let mut change_source = None;
        let mut change_target = None;

        let mut header = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        header.spacing_mut().item_spacing.x = 8.0;

        header.label(
            egui::RichText::new("●")
                .size(11.0)
                .color(self.status_color()),
        );
        header.label(egui::RichText::new("OpenTranslator").strong().size(15.0));

        let source_text = languages::source_label(&self.source).to_string();
        egui::ComboBox::from_id_salt("source_lang")
            .selected_text(egui::RichText::new(source_text).size(13.0))
            .width(88.0)
            .show_ui(&mut header, |ui| {
                for (code, name) in languages::source_options() {
                    let is_selected = self.source == code;
                    if ui.selectable_label(is_selected, name).clicked() && !is_selected {
                        change_source = Some(code.to_string());
                    }
                }
            });

        if self.source == languages::AUTO_CODE {
            if let Some(tag) = self.detected_source {
                header.label(
                    egui::RichText::new(format!("（{}）", languages::label(tag)))
                        .weak()
                        .size(12.0),
                );
            }
        }

        let swap_enabled = match self.source.as_str() {
            languages::AUTO_CODE => self
                .detected_source
                .map(|tag| tag != self.target)
                .unwrap_or(false),
            source => source != self.target,
        };

        let (swap_rect, swap_response) =
            header.allocate_exact_size(egui::vec2(26.0, 24.0), egui::Sense::click());
        let swap_color = if !swap_enabled {
            ui.visuals().weak_text_color()
        } else if swap_response.hovered() {
            ui.visuals().strong_text_color()
        } else {
            ui.visuals().text_color()
        };
        let center = swap_rect.center();
        let half = 9.0;
        let head = 4.0;
        let stroke = egui::Stroke::new(1.6, swap_color);
        let painter = header.painter();

        for (offset, forward) in [(-3.0_f32, true), (3.0, false)] {
            let y = center.y + offset;
            let left = egui::pos2(center.x - half, y);
            let right = egui::pos2(center.x + half, y);
            let (tip, back_x) = if forward {
                (right, center.x + half - head)
            } else {
                (left, center.x - half + head)
            };

            painter.line_segment([left, right], stroke);
            painter.line_segment([egui::pos2(back_x, y - head), tip], stroke);
            painter.line_segment([egui::pos2(back_x, y + head), tip], stroke);
        }

        if swap_enabled {
            let swap_response = swap_response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text("互换源语言与目标语言");

            if swap_response.clicked() {
                actions.swap = true;
            }
        }

        let selected = languages::label(&self.target).to_string();
        egui::ComboBox::from_id_salt("target_lang")
            .selected_text(egui::RichText::new(selected).size(13.0))
            .width(104.0)
            .show_ui(&mut header, |ui| {
                for (code, name) in languages::LANGUAGES {
                    let is_selected = self.target == *code;
                    if ui.selectable_label(is_selected, *name).clicked() && !is_selected {
                        change_target = Some((*code).to_string());
                    }
                }
            });

        header.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let close = ui.add(
                egui::Button::new(egui::RichText::new("×").size(17.0).weak())
                    .frame(false)
                    .min_size(egui::vec2(26.0, 26.0)),
            );

            if close.on_hover_text("隐藏").clicked() {
                actions.close_window = true;
            }
        });

        (change_source, change_target)
    }

    fn status_color(&self) -> egui::Color32 {
        match &self.model {
            ModelState::Failed(_) => egui::Color32::from_rgb(0xE0, 0x5A, 0x5A),
            ModelState::Downloading { .. } => egui::Color32::from_rgb(0xE0, 0xA5, 0x3C),
            ModelState::Ready => match &self.translation {
                TranslationState::Failed { .. } => egui::Color32::from_rgb(0xE0, 0x5A, 0x5A),
                TranslationState::Running { .. } | TranslationState::Waiting { .. } => ACCENT,
                _ => egui::Color32::from_rgb(0x3D, 0xB5, 0x7A),
            },
        }
    }

    fn banners(&self, ui: &mut egui::Ui, actions: &mut Actions) {
        if let Some(notice) = &self.notice {
            egui::Frame::new()
                .fill(ui.visuals().warn_fg_color.gamma_multiply(0.12))
                .corner_radius(10)
                .inner_margin(egui::Margin::symmetric(12, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(egui::RichText::new(notice).size(12.5))
                                .wrap(),
                        );

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("忽略").clicked() {
                                actions.dismiss_notice = true;
                            }
                        });
                    });
                });

            ui.add_space(4.0);
        }

        if self.update_dismissed {
            return;
        }

        let Some(info) = self.update.clone() else {
            return;
        };

        egui::Frame::new()
            .fill(ACCENT.gamma_multiply(0.14))
            .corner_radius(10)
            .inner_margin(egui::Margin::symmetric(12, 6))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("发现新版本 v{}", info.version))
                            .strong()
                            .size(13.0),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("忽略").clicked() {
                            actions.dismiss_update = true;
                        }

                        if ui.small_button("查看").clicked() {
                            actions.open_url = Some(info.url.clone());
                        }
                    });
                });
            });

        ui.add_space(4.0);
    }

    fn body(&mut self, ui: &mut egui::Ui, actions: &mut Actions, max_window_height: f32) -> f32 {
        let source = self.source_text().map(str::to_string);

        if let Some(source) = source {
            egui::Frame::new()
                .fill(ui.visuals().faint_bg_color)
                .corner_radius(10)
                .inner_margin(egui::Margin::symmetric(12, 8))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("source_scroll")
                        .max_height(56.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(
                                    egui::RichText::new(source).weak().size(13.0),
                                )
                                .selectable(false)
                                .wrap(),
                            );
                        });
                });

            ui.add_space(6.0);
        }

        let above = ui.min_rect().height();
        let translation_max =
            (max_window_height - CARD_CHROME - above - FOOTER_SPACE).max(96.0);

        let mut translation_height = 0.0_f32;

        egui::ScrollArea::vertical()
            .id_salt("translation_scroll")
            .auto_shrink([false, true])
            .max_height(translation_max)
            .show(ui, |ui| {
                self.translation_area(ui, actions);
                translation_height = ui.min_rect().height();
            });

        let translation_height = translation_height.min(translation_max);

        ui.add_space(6.0);
        let footer_top = ui.min_rect().height();
        self.footer(ui, actions);
        let footer_height = ui.min_rect().height() - footer_top;

        above + translation_height + 6.0 + footer_height
    }

    fn translation_area(&mut self, ui: &mut egui::Ui, actions: &mut Actions) {
        match &self.translation {
            TranslationState::Done { translation, .. } => {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(translation).size(17.0).line_height(Some(24.0)),
                    )
                    .selectable(true)
                    .wrap(),
                );
            }
            TranslationState::Running { partial, .. } if partial.is_empty() => {
                let time = ui.input(|input| input.time);
                let pulse = ((time * 2.2).sin() * 0.5 + 0.5) as f32;
                let color = ui
                    .visuals()
                    .text_color()
                    .gamma_multiply(0.14 + 0.16 * pulse);

                ui.add_space(8.0);
                for fraction in [1.0_f32, 0.86, 0.6] {
                    let (rect, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width() * fraction, 14.0),
                        egui::Sense::hover(),
                    );
                    ui.painter()
                        .rect_filled(rect, egui::CornerRadius::same(7), color);
                    ui.add_space(10.0);
                }
            }
            TranslationState::Running { partial, .. } => {
                let time = ui.input(|input| input.time);
                let pulse = ((time * 2.4).sin() * 0.5 + 0.5) as f32;
                let caret = egui::Color32::from_rgba_unmultiplied(
                    ACCENT.r(),
                    ACCENT.g(),
                    ACCENT.b(),
                    (60.0 + 180.0 * pulse) as u8,
                );

                let mut job = egui::text::LayoutJob::default();
                job.wrap.max_width = ui.available_width();
                job.append(
                    partial,
                    0.0,
                    egui::TextFormat {
                        font_id: egui::FontId::proportional(17.0),
                        line_height: Some(24.0),
                        color: ui.visuals().text_color(),
                        ..Default::default()
                    },
                );
                job.append(
                    "▍",
                    0.0,
                    egui::TextFormat {
                        font_id: egui::FontId::proportional(17.0),
                        line_height: Some(24.0),
                        color: caret,
                        ..Default::default()
                    },
                );

                ui.add(egui::Label::new(job).selectable(true).wrap());
            }
            TranslationState::Failed { text, message } => {
                self.error_card(ui, message, !text.is_empty(), actions);
            }
            TranslationState::Waiting { .. }
            | TranslationState::Idle
            | TranslationState::Empty => match &self.model {
                ModelState::Downloading { downloaded, total } => {
                    self.download_card(ui, *downloaded, *total);
                }
                ModelState::Failed(message) => {
                    let message = message.clone();
                    self.error_card(ui, &message, false, actions);
                }
                ModelState::Ready => match &self.translation {
                    TranslationState::Empty => {
                        ui.vertical_centered(|ui| {
                            ui.add_space(18.0);
                            ui.label(egui::RichText::new("未选中文本").size(15.0));
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new("请在其它应用中选中要翻译的内容")
                                    .weak()
                                    .size(12.0),
                            );
                        });
                    }
                    _ => {
                        ui.vertical_centered(|ui| {
                            ui.add_space(18.0);
                            ui.label(egui::RichText::new("等待划词").size(15.0).weak());
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new(format!(
                                    "按 {} 翻译选中文本",
                                    self.hotkey_label
                                ))
                                .weak()
                                .size(12.0),
                            );
                        });
                    }
                },
            },
        }
    }

    fn download_card(&self, ui: &mut egui::Ui, downloaded: u64, total: Option<u64>) {
        const MB: f64 = 1_000_000.0;

        ui.vertical_centered(|ui| {
            ui.add_space(12.0);
            ui.spinner();
            ui.add_space(6.0);
            ui.label(egui::RichText::new("正在下载模型").strong().size(14.0));
            ui.add_space(4.0);

            match total.filter(|total| *total > 0) {
                Some(total) => {
                    let fraction = (downloaded as f32 / total as f32).clamp(0.0, 1.0);
                    ui.add(
                        egui::ProgressBar::new(fraction)
                            .show_percentage()
                            .animate(true)
                            .desired_width(ui.available_width().min(320.0)),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(format!(
                            "{:.0} / {:.0} MB · 完成后自动翻译",
                            downloaded as f64 / MB,
                            total as f64 / MB
                        ))
                        .weak()
                        .size(12.0),
                    );
                }
                None => {
                    ui.label(
                        egui::RichText::new(format!(
                            "已下载 {:.0} MB · 完成后自动翻译",
                            downloaded as f64 / MB
                        ))
                        .weak()
                        .size(12.0),
                    );
                }
            }
        });
    }

    fn error_card(
        &self,
        ui: &mut egui::Ui,
        message: &str,
        retryable: bool,
        actions: &mut Actions,
    ) {
        egui::Frame::new()
            .fill(ui.visuals().error_fg_color.gamma_multiply(0.12))
            .corner_radius(10)
            .inner_margin(egui::Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new("出错了")
                        .strong()
                        .size(13.0)
                        .color(ui.visuals().error_fg_color),
                );
                ui.add_space(2.0);
                ui.label(egui::RichText::new(message).size(13.0));

                if retryable {
                    ui.add_space(6.0);

                    if ui.button("重试").clicked() {
                        actions.retranslate = true;
                    }
                }
            });
    }

    fn footer(&mut self, ui: &mut egui::Ui, actions: &mut Actions) {
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            if let Some(status) = self.status_text() {
                ui.label(egui::RichText::new(status).weak().size(12.0));
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let copied = self.copied_recently();
                let enabled = self.translation_text().is_some();

                let label = if copied { "已复制 ✓" } else { "复制译文" };
                let copy = egui::Button::new(egui::RichText::new(label).size(13.0).color(
                    if enabled {
                        egui::Color32::WHITE
                    } else {
                        ui.visuals().weak_text_color()
                    },
                ))
                .fill(if enabled {
                    ACCENT
                } else {
                    ui.visuals().faint_bg_color
                });

                if ui.add_enabled(enabled, copy).clicked() {
                    actions.copy = self.translation_text().map(str::to_string);
                }

                if cfg!(target_os = "windows")
                    && self.replace_window.is_some()
                    && ui
                        .add_enabled(
                            enabled,
                            egui::Button::new(egui::RichText::new(if self.replaced_recently() {
                                "已替换 ✓"
                            } else {
                                "替换原文"
                            })
                            .size(13.0)),
                        )
                        .clicked()
                {
                    actions.replace = self.translation_text().map(str::to_string);
                }

                if self.can_retranslate()
                    && ui
                        .add(egui::Button::new(
                            egui::RichText::new("重新翻译").size(13.0),
                        ))
                        .clicked()
                {
                    actions.retranslate = true;
                }

                if !self.tray_active()
                    && ui
                        .add(egui::Button::new(egui::RichText::new("退出").size(13.0)))
                        .clicked()
                {
                    actions.quit = true;
                }
            });
        });
    }
}

fn spawn_startup(
    dest: PathBuf,
    url: String,
    sha256: String,
    prompt_style: PromptStyle,
    sender: Sender<StartupEvent>,
) {
    std::thread::spawn(move || {
        let client = match translator_core::models::download_client() {
            Ok(client) => client,
            Err(error) => {
                let _ = sender.send(StartupEvent::Failed(format!("下载初始化失败：{error}")));
                return;
            }
        };

        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                let _ = sender.send(StartupEvent::Failed(format!("运行时启动失败：{error}")));
                return;
            }
        };

        let result = runtime.block_on(translator_core::models::download(
            &client,
            &url,
            &dest,
            Some(&sha256),
            |downloaded, total| {
                let _ = sender.send(StartupEvent::Progress { downloaded, total });
            },
        ));

        if let Err(error) = result {
            let _ = sender.send(StartupEvent::Failed(format!("模型下载失败：{error}")));
            return;
        }

        match LlamaCppEngine::load(
            dest.to_string_lossy().as_ref(),
            prompt_style,
            crate::DEFAULT_N_CTX,
        ) {
            Ok(engine) => {
                let _ = sender.send(StartupEvent::Ready(Arc::new(engine)));
            }
            Err(error) => {
                let _ = sender.send(StartupEvent::Failed(format!("模型加载失败：{error}")));
            }
        }
    });
}

fn spawn_update_check(sender: Sender<ReleaseInfo>) {
    std::thread::spawn(move || {
        let client = match translator_core::translate::build_client() {
            Ok(client) => client,
            Err(_) => return,
        };

        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(_) => return,
        };

        if let Ok(Some(info)) = runtime.block_on(translator_core::update::check(
            &client,
            translator_core::update::current_version(),
            &translator_core::update::api_url(),
        )) {
            let _ = sender.send(info);
        }
    });
}

pub fn open_url(url: &str) {
    #[cfg(target_os = "windows")]
    let (program, args): (&str, Vec<&str>) = ("cmd", vec!["/C", "start", "", url]);
    #[cfg(target_os = "macos")]
    let (program, args): (&str, Vec<&str>) = ("open", vec![url]);
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let (program, args): (&str, Vec<&str>) = ("xdg-open", vec![url]);

    let _ = std::process::Command::new(program).args(args).spawn();
}

fn spawn_worker(
    engine: Arc<LlamaCppEngine>,
    source: String,
    target: String,
    text: String,
    sender: Sender<Progress>,
) {
    std::thread::spawn(move || {
        let request = TranslationRequest { text, source, target };
        let deltas = sender.clone();

        let result = engine.translate_blocking_streaming(&request, move |piece| {
            let _ = deltas.send(Progress::Delta(piece.to_string()));
        });

        let _ = match result {
            Ok(result) => sender.send(Progress::Done(result.translated_text)),
            Err(error) => sender.send(Progress::Error(error.to_string())),
        };
    });
}

#[cfg(test)]
mod placement_tests {
    use super::{clamp_to_work_area, fit_to_work_area};
    use eframe::egui;

    const WORK: (f32, f32, f32, f32) = (0.0, 0.0, 1920.0, 1080.0);

    #[test]
    fn keeps_the_window_position_when_it_fits() {
        let (x, y) = fit_to_work_area((100.0, 200.0), egui::vec2(400.0, 300.0), WORK, 12.0);

        assert_eq!((x, y), (100.0, 200.0));
    }

    #[test]
    fn lifts_the_window_when_it_grows_past_the_bottom() {
        let (x, y) = fit_to_work_area((100.0, 900.0), egui::vec2(400.0, 300.0), WORK, 12.0);

        assert_eq!((x, y), (100.0, 768.0));
    }

    #[test]
    fn pulls_the_window_back_from_the_right_edge() {
        let (x, y) = fit_to_work_area((1800.0, 200.0), egui::vec2(400.0, 300.0), WORK, 12.0);

        assert_eq!((x, y), (1508.0, 200.0));
    }

    #[test]
    fn places_below_right_of_the_cursor() {
        let (x, y) = clamp_to_work_area((100.0, 200.0), egui::vec2(400.0, 300.0), WORK, 12.0);

        assert_eq!((x, y), (112.0, 212.0));
    }

    #[test]
    fn clamps_the_window_into_the_work_area() {
        let (x, y) = clamp_to_work_area((1900.0, 1070.0), egui::vec2(400.0, 300.0), WORK, 12.0);

        assert_eq!((x, y), (1508.0, 768.0));
    }

    #[test]
    fn keeps_an_oversized_window_at_the_origin() {
        let (x, y) = clamp_to_work_area(
            (500.0, 500.0),
            egui::vec2(4000.0, 3000.0),
            WORK,
            12.0,
        );

        assert_eq!((x, y), (12.0, 12.0));
    }

    #[test]
    fn handles_monitors_left_of_the_primary() {
        let work = (-1920.0, 0.0, 0.0, 1080.0);
        let (x, y) = clamp_to_work_area((-1800.0, 100.0), egui::vec2(400.0, 300.0), work, 12.0);

        assert_eq!((x, y), (-1788.0, 112.0));
    }
}
