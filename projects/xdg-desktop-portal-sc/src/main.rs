mod portal;

use anyhow::Result;
use tokio::signal;
use tracing::info;
use tracing_subscriber::EnvFilter;
use zbus::ConnectionBuilder;

use crate::portal::{desktop_path, ScreenCastPortal};

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let connection = ConnectionBuilder::session()?
        .name("org.freedesktop.portal.Desktop")?
        .build()
        .await?;

    let portal = ScreenCastPortal::new(connection.clone());
    connection
        .object_server()
        .at(desktop_path(), portal)
        .await?;

    println!("ScreenComposer portal running");
    info!("Registered ScreenCast portal on session bus");

    signal::ctrl_c().await?;
    info!("Shutdown requested");

    Ok(())
}

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(env_filter).init();
}
