use iced;
use iced::widget::button;
use iced::widget::column;
use iced::widget::text;
use iced::Application;

#[derive(Debug)]
enum Scrcap {
    Loading,
    Loaded(State),
}

impl iced::Application for Scrcap {
    type Executor = iced::executor::Default;
    type Message = Message;
    type Theme = iced::Theme;
    type Flags = ();

    fn new(_flags: Self::Flags) -> (Self, iced::Command<Self::Message>) {
        (
            Scrcap::Loading,
            iced::Command::perform(State::new(), Message::Loaded),
        )
    }

    fn title(&self) -> String {
        "scrcap".into()
    }

    fn update(&mut self, message: Self::Message) -> iced::Command<Self::Message> {
        match self {
            Scrcap::Loading => match message {
                Message::Loaded(Ok(state)) => {
                    *self = Self::Loaded(state);
                }
                Message::Loaded(Err(_)) => {
                    panic!("Could not init capture backend!");
                }
                _ => {}
            },
            Scrcap::Loaded(_state) => {}
        }

        iced::Command::none()
    }

    fn view(&self) -> iced::Element<'_, Self::Message, iced::Renderer<Self::Theme>> {
        column![
            text("Take screenshot"),
            button("Capture").on_press(Message::TakeScreenshot)
        ]
        .into()
    }
}

pub fn run() -> iced::Result {
    Scrcap::run(iced::Settings {
        window: iced::window::Settings {
            size: (200, 200),
            ..iced::window::Settings::default()
        },
        ..iced::Settings::default()
    })
}

#[derive(Debug, Clone)]
enum Message {
    Loaded(Result<State, LoadError>),
    TakeScreenshot,
}

#[derive(Debug, Clone)]
struct State {}

impl State {
    async fn new() -> Result<Self, LoadError> {
        Ok(Self {})
    }
}

#[derive(Debug, Clone)]
struct LoadError;
