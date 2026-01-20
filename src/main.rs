static POSSIBLE_BACKENDS: &[&str] = &[
    #[cfg(feature = "winit")]
    "--winit : Run anvil as a X11 or Wayland client using winit.",
    #[cfg(feature = "udev")]
    "--tty-udev : Run anvil as a tty udev client (requires root if without logind).",
    #[cfg(feature = "udev")]
    "--probe : Probe available displays and resolutions, then exit.",
    #[cfg(feature = "x11")]
    "--x11 : Run anvil as an X11 client.",
];

#[cfg(feature = "profile-with-tracy-mem")]
#[global_allocator]
static GLOBAL: profiling::tracy_client::ProfiledAllocator<std::alloc::System> =
    profiling::tracy_client::ProfiledAllocator::new(std::alloc::System, 10);

#[tokio::main]
async fn main() {
    if let Ok(env_filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt()
            .compact()
            .with_env_filter(env_filter)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter("info")
            .compact()
            .init();
    }

    #[cfg(feature = "profile-with-tracy")]
    profiling::tracy_client::Client::start();

    profiling::register_thread!("Main Thread");

    #[cfg(feature = "profile-with-puffin")]
    let _server = puffin_http::Server::new(&format!("0.0.0.0:{}", puffin_http::DEFAULT_PORT));
    #[cfg(feature = "profile-with-puffin")]
    profiling::puffin::set_scopes_on(true);

    let arg = ::std::env::args().nth(1);
    match arg.as_ref().map(|s| &s[..]) {
        #[cfg(feature = "winit")]
        Some("--winit") => {
            tracing::info!("Starting screen-composer with winit backend");
            std::env::set_var("SCREEN_COMPOSER_BACKEND", "winit");
            screen_composer::winit::run_winit();
        }
        #[cfg(feature = "udev")]
        Some("--tty-udev") => {
            tracing::info!("Starting screen-composer on a tty using udev");
            std::env::set_var("SCREEN_COMPOSER_BACKEND", "tty-udev");
            screen_composer::udev::run_udev();
        }
        #[cfg(feature = "udev")]
        Some("--probe") => {
            tracing::info!("Probing available displays and resolutions");
            screen_composer::udev::probe_displays();
        }
        #[cfg(feature = "x11")]
        Some("--x11") => {
            tracing::info!("Starting screen-composer with x11 backend");
            std::env::set_var("SCREEN_COMPOSER_BACKEND", "x11");
            screen_composer::x11::run_x11();
        }
        Some(other) => {
            tracing::error!("Unknown backend: {}", other);
        }
        None => {
            #[allow(clippy::disallowed_macros)]
            {
                println!("USAGE: screen-composer --backend");
                println!();
                println!("Possible backends are:");
                for b in POSSIBLE_BACKENDS {
                    println!("\t{}", b);
                }
            }
        }
    }
}
