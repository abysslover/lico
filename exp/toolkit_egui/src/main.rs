//! M01-P07a candidate: egui on the CPU path. Menu bar (File, Session, Help), a 100-entry session list, one entry line
//! and a read-only output area for the synthetic host. Commands on stdin, events on stdout: see `lico-toolkit-common`.

use std::sync::mpsc::{channel, Receiver};

use egui::{Color32, Event, FontData, FontDefinitions, FontFamily, Id, LayerId, Order, Panel, Rect, RichText, ScrollArea, TextEdit, ViewportCommand};
use egui_software_backend::{App, SoftwareBackend, SoftwareBackendAppConfiguration};
use lico_toolkit_common as common;
use lico_toolkit_common::{Command, Core, Msg};

const FONT_PRIMARY: &str = "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf";
const FONT_FALLBACK: &str = "/usr/share/fonts/truetype/nanum/NanumSquare_acR.ttf";

struct Client {
    core: Core,
    rx: Receiver<Msg>,
    input: String,
    find: String,
    last_text: String,
    composing: bool,
    first_frame: bool,
    pending_pick: Option<String>,
}

fn install_fonts(ctx: &egui::Context) {
    // egui has no system-font discovery: the two fonts the reference client uses are read into memory.
    let mut fonts = FontDefinitions::empty();
    for (name, path) in [("primary", FONT_PRIMARY), ("fallback", FONT_FALLBACK)] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(name.to_string(), std::sync::Arc::new(FontData::from_owned(bytes)));
            for family in [FontFamily::Proportional, FontFamily::Monospace] {
                fonts.families.entry(family).or_default().push(name.to_string());
            }
        }
    }
    ctx.set_fonts(fonts);
}

impl Client {
    fn new(ctx: egui::Context) -> Self {
        install_fonts(&ctx);
        let (tx, rx) = channel::<Msg>();
        let wake = ctx.clone();
        let tx = std::sync::Mutex::new(tx);
        let socket = common::arg_value("--socket").expect("--socket PATH");
        let core = common::start(&socket, move |msg| {
            let _ = tx.lock().unwrap().send(msg);
            wake.request_repaint();
        });
        common::emit_ready();
        Client {
            core, rx, input: String::new(), find: String::new(), last_text: String::new(), composing: false,
            first_frame: true, pending_pick: None,
        }
    }

    fn drain(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Sock(line) => {
                    self.core.on_sock_line(&line);
                }
                Msg::Cmd(line) => match self.core.on_command(&line) {
                    Command::Quit => ctx.send_viewport_cmd(ViewportCommand::Close),
                    Command::Text => common::emit_ime("text", &self.input),
                    Command::None => {}
                },
            }
        }
    }

    fn watch_ime(&mut self, ui: &egui::Ui) {
        for event in ui.input(|i| i.events.clone()) {
            if let Event::Ime(ime) = event {
                match ime {
                    egui::ImeEvent::Preedit(text) => {
                        self.composing = !text.is_empty();
                        common::emit_ime("preedit", &text);
                    }
                    egui::ImeEvent::Commit(text) => {
                        self.composing = false;
                        common::emit_ime("commit", &text);
                    }
                    egui::ImeEvent::Enabled => common::emit_ime("enabled", ""),
                    egui::ImeEvent::Disabled => common::emit_ime("disabled", ""),
                }
            }
        }
    }
}

fn marker(ui: &egui::Ui, x: f32, rgb: u32) {
    let ppp = ui.ctx().pixels_per_point();
    let side = common::MARKER_PHYSICAL_PX / ppp;
    let rect = Rect::from_min_size(egui::pos2(x * side, 0.0), egui::vec2(side, side));
    let color = Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8);
    ui.ctx().layer_painter(LayerId::new(Order::Debug, Id::new("marker"))).rect_filled(rect, 0.0, color);
}

impl App for Client {
    fn ui(&mut self, ui: &mut egui::Ui, _backend: &mut SoftwareBackend) {
        let ctx = ui.ctx().clone();
        self.drain(&ctx);
        self.watch_ime(ui);
        let ppp = ctx.pixels_per_point();

        Panel::top("menu").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(2.0 * common::MARKER_PHYSICAL_PX / ppp);
                egui::MenuBar::new().ui(ui, |ui| {
                    ui.menu_button("File", |ui| {
                        if ui.button("Open session list").clicked() {
                            common::emit_note("menu", "File/Open");
                        }
                        if ui.button("Quit").clicked() {
                            common::emit_note("menu", "File/Quit");
                            ctx.send_viewport_cmd(ViewportCommand::Close);
                        }
                    });
                    // a text field inside a menu needs the menu not to close on a click inside it (egui's default closes)
                    let config = egui::containers::menu::MenuConfig::new()
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
                    egui::containers::menu::MenuButton::new("Session").config(config).ui(ui, |ui| {
                        if ui.button("New session").clicked() {
                            common::emit_note("menu", "Session/New");
                        }
                        ui.separator();
                        ui.label("Find session:");
                        let response = ui.add(TextEdit::singleline(&mut self.find).desired_width(160.0));
                        if response.changed() {
                            common::emit_note("find", &self.find);
                        }
                    });
                    ui.menu_button("Help", |ui| {
                        if ui.button("About").clicked() {
                            common::emit_note("menu", "Help/About");
                        }
                    });
                });
            });
        });

        Panel::left("sessions").exact_size(240.0).resizable(false).show_inside(ui, |ui| {
            let count = self.core.sessions.len();
            ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, ui.text_style_height(&egui::TextStyle::Body) + 4.0, count, |ui, range| {
                for index in range {
                    let selected = self.core.sessions[index] == self.core.session && self.core.view_resident;
                    if ui.selectable_label(selected, self.core.label(index)).clicked() {
                        self.pending_pick = Some(self.core.sessions[index].clone());
                    }
                }
            });
        });
        if let Some(id) = self.pending_pick.take() {
            self.core.show(&id, None);
        }

        Panel::bottom("entry").show_inside(ui, |ui| {
            let response = ui.add(TextEdit::singleline(&mut self.input).desired_width(f32::INFINITY).font(egui::TextStyle::Monospace));
            if self.first_frame {
                response.request_focus();
                self.first_frame = false;
            }
        });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                ui.label(RichText::new("\u{d55c}\u{ae00} Lico session view").monospace());
                for line in &self.core.lines {
                    ui.label(RichText::new(line.as_str()).monospace());
                }
            });
        });

        marker(ui, 0.0, self.core.session_rgb);
        marker(ui, 1.0, self.core.seq_rgb);

        if !self.composing && self.input != self.last_text {
            self.last_text = self.input.clone();
            common::emit_ime("text", &self.input);
        }
    }
}

fn main() {
    let settings = SoftwareBackendAppConfiguration::new()
        .inner_size(Some(egui::vec2(1200.0, 800.0)))
        .title(Some("Lico candidate egui".to_string()));
    egui_software_backend::run_app_with_software_backend(settings, Client::new).expect("run");
}
