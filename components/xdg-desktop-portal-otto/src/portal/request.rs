//! D-Bus Request object for async portal responses.

use tracing::info;
use zbus::fdo;
use zbus::interface;
use zbus::zvariant::OwnedObjectPath;

/// Represents a pending portal request.
///
/// The frontend creates these objects to receive async responses.
#[derive(Clone)]
pub struct Request {
    path: OwnedObjectPath,
}

impl Request {
    /// Creates a new request with the given D-Bus object path.
    pub fn new(path: OwnedObjectPath) -> Self {
        Self { path }
    }
}

#[interface(name = "org.freedesktop.impl.portal.Request")]
impl Request {
    /// Called by the frontend to cancel the request.
    async fn close(&self) -> fdo::Result<()> {
        info!(request = %self.path, "Request.Close called");
        Ok(())
    }
}
