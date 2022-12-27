use crate::platform::Frame;
use anyhow::Result;
use image::codecs::pnm::{self, PnmEncoder};
use image::ImageEncoder;
use image::{
    codecs::{jpeg::JpegEncoder, png::PngEncoder},
    ColorType,
};
use log::debug;
use std::env;
use std::io::Write;

/// Supported image encoding formats.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum EncodingFormat {
    /// Jpeg / Jpg encoder.
    Jpg,
    /// Png encoder.
    Png,
    /// Ppm encoder
    Ppm,
}

impl From<String> for EncodingFormat {
    fn from(value: String) -> Self {
        let value = value.to_lowercase();
        match value.as_str() {
            "jpg" => EncodingFormat::Jpg,
            "jpeg" => EncodingFormat::Jpg,
            "png" => EncodingFormat::Png,
            "ppm" => EncodingFormat::Ppm,
            _ => EncodingFormat::Png,
        }
    }
}

impl Into<String> for EncodingFormat {
    fn into(self) -> String {
        match self {
            EncodingFormat::Png => "png".into(),
            EncodingFormat::Jpg => "jpg".into(),
            EncodingFormat::Ppm => "ppm".into(),
        }
    }
}

// Write an instance of FrameCopy to anything that implements Write trait. Eg: Stdout or a file
/// on the disk.
pub fn write_to_file(
    mut output_file: impl Write,
    encoding_format: EncodingFormat,
    frame_copy: Frame,
) -> Result<()> {
    debug!(
        "Writing to disk with encoding format: {:?}",
        encoding_format
    );
    match encoding_format {
        EncodingFormat::Jpg => {
            JpegEncoder::new(&mut output_file).write_image(
                &frame_copy.frame_mmap,
                frame_copy.frame_format.width,
                frame_copy.frame_format.height,
                frame_copy.frame_color_type,
            )?;
            output_file.flush()?;
        }
        EncodingFormat::Png => {
            PngEncoder::new(&mut output_file).write_image(
                &frame_copy.frame_mmap,
                frame_copy.frame_format.width,
                frame_copy.frame_format.height,
                frame_copy.frame_color_type,
            )?;
            output_file.flush()?;
        }
        EncodingFormat::Ppm => {
            let rgb8_data = if let ColorType::Rgba8 = frame_copy.frame_color_type {
                let mut data = Vec::with_capacity(
                    (3 * frame_copy.frame_format.width * frame_copy.frame_format.height) as _,
                );
                for chunk in frame_copy.frame_mmap.chunks_exact(4) {
                    data.extend_from_slice(&chunk[..3]);
                }
                data
            } else {
                unimplemented!("Currently only ColorType::Rgba8 is supported")
            };

            PnmEncoder::new(&mut output_file)
                .with_subtype(pnm::PnmSubtype::Pixmap(pnm::SampleEncoding::Binary))
                .write_image(
                    &rgb8_data,
                    frame_copy.frame_format.width,
                    frame_copy.frame_format.height,
                    ColorType::Rgb8,
                )?;
            output_file.flush()?;
        }
    }

    Ok(())
}

pub fn get_screenshot_directory() -> Result<String> {
    // First try to use XDG_PICTURES_DIR.
    // If that fails use home directory.
    // If that fails use the current directory
    Ok(dirs::picture_dir()
        .unwrap_or(dirs::home_dir().unwrap_or(env::current_dir()?))
        .to_string_lossy()
        .into())
}
