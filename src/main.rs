mod convert;
mod output;
mod screenshot;
mod sway;

use clap::Parser;
use output::EncodingFormat;

use std::fs::File;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::output::{get_screenshot_directory, write_to_file};
use crate::screenshot::{Region, ScreenshotBackend, ScreenshotBackendWayland};
use crate::sway::active_window_area;
use anyhow::{bail, Context, Result};
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
    let filename = args.filename.unwrap_or({
        // Generate a name
        let time = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(n) => n.as_secs().to_string(),
            Err(_) => {
                warn!("SystemTime before UNIX EPOCH!");
                "TIME-BEFORE-UNIX-EPOCH".into()
            }
        };
        format!("screenshot-{}", time)
    });

    // Get encoding that should be used for screenshot
    let image_encoding = args.encoding_format.unwrap_or(EncodingFormat::Png);

    // Get the directory where the screenshot should be saved
    let directory = args.directory.unwrap_or(
        get_screenshot_directory().context("Could not get a writeable directory for screenshot")?,
    );

    // Take the screenshot
    let mut screenshot_backend = ScreenshotBackendWayland::new()?;
    let outputs = screenshot_backend.outputs();
    let output = &outputs[0];

    let region = if args.active {
        let region = active_window_area()?;
        Some(Region {
            x: region.0,
            y: region.1,
            width: region.2,
            height: region.3,
        })
    } else if args.x.is_some() || args.y.is_some() || args.width.is_some() || args.height.is_some()
    {
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
            bail!("Region is invalid");
        }

        Some(Region {
            x,
            y,
            width,
            height,
        })
    } else {
        None
    };

    let frame = if let Some(region) = region {
        screenshot_backend.screenshot(&output.output, false, Some(region))?
    } else {
        screenshot_backend.screenshot(&output.output, false, None)?
    };

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
