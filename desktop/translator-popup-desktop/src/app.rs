use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use eframe::egui;
use translator_core::args::{Args, read_stdin};
use translator_core::detect;
use translator_core::history::HistoryEntry;
use translator_core::languages;
use translator_core::settings::{persist_recent_targets, persist_source, persist_target};
use translator_core::update::{ReleaseAsset, ReleaseInfo};
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

/// Progress and outcome of the one-click update download/install worker.
enum UpdateEvent {
    Progress { downloaded: u64, total: Option<u64> },
    Installed,
    Failed(String),
}

enum UpdateProgress {
    Downloading { downloaded: u64, total: Option<u64> },
    Failed(String),
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
    install_update: bool,
    toggle_history: bool,
    toggle_pin: bool,
    clear_history: bool,
    load_history: Option<usize>,
    toggle_settings: bool,
    apply_hotkey: bool,
    save_model_path: bool,
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
    update_progress: Option<UpdateProgress>,
    update_install_receiver: Option<Receiver<UpdateEvent>>,
    history: Vec<HistoryEntry>,
    history_open: bool,
    settings_open: bool,
    settings_hotkey: String,
    settings_hotkey_error: Option<String>,
    settings_model_path: String,
    settings_auto_download: bool,
    settings_check_updates: bool,
    settings_serve_extension: bool,
    settings_saved_at: Option<Instant>,
    pinned: bool,
    copied_at: Option<Instant>,
    replaced_at: Option<Instant>,
    replace_window: Option<isize>,
    notice: Option<String>,
    requested_height: Option<f32>,
    hotkey: Option<Hotkey>,
    hotkey_label: String,
    tray: Option<Tray>,
    tray_status: Option<String>,
    window: Option<isize>,
    window_visible: bool,
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

/// One-click update package for this platform. Windows ships a zip that
/// contains `install.ps1`; other platforms keep opening the release page.
#[cfg(target_os = "windows")]
fn update_asset(info: &ReleaseInfo) -> Option<&ReleaseAsset> {
    info.assets
        .iter()
        .find(|asset| asset.name == "OpenTranslator-windows-x64.zip")
}

#[cfg(not(target_os = "windows"))]
fn update_asset(_info: &ReleaseInfo) -> Option<&ReleaseAsset> {
    None
}

fn format_update_progress(downloaded: u64, total: Option<u64>) -> String {
    const MB: f64 = 1_000_000.0;

    match total {
        Some(total) if total > 0 => format!(
            "正在下载更新：{:.0}%（{:.0}/{:.0} MB）",
            downloaded as f64 / total as f64 * 100.0,
            downloaded as f64 / MB,
            total as f64 / MB
        ),
        _ => format!("正在下载更新：已下载 {:.0} MB", downloaded as f64 / MB),
    }
}

fn bool_value(value: Option<&str>, default: bool) -> bool {
    match value {
        Some("true") => true,
        Some("false") => false,
        _ => default,
    }
}

fn bool_str(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

fn history_preview(value: &str, max_chars: usize) -> String {
    let mut preview: String = value.chars().take(max_chars).collect();

    if value.chars().count() > max_chars {
        preview.push('…');
    }

    preview
}

/// Tray tooltip for the current model state. Download/load progress and errors
/// are surfaced here because the window stays hidden until the user asks for it.
fn tray_tooltip(model: &ModelState, hotkey_label: &str) -> String {
    match model {
        ModelState::Downloading { downloaded, total } => format!(
            "OpenTranslator · {}",
            translator_core::models::format_download_status(*downloaded, *total)
        ),
        ModelState::Failed(error) => format!("OpenTranslator · {error}"),
        _ => format!("OpenTranslator（{hotkey_label}）"),
    }
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
        let file_config = translator_core::settings::load_config();

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
            update_progress: None,
            update_install_receiver: None,
            history: translator_core::history::load(),
            history_open: false,
            settings_open: false,
            settings_hotkey: hotkey_spec.to_string(),
            settings_hotkey_error: None,
            settings_model_path: file_config.model_path.clone().unwrap_or_default(),
            settings_auto_download: bool_value(file_config.auto_download.as_deref(), true),
            settings_check_updates: bool_value(file_config.check_updates.as_deref(), true),
            settings_serve_extension: bool_value(file_config.serve_extension.as_deref(), true),
            settings_saved_at: None,
            pinned: false,
            copied_at: None,
            replaced_at: None,
            replace_window: None,
            notice: None,
            requested_height: None,
            hotkey: None,
            hotkey_label: hotkey_spec.to_string(),
            tray: None,
            tray_status: None,
            window,
            window_visible: false,
            quit: false,
        };

        if check_updates {
            let (sender, receiver) = channel::<ReleaseInfo>();
            app.update_receiver = Some(receiver);
            spawn_update_check(sender);
        }

        let mut show_on_start = false;
        let mut startup_notice: Option<String> = None;

        if app.args.settings {
            app.settings_open = true;
            show_on_start = true;
        }

        match startup {
            Startup::Loaded(Ok(engine)) => {
                app.engine = Some(engine);
                app.maybe_start_server();
            }
            Startup::Loaded(Err(error)) => {
                let message = format!("模型加载失败：{error}");
                app.model = ModelState::Failed(message.clone());
                startup_notice = Some(message);
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
            }
        }

        let mut hotkey_error: Option<String> = None;

        match Hotkey::register(hotkey_spec) {
            Ok(hotkey) => app.hotkey = Some(hotkey),
            Err(error) => {
                hotkey_error = Some(error.clone());
                app.notice = Some(error);
            }
        }

        match Tray::new(&format!("OpenTranslator（{hotkey_spec}）")) {
            Ok(tray) => app.tray = Some(tray),
            Err(error) => {
                if app.notice.is_none() {
                    app.notice = Some(error);
                }
            }
        }

        app.refresh_tray_tooltip();

        if app.args.stdin {
            let initial = read_stdin().unwrap_or_default();

            if !initial.is_empty() {
                app.begin_translation(initial);
                show_on_start = true;
            }
        }

        // Only the login autostart (`--autostart`, passed by the installers)
        // stays silent; a manual launch shows the window, and without a
        // registered hotkey or tray there is no way to bring it back, so
        // surface it instead of starting silent.
        let start_hidden = !show_on_start && app.args.autostart && app.can_restore();

        if !start_hidden {
            app.show_window(&cc.egui_ctx);
        } else {
            // While hidden there is no other surface for problems, so report
            // them through the system notification center.
            if let Some(error) = hotkey_error {
                crate::notify::show("OpenTranslator", &format!("快捷键注册失败：{error}"));
            }
            if let Some(message) = startup_notice {
                crate::notify::show("OpenTranslator", &message);
            }

            // eframe forces the window visible after the first painted frame
            // (its white-flash fix), which overrides
            // `ViewportBuilder::with_visible(false)`. Queue a hide command so
            // an autostart at login stays silent in the tray; it is applied
            // right after that first frame.
            cc.egui_ctx
                .send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }

        app
    }

    fn trigger(&mut self) {
        self.settings_open = false;
        self.history_open = false;
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

    fn show_window(&mut self, ctx: &egui::Context) {
        self.window_visible = true;
        self.place_near_cursor(ctx);
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    fn refresh_tray_tooltip(&mut self) {
        if self.tray.is_none() {
            return;
        }

        let tooltip = tray_tooltip(&self.model, &self.hotkey_label);

        if self.tray_status.as_deref() == Some(tooltip.as_str()) {
            return;
        }

        if let Some(tray) = &self.tray {
            tray.set_tooltip(&tooltip);
        }

        self.tray_status = Some(tooltip);
    }

    fn start_update_install(&mut self, asset_url: String) {
        let (sender, receiver) = channel();
        self.update_install_receiver = Some(receiver);
        self.update_progress = Some(UpdateProgress::Downloading {
            downloaded: 0,
            total: None,
        });
        spawn_update_install(asset_url, sender);
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

    fn apply_hotkey(&mut self) {
        let spec = self.settings_hotkey.trim().to_string();
        let previous = self.hotkey_label.clone();

        // Drop the current binding first so re-applying the same spec works;
        // restore it if the new one cannot be registered.
        self.hotkey = None;

        match Hotkey::register(&spec) {
            Ok(hotkey) => {
                self.hotkey = Some(hotkey);
                self.hotkey_label = spec.clone();
                self.settings_hotkey_error = None;
                self.settings_saved_at = Some(Instant::now());
                translator_core::settings::persist_value("hotkey", &spec);
            }
            Err(error) => {
                if let Ok(old) = Hotkey::register(&previous) {
                    self.hotkey = Some(old);
                }

                self.settings_hotkey_error = Some(format!("{error}（已保留 {previous}）"));
            }
        }
    }

    fn load_history(&mut self, index: usize) {
        let Some(entry) = self.history.get(index).cloned() else {
            return;
        };

        self.source = entry.source.clone();
        self.target = entry.target.clone();
        persist_source(&entry.source);
        persist_target(&entry.target);
        self.detected_source = None;
        self.history_open = false;
        self.translation = TranslationState::Done {
            text: entry.text,
            translation: entry.translation,
            elapsed: Duration::ZERO,
        };
    }

    fn hide(&mut self, ctx: &egui::Context) {
        self.window_visible = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
    }

    fn close_window(&mut self, ctx: &egui::Context) {
        if self.pinned {
            return;
        }

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

    fn settings_saved_recently(&self) -> bool {
        self.settings_saved_at
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
            Some(TrayCommand::History) => {
                self.history_open = true;
                self.settings_open = false;
                self.show_window(ctx);
            }
            Some(TrayCommand::Settings) => {
                self.settings_open = true;
                self.history_open = false;
                self.show_window(ctx);
            }
            Some(TrayCommand::Update) => {
                if let Some(info) = self.update.clone() {
                    match update_asset(&info) {
                        Some(asset) => self.start_update_install(asset.url.clone()),
                        None => open_url(&info.url),
                    }
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

                    if !text.is_empty() {
                        let source = self
                            .detected_source
                            .unwrap_or(self.source.as_str())
                            .to_string();

                        translator_core::history::push(
                            &mut self.history,
                            HistoryEntry {
                                source,
                                target: self.target.clone(),
                                text: text.clone(),
                                translation: translation.clone(),
                            },
                        );
                        translator_core::history::save(&self.history);
                    }

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
                    let was_downloading =
                        matches!(self.model, ModelState::Downloading { .. });

                    self.engine = Some(engine);
                    self.model = ModelState::Ready;
                    self.maybe_start_server();

                    if was_downloading && !self.window_visible {
                        crate::notify::show(
                            "OpenTranslator",
                            &format!("模型下载完成，按 {} 开始翻译", self.hotkey_label),
                        );
                    }

                    if let TranslationState::Waiting { text } =
                        std::mem::replace(&mut self.translation, TranslationState::Idle)
                    {
                        self.begin_translation(text);
                    }
                }
                StartupEvent::Failed(error) => {
                    self.model = ModelState::Failed(error.clone());

                    if !self.window_visible {
                        crate::notify::show(
                            "OpenTranslator",
                            &format!("模型加载失败：{error}"),
                        );
                    }
                }
            }
        }

        let install_messages: Vec<UpdateEvent> = self
            .update_install_receiver
            .as_ref()
            .map(|receiver| receiver.try_iter().collect())
            .unwrap_or_default();

        for message in install_messages {
            match message {
                UpdateEvent::Progress { downloaded, total } => {
                    self.update_progress = Some(UpdateProgress::Downloading { downloaded, total });
                }
                UpdateEvent::Installed => {
                    // The installer replaces the binary and starts the new
                    // version, so it is time to make room for it.
                    self.quit = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                UpdateEvent::Failed(error) => {
                    self.update_progress = Some(UpdateProgress::Failed(error));
                }
            }
        }

        self.refresh_tray_tooltip();

        if ctx.input(|input| input.viewport().close_requested()) {
            if self.quit || !self.can_restore() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.hide(ctx);
            }
        }

        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            if self.settings_open {
                self.settings_open = false;
            } else if self.history_open {
                self.history_open = false;
            } else {
                self.close_window(ctx);
            }
        }

        if ctx.input(|input| input.modifiers.command && input.key_pressed(egui::Key::H)) {
            self.history_open = !self.history_open;
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

        if actions.install_update {
            if let Some(info) = self.update.clone() {
                if let Some(asset) = update_asset(&info) {
                    self.start_update_install(asset.url.clone());
                }
            }
        }

        if actions.toggle_history {
            self.history_open = !self.history_open;
            self.settings_open = false;
        }

        if actions.toggle_settings {
            self.settings_open = !self.settings_open;
            self.history_open = false;
            self.settings_hotkey_error = None;
        }

        if actions.apply_hotkey {
            self.apply_hotkey();
        }

        if actions.save_model_path {
            translator_core::settings::persist_value("model_path", self.settings_model_path.trim());
            self.settings_saved_at = Some(Instant::now());
        }

        if actions.toggle_pin {
            self.pinned = !self.pinned;
        }

        if actions.clear_history {
            self.history.clear();
            translator_core::history::save(&self.history);
            self.history_open = false;
        }

        if let Some(index) = actions.load_history {
            self.load_history(index);
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
            if self.pinned {
                ui.label(egui::RichText::new("已固定").weak().size(11.0))
                    .on_hover_text("点击下方“取消固定”后 Esc/× 才会隐藏窗口");
            } else {
                let close = ui.add(
                    egui::Button::new(egui::RichText::new("×").size(17.0).weak())
                        .frame(false)
                        .min_size(egui::vec2(26.0, 26.0)),
                );

                if close.on_hover_text("隐藏（Esc）").clicked() {
                    actions.close_window = true;
                }
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

        let downloading = matches!(
            self.update_progress,
            Some(UpdateProgress::Downloading { .. })
        );
        let failed = matches!(self.update_progress, Some(UpdateProgress::Failed(_)));
        let installable = update_asset(&info).is_some();

        egui::Frame::new()
            .fill(ACCENT.gamma_multiply(0.14))
            .corner_radius(10)
            .inner_margin(egui::Margin::symmetric(12, 6))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let text = match &self.update_progress {
                        Some(UpdateProgress::Downloading { downloaded, total }) => {
                            format_update_progress(*downloaded, *total)
                        }
                        Some(UpdateProgress::Failed(error)) => {
                            format!("更新失败：{error}")
                        }
                        None => format!("发现新版本 v{}", info.version),
                    };

                    ui.add(
                        egui::Label::new(egui::RichText::new(text).strong().size(13.0)).wrap(),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if !downloading && ui.small_button("忽略").clicked() {
                            actions.dismiss_update = true;
                        }

                        if !downloading && ui.small_button("查看").clicked() {
                            actions.open_url = Some(info.url.clone());
                        }

                        if installable && !downloading {
                            let label = if failed { "重试" } else { "立即更新" };

                            if ui.small_button(label).clicked() {
                                actions.install_update = true;
                            }
                        }
                    });
                });
            });

        ui.add_space(4.0);
    }

    fn body(&mut self, ui: &mut egui::Ui, actions: &mut Actions, max_window_height: f32) -> f32 {
        if self.settings_open {
            return self.settings_page(ui, actions);
        }

        let mut content = 0.0;

        if self.history_open {
            content += self.history_panel(ui, actions);
            ui.add_space(6.0);
        }

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

        content + above + translation_height + 6.0 + footer_height
    }

    fn settings_page(&mut self, ui: &mut egui::Ui, actions: &mut Actions) -> f32 {
        let mut height = 0.0;

        egui::Frame::new()
            .fill(ui.visuals().faint_bg_color)
            .corner_radius(10)
            .inner_margin(egui::Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("设置").strong().size(14.0));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("返回翻译").clicked() {
                            actions.toggle_settings = true;
                        }
                    });
                });
                ui.add_space(6.0);

                ui.label(egui::RichText::new("快捷键").weak().size(12.0));
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.settings_hotkey)
                            .desired_width(150.0)
                            .font(egui::TextStyle::Monospace),
                    );

                    if ui.small_button("应用").clicked() {
                        actions.apply_hotkey = true;
                    }

                    if let Some(error) = &self.settings_hotkey_error {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(error)
                                    .size(11.5)
                                    .color(ui.visuals().error_fg_color),
                            )
                            .wrap(),
                        );
                    }
                });
                ui.label(
                    egui::RichText::new("例如 Ctrl+Alt+T；应用后立即生效，被占用时会保留原快捷键")
                        .weak()
                        .size(11.0),
                );

                ui.add_space(8.0);

                ui.label(egui::RichText::new("模型路径").weak().size(12.0));
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.settings_model_path)
                            .desired_width(300.0)
                            .hint_text("留空使用默认位置"),
                    );

                    if ui.small_button("保存").clicked() {
                        actions.save_model_path = true;
                    }

                    if self.settings_saved_recently() {
                        ui.label(
                            egui::RichText::new("已保存")
                                .size(11.5)
                                .color(egui::Color32::from_rgb(0x3D, 0xB5, 0x7A)),
                        );
                    }
                });
                ui.label(
                    egui::RichText::new("模型文件（.gguf）的完整路径，下次启动生效")
                        .weak()
                        .size(11.0),
                );

                ui.add_space(8.0);

                if ui
                    .checkbox(&mut self.settings_auto_download, "自动下载模型")
                    .on_hover_text("模型文件缺失时从 ModelScope 下载（约 1.1 GB）；下次启动生效")
                    .changed()
                {
                    translator_core::settings::persist_value(
                        "auto_download",
                        bool_str(self.settings_auto_download),
                    );
                }

                if ui
                    .checkbox(&mut self.settings_check_updates, "启动时检查更新")
                    .on_hover_text("下次启动生效")
                    .changed()
                {
                    translator_core::settings::persist_value(
                        "check_updates",
                        bool_str(self.settings_check_updates),
                    );
                }

                if ui
                    .checkbox(&mut self.settings_serve_extension, "提供浏览器扩展接口")
                    .on_hover_text("在 service_url 上提供本地 HTTP API；下次启动生效")
                    .changed()
                {
                    translator_core::settings::persist_value(
                        "serve_extension",
                        bool_str(self.settings_serve_extension),
                    );
                }

                ui.add_space(8.0);

                if let Some(path) = translator_core::paths::config_path() {
                    ui.horizontal(|ui| {
                        if ui.small_button("打开配置目录").clicked() {
                            if let Some(dir) = path.parent() {
                                actions.open_url = Some(dir.to_string_lossy().to_string());
                            }
                        }

                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(path.to_string_lossy())
                                    .weak()
                                    .size(10.5),
                            )
                            .wrap(),
                        );
                    });
                }

                height = ui.min_rect().height();
            });

        height
    }

    fn history_panel(&self, ui: &mut egui::Ui, actions: &mut Actions) -> f32 {
        let mut height = 0.0;

        egui::Frame::new()
            .fill(ui.visuals().faint_bg_color)
            .corner_radius(10)
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("最近翻译").strong().size(13.0));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if !self.history.is_empty() && ui.small_button("清空").clicked() {
                            actions.clear_history = true;
                        }
                    });
                });

                ui.add_space(4.0);

                if self.history.is_empty() {
                    ui.label(egui::RichText::new("暂无历史").weak().size(12.0));
                } else {
                    egui::ScrollArea::vertical()
                        .id_salt("history_scroll")
                        .max_height(150.0)
                        .show(ui, |ui| {
                            for (index, entry) in self.history.iter().enumerate() {
                                let label = format!(
                                    "{} · {} → {}",
                                    languages::label(&entry.target),
                                    history_preview(&entry.text, 16),
                                    history_preview(&entry.translation, 22),
                                );

                                if ui
                                    .selectable_label(
                                        false,
                                        egui::RichText::new(label).size(12.5),
                                    )
                                    .clicked()
                                {
                                    actions.load_history = Some(index);
                                }
                            }
                        });
                }

                height = ui.min_rect().height();
            });

        height
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

                let pin_label = if self.pinned { "取消固定" } else { "固定" };
                let pin = egui::Button::new(egui::RichText::new(pin_label).size(13.0).color(
                    if self.pinned {
                        egui::Color32::WHITE
                    } else {
                        ui.visuals().text_color()
                    },
                ))
                .fill(if self.pinned {
                    ACCENT.gamma_multiply(0.85)
                } else {
                    ui.visuals().faint_bg_color
                });

                if ui.add(pin).clicked() {
                    actions.toggle_pin = true;
                }

                // The tray menu owns 历史…/设置…/退出; without a tray the
                // window keeps its own entry points so the app stays usable.
                if !self.tray_active() {
                    if ui
                        .add(egui::Button::new(egui::RichText::new("退出").size(13.0)))
                        .clicked()
                    {
                        actions.quit = true;
                    }

                    if ui
                        .add(egui::Button::new(egui::RichText::new("历史").size(13.0)))
                        .clicked()
                    {
                        actions.toggle_history = true;
                    }

                    if ui
                        .add(egui::Button::new(egui::RichText::new("设置").size(13.0)))
                        .clicked()
                    {
                        actions.toggle_settings = true;
                    }
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

fn spawn_update_install(asset_url: String, sender: Sender<UpdateEvent>) {
    std::thread::spawn(move || {
        #[cfg(target_os = "windows")]
        {
            if let Err(error) = run_update_install(&asset_url, &sender) {
                let _ = sender.send(UpdateEvent::Failed(error));
            }
        }

        #[cfg(not(target_os = "windows"))]
        let _ = (asset_url, sender);
    });
}

/// Download the release package, extract it and start its installer. The
/// installer stops this process, replaces the binary and starts the new
/// version in the tray, so the UI quits after `Installed`.
#[cfg(target_os = "windows")]
fn run_update_install(asset_url: &str, sender: &Sender<UpdateEvent>) -> Result<(), String> {
    let client = translator_core::models::download_client().map_err(|error| error.to_string())?;

    let dir = std::env::temp_dir().join("open-translator-update");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|error| format!("创建更新目录失败：{error}"))?;

    let archive = dir.join("OpenTranslator-windows-x64.zip");
    let progress_sender = sender.clone();

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("初始化更新下载失败：{error}"))?;

    runtime
        .block_on(translator_core::models::download(
            &client,
            asset_url,
            &archive,
            None,
            move |downloaded, total| {
                let _ = progress_sender.send(UpdateEvent::Progress { downloaded, total });
            },
        ))
        .map_err(|error| format!("下载更新失败：{error}"))?;

    let package = dir.join("package");
    let expand = format!(
        "Expand-Archive -LiteralPath {} -DestinationPath {} -Force",
        ps_quote(&archive.to_string_lossy()),
        ps_quote(&package.to_string_lossy())
    );

    let status = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
        ])
        .arg(&expand)
        .status()
        .map_err(|error| format!("解压更新失败：{error}"))?;

    if !status.success() {
        return Err("解压更新失败".to_string());
    }

    let installer = package.join("install.ps1");
    let exe = package.join("translator-popup-desktop.exe");

    if !installer.is_file() || !exe.is_file() {
        return Err("更新包内容不完整".to_string());
    }

    std::process::Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&installer)
        .spawn()
        .map_err(|error| format!("启动安装程序失败：{error}"))?;

    let _ = sender.send(UpdateEvent::Installed);

    Ok(())
}

#[cfg(target_os = "windows")]
fn ps_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
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

#[cfg(test)]
mod tray_tests {
    use super::{ModelState, tray_tooltip};

    #[test]
    fn shows_the_hotkey_when_ready() {
        assert_eq!(
            tray_tooltip(&ModelState::Ready, "Ctrl+Alt+T"),
            "OpenTranslator（Ctrl+Alt+T）"
        );
    }

    #[test]
    fn shows_download_progress() {
        let tooltip = tray_tooltip(
            &ModelState::Downloading {
                downloaded: 50_000_000,
                total: Some(100_000_000),
            },
            "Ctrl+Alt+T",
        );

        assert!(tooltip.contains("50%"), "unexpected tooltip: {tooltip}");
    }

    #[test]
    fn shows_model_errors() {
        assert_eq!(
            tray_tooltip(&ModelState::Failed("模型文件不存在".to_string()), "Ctrl+Alt+T"),
            "OpenTranslator · 模型文件不存在"
        );
    }
}

#[cfg(test)]
mod history_tests {
    use super::history_preview;

    #[test]
    fn truncates_long_values() {
        assert_eq!(history_preview("短文本", 16), "短文本");
        assert_eq!(history_preview("0123456789abcdefgh", 16), "0123456789abcdef…");
    }
}

#[cfg(test)]
mod settings_tests {
    use super::{bool_str, bool_value};

    #[test]
    fn parses_config_bools() {
        assert!(bool_value(None, true));
        assert!(!bool_value(Some("false"), true));
        assert!(bool_value(Some("true"), false));
        assert!(bool_value(Some("bogus"), true));
    }

    #[test]
    fn formats_config_bools() {
        assert_eq!(bool_str(true), "true");
        assert_eq!(bool_str(false), "false");
    }
}

#[cfg(all(test, target_os = "windows"))]
mod update_tests {
    use super::{UpdateEvent, format_update_progress, run_update_install};
    use std::sync::mpsc::channel;
    use std::time::{Duration, Instant};

    #[test]
    fn formats_update_progress() {
        assert_eq!(
            format_update_progress(50_000_000, Some(100_000_000)),
            "正在下载更新：50%（50/100 MB）"
        );
        assert_eq!(
            format_update_progress(12_000_000, None),
            "正在下载更新：已下载 12 MB"
        );
    }

    #[test]
    fn downloads_extracts_and_launches_the_installer() {
        let marker = std::env::temp_dir().join("open-translator-update-test-marker.txt");
        let _ = std::fs::remove_file(&marker);

        let staging = std::env::temp_dir().join("open-translator-update-test-staging");
        let _ = std::fs::remove_dir_all(&staging);
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(
            staging.join("install.ps1"),
            format!("Set-Content -LiteralPath '{}' -Value ok", marker.display()),
        )
        .unwrap();
        std::fs::write(staging.join("translator-popup-desktop.exe"), b"stub").unwrap();

        let archive = std::env::temp_dir().join("open-translator-update-test.zip");
        let _ = std::fs::remove_file(&archive);

        let status = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command"])
            .arg(format!(
                "Compress-Archive -Path '{}' -DestinationPath '{}' -Force",
                staging.join("*").display(),
                archive.display()
            ))
            .status()
            .unwrap();
        assert!(status.success());

        let payload = std::fs::read(&archive).unwrap();
        let app = axum::Router::new().route(
            "/pkg.zip",
            axum::routing::get(move || {
                let payload = payload.clone();
                async move { payload }
            }),
        );
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();

        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                axum::serve(listener, app).await.unwrap();
            });
        });

        let (sender, receiver) = channel();
        run_update_install(&format!("http://{address}/pkg.zip"), &sender).unwrap();

        let deadline = Instant::now() + Duration::from_secs(10);
        while !marker.is_file() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(marker.is_file(), "the installer script did not run");

        let mut progress = false;
        let mut installed = false;

        while let Ok(event) = receiver.try_recv() {
            match event {
                UpdateEvent::Progress { .. } => progress = true,
                UpdateEvent::Installed => installed = true,
                UpdateEvent::Failed(error) => panic!("update failed: {error}"),
            }
        }

        assert!(progress);
        assert!(installed);

        let _ = std::fs::remove_file(&marker);
        let _ = std::fs::remove_file(&archive);
        let _ = std::fs::remove_dir_all(&staging);
        let _ = std::fs::remove_dir_all(std::env::temp_dir().join("open-translator-update"));
    }
}
