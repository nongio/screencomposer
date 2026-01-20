//! Watchdog to monitor compositor health via D-Bus pings.
//!
//! Sends periodic pings to the compositor and terminates it if it becomes unresponsive.

use anyhow::{Context, Result};
use std::process::Command;
use std::time::Duration;
use tokio::time::{interval, sleep, timeout};
use tracing::{debug, error, info, warn};
use zbus::Connection;

/// D-Bus interface for the compositor's health monitoring.
#[zbus::proxy(
    interface = "org.screencomposer.Compositor",
    default_service = "org.screencomposer.Compositor",
    default_path = "/org/screencomposer/Compositor"
)]
trait Compositor {
    /// Ping the compositor to check if it's responsive.
    async fn ping(&self) -> zbus::Result<String>;
}

/// Watchdog configuration.
#[derive(Debug, Clone)]
pub struct WatchdogConfig {
    /// Delay before starting watchdog monitoring.
    pub startup_delay: Duration,
    /// Interval between ping attempts.
    pub ping_interval: Duration,
    /// Timeout for ping responses.
    pub ping_timeout: Duration,
    /// Number of consecutive failures before killing compositor.
    pub max_failures: u32,
    /// Maximum time to wait for compositor D-Bus service to appear.
    pub service_wait_timeout: Duration,
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            startup_delay: Duration::from_secs(5),
            ping_interval: Duration::from_secs(5),
            ping_timeout: Duration::from_secs(1),
            max_failures: 3,
            service_wait_timeout: Duration::from_secs(30),
        }
    }
}

/// Watchdog that monitors the compositor's health.
pub struct Watchdog {
    config: WatchdogConfig,
    connection: Connection,
}

impl Watchdog {
    /// Create a new watchdog instance.
    pub fn new(connection: Connection, config: WatchdogConfig) -> Self {
        Self { config, connection }
    }

    /// Run the watchdog loop.
    ///
    /// This function will run indefinitely, sending pings to the compositor
    /// at regular intervals. If a ping times out, it will kill the compositor.
    pub async fn run(self) -> Result<()> {
        info!(
            startup_delay_secs = self.config.startup_delay.as_secs(),
            interval_secs = self.config.ping_interval.as_secs(),
            timeout_secs = self.config.ping_timeout.as_secs(),
            max_failures = self.config.max_failures,
            "Starting compositor watchdog"
        );

        // Wait for compositor to fully initialize
        sleep(self.config.startup_delay).await;
        
        // Wait for the compositor D-Bus service to be available
        info!("Waiting for compositor D-Bus service to be available...");
        let proxy = self.wait_for_service().await?;
        info!("Compositor D-Bus service is available, beginning health checks");

        let mut ticker = interval(self.config.ping_interval);
        let mut consecutive_failures = 0u32;

        loop {
            ticker.tick().await;

            debug!("Sending ping to compositor");

            match timeout(self.config.ping_timeout, proxy.ping()).await {
                Ok(Ok(response)) => {
                    debug!("Received response: {}", response);
                    consecutive_failures = 0; // Reset counter on success
                }
                Ok(Err(e)) => {
                    consecutive_failures += 1;
                    error!(
                        consecutive_failures,
                        max_failures = self.config.max_failures,
                        "Ping failed with D-Bus error: {}", e
                    );
                    
                    if consecutive_failures >= self.config.max_failures {
                        warn!("Compositor appears unresponsive after {} failures, terminating...", consecutive_failures);
                        Self::kill_compositor()?;
                        return Ok(());
                    }
                }
                Err(_) => {
                    consecutive_failures += 1;
                    error!(
                        consecutive_failures,
                        max_failures = self.config.max_failures,
                        timeout_ms = self.config.ping_timeout.as_millis(),
                        "Ping timeout exceeded"
                    );
                    
                    if consecutive_failures >= self.config.max_failures {
                        warn!("Compositor not responding after {} timeouts, terminating...", consecutive_failures);
                        Self::kill_compositor()?;
                        return Ok(());
                    }
                }
            }
        }
    }

    /// Wait for the compositor D-Bus service to be available.
    async fn wait_for_service(&self) -> Result<CompositorProxy<'_>> {
        let start = tokio::time::Instant::now();
        let mut retry_interval = interval(Duration::from_millis(500));

        loop {
            retry_interval.tick().await;

            match CompositorProxy::new(&self.connection).await {
                Ok(proxy) => {
                    // Try a test ping to ensure the service is really available
                    match timeout(Duration::from_secs(2), proxy.ping()).await {
                        Ok(Ok(_)) => {
                            info!("Compositor service responded successfully");
                            return Ok(proxy);
                        }
                        Ok(Err(e)) => {
                            debug!("Service exists but ping failed: {}", e);
                        }
                        Err(_) => {
                            debug!("Service exists but ping timed out");
                        }
                    }
                }
                Err(e) => {
                    debug!("Waiting for compositor service: {}", e);
                }
            }

            if start.elapsed() > self.config.service_wait_timeout {
                return Err(anyhow::anyhow!(
                    "Compositor D-Bus service did not appear within {} seconds",
                    self.config.service_wait_timeout.as_secs()
                ));
            }
        }
    }

    /// Kill the compositor process.
    fn kill_compositor() -> Result<()> {
        info!("Attempting to kill compositor");

        // Try to find and kill the compositor process by name
        let output = Command::new("pkill")
            .arg("-9")
            .arg("screen-composer")
            .output()
            .context("Failed to execute pkill")?;

        if output.status.success() {
            info!("Successfully terminated compositor");
        } else {
            warn!(
                "pkill returned non-zero status: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }

        Ok(())
    }
}
