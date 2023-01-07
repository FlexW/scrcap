mod gui;
mod output;
mod platform;

use clap::Parser;
use output::EncodingFormat;
use platform::{create_platform, Output, Region};

use std::fs::File;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::output::{get_screenshot_directory, write_to_file};
use anyhow::{anyhow, bail, Context, Result};
use log::{debug, warn, LevelFilter};
use simple_logger::SimpleLogger;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
/// A screenshot tool written in Rust
struct CmdArgs {
    /// Filename to use for screenshot without file extension
    #[arg(short, long)]
    filename: Option<String>,
    /// Directory where the screenshot will be saved
    #[arg(short, long)]
    directory: Option<String>,
    /// Format to use for encoding screenshot (png, jpg, ppm)
    #[arg(short, long)]
    encoding_format: Option<EncodingFormat>,
    /// X coordinate for screenshot region
    #[arg(short, long)]
    x: Option<i32>,
    /// Y coordinate for screenshot region
    #[arg(short, long)]
    y: Option<i32>,
    /// Width for screenshot region
    #[arg(short, long)]
    width: Option<i32>,
    /// Height for screenshot region
    #[arg(short, long)]
    height: Option<i32>,
    /// Make a screenshot of the active window
    #[arg(short, long)]
    active: bool,
    /// Name of the output to screenshot. E.g. DP-1, eDP-1
    #[arg(short, long)]
    output_name: Option<String>,
}

fn main() -> Result<()> {
    // Setup logger
    SimpleLogger::new()
        .with_level(LevelFilter::Warn)
        .env()
        .init()
        .unwrap();

    gui::run()?;
    return Ok(());

    // Parse command line args
    let args = CmdArgs::parse();

    // Get filename
    let filename = if let Some(filename) = args.filename.as_ref() {
        filename.clone()
    } else {
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

    // Get encoding that should be used for screenshot
    let image_encoding = args.encoding_format.unwrap_or(EncodingFormat::Png);

    // Get the directory where the screenshot should be saved
    let directory = if let Some(directory) = args.directory.as_ref() {
        directory.clone()
    } else {
        get_screenshot_directory().context("Could not get a writeable directory for screenshot")?
    };

    // Take the screenshot
    let mut platform = create_platform()?;
    let outputs = platform.outputs();

    // Find output by name if needed
    let output = get_output(args.output_name.clone(), &outputs)?;

    // Get region on which screenshot should be captured
    let region = if args.active {
        Some(platform.focused_window_area()?)
    } else if let Some(region) = get_region_from_args(&args, output) {
        Some(region?)
    } else {
        None
    };

    // Get matching output for region if needed
    let output = if let Some(region) = region {
        find_output_from_region(region, &outputs)?
    } else {
        output
    };
    debug!("Take screenshot on output {:?}", output);

    let frame = platform.capture_frame(output, false, region)?;

    // Write screenshot to disk
    let path = format!(
        "{}/{}.{}",
        directory,
        filename,
        Into::<String>::into(image_encoding)
    );
    debug!("Write screenshot to {}", path);
    write_to_file(File::create(path)?, image_encoding, frame)?;

    Ok(())
}

/// Extract region from command line arguments
fn get_region_from_args(args: &CmdArgs, output: &Output) -> Option<Result<Region>> {
    if args.x.is_some() || args.y.is_some() || args.width.is_some() || args.height.is_some() {
        let x = args.x.unwrap_or(0);
        let y = args.y.unwrap_or(0);
        let width = args.width.unwrap_or((output.width as i32 - x).max(0));
        let height = args.height.unwrap_or((output.height as i32 - y).max(0));

        let capture_region = Region::new(x, y, width, height);
        // TODO: Make output_region part of Output
        let output_region = Region::new(output.x, output.y, output.width, output.height);
        if !output_region.contains(capture_region) {
            Some(anyhow!("Region is invalid"));
        }

        return Some(Ok(Region::new(x, y, width, height)));
    }

    None
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
