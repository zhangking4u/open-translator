use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui;
use translator_core::args::{Args, read_stdin};
use translator_core::services::{self, ServiceConfig};
use translator_core::settings::persist_target;
use translator_core::translate;

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

enum Progress {
    Translating,
    Done(String),
    Error(String),
}

pub struct PopupApp {
    args: Args,
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

impl PopupApp {
    pub fn new(cc: &eframe::CreationContext<'_>, args: Args, hotkey_spec: &str) -> Self {
        let mut app = Self {
            target: args.target.clone(),
            args,
            source_text: String::new(),
            translation: String::new(),
            status: String::new(),
            error: false,
            receiver: None,
            hotkey: None,
            tray: None,
            quit: false,
        };

        match Hotkey::register(hotkey_spec) {
            Ok(hotkey) => app.hotkey = Some(hotkey),
            Err(error) => app.show_error(&error),
        }

        match Tray::new(&format!("OpenTranslator（{hotkey_spec}）")) {
            Ok(tray) => app.tray = Some(tray),
            Err(error) => {
                if !app.error {
                    app.show_error(&error);
                }
            }
        }

        let initial = if app.args.stdin {
            read_stdin().unwrap_or_default()
        } else {
            String::new()
        };

        if !initial.is_empty() {
            app.begin_translation(initial);
        } else if !app.args.stdin {
            app.trigger();
        }

        cc.egui_ctx
            .send_viewport_cmd(egui::ViewportCommand::Visible(true));
        cc.egui_ctx
            .send_viewport_cmd(egui::ViewportCommand::Focus);

        app
    }

    fn trigger(&mut self) {
        match capture::capture_selection() {
            Ok(text) => self.begin_translation(text),
            Err(error) => self.show_error(&error),
        }
    }

    fn begin_translation(&mut self, text: String) {
        self.source_text = text.clone();
        self.translation.clear();
        self.status = "翻译中…".to_string();
        self.error = false;

        let (sender, receiver) = channel();
        self.receiver = Some(receiver);
        spawn_worker(self.args.clone(), self.target.clone(), text, sender);
    }

    fn show_error(&mut self, message: &str) {
        self.status = message.to_string();
        self.error = true;
    }

    fn hide(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
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
                Progress::Translating => {
                    self.status = "翻译中…".to_string();
                    self.error = false;
                }
                Progress::Done(translation) => {
                    self.translation = translation;
                    self.status.clear();
                    self.error = false;
                }
                Progress::Error(error) => self.show_error(&error),
            }
        }

        if ctx.input(|input| input.viewport().close_requested()) {
            if self.quit {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.hide(ctx);
            }
        }

        if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
            self.hide(ctx);
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

fn spawn_worker(args: Args, target: String, text: String, sender: Sender<Progress>) {
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                let _ = sender.send(Progress::Error(format!("failed to start runtime: {error}")));
                return;
            }
        };

        runtime.block_on(async move {
            let client = match translate::build_client() {
                Ok(client) => client,
                Err(error) => {
                    let _ = sender.send(Progress::Error(error));
                    return;
                }
            };

            let config = ServiceConfig::from_env(&args.service_url, !args.no_start);

            if let Err(error) = services::ensure(&client, &config).await {
                let _ = sender.send(Progress::Error(error));
                return;
            }

            let _ = sender.send(Progress::Translating);

            let outcome =
                translate::translate(&client, &args.service_url, &args.source, &target, &text)
                    .await;

            let _ = match outcome {
                Ok(translation) => sender.send(Progress::Done(translation)),
                Err(error) => sender.send(Progress::Error(error)),
            };
        });
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
