use std::fs::{File, OpenOptions};
use std::os::fd::{AsFd, OwnedFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use drm_fourcc::DrmFourcc;
use gbm::{BufferObjectFlags, Device, Format};
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::wl_buffer::{self, WlBuffer};
use wayland_client::protocol::wl_callback::{self, WlCallback};
use wayland_client::protocol::wl_compositor::WlCompositor;
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{delegate_noop, Connection, Dispatch, QueueHandle};
use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_buffer_params_v1::{
    self, ZwpLinuxBufferParamsV1,
};
use wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1;
use wayland_protocols::xdg::shell::client::xdg_surface::{self, XdgSurface};
use wayland_protocols::xdg::shell::client::xdg_toplevel::{self, XdgToplevel};
use wayland_protocols::xdg::shell::client::xdg_wm_base::{self, XdgWmBase};

fn main() -> Result<()> {
    let allocator = GbmAllocator::new().context("initializing GBM allocator")?;
    let conn = Connection::connect_to_env().context("connecting to the Wayland compositor")?;
    let (globals, mut event_queue) = registry_queue_init(&conn).context("querying globals")?;
    let qh = event_queue.handle();

    let compositor = globals
        .bind::<WlCompositor, _, _>(&qh, 1..=u32::MAX, ())
        .context("binding wl_compositor")?;
    let surface = compositor.create_surface(&qh, ());

    let xdg_wm_base = globals
        .bind::<XdgWmBase, _, _>(&qh, 1..=u32::MAX, ())
        .context("binding xdg_wm_base")?;
    let xdg_surface = xdg_wm_base.get_xdg_surface(&surface, &qh, ());
    let xdg_toplevel = xdg_surface.get_toplevel(&qh, ());
    xdg_toplevel.set_title("ScreenComposer Client Rainbow".to_string());

    let dmabuf = globals
        .bind::<ZwpLinuxDmabufV1, _, _>(&qh, 1..=u32::MAX, ())
        .context("binding zwp_linux_dmabuf_v1")?;

    surface.commit();

    let mut app = AppState::new(allocator, dmabuf.clone(), surface.clone());

    event_queue
        .roundtrip(&mut app)
        .context("performing initial roundtrip")?;
    while !app.configured {
        event_queue.blocking_dispatch(&mut app)?;
    }

    app.render_frame(&qh)?;

    while app.running {
        event_queue.dispatch_pending(&mut app)?;
        app.cleanup_buffers();
        if app.running && app.configured && app.ready_for_next_frame() {
            app.render_frame(&qh)?;
        }
        event_queue.blocking_dispatch(&mut app)?;
        app.cleanup_buffers();
    }

    drop(xdg_toplevel);
    drop(xdg_surface);

    Ok(())
}

struct BufferUserData {
    released: Arc<AtomicBool>,
}

struct FrameCallbackData {
    done: Arc<AtomicBool>,
}

struct PendingBuffer {
    buffer: WlBuffer,
    released: Arc<AtomicBool>,
}

struct AppState {
    running: bool,
    configured: bool,
    width: u32,
    height: u32,
    allocator: GbmAllocator,
    dmabuf: ZwpLinuxDmabufV1,
    surface: WlSurface,
    buffers: Vec<PendingBuffer>,
    frame_callback: Option<(WlCallback, Arc<AtomicBool>)>,
    start: Instant,
}

impl AppState {
    fn new(allocator: GbmAllocator, dmabuf: ZwpLinuxDmabufV1, surface: WlSurface) -> Self {
        Self {
            running: true,
            configured: false,
            width: 640,
            height: 480,
            allocator,
            dmabuf,
            surface,
            buffers: Vec::new(),
            frame_callback: None,
            start: Instant::now(),
        }
    }

    fn ready_for_next_frame(&self) -> bool {
        self.frame_callback
            .as_ref()
            .map(|(_, done)| done.load(Ordering::Acquire))
            .unwrap_or(true)
    }

    fn cleanup_buffers(&mut self) {
        self.buffers.retain(|entry| {
            if entry.released.load(Ordering::Acquire) {
                entry.buffer.clone().destroy();
                false
            } else {
                true
            }
        });
    }

    fn render_frame(&mut self, qh: &QueueHandle<Self>) -> Result<()> {
        if !self.configured {
            return Ok(());
        }

        self.cleanup_buffers();

        let (width, height) = (self.width.max(1), self.height.max(1));
        let elapsed = self.start.elapsed();
        let color = animated_color(elapsed);

        let frame = self
            .allocator
            .create_frame(width, height, color)
            .context("creating GBM buffer")?;

        let GbmFrame {
            fd,
            stride,
            offset,
            modifier,
        } = frame;

        let released = Arc::new(AtomicBool::new(false));
        let params = self.dmabuf.create_params(qh, ());
        let modifier_hi = (modifier >> 32) as u32;
        let modifier_lo = (modifier & 0xffff_ffff) as u32;
        params.add(fd.as_fd(), 0, offset, stride, modifier_hi, modifier_lo);
        let buffer = params.create_immed(
            width as i32,
            height as i32,
            DrmFourcc::Xrgb8888 as u32,
            zwp_linux_buffer_params_v1::Flags::empty(),
            qh,
            BufferUserData {
                released: released.clone(),
            },
        );
        params.destroy();

        self.surface.attach(Some(&buffer), 0, 0);
        self.surface
            .damage_buffer(0, 0, width as i32, height as i32);
        let done = Arc::new(AtomicBool::new(false));
        let callback = self
            .surface
            .frame(qh, FrameCallbackData { done: done.clone() });
        self.frame_callback = Some((callback.clone(), done));
        self.surface.commit();

        self.buffers.push(PendingBuffer { buffer, released });

        Ok(())
    }
}

impl Dispatch<WlCallback, FrameCallbackData> for AppState {
    fn event(
        state: &mut Self,
        callback: &WlCallback,
        event: wl_callback::Event,
        data: &FrameCallbackData,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { .. } = event {
            data.done.store(true, Ordering::Release);
            if let Some((tracked, _)) = &state.frame_callback {
                if tracked == callback {
                    state.frame_callback = None;
                }
            }
        }
    }
}

impl Dispatch<WlBuffer, BufferUserData> for AppState {
    fn event(
        _state: &mut Self,
        _buffer: &WlBuffer,
        event: wl_buffer::Event,
        data: &BufferUserData,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let wl_buffer::Event::Release = event {
            data.released.store(true, Ordering::Release);
        }
    }
}

impl Dispatch<XdgWmBase, ()> for AppState {
    fn event(
        _state: &mut Self,
        base: &XdgWmBase,
        event: xdg_wm_base::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            base.pong(serial);
        }
    }
}

impl Dispatch<XdgSurface, ()> for AppState {
    fn event(
        state: &mut Self,
        surface: &XdgSurface,
        event: xdg_surface::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            surface.ack_configure(serial);
            state.configured = true;
        }
    }
}

impl Dispatch<XdgToplevel, ()> for AppState {
    fn event(
        state: &mut Self,
        _toplevel: &XdgToplevel,
        event: xdg_toplevel::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            xdg_toplevel::Event::Configure { width, height, .. } => {
                if width > 0 {
                    state.width = width as u32;
                }
                if height > 0 {
                    state.height = height as u32;
                }
            }
            xdg_toplevel::Event::Close => {
                state.running = false;
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwpLinuxBufferParamsV1, ()> for AppState {
    fn event(
        state: &mut Self,
        params: &ZwpLinuxBufferParamsV1,
        event: zwp_linux_buffer_params_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let zwp_linux_buffer_params_v1::Event::Failed = event {
            params.destroy();
            eprintln!("dma-buf allocation failed");
            state.running = false;
        }
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for AppState {
    fn event(
        _state: &mut Self,
        _registry: &WlRegistry,
        _event: wayland_client::protocol::wl_registry::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(AppState: WlCompositor);
delegate_noop!(AppState: WlSurface);
delegate_noop!(AppState: ZwpLinuxDmabufV1);

struct GbmFrame {
    fd: OwnedFd,
    stride: u32,
    offset: u32,
    modifier: u64,
}

struct GbmAllocator {
    device: Device<File>,
}

impl GbmAllocator {
    fn new() -> Result<Self> {
        let file = open_render_node().context("opening DRM render node")?;
        let device = Device::new(file).context("creating GBM device")?;
        Ok(Self { device })
    }

    fn create_frame(&self, width: u32, height: u32, color: [u8; 4]) -> Result<GbmFrame> {
        let mut bo = self
            .device
            .create_buffer_object::<()>(
                width,
                height,
                Format::Xrgb8888,
                BufferObjectFlags::LINEAR | BufferObjectFlags::RENDERING | BufferObjectFlags::WRITE,
            )
            .context("allocating GBM buffer object")?;

        bo.map_mut(&self.device, 0, 0, width, height, |mapping| {
            let stride = mapping.stride() as usize;
            let buffer = mapping.buffer_mut();
            let width_px = width as usize;
            let height_px = height as usize;
            for y in 0..height_px {
                let row_start = y * stride;
                let row = &mut buffer[row_start..row_start + width_px * 4];
                for x in 0..width_px {
                    let offset = x * 4;
                    row[offset..offset + 4].copy_from_slice(&color);
                }
            }
        })
        .context("mapping GBM buffer")??;

        let stride = bo.stride().context("reading stride")?;
        let offset = bo.offset(0).context("reading plane offset")?;
        let modifier: u64 = bo.modifier().context("reading modifier")?.into();
        let fd = bo.fd().context("exporting dma-buf fd")?;

        Ok(GbmFrame {
            fd,
            stride,
            offset,
            modifier,
        })
    }
}

fn open_render_node() -> Result<File> {
    const CANDIDATES: &[&str] = &[
        "/dev/dri/renderD128",
        "/dev/dri/renderD129",
        "/dev/dri/renderD130",
        "/dev/dri/renderD131",
        "/dev/dri/renderD132",
        "/dev/dri/renderD133",
        "/dev/dri/renderD134",
        "/dev/dri/renderD135",
        "/dev/dri/renderD136",
        "/dev/dri/card0",
    ];

    for path in CANDIDATES {
        if let Ok(file) = OpenOptions::new().read(true).write(true).open(path) {
            return Ok(file);
        }
    }

    bail!("no suitable DRM render node found");
}

fn animated_color(elapsed: Duration) -> [u8; 4] {
    let t = elapsed.as_secs_f32();
    let r = ((t * 0.7).sin() * 0.5 + 0.5) * 255.0;
    let g = ((t * 1.1 + 1.5).sin() * 0.5 + 0.5) * 255.0;
    let b = ((t * 0.9 + 3.2).sin() * 0.5 + 0.5) * 255.0;
    [b as u8, g as u8, r as u8, 0xff]
}
