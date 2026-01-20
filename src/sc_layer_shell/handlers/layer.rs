use lay_rs::types::BorderRadius;
use wayland_backend::server::ClientId;
use wayland_server::{Client, DataInit, Dispatch, DisplayHandle, Resource};

use crate::{ScreenComposer, sc_layer_shell::handlers::{ScLayerUserData, accumulate_change, find_active_transaction_for_client, trigger_window_update, wl_fixed_to_f32}, state::Backend};

use super::super::protocol::{
    gen::{
        sc_layer_v1::{self, ScLayerV1},

    },
    ScLayerShellHandler,
};

impl<BackendData: Backend> Dispatch<ScLayerV1, ScLayerUserData> for ScreenComposer<BackendData> {
    fn request(
        state: &mut Self,
        _client: &Client,
        layer_obj: &ScLayerV1,
        request: sc_layer_v1::Request,
        _data: &ScLayerUserData,
        _dhandle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        let layer_id = layer_obj.id();

        // Find the sc_layer in any parent's list
        let sc_layer = state
            .sc_layers
            .values()
            .flat_map(|layers| layers.iter())
            .find(|layer| layer.wl_layer.id() == layer_id);

        let Some(sc_layer) = sc_layer else {
            tracing::warn!("Layer {:?} not found in state", layer_id);
            return;
        };

        // Find active transaction for this client (if any)
        let active_transaction = find_active_transaction_for_client(state, _client);

        match request {
            sc_layer_v1::Request::SetPosition { x, y } => {
                let x = wl_fixed_to_f32(x);
                let y = wl_fixed_to_f32(y);

                if let Some(txn_id) = active_transaction {
                    // Accumulate change in transaction
                    let change = sc_layer
                        .layer
                        .change_position(lay_rs::types::Point { x, y });
                    accumulate_change(state, txn_id, change);
                } else {
                    // Apply immediately
                    sc_layer.layer.set_position((x, y), None);
                    trigger_window_update(state, &sc_layer.surface.id());
                }
            }

            sc_layer_v1::Request::SetSize { width, height } => {
                let width = wl_fixed_to_f32(width);
                let height = wl_fixed_to_f32(height);

                if let Some(txn_id) = active_transaction {
                    let change = sc_layer
                        .layer
                        .change_size(lay_rs::types::Size::points(width, height));
                    accumulate_change(state, txn_id, change);
                } else {
                    sc_layer
                        .layer
                        .set_size(lay_rs::types::Size::points(width, height), None);
                    trigger_window_update(state, &sc_layer.surface.id());
                }
            }

            sc_layer_v1::Request::SetOpacity { opacity } => {
                let opacity = wl_fixed_to_f32(opacity).clamp(0.0, 1.0);

                if let Some(txn_id) = active_transaction {
                    let change = sc_layer.layer.change_opacity(opacity);
                    accumulate_change(state, txn_id, change);
                    tracing::debug!("Accumulated opacity change: {} in transaction", opacity);
                } else {
                    sc_layer.layer.set_opacity(opacity, None);
                    trigger_window_update(state, &sc_layer.surface.id());
                    tracing::debug!("Applied opacity immediately: {}", opacity);
                }
            }

            sc_layer_v1::Request::SetBackgroundColor {
                red,
                green,
                blue,
                alpha,
            } => {
                let red = wl_fixed_to_f32(red);
                let green = wl_fixed_to_f32(green);
                let blue = wl_fixed_to_f32(blue);
                let alpha = wl_fixed_to_f32(alpha);

                if let Some(txn_id) = active_transaction {
                    let color = lay_rs::types::Color::new_rgba(red, green, blue, alpha);
                    let change = sc_layer.layer.change_background_color(color);
                    accumulate_change(state, txn_id, change);
                } else {
                    let color = lay_rs::types::Color::new_rgba(red, green, blue, alpha);
                    sc_layer.layer.set_background_color(color, None);
                    trigger_window_update(state, &sc_layer.surface.id());
                }
            }

            sc_layer_v1::Request::SetCornerRadius { radius } => {
                let radius = wl_fixed_to_f32(radius);

                if let Some(txn_id) = active_transaction {
                    let change = sc_layer.layer.change_border_corner_radius(radius);
                    accumulate_change(state, txn_id, change);
                } else {
                    sc_layer.layer.set_border_corner_radius(BorderRadius::new_single(radius), None);
                    // trigger_window_update(state, &sc_layer.surface.id());
                }
            }

            sc_layer_v1::Request::SetBorder {
                width,
                red,
                green,
                blue,
                alpha,
            } => {
                let width = wl_fixed_to_f32(width);
                let red = wl_fixed_to_f32(red);
                let green = wl_fixed_to_f32(green);
                let blue = wl_fixed_to_f32(blue);
                let alpha = wl_fixed_to_f32(alpha);

                let color = lay_rs::types::Color::new_rgba(red, green, blue, alpha);

                if let Some(txn_id) = active_transaction {
                    // Create both changes before accumulating
                    let layer = sc_layer.layer.clone();
                    let width_change = layer.change_border_width(width);
                    let color_change = layer.change_border_color(color);
                    
                    // Accumulate both changes
                    accumulate_change(state, txn_id.clone(), width_change);
                    accumulate_change(state, txn_id, color_change);
                } else {
                    // Apply immediately
                    sc_layer.layer.set_border_width(width, None);
                    sc_layer.layer.set_border_color(color, None);
                    trigger_window_update(state, &sc_layer.surface.id());
                }
            }


            sc_layer_v1::Request::SetShadow {
                opacity,
                radius,
                offset_x,
                offset_y,
                red,
                green,
                blue,
            } => {
                let opacity = wl_fixed_to_f32(opacity);
                let radius = wl_fixed_to_f32(radius);
                let offset_x = wl_fixed_to_f32(offset_x);
                let offset_y = wl_fixed_to_f32(offset_y);
                let red = wl_fixed_to_f32(red);
                let green = wl_fixed_to_f32(green);
                let blue = wl_fixed_to_f32(blue);

                // Shadow properties in lay-rs
                sc_layer.layer.set_shadow_color(
                    lay_rs::prelude::Color::new_rgba255(
                        (red * 255.0) as u8,
                        (green * 255.0) as u8,
                        (blue * 255.0) as u8,
                        (opacity * 255.0) as u8,
                    ),
                    None,
                );
                sc_layer.layer.set_shadow_radius(radius, None);
                sc_layer.layer.set_shadow_offset((offset_x, offset_y), None);

                trigger_window_update(state, &sc_layer.surface.id());
            }

            sc_layer_v1::Request::SetHidden { hidden } => {
                let hidden = hidden != 0;

                // Hidden doesn't animate, always apply immediately
                sc_layer.layer.set_hidden(hidden);
                trigger_window_update(state, &sc_layer.surface.id());
            }

            sc_layer_v1::Request::SetMasksToBounds { masks } => {
                let masks_to_bounds = masks != 0;

                sc_layer.layer.set_clip_content(masks_to_bounds, None);
            }

            sc_layer_v1::Request::SetBlendMode { mode } => {
                use super::super::protocol::gen::sc_layer_v1::BlendMode;
                use lay_rs::types::BlendMode as LayrsBlendMode;

                let blend_mode = match mode.into_result().ok() {
                    Some(BlendMode::Normal) => LayrsBlendMode::default(),
                    Some(BlendMode::BackgroundBlur) => LayrsBlendMode::BackgroundBlur,
                    _ => {
                        tracing::warn!("Invalid blend_mode value: {:?}", mode);
                        return;
                    }
                };

                // Blend mode doesn't animate, always apply immediately
                sc_layer.layer.set_blend_mode(blend_mode);
                trigger_window_update(state, &sc_layer.surface.id());
            }

            sc_layer_v1::Request::SetZOrder { z_order } => {
                use super::super::protocol::gen::sc_layer_v1::ZOrder;
                use crate::sc_layer_shell::ScLayerZOrder;

                // Update z-order configuration
                let new_z_order = match z_order.into_result().ok() {
                    Some(ZOrder::BelowSurface) => ScLayerZOrder::BelowSurface,
                    Some(ZOrder::AboveSurface) => ScLayerZOrder::AboveSurface,
                    _ => {
                        tracing::warn!("Invalid z_order value: {:?}", z_order);
                        return;
                    }
                };

                // Find window and reattach layer
                let surface_id = sc_layer.surface.id();
                if let Some(window) = state
                    .workspaces
                    .get_window_for_surface(&surface_id)
                    .cloned()
                {
                    // TODO: lay-rs doesn't support remove_sublayer yet
                    // For now we just add it again (this may cause duplication)
                    // window.layer().remove_sublayer(&sc_layer.layer);

                    // Reattach based on new z-order
                    // TODO: lay-rs doesn't support insert_sublayer_at yet
                    // For now we can only add to the top
                    match new_z_order {
                        ScLayerZOrder::BelowSurface => {
                            window.layer().add_sublayer(&sc_layer.layer);
                        }
                        ScLayerZOrder::AboveSurface => {
                            window.layer().add_sublayer(&sc_layer.layer);
                        }
                    }

                    // Update stored z-order
                    if let Some(layers) = state.sc_layers.get_mut(&surface_id) {
                        if let Some(layer) = layers.iter_mut().find(|l| l.wl_layer.id() == layer_id)
                        {
                            layer.z_order = new_z_order;
                        }
                    }

                    tracing::debug!("Updated sc-layer z-order to {:?}", new_z_order);
                }
            }

            sc_layer_v1::Request::Destroy => {
                // Handled by destructor
            }

            _ => {
                tracing::warn!("Unimplemented sc_layer request: {:?}", request);
            }
        }
    }

    fn destroyed(
        state: &mut Self,
        _client: ClientId,
        resource: &ScLayerV1,
        _data: &ScLayerUserData,
    ) {
        let layer_id = resource.id();

        // Find and remove the sc_layer from the appropriate parent's list
        let sc_layer = state
            .sc_layers
            .values()
            .flat_map(|layers| layers.iter())
            .find(|layer| layer.wl_layer.id() == layer_id)
            .cloned();

        if let Some(sc_layer) = sc_layer {
            ScLayerShellHandler::destroy_layer(state, &sc_layer);
        }
    }
}