use std::{
    cell::RefCell,
    fs::File,
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
};

use crate::platform::FrameDescription;

use super::{convert::create_converter, Frame, FrameFormat, Output, Platform, Region};
use anyhow::{bail, Context, Result};
use log::{debug, error, info};
use memmap2::MmapMut;
use nix::sys::{memfd, mman, stat};
use nix::{fcntl, unistd};
use std::ffi::CStr;
use std::os::fd::RawFd;
use std::os::unix::io::FromRawFd;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use wayland_client::{
    global_filter,
    protocol::{wl_output::WlOutput, wl_shm},
    Display, EventQueue, GlobalManager, Main,
};
use wayland_protocols::{
    unstable::xdg_output::v1::client::zxdg_output_manager_v1::ZxdgOutputManagerV1,
    wlr::unstable::screencopy::v1::client::zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1,
};

const WL_OUTPUT_VERSION: u32 = 4;

pub struct PlatformWayland {
    event_queue: EventQueue,
    globals: GlobalManager,
    screencopy_manager: Main<ZwlrScreencopyManagerV1>,
    outputs: Vec<WaylandOutput>,
}

impl PlatformWayland {
    pub fn new() -> Result<Self> {
        // Connect to the server
        let display = Display::connect_to_env().context("Could not connect to Wayland server")?;
        let mut event_queue = display.create_event_queue();
        let attached_display = (*display).clone().attach(event_queue.token());

        let wayland_outputs = Rc::new(RefCell::new(Vec::new()));

        let globals = GlobalManager::new_with_cb(
            &attached_display,
            global_filter!([WlOutput, WL_OUTPUT_VERSION, {
                let wayland_outputs = wayland_outputs.clone();
                move |output_handle: Main<WlOutput>, _: DispatchData| {
                    let wayland_outputs = wayland_outputs.clone();

                    output_handle.quick_assign(move |output_handle, event, _| {
                        use wayland_client::protocol::wl_output::Event;
                        match event {
                            Event::Geometry {
                                x: _,
                                y: _,
                                physical_width: _,
                                physical_height: _,
                                subpixel: _,
                                make: _,
                                model: _,
                                transform: _,
                            } => {
                                debug!("Output geometry event");
                                let wayland_output = WaylandOutput {
                                    raw: output_handle.clone(),
                                    output: Output::default(),
                                };
                                wayland_outputs.borrow_mut().push(wayland_output);
                            }
                            _ => (),
                        }
                    })
                }
            }]),
        );

        // A roundtrip synchronization to make sure the server received our registry
        // creation and sent us the global list
        event_queue.sync_roundtrip(&mut (), |_, _, _| unreachable!())?;

        // Init outputs
        event_queue.sync_roundtrip(&mut (), |_, _, _| ())?;

        let mut final_wayland_outputs = Vec::new();

        let xdg_output_manager = globals.instantiate_exact::<ZxdgOutputManagerV1>(3).context("Failed to create xdg output manger. Does your compositor implement ZxdgOutputManagerV1?")?;
        for wayland_output in wayland_outputs.borrow().iter() {
            let xdg_output = xdg_output_manager.get_xdg_output(&wayland_output.raw);

            let output_name = Rc::new(RefCell::new(String::new()));
            let output_x = Rc::new(RefCell::new(0));
            let output_y = Rc::new(RefCell::new(0));
            let output_width = Rc::new(RefCell::new(0));
            let output_height = Rc::new(RefCell::new(0));

            xdg_output.quick_assign({
                let output_name = output_name.clone();
                let output_x = output_x.clone();
                let output_y = output_y.clone();
                let output_width = output_width.clone();
                let output_height = output_height.clone();

                move |_handle, event, _| {
                    use wayland_protocols::unstable::xdg_output::v1::client::zxdg_output_v1::Event;
                    match event {
                        Event::LogicalPosition { x, y } => {
                            debug!("Xdg output logical position event");
                            *output_x.borrow_mut() = x;
                            *output_y.borrow_mut() = y;
                        }
                        Event::LogicalSize { width, height } => {
                            debug!("Xdg output logical size event");
                            *output_width.borrow_mut() = width;
                            *output_height.borrow_mut() = height;
                        }
                        Event::Name { name } => {
                            *output_name.borrow_mut() = name.clone();
                        }
                        Event::Done => {
                            debug!("Xdg output done event");
                        }
                        _ => (),
                    }
                }
            });

            event_queue
                .sync_roundtrip(&mut (), |_, _, _| unreachable!())
                .unwrap();

            let wayland_output = WaylandOutput {
                raw: wayland_output.raw.clone(),
                output: Output {
                    name: output_name.take(),
                    x: output_x.take(),
                    y: output_y.take(),
                    width: output_width.take(),
                    height: output_height.take(),
                    scale: wayland_output.output.scale,
                },
            };
            info!("Found output: {:?}", wayland_output);

            final_wayland_outputs.push(wayland_output);
        }

        // Instantiating screencopy manager
        let screencopy_manager = globals
            .instantiate_exact::<ZwlrScreencopyManagerV1>(3)
            .context(
            "Failed to create screencopy manager. Does your compositor implement ZwlrScreencopy?",
        )?;

        Ok(PlatformWayland {
            event_queue,
            globals,
            screencopy_manager,
            outputs: final_wayland_outputs,
        })
    }

    fn find_wl_output(&self, output: &Output) -> Result<Main<WlOutput>> {
        for wayland_output in &self.outputs {
            if wayland_output.output.name == output.name {
                return Ok(wayland_output.raw.clone());
            }
        }
        bail!("No output found")
    }
}

impl Platform for PlatformWayland {
    fn outputs(&self) -> Vec<Output> {
        self.outputs
            .iter()
            .map(|wayland_output| wayland_output.output.clone())
            .collect::<Vec<_>>()
    }

    fn capture_frame(
        &mut self,
        output: &Output,
        overlay_cursor: bool,
        region: Option<Region>,
    ) -> anyhow::Result<Frame> {
        debug!("Taking screenshot of output {:?}", output.name);
        let wl_output_handle = self.find_wl_output(output)?;

        let frame = if let Some(region) = region {
            debug!("Capture screenshot of region {:?}", region);
            self.screencopy_manager.capture_output_region(
                overlay_cursor as i32,
                &wl_output_handle,
                region.x - output.x as i32,
                region.y - output.y as i32,
                region.width as i32,
                region.height as i32,
            )
        } else {
            debug!("Capture screenshot of whole screen");
            self.screencopy_manager
                .capture_output(overlay_cursor as i32, &wl_output_handle)
        };

        let frame_formats = Rc::new(RefCell::new(Vec::new()));
        let frame_state = Rc::new(RefCell::new(None));
        let frame_buffer_done = Rc::new(AtomicBool::new(false));

        frame.quick_assign({
        let frame_formats = frame_formats.clone();
        let frame_state = frame_state.clone();
        let frame_buffer_done = frame_buffer_done.clone();
        move |_, event, _| {
            use wayland_protocols::wlr::unstable::screencopy::v1::client::zwlr_screencopy_frame_v1::Event;
            match event {
                Event::Buffer { format, width, height, stride } =>  {
                    debug!("Received Buffer event");
                    frame_formats.borrow_mut().push(FrameDescription {
                        format: format.into(),
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
            self.event_queue
                .dispatch(&mut (), |_, _, _| unreachable!())?;
        }

        debug!(
            "Received compositor frame buffer formats: {:?}",
            frame_formats
        );

        // Filter advertised formats and select the first one that matches.
        let frame_format = frame_formats
            .borrow()
            .iter()
            .find(|frame| {
                matches!(
                    frame.format,
                    FrameFormat::Xbgr2101010
                        | FrameFormat::Abgr2101010
                        | FrameFormat::Argb8888
                        | FrameFormat::Xrgb8888
                        | FrameFormat::Xbgr8888
                )
            })
            .copied();
        debug!("Selected frame buffer format: {:?}", frame_format);

        // Check if frame format exists.
        let frame_format = match frame_format {
            Some(format) => format,
            None => {
                bail!("No suitable frame format found");
            }
        };

        // Bytes of data in the frame = stride * height.
        let frame_bytes = frame_format.stride * frame_format.height;

        // Create an in memory file and return it's file descriptor.
        let mem_fd = create_shm_fd()?;
        let mem_file = unsafe { File::from_raw_fd(mem_fd) };
        mem_file.set_len(frame_bytes as u64)?;

        // Instantiate shm global.
        let shm = self.globals.instantiate_exact::<wl_shm::WlShm>(1)?;
        let shm_pool = shm.create_pool(mem_fd, frame_bytes as i32);
        let buffer = shm_pool.create_buffer(
            0,
            frame_format.width as i32,
            frame_format.height as i32,
            frame_format.stride as i32,
            frame_format.format.into(),
        );

        // Copy the pixel data advertised by the compositor into the buffer we just created.
        frame.copy(&buffer);

        let frame = read_frame(&mut self.event_queue, frame_state, frame_format, &mem_file)?;

        Ok(frame)
    }

    fn focused_window_area(&self) -> Result<Region> {
        let mut connection = swayipc::Connection::new()?;
        let tree = connection.get_tree()?;
        let focused_node = tree.find_focused_as_ref(|node: _| node.focused);
        if let Some(focused_node) = focused_node {
            let rect = &focused_node.rect;
            let window_rect = &focused_node.window_rect;

            let x = rect.x + window_rect.x;
            let y = rect.y + window_rect.y;
            let width = window_rect.width;
            let height = window_rect.height;

            debug!(
                "Focused window: {:?} x:{}, y: {}, width: {}, height: {}",
                focused_node.name, x, y, width, height
            );

            return Ok(Region::new(x, y, width, height));
        }

        bail!("Could not find an active window")
    }
}

#[derive(Debug)]
struct WaylandOutput {
    raw: Main<WlOutput>,
    output: Output,
}

/// State of the frame after attemting to copy it's data to a wl_buffer.
#[derive(Debug, Copy, Clone, PartialEq)]
enum FrameState {
    /// Compositor returned a failed event on calling `frame.copy`.
    Failed,
    /// Compositor sent a Ready event on calling `frame.copy`.
    Finished,
}

impl From<wl_shm::Format> for FrameFormat {
    fn from(value: wl_shm::Format) -> Self {
        match value {
            wl_shm::Format::Xbgr2101010 => FrameFormat::Xbgr2101010,
            wl_shm::Format::Xrgb8888 => FrameFormat::Xrgb8888,
            wl_shm::Format::Xbgr8888 => FrameFormat::Xbgr8888,
            wl_shm::Format::Abgr2101010 => FrameFormat::Abgr2101010,
            wl_shm::Format::Abgr8888 => FrameFormat::Abgr8888,
            wl_shm::Format::Argb8888 => FrameFormat::Argb8888,
            _ => panic!("Unsupported wl_shm frame format"),
        }
    }
}

impl Into<wl_shm::Format> for FrameFormat {
    fn into(self) -> wl_shm::Format {
        match self {
            FrameFormat::Xbgr2101010 => wl_shm::Format::Xbgr2101010,
            FrameFormat::Xrgb8888 => wl_shm::Format::Xrgb8888,
            FrameFormat::Xbgr8888 => wl_shm::Format::Xbgr8888,
            FrameFormat::Abgr2101010 => wl_shm::Format::Abgr2101010,
            FrameFormat::Argb8888 => wl_shm::Format::Argb8888,
            FrameFormat::Abgr8888 => wl_shm::Format::Abgr8888,
        }
    }
}

#[derive(Error, Debug)]
enum ReadFrameError {
    #[error("Could not copy frame from compositor to client")]
    FrameCopy,
}

fn read_frame(
    event_queue: &mut wayland_client::EventQueue,
    frame_state: Rc<RefCell<Option<FrameState>>>,
    frame_format: FrameDescription,
    mem_file: &File,
) -> Result<Frame> {
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
    frame_format: FrameDescription,
    mem_file: &File,
) -> Result<Option<Frame>> {
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
                let converter = create_converter(frame_format.format);
                let frame_color_type = converter.convert_inplace(data);
                Frame {
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
