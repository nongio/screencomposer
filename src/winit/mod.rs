use std::time::Duration;

use crate::{
    state::{Backend, ScreenComposer},
    CalloopData,
};

use smithay::{
    backend::{
        renderer::{damage::OutputDamageTracker, gles::GlesRenderer},
        winit::{self, WinitEvent, WinitEventLoop, WinitGraphicsBackend},
    },
    output::{Mode, Output, PhysicalProperties, Scale, Subpixel},
    reexports::{
        calloop::{
            timer::{TimeoutAction, Timer},
            LoopHandle,
        },
        wayland_server::{protocol::wl_surface::WlSurface, Display},
        winit::{dpi::LogicalSize, window::WindowBuilder},
    },
    utils::Transform,
};
use tracing::{error, trace};

pub struct WinitData {
    backend: WinitGraphicsBackend<GlesRenderer>,
    pub output: Output,
    damage_tracker: OutputDamageTracker,
    // dmabuf_state: (DmabufState, DmabufGlobal, Option<DmabufFeedback>),
    full_redraw: u8,
    #[cfg(feature = "debug")]
    pub fps: fps_ticker::Fps,
}

impl Backend for WinitData {
    fn seat_name(&self) -> String {
        String::from("winit")
    }
    fn reset_buffers(&mut self, _: &Output) {
        self.full_redraw = 4;
    }
    fn early_import(&mut self, _surface: &WlSurface) {}
}

// struct LayerCommitTexture {
//     pub texture: u32,
//     pub commit_counter: CommitCounter,
// }

pub fn init_winit(
    event_loop_handle: LoopHandle<'static, CalloopData<WinitData>>,
) -> Result<ScreenComposer<WinitData>, &'static str> {
    let display: Display<ScreenComposer<WinitData>> = Display::new().unwrap();
    let display_handle = display.handle();

    let (mut backend, mut winit_event_loop) = winit::init_from_builder(
        WindowBuilder::new()
            .with_inner_size(LogicalSize::new(2256.0 / 1.5, 1504.0 / 1.5))
            .with_title("ScreenComposer")
            .with_visible(true),
    )
    .unwrap();
    let size = backend.window_size();
    // let scale = backend.window().scale_factor();

    let mode = Mode {
        size,
        refresh: 60_000,
    };

    let output = Output::new(
        "winit".to_string(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "ScreenComposer".into(),
            model: "Winit".into(),
        },
    );

    // let sample_count: usize = 0;
    // let stencil_bits: usize = 8;

    let egl_surface = backend.egl_surface();
    let renderer: &mut GlesRenderer = backend.renderer();
    let egl_context = renderer.egl_context();
    unsafe {
        let res = egl_context.make_current_with_surface(&egl_surface);
        res.unwrap_or_else(|err| {
            error!("Error making context current: {:?}", err);
        })
    }
    backend.submit(None).unwrap();

    let _global = output.create_global::<ScreenComposer<WinitData>>(&display.handle());
    output.change_current_state(
        Some(mode),
        Some(Transform::Flipped180),
        Some(Scale::Fractional(1.5)),
        Some((0, 0).into()),
    );
    output.set_preferred(mode);

    let damage_tracker: OutputDamageTracker = OutputDamageTracker::from_output(&output);

    let winit_data = WinitData {
        backend,
        output: output.clone(),
        damage_tracker,
        fps: fps_ticker::Fps::default(),
        // dmabuf_state,
        full_redraw: 0,
    };

    let timer = Timer::immediate();
    let mut state =
        ScreenComposer::new(event_loop_handle.clone(), display_handle, winit_data, true);

    state.space.map_output(&output, (0, 0));

    event_loop_handle
        .insert_source(timer, move |_, _data, calloop_data| {
            winit_dispatch(
                // &mut backend,
                &mut winit_event_loop,
                calloop_data,
                // &mut damage_tracker,
            )
            .unwrap();
            TimeoutAction::ToDuration(Duration::from_millis(16))
        })
        .unwrap();

    Ok(state)
}

pub fn winit_dispatch(
    // backend: &mut WinitGraphicsBackend<GlesRenderer>,
    winit: &mut WinitEventLoop,
    data: &mut CalloopData<WinitData>,
    // output: &Output,
    // _damage_tracker: &mut OutputDamageTracker,
) -> Result<(), Box<dyn std::error::Error>> {
    let res = winit.dispatch_new_events(|event| match event {
        WinitEvent::Resized { size, .. } => {
            data.state.backend_data.output.change_current_state(
                Some(Mode {
                    size,
                    refresh: 60_000,
                }),
                None,
                None,
                None,
            );
        }
        WinitEvent::Input(event) => {
            if let smithay::backend::input::InputEvent::Keyboard { event, .. } = event {
                trace!("winit event input: {:?}", event);
            }
            data.state.process_input_event_winit(event)
        }
        WinitEvent::Redraw => {
            // let now = instant.elapsed().as_secs_f64();
            // let frame_number = (now / 0.016).floor() as i32;
            // if update_frame != frame_number {
            // update_frame = frame_number;
            let dt = 0.016;
            // state.needs_redraw =
            data.state.engine.update(dt);
            // if needs_redraw {
            // env.windowed_context.window().request_redraw();
            // draw_frame = -1;
            // }
            // }
        }
        _ => (),
    });

    // if let Err(x) = res {
    //     // Stop the loop
    //     // state.event_loop_handle.

    //     return Ok(());
    // } else {
    //     res?;
    // }

    Ok(())
}
