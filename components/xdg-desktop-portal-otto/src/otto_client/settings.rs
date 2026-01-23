//! Client proxy for `org.otto.Settings`.
//!
//! This module speaks to `org.otto.Settings` (the backend interface
//! exposed by the Otto compositor).

use zbus::{proxy, Result};

/// D-Bus proxy for `org.otto.Settings` service.
#[proxy(
    interface = "org.otto.Settings",
    default_service = "org.otto.Settings",
    default_path = "/org/otto/Settings"
)]
trait OttoSettings {
    /// Get the color scheme preference from the compositor.
    ///
    /// Returns:
    /// - 0: No preference
    /// - 1: Prefer dark appearance
    /// - 2: Prefer light appearance
    async fn get_color_scheme(&self) -> Result<u32>;
}
