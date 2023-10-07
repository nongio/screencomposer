#![allow(irrefutable_let_patterns)]
#![allow(deprecated)]

use screen_composer::CalloopData;
use screen_composer::{state::ScreenComposer, winit::WinitData};
use smithay::backend::renderer::damage::OutputDamageTracker;
use smithay::reexports::{calloop::EventLoop, wayland_server::Display};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Ok(env_filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_max_level(tracing::Level::TRACE)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .init();
    }

    let mut event_loop: EventLoop<CalloopData<WinitData>> = EventLoop::try_new()?;

    let mut display: Display<ScreenComposer<WinitData>> = Display::new()?;

    let (winit_data) = screen_composer::winit::init_winit(event_loop.handle(), &mut display)?;

    let state = ScreenComposer::new(event_loop.handle(), &mut display, &winit_data, true);

    let mut data = CalloopData { state, display };

    // let skia_renderer = Some(layers::renderer::skia_fbo::SkiaFboRenderer::create(
    //     size.to_point().x,
    //     size.to_point().y,
    //     sample_count,
    //     stencil_bits,
    //     0,
    // ));

    data.state.space.map_output(&winit_data.output, (0, 0));
    if let Some(socket_name) = data.state.socket_name.clone() {
        std::env::set_var("WAYLAND_DISPLAY", socket_name);
    }

    let mut args = std::env::args().skip(1);
    let flag = args.next();
    let arg = args.next();

    match (flag.as_deref(), arg) {
        (Some("-c") | Some("--command"), Some(command)) => {
            std::process::Command::new(command).spawn().ok();
        }
        _ => {
            std::process::Command::new("terminator").spawn().ok();
            // std::process::Command::new("firefox").spawn().ok();
        }
    }

    event_loop.run(None, &mut data, move |_| {
        // Smallvil is running
    })?;

    Ok(())
}
