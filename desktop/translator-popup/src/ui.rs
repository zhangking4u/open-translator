use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use gtk4 as gtk;
use gtk::glib;
use gtk::prelude::*;
use gtk::{
    Align, Application, ApplicationWindow, Box as GtkBox, Button, ComboBoxText, CssProvider,
    EventControllerKey, EventControllerScroll, EventControllerScrollFlags, Label, Orientation,
    ProgressBar, ScrolledWindow, Spinner,
};

use translator_core::args::{Args, read_stdin};
use translator_core::detect;
use translator_core::languages;
use translator_core::services::{self, ServiceConfig};
use translator_core::settings::{
    load_config, persist_recent_targets, persist_source, persist_target,
};
use translator_core::translate;
use translator_core::update::{self, ReleaseInfo};

use crate::read_selection;

const CSS: &str = "
.ot-card {
    background-color: @theme_base_color;
    border: 1px solid alpha(@borders, 0.55);
    border-radius: 12px;
    padding: 12px;
}
.ot-source-card {
    background-color: alpha(@theme_fg_color, 0.05);
    border-radius: 10px;
    padding: 8px 12px;
}
.ot-title { font-weight: 700; font-size: 15px; }
.ot-translation { font-size: 17px; }
.ot-status { font-size: 12px; }
.ot-primary {
    background-image: none;
    background-color: #4f7cff;
    color: #ffffff;
    border-radius: 8px;
    padding: 6px 14px;
    font-weight: 600;
}
.ot-primary:disabled { opacity: 0.45; }
.ot-banner {
    background-color: alpha(#4f7cff, 0.15);
    border-radius: 10px;
    padding: 6px 10px;
}
.ot-error-card {
    background-color: alpha(@error_color, 0.12);
    border-radius: 10px;
    padding: 10px 12px;
}
.ot-dim { opacity: 0.6; }
.ot-ready { color: #3db57a; }
.ot-busy { color: #4f7cff; }
.ot-error { color: #e05a5a; }
.ot-status-dim { opacity: 0.55; }
";

const WINDOW_WIDTH: i32 = 560;
const MIN_WINDOW_HEIGHT: i32 = 260;
const CHROME_HEIGHT: i32 = 250;
const MAX_WINDOW_FRACTION: f64 = 0.7;

#[derive(Clone)]
enum Mode {
    Selection { clipboard: bool },
    Stdin(Result<String, String>),
}

#[derive(Clone, PartialEq)]
enum TranslationState {
    Idle,
    Empty,
    Preparing {
        text: String,
    },
    Downloading {
        text: String,
        downloaded: u64,
        total: Option<u64>,
    },
    Translating {
        text: String,
        translation: String,
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

enum Progress {
    Downloading { downloaded: u64, total: Option<u64> },
    Translating,
    Delta(String),
    Done {
        translation: String,
        elapsed: Duration,
    },
    Error(String),
}

#[derive(Clone)]
struct Widgets {
    window: ApplicationWindow,
    root: GtkBox,
    scroller: ScrolledWindow,
    source_card: GtkBox,
    source_label: Label,
    source_hint: Label,
    translation_label: Label,
    status_label: Label,
    spinner: Spinner,
    progress_bar: ProgressBar,
    error_card: GtkBox,
    error_label: Label,
    copy_button: Button,
    retranslate_button: Button,
    dot: Label,
    store: Rc<RefCell<String>>,
    last: Rc<RefCell<Option<TranslationState>>>,
}

impl Widgets {
    fn apply(&self, state: &TranslationState) {
        if self.last.borrow().as_ref() == Some(state) {
            return;
        }

        *self.last.borrow_mut() = Some(state.clone());

        match state {
            TranslationState::Idle => {
                self.set_source("");
                self.translation_label.set_text("等待划词");
                self.translation_label.add_css_class("ot-dim");
                self.status_label.set_text("");
                self.set_busy(false);
                self.progress_bar.set_visible(false);
                self.error_card.set_visible(false);
                self.copy_button.set_sensitive(false);
                self.retranslate_button.set_sensitive(false);
                self.store.borrow_mut().clear();
                self.set_dot("ot-ready");
            }
            TranslationState::Empty => {
                self.set_source("");
                self.translation_label.set_text("未选中文本");
                self.translation_label.add_css_class("ot-dim");
                self.status_label.set_text("请先在其它应用中选中要翻译的内容");
                self.set_busy(false);
                self.progress_bar.set_visible(false);
                self.error_card.set_visible(false);
                self.copy_button.set_sensitive(false);
                self.retranslate_button.set_sensitive(false);
                self.store.borrow_mut().clear();
                self.set_dot("ot-ready");
            }
            TranslationState::Preparing { text } => {
                self.set_source(text);
                self.translation_label.set_text("正在准备翻译服务…");
                self.translation_label.add_css_class("ot-dim");
                self.status_label.set_text("准备中…");
                self.set_busy(true);
                self.progress_bar.set_visible(false);
                self.error_card.set_visible(false);
                self.copy_button.set_sensitive(false);
                self.retranslate_button.set_sensitive(false);
                self.store.borrow_mut().clear();
                self.set_dot("ot-busy");
            }
            TranslationState::Downloading {
                text,
                downloaded,
                total,
            } => {
                self.set_source(text);
                self.translation_label.set_text("正在下载模型…");
                self.translation_label.add_css_class("ot-dim");
                self.status_label
                    .set_text(&format!("已下载 {:.0} MB", *downloaded as f64 / 1_000_000.0));
                self.set_busy(true);
                self.progress_bar.set_visible(true);
                self.error_card.set_visible(false);
                self.copy_button.set_sensitive(false);
                self.retranslate_button.set_sensitive(false);
                self.store.borrow_mut().clear();
                self.set_dot("ot-busy");

                match total.filter(|total| *total > 0) {
                    Some(total) => {
                        let fraction = (*downloaded as f64 / total as f64).clamp(0.0, 1.0);
                        self.progress_bar.set_fraction(fraction);
                        self.progress_bar.set_text(Some(&format!(
                            "{:.0}% · {:.0}/{:.0} MB",
                            fraction * 100.0,
                            *downloaded as f64 / 1_000_000.0,
                            total as f64 / 1_000_000.0
                        )));
                    }
                    None => {
                        self.progress_bar.set_fraction(0.0);
                        self.progress_bar.set_text(Some(&translator_core::models::format_download_status(
                            *downloaded, *total,
                        )));
                    }
                }
            }
            TranslationState::Translating { text, translation } => {
                self.set_source(text);

                if translation.trim().is_empty() {
                    self.translation_label.set_text("翻译中…");
                    self.translation_label.add_css_class("ot-dim");
                    self.status_label.set_text("");
                } else {
                    self.translation_label.remove_css_class("ot-dim");
                    self.translation_label.set_text(translation);
                    self.status_label
                        .set_text(&format!("翻译中… {} 字符", translation.chars().count()));
                }

                self.set_busy(true);
                self.progress_bar.set_visible(false);
                self.error_card.set_visible(false);
                self.copy_button.set_sensitive(false);
                self.retranslate_button.set_sensitive(false);
                self.store.borrow_mut().clear();
                self.set_dot("ot-busy");
            }
            TranslationState::Done {
                text,
                translation,
                elapsed,
            } => {
                self.set_source(text);
                self.translation_label.remove_css_class("ot-dim");
                self.translation_label.set_text(translation);
                self.status_label.set_text(&format!(
                    "{} 字符 · {:.0} ms",
                    text.chars().count(),
                    elapsed.as_secs_f64() * 1000.0
                ));
                self.set_busy(false);
                self.progress_bar.set_visible(false);
                self.error_card.set_visible(false);
                self.copy_button.set_sensitive(true);
                self.copy_button.set_label("复制译文");
                self.retranslate_button.set_sensitive(true);
                *self.store.borrow_mut() = translation.clone();
                self.set_dot("ot-ready");
            }
            TranslationState::Failed { text, message } => {
                self.set_source(text);
                self.translation_label.remove_css_class("ot-dim");
                self.translation_label.set_text("");
                self.status_label.set_text("");
                self.set_busy(false);
                self.progress_bar.set_visible(false);
                self.error_label.set_text(message);
                self.error_card.set_visible(true);
                self.copy_button.set_sensitive(false);
                self.retranslate_button.set_sensitive(!text.is_empty());
                self.store.borrow_mut().clear();
                self.set_dot("ot-error");
            }
        }

        self.fit_height();
    }

    /// Height-follows-content: the window grows with the translation up to a
    /// monitor-relative cap, after which the scroller takes over.
    fn fit_height(&self) {
        if !self.window.is_visible() {
            return;
        }

        let max_height = self.max_height();
        let width = self.window.width().max(WINDOW_WIDTH);
        let overhead = (self.window.height() - self.root.height()).max(0);
        let max_content_height = (max_height - overhead).max(MIN_WINDOW_HEIGHT);

        self.scroller
            .set_max_content_height((max_content_height - CHROME_HEIGHT).max(120));

        let (_, natural_height, _, _) =
            self.root.measure(gtk::Orientation::Vertical, width);
        let (_, scroller_natural, _, _) = self.scroller.measure(gtk::Orientation::Vertical, width);
        let label_width = if self.scroller.width() > 50 {
            self.scroller.width()
        } else {
            (width - 56).max(50)
        };
        let (_, label_natural, _, _) = self
            .translation_label
            .measure(gtk::Orientation::Vertical, label_width);
        let content_height = natural_height - scroller_natural.min(natural_height) + label_natural;
        let target_content = content_height.clamp(MIN_WINDOW_HEIGHT, max_content_height);
        let target = target_content + overhead;

        if (self.window.height() - target).abs() > 8 {
            self.window.set_default_size(width, target);
            self.window.queue_resize();
        }
    }

    fn max_height(&self) -> i32 {
        let monitor = gtk::prelude::WidgetExt::display(&self.window)
            .monitors()
            .item(0)
            .and_then(|monitor| monitor.downcast::<gtk::gdk::Monitor>().ok());

        match monitor {
            Some(monitor) => (monitor.geometry().height() as f64 * MAX_WINDOW_FRACTION) as i32,
            None => 900,
        }
    }

    fn set_source(&self, text: &str) {
        if text.is_empty() {
            self.source_card.set_visible(false);
            self.source_label.set_text("");
            self.source_hint.set_text("");
        } else {
            self.source_label.set_text(text);
            self.source_card.set_visible(true);
        }
    }

    fn set_source_hint(&self, detected: Option<&str>) {
        match detected {
            Some(tag) => self
                .source_hint
                .set_text(&format!("（{}）", languages::label(tag))),
            None => self.source_hint.set_text(""),
        }
    }

    fn set_busy(&self, busy: bool) {
        self.spinner.set_visible(busy);

        if busy {
            self.spinner.start();
        } else {
            self.spinner.stop();
        }
    }

    fn set_dot(&self, class: &str) {
        for class in ["ot-ready", "ot-busy", "ot-error"] {
            self.dot.remove_css_class(class);
        }

        self.dot.add_css_class(class);
    }
}

struct Ui {
    window: ApplicationWindow,
    widgets: Widgets,
    receiver: Rc<RefCell<Option<Receiver<Progress>>>>,
    state: Rc<RefCell<TranslationState>>,
    source: RefCell<String>,
    target: RefCell<String>,
    detected_source: RefCell<Option<&'static str>>,
    recent_targets: RefCell<Vec<String>>,
    source_combo: ComboBoxText,
    target_combo: ComboBoxText,
    swap_button: Button,
    args: Args,
    mode: Mode,
}

pub fn run(args: Args) -> i32 {
    register_shortcut_if_missing();

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

fn register_shortcut_if_missing() {
    // Installed layouts ship open-translator-setup; dev builds usually do not.
    // The helper exits silently when the shortcut already exists.
    let _ = std::process::Command::new("open-translator-setup")
        .arg("--if-missing")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

fn install_css() {
    let Some(display) = gtk::gdk::Display::default() else {
        return;
    };

    let provider = CssProvider::new();
    provider.load_from_data(CSS);

    gtk::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn build_ui(
    app: &Application,
    args: &Args,
    mode: &Mode,
    state: Rc<RefCell<Option<Ui>>>,
) -> Ui {
    install_css();

    let title = format!(
        "OpenTranslator ({} → {})",
        languages::source_label(&args.source),
        languages::label(&args.target)
    );

    let window = ApplicationWindow::builder()
        .application(app)
        .title(&title)
        .default_width(560)
        .default_height(380)
        .build();

    let container = GtkBox::new(Orientation::Vertical, 10);
    container.set_margin_top(14);
    container.set_margin_bottom(14);
    container.set_margin_start(14);
    container.set_margin_end(14);

    let update_box = GtkBox::new(Orientation::Horizontal, 8);
    update_box.add_css_class("ot-banner");
    update_box.set_visible(false);

    let header = GtkBox::new(Orientation::Horizontal, 8);
    let dot = Label::new(Some("●"));
    dot.add_css_class("ot-ready");
    let title_label = Label::new(Some("OpenTranslator"));
    title_label.add_css_class("ot-title");
    title_label.set_xalign(0.0);
    header.append(&dot);
    header.append(&title_label);

    let source_caption = Label::new(Some("源语言"));
    source_caption.add_css_class("ot-dim");
    let source_combo = ComboBoxText::new();

    for (code, name) in languages::source_options() {
        source_combo.append(Some(code), name);
    }

    if args.source != languages::AUTO_CODE && !languages::is_supported(&args.source) {
        source_combo.append(Some(args.source.as_str()), args.source.as_str());
    }

    source_combo.set_active_id(Some(args.source.as_str()));

    let source_scroll = EventControllerScroll::new(EventControllerScrollFlags::VERTICAL);
    source_scroll.connect_scroll(|_, _, _| glib::Propagation::Stop);
    source_combo.add_controller(source_scroll);

    let source_hint = Label::new(None);
    source_hint.add_css_class("ot-dim");
    source_hint.add_css_class("ot-status");

    let target_caption = Label::new(Some("目标语言"));
    target_caption.add_css_class("ot-dim");
    let language_combo = ComboBoxText::new();

    for (code, name) in languages::LANGUAGES {
        language_combo.append(Some(code), name);
    }

    if !languages::is_supported(&args.target) {
        language_combo.append(Some(args.target.as_str()), args.target.as_str());
    }

    language_combo.set_active_id(Some(args.target.as_str()));

    let scroll_controller = EventControllerScroll::new(EventControllerScrollFlags::VERTICAL);
    scroll_controller.connect_scroll(|_, _, _| glib::Propagation::Stop);
    language_combo.add_controller(scroll_controller);

    let swap_button = Button::from_icon_name("object-flip-horizontal-symbolic");
    swap_button.set_tooltip_text(Some("互换源语言与目标语言"));
    swap_button.add_css_class("flat");
    swap_button.set_sensitive(
        args.source != languages::AUTO_CODE && args.source != args.target,
    );

    let language_box = GtkBox::new(Orientation::Horizontal, 6);
    let spacer = GtkBox::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    language_box.append(&spacer);
    language_box.append(&source_caption);
    language_box.append(&source_combo);
    language_box.append(&source_hint);
    language_box.append(&swap_button);
    language_box.append(&target_caption);
    language_box.append(&language_combo);

    let source_label = Label::new(None);
    source_label.set_xalign(0.0);
    source_label.set_wrap(true);
    source_label.set_lines(3);
    source_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    source_label.set_selectable(true);
    source_label.add_css_class("ot-dim");

    let source_card = GtkBox::new(Orientation::Vertical, 0);
    source_card.add_css_class("ot-source-card");
    source_card.append(&source_label);
    source_card.set_visible(false);

    let spinner = Spinner::new();
    spinner.set_visible(false);

    let translation_title = Label::new(Some("译文"));
    translation_title.add_css_class("ot-dim");
    translation_title.add_css_class("ot-status");

    let translation_header = GtkBox::new(Orientation::Horizontal, 8);
    translation_header.append(&spinner);
    translation_header.append(&translation_title);

    let translation_label = Label::new(None);
    translation_label.set_xalign(0.0);
    translation_label.set_wrap(true);
    translation_label.set_selectable(true);
    translation_label.add_css_class("ot-translation");
    translation_label.add_css_class("ot-dim");

    let scroller = ScrolledWindow::builder()
        .vexpand(true)
        .propagate_natural_height(true)
        .child(&translation_label)
        .build();

    let progress_bar = ProgressBar::new();
    progress_bar.set_show_text(true);
    progress_bar.set_visible(false);

    let error_label = Label::new(None);
    error_label.set_xalign(0.0);
    error_label.set_wrap(true);

    let error_card = GtkBox::new(Orientation::Vertical, 4);
    error_card.add_css_class("ot-error-card");
    error_card.append(&error_label);
    error_card.set_visible(false);

    let translation_card = GtkBox::new(Orientation::Vertical, 8);
    translation_card.add_css_class("ot-card");
    translation_card.set_vexpand(true);
    translation_card.append(&translation_header);
    translation_card.append(&progress_bar);
    translation_card.append(&error_card);
    translation_card.append(&scroller);

    let status_label = Label::new(None);
    status_label.add_css_class("ot-status");
    status_label.add_css_class("ot-status-dim");
    status_label.set_xalign(0.0);
    status_label.set_hexpand(true);
    status_label.set_ellipsize(gtk::pango::EllipsizeMode::End);

    let copy_button = Button::with_label("复制译文");
    copy_button.add_css_class("ot-primary");
    copy_button.set_sensitive(false);

    let retranslate_button = Button::with_label("重新翻译");
    retranslate_button.set_sensitive(false);

    let footer = GtkBox::new(Orientation::Horizontal, 8);
    footer.append(&status_label);
    footer.append(&retranslate_button);
    footer.append(&copy_button);

    container.append(&update_box);
    container.append(&header);
    container.append(&language_box);
    container.append(&source_card);
    container.append(&translation_card);
    container.append(&footer);

    window.set_child(Some(&container));

    let receiver: Rc<RefCell<Option<Receiver<Progress>>>> = Rc::new(RefCell::new(None));
    let translated: Rc<RefCell<TranslationState>> = Rc::new(RefCell::new(TranslationState::Idle));

    let widgets = Widgets {
        window: window.clone(),
        root: container.clone(),
        scroller: scroller.clone(),
        source_card,
        source_label,
        source_hint,
        translation_label,
        status_label,
        spinner,
        progress_bar,
        error_card,
        error_label,
        copy_button: copy_button.clone(),
        retranslate_button: retranslate_button.clone(),
        dot,
        store: Rc::new(RefCell::new(String::new())),
        last: Rc::new(RefCell::new(None)),
    };

    glib::timeout_add_local(std::time::Duration::from_millis(100), {
        let widgets = widgets.clone();
        let receiver = receiver.clone();
        let translated = translated.clone();

        move || {
            let messages: Vec<Progress> = receiver
                .borrow()
                .as_ref()
                .map(|receiver| receiver.try_iter().collect())
                .unwrap_or_default();

            for message in messages {
                let next = {
                    let current = translated.borrow();

                    match message {
                        Progress::Downloading { downloaded, total } => {
                            let text = state_text(&current);
                            TranslationState::Downloading {
                                text,
                                downloaded,
                                total,
                            }
                        }
                        Progress::Translating => TranslationState::Translating {
                            text: state_text(&current),
                            translation: String::new(),
                        },
                        Progress::Delta(delta) => {
                            let translation = match &*current {
                                TranslationState::Translating { translation, .. } => {
                                    format!("{translation}{delta}")
                                }
                                _ => delta,
                            };

                            TranslationState::Translating {
                                text: state_text(&current),
                                translation,
                            }
                        }
                        Progress::Done {
                            translation,
                            elapsed,
                        } => TranslationState::Done {
                            text: state_text(&current),
                            translation,
                            elapsed,
                        },
                        Progress::Error(message) => TranslationState::Failed {
                            text: if current_text_retryable(&current) {
                                state_text(&current)
                            } else {
                                String::new()
                            },
                            message,
                        },
                    }
                };

                *translated.borrow_mut() = next;
            }

            let snapshot = translated.borrow().clone();
            widgets.apply(&snapshot);

            if let TranslationState::Downloading { total: None, .. } = snapshot {
                widgets.progress_bar.pulse();
            }

            glib::ControlFlow::Continue
        }
    });

    copy_button.connect_clicked({
        let window = window.clone();
        let copy_button = copy_button.clone();
        let store = widgets.store.clone();

        move |_| {
            let text = store.borrow().clone();
            if !text.is_empty() {
                window.clipboard().set_text(&text);
                copy_button.set_label("已复制");
                copy_button.set_sensitive(false);

                let button = copy_button.clone();
                let store = store.clone();
                glib::timeout_add_seconds_local_once(2, move || {
                    button.set_label("复制译文");
                    if !store.borrow().is_empty() {
                        button.set_sensitive(true);
                    }
                });
            }
        }
    });

    retranslate_button.connect_clicked({
        let state = state.clone();
        move |_| {
            let text = {
                let slot = state.borrow();
                let Some(ui) = slot.as_ref() else {
                    return;
                };

                match &*ui.state.borrow() {
                    TranslationState::Done { text, .. }
                    | TranslationState::Failed { text, .. }
                    | TranslationState::Preparing { text }
                    | TranslationState::Downloading { text, .. }
                    | TranslationState::Translating { text, .. } => Some(text.clone()),
                    TranslationState::Idle | TranslationState::Empty => None,
                }
            };

            if let Some(text) = text.filter(|text| !text.is_empty()) {
                begin_translation(&state, text);
            }
        }
    });

    source_combo.connect_changed({
        let state = state.clone();

        move |combo| {
            let Some(source) = combo.active_id() else {
                return;
            };

            change_source(&state, source.to_string());
        }
    });

    language_combo.connect_changed({
        let state = state.clone();

        move |combo| {
            let Some(target) = combo.active_id() else {
                return;
            };

            change_target(&state, target.to_string());
        }
    });

    swap_button.connect_clicked({
        let state = state.clone();

        move |_| swap_languages(&state)
    });

    let key_controller = EventControllerKey::new();
    key_controller.connect_key_pressed({
        let window = window.clone();
        let state = state.clone();
        let store = widgets.store.clone();

        move |_, key, _, modifiers| {
            let control = gtk::gdk::ModifierType::CONTROL_MASK;

            if key == gtk::gdk::Key::Escape {
                window.close();
                glib::Propagation::Stop
            } else if key == gtk::gdk::Key::Return && modifiers.contains(control) {
                let text = state_text_opt(&state);
                if let Some(text) = text {
                    begin_translation(&state, text);
                }
                glib::Propagation::Stop
            } else if key == gtk::gdk::Key::C
                && modifiers.contains(control | gtk::gdk::ModifierType::SHIFT_MASK)
            {
                let text = store.borrow().clone();
                if !text.is_empty() {
                    window.clipboard().set_text(&text);
                }
                glib::Propagation::Stop
            } else if modifiers.contains(control)
                && !modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK)
                && !modifiers.contains(gtk::gdk::ModifierType::ALT_MASK)
            {
                let index = if key == gtk::gdk::Key::_1 {
                    Some(0)
                } else if key == gtk::gdk::Key::_2 {
                    Some(1)
                } else if key == gtk::gdk::Key::_3 {
                    Some(2)
                } else {
                    None
                };

                match index {
                    Some(index) => {
                        select_recent_target(&state, index);
                        glib::Propagation::Stop
                    }
                    None => glib::Propagation::Proceed,
                }
            } else {
                glib::Propagation::Proceed
            }
        }
    });
    window.add_controller(key_controller);

    if update::enabled(&load_config()).unwrap_or(false) {
        let (sender, receiver) = channel::<ReleaseInfo>();

        std::thread::spawn(move || {
            let Ok(client) = translate::build_client() else {
                return;
            };

            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(_) => return,
            };

            if let Ok(Some(info)) = runtime.block_on(update::check(
                &client,
                update::current_version(),
                &update::api_url(),
            )) {
                let _ = sender.send(info);
            }
        });

        glib::timeout_add_local(std::time::Duration::from_millis(200), move || {
            if let Ok(info) = receiver.try_recv() {
                let link = gtk::LinkButton::with_label(
                    &info.url,
                    &format!("有新版本 v{}，点击查看", info.version),
                );
                link.set_hexpand(true);
                link.set_halign(Align::Start);
                update_box.append(&link);

                let dismiss = Button::with_label("忽略");
                dismiss.connect_clicked({
                    let update_box = update_box.clone();
                    move |_| update_box.set_visible(false)
                });
                update_box.append(&dismiss);

                update_box.set_visible(true);
                return glib::ControlFlow::Break;
            }

            glib::ControlFlow::Continue
        });
    }

    window.present();

    Ui {
        window,
        widgets,
        receiver,
        state: translated,
        source: RefCell::new(args.source.clone()),
        target: RefCell::new(args.target.clone()),
        detected_source: RefCell::new(None),
        recent_targets: RefCell::new(languages::recent_target_list(
            &args.recent_targets,
            &args.target,
        )),
        source_combo: source_combo.clone(),
        target_combo: language_combo.clone(),
        swap_button,
        args: args.clone(),
        mode: mode.clone(),
    }
}

fn state_text(state: &TranslationState) -> String {
    match state {
        TranslationState::Preparing { text }
        | TranslationState::Downloading { text, .. }
        | TranslationState::Translating { text, .. }
        | TranslationState::Done { text, .. }
        | TranslationState::Failed { text, .. } => text.clone(),
        TranslationState::Idle | TranslationState::Empty => String::new(),
    }
}

fn current_text_retryable(state: &TranslationState) -> bool {
    !state_text(state).is_empty()
}

fn state_text_opt(state: &Rc<RefCell<Option<Ui>>>) -> Option<String> {
    let slot = state.borrow();
    let ui = slot.as_ref()?;
    let current = ui.state.borrow();

    match &*current {
        TranslationState::Done { text, .. } | TranslationState::Failed { text, .. }
            if !text.is_empty() =>
        {
            Some(text.clone())
        }
        _ => None,
    }
}

impl Ui {
    fn source_text(&self) -> String {
        state_text(&self.state.borrow())
    }

    fn remember_target(&self, previous: &str) {
        let target = self.target.borrow().clone();

        if previous == target {
            return;
        }

        let mut recents = self.recent_targets.borrow_mut();
        recents.retain(|entry| entry != &target && entry != previous);
        recents.insert(0, previous.to_string());
        recents.truncate(3);
        persist_recent_targets(&recents);
    }
}

fn update_title(state: &Rc<RefCell<Option<Ui>>>) {
    let slot = state.borrow();
    let Some(ui) = slot.as_ref() else {
        return;
    };

    let title = format!(
        "OpenTranslator ({} → {})",
        languages::source_label(&ui.source.borrow()),
        languages::label(&ui.target.borrow())
    );

    ui.window.set_title(Some(&title));
}

fn update_swap(state: &Rc<RefCell<Option<Ui>>>) {
    let slot = state.borrow();
    let Some(ui) = slot.as_ref() else {
        return;
    };

    let source = ui.source.borrow();
    let target = ui.target.borrow();

    let enabled = if *source == languages::AUTO_CODE {
        ui.detected_source
            .borrow()
            .map(|tag| tag != target.as_str())
            .unwrap_or(false)
    } else {
        source.as_str() != target.as_str()
    };

    ui.swap_button.set_sensitive(enabled);
}

fn change_source(state: &Rc<RefCell<Option<Ui>>>, source: String) {
    let source_text = {
        let slot = state.borrow();
        let Some(ui) = slot.as_ref() else {
            return;
        };

        if *ui.source.borrow() == source {
            return;
        }

        ui.source.replace(source.clone());
        ui.source_text()
    };

    {
        let slot = state.borrow();
        if let Some(ui) = slot.as_ref() {
            ui.source_combo.set_active_id(Some(&source));
        }
    }

    persist_source(&source);
    update_title(state);
    update_swap(state);

    if source_text.is_empty() {
        refresh(state);
    } else {
        begin_translation(state, source_text);
    }
}

fn change_target(state: &Rc<RefCell<Option<Ui>>>, target: String) {
    let source_text = {
        let slot = state.borrow();
        let Some(ui) = slot.as_ref() else {
            return;
        };

        if *ui.target.borrow() == target {
            return;
        }

        let previous = ui.target.replace(target.clone());
        ui.remember_target(&previous);
        ui.source_text()
    };

    {
        let slot = state.borrow();
        if let Some(ui) = slot.as_ref() {
            ui.target_combo.set_active_id(Some(&target));
        }
    }

    persist_target(&target);
    update_title(state);
    update_swap(state);

    if source_text.is_empty() {
        refresh(state);
    } else {
        begin_translation(state, source_text);
    }
}

fn select_recent_target(state: &Rc<RefCell<Option<Ui>>>, index: usize) {
    let target = {
        let slot = state.borrow();
        let Some(ui) = slot.as_ref() else {
            return;
        };

        ui.recent_targets.borrow().get(index).cloned()
    };

    if let Some(target) = target {
        change_target(state, target);
    }
}

fn swap_languages(state: &Rc<RefCell<Option<Ui>>>) {
    // Swapping exchanges the languages *and* feeds the translation back as the
    // new source text, so the result is translated into the original language.
    let outcome = {
        let slot = state.borrow();
        let Some(ui) = slot.as_ref() else {
            return;
        };

        let source = ui.source.borrow().clone();
        let target = ui.target.borrow().clone();
        let detected = *ui.detected_source.borrow();

        let Some((next_source, next_target)) = languages::swapped_pair(&source, &target, detected)
        else {
            return;
        };

        let translated = match &*ui.state.borrow() {
            TranslationState::Done { translation, .. } if !translation.trim().is_empty() => {
                Some(translation.clone())
            }
            _ => None,
        };

        (next_source, next_target, translated, ui.source_text())
    };

    let (next_source, next_target, translated, source_text) = outcome;

    {
        let slot = state.borrow();
        let Some(ui) = slot.as_ref() else {
            return;
        };

        let old_target = ui.target.replace(next_target.clone());
        ui.source.replace(next_source.clone());
        ui.detected_source.replace(None);
        ui.remember_target(&old_target);
    }

    {
        let slot = state.borrow();
        if let Some(ui) = slot.as_ref() {
            ui.source_combo.set_active_id(Some(&next_source));
            ui.target_combo.set_active_id(Some(&next_target));
        }
    }

    persist_source(&next_source);
    persist_target(&next_target);
    update_title(state);

    if let Some(translation) = translated {
        begin_translation(state, translation);
    } else if !source_text.is_empty() {
        begin_translation(state, source_text);
    }
}

fn refresh(state: &Rc<RefCell<Option<Ui>>>) {
    let outcome = {
        let slot = state.borrow();
        let Some(ui) = slot.as_ref() else {
            return;
        };

        match &ui.mode {
            Mode::Selection { clipboard } => read_selection(*clipboard),
            Mode::Stdin(Ok(text)) => Ok(text.clone()),
            Mode::Stdin(Err(error)) => Err(error.clone()),
        }
    };

    match outcome {
        Ok(text) if !text.trim().is_empty() => begin_translation(state, text),
        Ok(_) => set_state(state, TranslationState::Empty),
        Err(error) => set_state(
            state,
            TranslationState::Failed {
                text: String::new(),
                message: error,
            },
        ),
    }
}

fn set_state(state: &Rc<RefCell<Option<Ui>>>, next: TranslationState) {
    let (widgets, snapshot) = {
        let slot = state.borrow();
        let Some(ui) = slot.as_ref() else {
            return;
        };

        *ui.state.borrow_mut() = next;
        *ui.receiver.borrow_mut() = None;
        (ui.widgets.clone(), ui.state.borrow().clone())
    };

    widgets.apply(&snapshot);
}

fn begin_translation(state: &Rc<RefCell<Option<Ui>>>, text: String) {
    let (widgets, snapshot, sender, source, target, args) = {
        let slot = state.borrow();
        let Some(ui) = slot.as_ref() else {
            return;
        };

        *ui.state.borrow_mut() = TranslationState::Preparing { text: text.clone() };

        let snapshot = ui.state.borrow().clone();
        let (sender, receiver) = channel();
        *ui.receiver.borrow_mut() = Some(receiver);

        (
            ui.widgets.clone(),
            snapshot,
            sender,
            ui.source.borrow().clone(),
            ui.target.borrow().clone(),
            ui.args.clone(),
        )
    };

    let (source, detected) = detect::resolve_source(&source, &text);
    widgets.set_source_hint(detected);

    {
        let slot = state.borrow();
        if let Some(ui) = slot.as_ref() {
            ui.detected_source.replace(detected);
        }
    }

    update_swap(state);

    widgets.apply(&snapshot);
    spawn_worker(&args, &source, &target, text, sender);
}

fn spawn_worker(args: &Args, source: &str, target: &str, text: String, sender: Sender<Progress>) {
    let args = args.clone();
    let source = source.to_string();
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

            let config = match ServiceConfig::from_env(&args.service_url, !args.no_start) {
                Ok(config) => config,
                Err(error) => {
                    let _ = sender.send(Progress::Error(error));
                    return;
                }
            };

            let mut last_megabyte = 0u64;
            let ensured = services::ensure_with_download(&client, &config, |downloaded, total| {
                if downloaded / 1_000_000 != last_megabyte / 1_000_000 {
                    last_megabyte = downloaded;
                    let _ = sender.send(Progress::Downloading { downloaded, total });
                }
            })
            .await;

            if let Err(error) = ensured {
                let _ = sender.send(Progress::Error(error));
                return;
            }

            let _ = sender.send(Progress::Translating);

            let started = Instant::now();

            let delta_sender = sender.clone();
            let outcome = translate::translate_stream(
                &client,
                &args.service_url,
                &source,
                &target,
                &text,
                move |delta| {
                    let _ = delta_sender.send(Progress::Delta(delta.to_string()));
                },
            )
            .await;

            let elapsed = started.elapsed();

            let _ = match outcome {
                Ok(translation) => sender.send(Progress::Done {
                    translation,
                    elapsed,
                }),
                Err(error) => sender.send(Progress::Error(error)),
            };
        });
    });
}
