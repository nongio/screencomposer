use std::{
    cell::RefCell, collections::{HashMap, HashSet}, sync::{atomic::AtomicBool, Arc, RwLock}, time::Duration
};

use layers::{
    engine::{
        animation::{timing::TimingFunction, Transition},
        LayersEngine, NodeRef,
    },
    prelude::{taffy, Color, Layer},
    taffy::{prelude::FromLength, style::Style, LengthPercentageAuto},
    types::{BlendMode, PaintColor, Size},
    view::{BuildLayerTree, LayerTreeBuilder},
};
use smithay::{backend::input::ButtonState, utils::IsAlive};
use tokio::sync::mpsc;

use crate::{
    config::Config,
    interactive_view::ViewInteractions,
    utils::Observer,
    workspace::{Application, Window, Workspace, WorkspaceModel},
};

use super::{model::DockModel, render::draw_app_icon, render_app::draw_balloon_rect};

#[derive(Debug, Clone)]
pub struct DockView {
    layers_engine: LayersEngine,
    // layers
    pub wrap_layer: layers::prelude::Layer,
    pub view_layer: layers::prelude::Layer,
    bar_layer: layers::prelude::Layer,
    dock_apps_container: layers::prelude::Layer,
    dock_windows_container: layers::prelude::Layer,

    app_layers: Arc<RwLock<HashMap<String, (Layer, Layer)>>>,
    // pub view: layers::prelude::View<DockModel>,
    state: Arc<RwLock<DockModel>>,
    active: Arc<AtomicBool>,
    notify_tx: tokio::sync::mpsc::Sender<WorkspaceModel>,
    latest_event: Arc<tokio::sync::RwLock<Option<WorkspaceModel>>>,
    magnification_position: Arc<RwLock<f32>>,
}
impl PartialEq for DockView {
    fn eq(&self, other: &Self) -> bool {
        self.wrap_layer == other.wrap_layer
    }
}
impl IsAlive for DockView {
    fn alive(&self) -> bool {
        self.active.load(std::sync::atomic::Ordering::Relaxed)
    }
}
#[allow(dead_code)]
struct FontCache {
    font_collection: skia_safe::textlayout::FontCollection,
    pub font_mgr: skia_safe::FontMgr,
    type_face_font_provider: RefCell<skia_safe::textlayout::TypefaceFontProvider>,
}
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

impl DockView {
    pub fn new(layers_engine: LayersEngine) -> Self {
        let wrap_layer = layers_engine.new_layer();
        wrap_layer.set_key("dock-wrapper");
        wrap_layer.set_pointer_events(false);
        wrap_layer.set_size(Size::percent(1.0, 1.0), None);
        wrap_layer.set_layout_style(Style {
            position: layers::taffy::style::Position::Absolute,
            display: layers::taffy::style::Display::Flex,
            justify_content: Some(taffy::JustifyContent::Center), // horizontal
            align_items: Some(taffy::AlignItems::FlexEnd),        // vertical alignment
            justify_items: Some(taffy::JustifyItems::Center),
            ..Default::default()
        });

        layers_engine.scene_add_layer(wrap_layer.clone());

        let view_layer = layers_engine.new_layer();

        wrap_layer.add_sublayer(view_layer.clone());
        // FIXME
        view_layer.set_position((0.0, 1000.0), None);

        

        let view_tree = LayerTreeBuilder::default()
        .key("dock-view")
        .layout_style(taffy::Style {
            position: taffy::Position::Relative,
            display: taffy::Display::Flex,
            flex_direction: taffy::FlexDirection::Row,
            justify_content: Some(taffy::JustifyContent::Center),
            align_items: Some(taffy::AlignItems::Center),
            justify_items: Some(taffy::JustifyItems::Center),
            gap: taffy::Size::<taffy::LengthPercentage>::from_length(10.0),
            ..Default::default()
        })
        .build()
        .unwrap();
        
        view_layer.build_layer_tree(&view_tree, &mut HashMap::new());


        let bar_layer = layers_engine.new_layer();
        view_layer.add_sublayer(bar_layer.clone());

        let bar_tree = LayerTreeBuilder::default()
            .key("dock-bar")
            .pointer_events(false)
            // .position(((0.0, 0.0).into(), None))
            .blend_mode(BlendMode::BackgroundBlur)
            .background_color(PaintColor::Solid {
                color:  Color::new_rgba(0.94, 0.94, 0.94, 0.44),
            })
            .border_width((2.5, None))
            .border_color(Color::new_rgba(1.0, 1.0, 1.0, 0.3))
            .shadow_color(Color::new_rgba(0.0, 0.0, 0.0, 0.2))
            .shadow_offset(((0.0, -5.0).into(), None))
            .shadow_radius((20.0, None))
            .layout_style(taffy::Style {
                position: taffy::Position::Absolute,
                ..Default::default()
            })
            .build()
            .unwrap();

        bar_layer.build_layer_tree(&bar_tree, &mut HashMap::new());

        let dock_apps_container = layers_engine.new_layer();
        view_layer.add_sublayer(dock_apps_container.clone());

        let container_tree = LayerTreeBuilder::default()
            .key("dock_app_container")
            .pointer_events(false)
            .position(((0.0, 0.0).into(), None))
            .size((
                Size {
                    width: taffy::Dimension::Auto,
                    height: taffy::Dimension::Auto,
                },
                None,
            ))
            .layout_style(taffy::Style {
                // position: taffy::Position::Absolute,
                display: taffy::Display::Flex,
                justify_content: Some(taffy::JustifyContent::FlexEnd),
                justify_items: Some(taffy::JustifyItems::FlexEnd),
                align_items: Some(taffy::AlignItems::Baseline),
                gap: taffy::Size::<taffy::LengthPercentage>::from_length(0.0),
                ..Default::default()
            })

            .build()
            .unwrap();
        dock_apps_container.build_layer_tree(&container_tree, &mut HashMap::new());

        let dock_windows_container = layers_engine.new_layer();
        view_layer.add_sublayer(dock_windows_container.clone());

        let container_tree = LayerTreeBuilder::default()
            .key("dock_windows_container")
            .pointer_events(false)
            .position(((0.0, 0.0).into(), None))
            .size((
                Size {
                    width: taffy::Dimension::Auto,
                    height: taffy::Dimension::Auto,
                },
                None,
            ))
            .layout_style(taffy::Style {
                // position: taffy::Position::Absolute,
                display: taffy::Display::Flex,
                justify_content: Some(taffy::JustifyContent::FlexEnd),
                justify_items: Some(taffy::JustifyItems::FlexEnd),
                align_items: Some(taffy::AlignItems::Center),
                gap: taffy::Size::<taffy::LengthPercentage>::from_length(10.0),
                ..Default::default()
            })

            .build()
            .unwrap();
        dock_windows_container.build_layer_tree(&container_tree, &mut HashMap::new());
        
        let mut initial_state = DockModel::new();
        initial_state.width = 1000;
        
        let (notify_tx, notify_rx) = mpsc::channel(5);
        let dock = Self {
            layers_engine,

            wrap_layer,
            view_layer,
            bar_layer,
            dock_apps_container,
            dock_windows_container,
            app_layers: Arc::new(RwLock::new(HashMap::new())),
            state: Arc::new(RwLock::new(initial_state)),
            active: Arc::new(AtomicBool::new(true)),
            notify_tx,
            latest_event: Arc::new(tokio::sync::RwLock::new(None)),
            magnification_position: Arc::new(RwLock::new(-500.0)),
        };
        dock.update_dock();
        dock.init_notification_handler(notify_rx);

        dock
    }
    pub fn update_state(&self, state: &DockModel) {
        {
            *self.state.write().unwrap() = state.clone();
        }
        self.update_dock();
    }
    pub fn get_state(&self) -> DockModel {
        self.state.read().unwrap().clone()
    }
    pub fn hide(&self) {
        self.active
            .store(false, std::sync::atomic::Ordering::Relaxed);
        self.wrap_layer.set_opacity(
            0.0,
            Some(Transition {
                duration: 0.4,
                delay: 0.0,
                timing: TimingFunction::default(),
            }),
        );
    }
    fn get_app_layers(&self) -> Vec<Layer> {
        let app_layers = self.app_layers.read().unwrap();
        app_layers.values().cloned().map(|(layer, _)| layer).collect()
    }
    fn layers_for_state(&self, available_icon_width: f32) {
        let state = self.get_state();
        let mut current_app_layers = self.get_app_layers();
        let mut layers_map = self.app_layers.write().unwrap();
        for app in state.running_apps {
            // println!("layer for dock app: {:?}", app.identifier);
            let app_copy = app.clone();
            let app_name=  app.clone().desktop_name.unwrap_or(app.identifier.clone());
            let (layer, label) = layers_map.entry(app.identifier.clone())
                .or_insert_with(||{
                    let new_layer= self.layers_engine.new_layer();
                    self.setup_app_icon(&new_layer, app.clone(), available_icon_width);
                    self.dock_apps_container.add_sublayer(new_layer.clone());
                    let label_layer = self.layers_engine.new_layer();

                    new_layer.add_sublayer(label_layer.clone());
                    self.setup_app_label(&label_layer, app_name);
                    (new_layer, label_layer)
                });
            let draw_picture = draw_app_icon(&app_copy, false);
            layer.set_draw_content(Some(draw_picture));

            let app_copy = app.clone();
            layer.add_on_pointer_press(move |layer: Layer, _, _| {
                let draw_picture = draw_app_icon(&app_copy, true);
                layer.set_draw_content(Some(draw_picture));
            });
            let app_copy = app.clone();
            layer.add_on_pointer_release(move |layer: Layer, _, _| {
                let draw_picture = draw_app_icon(&app_copy, false);
                layer.set_draw_content(Some(draw_picture));
            });
            let lab = label.clone();
            layer.add_on_pointer_in(move |_, _, _| {
                lab.set_opacity(
                    1.0,
                    Some(Transition {
                        duration: 0.1,
                        ..Default::default()
                    }),
                );
            });
            let lab = label.clone();
            layer.add_on_pointer_out(move |_, _, _| {
                lab.set_opacity(
                    0.0,
                    Some(Transition {
                        duration: 0.1,
                        ..Default::default()
                    }),
                );
            });
            current_app_layers.retain(|l| l.id() != layer.id());
            
        }

        // remove the layers not used
        for layer in current_app_layers {
            self.layers_engine.scene_remove_layer(layer.id());
            layers_map.retain(|_k,(v, _)| {
                v.id() != layer.id()
            });
        }

        // for win in state.minimized_windows {
        //     // println!("layer for dock app: {:?}", app.identifier);
            
            
            
        // }
    }

    fn setup_app_icon(&self, layer: &Layer, application: Application, icon_width: f32) {
        let app_name = application
        .desktop_name
        .clone()
        .unwrap_or(application.identifier.clone());
        
        let draw_picture = draw_app_icon(&application, false);
        
        let icon_tree= LayerTreeBuilder::default()
            .key(app_name)
            .layout_style(taffy::Style {
                display: taffy::Display::Block,
                position: taffy::Position::Relative,
                overflow: taffy::geometry::Point {
                    x: taffy::style::Overflow::Visible,
                    y: taffy::style::Overflow::Visible,
                },
                ..Default::default()
            })
            .size((
                Size {
                    width: taffy::Dimension::Length(icon_width),
                    height: taffy::Dimension::Length(icon_width + 30.0),
                },
                Some(Transition {
                    duration: 0.2,
                    ..Default::default()
                }), // None
            ))
            .background_color(PaintColor::Solid {
                color: Color::new_rgba(1.0, 0.0, 0.0, 0.0),
            })
            .content(Some(draw_picture))
            .build()
            .unwrap();
        layer.build_layer_tree(&icon_tree, &mut HashMap::new());        
    }
    fn setup_app_label(&self, new_layer: &Layer, app_name: String) {
        let text_size = 26.0;

        let typeface = FONT_CACHE
        .with(|font_cache| {
            font_cache
                .font_mgr
                .match_family_style("Inter", skia_safe::FontStyle::default())
        })
        .unwrap();
        let font = skia_safe::Font::from_typeface_with_params(typeface, text_size, 1.0, 0.0);

        let text = app_name.clone();
        let paint = skia_safe::Paint::default();
        let text_bounds = font.measure_str(text, Some(&paint));

        let text_bounds = text_bounds.1;
        let arrow_height = 20.0;
        let text_padding_h = 30.0;
        let text_padding_v = 14.0;
        let safe_margin = 100.0;
        let label_size_width = text_bounds.width() + text_padding_h * 2.0 + safe_margin * 2.0;
        let label_size_height =
            text_bounds.height() + arrow_height + text_padding_v * 2.0 + safe_margin * 2.0;
    
        let draw_label = move |canvas: &skia_safe::Canvas, w: f32, h: f32| -> skia_safe::Rect {
            // Tooltip parameters
            // let text = "This is a tooltip!";
            let text = app_name.clone();
            let rect_corner_radius = 10.0;
            let arrow_width = 25.0;
            let arrow_corner_radius = 3.0;
    
            // Paint for the tooltip background
            let mut paint = skia_safe::Paint::default();
            paint.set_color(skia_safe::Color::from_argb(230, 255, 255, 255)); // Light gray
            paint.set_anti_alias(true);
    
            // Calculate tooltip dimensions
            let tooltip_width = w - safe_margin * 2.0;
            let tooltip_height = h - safe_margin * 2.0;
    
            let arrow_path = draw_balloon_rect(
                safe_margin,
                safe_margin,
                tooltip_width,
                tooltip_height,
                rect_corner_radius,
                arrow_width,
                arrow_height,
                0.5,
                arrow_corner_radius,
            );
            let shadow_color = skia_safe::Color::from_argb(80, 0, 0, 0); // semi-transparent black
            let mut shadow_paint = skia_safe::Paint::default();
            shadow_paint.set_color(shadow_color);
            shadow_paint.set_anti_alias(true);
            shadow_paint.set_mask_filter(skia_safe::MaskFilter::blur(
                skia_safe::BlurStyle::Normal,
                10.0,
                None,
            ));
    
            let mut shadow_path = arrow_path.clone();
            shadow_path.offset((-0.0, -0.0));
            canvas.draw_path(&shadow_path, &shadow_paint);
    
            // // Draw the arrow path (under the rectangle)
            canvas.draw_path(&arrow_path, &paint);
    
            // // Paint for the text
            let mut text_paint = skia_safe::Paint::default();
            text_paint.set_color(skia_safe::Color::BLACK);
            text_paint.set_anti_alias(true);
    
            // // Draw the text inside the tooltip
            let text_x = safe_margin + text_padding_h;
            let text_y = text_bounds.height() + text_padding_v + safe_margin - text_size * 0.2;
            canvas.draw_str(text.as_str(), (text_x, text_y), &font, &text_paint);
            skia_safe::Rect::from_xywh(0.0, 0.0, w, h)
        };
        let label_tree = LayerTreeBuilder::default()
        .key(format!("{}_label", new_layer.key()))
        .layout_style(taffy::Style {
            position: taffy::Position::Relative,
            max_size: taffy::geometry::Size {
                width: taffy::style::Dimension::Length(label_size_width),
                height: taffy::style::Dimension::Length(label_size_height),
            },
            inset: taffy::geometry::Rect::<LengthPercentageAuto> {
                top: LengthPercentageAuto::Auto,
                right: LengthPercentageAuto::Auto,
                bottom: LengthPercentageAuto::Auto,
                left: LengthPercentageAuto::Percent(0.5),
            },
            ..Default::default()
        })
        .size(Size {
            width: taffy::Dimension::Length(label_size_width),
            height: taffy::Dimension::Length(label_size_height),
        })
        .position(layers::prelude::Point {
            x: -label_size_width / 2.0,
            y: -label_size_height - 10.0 + safe_margin,
        })
        .opacity((0.0, None))
        .pointer_events(false)
        .content(Some(draw_label))
        .build()
        .unwrap();

        new_layer.build_layer_tree(&label_tree, &mut HashMap::new());
    }
    fn update_dock(&self) {

        let state = self.get_state();
        let draw_scale = Config::with(|config| config.screen_scale) as f32 * 0.8;
        // those are constant like values
        let available_width = state.width as f32 - 20.0 * draw_scale;
        let ICON_SIZE: f32 = 100.0 * draw_scale;

        let apps_len = state.running_apps.len() as f32;
        let windows_len = state.minimized_windows.len() as f32;

        let mut component_padding_h: f32 = ICON_SIZE * 0.09 * draw_scale;
        if component_padding_h > 5.0 * draw_scale {
            component_padding_h = 5.0 * draw_scale;
        }
        let mut component_padding_v: f32 = ICON_SIZE * 0.09 * draw_scale;
        if component_padding_v > 50.0 * draw_scale {
            component_padding_v = 50.0 * draw_scale;
        }
        let available_icon_size =
            (available_width - component_padding_h * 2.0) / (apps_len + windows_len);
        let available_icon_size = ICON_SIZE.min(available_icon_size);

        let component_width = (apps_len + windows_len) * available_icon_size + component_padding_h * 2.0;
        let component_height = available_icon_size + component_padding_v * 2.0;

       self.view_layer.set_size(
            Size {
                width: taffy::Dimension::Length(component_width * 2.0),
                height: taffy::Dimension::Length(component_height + 20.0),
            },
            Some(Transition {
                duration: 1.0,
                ..Default::default()
            }),
        );

        self.bar_layer.set_size(Size {
            width: taffy::Dimension::Length(component_width),
            height: taffy::Dimension::Length(component_height),
        },
        Some(Transition {
            duration: 0.5,
            ..Default::default()
        }));
        self.bar_layer.set_border_corner_radius(ICON_SIZE / 4.0, None);

        self.layers_for_state(available_icon_size);
        self.magnify_elements();
        
    }
    fn init_notification_handler(&self, mut rx: tokio::sync::mpsc::Receiver<WorkspaceModel>) {
        // let view = self.view.clone();
        let latest_event = self.latest_event.clone();
        // Task to receive events
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                // Store the latest event
                *latest_event.write().await = Some(event.clone());
            }
        });
        let latest_event = self.latest_event.clone();
        let dock = self.clone();
        tokio::spawn(async move {
            loop {
                // app switcher updates don't need to be instantanious
                tokio::time::sleep(Duration::from_secs_f32(0.4)).await;

                let event = {
                    let mut latest_event_lock = latest_event.write().await;
                    latest_event_lock.take()
                };

                if let Some(workspace) = event {
                    let mut app_set = HashSet::new();
                    let apps: Vec<Application> = workspace
                        .application_list
                        .iter()
                        .rev()
                        .filter_map(|app_id| {
                            let app = workspace.applications_cache.get(app_id).unwrap().to_owned();

                            if app_set.insert(app.identifier.clone()) {
                                Some(app)
                            } else {
                                None
                            }
                        })
                        .collect();

                    let minimized_windows: Vec<_> = workspace.minimized_windows
                        .iter()
                        .filter_map(|(id, _)| workspace.windows_cache.get(id).cloned())
                        .collect();

                    let state = dock.get_state();

                    dock.update_state(&DockModel {
                        running_apps: apps,
                        minimized_windows,
                        ..state
                    });
                }
            }
        });
    }
    pub fn magnify_elements(&self) {
        let pos = self.magnification_position.read().unwrap().clone();
        let bounds = self.view_layer.render_bounds();
        let focus = pos / bounds.width();
        let state = self.get_state();

        let draw_scale = Config::with(|config| config.screen_scale) as f32 * 0.8;
        let icon_size: f32 = 100.0 * draw_scale;

        let apps_len = state.running_apps.len() as f32;
        let windows_len = state.minimized_windows.len() as f32;

        let mut component_padding_h: f32 = icon_size * 0.09 * draw_scale;
        if component_padding_h > 5.0 * draw_scale {
            component_padding_h = 5.0 * draw_scale;
        }
        let mut component_padding_v: f32 = icon_size * 0.09 * draw_scale;
        if component_padding_v > 50.0 * draw_scale {
            component_padding_v = 50.0 * draw_scale;
        }
        let mut total_width = component_padding_h * 2.0;
        let tot_elements = apps_len + windows_len;
        for (index, app) in state.running_apps.iter().enumerate() {
            let id = &app.identifier;
            let layers_map = self.app_layers.read().unwrap();
            if let Some((layer, _)) = layers_map.get(id) {
                let icon_pos = 1.0 / tot_elements * index as f32 + 1.0 / (tot_elements * 2.0);
                let icon_focus = 1.0 + magnify_function(focus - icon_pos) * 0.2;
                // println!("x: {} -> {}", icon_pos, icon_focus);
                let focused_icon_size = icon_size * icon_focus as f32;
                
                layer.set_size(
                    Size::points(focused_icon_size, focused_icon_size + 30.0),
                    Some(Transition {
                        duration: 0.1,
                        ..Default::default()
                    }),
                );
                total_width += focused_icon_size;
            }
        }
        // TODO windows magnify
        for (index, win) in state.minimized_windows.iter().enumerate() {
            let index = index + state.running_apps.len();
            let icon_pos = 1.0 / tot_elements * index as f32 + 1.0 / (tot_elements * 2.0);
            let icon_focus = 1.0 + magnify_function(focus - icon_pos) * 0.2;
            // println!("x: {} -> {}", icon_pos, icon_focus);
            let focused_icon_size = icon_size * icon_focus as f32;

            win.base_layer.set_size(
                Size::points(focused_icon_size, focused_icon_size + 30.0),
                Some(Transition {
                    duration: 0.1,
                    ..Default::default()
                }),
            );
            total_width += focused_icon_size;
        }
        self.bar_layer.set_size(
            Size::points(total_width, component_padding_v * 2.0 + icon_size),
            Some(Transition {
                duration: 0.1,
                ..Default::default()
            }),
        );
    }
    pub fn update_magnification_position(&self, pos: f32) {
        *self.magnification_position.write().unwrap()= pos;
        self.magnify_elements();
    }

    pub fn app_for_layer(&self, layer: &NodeRef) -> Option<String> {
        let layers_map = self.app_layers.read().unwrap();
        layers_map.iter().find(|(_, (app_layer, _))| app_layer.id() == Some(*layer))
            .map(|(key, _)| key.clone())
    }

    pub fn window_for_layer(&self, layer: &NodeRef) -> Option<Window> {
        let state = self.get_state();
        state.minimized_windows.iter().find_map(|win| {
            let win = win.clone();
            if win.base_layer.id() == Some(*layer) {
                Some(win)
            } else {
                None
            }
        })
    }
    pub fn minimize_window(&self, window: &Window, workspace: &Workspace) {

        
        let width = 160.0;
        let scale = width / window.w;
        let height = window.h * scale;
        let id = window.id().unwrap();
        if let Some(view) = workspace.get_window_view(&id) {
            
            view.content_layer.set_anchor_point((1.0, 1.0), None);
            view.shadow_layer.set_anchor_point((1.0, 1.0), None);
            let animation_id = view.content_layer.set_scale((scale, scale), Some(Transition::default()));
            view.shadow_layer.set_scale((scale, scale), Some(Transition::default()));
            window.base_layer.set_size(Size::points(width, height), None);
            let style = window.base_layer.node_layout_style();
            window.base_layer.set_layout_style(taffy::Style {
                position: taffy::Position::Relative,
                ..style
            });
            window.base_layer.set_pointer_events(true);
            let dock_windows_container = self.dock_windows_container.clone();
            let base_layer_ref = window.base_layer.clone();
            let engine_ref = self.layers_engine.clone();
            self.layers_engine.on_finish(animation_id, move|_| {
                
                let transaction = base_layer_ref.set_position(
                    layers::types::Point {
                        x: 0.0,
                        y: 0.0,
                    },
                    Some(Transition::default()),
                );
                let engine_ref = engine_ref.clone();
                let dock_windows_container = dock_windows_container.clone();
                let base_layer_ref = base_layer_ref.clone();
                engine_ref.on_finish(transaction, move |_| {
                    dock_windows_container.add_sublayer(base_layer_ref.clone());
                });
            });
        
        }

        
        // let dock = self.clone();
        // let window_ref = window.clone();
        // window.base_layer.add_on_pointer_release(move |_, _, _| {
        //     // dock.unminimize_window(window_ref);
        //     // DockView::unminimize_window(&dock, window_ref.clone());
        // });
        
    }
    pub fn unminimize_window(&self, window: Window, workspace: &Workspace) {
        println!("unminimize window");
        window.base_layer.set_position(
            layers::types::Point {
                x: window.x,
                y: window.y,
            },
            Some(Transition::default()),
        );

        let id = window.id().unwrap();
        let scale = 1.0;
        if let Some(view) = workspace.get_window_view(&id) {
            view.content_layer.set_scale((scale, scale), Some(Transition::default()));
            view.shadow_layer.set_scale((scale, scale), Some(Transition::default()));
        }
        window.base_layer.set_size(Size::points(0.0, 0.0), None);
        let style = window.base_layer.node_layout_style();
        window.base_layer.set_layout_style(taffy::Style {
            position: taffy::Position::Absolute,
            ..style
        });
        window.base_layer.set_pointer_events(false);
        window.base_layer.remove_all_handlers();
    }

}

impl Observer<WorkspaceModel> for DockView {
    fn notify(&self, event: &WorkspaceModel) {
        let _ = self.notify_tx.try_send(event.clone());
    }
}

impl<Backend: crate::state::Backend> ViewInteractions<Backend> for DockView {
    fn id(&self) -> Option<usize> {
        self.wrap_layer.id().map(|id| id.0.into())
    }
    fn is_alive(&self) -> bool {
        self.alive()
    }
    fn on_motion(
        &self,
        _seat: &smithay::input::Seat<crate::ScreenComposer<Backend>>,
        data: &mut crate::ScreenComposer<Backend>,
        event: &smithay::input::pointer::MotionEvent,
    ) {
        // let _id = self.view_layer.id().unwrap();
        let scale = Config::with(|c| c.screen_scale);
        if let Some(layer_id) = data.layers_engine.current_hover() {
            // println!("dock hover: {:?}", layer_id);
        }
        self.update_magnification_position(
            (event.location.x * scale) as f32 - self.view_layer.render_position().x,
        );
    }
    fn on_leave(&self, _serial: smithay::utils::Serial, _time: u32) {
        self.update_magnification_position(-500.0);
    }
    fn on_button(
        &self,
        _seat: &smithay::input::Seat<crate::ScreenComposer<Backend>>,
        data: &mut crate::ScreenComposer<Backend>,
        event: &smithay::input::pointer::ButtonEvent,
    ) {

        match event.state {
            ButtonState::Pressed => {
                // println!("dock Button pressed");
            }
            ButtonState::Released => {
                if let Some(layer_id) = data.layers_engine.current_hover() {

                    if let Some(identifier) = self.app_for_layer(&layer_id) {
                        data.raise_app_elements(&identifier, true, Some(event.serial));
                    } else if let Some(window) = self.window_for_layer(&layer_id) {
                        data.unminimize_window(&window.window_element.unwrap().clone());
                    }
                }
            }
        }
    }
}
use std::f64::consts::E;

pub fn magnify_function(x: impl Into<f64>) -> f64 {
    let x = x.into();
    E.powf(-10.0 * (x).powi(2))
}
