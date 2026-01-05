use smithay::{
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::wayland_server::DisplayHandle,
    utils::Transform,
};

use crate::config::{Config, VirtualScreenConfig};
use crate::state::Backend;
use crate::ScreenComposer;

/// Creates virtual outputs from configuration
pub fn create_virtual_outputs<BackendData: Backend + 'static>(
    display: &DisplayHandle,
) -> Vec<Output> {
    let configs = Config::with(|c| c.virtual_screens.clone());
    
    configs
        .iter()
        .filter(|config| config.enabled)
        .map(|config| create_virtual_output::<BackendData>(display, config))
        .collect()
}

/// Creates a single virtual output from configuration
fn create_virtual_output<BackendData: Backend + 'static>(
    display: &DisplayHandle,
    config: &VirtualScreenConfig,
) -> Output {
    let mode = Mode {
        size: (config.width as i32, config.height as i32).into(),
        refresh: (config.refresh_rate * 1000) as i32, // Convert Hz to mHz
    };

    let output = Output::new(
        config.name.clone(),
        PhysicalProperties {
            size: (0, 0).into(), // Virtual outputs have no physical size
            subpixel: Subpixel::Unknown,
            make: "ScreenComposer".into(),
            model: "Virtual".into(),
        },
    );

    // Create the global so clients can see this output
    let _global = output.create_global::<ScreenComposer<BackendData>>(display);

    // Set the output mode and properties
    output.change_current_state(
        Some(mode),
        Some(Transform::Normal),
        Some(smithay::output::Scale::Fractional(config.scale)),
        Some((0, 0).into()),
    );
    output.set_preferred(mode);

    tracing::info!(
        "Created virtual output '{}' {}x{}@{}Hz (scale: {})",
        config.name,
        config.width,
        config.height,
        config.refresh_rate,
        config.scale
    );

    output
}
