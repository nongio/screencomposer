use smithay::reexports::wayland_server;
use wayland_server::Resource;

use crate::{state::Backend, ScreenComposer};

use smithay::reexports::wayland_server::protocol::*;

pub mod gen {
    pub use smithay::reexports::wayland_server;
    pub use smithay::reexports::wayland_server::protocol::__interfaces::*;
    pub use smithay::reexports::wayland_server::protocol::*;
    pub use smithay::reexports::wayland_server::*;

    wayland_scanner::generate_interfaces!("./protocols/sc-layer-v1.xml");
    wayland_scanner::generate_server_code!("./protocols/sc-layer-v1.xml");
}

pub use gen::sc_layer_v1::ScLayerV1 as ZscLayerV1;
pub use gen::sc_transaction_v1::ScTransactionV1;

/// Z-order configuration for sc-layer relative to parent surface content
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScLayerZOrder {
    BelowSurface,
    AboveSurface,
}

impl Default for ScLayerZOrder {
    fn default() -> Self {
        Self::AboveSurface
    }
}

/// Compositor-side layer state (pure augmentation, no wl_surface)
#[derive(Debug, Clone)]
pub struct ScLayer {
    /// The Wayland protocol object
    pub wl_layer: ZscLayerV1,

    /// The lay-rs layer backing this augmentation
    pub layer: lay_rs::prelude::Layer,

    /// Surface being augmented (any role)
    pub surface: wl_surface::WlSurface,

    /// Z-order relative to surface content
    pub z_order: ScLayerZOrder,
}

impl PartialEq for ScLayer {
    fn eq(&self, other: &Self) -> bool {
        self.wl_layer.id() == other.wl_layer.id()
    }
}

/// Transaction state for batching animated changes
pub struct ScTransaction {
    /// The protocol object
    pub wl_transaction: ScTransactionV1,

    /// Animation duration in milliseconds (None = immediate)
    pub duration_ms: Option<f32>,

    /// Animation delay in milliseconds
    pub delay_ms: Option<f32>,

    /// Timing function configured by client
    pub timing_function: Option<lay_rs::prelude::Transition>,

    /// Whether to send completion event
    pub send_completion: bool,

    /// Accumulated layer changes ready for scheduling
    pub accumulated_changes: Vec<lay_rs::engine::AnimatedNodeChange>,
}

impl Clone for ScTransaction {
    fn clone(&self) -> Self {
        Self {
            wl_transaction: self.wl_transaction.clone(),
            duration_ms: self.duration_ms,
            delay_ms: self.delay_ms,
            timing_function: self.timing_function,
            send_completion: self.send_completion,
            accumulated_changes: self.accumulated_changes.clone(),
        }
    }
}

impl std::fmt::Debug for ScTransaction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScTransaction")
            .field("duration_ms", &self.duration_ms)
            .field("delay_ms", &self.delay_ms)
            .field("send_completion", &self.send_completion)
            .field("num_changes", &self.accumulated_changes.len())
            .finish()
    }
}

/// Handler for sc_layer_shell
pub trait ScLayerShellHandler {
    /// Create a new layer surface
    fn new_layer(&mut self, layer: ScLayer);

    /// A layer was destroyed
    fn destroy_layer(&mut self, _layer: &ScLayer) {}
}

impl<BackendData: Backend> ScLayerShellHandler for ScreenComposer<BackendData> {
    fn new_layer(&mut self, mut layer: ScLayer) {
        let layer_id = layer.wl_layer.id();
        let surface_id = layer.surface.id();

        // Try to find the existing rendering layer for this surface
        // Walk up the parent chain for subsurfaces to find the actual rendered surface
        use smithay::wayland::compositor::get_parent;
        
        let mut current_surface = layer.surface.clone();
        let found = loop {
            let current_id = current_surface.id();
            
            // Check if we have a rendering layer for this surface
            if let Some(rendering_layer) = self.surface_layers.get(&current_id).cloned() {
                // Replace the empty layer with the actual rendering layer
                layer.layer = rendering_layer;
                break true;
            }
            
            // Walk up to parent surface (for subsurfaces)
            if let Some(parent) = get_parent(&current_surface) {
                current_surface = parent;
            } else {
                // No parent, surface not found in rendering layers
                break false;
            }
        };

        if !found {
            tracing::warn!(
                "No rendering layer found for surface {:?} (or its parents) for sc-layer",
                surface_id
            );
            return;
        }

        tracing::info!(
            "Attached sc-layer {:?} to rendering layer for surface {:?}, z-order: {:?}",
            layer_id,
            surface_id,
            layer.z_order
        );

        // Store in per-surface map
        self.sc_layers.entry(surface_id).or_default().push(layer);
    }

    fn destroy_layer(&mut self, layer: &ScLayer) {
        // Remove from surface's list
        let surface_id = layer.surface.id();
        if let Some(layers) = self.sc_layers.get_mut(&surface_id) {
            layers.retain(|l| l.wl_layer.id() != layer.wl_layer.id());
            if layers.is_empty() {
                self.sc_layers.remove(&surface_id);
            }
        }

        // Remove from surface_layers map (rendering layer reference)
        self.surface_layers.remove(&surface_id);

        tracing::info!("Destroyed sc-layer {:?}", layer.wl_layer.id());
    }
}
