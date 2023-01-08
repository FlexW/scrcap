use iced;
use iced::theme;
use iced::widget::button;
use iced::widget::column;
use iced::widget::container;
use iced::widget::row;
use iced::widget::text;
use iced::window::Position;
use iced::Alignment;
use iced::Application;
use iced::Element;
use iced::Length;

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

    fn view(&self) -> Element<'_, Self::Message, iced::Renderer<Self::Theme>> {
        let mode_controls = screenshot_mode_controls(ScreenshotMode::Screen);

        let content = column![text("Take screenshot"), mode_controls]
            .spacing(20)
            .max_width(800);

        container(content)
            .width(Length::Fill)
            .padding(46)
            .center_x()
            .into()
    }
}

pub fn run() -> iced::Result {
    let win_size = (400, 300);
    Scrcap::run(iced::Settings {
        window: iced::window::Settings {
            min_size: Some(win_size),
            size: win_size,
            max_size: Some(win_size),
            resizable: false,
            position: Position::Centered,
            ..iced::window::Settings::default()
        },
        ..iced::Settings::default()
    })
}

#[derive(Debug, Clone)]
enum Message {
    Loaded(Result<State, LoadError>),
    TakeScreenshot,
    ScreenshotModeChanged(ScreenshotMode),
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

/// Create the button row for choosing the screenshot mode
fn screenshot_mode_controls(current_mode: ScreenshotMode) -> Element<'static, Message> {
    // Create a button with primary style if the current screenshot mode matches
    // the buttons mode or create a button with text style if the buttons mode
    // does not match.
    let mode_button = |label, mode, current_mode| {
        let label = text(label).size(16);

        let button = button(label).style(if mode == current_mode {
            theme::Button::Primary
        } else {
            theme::Button::Text
        });

        button
            .on_press(Message::ScreenshotModeChanged(mode))
            .padding(8)
    };

    row![
        mode_button("Screen", ScreenshotMode::Screen, current_mode),
        mode_button("Window", ScreenshotMode::Window, current_mode)
    ]
    .spacing(20)
    .align_items(Alignment::Center)
    .into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScreenshotMode {
    Screen,
    Window,
}
