//! Minimal PipeWire video source example using ALLOC_BUFFERS mode.
//!
//! This example demonstrates how to create a PipeWire video source that:
//! - Allocates its own buffers (memfd)
//! - Runs in DRIVER mode (controls its own timing)
//! - Produces a simple animated test pattern
//!
//! Run with: cargo run
//! View with: gst-launch-1.0 pipewiresrc path=<node_id> ! videoconvert ! autovideosink

use std::cell::Cell;
use std::collections::HashMap;
use std::io::Cursor;
use std::mem::size_of;
use std::os::fd::AsRawFd;
use std::rc::Rc;
use std::time::Duration;

use anyhow::{Context, Result};
use memmap2::MmapMut;
use pipewire as pw;
use pw::spa::buffer::DataType;
use pw::spa::param::format::{FormatProperties, MediaSubtype, MediaType};
use pw::spa::param::video::{VideoFormat, VideoInfoRaw};
use pw::spa::param::ParamType;
use pw::spa::pod::serialize::PodSerializer;
use pw::spa::pod::{self, ChoiceValue, Pod, Property, Value};
use pw::spa::sys::*;
use pw::spa::utils::{Choice, ChoiceEnum, ChoiceFlags, Fraction, Id, Rectangle, SpaTypes};
use pw::spa::{param::format_utils, utils};
use pw::stream::{StreamFlags, StreamState};

struct BufferData {
    _memfd: memfd::Memfd,
    mmap: MmapMut,
}

struct StreamState_ {
    format: VideoInfoRaw,
    frame: u64,
    buffers: HashMap<i64, BufferData>,
}

/// Serialize a pod object to bytes and return a Pod reference.
fn make_pod<'a>(buf: &'a mut Vec<u8>, obj: pod::Object) -> &'a Pod {
    *buf = PodSerializer::serialize(Cursor::new(Vec::new()), &Value::Object(obj))
        .expect("serialize pod")
        .0
        .into_inner();
    Pod::from_bytes(buf).expect("pod from bytes")
}

/// Render a simple animated test pattern (XOR gradient).
fn render_pattern(buf: &mut [u8], width: usize, height: usize, stride: usize, frame: u64) {
    for y in 0..height {
        for x in 0..width {
            let idx = y * stride + x * 4;
            buf[idx] = ((x as u64 + frame) & 0xff) as u8; // B
            buf[idx + 1] = ((y as u64 * 2 + frame) & 0xff) as u8; // G
            buf[idx + 2] = (((x ^ y) as u64 + frame * 3) & 0xff) as u8; // R
            buf[idx + 3] = 0xff; // A
        }
    }
}

fn main() -> Result<()> {
    pw::init();

    let width: u32 = 640;
    let height: u32 = 360;
    let fps: u32 = 30;

    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;

    let stream = pw::stream::StreamRc::new(
        core,
        "pipewire-video-source-example",
        pw::properties::properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Camera",
            *pw::keys::MEDIA_CLASS => "Video/Source",
            *pw::keys::NODE_DRIVER => "true",
        },
    )?;

    // Track streaming state for the timer
    let is_streaming = Rc::new(Cell::new(false));
    let is_streaming_for_timer = is_streaming.clone();

    // Timer triggers frame production at ~30fps when streaming
    let stream_ptr = stream.as_raw_ptr();
    let timer = mainloop.loop_().add_timer(move |_| {
        if is_streaming_for_timer.get() {
            unsafe { pw::sys::pw_stream_trigger_process(stream_ptr) };
        }
    });
    timer.update_timer(
        Some(Duration::from_millis(33)),
        Some(Duration::from_millis(33)),
    );

    let state = StreamState_ {
        format: VideoInfoRaw::default(),
        frame: 0,
        buffers: HashMap::new(),
    };

    let is_streaming_for_cb = is_streaming.clone();
    let _listener = stream
        .add_local_listener_with_user_data(state)
        .state_changed(move |stream, _state, _old, new| {
            println!("Stream state: {:?} (node {})", new, stream.node_id());
            is_streaming_for_cb.set(matches!(new, StreamState::Streaming));
        })
        .param_changed(|_stream, state, id, param| {
            let Some(param) = param else { return };
            if id != ParamType::Format.as_raw() {
                return;
            }

            let Ok((media_type, media_subtype)) = format_utils::parse_format(param) else {
                return;
            };
            if media_type != MediaType::Video || media_subtype != MediaSubtype::Raw {
                return;
            }
            if state.format.parse(param).is_err() {
                return;
            }

            let size = state.format.size();
            println!(
                "Format: {:?} {}x{} @ {}/{}",
                state.format.format(),
                size.width,
                size.height,
                state.format.framerate().num,
                state.format.framerate().denom
            );

            // Declare buffer requirements
            let stride = (size.width * 4) as i32;
            let frame_bytes = (size.width * size.height * 4) as i32;

            let buffers_obj = pod::object!(
                SpaTypes::ObjectParamBuffers,
                ParamType::Buffers,
                Property::new(
                    SPA_PARAM_BUFFERS_buffers,
                    pod::Value::Choice(ChoiceValue::Int(Choice(
                        ChoiceFlags::empty(),
                        ChoiceEnum::Range { default: 4, min: 2, max: 8 }
                    )))
                ),
                Property::new(SPA_PARAM_BUFFERS_blocks, pod::Value::Int(1)),
                Property::new(SPA_PARAM_BUFFERS_size, pod::Value::Int(frame_bytes)),
                Property::new(SPA_PARAM_BUFFERS_stride, pod::Value::Int(stride)),
                Property::new(SPA_PARAM_BUFFERS_align, pod::Value::Int(16)),
                Property::new(
                    SPA_PARAM_BUFFERS_dataType,
                    pod::Value::Choice(ChoiceValue::Int(Choice(
                        ChoiceFlags::empty(),
                        ChoiceEnum::Flags {
                            default: 1 << DataType::MemFd.as_raw(),
                            flags: vec![1 << DataType::MemFd.as_raw()],
                        },
                    )))
                ),
            );

            let meta_header = pod::object!(
                SpaTypes::ObjectParamMeta,
                ParamType::Meta,
                Property::new(SPA_PARAM_META_type, pod::Value::Id(Id(SPA_META_Header))),
                Property::new(SPA_PARAM_META_size, pod::Value::Int(size_of::<spa_meta_header>() as i32)),
            );

            let mut b1 = Vec::new();
            let mut b2 = Vec::new();
            let _ = _stream.update_params(&mut [make_pod(&mut b1, buffers_obj), make_pod(&mut b2, meta_header)]);
        })
        .add_buffer(|_stream, state, pw_buffer| {
            let size = state.format.size();
            let frame_bytes = (size.width * size.height * 4) as usize;

            // Create memfd and mmap it
            let memfd = memfd::MemfdOptions::default()
                .create("pw-video-buf")
                .expect("create memfd");
            memfd.as_file().set_len(frame_bytes as u64).expect("set memfd size");
            let mmap = unsafe { MmapMut::map_mut(memfd.as_file()) }.expect("mmap memfd");
            let fd = memfd.as_raw_fd() as i64;

            // Fill in PipeWire buffer metadata
            unsafe {
                let buffer_data = (*(*pw_buffer).buffer).datas;
                (*buffer_data).type_ = DataType::MemFd.as_raw();
                (*buffer_data).fd = fd;
                (*buffer_data).flags = SPA_DATA_FLAG_READWRITE;
                (*buffer_data).mapoffset = 0;
                (*buffer_data).maxsize = frame_bytes as u32;
                (*buffer_data).data = mmap.as_ptr() as *mut _;
            }

            state.buffers.insert(fd, BufferData { _memfd: memfd, mmap });
        })
        .remove_buffer(|_stream, state, pw_buffer| {
            let fd = unsafe { (*(*(*pw_buffer).buffer).datas).fd };
            state.buffers.remove(&fd);
        })
        .process(|stream, state| {
            let size = state.format.size();
            if size.width == 0 || size.height == 0 {
                return;
            }

            let Some(mut buffer) = stream.dequeue_buffer() else { return };
            let datas = buffer.datas_mut();
            if datas.is_empty() {
                return;
            }

            let fd = datas[0].fd() as i64;
            let width = size.width as usize;
            let height = size.height as usize;
            let stride = width * 4;
            let frame_len = stride * height;

            if let Some(buf_data) = state.buffers.get_mut(&fd) {
                render_pattern(&mut buf_data.mmap[..frame_len], width, height, stride, state.frame);

                let chunk = datas[0].chunk_mut();
                *chunk.offset_mut() = 0;
                *chunk.size_mut() = frame_len as u32;
                *chunk.stride_mut() = stride as i32;

                state.frame = state.frame.wrapping_add(1);
            }
        })
        .register()?;

    // Build initial format parameters
    let format = pw::spa::pod::object!(
        SpaTypes::ObjectParamFormat,
        ParamType::EnumFormat,
        pw::spa::pod::property!(FormatProperties::MediaType, Id, MediaType::Video),
        pw::spa::pod::property!(FormatProperties::MediaSubtype, Id, MediaSubtype::Raw),
        pw::spa::pod::property!(FormatProperties::VideoFormat, Id, VideoFormat::BGRx),
        pw::spa::pod::property!(FormatProperties::VideoSize, Rectangle, Rectangle { width, height }),
        pw::spa::pod::property!(FormatProperties::VideoFramerate, Fraction, Fraction { num: fps, denom: 1 }),
    );
    let format_bytes: Vec<u8> = PodSerializer::serialize(Cursor::new(Vec::new()), &Value::Object(format))
        .context("serialize format")?
        .0
        .into_inner();
    let mut params = [Pod::from_bytes(&format_bytes).unwrap()];

    stream.connect(
        utils::Direction::Output,
        None,
        StreamFlags::DRIVER | StreamFlags::ALLOC_BUFFERS,
        &mut params,
    )?;
    stream.set_active(true)?;

    println!("Video source running at {}x{} @ {} fps", width, height, fps);
    println!("View with: gst-launch-1.0 pipewiresrc path=<node_id> ! videoconvert ! autovideosink");

    let _timer = timer; // Keep timer alive
    mainloop.run();

    Ok(())
}
