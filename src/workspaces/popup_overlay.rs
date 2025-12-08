use lay_rs::{
    engine::Engine,
    prelude::{taffy, Layer, View},
    types::Point,
    view::RenderLayerTree,
};
use smithay::reexports::wayland_server::backend::ObjectId;
use std::{collections::HashMap, sync::Arc};

use crate::workspaces::{utils::view_render_elements, WindowViewSurface};

/// A popup with its layer and root window reference
pub struct PopupLayer {
    pub popup_id: ObjectId,
    pub root_window_id: ObjectId,
    pub layer: Layer,
    pub content_layer: Layer,
    pub view_content: View<Vec<WindowViewSurface>>,
}

/// View for rendering popups on top of all windows
///
/// Popups (menus, dropdowns, tooltips) need to be rendered above all windows
/// to prevent clipping when they extend beyond their parent window bounds.
pub struct PopupOverlayView {
    pub layer: Layer,
    layers_engine: Arc<Engine>,
    /// Map from popup surface ID to its layer
    popup_layers: HashMap<ObjectId, PopupLayer>,
}

impl PopupOverlayView {
    pub fn new(layers_engine: Arc<Engine>) -> Self {
        let layer = layers_engine.new_layer();
        layer.set_key("popup_overlay");
        layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            size: taffy::Size {
                width: taffy::Dimension::Percent(1.0),
                height: taffy::Dimension::Percent(1.0),
            },
            ..Default::default()
        });
        layer.set_pointer_events(false);

        layers_engine.add_layer(&layer);

        Self {
            layer,
            layers_engine,
            popup_layers: HashMap::new(),
        }
    }

    /// Get or create a popup layer for the given popup surface
    pub fn get_or_create_popup_layer(
        &mut self,
        popup_id: ObjectId,
        root_window_id: ObjectId,
    ) -> &mut PopupLayer {
        self.popup_layers
            .entry(popup_id.clone())
            .or_insert_with(|| {
                let layer = self.layers_engine.new_layer();
                layer.set_key(format!("popup_{:?}", popup_id));
                layer.set_layout_style(taffy::Style {
                    position: taffy::Position::Absolute,
                    ..Default::default()
                });
                layer.set_pointer_events(false);

                let content_layer = self.layers_engine.new_layer();
                content_layer.set_layout_style(taffy::Style {
                    position: taffy::Position::Absolute,
                    ..Default::default()
                });
                content_layer.set_pointer_events(false);

                self.layers_engine.append_layer(&layer, self.layer.id());
                self.layers_engine.append_layer(&content_layer, layer.id());

                let view_content = View::new(
                    format!("popup_content_{:?}", popup_id),
                    Vec::new(),
                    view_render_elements,
                );
                view_content.mount_layer(content_layer.clone());

                PopupLayer {
                    popup_id,
                    root_window_id,
                    layer,
                    content_layer,
                    view_content,
                }
            })
    }

    /// Update popup position and surfaces
    pub fn update_popup(
        &mut self,
        popup_id: &ObjectId,
        root_window_id: &ObjectId,
        position: Point,
        surfaces: Vec<WindowViewSurface>,
    ) {
        let popup = self.get_or_create_popup_layer(popup_id.clone(), root_window_id.clone());
        popup.layer.set_position(position, None);
        popup.view_content.update_state(&surfaces);
    }

    /// Remove a popup layer
    pub fn remove_popup(&mut self, popup_id: &ObjectId) {
        if let Some(popup) = self.popup_layers.remove(popup_id) {
            popup.layer.remove();
        }
    }

    /// Remove all popups belonging to a specific root window
    pub fn remove_popups_for_window(&mut self, root_window_id: &ObjectId) {
        let to_remove: Vec<ObjectId> = self
            .popup_layers
            .iter()
            .filter(|(_, popup)| &popup.root_window_id == root_window_id)
            .map(|(id, _)| id.clone())
            .collect();

        for id in to_remove {
            self.remove_popup(&id);
        }
    }

    /// Clear all popup layers
    pub fn clear(&mut self) {
        for (_, popup) in self.popup_layers.drain() {
            popup.layer.remove();
        }
    }

    /// Get a popup layer by ID
    pub fn get_popup(&self, popup_id: &ObjectId) -> Option<&PopupLayer> {
        self.popup_layers.get(popup_id)
    }

    /// Show or hide the popup overlay layer
    pub fn set_hidden(&self, hidden: bool) {
        self.layer.set_hidden(hidden);
    }
}
