use std::{
    collections::{HashMap, HashSet}, sync::{atomic::AtomicBool, Arc, RwLock}, time::Duration
};

use layers::{
    engine::{
        animation::Transition,
        LayersEngine, NodeRef, TransactionRef,
    }, 
    prelude::{taffy, Color, Layer, Point}, 
    skia, 
    taffy::{prelude::FromLength, style::Style}, types::{BlendMode, PaintColor, Size}, view::{BuildLayerTree, LayerTreeBuilder}
};
use smithay::utils::IsAlive;
use tokio::sync::mpsc;

use crate::{
    config::Config,
    utils::Observer,
    workspace::{Application, Window, WorkspaceModel},
};

use super::{model::DockModel, render::{draw_app_icon, setup_app_icon, setup_label, setup_miniwindow_icon}};

#[derive(Debug, Clone)]
pub struct DockView {
    layers_engine: LayersEngine,
    // layers
    pub wrap_layer: layers::prelude::Layer,
    pub view_layer: layers::prelude::Layer,
    bar_layer: layers::prelude::Layer,
    pub resize_handle: layers::prelude::Layer,
    dock_apps_container: layers::prelude::Layer,
    dock_windows_container: layers::prelude::Layer,

    app_layers: Arc<RwLock<HashMap<String, (Layer, Layer, Layer)>>>,
    miniwindow_layers: Arc<RwLock<HashMap<Window, (Layer, Layer, Layer)>>>,
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

impl DockView {
    pub fn new(layers_engine: LayersEngine) -> Self {
        let draw_scale = Config::with(|config| config.screen_scale) as f32 * 0.8;
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
        .size(Size::auto()) 
        .layout_style(taffy::Style {
            position: taffy::Position::Relative,
            display: taffy::Display::Flex,
            flex_direction: taffy::FlexDirection::Row,
            justify_content: Some(taffy::JustifyContent::Center),
            justify_items: Some(taffy::JustifyItems::Center),
            align_items: Some(taffy::AlignItems::FlexEnd),
            gap: taffy::Size::<taffy::LengthPercentage>::from_length(0.0),
            padding: taffy::Rect {
                top: taffy::length(20.0),
                bottom: taffy::length(20.0),
                right: taffy::length(10.0),
                left: taffy::length(10.0),
            },
            ..Default::default()
        })
        .build()
        .unwrap();
        
        view_layer.build_layer_tree(&view_tree);


        let bar_layer = layers_engine.new_layer();
        view_layer.add_sublayer(bar_layer.clone());
        const DOCK_BAR_HEIGHT: f32 = 100.0;
        let bar_tree = LayerTreeBuilder::default()
            .key("dock-bar")
            .pointer_events(false)
            .size(Size{
                width: taffy::percent(1.0),
                height: taffy::Dimension::Length(DOCK_BAR_HEIGHT * draw_scale),
            })
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

        bar_layer.build_layer_tree(&bar_tree);

        let dock_apps_container = layers_engine.new_layer();
        view_layer.add_sublayer(dock_apps_container.clone());

        let container_tree = LayerTreeBuilder::default()
            .key("dock_app_container")
            .pointer_events(false)
            .size(Size::auto())
            .layout_style(taffy::Style {
                display: taffy::Display::Flex,
                justify_content: Some(taffy::JustifyContent::FlexEnd),
                justify_items: Some(taffy::JustifyItems::FlexEnd),
                align_items: Some(taffy::AlignItems::Baseline),
                gap: taffy::Size::<taffy::LengthPercentage>::from_length(0.0),
                ..Default::default()
            })

            .build()
            .unwrap();
        dock_apps_container.build_layer_tree(&container_tree);

        let dock_handle = layers_engine.new_layer();
        view_layer.add_sublayer(dock_handle.clone());

        let draw_scale = Config::with(|config| config.screen_scale) as f32 * 0.8;
        let handle_tree = LayerTreeBuilder::default()
            .key("dock_handle")
            .pointer_events(false)
            .size(Size {
                width: taffy::Dimension::Length(25.0 * draw_scale),
                height: taffy::Dimension::Percent(DOCK_BAR_HEIGHT * draw_scale),
            })
            // .background_color(Color::new_rgba(0.0, 0.0, 0.0, 0.0     ))
            .content(Some(move |canvas: &skia::Canvas, w, h| {
                let mut paint = layers::skia::Paint::default();
                paint.set_color(layers::skia::Color::from_argb(70, 0, 0, 0));
                let line_width: f32 = 3.0 * draw_scale;
                let margin_h = (w - line_width) / 2.0;
                let margin_v = 15.0 * draw_scale;
                let rect= layers::skia::Rect::from_xywh(margin_h, margin_v, w-2.0*margin_h, h - 2.0*margin_v);
                let rrect = layers::skia::RRect::new_rect_xy(rect, 3.0, 3.0);
                canvas.draw_rrect(rrect, &paint);
                skia::Rect::from_xywh(0.0, 0.0, w, h)
            }))
            .build()
            .unwrap();
        dock_handle.build_layer_tree(&handle_tree);

        let dock_windows_container = layers_engine.new_layer();
        view_layer.add_sublayer(dock_windows_container.clone());

        let container_tree = LayerTreeBuilder::default()
            .key("dock_windows_container")
            .pointer_events(false)
            .position(Point::new(0.0, 0.0))
            .size(Size {
                width: taffy::Dimension::Auto,
                height: taffy::Dimension::Percent(1.0),
            })
            .layout_style(taffy::Style {
                display: taffy::Display::Flex,
                justify_content: Some(taffy::JustifyContent::FlexEnd),
                justify_items: Some(taffy::JustifyItems::FlexEnd),
                align_items: Some(taffy::AlignItems::Center),
                ..Default::default()
            })

            .build()                
            .unwrap();
        dock_windows_container.build_layer_tree(&container_tree);
        
        let mut initial_state = DockModel::new();
        initial_state.width = 1000;
        
        let (notify_tx, notify_rx) = mpsc::channel(5);
        let dock = Self {
            layers_engine,

            wrap_layer,
            view_layer,
            bar_layer,
            resize_handle: dock_handle,
            dock_apps_container,
            dock_windows_container,
            app_layers: Arc::new(RwLock::new(HashMap::new())),
            miniwindow_layers: Arc::new(RwLock::new(HashMap::new())),
            state: Arc::new(RwLock::new(initial_state)),
            active: Arc::new(AtomicBool::new(true)),
            notify_tx,
            latest_event: Arc::new(tokio::sync::RwLock::new(None)),
            magnification_position: Arc::new(RwLock::new(-500.0)),
        };
        dock.render_dock();
        dock.notification_handler(notify_rx);

        dock
    }
    
    pub fn update_state(&self, state: &DockModel) {
        {
            *self.state.write().unwrap() = state.clone();
        }
        self.render_dock();
    }
    pub fn get_state(&self) -> DockModel {
        self.state.read().unwrap().clone()
    }
    
    pub fn hide(&self) -> TransactionRef {
        self.active
            .store(false, std::sync::atomic::Ordering::Relaxed);
        self.wrap_layer.set_opacity(
            0.0,
            Some(Transition::ease_out_quad(0.4)),
        )
    }
    
    fn render_elements_layers(&self, available_icon_width: f32) {
        let draw_scale= Config::with(|config| config.screen_scale) as f32 * 0.8;
        let state = self.get_state();
        let app_height = available_icon_width + 30.0;
        let miniwindow_height = available_icon_width + 60.0;
        let bar_height = app_height;
        
        self.bar_layer.set_border_corner_radius(available_icon_width / 4.0, None);
        
        self.resize_handle.set_size(Size{
            width: taffy::length(25.0 * draw_scale),
            height: taffy::Dimension::Length(bar_height),
        }, None);

        self.bar_layer.set_size(Size{
            width: taffy::percent(1.0),
            height: taffy::Dimension::Length(bar_height),
        }, None);

        let mut previous_app_layers = self.get_app_layers();
        let mut apps_layers_map = self.app_layers.write().unwrap();
        for app in state.running_apps {
            let app_copy = app.clone();
            let app_name=  app.clone().desktop_name.unwrap_or(app.identifier.clone());
            let (layer, icon, label) = apps_layers_map.entry(app.identifier.clone())
                .or_insert_with(||{
                    let new_layer= self.layers_engine.new_layer();
                    let icon_layer= self.layers_engine.new_layer();
                    setup_app_icon(&new_layer, &icon_layer, app.clone(), available_icon_width);
                    icon_layer.set_image_cache(true);

                    self.dock_apps_container.add_sublayer(new_layer.clone());
                    let label_layer = self.layers_engine.new_layer();
                    
                    setup_label(&label_layer, app_name);
                    new_layer.add_sublayer(icon_layer.clone());
                    new_layer.add_sublayer(label_layer.clone());
                    (new_layer, icon_layer, label_layer)
                });
            
            let draw_picture = draw_app_icon(&app_copy);
            icon.set_draw_content(draw_picture);


            
            let darken_color = skia::Color::from_argb(100, 100, 100, 100);
            let add = skia::Color::from_argb(0, 0, 0, 0);
            let filter = skia::color_filters::lighting(darken_color, add);
            
            let icon_ref = icon.clone();
            layer.remove_all_pointer_handlers();

            layer.add_on_pointer_press(move |_, _, _| {
                icon_ref.set_color_filter(filter.clone());
            });

            let icon_ref = icon.clone();
            layer.add_on_pointer_release(move |_, _, _| {
                icon_ref.set_color_filter(None);
            });
            let label_ref = label.clone();
            layer.add_on_pointer_in(move |_, _, _| {
                label_ref.set_opacity(
                    1.0,
                    Some(Transition {
                        duration: 0.1,
                        ..Default::default()
                    }),
                );
            });
            let label_ref = label.clone();
            let icon_ref = icon.clone();
            layer.add_on_pointer_out(move |_, _, _| {
                label_ref.set_opacity(
                    0.0,
                    Some(Transition {
                        duration: 0.1,
                        ..Default::default()
                    }),
                );
                icon_ref.set_color_filter(None);
            });
            previous_app_layers.retain(|l| l.id() != layer.id());
        }

        let mut previous_miniwindows = self.get_miniwin_layers();
        let mut miniwindows_layers_map = self.miniwindow_layers.write().unwrap();
        {
            for win in state.minimized_windows {
                let (layer, _, label) = miniwindows_layers_map.entry(win.clone())
                    .or_insert_with(||{
                        let new_layer= self.layers_engine.new_layer();                    
                        let inner_layer= self.layers_engine.new_layer();
                        let label_layer = self.layers_engine.new_layer();

                        self.dock_windows_container.add_sublayer(new_layer.clone());
                        
                        
                        setup_miniwindow_icon(&new_layer, &inner_layer, available_icon_width);
                        
                        setup_label(&label_layer, win.title.clone());
                        new_layer.add_sublayer(label_layer.clone());
                        
                        (new_layer, inner_layer, label_layer)
                    });

                    layer.remove_all_pointer_handlers();

                    let darken_color = skia::Color::from_argb(100, 100, 100, 100);
                    let add = skia::Color::from_argb(0, 0, 0, 0);
                    let filter = skia::color_filters::lighting(darken_color, add);
                                    
                    layer.remove_all_pointer_handlers();

                    layer.add_on_pointer_press(move |l: Layer, _:f32, _:f32| {
                        l.children().iter().for_each(|child| {
                            child.set_color_filter(filter.clone());
                        });
                    });
                    // let inner_ref = inner.clone();
                    layer.add_on_pointer_release(move |l: Layer, _:f32, _:f32| {
                        l.children().iter().for_each(|child| {
                            child.set_color_filter(None);
                        });
                    });
        
                    let label_ref = label.clone();
                    layer.add_on_pointer_in(move |_, _, _| {
                        label_ref.set_opacity(
                            1.0,
                            Some(Transition {
                                duration: 0.1,
                                ..Default::default()
                            }),
                        );
                    });
                    let label_ref = label.clone();

                    layer.add_on_pointer_out(move |l: Layer, _:f32, _:f32| {
                        label_ref.set_opacity(
                            0.0,
                            Some(Transition {
                                duration: 0.1,
                                ..Default::default()
                            }),
                        );
                        l.children().iter().for_each(|child| {
                            child.set_color_filter(None);
                        });
                    });     
                    previous_miniwindows.retain(|l| l.id() != layer.id());
            }
        }

        // Cleanup layers

        // App layers
        for layer in previous_app_layers {
            layer.set_opacity(0.0, Transition::ease_out_quad(0.2));
            layer.set_size(layers::types::Size::points(0.0, app_height), Transition::ease_out_quad(0.3))
            .on_finish(|l:&Layer, _| {
                l.remove();
            }, true);
            apps_layers_map.retain(|_k,(v, _, _)| {
                v.id() != layer.id()
            });
        }

        // Mini window layers
        for layer in previous_miniwindows {
            layer.set_opacity(0.0, Transition::ease_out_quad(0.2));
            layer.set_size(layers::types::Size::points(0.0, miniwindow_height), Transition::ease_out_quad(0.3))
            .on_finish(|l:&Layer, _| {
                l.remove();
            }, true);

            miniwindows_layers_map.retain(|_k,(v, _, _)| {
                v.id() != layer.id()
            });
        }
    }
    fn available_icon_size(&self) -> f32 {
        let state = self.get_state();
        let draw_scale = Config::with(|config| config.screen_scale) as f32 * 0.8;
        // those are constant like values
        let available_width = state.width as f32 - 20.0 * draw_scale;
        let icon_size: f32 = 100.0 * draw_scale;

        let apps_len = state.running_apps.len() as f32;
        let windows_len = state.minimized_windows.len() as f32;

        let mut component_padding_h: f32 = icon_size * 0.09 * draw_scale;
        if component_padding_h > 5.0 * draw_scale {
            component_padding_h = 5.0 * draw_scale;
        }

        let available_icon_size =
            (available_width - component_padding_h * 2.0) / (apps_len + windows_len);
        let available_icon_size = icon_size.min(available_icon_size);

        available_icon_size
    }
    fn render_dock(&self) {
        let available_icon_size = self.available_icon_size();

        self.render_elements_layers(available_icon_size);
        self.magnify_elements();
        
    }
    
    fn notification_handler(&self, mut rx: tokio::sync::mpsc::Receiver<WorkspaceModel>) {
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
    
    fn get_app_layers(&self) -> Vec<Layer> {
        let app_layers = self.app_layers.read().unwrap();
        app_layers.values().cloned().map(|(layer, _, _)| layer).collect()
    }
    fn get_miniwin_layers(&self) -> Vec<Layer> {
        let miniwin_layers = self.miniwindow_layers.read().unwrap();
        miniwin_layers.values().cloned().map(|(layer, _, _)| layer).collect()
    }
    pub fn get_appid_from_layer(&self, layer: &NodeRef) -> Option<String> {
        let layers_map = self.app_layers.read().unwrap();
        layers_map.iter().find(|(_, (app_layer, _, _))| app_layer.id() == Some(*layer))
            .map(|(key, _)| key.clone())
    }
    pub fn get_window_from_layer(&self, layer: &NodeRef) -> Option<Window> {

        let miniwindow_layers = self.miniwindow_layers.read().unwrap();
        if let Some((window, ..)) = miniwindow_layers.iter().find(|(_win, (l, ..))| l.id() == Some(layer.clone())) {
            return Some(window.clone());
        }

        None
    }
    pub fn add_window_element(&self, window: &Window) -> (Layer, Layer) {
        let state = self.get_state();
        let mut minimized_windows= state.minimized_windows.clone();
        minimized_windows.push(window.clone());

        self.update_state(&DockModel {
            minimized_windows,
            ..self.get_state()
        });
        let layers_map = self.miniwindow_layers.read().unwrap();
        let (drawer, inner, ..) = layers_map.get(window).unwrap();

        (drawer.clone(), inner.clone())
    }
    pub fn remove_window_element(&self, window: &Window) -> Option<Layer>{
        let mut drawer = None;
        let mut miniwindow_layers = self.miniwindow_layers.write().unwrap();
        if let Some((d, _, label)) = miniwindow_layers.get(window) {
            drawer = Some(d.clone());
            // hide the label
            label.set_opacity(0.0, None);
            miniwindow_layers.remove(window);
        }
        drawer
    }

    // Magnify elements
    fn magnify_elements(&self) {
        let pos = self.magnification_position.read().unwrap().clone();
        let bounds = self.view_layer.render_bounds_transformed();
        let pos = pos - bounds.x();
        let padding = 20.0;
        let focus = pos / (bounds.width() - padding);
        let state = self.get_state();

        let draw_scale = Config::with(|config| config.screen_scale) as f32 * 0.8;
        let icon_size: f32 = 100.0 * draw_scale;

        let apps_len = state.running_apps.len() as f32;
        let windows_len = state.minimized_windows.len() as f32;

        let tot_elements = apps_len + windows_len;
        let animation = self.layers_engine.new_animation(Transition::ease_out_quad(0.2), false);
        let mut changes = Vec::new();
        let genie_scale = Config::with(|c| c.genie_scale);
        for (index, app) in state.running_apps.iter().enumerate() {
            let id = &app.identifier;
            let layers_map = self.app_layers.read().unwrap();
            if let Some((layer, _, _)) = layers_map.get(id) {
                let icon_pos = 1.0 / tot_elements * index as f32 + 1.0 / (tot_elements * 2.0);
                let icon_focus = 1.0 + magnify_function(focus - icon_pos) * genie_scale;
                let focused_icon_size = icon_size * icon_focus as f32;
                

                let change = layer.change_size(Size::points(focused_icon_size, focused_icon_size + 30.0));
                changes.push(change);
            }
        }

        let miniwindow_layers = self.miniwindow_layers.read().unwrap();
        

        for (index, win) in state.minimized_windows.iter().enumerate() {
            if let Some((layer, ..)) = miniwindow_layers.get(win) {
                let index = index + state.running_apps.len();
                let icon_pos = 1.0 / tot_elements * index as f32 + 1.0 / (tot_elements * 2.0);
                let icon_focus = 1.0 + magnify_function(focus - icon_pos) * genie_scale;
                let focused_icon_size = icon_size * icon_focus as f32;
                // let ratio = win.w / win.h;
                // let icon_height = focused_icon_size / ratio + 60.0;
                let change = layer.change_size(Size::points(focused_icon_size, focused_icon_size));
                changes.push(change);
            }
        }

        self.layers_engine.add_animated_changes(&changes, animation);

        self.layers_engine.start_animation(animation, 0.0);
    }
    pub fn update_magnification_position(&self, pos: f32) {
        *self.magnification_position.write().unwrap()= pos;
        self.magnify_elements();
    }
}

// Dock view observer
impl Observer<WorkspaceModel> for DockView {
    fn notify(&self, event: &WorkspaceModel) {
        let _ = self.notify_tx.try_send(event.clone());
    }
}

// https://www.wolframalpha.com/input?i=plot+e%5E%28-8*x%5E2%29
use std::f64::consts::E;
pub fn magnify_function(x: impl Into<f64>) -> f64 {
    let x = x.into();
    let genie_span = Config::with(|c| c.genie_span);
    let genie_span = -1.0 * genie_span;
    E.powf(genie_span * (x).powi(2))
}