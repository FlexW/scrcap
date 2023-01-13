use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;

use anyhow::Result;
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

use crate::gui_backend;

#[derive(Debug)]
enum Scrcap {
    Loading,
    Ready(Arc<Mutex<State>>),
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
                        .lock()
                        .unwrap()
                        .cmd_tx
                        .send(gui_backend::Command::ListOutputs)
                        .unwrap();

                    *self = Self::Ready(state);
                }
                Message::Loaded(Err(_)) => {
                    panic!("Could not init capture backend!");
                }
                _ => (),
            },
            Scrcap::Ready(state) => {
                state.lock().unwrap().process_backend_cmd_results();

                // info!(
                //     "outputs: {:?}, mode: {:?}",
                //     state.outputs, state.current_screenshot_mode
                // );

                match message {
                    Message::ScreenshotModeChanged(mode) => {
                        state.lock().unwrap().current_screenshot_mode = mode;
                    }
                    Message::ShowPointer(is_shown) => {
                        state.lock().unwrap().is_show_pointer = is_shown;
                    }
                    Message::IncrementDelay => {
                        state.lock().unwrap().delay_in_seconds += 1;
                    }
                    Message::DecrementDelay => {
                        if state.lock().unwrap().delay_in_seconds > 0 {
                            state.lock().unwrap().delay_in_seconds -= 1;
                        }
                    }
                    Message::TakeScreenshot => {
                        // TODO: Set window invisible
                        let current_screenshot_mode = state.lock().unwrap().current_screenshot_mode;
                        match current_screenshot_mode {
                            ScreenshotMode::Screen => {
                                let output = state.lock().unwrap().choosen_output.clone();
                                let output = if let Some(output) = output {
                                    output.into()
                                } else if !state.lock().unwrap().outputs.is_empty() {
                                    state.lock().unwrap().outputs[0].clone()
                                } else {
                                    panic!("Could not find output for capturing");
                                };

                                state
                                    .lock()
                                    .unwrap()
                                    .cmd_tx
                                    .send(gui_backend::Command::CaptureScreen(output))
                                    .unwrap();
                            }
                            ScreenshotMode::Window => {
                                state
                                    .lock()
                                    .unwrap()
                                    .cmd_tx
                                    .send(gui_backend::Command::CaptureWindow)
                                    .unwrap();
                            }
                        }

                        // TODO: Handle errors
                        // *self = Self::ScreenshotTaken;
                    }
                    _ => (),
                }
            }
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
                let state = state.lock().unwrap();

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
    Loaded(Result<Arc<Mutex<State>>, LoadError>),
    TakeScreenshot,
    ScreenshotModeChanged(ScreenshotMode),
    ShowPointer(bool),
    IncrementDelay,
    DecrementDelay,
}

#[derive(Debug)]
struct State {
    current_screenshot_mode: ScreenshotMode,
    is_show_pointer: bool,
    delay_in_seconds: u32,

    outputs: Vec<String>,
    choosen_output: Option<String>,

    cmd_tx: mpsc::Sender<gui_backend::Command>,
    cmd_res_rx: mpsc::Receiver<gui_backend::CommandResult>,
}

impl State {
    async fn new() -> Result<Arc<Mutex<Self>>, LoadError> {
        // let (cmd_tx, cmd_rx) = mpsc::sync_channel(3);
        // let (cmd_res_tx, cmd_res_rx) = mpsc::sync_channel(3);
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (cmd_res_tx, cmd_res_rx) = mpsc::channel();

        gui_backend::run_backend(cmd_rx, cmd_res_tx);

        Ok(Arc::new(Mutex::new(Self {
            current_screenshot_mode: ScreenshotMode::Screen,
            is_show_pointer: false,
            delay_in_seconds: 0,
            outputs: Vec::new(),
            choosen_output: None,
            cmd_tx,
            cmd_res_rx,
        })))
    }

    fn process_backend_cmd_results(&mut self) {
        loop {
            match self.cmd_res_rx.try_recv() {
                Ok(res) => match res {
                    gui_backend::CommandResult::Outputs(outputs) => {
                        info!("Outpus received {:?}", outputs);
                        self.outputs = outputs;
                    }
                    gui_backend::CommandResult::FrameCaptured(frame) => {
                        info!("Frame captured");
                        self.cmd_tx
                            .send(gui_backend::Command::SaveToDisk(None, frame))
                            .unwrap();
                    }
                    gui_backend::CommandResult::SaveToDiskSuccess => {
                        info!("Frame saved succesfully to disk");
                    }
                },
                Err(mpsc::TryRecvError::Empty) => {
                    debug!("No command results");
                    break;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    debug!("Backend thread has disconnected");
                    break;
                }
            }
        }
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
