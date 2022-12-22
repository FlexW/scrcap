use crate::screenshot::FrameFormat;
use image::ColorType;

pub trait Convert {
    /// Convert raw image data into output type, return said type
    fn convert_inplace(&self, data: &mut [u8]) -> ColorType;
}

#[derive(Default)]
struct ConvertBGR10 {}

#[derive(Default)]
struct ConvertNone {}

#[derive(Default)]
struct ConvertRGB8 {}

const SHIFT10BITS_1: u32 = 20;
const SHIFT10BITS_2: u32 = 10;

/// Creates format converter based of input format, return None if conversion
/// isn't possible. Conversion is happening inplace.
pub fn create_converter(format: FrameFormat) -> Box<dyn Convert> {
    match format {
        FrameFormat::Xbgr8888 | FrameFormat::Abgr8888 => Box::new(ConvertNone::default()),
        FrameFormat::Xrgb8888 | FrameFormat::Argb8888 => Box::new(ConvertRGB8::default()),
        FrameFormat::Xbgr2101010 | FrameFormat::Abgr2101010 => Box::new(ConvertBGR10::default()),
    }
}

impl Convert for ConvertNone {
    fn convert_inplace(&self, _data: &mut [u8]) -> ColorType {
        ColorType::Rgba8
    }
}

impl Convert for ConvertRGB8 {
    fn convert_inplace(&self, data: &mut [u8]) -> ColorType {
        for chunk in data.chunks_exact_mut(4) {
            chunk.swap(0, 2);
        }
        ColorType::Rgba8
    }
}

/// Simple conversion from 10 to 8 bits for one channel
fn convert10_to_8(color: u32) -> u8 {
    ((color >> 2) & 255) as u8
}

impl Convert for ConvertBGR10 {
    fn convert_inplace(&self, data: &mut [u8]) -> ColorType {
        for chunk in data.chunks_exact_mut(4) {
            let pixel = ((chunk[3] as u32) << 24)
                | ((chunk[2] as u32) << 16)
                | ((chunk[1] as u32) << 8)
                | chunk[0] as u32;
            let r = convert10_to_8(pixel >> SHIFT10BITS_1);
            let g = convert10_to_8(pixel >> SHIFT10BITS_2);
            let b = convert10_to_8(pixel);
            chunk[0] = b;
            chunk[1] = g;
            chunk[2] = r;
            chunk[3] = 255;
        }
        ColorType::Rgba8
    }
}
