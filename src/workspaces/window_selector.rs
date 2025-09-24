use lay_rs::{prelude::*, skia};
use smithay::{
    input::pointer::{CursorIcon, CursorImageStatus},
    reexports::wayland_server::backend::ObjectId,
};
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::{Arc, RwLock},
};

use crate::{config::Config, interactive_view::ViewInteractions, utils::Observer};

use super::{utils::FONT_CACHE, WorkspacesModel};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct WindowSelection {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub window_title: String,
    pub visible: bool,
    pub index: usize,
    pub window_id: Option<ObjectId>,
}

#[derive(Debug, Clone)]
pub struct WindowSelectorState {
    pub rects: Vec<WindowSelection>,
    pub current_selection: Option<usize>,
}

impl Hash for WindowSelectorState {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let current = self
            .current_selection
            .as_ref()
            .map(|x| self.rects.get(*x).unwrap());
        current.hash(state);
        self.rects.hash(state);
    }
}

impl Hash for WindowSelection {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.x.to_bits().hash(state);
        self.y.to_bits().hash(state);
        self.w.to_bits().hash(state);
        self.h.to_bits().hash(state);
        self.window_title.hash(state);
        self.visible.hash(state);
    }
}

#[derive(Clone)]
pub struct HandlerFunction(pub std::sync::Arc<dyn Fn(usize) + 'static + Send + Sync>);

impl PartialEq for HandlerFunction {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0)
    }
}

impl<F: Fn(usize) + Send + Sync + 'static> From<F> for HandlerFunction {
    fn from(f: F) -> Self {
        HandlerFunction(std::sync::Arc::new(f))
    }
}

#[derive(Clone)]
pub struct WindowSelectorView {
    pub layer: lay_rs::prelude::Layer,
    pub background_layer: lay_rs::prelude::Layer,
    pub windows_layer: lay_rs::prelude::Layer,
    pub overlay_layer: lay_rs::prelude::Layer,
    pub view: lay_rs::prelude::View<WindowSelectorState>,
    pub windows: std::sync::Arc<RwLock<HashMap<ObjectId, Layer>>>,
}

/// # WindowSelectorView Layer Structure
///
/// ```diagram
/// WindowSelectorView
/// ├── layer
/// │   ├── background_layer
/// │   ├── windows_layer
/// │   ├── overlay_layer (view(view_window_selector))
/// │   │   └── window_selector_label
/// ```
///
/// - `layer`: The root layer for the window selector view.
/// - `background_layer`: a replica of the workspace background
/// - `windows_layer`: windows replica container
/// - `overlay_layer`: draw the window selection and text
/// - `window_selector_label`: text layer for the window title
///

impl WindowSelectorView {
    pub fn new(index: usize, layers_engine: Arc<Engine>, background_layer: Layer) -> Self {
        let window_selector_root = layers_engine.new_layer();
        window_selector_root.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            // flex_grow: 1.0,
            // flex_shrink: 0.0,
            // flex_basis: taffy::Dimension::Percent(1.0),
            ..Default::default()
        });

        window_selector_root.set_size(lay_rs::types::Size::percent(1.0, 1.0), None);

        window_selector_root.set_key(format!("window_selector_root_{}", index));
        layers_engine.add_layer(&window_selector_root);
        let overlay_layer = layers_engine.new_layer();
        overlay_layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            ..Default::default()
        });
        overlay_layer.set_size(lay_rs::types::Size::percent(1.0, 1.0), None);
        overlay_layer.set_pointer_events(false);

        let state = WindowSelectorState {
            rects: vec![],
            current_selection: None,
        };
        let view = lay_rs::prelude::View::new(
            format!("window_selector_view_{}", index),
            state,
            view_window_selector,
        );
        view.mount_layer(overlay_layer.clone());

        let clone_background_layer = layers_engine.new_layer();
        clone_background_layer.set_key(format!("window_selector_background_{}", index));
        clone_background_layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            ..Default::default()
        });
        clone_background_layer.set_size(lay_rs::types::Size::percent(1.0, 1.0), None);
        window_selector_root.add_sublayer(&clone_background_layer);

        clone_background_layer.set_draw_content(layers_engine.layer_as_content(&background_layer));
        let mut node = layers_engine
            .scene_get_node(clone_background_layer.id)
            .unwrap();
        let node = node.get_mut();
        node.set_follow_node(background_layer);
        let windows_layer = layers_engine.new_layer();
        windows_layer.set_key(format!("window_selector_windows_container_{}", index));
        windows_layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            ..Default::default()
        });
        windows_layer.set_size(lay_rs::types::Size::percent(1.0, 1.0), None);
        window_selector_root.add_sublayer(&windows_layer);

        window_selector_root.add_sublayer(&overlay_layer);

        Self {
            view,
            layer: window_selector_root,
            background_layer: clone_background_layer,
            windows_layer,
            overlay_layer,
            windows: std::sync::Arc::new(RwLock::new(HashMap::new())),
        }
    }
    pub fn layer_for_window(&self, window: &ObjectId) -> Option<Layer> {
        self.windows.read().unwrap().get(window).cloned()
    }

    /// add a window layer to windows map
    /// and append the window to the windows_layer
    pub fn map_window(&self, window_id: ObjectId, layer: Layer) {
        self.windows_layer.add_sublayer(&layer);
        self.windows.write().unwrap().insert(window_id, layer);
    }
    /// remove the window from the windows map
    /// and remove the layer from windows_layer
    pub fn unmap_window(&self, window_id: &ObjectId) {
        if let Some(layer) = self.windows.write().unwrap().remove(window_id) {
            layer.remove();
        }
    }
}

pub fn get_paragraph_for_text(text: &str, font_size: f32) -> skia::textlayout::Paragraph {
    let mut text_style = skia::textlayout::TextStyle::new();

    text_style.set_font_size(font_size);
    let font_style = skia::FontStyle::new(
        skia::font_style::Weight::BOLD,
        skia::font_style::Width::CONDENSED,
        skia::font_style::Slant::Upright,
    );
    text_style.set_font_style(font_style);
    text_style.set_letter_spacing(-1.0);
    let foreground_paint = skia::Paint::new(skia::Color4f::new(0.1, 0.1, 0.1, 0.9), None);
    text_style.set_foreground_paint(&foreground_paint);
    let ff = Config::with(|c| c.font_family.clone());
    text_style.set_font_families(&[ff]);

    let mut paragraph_style = skia::textlayout::ParagraphStyle::new();
    paragraph_style.set_text_style(&text_style);
    paragraph_style.set_max_lines(1);
    paragraph_style.set_text_align(skia::textlayout::TextAlign::Center);
    paragraph_style.set_text_direction(skia::textlayout::TextDirection::LTR);
    paragraph_style.set_ellipsis("…");

    let mut builder = FONT_CACHE.with(|font_cache| {
        skia::textlayout::ParagraphBuilder::new(
            &paragraph_style,
            font_cache.font_collection.clone(),
        )
    });
    let paragraph = builder.add_text(text).build();

    paragraph
}

pub fn view_window_selector(
    state: &WindowSelectorState,
    view: &View<WindowSelectorState>,
) -> LayerTree {
    let draw_scale = Config::with(|config| config.screen_scale) as f32 * 0.8;

    let font_size: f32 = 20.0 * draw_scale;
    let current = state
        .current_selection
        .map(|x| state.rects.get(x).unwrap().clone())
        .map(|window_selection| {
            let mut paragraph = get_paragraph_for_text(&window_selection.window_title, font_size);

            paragraph.layout(1000.0 * draw_scale);
            let range: std::ops::Range<usize> = 0..window_selection.window_title.len();
            let rects = paragraph.get_rects_for_range(
                range,
                skia::textlayout::RectHeightStyle::Tight,
                skia::textlayout::RectWidthStyle::Tight,
            );
            let text_bounding_box = rects.iter().fold(skia::Rect::new_empty(), |acc, b| {
                skia::Rect::join2(acc, b.rect)
            });
            (window_selection, text_bounding_box)
        });

    let window_selection = current
        .as_ref()
        .map(|(window_selection, _)| window_selection.clone());

    let draw_container = Some(move |canvas: &skia::Canvas, w, h| {
        if window_selection.is_some() {
            let window_selection = window_selection.as_ref().unwrap();
            let color = skia::Color4f::new(85.0 / 255.0, 150.0 / 255.0, 244.0 / 255.0, 1.0);
            let mut paint = skia::Paint::new(color, None);
            paint.set_stroke(true);
            paint.set_stroke_width(10.0 * draw_scale);
            let rrect = skia::RRect::new_rect_xy(
                skia::Rect::from_xywh(
                    window_selection.x,
                    window_selection.y,
                    window_selection.w,
                    window_selection.h,
                )
                .with_outset((draw_scale * 6.0, draw_scale * 6.0)),
                10.0 * draw_scale,
                10.0 * draw_scale,
            );

            canvas.draw_rrect(rrect, &paint);
        }
        skia::Rect::from_xywh(0.0, 0.0, w, h)
    });

    let text_padding_x: f32 = 10.0 * draw_scale;
    let text_padding_y: f32 = 5.0 * draw_scale;
    // let text_x = 0.0;
    // let text_y = 0.0;
    let (text_rect, text_bounding_box) = current
        .as_ref()
        .map(|(rect, bb)| (rect.clone(), *bb))
        .unwrap_or((WindowSelection::default(), skia::Rect::new_empty()));
    let text_layer_size = lay_rs::types::Size::points(
        if text_bounding_box.width() == 0.0 {
            0.0
        } else {
            text_bounding_box.width() + text_padding_x * 2.0
        },
        if text_bounding_box.height() == 0.0 {
            0.0
        } else {
            text_bounding_box.height() + text_padding_y * 2.0
        },
    );
    LayerTreeBuilder::default()
        .key(view.get_key())
        .position(((0.0, 0.0).into(), None))
        .size(lay_rs::types::Size::percent(1.0, 1.0))
        .content(draw_container)
        .children(vec![LayerTreeBuilder::default()
            .key("window_selector_label")
            .layout_style(taffy::Style {
                position: taffy::Position::Absolute,
                ..Default::default()
            })
            .position((
                (
                    text_rect.x + text_rect.w / 2.0 - text_bounding_box.width() / 2.0,
                    text_rect.y + text_rect.h / 2.0 - text_bounding_box.height() / 2.0,
                )
                    .into(),
                None,
            ))
            .size((text_layer_size, None))
            .blend_mode(lay_rs::prelude::BlendMode::BackgroundBlur)
            .border_corner_radius((BorderRadius::new_single(8.0 * draw_scale), None))
            .background_color((
                PaintColor::Solid {
                    color: Color::new_rgba(1.0, 1.0, 1.0, 0.4),
                },
                None,
            ))
            .shadow_color((Color::new_rgba(0.0, 0.0, 0.0, 0.2), None))
            .shadow_offset(((0.0, 0.0).into(), None))
            .shadow_radius((5.0, None))
            // .shadow_spread((10.0, None))
            .content(Some(move |canvas: &skia::Canvas, w, h| {
                let mut paragraph = get_paragraph_for_text(&text_rect.window_title, font_size);
                paragraph.layout(w);
                // let text_x = TEXT_PADDING_X;
                let text_y = text_padding_y;

                paragraph.paint(canvas, (0.0, text_y));
                let safe = 200.0 * draw_scale;
                skia::Rect::from_xywh(-safe, -safe, w + safe * 2.0, h + safe * 2.0)
            }))
            .build()
            .unwrap()])
        .build()
        .unwrap()
}
impl Observer<WorkspacesModel> for WindowSelectorView {
    fn notify(&self, _workspaces: &WorkspacesModel) {}
}
impl<Backend: crate::state::Backend> ViewInteractions<Backend> for WindowSelectorView {
    fn id(&self) -> Option<usize> {
        self.view
            .layer
            .read()
            .unwrap()
            .as_ref()
            .and_then(|l| Some(l.id.0.into()))
    }

    fn is_alive(&self) -> bool {
        !self
            .view
            .layer
            .read()
            .unwrap()
            .as_ref()
            .map(|l| l.hidden())
            .unwrap_or(true)
    }
    fn on_motion(
        &self,
        _seat: &smithay::input::Seat<crate::ScreenComposer<Backend>>,
        data: &mut crate::ScreenComposer<Backend>,
        event: &smithay::input::pointer::MotionEvent,
    ) {
        // println!("on_motion");
        let mut state = self.view.get_state().clone();
        let screen_scale = Config::with(|config| config.screen_scale);
        let location = event.location.to_physical(screen_scale);

        let rect = state
            .rects
            .iter()
            .find(|rect| {
                if rect.x < location.x as f32
                    && rect.x + rect.w > location.x as f32
                    && rect.y < location.y as f32
                    && rect.y + rect.h > location.y as f32
                {
                    // println!("Found rect {:?}", rect);
                    state.current_selection = Some(rect.index);
                    let cursor = CursorImageStatus::Named(CursorIcon::Pointer);
                    data.set_cursor(&cursor);
                    true
                } else {
                    let cursor = CursorImageStatus::Named(CursorIcon::default());
                    data.set_cursor(&cursor);
                    false
                }
            })
            .map(|x| x.index);

        self.view.update_state(&WindowSelectorState {
            rects: state.rects,
            current_selection: rect,
        });
    }
    fn on_button(
        &self,
        _seat: &smithay::input::Seat<crate::ScreenComposer<Backend>>,
        screencomposer: &mut crate::ScreenComposer<Backend>,
        _event: &smithay::input::pointer::ButtonEvent,
    ) {
        let selector_state = self.view.get_state();
        if let Some(index) = selector_state.current_selection {
            let wid = selector_state
                .rects
                .get(index)
                .unwrap()
                .window_id
                .clone()
                .unwrap();

            screencomposer.workspaces.focus_app_with_window(&wid);
            screencomposer.set_keyboard_focus_on_surface(&wid);
        }
        screencomposer.workspaces.expose_show_all(-1.0, true);
        screencomposer.set_cursor(&CursorImageStatus::default_named());
    }
}
