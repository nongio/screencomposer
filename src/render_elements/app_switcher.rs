use std::collections::{HashMap, HashSet};

use layers::{
    engine::{
        animation::{timing::TimingFunction, Transition},
        LayersEngine,
    },
    prelude::taffy,
    taffy::style::Style,
    types::Size,
};
use smithay::{
    backend::renderer::{
        element::{Element, Id, RenderElement},
        utils::CommitCounter,
        Renderer,
    },
    utils::{Buffer, Physical, Point, Rectangle, Scale},
    wayland::shell::xdg::XdgToplevelSurfaceData,
};
use usvg::TreeParsing;

use crate::{
    app_switcher::{view::view_app_switcher, App, AppSwitcher},
    shell::WindowElement,
    skia_renderer::SkiaRenderer,
    udev::UdevRenderer, utils::image_from_svg,
};

pub struct AppSwitcherElement {
    id: Id,
    commit_counter: CommitCounter,
    pub app_switcher: AppSwitcher,
    icons: HashMap<std::string::String, skia_safe::Image>,
    pub layer: layers::prelude::Layer,
    pub view: layers::prelude::View<AppSwitcher>,
    active: bool,
}

impl AppSwitcherElement {
    pub fn new(layers_engine: LayersEngine) -> Self {
        let wrap = layers_engine.new_layer();
        wrap.set_size(Size::percent(1.0, 1.0), None);
        wrap.set_layout_style(Style {
            display: layers::taffy::style::Display::Flex,
            justify_content: Some(taffy::JustifyContent::Center),
            align_items: Some(taffy::AlignItems::Center),
            justify_items: Some(taffy::JustifyItems::Center),
            ..Default::default()
        });
        wrap.set_opacity(0.0, None);

        layers_engine.scene_add_layer(wrap.clone());
        let layer = layers_engine.new_layer();
        wrap.add_sublayer(layer.clone());

        let view = layers::prelude::View::new(layer.clone(), Box::new(view_app_switcher));
        Self {
            id: Id::new(),
            commit_counter: CommitCounter::default(),
            app_switcher: AppSwitcher::new(),
            icons: HashMap::new(),
            layer: wrap.clone(),
            view,
            active: false,
        }
    }

    pub fn update_icons(&mut self) {
        for (
            App {
                name,
                icon_path: icon,
            },
            _,
        ) in self.app_switcher.apps.iter()
        {
            if self.icons.contains_key(name) {
                continue;
            }
            if icon.is_none() {
                continue;
            }
            let icon_path = icon.as_ref().unwrap();
            let icon_data = std::fs::read(icon_path).unwrap();

            let image = if std::path::Path::new(icon_path)
                .extension()
                .and_then(std::ffi::OsStr::to_str)
                == Some("svg")
            {
                image_from_svg(&icon_data)
            } else {
                skia_safe::Image::from_encoded(skia_safe::Data::new_copy(icon_data.as_slice()))
                    .unwrap()
            };

            self.icons.insert(name.clone(), image);
        }
    }
    pub fn update(&mut self) {
        self.update_icons();
        self.app_switcher.width = 1000;
        if self.view.render(&self.app_switcher) {
            self.commit_counter.increment();
        }
    }

    pub(crate) fn update_with_window_elements(&mut self, windows: &[WindowElement]) {
        let mut apps = Vec::new();
        let mut seen = HashSet::new();
        windows
            .iter()
            .filter(|w| w.wl_surface().is_some())
            .for_each(|w| {
                smithay::wayland::compositor::with_states(
                    w.wl_surface().as_ref().unwrap(),
                    |states| {
                        let attributes = states
                            .data_map
                            .get::<XdgToplevelSurfaceData>()
                            .unwrap()
                            .lock()
                            .unwrap();
                        if let Some(app_id) = attributes.app_id.as_ref() {
                            if seen.insert(app_id.clone()) {
                                apps.push((app_id.clone(), w.clone()));
                            }
                        }
                    },
                );
            });

        self.app_switcher.update_apps(apps.as_slice());
        self.update();
    }
    pub fn next(&mut self) {
        if !self.app_switcher.apps.is_empty() {
            self.app_switcher.current_app =
                (self.app_switcher.current_app + 1) % self.app_switcher.apps.len();
        } else {
            self.app_switcher.current_app = 0;
        }

        self.update();
        self.active = true;
        self.layer.set_opacity(
            1.0,
            Some(Transition {
                duration: 0.1,
                delay: 0.1,
                timing: TimingFunction::default(),
            }),
        );
    }
    pub fn previous(&mut self) {
        self.app_switcher.current_app =
            (self.app_switcher.current_app + 1) % self.app_switcher.apps.len();

        self.update();
        self.active = true;
        self.layer.set_opacity(
            1.0,
            Some(Transition {
                duration: 0.1,
                delay: 0.1,
                timing: TimingFunction::default(),
            }),
        );
    }
    pub fn hide(&mut self) {
        self.active = false;
        self.layer.set_opacity(
            0.0,
            Some(Transition {
                duration: 0.3,
                delay: 0.0,
                timing: TimingFunction::default(),
            }),
        );
    }

    pub fn quit_current_app(&mut self) {
        if self.active {
            let we = self.app_switcher.current_window_element();
            if let Some(we) = we {
                match we {
                    WindowElement::Wayland(w) => w.toplevel().send_close(),
                    #[cfg(feature = "xwayland")]
                    WindowElement::X11(w) => {
                        let _ = w.close();
                    }
                };
            }
        }
    }
}

impl Element for AppSwitcherElement {
    fn id(&self) -> &Id {
        &self.id
    }

    fn location(&self, _scale: Scale<f64>) -> Point<i32, Physical> {
        (100, 100).into()
    }

    fn src(&self) -> Rectangle<f64, Buffer> {
        Rectangle::from_loc_and_size((0, 0), (800, 250)).to_f64()
    }

    fn geometry(&self, scale: Scale<f64>) -> Rectangle<i32, Physical> {
        let width = self.app_switcher.apps.len() as i32 * 220 + 40;
        Rectangle::from_loc_and_size(self.location(scale), (width, 250))
    }

    fn current_commit(&self) -> CommitCounter {
        self.commit_counter
    }
    /// Get the damage since the provided commit relative to the element
    fn damage_since(
        &self,
        scale: Scale<f64>,
        _commit: Option<CommitCounter>,
    ) -> Vec<Rectangle<i32, Physical>> {
        vec![Rectangle::from_loc_and_size(
            (0, 0),
            self.geometry(scale).size,
        )]
    }
    fn alpha(&self) -> f32 {
        0.5
    }
}

impl RenderElement<SkiaRenderer> for AppSwitcherElement {
    fn draw(
        &self,
        frame: &mut <SkiaRenderer as Renderer>::Frame<'_>,
        _src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
    ) -> Result<(), <SkiaRenderer as Renderer>::Error> {
        let mut canvas = frame.skia_surface.clone();
        let canvas = canvas.canvas();

        let instances = damage
            .iter()
            .map(|rect| {
                let dest_size = dst.size;

                let rect_constrained_loc = rect
                    .loc
                    .constrain(Rectangle::from_extemities((0, 0), dest_size.to_point()));
                let rect_clamped_size = rect.size.clamp(
                    (0, 0),
                    (dest_size.to_point() - rect_constrained_loc).to_size(),
                );

                let rect = Rectangle::from_loc_and_size(rect_constrained_loc, rect_clamped_size);
                skia_safe::Rect::from_xywh(
                    (dst.loc.x + rect.loc.x) as f32,
                    (dst.loc.y + rect.loc.y) as f32,
                    (rect.size.w) as f32,
                    (rect.size.h) as f32,
                )
            })
            .collect::<Vec<skia_safe::Rect>>();

        let scale = Scale::from(1.0);
        let location = self.location(scale);
        let geometry = self.geometry(scale).size;
        let bounds = skia_safe::Rect::from_xywh(
            location.x as f32,
            location.y as f32,
            geometry.w as f32,
            geometry.h as f32,
        );

        let radius = 20.0;
        let rrect = skia_safe::RRect::new_rect_radii(
            bounds,
            &[
                skia_safe::Point::new(radius, radius),
                skia_safe::Point::new(radius, radius),
                skia_safe::Point::new(radius, radius),
                skia_safe::Point::new(radius, radius),
            ],
        );
        let background_color = skia_safe::Color4f::new(0.9, 0.9, 0.9, 0.3);
        let mut background_paint = skia_safe::Paint::new(background_color, None);
        background_paint.set_anti_alias(true);
        background_paint.set_style(skia_safe::PaintStyle::Fill);

        let mut save_layer_rec = skia_safe::canvas::SaveLayerRec::default();
        let blur = skia_safe::image_filters::blur(
            (40.0, 40.0),
            skia_safe::TileMode::Clamp,
            None,
            Some(skia_safe::image_filters::CropRect::from(bounds)),
        )
        .unwrap();

        let save_count = canvas.save();

        background_paint.set_blend_mode(skia_safe::BlendMode::SrcOver);
        let mut path = skia_safe::Path::new();
        for rect in instances.iter() {
            path.add_rect(*rect, None);
        }

        canvas.clip_path(&path, None, Some(true));
        canvas.clip_rrect(rrect, skia_safe::ClipOp::Intersect, Some(true));

        save_layer_rec = save_layer_rec.backdrop(&blur).bounds(&bounds);
        canvas.save_layer(&save_layer_rec);

        canvas.draw_paint(&background_paint);

        let mut paint = skia_safe::Paint::new(skia_safe::Color4f::new(1.0, 1.0, 1.0, 1.0), None);
        paint.set_blend_mode(skia_safe::BlendMode::SrcOver);
        let padding = 20.0;
        let icon_size = 200.0;
        let mut x = bounds.x() + padding;
        let y = bounds.y() + bounds.height() / 2.0 - icon_size / 2.0;
        for app in self.app_switcher.apps.iter() {
            let shadow_color = skia_safe::Color4f::new(0.0, 0.0, 0.0, 0.5);
            let mut shadow_paint = skia_safe::Paint::new(shadow_color, None);
            let shadow_offset = skia_safe::Vector::new(5.0, 5.0);
            let shadow_color = skia_safe::Color::from_argb(128, 0, 0, 0); // semi-transparent black
            let shadow_blur_radius = 5.0;
            let shadow_filter = skia_safe::image_filters::drop_shadow_only(
                (shadow_offset.x, shadow_offset.y),
                (shadow_blur_radius, shadow_blur_radius),
                shadow_color,
                None,
                skia_safe::image_filters::CropRect::default(),
            );
            shadow_paint.set_image_filter(shadow_filter);
            if let Some(icon) = self.icons.get(&app.0.name) {
                canvas.draw_image_rect(
                    icon,
                    None,
                    skia_safe::Rect::from_xywh(x, y, icon_size, icon_size),
                    &shadow_paint,
                );
                let resampler = skia_safe::CubicResampler::catmull_rom();
                canvas.draw_image_rect_with_sampling_options(
                    icon,
                    None,
                    skia_safe::Rect::from_xywh(x, y, icon_size, icon_size),
                    skia_safe::SamplingOptions::from(resampler),
                    &paint,
                );
            } else {
                canvas.draw_rect(
                    skia_safe::Rect::from_xywh(x, y, icon_size, icon_size),
                    &shadow_paint,
                );
                canvas.draw_rect(
                    skia_safe::Rect::from_xywh(x, y, icon_size, icon_size),
                    &paint,
                );
            }
            x += icon_size + padding;
        }

        canvas.restore();
        canvas.restore_to_count(save_count);
        Ok(())
    }
}

impl<'renderer, 'alloc> RenderElement<UdevRenderer<'renderer, 'alloc>> for AppSwitcherElement {
    fn draw(
        &self,
        frame: &mut <UdevRenderer<'renderer, 'alloc> as Renderer>::Frame<'_>,
        src: Rectangle<f64, Buffer>,
        dst: Rectangle<i32, Physical>,
        damage: &[Rectangle<i32, Physical>],
    ) -> Result<(), <UdevRenderer<'renderer, 'alloc> as Renderer>::Error> {
        RenderElement::<SkiaRenderer>::draw(self, frame.as_mut(), src, dst, damage)
            .map_err(|e| e.into())
    }
}
