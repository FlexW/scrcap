mod wayland;

use self::wayland::OutputWayland;
pub use self::wayland::ScreenshotBackendWayland;
pub use anyhow::Result;
use image::ColorType;
use memmap2::MmapMut;

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum FrameFormat {
    Xbgr2101010,
    Xrgb8888,
    Xbgr8888,
    Abgr2101010,
    Abgr8888,
    Argb8888,
}

/// Type of frame supported by the compositor. For now we only support Argb8888,
/// Xrgb8888, and Xbgr8888.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct FrameDescription {
    pub format: FrameFormat,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
}

/// The frame (Screenshot) comprising of the FrameFormat, ColorType (Rgba8), and a memory backed shm
/// file that holds the image data in it.
pub struct Frame {
    pub frame_format: FrameDescription,
    pub frame_color_type: ColorType,
    pub frame_mmap: MmapMut,
}

#[derive(Clone)]
pub enum Output {
    Wayland(OutputWayland),
}

pub struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

pub trait ScreenshotBackend {
    fn outputs(&self) -> Vec<Output>;
    fn screenshot(
        &mut self,
        output: &Output,
        overlay_cursor: bool,
        region: Option<Region>,
    ) -> Result<Frame>;
}
