use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui;
use translator_core::args::{Args, read_stdin};
use translator_core::settings::persist_target;
use translator_service::domain::prompt::PromptStyle;
use translator_service::domain::translation::TranslationRequest;
use translator_service::engine::llama_cpp::LlamaCppEngine;

use crate::capture;
use crate::hotkey::Hotkey;
use crate::tray::{Tray, TrayCommand};

pub const LANGUAGES: &[(&str, &str)] = &[
    ("zh", "中文"),
    ("en", "英语"),
    ("ja", "日语"),
    ("ko", "韩语"),
    ("fr", "法语"),
    ("de", "德语"),
    ("es", "西班牙语"),
    ("ru", "俄语"),
];

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
    Done(String),
    Error(String),
}

pub struct PopupApp {
    args: Args,
    engine: Option<Arc<LlamaCppEngine>>,
    startup_receiver: Option<Receiver<StartupEvent>>,
    server_plan: Option<ServerPlan>,
    server_started: bool,
    source_text: String,
    translation: String,
    status: String,
    error: bool,
    target: String,
    receiver: Option<Receiver<Progress>>,
    hotkey: Option<Hotkey>,
    tray: Option<Tray>,
    quit: bool,
}

fn system_cjk_font() -> Option<(String, Vec<u8>)> {
    let candidates: Vec<PathBuf> = if cfg!(target_os = "windows") {
        let windir = std::env::var_os("WINDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        let fonts = windir.join("Fonts");

        ["msyh.ttc", "msyh.ttf", "simhei.ttf", "simsun.ttc", "Deng.ttf"]
            .iter()
            .map(|name| fonts.join(name))
            .collect()
    } else if cfg!(target_os = "macos") {
        [
            "/System/Library/Fonts/PingFang.ttc",
            "/System/Library/Fonts/STHeiti Medium.ttc",
            "/System/Library/Fonts/Hiragino Sans GB.ttc",
            "/Library/Fonts/Arial Unicode.ttf",
        ]
        .iter()
        .map(PathBuf::from)
        .collect()
    } else {
        [
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
            "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
        ]
        .iter()
        .map(PathBuf::from)
        .collect()
    };

    for path in candidates {
        if let Ok(bytes) = std::fs::read(&path) {
            return Some((path.display().to_string(), bytes));
        }
    }

    None
}

fn install_cjk_font(ctx: &egui::Context) {
    let Some((name, bytes)) = system_cjk_font() else {
        return;
    };

    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert(name.clone(), Arc::new(egui::FontData::from_owned(bytes)));

    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push(name.clone());
    }

    ctx.set_fonts(fonts);
}

impl PopupApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        args: Args,
        hotkey_spec: &str,
        startup: Startup,
        server_plan: Option<ServerPlan>,
    ) -> Self {
        install_cjk_font(&cc.egui_ctx);

        let mut app = Self {
            target: args.target.clone(),
            args,
            engine: None,
            startup_receiver: None,
            server_plan,
            server_started: false,
            source_text: String::new(),
            translation: String::new(),
            status: String::new(),
            error: false,
            receiver: None,
            hotkey: None,
            tray: None,
            quit: false,
        };

        let mut show_on_start = false;

        match startup {
            Startup::Loaded(Ok(engine)) => {
                app.engine = Some(engine);
                app.maybe_start_server();
            }
            Startup::Loaded(Err(error)) => {
                app.show_error(&format!("模型加载失败：{error}"));
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
                app.status = "正在下载模型…".to_string();
                spawn_startup(dest, url, sha256, prompt_style, sender);
                show_on_start = true;
            }
        }

        match Hotkey::register(hotkey_spec) {
            Ok(hotkey) => app.hotkey = Some(hotkey),
            Err(error) => {
                if !app.error {
                    app.show_error(&error);
                }
            }
        }

        match Tray::new(&format!("OpenTranslator（{hotkey_spec}）")) {
            Ok(tray) => app.tray = Some(tray),
            Err(error) => {
                if !app.error {
                    app.show_error(&error);
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
            cc.egui_ctx
                .send_viewport_cmd(egui::ViewportCommand::Visible(true));
            cc.egui_ctx
                .send_viewport_cmd(egui::ViewportCommand::Focus);
        }

        app
    }

    fn trigger(&mut self) {
        match capture::capture_selection() {
            Ok(text) => self.begin_translation(text),
            Err(error) => self.show_error(&error),
        }
    }

    fn begin_translation(&mut self, text: String) {
        let Some(engine) = self.engine.clone() else {
            self.source_text = text;
            self.translation.clear();

            if !self.status.starts_with("正在下载模型") {
                self.status = "模型尚未就绪，请稍候…".to_string();
                self.error = false;
            }

            return;
        };

        self.source_text = text.clone();
        self.translation.clear();
        self.status = "翻译中…".to_string();
        self.error = false;

        let (sender, receiver) = channel();
        self.receiver = Some(receiver);

        spawn_worker(
            engine,
            self.args.source.clone(),
            self.target.clone(),
            text,
            sender,
        );
    }

    fn show_error(&mut self, message: &str) {
        self.status = message.to_string();
        self.error = true;
    }

    fn hide(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
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
                self.status = format!("扩展服务未启动：{address} 已被其他服务占用");
                self.error = false;
            }
            Err(error) => {
                if !self.error {
                    self.show_error(&format!("扩展服务启动失败：{error}"));
                }
            }
        }
    }

    fn can_restore(&self) -> bool {
        (self.hotkey.is_some() && Hotkey::is_supported())
            || (self.tray.is_some() && Tray::is_supported())
    }

    fn title(&self) -> String {
        format!("OpenTranslator ({} → {})", self.args.source, self.target)
    }
}

impl eframe::App for PopupApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let hotkey_pressed = self
            .hotkey
            .as_ref()
            .map(|hotkey| hotkey.pressed())
            .unwrap_or(false);

        if hotkey_pressed {
            self.trigger();
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }

        let tray_command = self.tray.as_ref().and_then(|tray| tray.poll());

        match tray_command {
            Some(TrayCommand::Show) => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
            Some(TrayCommand::Translate) => {
                self.trigger();
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
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
                Progress::Done(translation) => {
                    self.translation = translation;
                    self.status.clear();
                    self.error = false;
                }
                Progress::Error(error) => self.show_error(&error),
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
                    self.error = false;
                    self.status = format_download_status(downloaded, total);
                }
                StartupEvent::Ready(engine) => {
                    self.engine = Some(engine);
                    self.status.clear();
                    self.error = false;
                    self.maybe_start_server();
                }
                StartupEvent::Failed(error) => self.show_error(&error),
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
            if self.can_restore() {
                self.hide(ctx);
            } else {
                self.quit = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }

        // Keep polling while the window is hidden (hotkey events arrive via `logic`).
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let mut change_target: Option<String> = None;
        let mut copy: Option<String> = None;
        let mut hide = false;
        let mut quit = false;

        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("目标语言");

                let selected = target_label(&self.target).to_string();
                egui::ComboBox::from_id_salt("target")
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        for (code, name) in LANGUAGES {
                            let is_selected = self.target == *code;
                            if ui.selectable_label(is_selected, *name).clicked() && !is_selected {
                                change_target = Some((*code).to_string());
                            }
                        }
                    });
            });

            ui.separator();

            ui.add(
                egui::Label::new(egui::RichText::new(&self.source_text).weak()).selectable(false),
            );

            ui.separator();

            let text = if !self.translation.is_empty() {
                egui::RichText::new(&self.translation)
            } else if self.error {
                egui::RichText::new(&self.status).color(egui::Color32::RED)
            } else if !self.status.is_empty() {
                egui::RichText::new(&self.status).weak()
            } else {
                egui::RichText::new("等待划词…").weak()
            };

            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add(egui::Label::new(text).selectable(true).wrap());
            });

            ui.separator();

            ui.horizontal(|ui| {
                if ui.button("复制").clicked() && !self.translation.is_empty() {
                    copy = Some(self.translation.clone());
                }
                if ui.button("隐藏").clicked() {
                    hide = true;
                }
                if ui.button("退出").clicked() {
                    quit = true;
                }
            });
        });

        let ctx = ui.ctx().clone();

        if let Some(target) = change_target {
            self.target = target;
            persist_target(&self.target);
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.title()));

            if !self.source_text.is_empty() {
                let text = self.source_text.clone();
                self.begin_translation(text);
            }
        }

        if let Some(text) = copy {
            ctx.copy_text(text);
        }
        if hide {
            self.hide(&ctx);
        }
        if quit {
            self.quit = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

fn target_label(target: &str) -> &str {
    LANGUAGES
        .iter()
        .find(|(code, _)| *code == target)
        .map(|(_, name)| *name)
        .unwrap_or(target)
}

fn format_download_status(downloaded: u64, total: Option<u64>) -> String {
    const MB: f64 = 1_000_000.0;

    match total {
        Some(total) if total > 0 => format!(
            "正在下载模型：{:.0}%（{:.0}/{:.0} MB）",
            downloaded as f64 / total as f64 * 100.0,
            downloaded as f64 / MB,
            total as f64 / MB
        ),
        _ => format!("正在下载模型：已下载 {:.0} MB", downloaded as f64 / MB),
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

fn spawn_worker(
    engine: Arc<LlamaCppEngine>,
    source: String,
    target: String,
    text: String,
    sender: Sender<Progress>,
) {
    std::thread::spawn(move || {
        let request = TranslationRequest { text, source, target };

        let _ = match engine.translate_blocking(&request) {
            Ok(result) => sender.send(Progress::Done(result.translated_text)),
            Err(error) => sender.send(Progress::Error(error.to_string())),
        };
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_known_target_labels() {
        assert_eq!(target_label("zh"), "中文");
        assert_eq!(target_label("ja"), "日语");
        assert_eq!(target_label("xx"), "xx");
    }
}
