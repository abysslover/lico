//! M01-P07a candidate: iced with the tiny-skia renderer. Menu bar (File, Session, Help) from iced_aw, a 100-entry
//! session list, one entry line and a read-only output area. Commands on stdin, events on stdout: see `lico-toolkit-common`.

use std::sync::Mutex;

use iced::futures::channel::mpsc::{unbounded, UnboundedReceiver};
use iced::futures::StreamExt;
use iced::widget::text_editor::{Action, Content, Motion};
use iced::widget::{button, column, container, row, scrollable, space, text, text_editor, text_input};
use iced::{event, window, Color, Element, Event, Font, Length, Size, Subscription, Task};
use iced_aw::menu::Menu;
use iced_aw::{menu_bar, menu_items};
use lico_toolkit_common as common;
use lico_toolkit_common::{Command, Core, Msg};

static RX: Mutex<Option<UnboundedReceiver<Msg>>> = Mutex::new(None);
static CORE: Mutex<Option<Core>> = Mutex::new(None);

#[derive(Debug, Clone)]
enum Message {
    Sock(String),
    Cmd(String),
    Input(String),
    Find(String),
    Pick(String),
    Menu(&'static str),
    Output(Action),
    Opened,
    Scale(f32),
    Ime(iced::advanced::input_method::Event),
    Noop,
}

struct App {
    core: Core,
    input: String,
    find: String,
    last_text: String,
    composing: bool,
    output: Content,
    scale: f32,
    announced: bool,
}

fn worker() -> impl iced::futures::Stream<Item = Message> {
    let rx = RX.lock().unwrap().take().expect("receiver");
    iced::futures::stream::unfold(rx, |mut rx| async move {
        let msg = rx.next().await?;
        Some((
            match msg {
                Msg::Sock(line) => Message::Sock(line),
                Msg::Cmd(line) => Message::Cmd(line),
            },
            rx,
        ))
    })
}

fn boot() -> (App, Task<Message>) {
    let core = CORE.lock().unwrap().take().expect("core");
    let app = App {
        core, input: String::new(), find: String::new(), last_text: String::new(), composing: false,
        output: Content::new(), scale: 1.0, announced: false,
    };
    (app, iced::widget::operation::focus("entry"))
}

fn marker<'a>(side: f32, rgb: u32) -> Element<'a, Message> {
    let color = Color::from_rgb8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8);
    container(space().width(Length::Fixed(side)).height(Length::Fixed(side)))
        .style(move |_| container::Style { background: Some(color.into()), ..Default::default() })
        .into()
}

impl App {
    fn update(&mut self, message: Message) -> Task<Message> {
        let mut task = Task::none();
        match message {
            Message::Sock(line) => {
                self.core.on_sock_line(&line);
            }
            Message::Cmd(line) => match self.core.on_command(&line) {
                Command::Quit => task = iced::exit(),
                Command::Text => common::emit_ime("text", &self.input),
                Command::None => {}
            },
            Message::Input(value) => self.input = value,
            Message::Find(value) => {
                self.find = value;
                common::emit_note("find", &self.find);
            }
            Message::Pick(id) => self.core.show(&id, None),
            Message::Menu(item) => {
                common::emit_note("menu", item);
                if item == "File/Quit" {
                    task = iced::exit();
                }
            }
            Message::Output(action) => {
                if !action.is_edit() {
                    self.output.perform(action); // selection, motion and scrolling work; edits are dropped (read-only)
                }
            }
            Message::Opened => {
                if !self.announced {
                    self.announced = true;
                    common::emit_ready();
                    task = window::latest().and_then(window::scale_factor).map(Message::Scale);
                }
            }
            Message::Scale(scale) => self.scale = scale.max(0.5),
            Message::Ime(event) => {
                use iced::advanced::input_method::Event as Ime;
                match event {
                    Ime::Opened => common::emit_ime("enabled", ""),
                    Ime::Preedit(text, _) => {
                        self.composing = !text.is_empty();
                        common::emit_ime("preedit", &text);
                    }
                    Ime::Commit(text) => {
                        self.composing = false;
                        common::emit_ime("commit", &text);
                    }
                    Ime::Closed => common::emit_ime("disabled", ""),
                }
            }
            Message::Noop => {}
        }
        if self.core.dirty {
            self.core.dirty = false;
            let joined = self.core.lines.iter().map(String::as_str).collect::<Vec<_>>().join("\n");
            self.output = Content::with_text(&joined);
            self.output.perform(Action::Move(Motion::DocumentEnd));
        }
        if !self.composing && self.input != self.last_text {
            self.last_text = self.input.clone();
            common::emit_ime("text", &self.input);
        }
        task
    }

    fn view(&self) -> Element<'_, Message> {
        let side = common::MARKER_PHYSICAL_PX / self.scale;
        let menu = |items| Menu::new(items).width(220.0).offset(0.0).spacing(2.0);
        let bar = menu_bar!(
            (
                button(text("File")).on_press(Message::Noop),
                menu(menu_items!((button(text("Open session list")).width(Length::Fill).on_press(Message::Menu("File/Open"))),
                                 (button(text("Quit")).width(Length::Fill).on_press(Message::Menu("File/Quit")))))
            ),
            (
                button(text("Session")).on_press(Message::Noop),
                menu(menu_items!((button(text("New session")).width(Length::Fill).on_press(Message::Menu("Session/New"))),
                                 (text("Find session:")),
                                 (text_input("find", &self.find).on_input(Message::Find))))
            ),
            (
                button(text("Help")).on_press(Message::Noop),
                menu(menu_items!((button(text("About")).width(Length::Fill).on_press(Message::Menu("Help/About")))))
            )
        );
        let top = row![marker(side, self.core.session_rgb), marker(side, self.core.seq_rgb), bar].spacing(0);

        let entries = column(self.core.sessions.iter().enumerate().map(|(i, id)| {
            let selected = *id == self.core.session && self.core.view_resident;
            button(text(self.core.label(i)).shaping(text::Shaping::Advanced))
                .width(Length::Fill)
                .style(if selected { button::primary } else { button::text })
                .on_press(Message::Pick(id.clone()))
                .into()
        }));
        let list = scrollable(entries).width(240.0).height(Length::Fill);

        let output = text_editor(&self.output)
            .on_action(Message::Output)
            .font(Font::MONOSPACE)
            .height(Length::Fill);
        let entry = text_input("", &self.input).id("entry").on_input(Message::Input).font(Font::MONOSPACE);
        let right = column![output, entry].spacing(2);
        column![top, row![list, right].height(Length::Fill)].into()
    }
}

fn subscription(_app: &App) -> Subscription<Message> {
    Subscription::batch([
        Subscription::run(worker),
        event::listen_with(|event, _status, _window| match event {
            Event::InputMethod(ime) => Some(Message::Ime(ime)),
            Event::Window(window::Event::Opened { .. }) => Some(Message::Opened),
            _ => None,
        }),
    ])
}

fn main() -> iced::Result {
    let socket = common::arg_value("--socket").expect("--socket PATH");
    let (tx, rx) = unbounded::<Msg>();
    *RX.lock().unwrap() = Some(rx);
    let core = common::start(&socket, move |msg| {
        let _ = tx.unbounded_send(msg);
    });
    *CORE.lock().unwrap() = Some(core);
    iced::application(boot, App::update, App::view)
        .title("Lico candidate iced")
        .subscription(subscription)
        .window_size(Size::new(1200.0, 800.0))
        .run()
}
