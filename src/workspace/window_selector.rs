use layers::prelude::*;
use smithay::input::pointer::{CursorIcon, CursorImageStatus};
use std::{
    cell::RefCell,
    hash::{Hash, Hasher},
};

use crate::{config::Config, interactive_view::ViewInteractions, utils::Observer};

use super::Workspace;

// use skia_safe::document::state;
#[allow(unused)]
struct FontCache {
    font_collection: skia_safe::textlayout::FontCollection,
    font_mgr: skia_safe::FontMgr,
    type_face_font_provider: RefCell<skia_safe::textlayout::TypefaceFontProvider>,
}

// // source: slint ui
// // https://github.com/slint-ui/slint/blob/64e7bb27d12dd8f884275292c2333d37f4e224d5/internal/renderers/skia/textlayout.rs#L31
thread_local! {
    static FONT_CACHE: FontCache = {
        let font_mgr = skia_safe::FontMgr::new();
        let type_face_font_provider = skia_safe::textlayout::TypefaceFontProvider::new();
        let mut font_collection = skia_safe::textlayout::FontCollection::new();
        font_collection.set_asset_font_manager(Some(type_face_font_provider.clone().into()));
        font_collection.set_dynamic_font_manager(font_mgr.clone());
        FontCache { font_collection, font_mgr, type_face_font_provider: RefCell::new(type_face_font_provider) }
    };
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct WindowSelection {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub window_title: String,
    pub visible: bool,
    pub index: usize,
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
    pub layer: layers::prelude::Layer,
    pub view: layers::prelude::View<WindowSelectorState>,
}

impl WindowSelectorView {
    pub fn new(
        layers_engine: LayersEngine,
        _cursor_handler: std::sync::Arc<std::sync::Mutex<CursorImageStatus>>,
    ) -> Self {
        let layer = layers_engine.new_layer();
        layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            ..Default::default()
        });
        layer.set_size(layers::types::Size::percent(1.0, 1.0), None);

        layers_engine.scene_add_layer(layer.clone());

        let state = WindowSelectorState {
            rects: vec![],
            current_selection: None,
        };
        let view = layers::prelude::View::new("window_selector_view", state, view_window_selector);
        view.mount_layer(layer.clone());
        Self { view, layer }
    }
}

pub fn get_paragraph_for_text(text: &str, font_size: f32) -> skia_safe::textlayout::Paragraph {
    let mut text_style = skia_safe::textlayout::TextStyle::new();

    text_style.set_font_size(font_size);
    let font_style = skia_safe::FontStyle::new(
        skia_safe::font_style::Weight::BOLD,
        skia_safe::font_style::Width::CONDENSED,
        skia_safe::font_style::Slant::Upright,
    );
    text_style.set_font_style(font_style);
    text_style.set_letter_spacing(-1.0);
    let foreground_paint = skia_safe::Paint::new(skia_safe::Color4f::new(0.1, 0.1, 0.1, 0.9), None);
    text_style.set_foreground_paint(&foreground_paint);
    text_style.set_font_families(&["Inter"]);

    let mut paragraph_style = skia_safe::textlayout::ParagraphStyle::new();
    paragraph_style.set_text_style(&text_style);
    paragraph_style.set_max_lines(1);
    paragraph_style.set_text_align(skia_safe::textlayout::TextAlign::Center);
    paragraph_style.set_text_direction(skia_safe::textlayout::TextDirection::LTR);
    paragraph_style.set_ellipsis("…");

    let mut builder = FONT_CACHE.with(|font_cache| {
        skia_safe::textlayout::ParagraphBuilder::new(
            &paragraph_style,
            font_cache.font_collection.clone(),
        )
    });
    let paragraph = builder.add_text(text).build();

    paragraph
}

pub fn view_window_selector(
    state: &WindowSelectorState,
    _view: &View<WindowSelectorState>,
) -> LayerTree {
    let draw_scale = Config::with(|config| config.screen_scale) as f32 * 0.8;

    let font_size: f32 = 24.0 * draw_scale;
    let current = state
        .current_selection
        .map(|x| state.rects.get(x).unwrap().clone())
        .map(|window_selection| {
            let mut paragraph = get_paragraph_for_text(&window_selection.window_title, font_size);

            paragraph.layout(1000.0 * draw_scale);
            let range: std::ops::Range<usize> = 0..window_selection.window_title.len();
            let rects = paragraph.get_rects_for_range(
                range,
                skia_safe::textlayout::RectHeightStyle::Tight,
                skia_safe::textlayout::RectWidthStyle::Tight,
            );
            let text_bounding_box = rects.iter().fold(skia_safe::Rect::new_empty(), |acc, b| {
                skia_safe::Rect::join2(acc, b.rect)
            });
            (window_selection, text_bounding_box)
        });

    let window_selection = current
        .as_ref()
        .map(|(window_selection, _)| window_selection.clone());

    let draw_container = Some(move |canvas: &skia_safe::Canvas, w, h| {
        if window_selection.is_some() {
            let window_selection = window_selection.as_ref().unwrap();
            let color = skia_safe::Color4f::new(85.0 / 255.0, 150.0 / 255.0, 244.0 / 255.0, 1.0);
            let mut paint = skia_safe::Paint::new(color, None);
            paint.set_stroke(true);
            paint.set_stroke_width(10.0 * draw_scale);
            let rrect = skia_safe::RRect::new_rect_xy(
                skia_safe::Rect::from_xywh(
                    window_selection.x,
                    window_selection.y,
                    window_selection.w,
                    window_selection.h,
                ),
                15.0 * draw_scale,
                15.0 * draw_scale,
            );

            canvas.draw_rrect(rrect, &paint);
        }
        skia_safe::Rect::from_xywh(0.0, 0.0, w, h)
    });

    let text_padding_x: f32 = 10.0 * draw_scale;
    let text_padding_y: f32 = 5.0 * draw_scale;
    // let text_x = 0.0;
    // let text_y = 0.0;
    let (text_rect, text_bounding_box) = current
        .as_ref()
        .map(|(rect, bb)| (rect.clone(), *bb))
        .unwrap_or((WindowSelection::default(), skia_safe::Rect::new_empty()));
    let text_layer_size = layers::types::Size::points(
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
        .key("window_selector_view")
        .position(((0.0, 0.0).into(), None))
        .size((layers::types::Size::percent(1.0, 1.0), None))
        .border_width((10.0 * draw_scale, None))
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
            .blend_mode(layers::prelude::BlendMode::BackgroundBlur)
            .border_corner_radius((BorderRadius::new_single(10.0 * draw_scale), None))
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
            .content(Some(move |canvas: &skia_safe::Canvas, w, h| {
                let mut paragraph = get_paragraph_for_text(&text_rect.window_title, font_size);
                paragraph.layout(w);
                // let text_x = TEXT_PADDING_X;
                let text_y = text_padding_y;

                paragraph.paint(canvas, (0.0, text_y));
                let safe = 200.0 * draw_scale;
                skia_safe::Rect::from_xywh(-safe, -safe, w + safe * 2.0, h + safe * 2.0)
            }))
            .build()
            .unwrap()])
        .build()
        .unwrap()
}
impl Observer<Workspace> for WindowSelectorView {
    fn notify(&self, _event: &Workspace) {}
}
impl<Backend: crate::state::Backend> ViewInteractions<Backend> for WindowSelectorView {
    fn id(&self) -> Option<usize> {
        self.view.layer.id().map(|id| id.0.into())
    }
    fn is_alive(&self) -> bool {
        !self.view.layer.hidden()
    }
    fn on_motion(
        &self,
        _seat: &smithay::input::Seat<crate::ScreenComposer<Backend>>,
        data: &mut crate::ScreenComposer<Backend>,
        event: &smithay::input::pointer::MotionEvent,
    ) {
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

        self.view.update_state(WindowSelectorState {
            rects: state.rects,
            current_selection: rect,
        });
    }
    fn on_button(
        &self,
        _seat: &smithay::input::Seat<crate::ScreenComposer<Backend>>,
        data: &mut crate::ScreenComposer<Backend>,
        event: &smithay::input::pointer::ButtonEvent,
    ) {
        let state = self.view.get_state();
        if let Some(index) = state.current_selection {
            let window_selector_workspace_model = data.workspace.model.read();
            let window_selector_workspace_model = window_selector_workspace_model.unwrap();
            let oid = window_selector_workspace_model
                .windows_list
                .get(index)
                .unwrap()
                .clone();
            drop(window_selector_workspace_model);
            if let Some(window_view) = data.window_views.get(&oid).cloned() {
                data.raise_element(&window_view.window, true, Some(event.serial), true);
            }
            data.expose_show_all(-1.0, true);
            data.set_cursor(&CursorImageStatus::default_named());
        }
    }
}
