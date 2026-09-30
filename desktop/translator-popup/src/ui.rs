use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender, channel};

use gtk4 as gtk;
use gtk::gio::prelude::*;
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    Align, Application, ApplicationWindow, Box as GtkBox, Button, EventControllerKey, Label,
    Orientation, ScrolledWindow, Separator,
};

use crate::Args;
use crate::services::{self, ServiceConfig};
use crate::translate;
use crate::{read_selection, read_stdin};

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
            let mut slot = state.borrow_mut();
            let old = slot.take();

            let hold_guard = old.as_ref().map(|ui| {
                let guard = app.hold();
                ui.window.close();
                guard
            });

            *slot = Some(build_ui(app, &args, &mode));

            drop(hold_guard);
            drop(old);
            drop(slot);

            refresh(&state);
        }
    });

    let no_args: [&str; 0] = [];
    application.run_with_args(&no_args);
    0
}

fn build_ui(app: &Application, args: &Args, mode: &Mode) -> Ui {
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

    let buttons = GtkBox::new(Orientation::Horizontal, 8);
    buttons.set_halign(Align::End);
    buttons.append(&copy_button);
    buttons.append(&close_button);

    container.append(&source_label);
    container.append(&Separator::new(Orientation::Horizontal));
    container.append(&scroller);
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
        args: args.clone(),
        mode: mode.clone(),
    }
}

fn refresh(state: &Rc<RefCell<Option<Ui>>>) {
    let slot = state.borrow();
    let Some(ui) = slot.as_ref() else {
        return;
    };

    let text = match &ui.mode {
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
    };

    let Some(text) = text else {
        ui.source_label.set_text("");
        ui.copy_button.set_sensitive(false);
        ui.store.borrow_mut().clear();
        return;
    };

    ui.source_label.set_text(&text);
    ui.translation_label.set_text("正在准备翻译服务…");
    ui.copy_button.set_sensitive(false);
    ui.copy_button.set_label("复制");
    ui.store.borrow_mut().clear();

    let (sender, receiver) = channel();
    *ui.receiver.borrow_mut() = Some(receiver);

    spawn_worker(&ui.args, text, sender);
}

fn spawn_worker(args: &Args, text: String, sender: Sender<Progress>) {
    let args = args.clone();

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
                &args.target,
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
