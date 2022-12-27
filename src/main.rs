mod output;
mod platform;

use clap::Parser;
use output::EncodingFormat;
use platform::{create_platform, Output, Region};

use std::fs::File;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::output::{get_screenshot_directory, write_to_file};
use anyhow::{anyhow, Context, Result};
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
}

fn main() -> Result<()> {
    // Setup logger
    SimpleLogger::new()
        .with_level(LevelFilter::Warn)
        .env()
        .init()
        .unwrap();

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
    let output = &outputs[0];

    // Get region on which screenshot should be captured
    let region = if args.active {
        Some(platform.focused_window_area()?)
    } else if let Some(region) = get_region_from_args(&args, output) {
        Some(region?)
    } else {
        None
    };

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

fn get_region_from_args(args: &CmdArgs, output: &Output) -> Option<Result<Region>> {
    if args.x.is_some() || args.y.is_some() || args.width.is_some() || args.height.is_some() {
        let x = args.x.unwrap_or(0);
        let y = args.y.unwrap_or(0);
        let width = args.width.unwrap_or((output.width as i32 - x).max(0));
        let height = args.height.unwrap_or((output.height as i32 - y).max(0));

        if x < 0
            || y < 0
            || width < 0
            || height < 0
            || x >= output.width as i32
            || y >= output.height as i32
            || width == 0
            || height == 0
        {
            Some(anyhow!("Region is invalid"));
        }

        return Some(Ok(Region {
            x,
            y,
            width,
            height,
        }));
    }

    None
}
