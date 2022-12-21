mod convert;

use clap::Parser;
use std::cell::RefCell;
use std::ffi::CStr;
use std::fs::File;
use std::io::Write;
use std::os::fd::RawFd;
use std::os::unix::io::FromRawFd;
use std::process::exit;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Result};
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::codecs::pnm::{self, PnmEncoder};
use image::{ColorType, ImageEncoder};
use log::{debug, error, info, LevelFilter};
use memmap2::MmapMut;
use nix::sys::{memfd, mman, stat};
use nix::{fcntl, unistd};
use simple_logger::SimpleLogger;
use thiserror::Error;
use wayland_client::protocol::wl_shm;
use wayland_client::protocol::{wl_output::WlOutput, wl_shm::Format};
use wayland_client::{Display, GlobalManager};
use wayland_protocols::wlr::unstable::screencopy::v1::client::zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1;

use crate::convert::create_converter;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
/// A screenshot tool written in Rust
struct CmdArgs {
    /// Filename to use for screenshot without file extension
    #[arg(short, long)]
    filename: Option<String>,
    /// Format to use for encoding screenshot (png, jpg, ppm)
    #[arg(short, long)]
    encoding_format: Option<EncodingFormat>,
}

/// Type of frame supported by the compositor. For now we only support Argb8888,
/// Xrgb8888, and Xbgr8888.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct FrameFormat {
    pub format: Format,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
}

/// State of the frame after attemting to copy it's data to a wl_buffer.
#[derive(Debug, Copy, Clone, PartialEq)]
enum FrameState {
    /// Compositor returned a failed event on calling `frame.copy`.
    Failed,
    /// Compositor sent a Ready event on calling `frame.copy`.
    Finished,
}

/// The copied frame comprising of the FrameFormat, ColorType (Rgba8), and a memory backed shm
/// file that holds the image data in it.
#[derive(Debug)]
pub struct FrameCopy {
    pub frame_format: FrameFormat,
    pub frame_color_type: ColorType,
    pub frame_mmap: MmapMut,
}

/// Supported image encoding formats.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum EncodingFormat {
    /// Jpeg / jpg encoder.
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

fn main() -> Result<()> {
    // Setup logger
    SimpleLogger::new()
        .with_level(LevelFilter::Warn)
        .env()
        .init()
        .unwrap();

    // Parse command line args
    let args = CmdArgs::parse();
    let filename = match args.filename {
        Some(filename) => filename,
        None => "screenshot".into(),
    };
    let image_encoding = match args.encoding_format {
        Some(image_encoding) => image_encoding,
        None => EncodingFormat::Png,
    };

    // Connect to the server
    let display = Display::connect_to_env().unwrap();
    let mut event_queue = display.create_event_queue();
    let attached_display = (*display).clone().attach(event_queue.token());
    let globals = GlobalManager::new(&attached_display);

    // A roundtrip synchronization to make sure the server received our registry
    // creation and sent us the global list
    event_queue.sync_roundtrip(&mut (), |_, _, _| unreachable!())?;

    // Get the output
    let output = globals
        .instantiate_exact::<WlOutput>(4)
        .expect("Could not get output");
    output.quick_assign({
        use wayland_client::protocol::wl_output::Event;
        move |_, event, _| match event {
            Event::Name { name } => {
                info!("Use output: {}", name);
            }
            _ => (),
        }
    });

    event_queue.sync_roundtrip(&mut (), |_, _, _| ())?;

    // Instantiating screencopy manager.
    let screencopy_manager = match globals.instantiate_exact::<ZwlrScreencopyManagerV1>(3) {
        Ok(x) => x,
        Err(e) => {
            error!("Failed to create screencopy manager. Does your compositor implement ZwlrScreencopy?");
            panic!("{:#?}", e);
        }
    };

    let frame_formats = Rc::new(RefCell::new(Vec::new()));
    let frame_state = Rc::new(RefCell::new(None));
    let frame_buffer_done = Rc::new(AtomicBool::new(false));

    let cursor_overlay = 0;

    // Take screenshot
    let frame = screencopy_manager.capture_output(cursor_overlay, &output);
    frame.quick_assign({
        let frame_formats = frame_formats.clone();
        let frame_state = frame_state.clone();
        let frame_buffer_done = frame_buffer_done.clone();
        move |_, event, _| {
            use wayland_protocols::wlr::unstable::screencopy::v1::client::zwlr_screencopy_frame_v1::Event;
            match event {
                Event::Buffer { format, width, height, stride } =>  {
                    debug!("Received Buffer event");
                    frame_formats.borrow_mut().push(FrameFormat {
                        format,
                        width,
                        height,
                        stride,
                    })
                },
                Event::Flags { flags: _ } => {
                    debug!("Received Flags event");
                },
                Event::Ready { tv_sec_hi: _, tv_sec_lo: _, tv_nsec: _ } => {
                    // On succesfully copy, a Ready event is sent. Otherwise, a
                    // "Failed" event will be sent. This is useful to determine 
                    // if the copy was succesful.
                    debug!("Received Ready event");
                    frame_state.borrow_mut().replace(FrameState::Finished);
                },
                Event::Failed => {
                    debug!("Received Failed event");
                    frame_state.borrow_mut().replace(FrameState::Failed);
                },
                Event::Damage { x: _, y: _, width: _, height: _ } => {
                    debug!("Received Damage event");
                },
                Event::LinuxDmabuf { format: _, width: _, height: _ } => {
                    debug!("Received LinuxDmabuf event");
                },
                Event::BufferDone => {
                    // BufferDone event gets sent if all frame screen events are done.
                    // This event gets used to notify our code to proceed further and call the copy
                    // method on the frame.
                    debug!("Received BufferDone event");
                    frame_buffer_done.store(true, Ordering::SeqCst);
                },
                _ => unreachable!(),
            }
        }
    });

    // Empty internal event buffer until buffer_done is set to true which is when the Buffer done
    // event is fired, aka the capture from the compositor is succesful.
    while !frame_buffer_done.load(Ordering::SeqCst) {
        event_queue.dispatch(&mut (), |_, _, _| unreachable!())?;
    }

    debug!(
        "Received compositor frame buffer formats: {:#?}",
        frame_formats
    );
    // Filter advertised wl_shm formats and select the first one that matches.
    let frame_format = frame_formats
        .borrow()
        .iter()
        .find(|frame| {
            matches!(
                frame.format,
                wl_shm::Format::Xbgr2101010
                    | wl_shm::Format::Abgr2101010
                    | wl_shm::Format::Argb8888
                    | wl_shm::Format::Xrgb8888
                    | wl_shm::Format::Xbgr8888
            )
        })
        .copied();
    debug!("Selected frame buffer format: {:#?}", frame_format);

    // Check if frame format exists.
    let frame_format = match frame_format {
        Some(format) => format,
        None => {
            error!("No suitable frame format found");
            exit(1);
        }
    };

    // Bytes of data in the frame = stride * height.
    let frame_bytes = frame_format.stride * frame_format.height;

    // Create an in memory file and return it's file descriptor.
    let mem_fd = create_shm_fd()?;
    let mem_file = unsafe { File::from_raw_fd(mem_fd) };
    mem_file.set_len(frame_bytes as u64)?;

    // Instantiate shm global.
    let shm = globals.instantiate_exact::<wl_shm::WlShm>(1)?;
    let shm_pool = shm.create_pool(mem_fd, frame_bytes as i32);
    let buffer = shm_pool.create_buffer(
        0,
        frame_format.width as i32,
        frame_format.height as i32,
        frame_format.stride as i32,
        frame_format.format,
    );

    // Copy the pixel data advertised by the compositor into the buffer we just created.
    frame.copy(&buffer);

    let frame_copy = read_frame(&mut event_queue, frame_state, frame_format, &mem_file)?;

    // Write screenshot to disk
    let path = format!("{}.{}", filename, Into::<String>::into(image_encoding));
    debug!("Write screenshot to {}", path);
    write_to_file(File::create(path)?, image_encoding, frame_copy)?;

    Ok(())
}

#[derive(Error, Debug)]
enum ReadFrameError {
    #[error("Could not copy frame from compositor to client")]
    FrameCopy,
}

fn read_frame(
    event_queue: &mut wayland_client::EventQueue,
    frame_state: Rc<RefCell<Option<FrameState>>>,
    frame_format: FrameFormat,
    mem_file: &File,
) -> Result<FrameCopy> {
    loop {
        // Let the compositor dispatch Frame events
        debug!("Dispatch event queue and wait for Failed or Finished events");
        event_queue.dispatch(&mut (), |_, _, _| {})?;

        // Try to read the frame from the compositor
        let frame_copy = try_read_frame(frame_state.clone(), frame_format, &mem_file)?;
        if frame_copy.is_some() {
            debug!("Read frame succesful");
            // Compositor did not emit Finished or Failed events. Let's try again.
            return Ok(frame_copy.unwrap());
        }
        debug!("Failed or Finished events did not arrive yet. Try again.");
    }
}

fn try_read_frame(
    frame_state: Rc<RefCell<Option<FrameState>>>,
    frame_format: FrameFormat,
    mem_file: &File,
) -> Result<Option<FrameCopy>> {
    // Basically reads, if frame state is not None then...
    if let Some(state) = frame_state.borrow_mut().take() {
        let frame_copy = match state {
            FrameState::Failed => {
                error!("Frame copy failed");
                bail!(ReadFrameError::FrameCopy);
            }
            FrameState::Finished => {
                // Create a writeable memory map backed by a mem_file.
                let mut frame_mmap = unsafe { MmapMut::map_mut(mem_file)? };
                let data = &mut *frame_mmap;
                let frame_color_type = if let Some(converter) =
                    create_converter(frame_format.format)
                {
                    converter.convert_inplace(data)
                } else {
                    error!("Unsupported buffer format: {:?}", frame_format.format);
                    error!("You can send a feature request for the above format to the mailing list for wayshot over at https://sr.ht/~shinyzenith/wayshot.");
                    todo!()
                };
                FrameCopy {
                    frame_format,
                    frame_color_type,
                    frame_mmap,
                }
            }
        };
        return Ok(Some(frame_copy));
    }

    Ok(None)
}

/// Return a RawFd to a shm file. We use memfd create on linux and shm_open for BSD support.
/// You don't need to mess around with this function, it is only used by
/// capture_output_frame.
fn create_shm_fd() -> std::io::Result<RawFd> {
    // Only try memfd on linux and freebsd.
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    loop {
        // Create a file that closes on succesful execution and seal it's operations.
        match memfd::memfd_create(
            CStr::from_bytes_with_nul(b"wayshot\0").unwrap(),
            memfd::MemFdCreateFlag::MFD_CLOEXEC | memfd::MemFdCreateFlag::MFD_ALLOW_SEALING,
        ) {
            Ok(fd) => {
                // This is only an optimization, so ignore errors.
                // F_SEAL_SRHINK = File cannot be reduced in size.
                // F_SEAL_SEAL = Prevent further calls to fcntl().
                let _ = fcntl::fcntl(
                    fd,
                    fcntl::F_ADD_SEALS(
                        fcntl::SealFlag::F_SEAL_SHRINK | fcntl::SealFlag::F_SEAL_SEAL,
                    ),
                );
                return Ok(fd);
            }
            Err(nix::errno::Errno::EINTR) => continue,
            Err(nix::errno::Errno::ENOSYS) => break,
            Err(errno) => return Err(std::io::Error::from(errno)),
        }
    }

    // Fallback to using shm_open.
    let sys_time = SystemTime::now();
    let mut mem_file_handle = format!(
        "/wayshot-{}",
        sys_time.duration_since(UNIX_EPOCH).unwrap().subsec_nanos()
    );
    loop {
        match mman::shm_open(
            // O_CREAT = Create file if does not exist.
            // O_EXCL = Error if create and file exists.
            // O_RDWR = Open for reading and writing.
            // O_CLOEXEC = Close on succesful execution.
            // S_IRUSR = Set user read permission bit .
            // S_IWUSR = Set user write permission bit.
            mem_file_handle.as_str(),
            fcntl::OFlag::O_CREAT
                | fcntl::OFlag::O_EXCL
                | fcntl::OFlag::O_RDWR
                | fcntl::OFlag::O_CLOEXEC,
            stat::Mode::S_IRUSR | stat::Mode::S_IWUSR,
        ) {
            Ok(fd) => match mman::shm_unlink(mem_file_handle.as_str()) {
                Ok(_) => return Ok(fd),
                Err(errno) => match unistd::close(fd) {
                    Ok(_) => return Err(std::io::Error::from(errno)),
                    Err(errno) => return Err(std::io::Error::from(errno)),
                },
            },
            Err(nix::errno::Errno::EEXIST) => {
                // If a file with that handle exists then change the handle
                mem_file_handle = format!(
                    "/wayshot-{}",
                    sys_time.duration_since(UNIX_EPOCH).unwrap().subsec_nanos()
                );
                continue;
            }
            Err(nix::errno::Errno::EINTR) => continue,
            Err(errno) => return Err(std::io::Error::from(errno)),
        }
    }
}

// Write an instance of FrameCopy to anything that implements Write trait. Eg: Stdout or a file
/// on the disk.
pub fn write_to_file(
    mut output_file: impl Write,
    encoding_format: EncodingFormat,
    frame_copy: FrameCopy,
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
