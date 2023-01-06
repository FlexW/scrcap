mod convert;
mod sway;

use anyhow::Result;
use image::ColorType;
use memmap2::MmapMut;

use self::sway::PlatformWayland;

#[derive(Debug, Default, Clone, Copy)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone)]
pub struct Output {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub scale: i32,
}

impl Default for Output {
    fn default() -> Self {
        Self {
            name: "<unknown>".into(),
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            scale: 1,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum FrameFormat {
    Xbgr2101010,
    Xrgb8888,
    Xbgr8888,
    Abgr2101010,
    Abgr8888,
    Argb8888,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct FrameDescription {
    pub format: FrameFormat,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
}

pub struct Frame {
    pub frame_format: FrameDescription,
    pub frame_mmap: MmapMut,
    pub frame_color_type: ColorType,
}

pub trait Platform {
    fn outputs(&self) -> Vec<Output>;

    fn capture_frame(
        &mut self,
        output: &Output,
        overlay_cursor: bool,
        region: Option<Region>,
    ) -> Result<Frame>;

    fn focused_window_area(&self) -> Result<Region>;
}

pub fn create_platform() -> Result<Box<dyn Platform>> {
    Ok(Box::new(PlatformWayland::new()?))
}
