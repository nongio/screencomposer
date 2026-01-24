use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};

use crate::{state::Backend, Otto};
use layers::prelude::Transition;

use super::protocol::{
    gen::sc_layer_shell_v1::{self, ScLayerShellV1},
    ScLayer, ScLayerShellHandler,
};

/// User data for sc_layer
pub struct ScLayerUserData {
    pub layer_id: smithay::reexports::wayland_server::backend::ObjectId,
}

// Helper to convert wl_fixed to f32 (protocol now sends f64)
fn wl_fixed_to_f32(fixed: f64) -> f32 {
    fixed as f32
}

// Helper to find active transaction for a client
fn find_active_transaction_for_client<BackendData: Backend>(
    state: &Otto<BackendData>,
    client: &Client,
) -> Option<smithay::reexports::wayland_server::backend::ObjectId> {
    state
        .sc_transactions
        .iter()
        .find(|(_, txn)| txn.wl_transaction.client().map(|c| c.id()) == Some(client.id()))
        .map(|(id, _)| id.clone())
}

// Helper to accumulate a layer change in a transaction
fn accumulate_change<BackendData: Backend>(
    state: &mut Otto<BackendData>,
    txn_id: smithay::reexports::wayland_server::backend::ObjectId,
    change: layers::engine::AnimatedNodeChange,
) {
    if let Some(txn) = state.sc_transactions.get_mut(&txn_id) {
        txn.accumulated_changes.push(change);
    }
}

// Helper to trigger window redraw after layer property change
fn trigger_window_update<BackendData: Backend>(
    state: &mut Otto<BackendData>,
    surface_id: &smithay::reexports::wayland_server::backend::ObjectId,
) {
    if let Some(window) = state.workspaces.get_window_for_surface(surface_id).cloned() {
        state.update_window_view(&window);
    }
}

// Helper to commit a transaction and apply all accumulated changes
fn commit_transaction<BackendData: Backend>(
    state: &mut Otto<BackendData>,
    txn_id: smithay::reexports::wayland_server::backend::ObjectId,
) {
    let Some(txn) = state.sc_transactions.remove(&txn_id) else {
        return;
    };

    tracing::debug!(
        "Committing transaction with {} changes, duration_ms: {:?}",
        txn.accumulated_changes.len(),
        txn.duration_ms
    );

    // Use client-configured timing function, or create default from duration
    let transition = txn.timing_function.or_else(|| {
        txn.duration_ms.map(|duration_ms| {
            let duration_secs = duration_ms / 1000.0;
            tracing::debug!(
                "Creating transition with duration: {} seconds",
                duration_secs
            );
            Transition::ease_out_quad(duration_secs)
        })
    });

    // Schedule all accumulated changes together
    if !txn.accumulated_changes.is_empty() {
        if let Some(ref trans) = transition {
            // Create animation and start all changes together
            let animation = state
                .layers_engine
                .add_animation_from_transition(trans, false);
            state
                .layers_engine
                .schedule_changes(&txn.accumulated_changes, animation);
            state.layers_engine.start_animation(animation, 0.0);
            tracing::debug!("Animation started with {:?}", animation);
        } else {
            tracing::debug!("No transition - changes were already applied immediately");
        }
        // If no transition, changes were already applied immediately via set_* methods
    } else {
        tracing::warn!("Transaction committed with no accumulated changes!");
    }

    // Send completion event if requested
    if txn.send_completion {
        txn.wl_transaction.completed();
    }
}

pub mod layer;
pub mod transactions;

/// Create the sc_layer_shell global
pub fn create_layer_shell_global<BackendData: Backend + 'static>(
    display: &DisplayHandle,
) -> smithay::reexports::wayland_server::backend::GlobalId {
    display.create_global::<Otto<BackendData>, ScLayerShellV1, _>(1, ())
}

impl<BackendData: Backend> GlobalDispatch<ScLayerShellV1, ()> for Otto<BackendData> {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ScLayerShellV1>,
        _global_data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl<BackendData: Backend> Dispatch<ScLayerShellV1, ()> for Otto<BackendData> {
    fn request(
        state: &mut Self,
        _client: &Client,
        shell: &ScLayerShellV1,
        request: sc_layer_shell_v1::Request,
        _data: &(),
        _dhandle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            sc_layer_shell_v1::Request::GetLayer { id, surface } => {
                // Per protocol spec: "It can augment any surface type"
                // We just verify the surface is alive and valid
                if !surface.is_alive() {
                    shell.post_error(
                        sc_layer_shell_v1::Error::InvalidSurface,
                        "Surface does not exist",
                    );
                    return;
                }

                // Create lay-rs layer
                let layer = state.layers_engine.new_layer();

                // Set some defaults
                layer.set_layout_style(layers::taffy::Style {
                    position: layers::taffy::Position::Absolute,
                    ..Default::default()
                });

                // Initialize the wayland object - we'll use a placeholder ID for now
                let wl_layer = data_init.init(
                    id,
                    ScLayerUserData {
                        layer_id: surface.id(), // Temporary placeholder, will be overwritten
                    },
                );

                // Now get the actual layer ID and set it properly
                let layer_id = wl_layer.id();
                let layer_id_str = format!("sc_layer_{:?}", layer_id);
                layer.set_key(layer_id_str);

                // Create compositor state
                let sc_layer = ScLayer {
                    wl_layer: wl_layer.clone(),
                    layer: layer.clone(),
                    surface: surface.clone(),
                    z_order: crate::sc_layer_shell::ScLayerZOrder::default(),
                };

                // Notify handler
                ScLayerShellHandler::new_layer(state, sc_layer);
            }

            sc_layer_shell_v1::Request::BeginTransaction { id } => {
                use super::protocol::ScTransaction;

                let wl_transaction = data_init.init(id, ());
                let transaction = ScTransaction {
                    wl_transaction: wl_transaction.clone(),
                    duration_ms: None,
                    delay_ms: None,
                    timing_function: None,
                    send_completion: false,
                    accumulated_changes: Vec::new(),
                };

                state
                    .sc_transactions
                    .insert(wl_transaction.id(), transaction);
            }

            sc_layer_shell_v1::Request::Destroy => {
                // Nothing to do
            }

            _ => {
                tracing::warn!("Unimplemented sc_layer_shell request: {:?}", request);
            }
        }
    }
}
