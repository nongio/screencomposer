#![allow(irrefutable_let_patterns)]
#![allow(deprecated)]

use std::{sync::atomic::Ordering, time::Duration};

use screen_composer::{
    state::ScreenComposer,
    udev::{init_udev, UdevData},
    winit::{init_winit, WinitData},
    CalloopData,
};

use smithay::{
    reexports::calloop::EventLoop,
    utils::{Physical, Rectangle, Scale},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Ok(env_filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_max_level(tracing::Level::DEBUG)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();
    }

    // let mut event_loop: EventLoop<CalloopData<WinitData>> = EventLoop::try_new()?;

    // let mut display: Display<ScreenComposer<WinitData>> = Display::new()?;

    // let (winit_data) = screen_composer::winit::init_winit(event_loop.handle(), &mut display)?;

    // let state = ScreenComposer::new(event_loop.handle(), &mut display, &winit_data, true);

    // let mut data = CalloopData {
    //     state,
    //     display: display.handle(),
    // };

    let mut event_loop: EventLoop<'static, _> = EventLoop::try_new().unwrap();
    let mut state;

    let mut args = std::env::args().skip(1);
    let flag = args.next();
    let arg = args.next();

    // static POSSIBLE_BACKENDS: &[&str] = &[
    //     "--winit : Run screen-composer as a X11 or Wayland client using winit.",
    //     "--tty-udev : Run screen-composer as a tty udev client (requires root if without logind).",
    // ];

    // state = init_winit(event_loop.handle()).unwrap();
    state = init_udev(event_loop.handle()).unwrap();
    // match arg.as_ref().map(|s| &s[..]) {
    //     Some("--winit") => {
    //         tracing::info!("Starting with winit backend");
    //     }

    //     Some("--tty-udev") => {
    //         tracing::info!("Starting on a tty using udev");
    //     }

    //     Some(other) => {
    //         tracing::error!("Unknown backend: {}", other);
    //     }
    //     None => {
    //         println!("USAGE: screen-composer --backend");
    //         println!();
    //         println!("Possible backends are:");
    //         for b in POSSIBLE_BACKENDS {
    //             println!("\t{}", b);
    //         }
    //     }
    // }
    let output = state.space.outputs().next().unwrap();
    let output_geometry = state.space.output_geometry(output).unwrap();
    let scale = Scale::from(output.current_scale().integer_scale());
    let geom = output_geometry.to_physical(scale);
    state.init_scene(geom.size.w, geom.size.h);
    /*
     * And run our loop
     */
    // let sample_count: usize = 0;
    // let stencil_bits: usize = 8;

    // let skia_renderer = Some(layers::renderer::skia_fbo::SkiaFboRenderer::create(
    //     geom.size.w,
    //     geom.size.h,
    //     sample_count,
    //     stencil_bits,
    //     0,
    // ));
    while state.running.load(Ordering::SeqCst) {
        let mut calloop_data = CalloopData { state };
        let result = event_loop.dispatch(Some(Duration::from_millis(16)), &mut calloop_data);
        CalloopData { state } = calloop_data;

        if result.is_err() {
            state.running.store(false, Ordering::SeqCst);
        } else {
            // TODO replace with state.update()
            state.space.refresh();
            state.popups.cleanup();
            state.display_handle.flush_clients().unwrap();
        }
    }
    Ok(())
}
