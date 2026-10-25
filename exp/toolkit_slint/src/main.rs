//! M01-P07a candidate: Slint with the software renderer. Menu bar (File, Session, Help), a 100-entry session list,
//! one entry line and a read-only output area. Commands on stdin, events on stdout: see `lico-toolkit-common`.

use std::cell::RefCell;
use std::rc::Rc;

use lico_toolkit_common as common;
use lico_toolkit_common::{Command, Core, Msg};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

slint::include_modules!();

struct App {
    core: Core,
    window: MainWindow,
    model: Rc<VecModel<SharedString>>,
    last_text: String,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

fn rgb(value: u32) -> slint::Color {
    slint::Color::from_rgb_u8((value >> 16) as u8, (value >> 8) as u8, value as u8)
}

impl App {
    fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Sock(line) => {
                if self.core.on_sock_line(&line) {
                    let labels: Vec<SharedString> =
                        (0..self.core.sessions.len()).map(|i| SharedString::from(self.core.label(i))).collect();
                    self.model.set_vec(labels);
                }
            }
            Msg::Cmd(line) => match self.core.on_command(&line) {
                Command::Quit => {
                    let _ = slint::quit_event_loop();
                }
                Command::Text => common::emit_ime("text", self.window.get_entry().as_str()),
                Command::None => {}
            },
        }
        self.refresh();
    }

    fn refresh(&mut self) {
        let selected = if self.core.view_resident {
            self.core.sessions.iter().position(|s| *s == self.core.session).map(|i| i as i32).unwrap_or(-1)
        } else {
            -1
        };
        self.window.set_selected(selected);
        self.window.set_session_color(rgb(self.core.session_rgb));
        self.window.set_seq_color(rgb(self.core.seq_rgb));
        if self.core.dirty {
            self.core.dirty = false;
            let joined = self.core.lines.iter().map(String::as_str).collect::<Vec<_>>().join("\n");
            self.window.set_output(joined.into());
        }
        let text = self.window.get_entry().to_string();
        if text != self.last_text {
            self.last_text = text.clone();
            common::emit_ime("text", &text);
        }
    }
}

fn with_app(f: impl FnOnce(&mut App)) {
    APP.with(|cell| {
        if let Some(app) = cell.borrow_mut().as_mut() {
            f(app);
        }
    });
}

fn main() {
    let socket = common::arg_value("--socket").expect("--socket PATH");
    let window = MainWindow::new().expect("window");
    let model = Rc::new(VecModel::<SharedString>::default());
    window.set_sessions(ModelRc::from(model.clone()));
    let core = common::start(&socket, |msg| {
        let _ = slint::invoke_from_event_loop(move || with_app(|app| app.handle(msg)));
    });
    window.on_pick(|index| {
        with_app(|app| {
            if let Some(id) = app.core.sessions.get(index as usize).cloned() {
                app.core.show(&id, None);
            }
            app.refresh();
        });
    });
    window.on_menu(|item| {
        common::emit_note("menu", item.as_str());
        if item.as_str() == "File/Quit" {
            let _ = slint::quit_event_loop();
        }
    });
    window.on_entry_edited(|_| with_app(|app| app.refresh()));
    window.on_find_edited(|text| common::emit_note("find", text.as_str()));
    APP.with(|cell| *cell.borrow_mut() = Some(App { core, window: window.clone_strong(), model, last_text: String::new() }));
    window.show().expect("show");
    common::emit_ready();
    slint::run_event_loop().expect("event loop");
}
