use iced;
use iced::alignment;
use iced::theme;
use iced::widget::button;
use iced::widget::checkbox;
use iced::widget::column;
use iced::widget::container;
use iced::widget::row;
use iced::widget::text;
use iced::widget::Space;
use iced::window::Position;
use iced::Alignment;
use iced::Application;
use iced::Color;
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
                _ => (),
            },
            Scrcap::Loaded(state) => match message {
                Message::ScreenshotModeChanged(mode) => {
                    state.current_screenshot_mode = mode;
                }
                Message::ShowPointer(is_shown) => {
                    state.is_show_pointer = is_shown;
                }
                Message::IncrementDelay => {
                    state.delay_in_seconds += 1;
                }
                Message::DecrementDelay => {
                    if state.delay_in_seconds > 0 {
                        state.delay_in_seconds -= 1;
                    }
                }
                _ => (),
            },
        }

        iced::Command::none()
    }

    fn view(&self) -> Element<'_, Self::Message, iced::Renderer<Self::Theme>> {
        match self {
            Scrcap::Loading => loading_message_view(),
            Scrcap::Loaded(state) => {
                // let title =
                //     text("Take Screenshot").horizontal_alignment(alignment::Horizontal::Center);

                let mode_controls = screenshot_mode_view(state.current_screenshot_mode);

                let pointer_controls = include_pointer_view(state.is_show_pointer);

                let delay_controls = delay_view(state.delay_in_seconds);

                let screenshot_button = take_screenshot_button_view();

                let content = column![
                    // title,
                    mode_controls,
                    pointer_controls,
                    delay_controls,
                    screenshot_button,
                ]
                .spacing(20)
                .max_width(800);

                container(content)
                    .width(Length::Fill)
                    .padding(40)
                    .center_x()
                    .into()
            }
        }
    }
}

pub fn run() -> iced::Result {
    let win_size = (400, 250);
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
    ShowPointer(bool),
    IncrementDelay,
    DecrementDelay,
}

#[derive(Debug, Clone)]
struct State {
    current_screenshot_mode: ScreenshotMode,
    is_show_pointer: bool,
    delay_in_seconds: u32,
}

impl State {
    async fn new() -> Result<Self, LoadError> {
        Ok(Self {
            current_screenshot_mode: ScreenshotMode::Screen,
            is_show_pointer: false,
            delay_in_seconds: 0,
        })
    }
}

#[derive(Debug, Clone)]
struct LoadError;

/// Create the button row for choosing the screenshot mode
fn screenshot_mode_view(current_mode: ScreenshotMode) -> Element<'static, Message> {
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
            .width(Length::Shrink)
            .padding(8)
    };

    row![
        Space::with_width(Length::Fill),
        mode_button("Screen", ScreenshotMode::Screen, current_mode),
        mode_button("Window", ScreenshotMode::Window, current_mode),
        Space::with_width(Length::Fill),
    ]
    .spacing(10)
    .width(Length::Fill)
    .align_items(Alignment::Center)
    .into()
}

/// Create a simple loading message
fn loading_message_view() -> Element<'static, Message> {
    container(
        text("Loading...")
            .horizontal_alignment(alignment::Horizontal::Center)
            .size(50),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_y()
    .into()
}

/// Create a view that lets the user select if the pointer should be included
fn include_pointer_view(is_shown: bool) -> Element<'static, Message> {
    let text = text("Show Pointer").size(16).width(Length::Fill);
    // TODO: Investigate how to create a checkbox without a label
    let checkbox = checkbox("", is_shown, Message::ShowPointer)
        .width(Length::Shrink)
        .text_size(0);
    row![text, checkbox].width(Length::Fill).into()
}

/// Create a view that lets user choose a delay
fn delay_view(delay: u32) -> Element<'static, Message> {
    let inc_button = button(text("+").size(16)).on_press(Message::IncrementDelay);

    let dec_button = button(text("-").size(16));
    let dec_button = if delay > 0 {
        dec_button.on_press(Message::DecrementDelay)
    } else {
        dec_button
    };

    row![
        text("Delay in Seconds").width(Length::Fill).size(16),
        text(format!("{}", delay)).width(Length::Shrink),
        row![inc_button, dec_button]
            .width(Length::Shrink)
            .spacing(10)
    ]
    .spacing(20)
    .align_items(Alignment::Center)
    .into()
}

/// Create a view with a centered take screenshot button
fn take_screenshot_button_view() -> Element<'static, Message> {
    let screenshot_button = button(text("Take Screenshot").size(16))
        .on_press(Message::TakeScreenshot)
        .width(Length::Shrink)
        .padding(8);

    row![
        Space::with_width(Length::Fill),
        screenshot_button,
        Space::with_width(Length::Fill),
    ]
    .spacing(10)
    .width(Length::Fill)
    .align_items(Alignment::Center)
    .into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScreenshotMode {
    Screen,
    Window,
}
