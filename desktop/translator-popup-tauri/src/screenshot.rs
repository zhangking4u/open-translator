//! Region-screenshot translation (Linux / Windows).
//!
//! Flow: a transparent click layer covers the monitor (the screen stays live —
//! no freeze, no dim), the user drags a region, the selector hides, the
//! platform captures the screen — the XDG portal on Linux, the Windows
//! Graphics Capture API on Windows — and a Lens-style viewer window shows the
//! translations over the cropped pixels, anchored at the region. The stored
//! region can be replayed with the hotkey (while the viewer is visible) or the
//! 刷新 button — the game loop: select once, re-translate on demand.

use std::io::Cursor;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(target_os = "linux")]
use ashpd::desktop::screenshot::Screenshot;
use base64::Engine as _;
use serde::Serialize;
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewWindowBuilder,
};

use translator_core::history::HistoryEntry;
use translator_service::engine::{EngineRef, TimeoutEngine};

use crate::{AppState, TranslateConfig};

pub const SELECT_LABEL: &str = "shot-select";
pub const VIEWER_LABEL: &str = "shot";

/// Panel padding and toolbar height used for the viewer window layout
/// (logical pixels, matching `ui/shot.css`).
const VIEWER_PADDING: f64 = 8.0;
const VIEWER_TOOLBAR: f64 = 44.0;
/// Keep the viewer this far away from the monitor edges.
const VIEWER_MARGIN: f64 = 8.0;
/// A selection smaller than this (capture pixels) is treated as a misclick.
const MIN_SELECTION: u32 = 6;
/// Let the compositor remove the selector before capturing the screen.
const HIDE_SETTLE: Duration = Duration::from_millis(200);

#[derive(Default)]
pub struct ShotState {
    /// Monitor hosting the in-progress selection.
    active: Mutex<Option<MonitorBox>>,
    /// Everything needed to replay the last region without selecting again.
    last: Mutex<Option<LastShot>>,
}

/// Diagnostic sequence for the screenshot flow logs.
static SHOT_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn shot_seq() -> u64 {
    SHOT_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1
}

/// `TRANSLATOR_SHOT_DEBUG=1` dumps every recognized line and its translation
/// (private content; off by default).
fn shot_debug() -> bool {
    std::env::var_os("TRANSLATOR_SHOT_DEBUG").is_some()
}

struct LastShot {
    region: Selection,
    monitor: MonitorBox,
}

/// One monitor in logical coordinates plus its device scale factor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MonitorBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub scale: f64,
}

/// The part of a screen capture that belongs to the target monitor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Slice {
    /// Slice rect inside the capture (capture pixels).
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    /// Capture pixels per logical pixel for this monitor.
    pub scale: f64,
    /// Monitor origin in logical screen coordinates.
    pub origin_x: f64,
    pub origin_y: f64,
    pub monitor_width: f64,
    pub monitor_height: f64,
}

/// A selection in slice coordinates (capture pixels).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Selection {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Serialize)]
struct BlockPayload {
    text: String,
    translation: String,
    quad: [[f32; 2]; 4],
}

#[derive(Clone, Serialize)]
struct ViewerPayload {
    image: Option<String>,
    error: Option<String>,
    blocks: Vec<BlockPayload>,
}

/// Creates the hidden selector and viewer windows.
pub fn build_windows(app: &AppHandle) -> tauri::Result<()> {
    let selector = WebviewWindowBuilder::new(
        app,
        SELECT_LABEL,
        tauri::WebviewUrl::App("select.html".into()),
    )
    .title("OpenTranslator")
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .shadow(false)
    .inner_size(64.0, 64.0)
    .visible(false)
    .build()?;
    let _ = selector.set_focusable(true);

    let viewer = WebviewWindowBuilder::new(
        app,
        VIEWER_LABEL,
        tauri::WebviewUrl::App("shot.html".into()),
    )
    .title("OpenTranslator")
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .shadow(false)
    .inner_size(64.0, 64.0)
    .visible(false)
    .build()?;
    let _ = viewer.set_focusable(true);

    Ok(())
}

/// Hotkey entry point: replay the last region while the viewer is visible,
/// otherwise start a new selection.
pub fn trigger(app: AppHandle) {
    eprintln!(
        "SHOTDBG trigger #{} visible={}",
        shot_seq(),
        viewer_visible(&app)
    );

    if viewer_visible(&app) {
        refresh(app);
    } else {
        start_selection(app);
    }
}

/// Shows the transparent selection layer over the monitor under the cursor.
pub fn start_selection(app: AppHandle) {
    eprintln!("SHOTDBG start_selection #{}", shot_seq());

    tauri::async_runtime::spawn(async move {
        let Some(monitor) = cursor_monitor(&app) else {
            crate::notify::show("OpenTranslator", "截图失败：找不到显示器");
            return;
        };

        *app.state::<ShotState>().active.lock().unwrap() = Some(monitor);

        if let Some(selector) = app.get_webview_window(SELECT_LABEL) {
            // Show before sizing: geometry calls on an unrealized window are
            // dropped on Linux.
            let _ = selector.show();

            #[cfg(target_os = "linux")]
            {
                let _ = selector.set_position(LogicalPosition::new(monitor.x, monitor.y));
                let _ = selector.set_size(LogicalSize::new(monitor.width, monitor.height));
            }

            // Windows reports monitors in physical pixels, so place the
            // window physically: a logical position would be converted with
            // the wrong scale factor on a mixed-DPI desktop.
            #[cfg(target_os = "windows")]
            {
                let _ = selector.set_position(tauri::PhysicalPosition::new(
                    (monitor.x * monitor.scale).round() as i32,
                    (monitor.y * monitor.scale).round() as i32,
                ));
                let _ = selector.set_size(tauri::PhysicalSize::new(
                    (monitor.width * monitor.scale).round() as u32,
                    (monitor.height * monitor.scale).round() as u32,
                ));
            }

            let _ = selector.set_always_on_top(true);
            let _ = selector.set_focus();
        }
    });
}

/// Called by the selector with a drag rectangle in CSS pixels.
pub fn region_selected(
    app: &AppHandle,
    css_x: f64,
    css_y: f64,
    css_width: f64,
    css_height: f64,
    _viewport_width: f64,
    _viewport_height: f64,
) -> Result<(), String> {
    if css_width < 3.0 || css_height < 3.0 {
        return Err("选区太小，请重新框选".to_string());
    }

    eprintln!(
        "SHOTDBG region_selected #{} css=({css_x:.0},{css_y:.0},{css_width:.0},{css_height:.0})",
        shot_seq()
    );

    let state = app.state::<ShotState>();
    let monitor = state
        .active
        .lock()
        .unwrap()
        .take()
        .ok_or_else(|| "请重新开始截图".to_string())?;

    let selector = app.get_webview_window(SELECT_LABEL);

    // The window manager may have constrained the selector to the work area;
    // its actual position is part of the mapping.
    let (window_x, window_y, window_scale) = selector
        .as_ref()
        .and_then(|window| {
            let position = window.outer_position().ok()?;
            let scale = window.scale_factor().unwrap_or(monitor.scale);
            Some((position.x as f64, position.y as f64, scale))
        })
        .unwrap_or((monitor.x * monitor.scale, monitor.y * monitor.scale, monitor.scale));

    let mapped_monitor = MonitorBox {
        scale: window_scale,
        ..monitor
    };

    let selection = map_selection(
        (css_x, css_y, css_width, css_height),
        (window_x, window_y),
        mapped_monitor,
    )
    .ok_or_else(|| "选区太小，请重新框选".to_string())?;

    if let Some(selector) = &selector {
        let _ = selector.hide();
    }

    *state.last.lock().unwrap() = Some(LastShot { region: selection, monitor });

    let app = app.clone();

    tauri::async_runtime::spawn(async move {
        // Let the compositor remove the selector before capturing.
        tokio::time::sleep(HIDE_SETTLE).await;

        let (png, capture_width, capture_height) = match capture_screen(monitor).await {
            Ok(capture) => capture,
            Err(message) => {
                crate::notify::show("OpenTranslator", &message);
                eprintln!("{message}");
                return;
            }
        };

        let monitors = monitor_boxes(&app);
        let Some(slice) = monitor_slice((capture_width, capture_height), &monitors, monitor)
        else {
            crate::notify::show("OpenTranslator", "截图失败：无法定位显示器");
            return;
        };

        eprintln!(
            "SHOTDBG capture {capture_width}x{capture_height} slice {}x{} scale {:.2} selection {selection:?}",
            slice.width, slice.height, slice.scale
        );

        run_region(&app, &png, selection, slice, monitor).await;
    });

    Ok(())
}

/// Replays the stored region with a fresh capture.
pub fn refresh(app: AppHandle) {
    let stored = {
        let state = app.state::<ShotState>();
        let last = state.last.lock().unwrap();

        match last.as_ref() {
            Some(last) => (last.region, last.monitor),
            None => {
                drop(last);
                start_selection(app);
                return;
            }
        }
    };

    tauri::async_runtime::spawn(async move {
        let (region, monitor) = stored;

        let (png, capture_width, capture_height) = match capture_screen(monitor).await {
            Ok(capture) => capture,
            Err(message) => {
                crate::notify::show("OpenTranslator", &message);
                return;
            }
        };

        let monitors = monitor_boxes(&app);
        let Some(slice) = monitor_slice((capture_width, capture_height), &monitors, monitor)
        else {
            crate::notify::show("OpenTranslator", "截图失败：无法定位显示器");
            return;
        };

        // The new capture must still cover the stored region.
        if region.x + region.width > slice.width || region.y + region.height > slice.height {
            crate::notify::show("OpenTranslator", "屏幕分辨率已变化，请重新选择区域");
            return;
        }

        run_region(&app, &png, region, slice, monitor).await;
    });
}

/// Hides the selector and clears the in-progress selection.
pub fn cancel(app: &AppHandle) {
    if let Some(selector) = app.get_webview_window(SELECT_LABEL) {
        let _ = selector.hide();
    }

    *app.state::<ShotState>().active.lock().unwrap() = None;
}

/// Hides the viewer (Esc / close / blur).
pub fn close(app: &AppHandle) {
    if let Some(viewer) = app.get_webview_window(VIEWER_LABEL) {
        let _ = viewer.hide();
    }
}

/// Moves the viewer by a logical-pixel screen delta (its drag handler; the
/// Windows move loop does not engage for this tool window, so the drag is
/// applied from the page like the docked ball).
pub fn move_viewer_by(app: &AppHandle, dx: f64, dy: f64) {
    let Some(viewer) = app.get_webview_window(VIEWER_LABEL) else {
        return;
    };

    let Ok(position) = viewer.outer_position() else {
        return;
    };

    let scale = viewer.scale_factor().unwrap_or(1.0);
    let x = position.x + (dx * scale).round() as i32;
    let y = position.y + (dy * scale).round() as i32;

    let _ = viewer.set_position(tauri::PhysicalPosition::new(x, y));
}

fn viewer_visible(app: &AppHandle) -> bool {
    app.get_webview_window(VIEWER_LABEL)
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false)
}

async fn run_region(
    app: &AppHandle,
    png: &[u8],
    region: Selection,
    slice: Slice,
    monitor: MonitorBox,
) {
    let (image, cropped) = match crop_region(png, region, slice) {
        Ok(value) => value,
        Err(message) => {
            crate::notify::show("OpenTranslator", &message);
            return;
        }
    };

    // Debug aid: keep the exact inputs of a failing capture for offline
    // analysis (private content; only with TRANSLATOR_SHOT_DEBUG=1).
    if shot_debug() {
        let dir = std::env::temp_dir();
        let _ = std::fs::write(dir.join("open-translator-shot-full.png"), png);
        let _ = std::fs::write(dir.join("open-translator-shot-crop.png"), &cropped);
    }

    let payload = match translate_cropped(app, &cropped).await {
        Ok((source_text, translation_text, blocks)) => {
            eprintln!(
                "SHOTDBG region {}x{} -> {} blocks",
                region.width,
                region.height,
                blocks.len()
            );

            if shot_debug() {
                for block in &blocks {
                    eprintln!("SHOTDBG line: {} => {}", block.text, block.translation);
                }
            }

            record_history(app, source_text, translation_text);
            ViewerPayload {
                image: Some(image),
                error: None,
                blocks,
            }
        }
        Err(message) => {
            eprintln!("SHOTDBG region {}x{} failed: {message}", region.width, region.height);
            ViewerPayload {
                image: Some(image),
                error: Some(message),
                blocks: Vec::new(),
            }
        }
    };

    show_viewer(app, region, slice, monitor, payload);
}

async fn translate_cropped(
    app: &AppHandle,
    cropped_png: &[u8],
) -> Result<(String, String, Vec<BlockPayload>), String> {
    let engine = app
        .state::<AppState>()
        .engine
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "翻译模型尚未就绪".to_string())?;

    let ocr = crate::ocr_provider(app).ok_or_else(|| "OCR 模型不可用".to_string())?;

    let (source, target) = {
        let config = app.state::<std::sync::Mutex<TranslateConfig>>();
        let config = config.lock().unwrap();
        (config.source.clone(), config.target.clone())
    };

    let engine_ref: EngineRef =
        std::sync::Arc::new(TimeoutEngine::new(engine, crate::server::TIMEOUT));

    let translation = translator_service::image::translate_image_bytes(
        &engine_ref,
        &ocr,
        cropped_png,
        &source,
        &target,
        &[],
        crate::server::MAX_CHARS,
    )
    .await
    .map_err(|error| error.to_string())?;

    if translation.blocks.is_empty() {
        return Err("没有识别到可翻译的文字".to_string());
    }

    let source_text = translation
        .blocks
        .iter()
        .map(|block| block.text.trim())
        .collect::<Vec<_>>()
        .join("\n");
    let translation_text = translation
        .blocks
        .iter()
        .map(|block| block.translation.trim())
        .collect::<Vec<_>>()
        .join("\n");

    let blocks = translation
        .blocks
        .into_iter()
        .map(|block| BlockPayload {
            text: block.text,
            translation: block.translation,
            quad: block.quad,
        })
        .collect();

    Ok((source_text, translation_text, blocks))
}

fn show_viewer(
    app: &AppHandle,
    region: Selection,
    slice: Slice,
    monitor: MonitorBox,
    payload: ViewerPayload,
) {
    let Some(viewer) = app.get_webview_window(VIEWER_LABEL) else {
        return;
    };

    // Logical rect of the region on screen.
    let region_x = slice.origin_x + region.x as f64 / slice.scale;
    let region_y = slice.origin_y + region.y as f64 / slice.scale;
    let image_width = region.width as f64 / slice.scale;
    let image_height = region.height as f64 / slice.scale;

    // Shrink to fit the monitor if the region is large.
    let max_width = monitor.width - 2.0 * VIEWER_MARGIN - 2.0 * VIEWER_PADDING;
    let max_height = monitor.height - 2.0 * VIEWER_MARGIN - VIEWER_TOOLBAR - 2.0 * VIEWER_PADDING;
    let fit = 1.0_f64
        .min(max_width / image_width)
        .min(max_height / image_height)
        .max(0.1);

    let width = image_width * fit + 2.0 * VIEWER_PADDING;
    let height = image_height * fit + VIEWER_TOOLBAR + 2.0 * VIEWER_PADDING;

    let min_x = monitor.x + VIEWER_MARGIN;
    let min_y = monitor.y + VIEWER_MARGIN;
    let max_x = (monitor.x + monitor.width - width - VIEWER_MARGIN).max(min_x);
    let max_y = (monitor.y + monitor.height - height - VIEWER_MARGIN).max(min_y);

    // Keep the image over the region where possible: toolbar above, padding
    // around, clamped into the monitor.
    let x = (region_x - VIEWER_PADDING).max(min_x).min(max_x);
    let y = (region_y - VIEWER_TOOLBAR - VIEWER_PADDING)
        .max(min_y)
        .min(max_y);

    // Show first so the webview is laid out before the payload arrives (the
    // overlay needs the displayed image width); geometry calls on an
    // unrealized window are dropped on Linux, so size/position follow `show`.
    // A viewer the user has moved keeps its position on refresh.
    let was_visible = viewer.is_visible().unwrap_or(false);

    let _ = viewer.show();
    if !was_visible {
        let _ = viewer.set_position(LogicalPosition::new(x, y));
    }
    let _ = viewer.set_size(LogicalSize::new(width, height));
    let _ = viewer.set_always_on_top(true);
    let _ = viewer.set_focus();
    let _ = app.emit_to(VIEWER_LABEL, "shot-result", payload);

    eprintln!(
        "SHOTDBG viewer at ({x:.0},{y:.0}) {width:.0}x{height:.0}, image {image_width:.0}x{image_height:.0} fit {fit:.3}"
    );
}

fn record_history(app: &AppHandle, source_text: String, translation_text: String) {
    let (source, target) = {
        let config = app.state::<std::sync::Mutex<TranslateConfig>>();
        let config = config.lock().unwrap();
        (config.source.clone(), config.target.clone())
    };

    let state = app.state::<AppState>();
    let mut history = state.history.lock().unwrap();

    translator_core::history::push(
        &mut history,
        HistoryEntry {
            source,
            target,
            text: source_text,
            translation: translation_text,
            at: Some(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|elapsed| elapsed.as_millis() as u64)
                    .unwrap_or(0),
            ),
        },
    );
    translator_core::history::save(&history);
}

/// Captures the screen into PNG bytes plus its dimensions in capture pixels.
///
/// Linux asks the XDG portal, which returns the whole virtual desktop or one
/// monitor; Windows captures the target monitor through the Windows Graphics
/// Capture API (`xcap`), so a mixed-DPI multi-monitor desktop never needs
/// stitching.
#[cfg(target_os = "linux")]
async fn capture_screen(_target: MonitorBox) -> Result<(Vec<u8>, u32, u32), String> {
    let request = Screenshot::request()
        .interactive(false)
        .send()
        .await
        .map_err(|error| format!("截图失败：{error}"))?;
    let response = request
        .response()
        .map_err(|error| format!("截图失败：{error}"))?;

    let path = url::Url::parse(response.uri().as_str())
        .ok()
        .and_then(|url| url.to_file_path().ok())
        .ok_or_else(|| "截图失败：无法读取截图文件".to_string())?;

    let bytes = std::fs::read(&path).map_err(|error| format!("截图失败：{error}"))?;

    // GNOME's portal writes the capture into the Pictures folder; the file is
    // ours, so clean it up after reading.
    let _ = std::fs::remove_file(&path);

    let (width, height) = image::ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|error| format!("截图失败：{error}"))?
        .into_dimensions()
        .map_err(|error| format!("截图失败：{error}"))?;

    Ok((bytes, width, height))
}

/// Windows capture through the Windows Graphics Capture API. Tauri reports
/// monitor rects in physical pixels on Windows, exactly what the API returns,
/// so the target monitor is found by rect (with the primary monitor as a
/// fallback). Capturing only that monitor keeps the mixed-DPI geometry out.
#[cfg(target_os = "windows")]
async fn capture_screen(target: MonitorBox) -> Result<(Vec<u8>, u32, u32), String> {
    tokio::task::spawn_blocking(move || capture_monitor_windows(target))
        .await
        .map_err(|error| format!("截图失败：{error}"))?
}

#[cfg(target_os = "windows")]
fn capture_monitor_windows(target: MonitorBox) -> Result<(Vec<u8>, u32, u32), String> {
    // WinRT needs an apartment on the calling thread; blocking-pool threads
    // are reused, so S_FALSE / RPC_E_CHANGED_MODE just mean it is already set.
    unsafe {
        use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }

    let want = (
        (target.x * target.scale).round() as i32,
        (target.y * target.scale).round() as i32,
        (target.width * target.scale).round() as u32,
        (target.height * target.scale).round() as u32,
    );

    let monitors = xcap::Monitor::all().map_err(|error| format!("截图失败：{error}"))?;

    let monitor = monitors
        .iter()
        .find(|monitor| monitor_matches(monitor, want))
        .or_else(|| {
            monitors
                .iter()
                .find(|monitor| monitor.is_primary().unwrap_or(false))
        })
        .or_else(|| monitors.first())
        .ok_or_else(|| "截图失败：找不到显示器".to_string())?;

    let image = monitor
        .capture_image()
        .map_err(|error| format!("截图失败：{error}"))?;
    let (width, height) = (image.width(), image.height());

    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|error| format!("截图编码失败：{error}"))?;

    Ok((png, width, height))
}

#[cfg(target_os = "windows")]
fn monitor_matches(monitor: &xcap::Monitor, want: (i32, i32, u32, u32)) -> bool {
    let (Ok(x), Ok(y), Ok(width), Ok(height)) = (
        monitor.x(),
        monitor.y(),
        monitor.width(),
        monitor.height(),
    ) else {
        return false;
    };

    (x - want.0).abs() <= 2
        && (y - want.1).abs() <= 2
        && width == want.2
        && height == want.3
}

/// All monitors in logical coordinates.
fn monitor_boxes(app: &AppHandle) -> Vec<MonitorBox> {
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|monitor| {
            let scale = monitor.scale_factor();
            let position = monitor.position().to_logical::<f64>(scale);
            let size = monitor.size().to_logical::<f64>(scale);

            MonitorBox {
                x: position.x,
                y: position.y,
                width: size.width,
                height: size.height,
                scale,
            }
        })
        .collect()
}

/// The monitor under the cursor.
fn cursor_monitor(app: &AppHandle) -> Option<MonitorBox> {
    let monitors = monitor_boxes(app);
    let first_scale = monitors.first().map(|m| m.scale).unwrap_or(1.0);

    let cursor = app
        .cursor_position()
        .ok()
        .map(|position| position.to_logical::<f64>(first_scale))
        .unwrap_or((0.0, 0.0).into());

    monitors
        .iter()
        .find(|monitor| {
            cursor.x >= monitor.x
                && cursor.x < monitor.x + monitor.width
                && cursor.y >= monitor.y
                && cursor.y < monitor.y + monitor.height
        })
        .or_else(|| monitors.first())
        .copied()
}

/// Returns the target monitor's slice inside a capture that may span the whole
/// virtual desktop or a single monitor.
pub(crate) fn monitor_slice(
    capture: (u32, u32),
    monitors: &[MonitorBox],
    target: MonitorBox,
) -> Option<Slice> {
    let (capture_width, capture_height) = capture;

    if monitors.is_empty() || capture_width == 0 || capture_height == 0 {
        return None;
    }

    let min_x = monitors.iter().map(|m| m.x).fold(f64::MAX, f64::min);
    let min_y = monitors.iter().map(|m| m.y).fold(f64::MAX, f64::min);
    let max_x = monitors
        .iter()
        .map(|m| m.x + m.width)
        .fold(f64::MIN, f64::max);
    let max_y = monitors
        .iter()
        .map(|m| m.y + m.height)
        .fold(f64::MIN, f64::max);

    let union_width = max_x - min_x;
    let union_height = max_y - min_y;

    // The capture is either the full virtual desktop (Linux portal) or one
    // monitor (a portal single-monitor result or the Windows per-monitor
    // capture).
    let (span_x, span_y, scale) = if union_width > 0.0
        && (capture_width as f64 - union_width).abs() <= 2.0
        && (capture_height as f64 - union_height).abs() <= 2.0
    {
        (min_x, min_y, capture_width as f64 / union_width)
    } else if target.width > 0.0 && target.height > 0.0 {
        (target.x, target.y, capture_width as f64 / target.width)
    } else {
        return None;
    };

    let x = ((target.x - span_x) * scale).round().max(0.0) as u32;
    let y = ((target.y - span_y) * scale).round().max(0.0) as u32;
    let width = ((target.width * scale).round() as u32)
        .min(capture_width.saturating_sub(x))
        .max(1);
    let height = ((target.height * scale).round() as u32)
        .min(capture_height.saturating_sub(y))
        .max(1);

    Some(Slice {
        x,
        y,
        width,
        height,
        scale,
        origin_x: target.x,
        origin_y: target.y,
        monitor_width: target.width,
        monitor_height: target.height,
    })
}

/// Maps a CSS-pixel drag inside the selector to capture coordinates.
///
/// The window manager may constrain the selector to the monitor's work area
/// (dock, top bar), so the window position is part of the mapping — assuming
/// it covers the whole monitor shifted every selection by the work-area
/// offset, growing with the selection's width.
pub(crate) fn map_selection(
    css: (f64, f64, f64, f64),
    window_physical: (f64, f64),
    monitor: MonitorBox,
) -> Option<Selection> {
    if monitor.scale <= 0.0 {
        return None;
    }

    // Window origin relative to the monitor, in logical pixels.
    let origin_x = window_physical.0 / monitor.scale - monitor.x;
    let origin_y = window_physical.1 / monitor.scale - monitor.y;

    let logical_x = origin_x + css.0;
    let logical_y = origin_y + css.1;

    let monitor_capture_width = (monitor.width * monitor.scale).round() as u32;
    let monitor_capture_height = (monitor.height * monitor.scale).round() as u32;

    let x = (logical_x * monitor.scale).round().max(0.0) as u32;
    let y = (logical_y * monitor.scale).round().max(0.0) as u32;
    let width = (css.2 * monitor.scale).round() as u32;
    let height = (css.3 * monitor.scale).round() as u32;

    let x = x.min(monitor_capture_width.saturating_sub(1));
    let y = y.min(monitor_capture_height.saturating_sub(1));
    let width = width.min(monitor_capture_width - x);
    let height = height.min(monitor_capture_height - y);

    if width < MIN_SELECTION || height < MIN_SELECTION {
        return None;
    }

    Some(Selection {
        x,
        y,
        width,
        height,
    })
}

/// Crops the selection out of the full capture and returns a data URL plus
/// the encoded PNG for OCR.
fn crop_region(png: &[u8], region: Selection, slice: Slice) -> Result<(String, Vec<u8>), String> {
    let full = image::load_from_memory(png).map_err(|error| format!("截图解析失败：{error}"))?;
    let rgb = full.to_rgb8();

    let x = slice.x + region.x;
    let y = slice.y + region.y;

    if x + region.width > rgb.width() || y + region.height > rgb.height() {
        return Err("选区超出屏幕范围".to_string());
    }

    let cropped = image::imageops::crop_imm(&rgb, x, y, region.width, region.height).to_image();
    let mut encoded = Vec::new();

    image::DynamicImage::ImageRgb8(cropped)
        .write_to(&mut Cursor::new(&mut encoded), image::ImageFormat::Png)
        .map_err(|error| format!("截图编码失败：{error}"))?;

    let image = format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&encoded)
    );

    Ok((image, encoded))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(x: f64, y: f64, width: f64, height: f64, scale: f64) -> MonitorBox {
        MonitorBox {
            x,
            y,
            width,
            height,
            scale,
        }
    }

    #[test]
    fn single_monitor_slice_covers_the_capture() {
        let monitors = [monitor(0.0, 0.0, 1920.0, 1080.0, 1.0)];
        let result = monitor_slice((1920, 1080), &monitors, monitors[0]).unwrap();

        assert_eq!(
            (result.x, result.y, result.width, result.height),
            (0, 0, 1920, 1080)
        );
        assert_eq!(result.scale, 1.0);
    }

    #[test]
    fn hidpi_slice_maps_logical_monitor_to_capture_pixels() {
        let monitors = [monitor(0.0, 0.0, 1280.0, 720.0, 1.5)];
        let result = monitor_slice((1920, 1080), &monitors, monitors[0]).unwrap();

        assert_eq!(
            (result.x, result.y, result.width, result.height),
            (0, 0, 1920, 1080)
        );
        assert_eq!(result.scale, 1.5);
    }

    #[test]
    fn multi_monitor_capture_slices_the_target_monitor() {
        let monitors = [
            monitor(0.0, 0.0, 1920.0, 1080.0, 1.0),
            monitor(1920.0, 0.0, 1920.0, 1080.0, 1.0),
        ];
        let result = monitor_slice((3840, 1080), &monitors, monitors[1]).unwrap();

        assert_eq!(
            (result.x, result.y, result.width, result.height),
            (1920, 0, 1920, 1080)
        );
        assert_eq!(result.origin_x, 1920.0);
    }

    #[test]
    fn selection_uses_the_window_position_on_screen() {
        // GNOME constrained the selector to the work area (dock 67px, top bar
        // 32px): a drag at CSS (426,150) is at screen (493,182).
        let canvas = monitor(0.0, 0.0, 1920.0, 1080.0, 1.0);
        let selection = map_selection((426.0, 150.0, 663.0, 58.0), (67.0, 32.0), canvas).unwrap();

        assert_eq!(
            selection,
            Selection {
                x: 493,
                y: 182,
                width: 663,
                height: 58
            }
        );
    }

    #[test]
    fn selection_scales_with_the_device_factor() {
        let canvas = monitor(0.0, 0.0, 1280.0, 720.0, 1.5);
        let selection = map_selection((100.0, 50.0, 200.0, 80.0), (0.0, 0.0), canvas).unwrap();

        assert_eq!(
            selection,
            Selection {
                x: 150,
                y: 75,
                width: 300,
                height: 120
            }
        );
    }

    #[test]
    fn tiny_and_out_of_bounds_selections_are_rejected_or_clamped() {
        let canvas = monitor(0.0, 0.0, 1920.0, 1080.0, 1.0);

        assert!(map_selection((10.0, 10.0, 2.0, 2.0), (0.0, 0.0), canvas).is_none());

        let clamped = map_selection((1900.0, 1000.0, 500.0, 500.0), (0.0, 0.0), canvas).unwrap();

        assert_eq!(
            clamped,
            Selection {
                x: 1900,
                y: 1000,
                width: 20,
                height: 80
            }
        );
    }
}
