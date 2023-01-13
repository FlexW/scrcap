use std::fs::File;
use std::thread;
use std::time::UNIX_EPOCH;
use std::{sync::mpsc, time::SystemTime};

use anyhow::{bail, Result};
use log::{debug, error, info, warn};

use crate::output::{get_screenshot_directory, write_to_file, EncodingFormat};
use crate::platform::{create_platform, Frame, Output, Region};

pub enum Command {
    ListOutputs,
    CaptureScreen(String),
    CaptureWindow,
    SaveToDisk(Option<String>, Frame),
    Quit,
}

pub enum CommandResult {
    Outputs(Vec<String>),
    FrameCaptured(Frame),
    SaveToDiskSuccess,
}

pub fn run_backend(cmd_rx: mpsc::Receiver<Command>, res_tx: mpsc::Sender<CommandResult>) {
    thread::spawn(move || {
        info!("Start gui backend");
        let mut platform = create_platform().expect("Failed to create platform");

        loop {
            let cmd = cmd_rx.recv().unwrap();
            match cmd {
                Command::ListOutputs => {
                    debug!("Received list outputs cmd");
                    let outputs = platform
                        .outputs()
                        .iter()
                        .map(|output| output.name.clone())
                        .collect::<Vec<_>>();
                    res_tx.send(CommandResult::Outputs(outputs)).unwrap();
                }
                Command::CaptureScreen(output_name) => {
                    debug!("Received capture screen cmd for output {}", output_name);
                    let outputs = platform.outputs();
                    let output = get_output(None, &outputs).expect("Could not find an output");

                    let frame = platform
                        .capture_frame(output, false, None)
                        .expect("Could not capture");
                    res_tx.send(CommandResult::FrameCaptured(frame)).unwrap();
                }
                Command::CaptureWindow => {
                    debug!("Received capture window cmd");
                    let capture_region = platform
                        .focused_window_area()
                        .expect("Can not get window area");
                    debug!("Capture region: {:?}", capture_region);

                    let outputs = platform.outputs();
                    let output = find_output_from_region(capture_region, &outputs)
                        .expect("Can not find a matching output for region");
                    debug!("Capture on output {:?}", output);

                    let frame = platform
                        .capture_frame(output, false, Some(capture_region))
                        .expect("Could not capture");
                    res_tx
                        .send(CommandResult::FrameCaptured(frame))
                        .expect("Could not send frame");
                }
                Command::SaveToDisk(filename, frame) => {
                    let filename = if let Some(filename) = filename {
                        filename
                    } else {
                        // Write screenshot to disk
                        // Generate a name
                        let time = match SystemTime::now().duration_since(UNIX_EPOCH) {
                            Ok(n) => n.as_secs().to_string(),
                            Err(_) => {
                                warn!("SystemTime before UNIX EPOCH!");
                                "TIME-BEFORE-UNIX-EPOCH".into()
                            }
                        };
                        format!("screenshot-{}", time)
                    };

                    let directory =
                        get_screenshot_directory().expect("Could not get screenshot directory");
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
                }
                Command::Quit => {
                    debug!("Received quit cmd");
                    break;
                }
            }
        }
        info!("Gui backend finished");
    });
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
