//! Interface layer towards ScreenComposer's backend-facing APIs.
//!
//! This module owns the D-Bus bindings we use to talk to ScreenComposer.
//! Each backend API should live in its own submodule (e.g. Screencast,
//! RemoteDesktop). For now only the ScreenCast API is implemented.
//! See `ScreenCast-backend-spec.md` for the contract this module targets.

use zbus::{Connection, Result};

/// Client wrapper for ScreenComposer's D-Bus APIs.
pub struct ScreenComposerClient {
    pub(crate) connection: Connection,
}

impl ScreenComposerClient {
    /// Creates a new client using the given D-Bus connection.
    pub async fn new(connection: Connection) -> Result<Self> {
        Ok(Self { connection })
    }
}

pub mod screencast;
