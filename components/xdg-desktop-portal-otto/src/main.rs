//! XDG Desktop Portal backend for ScreenComposer.
//!
//! This binary implements the `org.freedesktop.impl.portal.ScreenCast` D-Bus
//! interface, enabling screen sharing through the standard portal API.

use anyhow::Result;
use tokio::signal;
use tracing::info;
use tracing_subscriber::EnvFilter;
use zbus::ConnectionBuilder;

use xdg_desktop_portal_otto::portal::{desktop_path, ScreenCastPortal};
use xdg_desktop_portal_otto::screencomposer_client::ScreenComposerClient;
use xdg_desktop_portal_otto::watchdog::{Watchdog, WatchdogConfig};

/// Well-known D-Bus name for the ScreenComposer portal backend.
const DBUS_NAME: &str = "org.freedesktop.impl.portal.desktop.screencomposer";

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let connection = ConnectionBuilder::session()?
        .name(DBUS_NAME)?
        .build()
        .await?;

    let sc_client = ScreenComposerClient::new(connection.clone()).await?;
    info!("Connected to D-Bus session bus");

    let portal = ScreenCastPortal::new(sc_client);
    connection
        .object_server()
        .at(desktop_path(), portal)
        .await?;

    info!(name = DBUS_NAME, "ScreenCast portal backend running");

    // Start the watchdog in a separate task
    let watchdog = Watchdog::new(connection.clone(), WatchdogConfig::default());
    let watchdog_handle = tokio::spawn(async move {
        if let Err(e) = watchdog.run().await {
            tracing::error!("Watchdog error: {}", e);
        }
    });

    // Wait for shutdown signal
    tokio::select! {
        _ = signal::ctrl_c() => {
            info!("Shutdown requested");
        }
        _ = watchdog_handle => {
            info!("Watchdog task terminated");
        }
    }

    Ok(())
}

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(env_filter).init();
}
