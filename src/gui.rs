use std::fs::File;
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use anyhow::{bail, Result};
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
use iced::Element;
use iced::Length;
use log::debug;
use log::info;
use log::warn;

use crate::gui_backend;
use crate::output::get_screenshot_directory;
use crate::output::write_to_file;
use crate::output::EncodingFormat;
use crate::platform::create_platform;
use crate::platform::Output;
use crate::platform::Region;

#[derive(Debug)]
enum Scrcap {
    Loading,
    Ready(State),
    TakingScreenshot,
    ScreenshotTaken,
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
                    state
                        .cmd_tx
                        .send(gui_backend::Command::ListOutputs)
                        .unwrap();
                    let outputs = state.cmd_res_rx.lock().unwrap().recv().unwrap();
                    if let gui_backend::CommandResult::Outputs(outputs) = outputs {
                        info!("Received outputs: {:?}", outputs);
                    } else {
                        warn!("Received unexpected result");
                    }

                    *self = Self::Ready(state);
                }
                Message::Loaded(Err(_)) => {
                    panic!("Could not init capture backend!");
                }
                _ => (),
            },
            Scrcap::Ready(state) => match message {
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
                Message::TakeScreenshot => {
                    // TODO: Set window invisible
                    // TODO: Capturing ...

                    // TODO: Handle errors
                    let mut platform = create_platform().expect("Could not create capture backend");
                    let outputs = platform.outputs();
                    // Find output by name if needed
                    let output = get_output(None, &outputs).expect("Could not find an output");

                    // Get region on which screenshot should be captured
                    let region = if state.current_screenshot_mode == ScreenshotMode::Window {
                        Some(
                            platform
                                .focused_window_area()
                                .expect("Can not get window area"),
                        )
                    } else {
                        None
                    };

                    // Get matching output for region if needed
                    let output = if let Some(region) = region {
                        find_output_from_region(region, &outputs)
                            .expect("Can not find a matching output for region")
                    } else {
                        output
                    };
                    debug!("Take screenshot on output {:?}", output);

                    let frame = platform
                        .capture_frame(output, false, region)
                        .expect("Could not capture");

                    // Write screenshot to disk
                    let directory =
                        get_screenshot_directory().expect("Could not get screenshot directory");
                    // Generate a name
                    let time = match SystemTime::now().duration_since(UNIX_EPOCH) {
                        Ok(n) => n.as_secs().to_string(),
                        Err(_) => {
                            warn!("SystemTime before UNIX EPOCH!");
                            "TIME-BEFORE-UNIX-EPOCH".into()
                        }
                    };
                    let filename = format!("screenshot-{}", time);
                    let image_encoding = EncodingFormat::Png;

                    let path = format!(
                        "{}/{}.{}",
                        directory,
                        filename,
                        Into::<String>::into(image_encoding)
                    );

                    debug!("Write screenshot to {}", path);
                    write_to_file(
                        File::create(path).expect("Could not create file"),
                        image_encoding,
                        frame,
                    )
                    .expect("Could not write screenshot");

                    // TODO: Should be taking screenshot and screenshot operation should be done in background
                    // *self = Self::TakingScreenshot;
                    *self = Self::ScreenshotTaken;
                }
                _ => (),
            },
            Scrcap::TakingScreenshot => {
                *self = Self::ScreenshotTaken;
            }
            Scrcap::ScreenshotTaken => {}
        }

        iced::Command::none()
    }

    fn view(&self) -> Element<'_, Self::Message, iced::Renderer<Self::Theme>> {
        match self {
            Scrcap::Loading => loading_message_view(),
            Scrcap::Ready(state) => {
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
            Scrcap::TakingScreenshot => take_screenshot_message_view(),
            Scrcap::ScreenshotTaken => message_view("Screenshot ready"),
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

#[derive(Clone, Debug)]
struct State {
    current_screenshot_mode: ScreenshotMode,
    is_show_pointer: bool,
    delay_in_seconds: u32,
    cmd_tx: mpsc::Sender<gui_backend::Command>,
    cmd_res_rx: Arc<Mutex<mpsc::Receiver<gui_backend::CommandResult>>>,
}

impl State {
    async fn new() -> Result<Self, LoadError> {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (cmd_res_tx, cmd_res_rx) = mpsc::channel();

        gui_backend::run_backend(cmd_rx, cmd_res_tx);

        Ok(Self {
            current_screenshot_mode: ScreenshotMode::Screen,
            is_show_pointer: false,
            delay_in_seconds: 0,
            cmd_tx,
            cmd_res_rx: Arc::new(Mutex::new(cmd_res_rx)),
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
    message_view("Loading ...")
}

/// Create a simple taking screenshot message
fn take_screenshot_message_view() -> Element<'static, Message> {
    message_view("Taking screenshot...")
}

/// Create a simple centered message
fn message_view(message: &str) -> Element<'static, Message> {
    container(
        text(message)
            .horizontal_alignment(alignment::Horizontal::Center)
            .size(20),
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

/// Find the matching output to output_name or return the first output
fn get_output(output_name: Option<String>, outputs: &[Output]) -> Result<&Output> {
    if let Some(output_name) = output_name {
        // Find output with matching name
        for output in outputs {
            if output.name == output_name {
                return Ok(output);
            }
        }
        bail!("No output named {} found!", output_name);
    } else if !outputs.is_empty() {
        // Take the first one
        return Ok(&outputs[0]);
    } else {
        bail!("No output found!");
    };
}

fn find_output_from_region(region: Region, outputs: &[Output]) -> Result<&Output> {
    for output in outputs {
        let output_region = Region::new(output.x, output.y, output.width, output.height);
        if output_region.contains(region) {
            return Ok(output);
        }
    }
    bail!("Did not find Output for given Region")
}
