use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender, channel};

use gtk4 as gtk;
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    Align, Application, ApplicationWindow, Box as GtkBox, Button, ComboBoxText,
    EventControllerKey, EventControllerScroll, EventControllerScrollFlags, Label, Orientation,
    ScrolledWindow, Separator,
};

use crate::Args;
use crate::services::{self, ServiceConfig};
use crate::translate;
use crate::{persist_target, read_selection, read_stdin};

const TARGET_LANGUAGES: &[(&str, &str)] = &[
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
enum Mode {
    Selection { clipboard: bool },
    Stdin(Result<String, String>),
}

enum Progress {
    Translating,
    Done(String),
    Error(String),
}

struct Ui {
    window: ApplicationWindow,
    source_label: Label,
    translation_label: Label,
    copy_button: Button,
    receiver: Rc<RefCell<Option<Receiver<Progress>>>>,
    store: Rc<RefCell<String>>,
    source_text: RefCell<String>,
    target: RefCell<String>,
    args: Args,
    mode: Mode,
}

pub fn run(args: Args) -> i32 {
    let mode = if args.stdin {
        Mode::Stdin(read_stdin())
    } else {
        Mode::Selection {
            clipboard: args.clipboard,
        }
    };

    let application = Application::builder()
        .application_id("io.github.opentranslator.popup")
        .build();

    let state: Rc<RefCell<Option<Ui>>> = Rc::new(RefCell::new(None));

    application.connect_activate({
        let state = state.clone();

        move |app| {
            let state_for_ui = state.clone();

            {
                let mut slot = state.borrow_mut();

                if let Some(ui) = slot.as_mut() {
                    ui.window.present();
                } else {
                    *slot = Some(build_ui(app, &args, &mode, state_for_ui));
                }
            }

            refresh(&state);
        }
    });

    let no_args: [&str; 0] = [];
    application.run_with_args(&no_args);
    0
}

fn build_ui(
    app: &Application,
    args: &Args,
    mode: &Mode,
    state: Rc<RefCell<Option<Ui>>>,
) -> Ui {
    let title = format!("OpenTranslator ({} → {})", args.source, args.target);

    let window = ApplicationWindow::builder()
        .application(app)
        .title(&title)
        .default_width(560)
        .default_height(300)
        .build();

    let container = GtkBox::new(Orientation::Vertical, 8);
    container.set_margin_top(12);
    container.set_margin_bottom(12);
    container.set_margin_start(12);
    container.set_margin_end(12);

    let source_label = Label::new(None);
    source_label.set_xalign(0.0);
    source_label.set_wrap(true);
    source_label.set_lines(3);
    source_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    source_label.add_css_class("dim-label");

    let translation_label = Label::new(None);
    translation_label.set_xalign(0.0);
    translation_label.set_wrap(true);
    translation_label.set_selectable(true);

    let scroller = ScrolledWindow::builder()
        .vexpand(true)
        .child(&translation_label)
        .build();

    let copy_button = Button::with_label("复制");
    copy_button.set_sensitive(false);

    let close_button = Button::with_label("关闭");

    let language_label = Label::new(Some("目标语言"));
    let language_combo = ComboBoxText::new();

    for (code, name) in TARGET_LANGUAGES {
        language_combo.append(Some(code), name);
    }
    if !TARGET_LANGUAGES.iter().any(|(code, _)| *code == args.target) {
        language_combo.append(Some(args.target.as_str()), args.target.as_str());
    }
    language_combo.set_active_id(Some(args.target.as_str()));

    let scroll_controller = EventControllerScroll::new(EventControllerScrollFlags::VERTICAL);
    scroll_controller.connect_scroll(|_, _, _| glib::Propagation::Stop);
    language_combo.add_controller(scroll_controller);

    let language_box = GtkBox::new(Orientation::Horizontal, 8);
    language_box.set_halign(Align::Start);
    language_box.append(&language_label);
    language_box.append(&language_combo);

    let buttons = GtkBox::new(Orientation::Horizontal, 8);
    buttons.set_halign(Align::End);
    buttons.append(&copy_button);
    buttons.append(&close_button);

    container.append(&source_label);
    container.append(&Separator::new(Orientation::Horizontal));
    container.append(&scroller);
    container.append(&language_box);
    container.append(&buttons);

    window.set_child(Some(&container));

    let receiver: Rc<RefCell<Option<Receiver<Progress>>>> = Rc::new(RefCell::new(None));
    let store: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));

    glib::timeout_add_local(std::time::Duration::from_millis(100), {
        let receiver = receiver.clone();
        let translation_label = translation_label.clone();
        let copy_button = copy_button.clone();
        let store = store.clone();

        move || {
            let messages: Vec<Progress> = receiver
                .borrow()
                .as_ref()
                .map(|receiver| receiver.try_iter().collect())
                .unwrap_or_default();

            for message in messages {
                match message {
                    Progress::Translating => translation_label.set_text("翻译中…"),
                    Progress::Done(translation) => {
                        translation_label.set_text(&translation);
                        *store.borrow_mut() = translation;
                        copy_button.set_sensitive(true);
                    }
                    Progress::Error(error) => {
                        translation_label.set_text(&format!("翻译失败：{error}"));
                    }
                }
            }

            glib::ControlFlow::Continue
        }
    });

    copy_button.connect_clicked({
        let window = window.clone();
        let copy_button = copy_button.clone();
        let store = store.clone();

        move |_| {
            let text = store.borrow().clone();
            if !text.is_empty() {
                window.clipboard().set_text(&text);
                copy_button.set_label("已复制");
            }
        }
    });

    close_button.connect_clicked({
        let window = window.clone();
        move |_| window.close()
    });

    language_combo.connect_changed({
        let state = state.clone();
        let window = window.clone();
        let source = args.source.clone();

        move |combo| {
            let Some(target) = combo.active_id() else {
                return;
            };
            let target = target.to_string();

            let source_text = {
                let slot = state.borrow();
                let Some(ui) = slot.as_ref() else {
                    return;
                };
                ui.target.replace(target.clone());
                ui.source_text.borrow().clone()
            };

            persist_target(&target);
            window.set_title(Some(&format!("OpenTranslator ({source} → {target})")));

            if source_text.is_empty() {
                refresh(&state);
            } else {
                begin_translation(&state, source_text);
            }
        }
    });

    let key_controller = EventControllerKey::new();
    key_controller.connect_key_pressed({
        let window = window.clone();

        move |_, key, _, _| {
            if key == gtk::gdk::Key::Escape {
                window.close();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        }
    });
    window.add_controller(key_controller);

    window.present();

    Ui {
        window,
        source_label,
        translation_label,
        copy_button,
        receiver,
        store,
        source_text: RefCell::new(String::new()),
        target: RefCell::new(args.target.clone()),
        args: args.clone(),
        mode: mode.clone(),
    }
}

fn refresh(state: &Rc<RefCell<Option<Ui>>>) {
    let text = {
        let slot = state.borrow();
        let Some(ui) = slot.as_ref() else {
            return;
        };

        match &ui.mode {
            Mode::Selection { clipboard } => match read_selection(*clipboard) {
                Ok(text) if !text.is_empty() => Some(text),
                Ok(_) => {
                    ui.translation_label.set_text("未选中文本");
                    None
                }
                Err(error) => {
                    ui.translation_label.set_text(&error);
                    None
                }
            },
            Mode::Stdin(Ok(text)) if !text.is_empty() => Some(text.clone()),
            Mode::Stdin(Ok(_)) => {
                ui.translation_label.set_text("未选中文本");
                None
            }
            Mode::Stdin(Err(error)) => {
                ui.translation_label.set_text(error);
                None
            }
        }
    };

    match text {
        Some(text) => begin_translation(state, text),
        None => {
            let slot = state.borrow();
            if let Some(ui) = slot.as_ref() {
                ui.source_label.set_text("");
                ui.source_text.borrow_mut().clear();
                ui.copy_button.set_sensitive(false);
                ui.store.borrow_mut().clear();
            }
        }
    }
}

fn begin_translation(state: &Rc<RefCell<Option<Ui>>>, text: String) {
    let slot = state.borrow();
    let Some(ui) = slot.as_ref() else {
        return;
    };

    ui.source_text.replace(text.clone());
    ui.source_label.set_text(&text);
    ui.translation_label.set_text("正在准备翻译服务…");
    ui.copy_button.set_sensitive(false);
    ui.copy_button.set_label("复制");
    ui.store.borrow_mut().clear();

    let (sender, receiver) = channel();
    *ui.receiver.borrow_mut() = Some(receiver);

    let target = ui.target.borrow().clone();
    spawn_worker(&ui.args, &target, text, sender);
}

fn spawn_worker(args: &Args, target: &str, text: String, sender: Sender<Progress>) {
    let args = args.clone();
    let target = target.to_string();

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

            let outcome = translate::translate(
                &client,
                &args.service_url,
                &args.source,
                &target,
                &text,
            )
            .await;

            let _ = match outcome {
                Ok(translation) => sender.send(Progress::Done(translation)),
                Err(error) => sender.send(Progress::Error(error)),
            };
        });
    });
}
